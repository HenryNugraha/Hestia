const PROFILE_STORAGE_FILE_NAME_METADATA_KEY: &str = "profile_storage_file_name";
const MODS_LOCKED_BLOCK_REASON: &str = "mods are currently locked";
const PROFILE_STORAGE_OUT_OF_DATE_ERROR: &str = "profile storage is out of date";

impl HestiaApp {
    fn validate_profile_name(
        &self,
        game_id: &str,
        name: &str,
        except: Option<ProfileId>,
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
        self.running_process_block_reason()
    }

    /// Both process guards in one scan. `profile_operations_blocked` runs from the render loop, so
    /// this must enumerate processes at most once per call.
    fn running_process_block_reason(&self) -> Option<&'static str> {
        let game = self.selected_game()?;

        let mut game_exe_names = Vec::new();
        for path in [game.vanilla_exe_path(), game.modded_exe_path()]
            .into_iter()
            .flatten()
        {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                game_exe_names.push(name.to_ascii_lowercase());
            }
        }

        // A tool launched out of the profile folder keeps file handles open there, which makes the
        // directory rename in `swap_roots` fail. Block up front rather than failing partway.
        let use_default = self.state.static_prefs.use_default_mods_path;
        let profile_roots: Vec<PathBuf> = game
            .mods_path(use_default)
            .into_iter()
            .chain(game.disabled_mods_path(use_default))
            .collect();

        if game_exe_names.is_empty() && profile_roots.is_empty() {
            return None;
        }

        // `exe` and `cmd` are opt-in; without these flags `Process::exe` is always `None` and
        // Proton/Wine command-line matches would never fire. Everything else stays off so this
        // stays cheap enough for the UI loop.
        let system = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_processes(
                sysinfo::ProcessRefreshKind::nothing()
                    .with_cmd(sysinfo::UpdateKind::OnlyIfNotSet)
                    .with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
            ),
        );
        let mut game_running = false;
        let mut tool_running = false;
        for process in system.processes().values() {
            if !game_exe_names.is_empty()
                && Self::process_matches_game_exe_names(
                    process.name(),
                    process.cmd(),
                    &game_exe_names,
                )
            {
                game_running = true;
            }
            if !tool_running
                && process
                    .exe()
                    .is_some_and(|exe| profile_roots.iter().any(|root| exe.starts_with(root)))
            {
                tool_running = true;
            }
        }
        if game_running && game.is_unreal_engine() {
            Some(MODS_LOCKED_BLOCK_REASON)
        } else {
            tool_running.then_some("a tool inside the profile folder is running")
        }
    }

    fn game_process_running(&self, game: &GameInstall) -> bool {
        let game_exe_names = Self::game_exe_names(game);
        if game_exe_names.is_empty() {
            return false;
        }

        let system = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_processes(
                sysinfo::ProcessRefreshKind::nothing()
                    .with_cmd(sysinfo::UpdateKind::OnlyIfNotSet),
            ),
        );
        system.processes().values().any(|process| {
            Self::process_matches_game_exe_names(process.name(), process.cmd(), &game_exe_names)
        })
    }

    fn game_exe_names(game: &GameInstall) -> Vec<String> {
        [game.vanilla_exe_path(), game.modded_exe_path()]
            .into_iter()
            .flatten()
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_ascii_lowercase())
            })
            .collect()
    }

    fn process_matches_game_exe_names(
        process_name: &std::ffi::OsStr,
        command_line: &[std::ffi::OsString],
        game_exe_names: &[String],
    ) -> bool {
        if game_exe_names.iter().any(|name| {
            process_name
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        }) {
            return true;
        }

        command_line.iter().any(|arg| {
            let arg = arg.to_string_lossy().to_ascii_lowercase();
            arg.split(['/', '\\'])
                .filter_map(|part| {
                    let trimmed = part.trim_matches(['"', '\'']);
                    (!trimmed.is_empty()).then_some(trimmed)
                })
                .any(|part| game_exe_names.iter().any(|name| part == name))
        })
    }

    /// Profile storage directory for the selected game, if one can be resolved. `None` disables the
    /// menu entry, which happens when the game has no mods path configured yet.
    fn selected_game_profiles_dir(&self) -> Option<PathBuf> {
        let game = self.selected_game()?;
        profiles::profile_roots(game, self.state.static_prefs.use_default_mods_path)
            .ok()
            .map(|roots| roots.profiles_dir)
    }

    pub(crate) fn open_selected_game_profiles_folder(&mut self) {
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        let roots = match profiles::profile_roots(&game, self.state.static_prefs.use_default_mods_path)
        {
            Ok(roots) => roots,
            Err(err) => {
                self.report_error_message(
                    format!("failed to resolve the profile folder: {err:#}"),
                    Some(self.text().could_not_open_location()),
                );
                return;
            }
        };
        // A game whose profiles have never been archived has no storage directory yet. Create it
        // along with the readme that explains the files, so the folder is never opened empty and
        // unexplained.
        if let Err(err) = profiles::ensure_profile_storage_layout(&roots) {
            self.report_error_message(
                format!(
                    "failed to prepare the profile folder {}: {err:#}",
                    roots.profiles_dir.display()
                ),
                Some(self.text().could_not_open_location()),
            );
            return;
        }
        if let Err(err) = open_in_explorer(&roots.profiles_dir) {
            self.report_error_message(
                format!(
                    "failed to open the profile folder {}: {err:#}",
                    roots.profiles_dir.display()
                ),
                Some(self.text().could_not_open_location()),
            );
        }
    }

    fn profile_record_metadata(
        game: &GameInstall,
        id: ProfileId,
        display_name: &str,
        portable_metadata: HashMap<String, serde_json::Value>,
        created_at: Option<DateTime<Utc>>,
        categories: Option<Vec<ModCategory>>,
        tools: Option<ProfileToolSnapshot>,
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
            tools: tools.as_ref().map(|snapshot| snapshot.tools.clone()),
            tool_blacklist: tools.map(|snapshot| snapshot.blacklist),
            source_fingerprint: None,
        }
    }

    /// A profile id no other profile of this game is using.
    ///
    /// 32 bits makes a clash vanishingly unlikely but not impossible, and the id is what keeps two
    /// profiles apart in storage - including two shared ones both called "Default" - so it is
    /// checked rather than assumed.
    fn unused_profile_id(&self, game_id: &str) -> ProfileId {
        let taken: Vec<ProfileId> = self
            .state
            .profiles_by_game
            .get(game_id)
            .map(|catalog| catalog.profiles.iter().map(|profile| profile.id).collect())
            .unwrap_or_default();
        ProfileId::random_unused(&taken)
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

    fn profile_tools_for_game(&self, game_id: &str) -> ProfileToolSnapshot {
        ProfileToolSnapshot {
            tools: self
                .state
                .tools
                .iter()
                .filter(|tool| tool.game_id == game_id)
                .cloned()
                .collect(),
            blacklist: self
                .state
                .static_prefs
                .tool_blacklist
                .get(game_id)
                .cloned()
                .unwrap_or_default(),
        }
    }

    fn sanitize_profile_tools(game_id: &str, tools: Vec<ToolEntry>) -> Vec<ToolEntry> {
        tools
            .into_iter()
            .map(|mut tool| {
                tool.game_id = game_id.to_string();
                tool
            })
            .collect()
    }

    /// Capture the live tool state into the profile that is about to be switched away from, so its
    /// launch options and pins come back untouched the next time it is activated.
    fn snapshot_active_profile_tools(
        &mut self,
        game_id: &str,
        active_id: Option<ProfileId>,
    ) -> ProfileToolSnapshot {
        let snapshot = self.profile_tools_for_game(game_id);
        let mut changed = false;
        if let Some(active_id) = active_id {
            if let Some(catalog) = self.state.profiles_by_game.get_mut(game_id) {
                if let Some(profile) = catalog
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id == active_id)
                {
                    if profile.tools.as_ref() != Some(&snapshot.tools)
                        || profile.tool_blacklist.as_ref() != Some(&snapshot.blacklist)
                    {
                        profile.tools = Some(snapshot.tools.clone());
                        profile.tool_blacklist = Some(snapshot.blacklist.clone());
                        profile.updated_at = Some(Utc::now());
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.save_state();
        }
        snapshot
    }

    fn profile_tool_snapshot_for(&self, game_id: &str, id: ProfileId) -> Option<ProfileToolSnapshot> {
        let profile = self
            .state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| catalog.profiles.iter().find(|profile| profile.id == id))?;
        profile.tools.as_ref().map(|tools| ProfileToolSnapshot {
            tools: tools.clone(),
            blacklist: profile.tool_blacklist.clone().unwrap_or_default(),
        })
    }

    /// Swap the live tool set over to the profile that just became active. Tools whose executable
    /// is gone are pruned by the tool sync that follows the queued game refresh, so restoring the
    /// full recorded set here is what preserves launch options for everything that survives.
    fn restore_profile_tools(&mut self, game_id: &str, snapshot: ProfileToolSnapshot) {
        let retired: Vec<String> = self
            .state
            .tools
            .iter()
            .filter(|tool| tool.game_id == game_id)
            .map(|tool| tool.id.clone())
            .collect();
        for tool_id in retired {
            self.tool_icon_textures.remove(&tool_id);
            self.tool_icon_texture_failures.remove(&tool_id);
        }
        self.state.tools.retain(|tool| tool.game_id != game_id);
        self.state
            .tools
            .extend(Self::sanitize_profile_tools(game_id, snapshot.tools));

        // Two profiles can hold different executables at the same path, so never carry a cached
        // icon across the swap even when a restored tool keeps its id.
        for tool in self.state.tools.iter().filter(|tool| tool.game_id == game_id) {
            self.tool_icon_textures.remove(&tool.id);
            self.tool_icon_texture_failures.remove(&tool.id);
        }

        if snapshot.blacklist.is_empty() {
            self.state.static_prefs.tool_blacklist.remove(game_id);
        } else {
            self.state
                .static_prefs
                .tool_blacklist
                .insert(game_id.to_string(), snapshot.blacklist);
        }

        // Restored orders were compacted against the other profile's tool set; rebuild both so the
        // window list and the four titlebar slots are contiguous and within their limits again.
        self.compact_tool_window_order_for_game(game_id);
        self.enforce_tool_titlebar_limit_for_game(game_id);
        self.compact_tool_titlebar_order_for_game(game_id);
    }

    fn snapshot_active_profile_categories(
        &mut self,
        game_id: &str,
        active_id: Option<ProfileId>,
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

    fn profile_storage_file_name(profile: &ProfileRecord) -> Option<&str> {
        profile
            .portable_metadata
            .get(PROFILE_STORAGE_FILE_NAME_METADATA_KEY)
            .and_then(|value| value.as_str())
            .filter(|name| {
                !name.is_empty()
                    && Path::new(name).file_name().is_some_and(|file_name| file_name == *name)
            })
    }

    fn profile_storage_name_key(file_name: &str) -> Option<(bool, ProfileId)> {
        let archive_suffix = format!(".{}", profiles::PROFILE_ARCHIVE_EXTENSION);
        let (stem, archive) = file_name
            .strip_suffix(&archive_suffix)
            .map(|stem| (stem, true))
            .unwrap_or((file_name, false));
        let (_, short_id) = profiles::parse_profile_storage_stem(stem)?;
        Some((archive, short_id.parse().ok()?))
    }

    fn rename_exact_profile_storage_file(
        roots: &profiles::ProfileRoots,
        current_file_name: &str,
        display_name: &str,
    ) -> Option<String> {
        Self::try_rename_exact_profile_storage_file(roots, current_file_name, display_name).ok()
    }

    fn try_rename_exact_profile_storage_file(
        roots: &profiles::ProfileRoots,
        current_file_name: &str,
        display_name: &str,
    ) -> Result<String> {
        let current = roots.profiles_dir.join(current_file_name);
        if !current.exists() {
            bail!("profile storage is missing: {}", current.display());
        }
        let file_name = current
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("profile storage name is invalid"))?;
        let archive_suffix = format!(".{}", profiles::PROFILE_ARCHIVE_EXTENSION);
        let (stem, is_archive) = file_name
            .strip_suffix(&archive_suffix)
            .map(|stem| (stem, true))
            .unwrap_or((file_name, false));
        let (_, short_id) = profiles::parse_profile_storage_stem(stem)
            .ok_or_else(|| anyhow!("profile storage name is invalid"))?;
        let new_stem = format!(
            "{} [{}]",
            profiles::sanitize_profile_label(display_name),
            short_id.to_ascii_lowercase()
        );
        let new_file_name = if is_archive {
            format!("{new_stem}.{}", profiles::PROFILE_ARCHIVE_EXTENSION)
        } else {
            new_stem
        };
        if new_file_name == current_file_name {
            return Ok(new_file_name);
        }
        let destination = roots.profiles_dir.join(&new_file_name);
        if destination.exists() {
            bail!("profile storage already exists: {}", destination.display());
        }
        std::fs::rename(&current, &destination)?;
        Ok(new_file_name)
    }

    fn profile_bound_storage_path(
        roots: &profiles::ProfileRoots,
        storage_file_name: &str,
        archive: bool,
    ) -> PathBuf {
        let archive_suffix = format!(".{}", profiles::PROFILE_ARCHIVE_EXTENSION);
        let (stem, storage_is_archive) = storage_file_name
            .strip_suffix(&archive_suffix)
            .map(|stem| (stem, true))
            .unwrap_or((storage_file_name, false));
        let file_name = if archive {
            format!("{stem}.{}", profiles::PROFILE_ARCHIVE_EXTENSION)
        } else {
            stem.to_string()
        };
        if archive == storage_is_archive {
            roots.profiles_dir.join(storage_file_name)
        } else {
            roots.profiles_dir.join(file_name)
        }
    }

    fn profile_bound_storage_exists(
        roots: &profiles::ProfileRoots,
        storage_file_name: &str,
    ) -> bool {
        Self::profile_bound_storage_path(roots, storage_file_name, true).is_file()
            || Self::profile_bound_storage_path(roots, storage_file_name, false).is_dir()
    }

    fn profile_record_storage_exists(
        roots: &profiles::ProfileRoots,
        profile: &ProfileRecord,
    ) -> bool {
        if let Some(storage_file_name) = Self::profile_storage_file_name(profile) {
            Self::profile_bound_storage_exists(roots, storage_file_name)
        } else {
            roots.archive_path(profile.id).is_file() || roots.profile_path(profile.id).is_dir()
        }
    }

    fn prune_missing_inactive_profile_records_from_catalog(
        catalog: &mut ProfileCatalog,
        roots: &profiles::ProfileRoots,
    ) -> Vec<ProfileId> {
        let active_profile_id = catalog.active_profile_id;
        let mut removed = Vec::new();
        catalog.profiles.retain(|profile| {
            if Some(profile.id) == active_profile_id
                || Self::profile_record_storage_exists(roots, profile)
            {
                true
            } else {
                removed.push(profile.id);
                false
            }
        });
        removed
    }

    fn prune_missing_inactive_profile_records(&mut self, game_id: &str) -> Vec<ProfileId> {
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
        else {
            return Vec::new();
        };
        let Ok(roots) = profiles::profile_roots(&game, self.state.static_prefs.use_default_mods_path)
        else {
            return Vec::new();
        };
        let removed = self
            .state
            .profiles_by_game
            .get_mut(game_id)
            .map(|catalog| {
                Self::prune_missing_inactive_profile_records_from_catalog(catalog, &roots)
            })
            .unwrap_or_default();
        if !removed.is_empty() {
            for profile_id in &removed {
                self.profile_compression_states
                    .remove(&(game_id.to_string(), *profile_id));
            }
            self.save_state();
        }
        removed
    }

    fn profile_error_is_storage_out_of_date(error: &anyhow::Error) -> bool {
        error.chain().any(|cause| {
            let message = cause.to_string();
            message.contains(PROFILE_STORAGE_OUT_OF_DATE_ERROR)
                || message.contains("profile storage is missing")
        })
    }

    fn profile_storage_path_for(
        &self,
        game_id: &str,
        game: &GameInstall,
        id: ProfileId,
        archive: bool,
    ) -> Result<PathBuf> {
        let roots = profiles::profile_roots(game, self.state.static_prefs.use_default_mods_path)?;
        if let Some(storage_file_name) = self
            .state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| catalog.profiles.iter().find(|profile| profile.id == id))
            .and_then(Self::profile_storage_file_name)
        {
            return Ok(Self::profile_bound_storage_path(
                &roots,
                storage_file_name,
                archive,
            ));
        }
        Ok(if archive {
            roots.archive_path(id)
        } else {
            roots.profile_path(id)
        })
    }

    fn profile_archive_for(
        &self,
        game_id: &str,
        game: &GameInstall,
        id: ProfileId,
    ) -> Result<PathBuf> {
        self.profile_storage_path_for(game_id, game, id, true)
    }

    fn profile_path_for(&self, game_id: &str, game: &GameInstall, id: ProfileId) -> Result<PathBuf> {
        self.profile_storage_path_for(game_id, game, id, false)
    }

    fn active_profile_id(&self, game_id: &str) -> Option<ProfileId> {
        self.state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| catalog.active_profile_id)
    }

    fn known_profile_ids_for_game(&self, game_id: &str) -> Vec<ProfileId> {
        self.state
            .profiles_by_game
            .get(game_id)
            .map(|catalog| catalog.profiles.iter().map(|profile| profile.id).collect())
            .unwrap_or_default()
    }

    fn known_profile_storage_file_names_for_game(&self, game_id: &str) -> Vec<String> {
        self.state
            .profiles_by_game
            .get(game_id)
            .map(|catalog| {
                let mut names = Vec::new();
                for profile in &catalog.profiles {
                    if let Some(name) = Self::profile_storage_file_name(profile) {
                        names.push(name.to_string());
                    }
                    let stem = profiles::profile_storage_stem(&profile.display_name, profile.id);
                    names.push(stem.clone());
                    names.push(format!(
                        "{stem}.{}",
                        profiles::PROFILE_ARCHIVE_EXTENSION
                    ));
                }
                names
            })
            .unwrap_or_default()
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
        profile_id: Option<ProfileId>,
        source_profile_id: Option<ProfileId>,
        target_profile_id: Option<ProfileId>,
        display_name: Option<String>,
        target_display_name: Option<String>,
        source_profile: Option<PathBuf>,
        target_profile: Option<PathBuf>,
        source_archive: Option<PathBuf>,
        target_archive: Option<PathBuf>,
        target_categories: Option<Vec<ModCategory>>,
        target_tools: Option<ProfileToolSnapshot>,
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
        let known_profile_ids = self.known_profile_ids_for_game(&game_id);
        let known_profile_storage_file_names =
            self.known_profile_storage_file_names_for_game(&game_id);
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
            source_profile,
            target_profile,
            source_archive,
            target_archive,
            target_categories,
            target_tools: target_tools
                .as_ref()
                .map(|snapshot| snapshot.tools.clone()),
            target_tool_blacklist: target_tools.map(|snapshot| snapshot.blacklist),
            known_profile_ids,
            known_profile_storage_file_names,
            metadata,
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

    fn profile_actions_paused_reason(&self, text: TextCatalog) -> Option<&'static str> {
        self.profile_operation_block_reason(ProfileOperationKind::Switch)
            .and_then(|reason| Self::profile_actions_paused_reason_text(text, reason))
    }

    fn profile_actions_paused_reason_text(
        text: TextCatalog,
        reason: &str,
    ) -> Option<&'static str> {
        match reason {
            "another profile operation is already running" => {
                Some(text.profile_actions_paused_profile_operation())
            }
            "a game refresh is running" | "mod refresh is running" => {
                Some(text.profile_actions_paused_refreshing_library())
            }
            "mod installation is running" => Some(text.profile_actions_paused_installing_mods()),
            "update checking is running" => Some(text.profile_actions_paused_checking_updates()),
            "an app update is running" => Some(text.profile_actions_paused_updating_hestia()),
            MODS_LOCKED_BLOCK_REASON => Some(text.profile_actions_paused_mods_locked()),
            "the selected game is running" => Some(text.profile_actions_paused_game_running()),
            "a tool inside the profile folder is running" => {
                Some(text.profile_actions_paused_profile_tool_running())
            }
            _ => None,
        }
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

    pub(crate) fn request_switch_profile(&mut self, id: ProfileId) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        self.switch_profile(&game_id, id)
    }

    pub(crate) fn request_rename_profile(&mut self, id: ProfileId, name: String) -> Result<()> {
        let game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone())
            .ok_or_else(|| anyhow!("no game is selected"))?;
        self.rename_profile(&game_id, id, name)
    }

    pub(crate) fn request_delete_profile(&mut self, id: ProfileId) -> Result<()> {
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
        let current_tools = self.profile_tools_for_game(game_id);
        let mut migrated = false;
        let existing_ready = if let Some(catalog) = self.state.profiles_by_game.get_mut(game_id) {
            for profile in &mut catalog.profiles {
                if profile.categories.is_none() {
                    profile.categories = Some(current_categories.clone());
                    migrated = true;
                }
                if profile.tools.is_none() {
                    profile.tools = Some(current_tools.tools.clone());
                    profile.tool_blacklist = Some(current_tools.blacklist.clone());
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
        let id = self.unused_profile_id(game_id);
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
            tools: Some(current_tools.tools),
            tool_blacklist: Some(current_tools.blacklist),
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
        let active_tools = self.snapshot_active_profile_tools(game_id, active_id);
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
                        Some(active_tools.clone()),
                    )
                })
        });
        let target_id = self.unused_profile_id(game_id);
        let active_archive =
            active_id.and_then(|id| self.profile_archive_for(game_id, &game, id).ok());
        // A new profile starts empty: no categories and no tools. Its mods folder is empty, so any
        // inherited tool would either be pruned by the sync that follows or dangle at a path this
        // profile has nothing at.
        self.begin_profile_operation(
            game_id.to_string(),
            game,
            ProfileOperationKind::Create,
            active_id,
            None,
            Some(target_id),
            metadata.as_ref().map(|m| m.display_name.clone()),
            Some(display_name),
            None,
            None,
            active_archive,
            None,
            Some(Vec::new()),
            Some(ProfileToolSnapshot::default()),
            metadata,
        )
    }

    pub(crate) fn duplicate_profile(
        &mut self,
        game_id: &str,
        source_id: ProfileId,
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
        let active_tools = self.snapshot_active_profile_tools(game_id, active_id);
        let source_archive = if Some(source_id) == active_id {
            active_id.and_then(|id| self.profile_archive_for(game_id, &game, id).ok())
        } else {
            Some(self.profile_archive_for(game_id, &game, source_id)?)
        };
        let source_profile = if Some(source_id) == active_id {
            None
        } else {
            Some(self.profile_path_for(game_id, &game, source_id)?)
        };
        let target_id = self.unused_profile_id(game_id);
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
                        Some(active_tools.clone()),
                    )
                })
        });
        let source_categories = source_record
            .categories
            .clone()
            .or_else(|| Some(active_categories.clone()));
        // A duplicate is a copy of the source, so it inherits the source's tools; fall back to the
        // live set when the source predates profile-scoped tools.
        let source_tools = self
            .profile_tool_snapshot_for(game_id, source_id)
            .unwrap_or_else(|| active_tools.clone());
        self.begin_profile_operation(
            game_id.to_string(),
            game,
            ProfileOperationKind::Duplicate,
            active_id,
            Some(source_id),
            Some(target_id),
            Some(source_record.display_name.clone()),
            Some(display_name),
            source_profile,
            None,
            source_archive,
            None,
            source_categories,
            Some(source_tools),
            metadata,
        )
    }

    pub(crate) fn switch_profile(&mut self, game_id: &str, target_id: ProfileId) -> Result<()> {
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
        let active_tools = self.snapshot_active_profile_tools(game_id, active_id);
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
        let target_archive = self.profile_archive_for(game_id, &game, target_id)?;
        let target_profile = self.profile_path_for(game_id, &game, target_id)?;
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
                        Some(active_tools),
                    )
                })
        });
        let active_archive =
            active_id.and_then(|id| self.profile_archive_for(game_id, &game, id).ok());
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
        let target_tools = self.profile_tool_snapshot_for(game_id, target_id);
        self.begin_profile_operation(
            game_id.to_string(),
            game,
            ProfileOperationKind::Switch,
            active_id,
            None,
            Some(target_id),
            None,
            target_display_name,
            None,
            Some(target_profile),
            active_archive,
            Some(target_archive),
            target_categories,
            target_tools,
            metadata,
        )
    }

    pub(crate) fn rename_profile(
        &mut self,
        game_id: &str,
        profile_id: ProfileId,
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
        let (active_profile, current_display_name, current_storage_file_name) = catalog
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .map(|profile| {
                (
                    catalog.active_profile_id == Some(profile_id),
                    profile.display_name.clone(),
                    Self::profile_storage_file_name(profile).map(str::to_owned),
                )
            })
            .expect("profile checked above");
        if current_display_name == normalized {
            return Ok(());
        }
        if !active_profile
            && let Some(current_storage_file_name) = current_storage_file_name.as_deref()
        {
            let roots = self
                .state
                .games
                .iter()
                .find(|game| game.definition.id == game_id)
                .ok_or_else(|| anyhow!("game {game_id} is not configured"))
                .and_then(|game| {
                    profiles::profile_roots(game, self.state.static_prefs.use_default_mods_path)
                })?;
            if !Self::profile_bound_storage_exists(&roots, current_storage_file_name) {
                self.queue_profile_recovery_for_game(game_id);
                bail!("could not rename profile because {PROFILE_STORAGE_OUT_OF_DATE_ERROR}");
            }
        }
        if catalog.profiles.iter().any(|other| {
            other.id != profile_id
                && TextCatalog::profile_names_equal(&other.display_name, normalized)
        }) {
            bail!("a profile with that name already exists");
        }
        let mut renamed_storage_file_name = None;
        // Inactive profile storage is labelled with the profile name. The active profile lives in
        // the normal mods folders, so active renames do not touch any inactive same-marker copy.
        if !active_profile
            && let Some(current_storage_file_name) = current_storage_file_name.as_deref()
        {
            let roots = self
                .state
                .games
                .iter()
                .find(|game| game.definition.id == game_id)
                .ok_or_else(|| anyhow!("game {game_id} is not configured"))
                .and_then(|game| {
                    profiles::profile_roots(game, self.state.static_prefs.use_default_mods_path)
                })?;
            match Self::try_rename_exact_profile_storage_file(
                &roots,
                current_storage_file_name,
                normalized,
            ) {
                Ok(new_storage_file_name) => {
                    renamed_storage_file_name = Some(new_storage_file_name);
                }
                Err(error) => {
                    self.queue_profile_recovery_for_game(game_id);
                    bail!(
                        "could not rename profile because {PROFILE_STORAGE_OUT_OF_DATE_ERROR}: {error}"
                    );
                }
            }
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
        profile.display_name = normalized.to_string();
        profile.updated_at = Some(Utc::now());
        if let Some(new_storage_file_name) = renamed_storage_file_name {
            profile.portable_metadata.insert(
                PROFILE_STORAGE_FILE_NAME_METADATA_KEY.to_string(),
                serde_json::Value::String(new_storage_file_name),
            );
        }
        let renamed_stored = profile.display_name.clone();
        self.save_state();
        if !active_profile
            && current_storage_file_name.is_none()
            && let Some(game) = self.state.games.iter().find(|game| game.definition.id == game_id)
            && let Ok(roots) =
                profiles::profile_roots(game, self.state.static_prefs.use_default_mods_path)
        {
            rename_profile_storage_label(&roots, profile_id, &renamed_stored);
        }
        let renamed = self.text().profile_display_name(&renamed_stored);
        self.set_message_ok(self.text().profile_renamed(&renamed));
        Ok(())
    }

    pub(crate) fn delete_profile(&mut self, game_id: &str, profile_id: ProfileId) -> Result<()> {
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
        let archive = self.profile_archive_for(game_id, &game, profile_id)?;
        let profile = self.profile_path_for(game_id, &game, profile_id)?;
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
        let current_storage_file_name = self
            .state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| catalog.profiles.iter().find(|profile| profile.id == profile_id))
            .and_then(Self::profile_storage_file_name)
            .map(str::to_owned);
        if let Some(current_storage_file_name) = current_storage_file_name.as_deref() {
            let roots = profiles::profile_roots(&game, self.state.static_prefs.use_default_mods_path)?;
            if !Self::profile_bound_storage_exists(&roots, current_storage_file_name) {
                self.queue_profile_recovery_for_game(game_id);
                bail!("could not delete profile because {PROFILE_STORAGE_OUT_OF_DATE_ERROR}");
            }
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
            Some(profile),
            None,
            Some(archive),
            None,
            None,
            None,
            None,
        )
    }

    fn dispatch_profile_recovery(&mut self) {
        self.profile_recovery_failed = false;
        self.profile_recovery_queue = self.state.games.clone().into();
        self.dispatch_next_profile_recovery();
    }

    fn queue_profile_recovery_for_game(&mut self, game_id: &str) {
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
        else {
            return;
        };
        self.profile_recovery_failed = false;
        if self
            .profile_recovery_queue
            .iter()
            .any(|queued| queued.definition.id == game.definition.id)
        {
            return;
        }
        self.profile_recovery_queue.push_back(game);
        self.dispatch_next_profile_recovery();
    }

    fn queue_profile_reconcile_for_game(&mut self, game_id: &str) {
        if self.profile_operation_inflight.is_some()
            || self
                .profile_recovery_queue
                .iter()
                .any(|game| game.definition.id == game_id)
            || self.profile_reconcile_inflight.contains(game_id)
        {
            return;
        }
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
        else {
            return;
        };
        let spec = ProfileReconcileSpec {
            game_id: game_id.to_string(),
            game,
            use_default_mods_path: self.state.static_prefs.use_default_mods_path,
            known_profile_ids: self.known_profile_ids_for_game(game_id),
            known_profile_storage_file_names: self.known_profile_storage_file_names_for_game(game_id),
        };
        if self
            .profile_reconcile_request_tx
            .send(spec)
            .is_ok()
        {
            self.profile_reconcile_inflight.insert(game_id.to_string());
        } else {
            self.report_error_message(
                format!("profile reconcile could not start for {game_id}"),
                None,
            );
        }
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
        profile_id: Option<ProfileId>,
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

    /// Restore catalog records for stored profiles the catalog had lost, so they become visible
    /// and switchable again instead of sitting on disk unreachable. Each record is rebuilt from
    /// the profile's own embedded metadata; `tools` is deliberately left `None` on profiles that
    /// predate profile-scoped tools so the usual migration seeds them on first use.
    fn adopt_orphaned_profiles(
        &mut self,
        game_id: &str,
        orphaned: Vec<OrphanedProfile>,
    ) -> Vec<String> {
        if orphaned.is_empty() {
            return Vec::new();
        }
        let roots = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .and_then(|game| {
                profiles::profile_roots(game, self.state.static_prefs.use_default_mods_path).ok()
            });
        let catalog = self
            .state
            .profiles_by_game
            .entry(game_id.to_string())
            .or_default();
        let adopted =
            Self::adopt_orphaned_profiles_into_catalog(catalog, roots.as_ref(), orphaned);
        if !adopted.is_empty() {
            self.save_state();
        }
        adopted
    }

    fn adopt_orphaned_profiles_into_catalog(
        catalog: &mut ProfileCatalog,
        roots: Option<&profiles::ProfileRoots>,
        orphaned: Vec<OrphanedProfile>,
    ) -> Vec<String> {
        let orphan_key_counts = orphaned.iter().fold(
            HashMap::<(bool, ProfileId), usize>::new(),
            |mut counts, orphan| {
                if let Some(key) = orphan
                    .storage_file_name
                    .as_deref()
                    .and_then(Self::profile_storage_name_key)
                {
                    *counts.entry(key).or_default() += 1;
                }
                counts
            },
        );
        let mut adopted = Vec::new();
        for orphan in orphaned {
            let metadata = orphan.metadata.clone();
            if let (Some(roots), Some(storage_file_name), Some(key)) = (
                roots,
                orphan.storage_file_name.as_deref(),
                orphan
                    .storage_file_name
                    .as_deref()
                    .and_then(Self::profile_storage_name_key),
            ) && orphan_key_counts.get(&key) == Some(&1)
            {
                let stale_matches: Vec<ProfileId> = catalog
                    .profiles
                    .iter()
                    .filter(|profile| Some(profile.id) != catalog.active_profile_id)
                    .filter_map(|profile| {
                        let current = Self::profile_storage_file_name(profile)?;
                        if roots.profiles_dir.join(current).exists() {
                            return None;
                        }
                        (Self::profile_storage_name_key(current) == Some(key))
                            .then_some(profile.id)
                    })
                    .collect();
                if let [profile_id] = stale_matches.as_slice() {
                    let preferred = orphan.label.as_deref().unwrap_or(&metadata.display_name);
                    let display_name = Self::unique_adopted_profile_name_except(
                        catalog,
                        preferred,
                        Some(*profile_id),
                    );
                    if let Some(profile) = catalog
                        .profiles
                        .iter_mut()
                        .find(|profile| profile.id == *profile_id)
                    {
                        profile.display_name = display_name.clone();
                        profile.archive_size = orphan.archive_size;
                        profile.uncompressed_size = Some(metadata.uncompressed_size);
                        profile.file_count = Some(metadata.file_count);
                        profile.created_at = profile.created_at.or(Some(metadata.created_at));
                        profile.updated_at = orphan.updated_at;
                        profile.portable_metadata.insert(
                            PROFILE_STORAGE_FILE_NAME_METADATA_KEY.to_string(),
                            serde_json::Value::String(storage_file_name.to_string()),
                        );
                        if profile.categories.is_none() {
                            profile.categories = metadata.categories;
                        }
                    }
                    adopted.push(display_name);
                    continue;
                }
            }
            if catalog
                .profiles
                .iter()
                .any(|profile| profile.id == orphan.profile_id)
            {
                continue;
            }
            // The embedded name is only refreshed when a profile cycles through being active, so a
            // profile renamed while archived comes back under its previous name. Keeping a
            // duplicate name would then be confusing, so disambiguate rather than collide.
            // Prefer the name on disk: renames keep it current, while the embedded name is
            // whatever the profile was called the last time its archive was written.
            let preferred = orphan.label.as_deref().unwrap_or(&metadata.display_name);
            let display_name = Self::unique_adopted_profile_name(catalog, preferred);
            let mut portable_metadata = metadata.portable_metadata;
            if let Some(storage_file_name) = orphan.storage_file_name {
                portable_metadata.insert(
                    PROFILE_STORAGE_FILE_NAME_METADATA_KEY.to_string(),
                    serde_json::Value::String(storage_file_name),
                );
            }
            catalog.profiles.push(ProfileRecord {
                id: orphan.profile_id,
                display_name: display_name.clone(),
                archive_size: orphan.archive_size,
                uncompressed_size: Some(metadata.uncompressed_size),
                file_count: Some(metadata.file_count),
                created_at: Some(metadata.created_at),
                updated_at: orphan.updated_at,
                portable_metadata,
                categories: metadata.categories,
                // Deliberately not adopted from the archive. A profile archive is shareable and
                // can be dropped into the folder by hand, and a tool entry carries an executable
                // path and launch arguments - importing those would let a shared archive place a
                // one-click arbitrary command in the toolbar. Leaving this `None` makes the usual
                // migration rediscover tools from this profile's own mods folder instead.
                tools: None,
                tool_blacklist: None,
            });
            adopted.push(display_name);
        }
        adopted
    }

    fn apply_recovered_profile_labels_to_catalog(
        catalog: &mut ProfileCatalog,
        renamed_profiles: Vec<RecoveredProfileLabel>,
    ) {
        for recovered in renamed_profiles {
            if catalog.active_profile_id == Some(recovered.profile_id) {
                continue;
            }
            if catalog.profiles.iter().any(|profile| {
                profile.id != recovered.profile_id
                    && TextCatalog::profile_names_equal(&profile.display_name, &recovered.label)
            }) {
                continue;
            }
            if let Some(profile) = catalog
                .profiles
                .iter_mut()
                .find(|profile| profile.id == recovered.profile_id)
            {
                if profile.display_name == recovered.label
                    || profiles::sanitize_profile_label(&profile.display_name) == recovered.label
                {
                    continue;
                }
                profile.display_name = recovered.label;
                profile.updated_at = Some(Utc::now());
            }
        }
    }

    fn apply_profile_reconcile_result(
        &mut self,
        game_id: &str,
        orphaned_profiles: Vec<OrphanedProfile>,
        renamed_profiles: Vec<RecoveredProfileLabel>,
        warnings: Vec<String>,
    ) {
        let adopted = self.adopt_orphaned_profiles(game_id, orphaned_profiles);
        if let Some(catalog) = self.state.profiles_by_game.get_mut(game_id) {
            Self::apply_recovered_profile_labels_to_catalog(catalog, renamed_profiles);
        }
        let pruned_profiles = self.prune_missing_inactive_profile_records(game_id);
        self.sync_profile_storage_labels(game_id);
        self.save_state();

        for profile_id in pruned_profiles {
            self.push_log(format!(
                "Profile removed from catalog because its storage is missing: {game_id}/{profile_id}"
            ));
        }
        for warning in warnings {
            self.report_error_message(warning, None);
        }
        if !adopted.is_empty() {
            let text = self.text();
            for name in &adopted {
                self.log_action(
                    text.profile_action_recovered(),
                    &text.profile_display_name(name),
                );
            }
        }
    }

    fn unique_adopted_profile_name(catalog: &ProfileCatalog, preferred: &str) -> String {
        Self::unique_adopted_profile_name_except(catalog, preferred, None)
    }

    fn unique_adopted_profile_name_except(
        catalog: &ProfileCatalog,
        preferred: &str,
        except: Option<ProfileId>,
    ) -> String {
        let preferred = preferred.trim();
        let preferred = if preferred.is_empty() {
            "Recovered profile"
        } else {
            preferred
        };
        if !catalog
            .profiles
            .iter()
            .any(|profile| {
                Some(profile.id) != except
                    && TextCatalog::profile_names_equal(&profile.display_name, preferred)
            })
        {
            return preferred.to_string();
        }
        for suffix in 2u32.. {
            let candidate = format!("{preferred} ({suffix})");
            if !catalog
                .profiles
                .iter()
                .any(|profile| {
                    Some(profile.id) != except
                        && TextCatalog::profile_names_equal(&profile.display_name, &candidate)
                })
            {
                return candidate;
            }
        }
        unreachable!()
    }

    /// Bring stored names back in line with the current catalog after recovery has folded any
    /// inactive Explorer renames into app state.
    ///
    /// The active profile lives in the normal mods folders, so same-id storage under
    /// `Mods_Profiles` is stale and is removed. Inactive profiles keep exactly one readable storage
    /// name that follows the catalog name.
    fn sync_profile_storage_labels(&mut self, game_id: &str) {
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
        else {
            return;
        };
        let Ok(roots) = profiles::profile_roots(&game, self.state.static_prefs.use_default_mods_path)
        else {
            return;
        };
        let (active_profile_id, named): (
            Option<ProfileId>,
            Vec<(ProfileId, String, Option<String>)>,
        ) = self
            .state
            .profiles_by_game
            .get(game_id)
            .map(|catalog| {
                (
                    catalog.active_profile_id,
                    catalog
                        .profiles
                        .iter()
                        .map(|profile| {
                            (
                                profile.id,
                                profile.display_name.clone(),
                                Self::profile_storage_file_name(profile).map(str::to_owned),
                            )
                        })
                        .collect(),
                )
            })
            .unwrap_or_default();
        let mut changed = false;
        for (profile_id, display_name, storage_file_name) in named {
            if Some(profile_id) == active_profile_id {
                continue;
            } else if let Some(storage_file_name) = storage_file_name.as_deref() {
                if let Some(new_storage_file_name) =
                    Self::rename_exact_profile_storage_file(&roots, storage_file_name, &display_name)
                    && new_storage_file_name != storage_file_name
                    && let Some(profile) = self
                        .state
                        .profiles_by_game
                        .get_mut(game_id)
                        .and_then(|catalog| {
                            catalog
                                .profiles
                                .iter_mut()
                                .find(|profile| profile.id == profile_id)
                        })
                {
                    profile.portable_metadata.insert(
                        PROFILE_STORAGE_FILE_NAME_METADATA_KEY.to_string(),
                        serde_json::Value::String(new_storage_file_name),
                    );
                    changed = true;
                }
            } else {
                rename_profile_storage_label(&roots, profile_id, &display_name);
            }
        }
        if changed {
            self.save_state();
        }
    }

    fn profile_log_display_name(&self, game_id: &str, profile_id: ProfileId) -> String {
        self.state
            .profiles_by_game
            .get(game_id)
            .and_then(|catalog| {
                catalog
                    .profiles
                    .iter()
                    .find(|profile| profile.id == profile_id)
            })
            .map(|profile| self.text().profile_display_name(&profile.display_name))
            .unwrap_or_else(|| profile_id.to_string())
    }

    fn consume_profile_events(&mut self) {
        while let Ok(event) = self.profile_event_rx.try_recv() {
            match event {
                ProfileEvent::ArchiveQueued {
                    game_id,
                    profile_id,
                    delay_seconds,
                } => {
                    self.profile_compression_states.insert(
                        (game_id.clone(), profile_id),
                        ProfileCompressionUiState::Queued,
                    );
                    let name = self.profile_log_display_name(&game_id, profile_id);
                    self.push_log(format!(
                        "Profile compression queued ({delay_seconds}-second delay): {name}"
                    ));
                    continue;
                }
                ProfileEvent::ArchiveStarted {
                    game_id,
                    profile_id,
                } => {
                    self.profile_compression_states.insert(
                        (game_id.clone(), profile_id),
                        ProfileCompressionUiState::Running,
                    );
                    let name = self.profile_log_display_name(&game_id, profile_id);
                    self.push_log(format!("Profile compression started: {name}"));
                    continue;
                }
                ProfileEvent::ArchiveSkipped {
                    game_id,
                    profile_id,
                } => {
                    self.profile_compression_states
                        .remove(&(game_id.clone(), profile_id));
                    let name = self.profile_log_display_name(&game_id, profile_id);
                    self.push_log(format!(
                        "Profile compression skipped because loose profile data was no longer present: {name}"
                    ));
                    continue;
                }
                ProfileEvent::ArchiveCompleted {
                    game_id,
                    profile_id,
                    archive,
                } => {
                    self.profile_compression_states
                        .remove(&(game_id.clone(), profile_id));
                    let name = self.profile_log_display_name(&game_id, profile_id);
                    self.apply_profile_archive_result(&game_id, Some(profile_id), &archive);
                    self.save_state();
                    self.push_log(format!("Profile compression completed: {name}"));
                    continue;
                }
                ProfileEvent::ArchiveFailed {
                    game_id,
                    profile_id,
                    error,
                } => {
                    self.profile_compression_states.insert(
                        (game_id.clone(), profile_id),
                        ProfileCompressionUiState::Failed,
                    );
                    self.report_error_message(
                        format!(
                            "background profile compression failed for {game_id}/{profile_id}: {error}"
                        ),
                        Some("Profile compression failed"),
                    );
                    continue;
                }
                ProfileEvent::ReconcileCompleted {
                    game_id,
                    orphaned_profiles,
                    renamed_profiles,
                    warnings,
                } => {
                    self.profile_reconcile_inflight.remove(&game_id);
                    self.apply_profile_reconcile_result(
                        &game_id,
                        orphaned_profiles,
                        renamed_profiles,
                        warnings,
                    );
                    continue;
                }
                ProfileEvent::ReconcileFailed { game_id, error } => {
                    self.profile_reconcile_inflight.remove(&game_id);
                    self.report_error_message(
                        format!("profile reconcile failed for {game_id}: {error}"),
                        None,
                    );
                    continue;
                }
                event => {
                    let event_id = match &event {
                        ProfileEvent::Completed { operation_id, .. }
                        | ProfileEvent::Failed { operation_id, .. }
                        | ProfileEvent::Canceled { operation_id, .. } => *operation_id,
                        ProfileEvent::ArchiveQueued { .. }
                        | ProfileEvent::ArchiveStarted { .. }
                        | ProfileEvent::ArchiveSkipped { .. }
                        | ProfileEvent::ArchiveCompleted { .. }
                        | ProfileEvent::ArchiveFailed { .. }
                        | ProfileEvent::ReconcileCompleted { .. }
                        | ProfileEvent::ReconcileFailed { .. } => unreachable!(),
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
                    } else {
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
            ProfileEvent::Completed { completed, .. } => {
                let ProfileCompleted {
                    game_id,
                    kind,
                    profile_id,
                    target_profile_id,
                    display_name,
                    archive,
                    active_profile_marker,
                    orphaned_profiles,
                    renamed_profiles,
                    warnings,
                } = *completed;
                let adopted = self.adopt_orphaned_profiles(&game_id, orphaned_profiles);
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
                let active_profile_tools = active_profile_marker.as_ref().and_then(|marker| {
                    marker.tools.clone().map(|tools| ProfileToolSnapshot {
                        tools,
                        blacklist: marker.tool_blacklist.clone().unwrap_or_default(),
                    })
                });
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
                                tools: Some(
                                    active_profile_tools
                                        .as_ref()
                                        .map(|snapshot| snapshot.tools.clone())
                                        .unwrap_or_default(),
                                ),
                                tool_blacklist: Some(
                                    active_profile_tools
                                        .as_ref()
                                        .map(|snapshot| snapshot.blacklist.clone())
                                        .unwrap_or_default(),
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
                                    tools: Some(
                                        active_profile_tools
                                            .as_ref()
                                            .map(|snapshot| snapshot.tools.clone())
                                            .unwrap_or_default(),
                                    ),
                                    tool_blacklist: Some(
                                        active_profile_tools
                                            .as_ref()
                                            .map(|snapshot| snapshot.blacklist.clone())
                                            .unwrap_or_default(),
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
                        profile
                            .portable_metadata
                            .remove(PROFILE_STORAGE_FILE_NAME_METADATA_KEY);
                        profile.categories = marker.categories.clone();
                        if marker.tools.is_some() {
                            profile.tools = marker.tools.clone();
                            profile.tool_blacklist =
                                Some(marker.tool_blacklist.clone().unwrap_or_default());
                        }
                    }
                }
                if kind == ProfileOperationKind::Recover {
                    Self::apply_recovered_profile_labels_to_catalog(catalog, renamed_profiles);
                }
                let pruned_profiles = if kind == ProfileOperationKind::Recover {
                    self.prune_missing_inactive_profile_records(&game_id)
                } else {
                    Vec::new()
                };
                if kind == ProfileOperationKind::Delete {
                    if let Some(profile_id) = profile_id {
                        self.profile_compression_states
                            .remove(&(game_id.clone(), profile_id));
                    }
                } else if matches!(
                    kind,
                    ProfileOperationKind::Create
                        | ProfileOperationKind::Duplicate
                        | ProfileOperationKind::Switch
                        | ProfileOperationKind::Recover
                ) && let Some(active_id) = target_profile_id
                {
                    self.profile_compression_states
                        .remove(&(game_id.clone(), active_id));
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
                    if let Some(tools) = active_profile_tools {
                        self.restore_profile_tools(&game_id, tools);
                    }
                }
                if kind == ProfileOperationKind::Recover {
                    self.sync_profile_storage_labels(&game_id);
                }
                self.save_state();
                for profile_id in pruned_profiles {
                    self.push_log(format!(
                        "Profile removed from catalog because its storage is missing: {game_id}/{profile_id}"
                    ));
                }
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
                // Say so rather than having profiles quietly appear: an adopted profile means the
                // catalog had lost track of data that was on disk the whole time.
                if !adopted.is_empty() {
                    let text = self.text();
                    for name in &adopted {
                        self.log_action(
                            text.profile_action_recovered(),
                            &text.profile_display_name(name),
                        );
                    }
                    self.set_message_ok(self.text().profiles_recovered(adopted.len()));
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
            ProfileEvent::Canceled { .. } => {
                self.set_message_ok(self.text().profile_canceled());
            }
            ProfileEvent::ArchiveQueued { .. }
            | ProfileEvent::ArchiveStarted { .. }
            | ProfileEvent::ArchiveSkipped { .. }
            | ProfileEvent::ArchiveCompleted { .. }
            | ProfileEvent::ArchiveFailed { .. }
            | ProfileEvent::ReconcileCompleted { .. }
            | ProfileEvent::ReconcileFailed { .. } => unreachable!(),
        }
    }
}

#[cfg(test)]
mod profile_storage_reconciliation_tests {
    use super::*;

    fn test_metadata(profile_id: ProfileId, display_name: &str) -> profiles::ProfileArchiveMetadata {
        profiles::ProfileArchiveMetadata {
            format_version: profiles::PROFILE_ARCHIVE_FORMAT_VERSION,
            profile_id,
            game_id: "test".to_string(),
            display_name: display_name.to_string(),
            backend: GameBackend::Xxmi,
            created_at: Utc::now(),
            uncompressed_size: 42,
            file_count: 2,
            portable_metadata: HashMap::new(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
            source_fingerprint: None,
        }
    }

    fn profile_record(
        id: ProfileId,
        display_name: &str,
        storage_file_name: Option<&str>,
    ) -> ProfileRecord {
        let mut portable_metadata = HashMap::new();
        if let Some(storage_file_name) = storage_file_name {
            portable_metadata.insert(
                PROFILE_STORAGE_FILE_NAME_METADATA_KEY.to_string(),
                serde_json::Value::String(storage_file_name.to_string()),
            );
        }
        ProfileRecord {
            id,
            display_name: display_name.to_string(),
            archive_size: None,
            uncompressed_size: None,
            file_count: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            portable_metadata,
            categories: Some(Vec::new()),
            tools: Some(Vec::new()),
            tool_blacklist: Some(Vec::new()),
        }
    }

    #[test]
    fn exact_bound_storage_does_not_fall_back_to_same_marker_files() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        std::fs::write(roots.profiles_dir.join("zzzDefault3 [7a15244b].tzst"), b"archive")
            .unwrap();

        let stale = HestiaApp::profile_bound_storage_path(
            &roots,
            "zDefault3 [7a15244b].tzst",
            true,
        );

        assert_eq!(stale, roots.profiles_dir.join("zDefault3 [7a15244b].tzst"));
        assert!(
            !stale.exists(),
            "a stale exact binding must stay missing instead of resolving by marker"
        );
        assert!(
            !HestiaApp::profile_bound_storage_exists(&roots, "zDefault3 [7a15244b].tzst"),
            "rename/delete preflight must also reject the stale exact binding"
        );
        assert!(
            HestiaApp::profile_bound_storage_exists(&roots, "zzzDefault3 [7a15244b].tzst"),
            "the renamed file itself is valid storage and will be picked up by recovery"
        );
    }

    #[test]
    fn exact_storage_rename_reports_missing_source_instead_of_succeeding_silently() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        std::fs::write(roots.profiles_dir.join("zzzDefault3 [7a15244b].tzst"), b"archive")
            .unwrap();

        let error = HestiaApp::try_rename_exact_profile_storage_file(
            &roots,
            "zDefault3 [7a15244b].tzst",
            "yDefault3",
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("profile storage is missing"),
            "stale exact-bound rows should produce an action error: {error:#}"
        );
        assert!(
            roots.profiles_dir.join("zzzDefault3 [7a15244b].tzst").is_file(),
            "the renamed-on-disk profile must be left for recovery"
        );
        assert!(
            !roots.profiles_dir.join("yDefault3 [7a15244b].tzst").exists(),
            "Hestia must not create a misleading app-side rename when the source is gone"
        );
    }

    #[test]
    fn reload_repairs_one_manual_rename_instead_of_adopting_a_duplicate_row() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let marker_id: ProfileId = "7a15244b".parse().unwrap();
        let mut row_id = ProfileId::random();
        while row_id == marker_id {
            row_id = ProfileId::random();
        }
        let renamed_file_name = "zzzDefault3 [7a15244b].tzst";
        std::fs::write(roots.profiles_dir.join(renamed_file_name), b"archive").unwrap();
        let mut catalog = ProfileCatalog {
            active_profile_id: Some(marker_id),
            profiles: vec![
                profile_record(marker_id, "Default", None),
                profile_record(row_id, "zDefault3", Some("zDefault3 [7a15244b].tzst")),
            ],
        };
        let orphan_id = ProfileId::random();
        let orphan = OrphanedProfile {
            profile_id: orphan_id,
            metadata: test_metadata(marker_id, "Default"),
            label: Some("zzzDefault3".to_string()),
            storage_file_name: Some(renamed_file_name.to_string()),
            archive_size: Some(100),
            updated_at: Some(Utc::now()),
        };

        let adopted =
            HestiaApp::adopt_orphaned_profiles_into_catalog(&mut catalog, Some(&roots), vec![orphan]);

        assert_eq!(adopted, vec!["zzzDefault3".to_string()]);
        assert_eq!(
            catalog.profiles.len(),
            2,
            "manual rename should update the stale row instead of adding another row"
        );
        assert!(!catalog.profiles.iter().any(|profile| profile.id == orphan_id));
        let repaired = catalog
            .profiles
            .iter()
            .find(|profile| profile.id == row_id)
            .unwrap();
        assert_eq!(repaired.display_name, "zzzDefault3");
        assert_eq!(
            HestiaApp::profile_storage_file_name(repaired),
            Some(renamed_file_name)
        );
    }

    #[test]
    fn missing_exact_bound_inactive_profile_is_pruned_from_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let active_id: ProfileId = "7a15244a".parse().unwrap();
        let stale_id: ProfileId = "7a15244b".parse().unwrap();
        let mut catalog = ProfileCatalog {
            active_profile_id: Some(active_id),
            profiles: vec![
                profile_record(active_id, "Default", None),
                profile_record(stale_id, "Deleted", Some("Deleted [7a15244b].tzst")),
            ],
        };

        let removed =
            HestiaApp::prune_missing_inactive_profile_records_from_catalog(&mut catalog, &roots);

        assert_eq!(removed, vec![stale_id]);
        assert_eq!(catalog.profiles.len(), 1);
        assert_eq!(catalog.profiles[0].id, active_id);
    }

    #[test]
    fn active_profile_is_not_pruned_when_no_inactive_storage_exists() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let active_id: ProfileId = "7a15244a".parse().unwrap();
        let mut catalog = ProfileCatalog {
            active_profile_id: Some(active_id),
            profiles: vec![profile_record(active_id, "Default", None)],
        };

        let removed =
            HestiaApp::prune_missing_inactive_profile_records_from_catalog(&mut catalog, &roots);

        assert!(removed.is_empty());
        assert_eq!(catalog.profiles.len(), 1);
        assert_eq!(catalog.profiles[0].id, active_id);
    }
}

#[cfg(test)]
mod profile_process_guard_tests {
    use super::*;

    #[test]
    fn selected_game_running_reason_gets_a_specific_paused_label() {
        let text = TextCatalog::new(AppLanguage::English);

        assert_eq!(
            HestiaApp::profile_actions_paused_reason_text(text, "the selected game is running"),
            Some("game is running")
        );
    }

    #[test]
    fn locked_mods_reason_gets_a_specific_paused_label() {
        let text = TextCatalog::new(AppLanguage::English);

        assert_eq!(
            HestiaApp::profile_actions_paused_reason_text(text, MODS_LOCKED_BLOCK_REASON),
            Some("mods are locked")
        );
    }

    #[test]
    fn game_process_match_checks_command_line_for_wine_and_proton() {
        let names = vec!["htgame.exe".to_string()];

        assert!(HestiaApp::process_matches_game_exe_names(
            std::ffi::OsStr::new("wine64-preloader"),
            &[std::ffi::OsString::from(
                "/mnt/games/NTE/HT/Binaries/Win64/HTGame.exe"
            )],
            &names,
        ));
        assert!(HestiaApp::process_matches_game_exe_names(
            std::ffi::OsStr::new("HTGame.exe"),
            &[],
            &names,
        ));
        assert!(!HestiaApp::process_matches_game_exe_names(
            std::ffi::OsStr::new("wine64-preloader"),
            &[std::ffi::OsString::from("wineserver")],
            &names,
        ));
    }

    /// `Process::exe` is opt-in on the refresh kind. If the `with_exe` flag is ever dropped from
    /// `running_process_block_reason`, every `exe()` silently becomes `None` and the guard that
    /// stops a profile swap while a tool holds the folder open would never fire again.
    #[test]
    fn process_refresh_kind_populates_exe_paths() {
        let system = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_processes(
                sysinfo::ProcessRefreshKind::nothing()
                    .with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
            ),
        );
        let own = system
            .process(sysinfo::Pid::from_u32(std::process::id()))
            .expect("the test process must be visible to sysinfo");

        assert!(
            own.exe().is_some_and(|exe| exe.components().count() > 1),
            "refreshing without `with_exe` leaves exe() empty and disables the tool guard"
        );
    }
}
