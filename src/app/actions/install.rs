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

/// Rebase an identity-targeted install onto the physical parent of its existing mod.
///
/// XXMI keeps active and disabled content in the same mod directory, so a nested
/// target must keep its category/grouping parent for Replace, Merge, and retry
/// installs. Unreal stores disabled mods in a separate root; only its archived
/// targets use the existing parent-root behavior. Unreal live/disabled targets
/// rebase both roots together so the worker can still detect conflicts across
/// its paired active and disabled locations.
fn update_target_roots_for_existing_mod(
    target_root: PathBuf,
    disabled_target_root: Option<PathBuf>,
    mods: &[ModEntry],
    game_id: &str,
    target_id: Option<&str>,
    backend: GameBackend,
) -> (PathBuf, Option<PathBuf>) {
    let Some(target_id) = target_id else {
        return (target_root, disabled_target_root);
    };
    let Some(target) = mods
        .iter()
        .find(|mod_entry| mod_entry.id == target_id && mod_entry.game_id == game_id)
    else {
        return (target_root, disabled_target_root);
    };
    if backend == GameBackend::Xxmi || target.status == ModStatus::Archived {
        let target_root = target
            .root_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(target_root);
        return (target_root, disabled_target_root);
    }
    let Some(disabled_target_root) = disabled_target_root else {
        return (target_root, None);
    };
    let source_root = if target.status == ModStatus::Disabled {
        &disabled_target_root
    } else {
        &target_root
    };
    let Ok(relative_target) = target.root_path.strip_prefix(source_root) else {
        return (target_root, Some(disabled_target_root));
    };
    if relative_target
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return (target_root, Some(disabled_target_root));
    }
    let Some(relative_parent) = relative_target.parent() else {
        return (target_root, Some(disabled_target_root));
    };
    (
        target_root.join(relative_parent),
        Some(disabled_target_root.join(relative_parent)),
    )
}

fn find_existing_install_target<'a>(
    mods: &'a [ModEntry],
    game_id: &str,
    update_target_mod_id: Option<&str>,
    active_target: &Path,
    disabled_target: Option<&Path>,
) -> Option<&'a ModEntry> {
    mods.iter().find(|mod_entry| {
        mod_entry.game_id == game_id
            && update_target_mod_id.is_none_or(|target_id| mod_entry.id == target_id)
            && (HestiaApp::install_path_matches_mod_root(active_target, &mod_entry.root_path)
                || disabled_target.is_some_and(|path| {
                    HestiaApp::install_path_matches_mod_root(path, &mod_entry.root_path)
                }))
    })
}

/// Return whether an install would replace an active Unreal mod while the game is running.
///
/// An update target is an identity, so its current physical root and status decide whether the
/// game lock applies. In particular, a disabled target may share a relative folder name with an
/// active mod in the paired root without touching that active mod.
fn install_targets_active_mod(
    mods: &[ModEntry],
    game_id: &str,
    update_target_mod_id: Option<&str>,
    target_root: &Path,
    disabled_target_root: Option<&Path>,
    allow_disabled_identity_override: bool,
    preferred_names: &[String],
) -> bool {
    let active_mod_matches_path = |target_path: &Path| {
        mods.iter().any(|mod_entry| {
            mod_entry.game_id == game_id
                && mod_entry.status == ModStatus::Active
                && HestiaApp::install_path_matches_mod_root(target_path, &mod_entry.root_path)
        })
    };

    if let Some(target_id) = update_target_mod_id {
        if let Some(target) = mods
            .iter()
            .find(|mod_entry| mod_entry.game_id == game_id && mod_entry.id == target_id)
        {
            return preferred_names.iter().any(|preferred_name| {
                let active_target_path = target_root.join(preferred_name);
                let disabled_identity_matches_name = allow_disabled_identity_override
                    && target.status == ModStatus::Disabled
                    && disabled_target_root.is_some_and(|root| {
                        let disabled_target_path = root.join(preferred_name);
                        HestiaApp::install_path_matches_mod_root(
                            &disabled_target_path,
                            &target.root_path,
                        )
                    });
                !disabled_identity_matches_name
                    && active_mod_matches_path(&active_target_path)
            });
        }
    }

    preferred_names.iter().any(|preferred_name| {
        active_mod_matches_path(&target_root.join(preferred_name))
    })
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
        target_root: PathBuf,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
        preferred_names: Vec<String>,
    ) {
        let Some(job) = self.install_inflight.get(&job_id).cloned() else {
            return;
        };
        let game_backend = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == job.game_id)
            .map(|game| game.definition.backend)
            .unwrap_or_default();
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
        let (rebased_target_root, disabled_target_root) = self.update_target_roots_for_job(
            job_id,
            target_root,
            disabled_target_root,
        );
        let target_root = rebased_target_root;
        let update_target_mod_id = self
            .pending_browse_install_meta
            .get(&job_id)
            .and_then(|meta| meta.update_target_mod_id.clone());
        if self.install_commit_touches_locked_active_mod(
            job_id,
            choice,
            &target_root,
            &preferred_names,
            disabled_target_root.as_deref(),
            update_target_mod_id.as_deref(),
        ) {
            self.cancel_install_job_as_locked(job_id);
            return;
        }
        let preserved_states = if job.preserve_existing_state
            && matches!(choice, ConflictChoice::Replace | ConflictChoice::Merge)
        {
            preferred_names
                .iter()
                .map(|preferred_name| {
                    let target_path = target_root.join(preferred_name);
                    let disabled_target_path = disabled_target_root
                        .as_ref()
                        .map(|root| root.join(preferred_name));
                    find_existing_install_target(
                        &self.state.mods,
                        &job.game_id,
                        update_target_mod_id.as_deref(),
                        &target_path,
                        disabled_target_path.as_deref(),
                    )
                    .map(|mod_entry| {
                        let status = mod_entry.status.clone();
                        let mut paths = vec![(target_path.clone(), status.clone())];
                        if let Some(disabled_target_path) = disabled_target_path.as_ref() {
                            paths.push((disabled_target_path.clone(), status));
                        }
                        paths
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
        if matches!(choice, ConflictChoice::Replace | ConflictChoice::Merge) {
            if let Err(err) = self.preserve_existing_install_metadata(
                &job.game_id,
                &target_root,
                disabled_target_root.as_deref(),
                update_target_mod_id.as_deref(),
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
        disabled_target_root: Option<&Path>,
        update_target_mod_id: Option<&str>,
        preferred_names: &[String],
        backend: GameBackend,
    ) -> Result<()> {
        let mut existing_ids = HashSet::new();
        for preferred_name in preferred_names {
            let active_target = target_root.join(preferred_name);
            let disabled_target = disabled_target_root.map(|root| root.join(preferred_name));
            if let Some(mod_entry) = find_existing_install_target(
                &self.state.mods,
                game_id,
                update_target_mod_id,
                &active_target,
                disabled_target.as_deref(),
            ) {
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
        disabled_target_root: Option<&Path>,
        update_target_mod_id: Option<&str>,
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

        install_targets_active_mod(
            &self.state.mods,
            &job.game_id,
            update_target_mod_id,
            target_root,
            disabled_target_root,
            job.preserve_existing_state || job.install_state == ModInstallState::Disabled,
            preferred_names,
        )
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

    fn update_target_roots_for_job(
        &self,
        job_id: u64,
        target_root: PathBuf,
        disabled_target_root: Option<PathBuf>,
    ) -> (PathBuf, Option<PathBuf>) {
        let Some(meta) = self.pending_browse_install_meta.get(&job_id) else {
            return (target_root, disabled_target_root);
        };
        let Some(backend) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == meta.game_id)
            .map(|game| game.definition.backend)
        else {
            return (target_root, disabled_target_root);
        };
        update_target_roots_for_existing_mod(
            target_root,
            disabled_target_root,
            &self.state.mods,
            &meta.game_id,
            meta.update_target_mod_id.as_deref(),
            backend,
        )
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

    fn mod_entry(id: &str, game_id: &str, root_path: &str, status: ModStatus) -> ModEntry {
        ModEntry {
            id: id.to_string(),
            game_id: game_id.to_string(),
            folder_name: Path::new(root_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("mod")
                .to_string(),
            root_path: PathBuf::from(root_path),
            status,
            metadata: Default::default(),
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
            source: None,
            update_state: Default::default(),
        }
    }

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

    #[test]
    fn nested_xxmi_update_uses_the_exact_target_parent() {
        let mods = vec![
            mod_entry(
                "same-name-peer",
                "game",
                "Mods/Other/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "target",
                "game",
                "Mods/Ardelia/Outfit A",
                ModStatus::Active,
            ),
        ];

        assert_eq!(
            update_target_roots_for_existing_mod(
                PathBuf::from("Mods"),
                None,
                &mods,
                "game",
                Some("target"),
                GameBackend::Xxmi,
            )
            .0,
            PathBuf::from("Mods/Ardelia")
        );
    }

    #[test]
    fn nested_disabled_xxmi_update_uses_the_exact_target_parent() {
        let mods = vec![mod_entry(
            "target",
            "game",
            "Mods/Ardelia/Outfit A",
            ModStatus::Disabled,
        )];

        assert_eq!(
            update_target_roots_for_existing_mod(
                PathBuf::from("Mods"),
                None,
                &mods,
                "game",
                Some("target"),
                GameBackend::Xxmi,
            )
            .0,
            PathBuf::from("Mods/Ardelia")
        );
    }

    #[test]
    fn archived_target_keeps_parent_root_behavior() {
        let mods = vec![mod_entry(
            "archived",
            "game",
            "Mods_Archived/Ardelia/Outfit A",
            ModStatus::Archived,
        )];

        assert_eq!(
            update_target_roots_for_existing_mod(
                PathBuf::from("Mods"),
                None,
                &mods,
                "game",
                Some("archived"),
                GameBackend::UnrealEngine,
            )
            .0,
            PathBuf::from("Mods_Archived/Ardelia")
        );
    }

    #[test]
    fn ordinary_jobs_and_unreal_targets_keep_their_supplied_root() {
        let mods = vec![
            mod_entry(
                "unreal-active",
                "game",
                "Paks/Ardelia/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "unreal-disabled",
                "game",
                "Paks/~mods-disabledByHestia/Ardelia/Outfit B",
                ModStatus::Disabled,
            ),
        ];
        let supplied_root = PathBuf::from("Paks/~mods");

        assert_eq!(
            update_target_roots_for_existing_mod(
                supplied_root.clone(),
                None,
                &mods,
                "game",
                None,
                GameBackend::Xxmi,
            )
            .0,
            supplied_root
        );
        assert_eq!(
            update_target_roots_for_existing_mod(
                PathBuf::from("Paks/~mods"),
                None,
                &mods,
                "game",
                Some("unreal-active"),
                GameBackend::UnrealEngine,
            )
            .0,
            PathBuf::from("Paks/~mods")
        );
        assert_eq!(
            update_target_roots_for_existing_mod(
                PathBuf::from("Paks/~mods"),
                None,
                &mods,
                "game",
                Some("unreal-disabled"),
                GameBackend::UnrealEngine,
            )
            .0,
            PathBuf::from("Paks/~mods")
        );
    }

    #[test]
    fn nested_unreal_updates_rebase_paired_roots_by_exact_identity() {
        let mods = vec![
            mod_entry(
                "same-name-peer",
                "game",
                "Active/Other/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "same-id-other-game",
                "other-game",
                "Active/Ardelia/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "active-target",
                "game",
                "Active/Ardelia/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "disabled-target",
                "game",
                "Disabled/Ardelia/Outfit B",
                ModStatus::Disabled,
            ),
        ];
        let active_root = PathBuf::from("Active");
        let disabled_root = PathBuf::from("Disabled");

        assert_eq!(
            update_target_roots_for_existing_mod(
                active_root.clone(),
                Some(disabled_root.clone()),
                &mods,
                "game",
                Some("active-target"),
                GameBackend::UnrealEngine,
            ),
            (
                PathBuf::from("Active/Ardelia"),
                Some(PathBuf::from("Disabled/Ardelia")),
            )
        );
        assert_eq!(
            update_target_roots_for_existing_mod(
                active_root.clone(),
                Some(disabled_root.clone()),
                &mods,
                "game",
                Some("disabled-target"),
                GameBackend::UnrealEngine,
            ),
            (
                PathBuf::from("Active/Ardelia"),
                Some(PathBuf::from("Disabled/Ardelia")),
            )
        );
        assert_eq!(
            update_target_roots_for_existing_mod(
                active_root.clone(),
                Some(disabled_root.clone()),
                &mods,
                "game",
                None,
                GameBackend::UnrealEngine,
            ),
            (active_root, Some(disabled_root))
        );
    }

    #[test]
    fn exact_unreal_update_identity_wins_over_opposite_root_same_name() {
        let mods = vec![
            mod_entry(
                "active-peer",
                "game",
                "Active/Ardelia/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "disabled-target",
                "game",
                "Disabled/Ardelia/Outfit A",
                ModStatus::Disabled,
            ),
        ];
        let selected = find_existing_install_target(
            &mods,
            "game",
            Some("disabled-target"),
            Path::new("Active/Ardelia/Outfit A"),
            Some(Path::new("Disabled/Ardelia/Outfit A")),
        )
        .expect("the identity-targeted disabled mod should be selected");

        assert_eq!(selected.id, "disabled-target");
        assert_eq!(selected.status, ModStatus::Disabled);
    }

    #[test]
    fn disabled_identity_update_does_not_lock_against_active_same_name_peer() {
        let mods = vec![
            mod_entry(
                "active-peer",
                "game",
                "Active/Ardelia/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "disabled-target",
                "game",
                "Disabled/Ardelia/Outfit A",
                ModStatus::Disabled,
            ),
        ];

        assert!(!install_targets_active_mod(
            &mods,
            "game",
            Some("disabled-target"),
            Path::new("Active/Ardelia"),
            Some(Path::new("Disabled/Ardelia")),
            true,
            &["Outfit A".to_string()],
        ));
        assert!(install_targets_active_mod(
            &mods,
            "game",
            Some("disabled-target"),
            Path::new("Active/Ardelia"),
            Some(Path::new("Disabled/Ardelia")),
            false,
            &["Outfit A".to_string()],
        ));
    }

    #[test]
    fn disabled_identity_batch_still_locks_other_active_candidate() {
        let mods = vec![
            mod_entry(
                "active-peer",
                "game",
                "Active/Ardelia/Outfit B",
                ModStatus::Active,
            ),
            mod_entry(
                "disabled-target",
                "game",
                "Disabled/Ardelia/Outfit A",
                ModStatus::Disabled,
            ),
        ];

        assert!(install_targets_active_mod(
            &mods,
            "game",
            Some("disabled-target"),
            Path::new("Active/Ardelia"),
            Some(Path::new("Disabled/Ardelia")),
            true,
            &["Outfit A".to_string(), "Outfit B".to_string()],
        ));
    }

    #[test]
    fn active_identity_update_still_locks_its_exact_physical_target() {
        let mods = vec![
            mod_entry(
                "same-name-peer",
                "game",
                "Active/Ardelia/Outfit A",
                ModStatus::Active,
            ),
            mod_entry(
                "active-target",
                "game",
                "Active/Other/Outfit A",
                ModStatus::Active,
            ),
        ];

        assert!(install_targets_active_mod(
            &mods,
            "game",
            Some("active-target"),
            Path::new("Active/Other"),
            None,
            true,
            &["Outfit A".to_string()],
        ));
    }

    #[test]
    fn ordinary_unreal_conflict_still_locks_active_path() {
        let mods = vec![mod_entry(
            "active-mod",
            "game",
            "Active/Ardelia/Outfit A",
            ModStatus::Active,
        )];

        assert!(install_targets_active_mod(
            &mods,
            "game",
            None,
            Path::new("Active/Ardelia"),
            None,
            false,
            &["Outfit A".to_string()],
        ));
    }

    #[test]
    fn stale_identity_falls_back_to_active_path_protection() {
        let mods = vec![mod_entry(
            "active-peer",
            "game",
            "Active/Ardelia/Outfit A",
            ModStatus::Active,
        )];

        assert!(install_targets_active_mod(
            &mods,
            "game",
            Some("missing-target"),
            Path::new("Active/Ardelia"),
            None,
            false,
            &["Outfit A".to_string()],
        ));
    }

    #[test]
    fn renamed_identity_falls_back_to_active_path_protection() {
        let mods = vec![
            mod_entry(
                "renamed-target",
                "game",
                "Active/Ardelia/Renamed Outfit",
                ModStatus::Active,
            ),
            mod_entry(
                "active-peer",
                "game",
                "Active/Ardelia/Outfit A",
                ModStatus::Active,
            ),
        ];

        assert!(install_targets_active_mod(
            &mods,
            "game",
            Some("renamed-target"),
            Path::new("Active/Ardelia"),
            None,
            false,
            &["Outfit A".to_string()],
        ));
    }
}
