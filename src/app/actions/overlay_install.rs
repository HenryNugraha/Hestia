// GameBanana installs the in-game overlay asks for.  Hestia installs them
// like its Browse page's "Install disabled", and tells the overlay how they
// go.  The questions Hestia's windows would ask go to the overlay instead, and
// nothing here waits for Hestia's window, which is often minimized while a
// game runs.

/// How often Hestia looks at an overlay install's progress.
const OVERLAY_INSTALL_POLL: Duration = Duration::from_millis(200);

/// A GameBanana mod the in-game overlay asked Hestia to install.
struct OverlayInstall {
    /// The GameBanana mod.
    mod_id: u64,
    game_id: String,
    /// The library category it goes into, or None for a new one.
    category_id: Option<String>,
    task_id: u64,
    question: Option<OverlayInstallQuestion>,
    /// What the overlay heard last.
    sent: Option<overlay_protocol::InstallStage>,
}

enum OverlayInstallQuestion {
    File {
        files: Vec<gamebanana::ModFile>,
        unsafe_content: bool,
    },
    SameName(PendingConflict),
}

impl HestiaApp {
    fn handle_game_overlay_install(
        &mut self,
        game_id: &str,
        message: overlay_protocol::FromOverlay,
    ) {
        use overlay_protocol::{FromOverlay, SameNameChoice};

        let text = self.text();
        match message {
            FromOverlay::Install(request) => self.start_game_overlay_install(game_id, request),
            FromOverlay::PickFile { mod_id, file_id } => {
                let Some(install) = self.game_overlay_install_mut(mod_id) else {
                    return;
                };
                let Some(OverlayInstallQuestion::File {
                    files,
                    unsafe_content,
                }) = &install.question
                else {
                    return;
                };
                let Some(file) = files.iter().find(|file| file.id == file_id).cloned() else {
                    return;
                };
                let unsafe_content = *unsafe_content;
                install.question = None;
                let (game_id, task_id) = (install.game_id.clone(), install.task_id);
                self.queue_browse_download(
                    game_id,
                    mod_id,
                    file.clone(),
                    vec![file],
                    Some(task_id),
                    unsafe_content,
                    None,
                    None,
                    true,
                    None,
                );
            }
            FromOverlay::SameName { mod_id, choice } => {
                let Some(install) = self.game_overlay_install_mut(mod_id) else {
                    return;
                };
                if !matches!(install.question, Some(OverlayInstallQuestion::SameName(_))) {
                    return;
                }
                let Some(OverlayInstallQuestion::SameName(conflict)) = install.question.take()
                else {
                    return;
                };
                let (choice, action) = match choice {
                    SameNameChoice::Replace => (ConflictChoice::Replace, text.conflict_replace()),
                    SameNameChoice::Merge => (ConflictChoice::Merge, text.conflict_merge()),
                    SameNameChoice::KeepBoth => {
                        (ConflictChoice::KeepBoth, text.conflict_keep_both())
                    }
                };
                self.log_action(action, &conflict.preferred_name);
                self.commit_import(
                    conflict.job_id,
                    conflict.candidate_indices,
                    choice,
                    conflict.target_root,
                    conflict.gb_profile,
                    vec![conflict.preferred_name],
                );
            }
            FromOverlay::CancelInstall { mod_id } => {
                let Some(install) = self.game_overlay_install_mut(mod_id) else {
                    return;
                };
                let task_id = install.task_id;
                match install.question.take() {
                    Some(OverlayInstallQuestion::File { .. }) => {
                        self.update_task_status(task_id, TaskStatus::Canceled);
                    }
                    Some(OverlayInstallQuestion::SameName(conflict)) => {
                        self.log_action(text.conflict_cancel(), &conflict.preferred_name);
                        self.drop_install_job(task_id, TaskStatus::Canceled);
                    }
                    None => return,
                }
                let title = self
                    .state
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .map(|task| task.title.clone());
                if let Some(title) = title {
                    self.set_message_ok(text.install_canceled(&title));
                }
            }
            _ => {}
        }
    }

    /// Starts an install with the steps of the Browse page's Install button,
    /// turned off.
    fn start_game_overlay_install(&mut self, game_id: &str, request: overlay_protocol::Install) {
        if self.game_overlay_install_mut(request.mod_id).is_some() {
            return;
        }
        let task_id = self.next_background_job_id();
        self.add_task(
            task_id,
            TaskKind::Download,
            TaskStatus::Queued,
            request.name.clone(),
            Some(game_id.to_owned()),
            None,
            false,
        );
        self.game_overlay.installs.push(OverlayInstall {
            mod_id: request.mod_id,
            game_id: game_id.to_owned(),
            category_id: request.category_id,
            task_id,
            question: None,
            sent: None,
        });
        if !self.game_can_download_mods(game_id) {
            self.update_task_status(task_id, TaskStatus::Failed);
            self.report_warn(
                self.game_mod_setup_message(game_id),
                Some(self.text().install_unavailable()),
            );
            return;
        }
        self.request_browse_detail(request.mod_id);
        self.resolve_browse_install_after_detail(PendingBrowseInstall {
            task_id,
            mod_id: request.mod_id,
            game_id: game_id.to_owned(),
            update_target_id: None,
            install_disabled: true,
        });
        self.set_message_ok(self.text().resolving_download(&request.name));
    }

    fn game_overlay_install_mut(&mut self, mod_id: u64) -> Option<&mut OverlayInstall> {
        self.game_overlay
            .installs
            .iter_mut()
            .find(|install| install.mod_id == mod_id)
    }

    fn is_game_overlay_install(&self, task_id: u64) -> bool {
        self.game_overlay
            .installs
            .iter()
            .any(|install| install.task_id == task_id)
    }

    /// GameBanana has several files for an overlay install, so the overlay
    /// asks which one instead of Hestia's window.  False when the install
    /// isn't the overlay's.
    fn ask_game_overlay_for_file(
        &mut self,
        task_id: u64,
        files: &[gamebanana::ModFile],
        unsafe_content: bool,
    ) -> bool {
        let Some(install) = self
            .game_overlay
            .installs
            .iter_mut()
            .find(|install| install.task_id == task_id)
        else {
            return false;
        };
        install.question = Some(OverlayInstallQuestion::File {
            files: files.to_vec(),
            unsafe_content,
        });
        true
    }

    /// Queues an inspected install for the review window, or does the
    /// window's step for an overlay install.  One version installs under the
    /// mod's name, and the overlay asks about a folder that has it.  Several
    /// install side by side, like "Install separately" with all of them.
    fn review_pending_import(&mut self, pending: PendingImport) {
        let PendingImport {
            job_id,
            inspection,
            gb_profile,
        } = pending;
        if !self.is_game_overlay_install(job_id) {
            self.pending_imports.push_back(PendingImport {
                job_id,
                inspection,
                gb_profile,
            });
            return;
        }
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == inspection.game_id)
            .cloned()
        else {
            self.drop_install_job(job_id, TaskStatus::Failed);
            return;
        };
        let target_root = game
            .mods_path(self.state.static_prefs.use_default_mods_path)
            .unwrap_or_default();
        let mod_name = self
            .pending_browse_install_meta
            .get(&job_id)
            .and_then(|meta| self.browse_mod_title_for_install(meta.mod_id, None))
            .or_else(|| {
                self.state
                    .tasks
                    .iter()
                    .find(|task| task.id == job_id)
                    .map(|task| task.title.clone())
            });
        match inspection.candidates.as_slice() {
            [] => self.drop_install_job(job_id, TaskStatus::Failed),
            [candidate] => {
                let preferred =
                    self.preferred_browse_folder_name(mod_name.as_deref(), &candidate.label);
                let existing_target = target_root.join(&preferred);
                if existing_target.exists() {
                    if let Some(install) = self
                        .game_overlay
                        .installs
                        .iter_mut()
                        .find(|install| install.task_id == job_id)
                    {
                        install.question =
                            Some(OverlayInstallQuestion::SameName(PendingConflict {
                                job_id,
                                candidate_indices: vec![0],
                                preferred_name: preferred,
                                target_root,
                                existing_target,
                                gb_profile,
                            }));
                    }
                } else {
                    self.commit_import(
                        job_id,
                        vec![0],
                        ConflictChoice::KeepBoth,
                        target_root,
                        gb_profile,
                        vec![preferred],
                    );
                }
            }
            candidates => {
                let title = self.sanitized_preferred_browse_title_name(mod_name.as_deref());
                let names = candidates
                    .iter()
                    .map(|candidate| {
                        let label = sanitize_folder_name(&candidate.label);
                        match &title {
                            Some(title) => format!("{title} - {label}"),
                            None => label,
                        }
                    })
                    .collect();
                self.commit_import(
                    job_id,
                    (0..candidates.len()).collect(),
                    ConflictChoice::KeepBoth,
                    target_root,
                    gb_profile,
                    names,
                );
            }
        }
    }

    /// Stops an install job before it installs, like the review window's
    /// Cancel.
    fn drop_install_job(&mut self, job_id: u64, status: TaskStatus) {
        let _ = self
            .install_request_tx
            .send(InstallRequest::Drop { job_id });
        if let Some(current) = self.install_inflight.remove(&job_id) {
            Self::cleanup_runtime_temp_for_source(&current.source);
            self.mark_usage_counters_dirty();
        }
        self.pending_browse_install_meta.remove(&job_id);
        self.pending_browse_install_safety.remove(&job_id);
        if self.install_batch_active {
            self.install_batch_stats.skipped += 1;
        }
        self.update_task_status(job_id, status);
    }

    /// Which category an overlay install's mods go into: `Some(Some(id))`
    /// for the category the overlay showed it in, `Some(None)` for a new one
    /// even where Browse downloads get none, and None for other installs.
    fn game_overlay_install_category(&self, job_id: u64) -> Option<Option<String>> {
        let install = self
            .game_overlay
            .installs
            .iter()
            .find(|install| install.task_id == job_id)?;
        Some(install.category_id.clone().filter(|category_id| {
            self.state
                .categories
                .iter()
                .any(|category| category.id == *category_id && category.game_id == install.game_id)
        }))
    }

    /// Tells the overlay an install is done, after the library with its mods.
    fn finish_game_overlay_install(&mut self, job_id: u64, mod_ids: &[String]) {
        let Some(index) = self
            .game_overlay
            .installs
            .iter()
            .position(|install| install.task_id == job_id)
        else {
            return;
        };
        let install = self.game_overlay.installs.remove(index);
        let Some(process) = &self.game_overlay.process else {
            return;
        };
        if let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == process.game_id)
        {
            let library = game_overlay_library(&self.state, game, self.text().uncategorized());
            if self.game_overlay.sent.as_ref() != Some(&library) {
                process.send_library(library.clone());
                self.game_overlay.sent = Some(library);
            }
        }
        if let Some(process) = &self.game_overlay.process {
            process.outbox().send(overlay_protocol::ToOverlay::Install(
                overlay_protocol::InstallUpdate {
                    mod_id: install.mod_id,
                    stage: overlay_protocol::InstallStage::Installed {
                        mods: mod_ids.to_vec(),
                    },
                },
            ));
        }
    }

    /// Tells the overlay how its installs go, each change once.
    fn sync_game_overlay_installs(&mut self, ctx: &egui::Context) {
        let Some(process) = &self.game_overlay.process else {
            // They go on in Hestia.
            self.game_overlay.installs.clear();
            return;
        };
        let outbox = process.outbox();
        let mut stages: Vec<overlay_protocol::InstallStage> = self
            .game_overlay
            .installs
            .iter()
            .map(|install| self.game_overlay_install_stage(install))
            .collect();
        stages.reverse();
        let mut busy = false;
        self.game_overlay.installs.retain_mut(|install| {
            use overlay_protocol::InstallStage;

            let stage = stages.pop().expect("a stage for each install");
            busy |= matches!(
                stage,
                InstallStage::Waiting | InstallStage::Downloading { .. } | InstallStage::Installing
            );
            let over = matches!(stage, InstallStage::Failed | InstallStage::Canceled);
            if install.sent.as_ref() != Some(&stage) {
                outbox.send(overlay_protocol::ToOverlay::Install(
                    overlay_protocol::InstallUpdate {
                        mod_id: install.mod_id,
                        stage: stage.clone(),
                    },
                ));
                install.sent = Some(stage);
            }
            !over
        });
        if busy {
            ctx.request_repaint_after(OVERLAY_INSTALL_POLL);
        }
    }

    fn game_overlay_install_stage(
        &self,
        install: &OverlayInstall,
    ) -> overlay_protocol::InstallStage {
        use overlay_protocol::{InstallFile, InstallStage};

        match &install.question {
            Some(OverlayInstallQuestion::File { files, .. }) => {
                return InstallStage::ChooseFile {
                    files: files
                        .iter()
                        .map(|file| InstallFile {
                            id: file.id,
                            name: file.file_name.clone(),
                            size: file.file_size,
                            description: file
                                .description
                                .clone()
                                .filter(|description| !description.trim().is_empty()),
                        })
                        .collect(),
                };
            }
            Some(OverlayInstallQuestion::SameName(conflict)) => {
                return InstallStage::SameName {
                    folder: conflict.preferred_name.clone(),
                };
            }
            None => {}
        }
        let status = self
            .state
            .tasks
            .iter()
            .find(|task| task.id == install.task_id)
            .map(|task| task.status);
        match status {
            Some(TaskStatus::Queued | TaskStatus::Canceling) => InstallStage::Waiting,
            Some(TaskStatus::Downloading) => InstallStage::Downloading {
                percent: self
                    .browse_download_inflight
                    .get(&install.task_id)
                    .and_then(|download| {
                        let progress = download.progress.read().ok()?;
                        let total = progress.total.filter(|total| *total > 0)?;
                        Some((progress.downloaded.saturating_mul(100) / total).min(100) as u8)
                    }),
            },
            Some(TaskStatus::Failed) => InstallStage::Failed,
            Some(TaskStatus::Canceled) => InstallStage::Canceled,
            // Completed waits for the library to take the mods in.  Clearing
            // finished tasks can take that one away meanwhile.
            Some(TaskStatus::Installing | TaskStatus::Completed) | None => InstallStage::Installing,
        }
    }
}
