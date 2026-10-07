// GameBanana installs the in-game overlay asks for.  Hestia installs them
// like its Browse page's "Install disabled", and tells the overlay how they
// go.  Questions are shown in the overlay while it runs; if it closes, the
// conflict goes to Hestia's normal conflict window and the file question is
// shown in a main-window fallback.  The GameBanana downloads Hestia's window
// starts for the overlay's game show there like its own installs.

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

/// Returns questions the main window still needs to render and moves
/// conflicts to its existing conflict queue.
fn handoff_overlay_question(
    question: OverlayInstallQuestion,
    conflicts: &mut Vec<PendingConflict>,
) -> Option<OverlayInstallQuestion> {
    match question {
        question @ OverlayInstallQuestion::File { .. } => Some(question),
        OverlayInstallQuestion::SameName(conflict) => {
            conflicts.push(conflict);
            None
        }
    }
}

fn first_overlay_file_question<'a>(
    installs: &'a [OverlayInstall],
    process_game_id: Option<&str>,
) -> Option<&'a OverlayInstall> {
    installs.iter().find(|install| {
        process_game_id != Some(install.game_id.as_str())
            && matches!(&install.question, Some(OverlayInstallQuestion::File { .. }))
    })
}

fn take_pending_overlay_conflict(
    conflicts: &mut VecDeque<PendingConflict>,
    job_id: u64,
) -> Option<PendingConflict> {
    let index = conflicts
        .iter()
        .position(|conflict| conflict.job_id == job_id)?;
    conflicts.remove(index)
}

fn overlay_install_task_is_live(
    status: Option<TaskStatus>,
    pending_finalize: bool,
    install_inflight: bool,
    pending_conflict: bool,
) -> bool {
    if pending_finalize || install_inflight || pending_conflict {
        return true;
    }
    matches!(
        status,
        Some(
            TaskStatus::Queued
                | TaskStatus::Canceling
                | TaskStatus::Downloading
                | TaskStatus::Installing
        )
    )
}

/// A GameBanana download Hestia's window started for the overlay's game,
/// which the overlay shows like one of its own installs.
struct WindowDownload {
    /// The GameBanana mod.
    mod_id: u64,
    game_id: String,
    name: String,
    /// Its tasks, one for each file picked.
    task_ids: Vec<u64>,
    /// The library mods its finished tasks installed.
    mods: Vec<String>,
    /// What the overlay heard last.
    sent: Option<overlay_protocol::InstallStage>,
}

/// Where a task of a window download is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowPart {
    /// Queued for its download.
    Waiting,
    Downloading(Option<u8>),
    /// The window asks which files to download, or, once downloaded, how to
    /// install it.
    Asking {
        downloaded: bool,
    },
    Installing,
    Done,
    Failed,
    Canceled,
}

impl WindowPart {
    fn running(self) -> bool {
        !matches!(self, Self::Done | Self::Failed | Self::Canceled)
    }

    /// Hestia works on it, so the overlay has more to hear soon.
    fn busy(self) -> bool {
        matches!(
            self,
            Self::Waiting | Self::Downloading(_) | Self::Installing
        )
    }
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
                    ModInstallState::Disabled,
                    false,
                    None,
                );
            }
            FromOverlay::SameName { mod_id, choice } => {
                let Some(task_id) = self
                    .game_overlay
                    .installs
                    .iter()
                    .find(|install| install.mod_id == mod_id)
                    .map(|install| install.task_id)
                else {
                    return;
                };
                let conflict = self
                    .game_overlay_install_mut(mod_id)
                    .and_then(|install| match install.question.take() {
                        Some(OverlayInstallQuestion::SameName(conflict)) => Some(conflict),
                        Some(question) => {
                            install.question = Some(question);
                            None
                        }
                        None => None,
                    })
                    .or_else(|| {
                        take_pending_overlay_conflict(&mut self.pending_conflicts, task_id)
                    });
                let Some(conflict) = conflict else {
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
                let Some(task_id) = self
                    .game_overlay
                    .installs
                    .iter()
                    .find(|install| install.mod_id == mod_id)
                    .map(|install| install.task_id)
                else {
                    return;
                };
                let question = self
                    .game_overlay_install_mut(mod_id)
                    .and_then(|install| install.question.take());
                match question {
                    Some(OverlayInstallQuestion::File { .. }) => {
                        self.update_task_status(task_id, TaskStatus::Canceled);
                    }
                    Some(OverlayInstallQuestion::SameName(conflict)) => {
                        self.log_action(text.conflict_cancel(), &conflict.preferred_name);
                        self.drop_install_job(task_id, TaskStatus::Canceled);
                    }
                    None => {
                        let Some(conflict) =
                            take_pending_overlay_conflict(&mut self.pending_conflicts, task_id)
                        else {
                            return;
                        };
                        self.log_action(text.conflict_cancel(), &conflict.preferred_name);
                        self.drop_install_job(task_id, TaskStatus::Canceled);
                    }
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
        // Hestia's window downloads it already, and the overlay hadn't heard
        // yet.  Its card shows that download from now on.
        self.track_window_downloads(game_id);
        if let Some(download) = self
            .game_overlay
            .window_downloads
            .iter_mut()
            .find(|download| download.mod_id == request.mod_id)
        {
            download.sent = Some(overlay_protocol::InstallStage::Waiting);
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
            install_state: ModInstallState::Disabled,
            preserve_existing_state: false,
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
                if let Some(existing_target) = self.existing_install_target(&game, &preferred, job_id) {
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
    /// for the category the overlay showed it in, `Some(None)` for their
    /// character's even where Browse downloads get none, and None for other
    /// installs.
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
        // A file of a download Hestia's window started.  The overlay hears
        // once all its files are done.
        if let Some(download) = self
            .game_overlay
            .window_downloads
            .iter_mut()
            .find(|download| download.task_ids.contains(&job_id))
        {
            download.mods.extend_from_slice(mod_ids);
            return;
        }
        let Some(index) = self
            .game_overlay
            .installs
            .iter()
            .position(|install| install.task_id == job_id)
        else {
            return;
        };
        let install = self.game_overlay.installs.remove(index);
        let same_game_process = self
            .game_overlay
            .process
            .as_ref()
            .is_some_and(|process| process.game_id == install.game_id);
        if same_game_process {
            self.send_game_overlay_library_now();
        }
        if same_game_process && let Some(process) = &self.game_overlay.process {
            process.outbox().send(overlay_protocol::ToOverlay::Install(
                overlay_protocol::InstallUpdate {
                    mod_id: install.mod_id,
                    stage: overlay_protocol::InstallStage::Installed {
                        mods: mod_ids.to_vec(),
                    },
                    name: None,
                },
            ));
        }
    }

    /// Moves a conflict question to Hestia's normal conflict window when the
    /// overlay can no longer receive an answer.  File questions stay on the
    /// overlay install record so the main window can render them without
    /// losing the install's category or task identity.
    fn handoff_game_overlay_questions(&mut self) {
        let mut conflicts = Vec::new();
        for install in &mut self.game_overlay.installs {
            install.sent = None;
            let Some(question) = install.question.take() else {
                continue;
            };
            install.question = handoff_overlay_question(question, &mut conflicts);
        }
        self.pending_conflicts.extend(conflicts);
    }

    /// Keeps an overlay install until its normal download/install lifecycle
    /// has consumed it.  This is needed after the overlay closes: the task
    /// may still be waiting for a file answer, or its final refresh may still
    /// need the overlay category captured on the install record.
    fn overlay_install_is_live(&self, task_id: u64) -> bool {
        overlay_install_task_is_live(
            self.state
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| task.status),
            self.pending_install_finalize.contains_key(&task_id),
            self.install_inflight.contains_key(&task_id),
            self.pending_conflicts
                .iter()
                .any(|conflict| conflict.job_id == task_id),
        )
    }

    /// Tells the overlay how its installs go, each change once.
    fn sync_game_overlay_installs(&mut self, ctx: &egui::Context) {
        let Some(process) = &self.game_overlay.process else {
            // They go on in Hestia. Keep their records for questions,
            // category assignment, and finalization after the overlay closes.
            self.handoff_game_overlay_questions();
            let live_task_ids: std::collections::HashSet<u64> = self
                .game_overlay
                .installs
                .iter()
                .filter(|install| self.overlay_install_is_live(install.task_id))
                .map(|install| install.task_id)
                .collect();
            self.game_overlay
                .installs
                .retain(|install| live_task_ids.contains(&install.task_id));
            self.game_overlay.window_downloads.clear();
            return;
        };
        let game_id = process.game_id.clone();
        let outbox = process.outbox();
        let mut stages: Vec<overlay_protocol::InstallStage> = self
            .game_overlay
            .installs
            .iter()
            .filter(|install| install.game_id == game_id)
            .map(|install| self.game_overlay_install_stage(install))
            .collect();
        stages.reverse();
        let mut busy = false;
        let live_task_ids: std::collections::HashSet<u64> = self
            .game_overlay
            .installs
            .iter()
            .filter(|install| self.overlay_install_is_live(install.task_id))
            .map(|install| install.task_id)
            .collect();
        self.game_overlay.installs.retain_mut(|install| {
            use overlay_protocol::InstallStage;

            if install.game_id != game_id {
                return live_task_ids.contains(&install.task_id);
            }

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
                        name: None,
                    },
                ));
                install.sent = Some(stage);
            }
            !over
        });
        busy |= self.sync_game_overlay_window_downloads(&game_id);
        if busy {
            ctx.request_repaint_after(OVERLAY_INSTALL_POLL);
        }
    }

    fn game_overlay_install_stage(
        &self,
        install: &OverlayInstall,
    ) -> overlay_protocol::InstallStage {
        use overlay_protocol::{InstallFile, InstallStage};

        // A conflict handed to Hestia's normal window remains attached to
        // this task while the game overlay may restart. Report the same
        // question to the overlay too; whichever surface answers first owns
        // the queued conflict and the other becomes a no-op.
        if let Some(conflict) = self
            .pending_conflicts
            .iter()
            .find(|conflict| conflict.job_id == install.task_id)
        {
            return InstallStage::SameName {
                folder: conflict.preferred_name.clone(),
            };
        }

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
                percent: self.browse_download_percent(install.task_id),
            },
            Some(TaskStatus::Failed) => InstallStage::Failed,
            Some(TaskStatus::Canceled) => InstallStage::Canceled,
            // Completed waits for the library to take the mods in.  Clearing
            // finished tasks can take that one away meanwhile.
            Some(TaskStatus::Installing | TaskStatus::Completed) | None => InstallStage::Installing,
        }
    }

    /// How much of a Browse download came, when GameBanana gave its size.
    fn browse_download_percent(&self, task_id: u64) -> Option<u8> {
        let download = self.browse_download_inflight.get(&task_id)?;
        let progress = download.progress.read().ok()?;
        let total = progress.total.filter(|total| *total > 0)?;
        Some((progress.downloaded.saturating_mul(100) / total).min(100) as u8)
    }

    /// Tells the overlay how the downloads Hestia's window started for its
    /// game go, each change once.  True while Hestia works on one.
    fn sync_game_overlay_window_downloads(&mut self, game_id: &str) -> bool {
        use overlay_protocol::InstallStage;

        self.game_overlay
            .window_downloads
            .retain(|download| download.game_id == game_id);
        self.track_window_downloads(game_id);
        let asks_files = self.window_file_prompt(game_id);
        let mut busy = false;
        let mut stages: Vec<InstallStage> = self
            .game_overlay
            .window_downloads
            .iter()
            .map(|download| {
                let mut parts: Vec<WindowPart> = download
                    .task_ids
                    .iter()
                    .filter_map(|&task_id| self.window_part(task_id))
                    .collect();
                if asks_files == Some(download.mod_id) {
                    parts.push(WindowPart::Asking { downloaded: false });
                }
                busy |= parts.iter().any(|part| part.busy());
                window_download_stage(&parts)
                    .unwrap_or_else(|| window_download_outcome(&download.mods, &parts))
            })
            .collect();
        if stages
            .iter()
            .any(|stage| matches!(stage, InstallStage::Installed { .. }))
        {
            // The library with the mods comes first.
            self.send_game_overlay_library_now();
        }
        let Some(process) = &self.game_overlay.process else {
            return false;
        };
        let outbox = process.outbox();
        stages.reverse();
        self.game_overlay.window_downloads.retain_mut(|download| {
            let stage = stages.pop().expect("a stage for each download");
            let over = matches!(
                stage,
                InstallStage::Installed { .. } | InstallStage::Failed | InstallStage::Canceled
            );
            if download.sent.as_ref() != Some(&stage) {
                outbox.send(overlay_protocol::ToOverlay::Install(
                    overlay_protocol::InstallUpdate {
                        mod_id: download.mod_id,
                        stage: stage.clone(),
                        name: Some(download.name.clone()),
                    },
                ));
                download.sent = Some(stage);
            }
            !over
        });
        busy
    }

    /// Follows the GameBanana downloads Hestia's window runs for `game_id`,
    /// one for each mod, with each file's task.  Updates aren't followed, nor
    /// mods the library has already: the overlay shows no card for those.
    fn track_window_downloads(&mut self, game_id: &str) {
        let mut found: Vec<(u64, Option<u64>)> = self
            .state
            .tasks
            .iter()
            .filter_map(|task| match &task.retry_payload {
                Some(TaskRetryPayload::BrowseDownload(payload))
                    if payload.game_id == game_id
                        && payload.update_target_mod_id.is_none()
                        && self
                            .window_task_part(task.id, Some(task.status))
                            .is_some_and(WindowPart::running) =>
                {
                    Some((payload.mod_id, Some(task.id)))
                }
                _ => None,
            })
            .collect();
        // Before GameBanana said which files the mod has.
        found.extend(
            self.browse_state
                .pending_installs
                .iter()
                .filter(|pending| {
                    pending.game_id == game_id
                        && pending.update_target_id.is_none()
                        && self
                            .window_part(pending.task_id)
                            .is_some_and(WindowPart::running)
                })
                .map(|pending| (pending.mod_id, Some(pending.task_id))),
        );
        found.extend(
            self.window_file_prompt(game_id)
                .map(|mod_id| (mod_id, None)),
        );
        for (mod_id, task_id) in found {
            if task_id.is_some_and(|task_id| self.is_game_overlay_install(task_id)) {
                continue;
            }
            if let Some(download) = self
                .game_overlay
                .window_downloads
                .iter_mut()
                .find(|download| download.mod_id == mod_id)
            {
                if let Some(task_id) = task_id
                    && !download.task_ids.contains(&task_id)
                {
                    download.task_ids.push(task_id);
                }
                continue;
            }
            let installed = self.state.mods.iter().any(|entry| {
                entry.game_id == game_id
                    && entry.status != ModStatus::Archived
                    && entry
                        .source
                        .as_ref()
                        .and_then(|source| source.gamebanana.as_ref())
                        .is_some_and(|link| link.mod_id == mod_id)
            });
            if installed
                || self
                    .game_overlay
                    .installs
                    .iter()
                    .any(|install| install.mod_id == mod_id)
            {
                continue;
            }
            let name = self
                .browse_mod_title_for_install(mod_id, None)
                .or_else(|| {
                    let task_id = task_id?;
                    let task = self.state.tasks.iter().find(|task| task.id == task_id)?;
                    Some(task.title.clone())
                })
                .unwrap_or_else(|| mod_id.to_string());
            self.game_overlay.window_downloads.push(WindowDownload {
                mod_id,
                game_id: game_id.to_owned(),
                name,
                task_ids: task_id.into_iter().collect(),
                mods: Vec::new(),
                sent: None,
            });
        }
    }

    /// The GameBanana mod Hestia's window asks which files to download of,
    /// for a new install in `game_id`.
    fn window_file_prompt(&self, game_id: &str) -> Option<u64> {
        self.browse_state
            .file_prompt
            .as_ref()
            .filter(|prompt| prompt.game_id == game_id && prompt.update_target_mod_id.is_none())
            .map(|prompt| prompt.mod_id)
    }

    /// Where the task of a window download is.  None once it's gone without
    /// installing anything.
    fn window_part(&self, task_id: u64) -> Option<WindowPart> {
        let status = self
            .state
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .map(|task| task.status);
        self.window_task_part(task_id, status)
    }

    /// Where the task of a window download is, by its status.
    fn window_task_part(&self, task_id: u64, status: Option<TaskStatus>) -> Option<WindowPart> {
        let asking = || {
            self.pending_imports
                .iter()
                .any(|pending| pending.job_id == task_id)
                || self
                    .pending_conflicts
                    .iter()
                    .any(|conflict| conflict.job_id == task_id)
        };
        Some(match status {
            Some(TaskStatus::Queued | TaskStatus::Canceling) => WindowPart::Waiting,
            Some(TaskStatus::Downloading) => {
                WindowPart::Downloading(self.browse_download_percent(task_id))
            }
            Some(TaskStatus::Installing) if asking() => WindowPart::Asking { downloaded: true },
            Some(TaskStatus::Installing) => WindowPart::Installing,
            // Completed waits for the library to take the mods in.  Clearing
            // finished tasks can take that one away meanwhile.
            Some(TaskStatus::Completed) | None
                if self.pending_install_finalize.contains_key(&task_id) =>
            {
                WindowPart::Installing
            }
            Some(TaskStatus::Completed) => WindowPart::Done,
            Some(TaskStatus::Failed) => WindowPart::Failed,
            Some(TaskStatus::Canceled) => WindowPart::Canceled,
            None => return None,
        })
    }
}

/// How a window download goes, by its tasks: None once none of them runs.
/// The questions Hestia's window asks show as waiting.
fn window_download_stage(parts: &[WindowPart]) -> Option<overlay_protocol::InstallStage> {
    use overlay_protocol::InstallStage;

    if !parts.iter().any(|part| part.running()) {
        return None;
    }
    if parts
        .iter()
        .any(|part| matches!(part, WindowPart::Downloading(_)))
    {
        // One percent for all its files.
        let mut sum = 0u32;
        let mut count = 0u32;
        let mut known = true;
        for part in parts {
            let percent = match *part {
                WindowPart::Waiting => 0,
                WindowPart::Downloading(percent) => {
                    known &= percent.is_some();
                    percent.unwrap_or(0)
                }
                WindowPart::Asking { downloaded: true }
                | WindowPart::Installing
                | WindowPart::Done => 100,
                // Not a download yet, or one that ended without installing.
                WindowPart::Asking { downloaded: false }
                | WindowPart::Failed
                | WindowPart::Canceled => continue,
            };
            sum += u32::from(percent);
            count += 1;
        }
        return Some(InstallStage::Downloading {
            percent: known.then(|| (sum / count) as u8),
        });
    }
    Some(
        if parts
            .iter()
            .any(|part| matches!(part, WindowPart::Waiting | WindowPart::Asking { .. }))
        {
            InstallStage::Waiting
        } else {
            InstallStage::Installing
        },
    )
}

/// How a window download ended, once none of its tasks runs.
fn window_download_outcome(
    mods: &[String],
    parts: &[WindowPart],
) -> overlay_protocol::InstallStage {
    use overlay_protocol::InstallStage;

    if !mods.is_empty() {
        InstallStage::Installed {
            mods: mods.to_vec(),
        }
    } else if parts.contains(&WindowPart::Failed) {
        InstallStage::Failed
    } else {
        InstallStage::Canceled
    }
}

#[cfg(test)]
mod window_download_tests {
    use super::*;
    use crate::overlay_protocol::InstallStage;

    #[test]
    fn overlay_questions_handoff_without_losing_file_or_conflict_state() {
        let file = gamebanana::ModFile {
            id: 7,
            file_name: "summer.zip".to_owned(),
            file_size: 12,
            download_url: Some("https://example.test/summer.zip".to_owned()),
            description: Some("the selected file".to_owned()),
            date_added: 0,
            download_count: 0,
            version: None,
            is_archived: false,
        };
        let kept = handoff_overlay_question(
            OverlayInstallQuestion::File {
                files: vec![file.clone()],
                unsafe_content: true,
            },
            &mut Vec::new(),
        );
        assert!(matches!(
            kept,
            Some(OverlayInstallQuestion::File {
                files,
                unsafe_content: true
            }) if files.len() == 1
                && files[0].id == file.id
                && files[0].file_name == file.file_name
        ));

        let conflict = PendingConflict {
            job_id: 42,
            candidate_indices: vec![0],
            preferred_name: "summer".to_owned(),
            target_root: PathBuf::from("mods"),
            existing_target: PathBuf::from("mods/summer"),
            gb_profile: None,
        };
        let mut conflicts = Vec::new();
        assert!(
            handoff_overlay_question(
                OverlayInstallQuestion::SameName(conflict.clone()),
                &mut conflicts,
            )
            .is_none()
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].job_id, conflict.job_id);
        assert_eq!(conflicts[0].preferred_name, conflict.preferred_name);
    }

    #[test]
    fn fallback_skips_file_question_for_the_running_game() {
        let installs = vec![
            OverlayInstall {
                mod_id: 1,
                game_id: "running".to_owned(),
                category_id: None,
                task_id: 11,
                question: Some(OverlayInstallQuestion::File {
                    files: Vec::new(),
                    unsafe_content: false,
                }),
                sent: None,
            },
            OverlayInstall {
                mod_id: 2,
                game_id: "closed".to_owned(),
                category_id: Some("category".to_owned()),
                task_id: 22,
                question: Some(OverlayInstallQuestion::File {
                    files: Vec::new(),
                    unsafe_content: true,
                }),
                sent: None,
            },
        ];
        let fallback = first_overlay_file_question(&installs, Some("running"))
            .expect("the other game's question stays available");
        assert_eq!(fallback.task_id, 22);
        assert_eq!(fallback.game_id, "closed");
        assert!(first_overlay_file_question(&installs, Some("none")).is_some());
    }

    #[test]
    fn a_handed_off_conflict_can_be_answered_only_once() {
        let conflict = PendingConflict {
            job_id: 91,
            candidate_indices: vec![0],
            preferred_name: "summer".to_owned(),
            target_root: PathBuf::from("mods"),
            existing_target: PathBuf::from("mods/summer"),
            gb_profile: None,
        };
        let mut conflicts = VecDeque::from([conflict]);
        assert!(take_pending_overlay_conflict(&mut conflicts, 91).is_some());
        assert!(take_pending_overlay_conflict(&mut conflicts, 91).is_none());
    }

    #[test]
    fn finalization_keeps_overlay_category_record_after_task_history_is_cleared() {
        assert!(overlay_install_task_is_live(None, true, false, false));
        assert!(overlay_install_task_is_live(None, false, true, false));
        assert!(!overlay_install_task_is_live(None, false, false, false));
        assert!(!overlay_install_task_is_live(
            Some(TaskStatus::Completed),
            false,
            false,
            false
        ));
    }

    #[test]
    fn a_window_download_shows_its_files_as_one() {
        use super::WindowPart::*;

        assert_eq!(window_download_stage(&[]), None);
        assert_eq!(
            window_download_stage(&[Waiting]),
            Some(InstallStage::Waiting)
        );
        assert_eq!(
            window_download_stage(&[Asking { downloaded: true }, Done]),
            Some(InstallStage::Waiting),
            "the window asks"
        );
        assert_eq!(
            window_download_stage(&[Asking { downloaded: false }, Downloading(Some(40))]),
            Some(InstallStage::Downloading { percent: Some(40) }),
            "the files the window asks about aren't downloads yet"
        );
        assert_eq!(
            window_download_stage(&[Downloading(Some(40)), Done, Failed]),
            Some(InstallStage::Downloading { percent: Some(70) }),
            "a failed file doesn't count"
        );
        assert_eq!(
            window_download_stage(&[Downloading(Some(40)), Downloading(None)]),
            Some(InstallStage::Downloading { percent: None })
        );
        assert_eq!(
            window_download_stage(&[Installing, Done]),
            Some(InstallStage::Installing)
        );
        assert_eq!(window_download_stage(&[Done, Failed]), None);
    }

    #[test]
    fn a_window_download_ends_installed_if_any_file_installed() {
        use super::WindowPart::*;

        let mods = vec!["summer".to_owned()];
        assert_eq!(
            window_download_outcome(&mods, &[Done, Failed]),
            InstallStage::Installed { mods: mods.clone() }
        );
        assert_eq!(
            window_download_outcome(&[], &[Canceled, Failed]),
            InstallStage::Failed
        );
        assert_eq!(
            window_download_outcome(&[], &[Canceled]),
            InstallStage::Canceled
        );
        assert_eq!(
            window_download_outcome(&[], &[]),
            InstallStage::Canceled,
            "the window's question was closed"
        );
    }
}
