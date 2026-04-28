impl HestiaApp {
    fn consume_install_events(&mut self) {
        while let Ok(event) = self.install_event_rx.try_recv() {
            match event {
                InstallEvent::InspectReady { job_id, inspection, gb_profile } => {
                    if !self.install_inflight.contains_key(&job_id) {
                        continue;
                    }
                    self.pending_imports.push_back(PendingImport { job_id, inspection, gb_profile });
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
                        format!("install inspection failed for {name}: {error}"),
                        Some("Install failed"),
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
                    if let Some(task) = self.state.tasks.iter_mut().find(|t| t.id == job_id) {
                        task.total_size = self.install_inflight.get(&job_id).and_then(|job| match &job.source {
                            ImportSource::Archive(path) => fs::metadata(path).ok().map(|m| m.len()),
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
                        format!("install failed for {preferred_name}: {error}"),
                        Some("Install failed"),
                    );
                    self.update_task_status(job_id, TaskStatus::Failed);
                }
                InstallEvent::SyncImagesDone {
                    _job_id: _,
                    mod_entry_id,
                    profile,
                    rel_paths,
                } => {
                    self.apply_mod_sync_result(&mod_entry_id, *profile, rel_paths);
                }
                InstallEvent::InstallCanceled { job_id } => {
                    self.pending_browse_install_meta.remove(&job_id);
                    if self.install_batch_active {
                        self.install_batch_stats.skipped += 1;
                    }
                    self.pending_browse_install_safety.remove(&job_id);
                    self.update_task_status(job_id, TaskStatus::Canceled);
                    self.set_message_ok("Install canceled");
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
        if self
            .install_request_tx
            .send(InstallRequest::Install {
                job_id,
                candidate_indices,
                preferred_names: preferred_names.clone(),
                choice,
                target_root,
                gb_profile,
            })
            .is_err()
        {
            let first_name = preferred_names.get(0).cloned().unwrap_or_else(|| "mod".to_string());
            if self.install_batch_active {
                self.install_batch_stats.failed += 1;
            }
            self.report_error_message(
                format!("install dispatch failed for {first_name}"),
                Some("Install failed"),
            );
            self.update_task_status(job_id, TaskStatus::Failed);
            if let Some(current) = self.install_inflight.remove(&job_id) {
                Self::cleanup_runtime_temp_for_source(&current.source);
            }
        }
    }

    fn enqueue_install_sources(&mut self, sources: Vec<ImportSource>) {
        if sources.is_empty() {
            return;
        }
        if !self.install_batch_active {
            self.install_batch_stats = InstallBatchStats::default();
            self.install_batch_active = true;
        }

        let mut added_any = false;
        for source in sources {
            let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
                self.report_warn("Select a game first.", None);
                return;
            };
            if !self.selected_game_is_installed_or_configured() {
                self.report_warn(
                    "Game is not installed or configured.",
                    Some("Install unavailable"),
                );
                return;
            }
            let path = Self::import_source_path(&source);
            let already_in_queue = self
                .install_queue
                .iter()
                .any(|item| Self::import_source_path(&item.source) == path);
            let is_inflight = self.install_inflight.values().any(|item| {
                Self::import_source_path(&item.source) == path
            });
            if already_in_queue || is_inflight {
                self.install_batch_stats.skipped += 1;
                continue;
            }
            let job = InstallJob {
                id: self.install_next_job_id,
                game_id,
                source,
                title: None,
                reuse_existing_task: false,
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
        if !self.game_is_installed_or_configured(&game_id) {
            self.report_warn(
                "Game is not installed or configured.",
                Some("Install unavailable"),
            );
            self.update_task_status(task_id, TaskStatus::Failed);
            return;
        }
        let job = InstallJob {
            id: task_id,
            game_id,
            source,
            title: Some(title),
            reuse_existing_task: true,
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
            let path_label = Self::import_source_path(&job.source).display().to_string();
            self.install_inflight.insert(job.id, job.clone());
            self.update_task_status(job.id, TaskStatus::Installing);
            match self.begin_import(job.clone()) {
                Ok(()) => {}
                Err(err) => {
                    self.install_batch_stats.failed += 1;
                    self.report_error_message(
                        format!("failed to start install for {path_label}: {err:#}"),
                        Some("Install failed"),
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
