impl HestiaApp {
    fn validate_profile_name(
        &self,
        game_id: &str,
        name: &str,
        except: Option<Uuid>,
    ) -> Result<String> {
        let normalized = name.trim();
        if normalized.is_empty() {
            bail!("profile name cannot be empty");
        }
        if self
            .state
            .profiles_by_game
            .get(game_id)
            .is_some_and(|catalog| {
                catalog.profiles.iter().any(|profile| {
                    Some(profile.id) != except
                        && TextCatalog::profile_names_equal(&profile.display_name, normalized)
                })
            })
        {
            bail!("a profile with that name already exists");
        }
        Ok(normalized.to_string())
    }

    fn profile_operation_block_reason(&self, kind: ProfileOperationKind) -> Option<&'static str> {
        if self.profile_operation_inflight.is_some() {
            return Some("another profile operation is already running");
        }
        if kind == ProfileOperationKind::Recover {
            return None;
        }
        if self.startup_scan_loading {
            return Some("a game refresh is running");
        }
        if self.install_batch_active
            || !self.install_inflight.is_empty()
            || !self.install_queue.is_empty()
        {
            return Some("mod installation is running");
        }
        if self.refresh_inflight {
            return Some("mod refresh is running");
        }
        if self.update_check_inflight {
            return Some("update checking is running");
        }
        if self.app_update_download_inflight.is_some() {
            return Some("an app update is running");
        }
        if self.selected_game().is_some_and(Self::game_process_running) {
            return Some("the selected game is running");
        }
        None
    }

    fn game_process_running(game: &GameInstall) -> bool {
        let mut names = Vec::new();
        if let Some(path) = game.vanilla_exe_path() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                names.push(name.to_ascii_lowercase());
            }
        }
        if let Some(path) = game.modded_exe_path() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                names.push(name.to_ascii_lowercase());
            }
        }
        if names.is_empty() {
            return false;
        }
        let system = sysinfo::System::new_all();
        system.processes().values().any(|process| {
            names
                .iter()
                .any(|name| process.name().to_string_lossy().eq_ignore_ascii_case(name))
        })
    }

    fn profile_record_metadata(
        game: &GameInstall,
        id: Uuid,
        display_name: &str,
        portable_metadata: HashMap<String, serde_json::Value>,
        created_at: Option<DateTime<Utc>>,
        categories: Option<Vec<ModCategory>>,
    ) -> profiles::ProfileArchiveMetadata {
        profiles::ProfileArchiveMetadata {
            format_version: profiles::PROFILE_ARCHIVE_FORMAT_VERSION,
            profile_id: id,
            game_id: game.definition.id.clone(),
            display_name: display_name.to_string(),
            backend: game.definition.backend,
            created_at: created_at.unwrap_or_else(Utc::now),
            uncompressed_size: 0,
            file_count: 0,
            portable_metadata,
            categories,
            source_fingerprint: None,
        }
    }

    fn profile_categories_for_game(&self, game_id: &str) -> Vec<ModCategory> {
        self.state
            .categories
            .iter()
            .filter(|category| category.game_id == game_id)
            .cloned()
            .collect()
    }

    fn sanitize_profile_categories(
        game_id: &str,
        categories: Option<Vec<ModCategory>>,
    ) -> Option<Vec<ModCategory>> {
        categories.map(|categories| {
            categories
                .into_iter()
                .map(|mut category| {
                    category.game_id = game_id.to_string();
                    category
                })
                .collect()
        })
    }

    fn snapshot_active_profile_categories(
        &mut self,
        game_id: &str,
        active_id: Option<Uuid>,
    ) -> Vec<ModCategory> {
        let categories = self.profile_categories_for_game(game_id);
        let mut changed = false;
        if let Some(active_id) = active_id {
            if let Some(catalog) = self.state.profiles_by_game.get_mut(game_id) {
                if let Some(profile) = catalog
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id == active_id)
                {
                    if profile.categories.as_ref() != Some(&categories) {
                        profile.categories = Some(categories.clone());
                        profile.updated_at = Some(Utc::now());
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.save_state();
        }
        categories
    }

    fn profile_archive_for(&self, game: &GameInstall, id: Uuid) -> Result<PathBuf> {
        profiles::profile_archive_path(game, self.state.static_prefs.use_default_mods_path, id)
    }

    fn profile_path_for(&self, game: &GameInstall, id: Uuid) -> Result<PathBuf> {
        profiles::profile_path(game, self.state.static_prefs.use_default_mods_path, id)
    }

    fn active_profile_id(&self, game_id: &str) -> Option<Uuid> {
        self.state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| catalog.active_profile_id)
    }

    fn dispatch_profile_spec(&mut self, spec: ProfileOperationSpec) -> Result<()> {
        let source_display_name = spec
            .metadata
            .as_ref()
            .map(|metadata| metadata.display_name.clone())
            .or_else(|| spec.display_name.clone());
        let target_display_name = spec.target_display_name.clone();
        let prepares_before_activating = matches!(
            spec.kind,
            ProfileOperationKind::Switch | ProfileOperationKind::Duplicate
        );
        if self
            .profile_request_tx
            .send(ProfileRequest::Execute(spec.clone()))
            .is_err()
        {
            return Err(anyhow!("failed to queue profile operation"));
        }
        self.profile_operation_inflight = Some(ProfileOperationInflight {
            operation_id: spec.operation_id,
            kind: spec.kind,
            source_display_name,
            target_display_name,
            prepares_before_activating,
            cancel: spec.cancel,
            progress: spec.progress,
            stage: spec.stage,
        });
        Ok(())
    }

    fn begin_profile_operation(
        &mut self,
        game_id: String,
        game: GameInstall,
        kind: ProfileOperationKind,
        profile_id: Option<Uuid>,
        source_profile_id: Option<Uuid>,
        target_profile_id: Option<Uuid>,
        display_name: Option<String>,
        target_display_name: Option<String>,
        source_archive: Option<PathBuf>,
        target_archive: Option<PathBuf>,
        target_categories: Option<Vec<ModCategory>>,
        metadata: Option<profiles::ProfileArchiveMetadata>,
    ) -> Result<()> {
        if let Some(reason) = self.profile_operation_block_reason(kind) {
            bail!("cannot start profile operation: {reason}");
        }
        let operation_id = self.profile_next_operation_id;
        self.profile_next_operation_id = self.profile_next_operation_id.wrapping_add(1).max(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU64::new(0));
        let stage = Arc::new(RwLock::new("Preparing profile operation".to_string()));
        self.dispatch_profile_spec(ProfileOperationSpec {
            operation_id,
            game_id,
            game,
            use_default_mods_path: self.state.static_prefs.use_default_mods_path,
            kind,
            profile_id,
            source_profile_id,
            target_profile_id,
            display_name,
            target_display_name,
            source_archive,
            target_archive,
            target_categories,
            metadata,
            delete_behavior: self.state.static_prefs.delete_behavior,
            cancel,
            progress,
            stage,
        })
    }

    pub(crate) fn cancel_profile_operation(&mut self) {
        if let Some(inflight) = &self.profile_operation_inflight {
            inflight.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn profile_operations_blocked(&self) -> bool {
        self.profile_operation_block_reason(ProfileOperationKind::Switch)
            .is_some()
    }

    pub(crate) fn profile_operation_locks_app(&self) -> bool {
        self.profile_operation_inflight
            .as_ref()
            .is_some_and(|inflight| {
                matches!(
                    inflight.kind,
                    ProfileOperationKind::Create
                        | ProfileOperationKind::Duplicate
                        | ProfileOperationKind::Switch
                )
            })
    }

    pub(crate) fn ensure_selected_game_default_profile(&mut self) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        self.ensure_default_profile(&game_id)
    }

    pub(crate) fn request_create_empty_profile(&mut self, name: String) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        self.create_profile(&game_id, name)
    }

    pub(crate) fn request_duplicate_current_profile(&mut self, name: String) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        let source = self
            .active_profile_id(&game_id)
            .ok_or_else(|| anyhow!("selected game has no active profile"))?;
        self.duplicate_profile(&game_id, source, name)
    }

    pub(crate) fn request_switch_profile(&mut self, id: Uuid) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        self.switch_profile(&game_id, id)
    }

    pub(crate) fn request_rename_profile(&mut self, id: Uuid, name: String) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        self.rename_profile(&game_id, id, name)
    }

    pub(crate) fn request_delete_profile(&mut self, id: Uuid) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        self.delete_profile(&game_id, id)
    }

    pub(crate) fn profile_operation_progress(&self) -> Option<(u8, String)> {
        let inflight = self.profile_operation_inflight.as_ref()?;
        let progress = inflight.progress.load(Ordering::Relaxed).min(100) as u8;
        let stage = inflight
            .stage
            .read()
            .map(|stage| stage.clone())
            .unwrap_or_else(|_| "Working".to_string());
        Some((progress, stage))
    }

    pub(crate) fn ensure_default_profile(&mut self, game_id: &str) -> Result<()> {
        let current_categories = self.profile_categories_for_game(game_id);
        let mut migrated = false;
        let existing_ready = if let Some(catalog) = self.state.profiles_by_game.get_mut(game_id) {
            for profile in &mut catalog.profiles {
                if profile.categories.is_none() {
                    profile.categories = Some(current_categories.clone());
                    migrated = true;
                }
            }
            if catalog.active_profile_id.is_none() && !catalog.profiles.is_empty() {
                catalog.active_profile_id = catalog.profiles.first().map(|profile| profile.id);
                migrated = true;
            }
            if catalog.active_profile_id.is_some() && !catalog.profiles.is_empty() {
                true
            } else {
                false
            }
        } else {
            false
        };
        if existing_ready {
            if migrated {
                self.save_state();
            }
            return Ok(());
        }
        if self
            .state
            .profiles_by_game
            .get(game_id)
            .is_some_and(|catalog| {
                catalog.active_profile_id.is_some() && !catalog.profiles.is_empty()
            })
        {
            return Ok(());
        }
        if self
            .state
            .games
            .iter()
            .all(|game| game.definition.id != game_id)
        {
            bail!("game {game_id} is not configured");
        }
        let id = Uuid::new_v4();
        let catalog = self
            .state
            .profiles_by_game
            .entry(game_id.to_string())
            .or_default();
        catalog.profiles.push(ProfileRecord {
            id,
            display_name: "Default".to_string(),
            archive_size: None,
            uncompressed_size: None,
            file_count: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            portable_metadata: HashMap::new(),
            categories: Some(current_categories),
        });
        catalog.active_profile_id = Some(id);
        self.save_state();
        Ok(())
    }

    pub(crate) fn create_profile(&mut self, game_id: &str, display_name: String) -> Result<()> {
        let display_name = self.validate_profile_name(game_id, &display_name, None)?;
        let game = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
            .ok_or_else(|| anyhow!("game {game_id} is not configured"))?;
        if self.active_profile_id(game_id).is_none() {
            self.ensure_default_profile(game_id)?;
        }
        let active_id = self.active_profile_id(game_id);
        let active_categories = self.snapshot_active_profile_categories(game_id, active_id);
        let metadata = active_id.and_then(|id| {
            self.state
                .profiles_by_game
                .get(game_id)
                .and_then(|catalog| catalog.profiles.iter().find(|profile| profile.id == id))
                .map(|profile| {
                    Self::profile_record_metadata(
                        &game,
                        id,
                        &profile.display_name,
                        profile.portable_metadata.clone(),
                        profile.created_at,
                        Some(active_categories.clone()),
                    )
                })
        });
        let target_id = Uuid::new_v4();
        let active_archive = active_id.and_then(|id| self.profile_archive_for(&game, id).ok());
        self.begin_profile_operation(
            game_id.to_string(),
            game,
            ProfileOperationKind::Create,
            active_id,
            None,
            Some(target_id),
            metadata.as_ref().map(|m| m.display_name.clone()),
            Some(display_name),
            active_archive,
            None,
            Some(Vec::new()),
            metadata,
        )
    }

    pub(crate) fn duplicate_profile(
        &mut self,
        game_id: &str,
        source_id: Uuid,
        display_name: String,
    ) -> Result<()> {
        let display_name = self.validate_profile_name(game_id, &display_name, None)?;
        let game = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
            .ok_or_else(|| anyhow!("game {game_id} is not configured"))?;
        self.ensure_default_profile(game_id)?;
        let active_id = self.active_profile_id(game_id);
        let active_categories = self.snapshot_active_profile_categories(game_id, active_id);
        let source_archive = if Some(source_id) == active_id {
            active_id.and_then(|id| self.profile_archive_for(&game, id).ok())
        } else {
            Some(self.profile_archive_for(&game, source_id)?)
        };
        let target_id = Uuid::new_v4();
        let source_record = self
            .state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| {
                catalog
                    .profiles
                    .iter()
                    .find(|profile| profile.id == source_id)
            })
            .ok_or_else(|| anyhow!("source profile does not exist"))?;
        let metadata = active_id.and_then(|id| {
            self.state
                .profiles_by_game
                .get(game_id)
                .and_then(|catalog| catalog.profiles.iter().find(|profile| profile.id == id))
                .map(|active_record| {
                    Self::profile_record_metadata(
                        &game,
                        id,
                        &active_record.display_name,
                        active_record.portable_metadata.clone(),
                        active_record.created_at,
                        Some(active_categories.clone()),
                    )
                })
        });
        let source_categories = source_record
            .categories
            .clone()
            .or_else(|| Some(active_categories.clone()));
        self.begin_profile_operation(
            game_id.to_string(),
            game,
            ProfileOperationKind::Duplicate,
            active_id,
            Some(source_id),
            Some(target_id),
            Some(source_record.display_name.clone()),
            Some(display_name),
            source_archive,
            None,
            source_categories,
            metadata,
        )
    }

    pub(crate) fn switch_profile(&mut self, game_id: &str, target_id: Uuid) -> Result<()> {
        let game = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
            .ok_or_else(|| anyhow!("game {game_id} is not configured"))?;
        self.ensure_default_profile(game_id)?;
        let active_id = self.active_profile_id(game_id);
        let active_categories = self.snapshot_active_profile_categories(game_id, active_id);
        if active_id == Some(target_id) {
            return Ok(());
        }
        if !self
            .state
            .profiles_by_game
            .get(game_id)
            .is_some_and(|catalog| {
                catalog
                    .profiles
                    .iter()
                    .any(|profile| profile.id == target_id)
            })
        {
            bail!("target profile does not exist");
        }
        let target_archive = self.profile_archive_for(&game, target_id)?;
        let target_profile = self.profile_path_for(&game, target_id)?;
        if !target_archive.is_file() && !target_profile.is_dir() {
            bail!("target profile data is missing");
        }
        let metadata = active_id.and_then(|id| {
            self.state
                .profiles_by_game
                .get(game_id)
                .and_then(|catalog| catalog.profiles.iter().find(|profile| profile.id == id))
                .map(|profile| {
                    Self::profile_record_metadata(
                        &game,
                        id,
                        &profile.display_name,
                        profile.portable_metadata.clone(),
                        profile.created_at,
                        Some(active_categories),
                    )
                })
        });
        let active_archive = active_id.and_then(|id| self.profile_archive_for(&game, id).ok());
        let target_display_name = self
            .state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| {
                catalog
                    .profiles
                    .iter()
                    .find(|profile| profile.id == target_id)
            })
            .map(|profile| profile.display_name.clone());
        let target_categories = self
            .state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| {
                catalog
                    .profiles
                    .iter()
                    .find(|profile| profile.id == target_id)
            })
            .and_then(|profile| profile.categories.clone());
        self.begin_profile_operation(
            game_id.to_string(),
            game,
            ProfileOperationKind::Switch,
            active_id,
            None,
            Some(target_id),
            None,
            target_display_name,
            active_archive,
            Some(target_archive),
            target_categories,
            metadata,
        )
    }

    pub(crate) fn rename_profile(
        &mut self,
        game_id: &str,
        profile_id: Uuid,
        display_name: String,
    ) -> Result<()> {
        let normalized = display_name.trim();
        if normalized.is_empty() {
            bail!("profile name cannot be empty");
        }
        let catalog = self
            .state
            .profiles_by_game
            .get(game_id)
            .ok_or_else(|| anyhow!("profile catalog does not exist"))?;
        if catalog
            .profiles
            .iter()
            .all(|profile| profile.id != profile_id)
        {
            bail!("profile does not exist");
        }
        if catalog.profiles.iter().any(|other| {
            other.id != profile_id
                && TextCatalog::profile_names_equal(&other.display_name, normalized)
        }) {
            bail!("a profile with that name already exists");
        }
        let catalog = self
            .state
            .profiles_by_game
            .get_mut(game_id)
            .expect("catalog checked above");
        let profile = catalog
            .profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .expect("profile checked above");
        if profile.display_name == normalized {
            return Ok(());
        }
        profile.display_name = normalized.to_string();
        profile.updated_at = Some(Utc::now());
        let renamed_stored = profile.display_name.clone();
        self.save_state();
        let renamed = self.text().profile_display_name(&renamed_stored);
        self.set_message_ok(self.text().profile_renamed(&renamed));
        Ok(())
    }

    pub(crate) fn delete_profile(&mut self, game_id: &str, profile_id: Uuid) -> Result<()> {
        if self.active_profile_id(game_id) == Some(profile_id) {
            bail!("switch profiles before deleting the active profile");
        }
        let game = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
            .ok_or_else(|| anyhow!("game {game_id} is not configured"))?;
        let archive = self.profile_archive_for(&game, profile_id)?;
        if !self
            .state
            .profiles_by_game
            .get(game_id)
            .is_some_and(|catalog| {
                catalog.profiles.len() > 1
                    && catalog
                        .profiles
                        .iter()
                        .any(|profile| profile.id == profile_id)
            })
        {
            bail!("profile does not exist");
        }
        self.begin_profile_operation(
            game_id.to_string(),
            game,
            ProfileOperationKind::Delete,
            Some(profile_id),
            None,
            None,
            None,
            None,
            None,
            Some(archive),
            None,
            None,
        )
    }

    fn dispatch_profile_recovery(&mut self) {
        self.profile_recovery_failed = false;
        self.profile_recovery_queue = self.state.games.clone().into();
        self.dispatch_next_profile_recovery();
    }

    fn dispatch_next_profile_recovery(&mut self) {
        if self.profile_operation_inflight.is_some() {
            return;
        }
        let Some(game) = self.profile_recovery_queue.pop_front() else {
            return;
        };
        if let Err(error) = self.begin_profile_operation(
            game.definition.id.clone(),
            game,
            ProfileOperationKind::Recover,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ) {
            self.profile_recovery_failed = true;
            self.report_error_message(
                format!("profile recovery could not start: {error:#}"),
                Some("Profile recovery failed"),
            );
        }
    }

    fn apply_profile_archive_result(
        &mut self,
        game_id: &str,
        profile_id: Option<Uuid>,
        archive: &profiles::ArchiveResult,
    ) {
        let Some(profile_id) = profile_id else {
            return;
        };
        if let Some(catalog) = self.state.profiles_by_game.get_mut(game_id) {
            if let Some(profile) = catalog
                .profiles
                .iter_mut()
                .find(|profile| profile.id == profile_id)
            {
                profile.archive_size = Some(archive.bytes);
                profile.uncompressed_size = Some(archive.uncompressed_size);
                profile.file_count = Some(archive.file_count);
                profile.updated_at = Some(Utc::now());
            }
        }
    }

    fn consume_profile_events(&mut self) {
        while let Ok(event) = self.profile_event_rx.try_recv() {
            match event {
                ProfileEvent::ArchiveCompleted {
                    game_id,
                    profile_id,
                    archive,
                } => {
                    self.apply_profile_archive_result(&game_id, Some(profile_id), &archive);
                    self.save_state();
                    continue;
                }
                ProfileEvent::ArchiveFailed {
                    game_id,
                    profile_id,
                    error,
                } => {
                    self.report_error_message(
                        format!(
                            "background profile compression failed for {game_id}/{profile_id}: {error}"
                        ),
                        Some("Profile compression failed"),
                    );
                    continue;
                }
                event => {
                    let event_id = match &event {
                        ProfileEvent::Completed { operation_id, .. }
                        | ProfileEvent::Failed { operation_id, .. }
                        | ProfileEvent::Canceled { operation_id, .. } => *operation_id,
                        ProfileEvent::ArchiveCompleted { .. }
                        | ProfileEvent::ArchiveFailed { .. } => unreachable!(),
                    };
                    if self
                        .profile_operation_inflight
                        .as_ref()
                        .is_none_or(|operation| operation.operation_id != event_id)
                    {
                        continue;
                    }
                    let Some(inflight) = self.profile_operation_inflight.take() else {
                        continue;
                    };
                    let was_recovery = inflight.kind == ProfileOperationKind::Recover;
                    let recovery_blocking = was_recovery
                        && matches!(
                            &event,
                            ProfileEvent::Failed {
                                recovery_blocking: true,
                                ..
                            }
                        );
                    self.consume_foreground_profile_event(event, inflight);
                    if was_recovery {
                        if recovery_blocking {
                            self.profile_recovery_failed = true;
                        }
                        self.dispatch_next_profile_recovery();
                    }
                }
            }
        }
    }

    fn consume_foreground_profile_event(
        &mut self,
        event: ProfileEvent,
        inflight: ProfileOperationInflight,
    ) {
        match event {
            ProfileEvent::Completed {
                game_id,
                kind,
                profile_id,
                target_profile_id,
                display_name,
                archive,
                active_profile_marker,
                warnings,
                ..
            } => {
                if let Some(archive) = &archive {
                    self.apply_profile_archive_result(&game_id, profile_id, archive);
                }
                let existing_name =
                    self.state
                        .profiles_by_game
                        .get(&game_id)
                        .and_then(|catalog| {
                            catalog.profiles.iter().find(|profile| {
                                Some(profile.id) == target_profile_id.or(profile_id)
                            })
                        })
                        .map(|profile| profile.display_name.clone());
                let success_name = display_name
                    .clone()
                    .or_else(|| existing_name.clone())
                    .map(|name| self.text().profile_display_name(&name));
                let active_profile_categories = active_profile_marker
                    .as_ref()
                    .and_then(|marker| marker.categories.clone());
                let catalog = self
                    .state
                    .profiles_by_game
                    .entry(game_id.clone())
                    .or_default();
                match kind {
                    ProfileOperationKind::Create | ProfileOperationKind::Duplicate => {
                        let id = target_profile_id.expect("target profile id");
                        if !catalog.profiles.iter().any(|profile| profile.id == id) {
                            catalog.profiles.push(ProfileRecord {
                                id,
                                display_name: display_name.unwrap_or_else(|| "Profile".to_string()),
                                archive_size: None,
                                uncompressed_size: None,
                                file_count: None,
                                created_at: Some(Utc::now()),
                                updated_at: Some(Utc::now()),
                                portable_metadata: HashMap::new(),
                                categories: Some(
                                    active_profile_categories.clone().unwrap_or_default(),
                                ),
                            });
                        }
                        catalog.active_profile_id = Some(id);
                    }
                    ProfileOperationKind::Switch => catalog.active_profile_id = target_profile_id,
                    ProfileOperationKind::Rename => {
                        if let Some(profile) = catalog
                            .profiles
                            .iter_mut()
                            .find(|profile| Some(profile.id) == profile_id)
                        {
                            profile.display_name =
                                display_name.unwrap_or_else(|| profile.display_name.clone());
                            profile.updated_at = Some(Utc::now());
                        }
                    }
                    ProfileOperationKind::Delete => catalog
                        .profiles
                        .retain(|profile| Some(profile.id) != profile_id),
                    ProfileOperationKind::Recover => {
                        if let Some(id) = target_profile_id {
                            if !catalog.profiles.iter().any(|profile| profile.id == id) {
                                catalog.profiles.push(ProfileRecord {
                                    id,
                                    display_name: display_name
                                        .clone()
                                        .unwrap_or_else(|| "Profile".to_string()),
                                    archive_size: None,
                                    uncompressed_size: None,
                                    file_count: None,
                                    created_at: Some(Utc::now()),
                                    updated_at: Some(Utc::now()),
                                    portable_metadata: HashMap::new(),
                                    categories: Some(
                                        active_profile_categories.clone().unwrap_or_default(),
                                    ),
                                });
                            }
                            catalog.active_profile_id = Some(id);
                        }
                    }
                }
                if let Some(marker) = active_profile_marker.as_ref() {
                    if let Some(profile) = catalog
                        .profiles
                        .iter_mut()
                        .find(|profile| profile.id == marker.profile_id)
                    {
                        profile.categories = marker.categories.clone();
                    }
                }
                if matches!(
                    kind,
                    ProfileOperationKind::Create
                        | ProfileOperationKind::Duplicate
                        | ProfileOperationKind::Switch
                        | ProfileOperationKind::Recover
                ) {
                    if let Some(categories) = active_profile_categories.clone() {
                        self.state
                            .categories
                            .retain(|category| category.game_id != game_id);
                        self.state.categories.extend(
                            Self::sanitize_profile_categories(&game_id, Some(categories))
                                .unwrap_or_default(),
                        );
                        if self
                            .selected_game()
                            .is_some_and(|game| game.definition.id == game_id)
                        {
                            self.selected_category_folder_id = None;
                            self.selected_category_ids.clear();
                            self.category_rename_target_id = None;
                            self.category_rename_focus_target_id = None;
                            self.category_rename_surface = None;
                            self.category_rename_name.clear();
                            self.dragging_category_id = None;
                            self.dragging_category_target_index = None;
                            self.settings_dragging_category_ids.clear();
                            self.settings_dragging_category_target_index = None;
                            self.library_card_cache.key = None;
                        }
                    }
                }
                self.save_state();
                if matches!(
                    kind,
                    ProfileOperationKind::Create
                        | ProfileOperationKind::Duplicate
                        | ProfileOperationKind::Switch
                ) {
                    self.queue_game_refresh(game_id);
                }
                for warning in warnings {
                    self.report_error_message(warning, Some("Profile data preserved"));
                }
                if let Some(name) = success_name {
                    let message = match kind {
                        ProfileOperationKind::Create => Some(self.text().profile_created(&name)),
                        ProfileOperationKind::Duplicate => {
                            Some(self.text().profile_duplicated(&name))
                        }
                        ProfileOperationKind::Switch => Some(self.text().profile_activated(&name)),
                        ProfileOperationKind::Delete => Some(self.text().profile_deleted(&name)),
                        _ => None,
                    };
                    if let Some(message) = message {
                        self.set_message_ok(message);
                    }
                }
                let _ = inflight;
            }
            ProfileEvent::Failed { game_id, error, .. } => {
                self.report_error_message(
                    format!("profile operation failed for {game_id}: {error}"),
                    Some("Profile operation failed"),
                );
            }
            ProfileEvent::Canceled { .. } => self.set_message_ok(self.text().profile_canceled()),
            ProfileEvent::ArchiveCompleted { .. } | ProfileEvent::ArchiveFailed { .. } => {
                unreachable!()
            }
        }
    }
}
