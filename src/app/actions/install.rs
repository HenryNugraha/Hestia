fn initial_candidate_install_status(
    requested: ModInstallState,
    preserved: Option<ModStatus>,
) -> ModStatus {
    match preserved {
        // Archived XXMI updates are staged in the archive root, where the scan supplies the
        // final Archived status.  The worker still needs an active physical layout so a stale
        // DISABLED_BY_HESTIA container is not carried forward.
        Some(ModStatus::Archived) => ModStatus::Active,
        Some(status) => status,
        None => requested.staged_status(),
    }
}

impl HestiaApp {
    fn consume_install_events(&mut self) {
        while let Ok(event) = self.install_event_rx.try_recv() {
            match event {
                InstallEvent::InspectReady {
                    job_id,
                    inspection,
                    gb_profile,
                } => {
                    if !self.install_inflight.contains_key(&job_id) {
                        continue;
                    }
                    self.review_pending_import(PendingImport {
                        job_id,
                        inspection,
                        gb_profile,
                    });
                }
                InstallEvent::InspectFailed { job_id, error } => {
                    let Some(current) = self.install_inflight.remove(&job_id) else {
                        continue;
                    };
                    self.pending_browse_install_meta.remove(&job_id);
                    Self::cleanup_runtime_temp_for_source(&current.source);
                    self.mark_usage_counters_dirty();
                    self.pending_browse_install_safety.remove(&job_id);
                    let name =
                        Self::import_source_path(&current.source).display().to_string();
                    if self.install_batch_active {
                        self.install_batch_stats.failed += 1;
                    }
                    self.report_error_message(
                        self.text().install_inspection_failed(&name, &error),
                        Some(self.text().install_failed()),
                    );
                    self.update_task_status(job_id, TaskStatus::Failed);
                }
                InstallEvent::InstallDone {
                    job_id,
                    installed_paths,
                    installed_candidate_labels,
                    gb_profile,
                    rel_paths,
                } => {
                    self.pending_known_installed_paths
                        .extend(installed_paths.iter().cloned());
                    let pending_meta = self.pending_browse_install_meta.remove(&job_id);
                    let install_game_id = self
                        .install_inflight
                        .get(&job_id)
                        .map(|job| job.game_id.clone());
                    let pending_unsafe = self
                        .pending_browse_install_safety
                        .remove(&job_id)
                        .unwrap_or(false);
                    let install_state = self
                        .install_inflight
                        .get(&job_id)
                        .map(|job| job.install_state)
                        .unwrap_or_default();
                    let preserve_existing_state = self
                        .install_inflight
                        .get(&job_id)
                        .is_some_and(|job| job.preserve_existing_state);
                    let preserved_states = self
                        .install_inflight
                        .get(&job_id)
                        .map(|job| job.preserved_states.clone())
                        .unwrap_or_default();
                    let target_category_id = self
                        .install_inflight
                        .get(&job_id)
                        .and_then(|job| job.category_id.clone());
                    self.apply_pending_update_source_metadata_before_refresh(
                        pending_meta.as_ref(),
                        gb_profile.as_deref(),
                    );
                    if let Some(task) = self.state.tasks.iter_mut().find(|t| t.id == job_id) {
                        task.total_size =
                            self.install_inflight
                                .get(&job_id)
                                .and_then(|job| match &job.source {
                                    ImportSource::Archive(path) => {
                                        importing::archive_source_total_size(path)
                                    }
                                    ImportSource::Folder(_) => None,
                                });
                    }
                    if let Some(current) = self.install_inflight.remove(&job_id) {
                        Self::cleanup_runtime_temp_for_source(&current.source);
                        self.mark_usage_counters_dirty();
                    }
                    self.pending_install_finalize.insert(
                        job_id,
                        PendingInstallFinalize {
                            installed_paths,
                            installed_candidate_labels,
                            gb_profile,
                            rel_paths,
                            pending_meta,
                            pending_unsafe,
                            install_state,
                            preserve_existing_state,
                            preserved_states,
                            target_category_id,
                        },
                    );
                    if self.install_batch_active {
                        self.install_batch_stats.installed += 1;
                    }
                    if let Some(game_id) = install_game_id {
                        self.queue_game_refresh(game_id);
                    }
                    self.update_task_status(job_id, TaskStatus::Completed);
                }
                InstallEvent::InstallFailed {
                    job_id,
                    preferred_name,
                    error,
                } => {
                    self.mod_image_sync_inflight.remove(&job_id);
                    self.pending_browse_install_meta.remove(&job_id);
                    if let Some(current) = self.install_inflight.remove(&job_id) {
                        Self::cleanup_runtime_temp_for_source(&current.source);
                        self.mark_usage_counters_dirty();
                    }
                    self.pending_browse_install_safety.remove(&job_id);
                    if self.install_batch_active {
                        self.install_batch_stats.failed += 1;
                    }
                    self.report_error_message(
                        self.text().install_failed_for(&preferred_name, &error),
                        Some(self.text().install_failed()),
                    );
                    self.update_task_status(job_id, TaskStatus::Failed);
                }
                InstallEvent::SyncImagesDone {
                    _job_id: job_id,
                    mod_entry_id,
                    profile,
                    rel_paths,
                } => {
                    self.mod_image_sync_inflight.remove(&job_id);
                    self.apply_mod_sync_result(&mod_entry_id, *profile, rel_paths);
                }
                InstallEvent::SyncImagesCover {
                    mod_entry_id,
                    cover_rel_path,
                } => {
                    self.apply_mod_sync_cover(&mod_entry_id, &cover_rel_path);
                }
                InstallEvent::InstallCanceled { job_id } => {
                    self.pending_browse_install_meta.remove(&job_id);
                    if self.install_batch_active {
                        self.install_batch_stats.skipped += 1;
                    }
                    self.pending_browse_install_safety.remove(&job_id);
                    self.update_task_status(job_id, TaskStatus::Canceled);
                    self.set_message_ok(self.text().install_canceled_label());
                    if let Some(current) = self.install_inflight.remove(&job_id) {
                        Self::cleanup_runtime_temp_for_source(&current.source);
                        self.mark_usage_counters_dirty();
                    }
                }
            }
        }
    }

    fn begin_import(&mut self, job: InstallJob) -> Result<()> {
        let gb_profile = if let Some(meta) = self.pending_browse_install_meta.get(&job.id) {
            self.browse_state.details.get(&meta.mod_id).map(|d| Box::new(d.profile.clone()))
        } else {
            None
        };
        self.install_request_tx
            .send(InstallRequest::Inspect {
                job_id: job.id,
                game_id: job.game_id.clone(),
                source: job.source.clone(),
                gb_profile,
            })
            .map_err(|_| anyhow!("failed to queue install"))?;
        Ok(())
    }

    fn commit_import(
        &mut self,
        job_id: u64,
        candidate_indices: Vec<usize>,
        choice: ConflictChoice,
        mut target_root: PathBuf,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
        preferred_names: Vec<String>,
    ) {
        let Some(job) = self.install_inflight.get(&job_id).cloned() else {
            return;
        };
        target_root = self.update_target_root_for_job(job_id, target_root);
        if self.install_commit_touches_locked_active_mod(
            job_id,
            choice,
            &target_root,
            &preferred_names,
        ) {
            self.cancel_install_job_as_locked(job_id);
            return;
        }
        let preserved_states = if job.preserve_existing_state
            && matches!(choice, ConflictChoice::Replace | ConflictChoice::Merge)
        {
            let disabled_root = self
                .state
                .games
                .iter()
                .find(|game| game.definition.id == job.game_id)
                .and_then(|game| {
                    game.disabled_mods_path(self.state.static_prefs.use_default_mods_path)
                });
            preferred_names
                .iter()
                .map(|preferred_name| {
                    let target_path = target_root.join(preferred_name);
                    let disabled_target_path =
                        disabled_root.as_ref().map(|root| root.join(preferred_name));
                    self.state.mods.iter().find_map(|mod_entry| {
                        if mod_entry.game_id != job.game_id {
                            return None;
                        }
                        let matches_target =
                            Self::install_path_matches_mod_root(&target_path, &mod_entry.root_path)
                                || disabled_target_path.as_ref().is_some_and(|path| {
                                    Self::install_path_matches_mod_root(path, &mod_entry.root_path)
                                });
                        if matches_target {
                            let status = mod_entry.status.clone();
                            let mut paths = vec![(target_path.clone(), status.clone())];
                            if let Some(disabled_target_path) = disabled_target_path.as_ref() {
                                paths.push((disabled_target_path.clone(), status));
                            }
                            Some(paths)
                        } else {
                            None
                        }
                    })
                })
                .flatten()
                .flatten()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let candidate_install_states = preferred_names
            .iter()
            .take(candidate_indices.len())
            .map(|preferred_name| {
                let target_path = target_root.join(preferred_name);
                let preserved = preserved_states
                    .iter()
                    .find(|(path, _)| Self::install_path_matches_mod_root(&target_path, path))
                    .map(|(_, status)| status.clone());
                initial_candidate_install_status(job.install_state, preserved)
            })
            .collect::<Vec<_>>();
        if let Some(current) = self.install_inflight.get_mut(&job_id) {
            current.preserved_states = preserved_states.clone();
        }
        let game_backend = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == job.game_id)
            .map(|game| game.definition.backend)
            .unwrap_or_default();
        if matches!(choice, ConflictChoice::Replace | ConflictChoice::Merge) {
            if let Err(err) = self.preserve_existing_install_metadata(
                &job.game_id,
                &target_root,
                &preferred_names,
                game_backend,
            ) {
                let _ = self.install_request_tx.send(InstallRequest::Drop { job_id });
                self.pending_imports.retain(|pending| pending.job_id != job_id);
                self.pending_conflicts
                    .retain(|conflict| conflict.job_id != job_id);
                if let Some(current) = self.install_inflight.remove(&job_id) {
                    Self::cleanup_runtime_temp_for_source(&current.source);
                    self.mark_usage_counters_dirty();
                }
                if self.install_batch_active {
                    self.install_batch_stats.failed += 1;
                }
                self.report_error_message(
                    format!("could not preserve existing mod metadata: {err:#}"),
                    Some(self.text().install_failed()),
                );
                self.update_task_status(job_id, TaskStatus::Failed);
                return;
            }
        }
        let disabled_target_root = if game_backend == GameBackend::UnrealEngine {
            self.state
                .games
                .iter()
                .find(|game| game.definition.id == job.game_id)
                .and_then(|game| {
                    game.disabled_mods_path(self.state.static_prefs.use_default_mods_path)
                })
        } else {
            None
        };
        if self
            .install_request_tx
            .send(InstallRequest::Install {
                job_id,
                candidate_indices,
                candidate_install_states,
                preferred_names: preferred_names.clone(),
                choice,
                target_root,
                game_backend,
                disabled_target_root,
                gb_profile,
            })
            .is_err()
        {
            let first_name = preferred_names.get(0).cloned().unwrap_or_else(|| "mod".to_string());
            if self.install_batch_active {
                self.install_batch_stats.failed += 1;
            }
            self.report_error_message(
                self.text().install_dispatch_failed(&first_name),
                Some(self.text().install_failed()),
            );
            self.update_task_status(job_id, TaskStatus::Failed);
            if let Some(current) = self.install_inflight.remove(&job_id) {
                Self::cleanup_runtime_temp_for_source(&current.source);
            }
        }
    }

    fn preserve_existing_install_metadata(
        &mut self,
        game_id: &str,
        target_root: &Path,
        preferred_names: &[String],
        backend: GameBackend,
    ) -> Result<()> {
        let disabled_root = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .and_then(|game| game.disabled_mods_path(self.state.static_prefs.use_default_mods_path));
        let mut existing_ids = HashSet::new();
        for mod_entry in &self.state.mods {
            if mod_entry.game_id != game_id {
                continue;
            }
            let matches_target = preferred_names.iter().any(|preferred_name| {
                let active_target = target_root.join(preferred_name);
                let disabled_target = disabled_root
                    .as_ref()
                    .map(|root| root.join(preferred_name));
                Self::install_path_matches_mod_root(&active_target, &mod_entry.root_path)
                    || disabled_target.as_ref().is_some_and(|path| {
                        Self::install_path_matches_mod_root(path, &mod_entry.root_path)
                    })
            });
            if matches_target {
                existing_ids.insert(mod_entry.id.clone());
            }
        }
        for mod_id in existing_ids {
            let Some(mod_entry) = self.state.mods.iter_mut().find(|entry| entry.id == mod_id) else {
                continue;
            };
            match backend {
                GameBackend::Xxmi => xxmi::save_mod_metadata(mod_entry),
                GameBackend::UnrealEngine => unrealengine::write_portable_metadata(mod_entry),
            }
            .map_err(|err| anyhow!("{} ({}): {err:#}", mod_entry.folder_name, mod_entry.root_path.display()))?;
        }
        Ok(())
    }

    fn install_commit_touches_locked_active_mod(
        &self,
        job_id: u64,
        choice: ConflictChoice,
        target_root: &Path,
        preferred_names: &[String],
    ) -> bool {
        if !matches!(choice, ConflictChoice::Replace | ConflictChoice::Merge) {
            return false;
        }
        let Some(job) = self.install_inflight.get(&job_id) else {
            return false;
        };
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == job.game_id)
        else {
            return false;
        };
        if !game.is_unreal_engine() || !self.game_process_running(game) {
            return false;
        }

        preferred_names.iter().any(|preferred_name| {
            let target_path = target_root.join(preferred_name);
            self.state.mods.iter().any(|mod_entry| {
                mod_entry.game_id == job.game_id
                    && mod_entry.status == ModStatus::Active
                    && Self::install_path_matches_mod_root(&target_path, &mod_entry.root_path)
            })
        })
    }

    /// Find an existing target in either Unreal storage root.  The dialog and
    /// overlay review paths use this before presenting Replace/Merge choices;
    /// a disabled Unreal mod lives beside the active root and still represents
    /// the same logical target.
    fn existing_install_target(
        &self,
        game: &GameInstall,
        preferred_name: &str,
        job_id: u64,
    ) -> Option<PathBuf> {
        // An update target is an identity, not a basename. Resolve the exact entry first so a
        // same-name peer in another storage root cannot steal the conflict prompt or commit path.
        if let Some(meta) = self.pending_browse_install_meta.get(&job_id)
            && meta.game_id == game.definition.id
            && let Some(target_id) = meta.update_target_mod_id.as_deref()
            && let Some(target) = self.state.mods.iter().find(|mod_entry| {
                mod_entry.id == target_id && mod_entry.game_id == game.definition.id
            })
        {
            if target.status == ModStatus::Archived || target.root_path.exists() {
                return Some(target.root_path.clone());
            }
        }

        let active_target = game
            .mods_path(self.state.static_prefs.use_default_mods_path)?
            .join(preferred_name);
        if active_target.exists() {
            return Some(active_target);
        }
        if let Some(disabled_target) = game
            .disabled_mods_path(self.state.static_prefs.use_default_mods_path)
            .map(|root| root.join(preferred_name))
            .filter(|path| path.exists())
        {
            return Some(disabled_target);
        }

        None
    }

    fn update_target_root_for_job(&self, job_id: u64, target_root: PathBuf) -> PathBuf {
        let Some(meta) = self.pending_browse_install_meta.get(&job_id) else {
            return target_root;
        };
        let Some(target_id) = meta.update_target_mod_id.as_deref() else {
            return target_root;
        };
        let Some(target) = self.state.mods.iter().find(|mod_entry| {
            mod_entry.id == target_id && mod_entry.game_id == meta.game_id
        }) else {
            return target_root;
        };
        if target.status != ModStatus::Archived {
            return target_root;
        }
        target
            .root_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(target_root)
    }

    fn cancel_install_job_as_locked(&mut self, job_id: u64) {
        let _ = self.install_request_tx.send(InstallRequest::Drop { job_id });
        self.pending_imports.retain(|pending| pending.job_id != job_id);
        self.pending_conflicts
            .retain(|conflict| conflict.job_id != job_id);
        if let Some(current) = self.install_inflight.remove(&job_id) {
            Self::cleanup_runtime_temp_for_source(&current.source);
            self.mark_usage_counters_dirty();
        }
        if self.install_batch_active {
            self.install_batch_stats.skipped += 1;
        }
        self.update_task_status(job_id, TaskStatus::Canceled);
        self.report_skipped_locked_mods();
    }

    fn enqueue_install_sources(&mut self, sources: Vec<ImportSource>) {
        let state = self.state.static_prefs.mod_install_state;
        self.enqueue_install_sources_with_state_and_preserve(sources, state, true);
    }

    /// Explicit install action used by the UI.  An explicit state overrides a
    /// replacement target's old state; normal installs use the preference and
    /// preserve an existing target during Replace/Merge.
    fn enqueue_install_sources_with_state(
        &mut self,
        sources: Vec<ImportSource>,
        state: ModInstallState,
    ) {
        self.enqueue_install_sources_with_state_and_preserve(sources, state, false);
    }

    fn enqueue_install_sources_with_state_and_preserve(
        &mut self,
        sources: Vec<ImportSource>,
        install_state: ModInstallState,
        preserve_existing_state: bool,
    ) {
        if sources.is_empty() {
            return;
        }
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            self.report_warn(self.text().select_game_first(), None);
            return;
        };
        if self.folder_batch_game_busy(&game_id) {
            self.report_warn(self.text().folder_batch_busy_tooltip(), None);
            return;
        }
        if !self.install_batch_active {
            self.install_batch_stats = InstallBatchStats::default();
            self.install_batch_active = true;
        }

        // External installs carry no GameBanana listing, so they normally land
        // uncategorized. When the user is drilled into a category folder, treat
        // that folder as the target so both the Install button and drag-and-drop
        // drop the mod where the user is looking. Only meaningful in the library
        // folder view; a drop from Browse/overview assigns nothing.
        let target_category_id = if self.current_view == ViewMode::Library
            && self.state.static_prefs.effective_library_group_mode() == LibraryGroupMode::Category
            && self.state.static_prefs.effective_library_category_display_mode()
                == LibraryCategoryDisplayMode::Folders
        {
            self.selected_category_folder_id.clone()
        } else {
            None
        };

        let mut added_any = false;
        for source in sources {
            // Split archive volumes all normalize to their first part so the
            // queue dedupe below collapses a multi-part set into one install.
            let source = match source {
                ImportSource::Archive(path) => {
                    let path = importing::resolve_split_archive(&path).unwrap_or(path);
                    ImportSource::Archive(path)
                }
                other => other,
            };
            if !self.selected_game_can_install_mods() {
                self.report_warn(
                    self.selected_game_mod_setup_message(),
                    Some(self.text().install_unavailable()),
                );
                return;
            }
            let path = Self::import_source_path(&source);
            let already_in_queue = self
                .install_queue
                .iter()
                .any(|item| Self::import_source_path(&item.source) == path);
            let is_inflight = self
                .install_inflight
                .values()
                .any(|item| Self::import_source_path(&item.source) == path);
            if already_in_queue || is_inflight {
                self.install_batch_stats.skipped += 1;
                continue;
            }
            let job = InstallJob {
                id: self.install_next_job_id,
                game_id: game_id.clone(),
                source,
                title: None,
                reuse_existing_task: false,
                install_state,
                preserve_existing_state,
                preserved_states: Vec::new(),
                category_id: target_category_id.clone(),
            };
            self.install_next_job_id = self.install_next_job_id.wrapping_add(1);
            self.install_queue.push_back(job.clone());
            self.add_install_task(&job);
            added_any = true;
        }
        if added_any {
            self.state.show_tasks = true;
            self.tasks_window_nonce = self.tasks_window_nonce.wrapping_add(1);
            self.tasks_force_default_pos = true;
            self.save_state();
        }
    }

    fn enqueue_install_source_for_existing_task(
        &mut self,
        task_id: u64,
        game_id: String,
        title: String,
        source: ImportSource,
        _gb_profile: Option<Box<gamebanana::ProfileResponse>>,
    ) {
        if !self.install_batch_active {
            self.install_batch_stats = InstallBatchStats::default();
            self.install_batch_active = true;
        }
        if !self.game_can_install_mods(&game_id) {
            self.report_warn(
                self.game_mod_setup_message(&game_id),
                Some(self.text().install_unavailable()),
            );
            self.update_task_status(task_id, TaskStatus::Failed);
            return;
        }
        let (install_state, preserve_existing_state) = self
            .pending_browse_install_meta
            .get(&task_id)
            .map(|meta| (meta.install_state, meta.preserve_existing_state))
            .unwrap_or((self.state.static_prefs.mod_install_state, true));
        let job = InstallJob {
            id: task_id,
            game_id,
            source,
            title: Some(title),
            reuse_existing_task: true,
            install_state,
            preserve_existing_state,
            preserved_states: Vec::new(),
            // Browse/update installs derive their category from the GameBanana
            // listing (see apply_browse_download_category), so no folder capture.
            category_id: None,
        };
        self.install_queue.push_back(job.clone());
        self.add_install_task(&job);
    }

    fn process_install_queue(&mut self) {
        if !self.install_batch_active {
            return;
        }
        let max_parallel = self.max_parallel_installs();
        while self.install_inflight.len() < max_parallel {
            let Some(job) = self.install_queue.pop_front() else { break; };
            if self.folder_batch_game_busy(&job.game_id) {
                // Keep completed downloads queued until the folder batch releases the game;
                // dropping them here would force an unnecessary re-download.
                self.install_queue.push_front(job);
                break;
            }
            let path_label = Self::import_source_path(&job.source).display().to_string();
            self.install_inflight.insert(job.id, job.clone());
            self.update_task_status(job.id, TaskStatus::Installing);
            match self.begin_import(job.clone()) {
                Ok(()) => {}
                Err(err) => {
                    self.install_batch_stats.failed += 1;
                    self.report_error_message(
                        self.text().install_start_failed(&path_label, &format!("{err:#}")),
                        Some(self.text().install_failed()),
                    );
                    self.update_task_status(job.id, TaskStatus::Failed);
                    if let Some(current) = self.install_inflight.remove(&job.id) {
                        Self::cleanup_runtime_temp_for_source(&current.source);
                    }
                }
            }
        }

        if self.install_queue.is_empty() && self.install_inflight.is_empty() {
            self.install_batch_active = false;
            self.install_batch_stats = InstallBatchStats::default();
        }
    }

    fn max_parallel_installs(&self) -> usize {
        FULL_IMAGE_LIMIT
    }
}

#[cfg(test)]
mod install_candidate_state_tests {
    use super::*;

    #[test]
    fn preserved_replace_state_overrides_each_configured_choice() {
        for requested in [
            ModInstallState::Enabled,
            ModInstallState::Disabled,
            ModInstallState::Auto,
        ] {
            assert_eq!(
                initial_candidate_install_status(requested, Some(ModStatus::Active)),
                ModStatus::Active
            );
            assert_eq!(
                initial_candidate_install_status(requested, Some(ModStatus::Disabled)),
                ModStatus::Disabled
            );
            assert_eq!(
                initial_candidate_install_status(requested, Some(ModStatus::Archived)),
                ModStatus::Active
            );
        }
    }

    #[test]
    fn new_candidates_use_the_requested_staging_state() {
        assert_eq!(
            initial_candidate_install_status(ModInstallState::Enabled, None),
            ModStatus::Active
        );
        assert_eq!(
            initial_candidate_install_status(ModInstallState::Disabled, None),
            ModStatus::Disabled
        );
        assert_eq!(
            initial_candidate_install_status(ModInstallState::Auto, None),
            ModStatus::Disabled
        );
    }
}
