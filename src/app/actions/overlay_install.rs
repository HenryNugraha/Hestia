// GameBanana installs the in-game overlay asks for.  Hestia installs them
// like its Browse page's "Install disabled", and tells the overlay how they
// go.  The questions Hestia's windows would ask go to the overlay instead, and
// nothing here waits for Hestia's window, which is often minimized while a
// game runs.  The GameBanana downloads Hestia's window starts for the
// overlay's game show there like its own installs, and their questions stay
// in the window.

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
        self.send_game_overlay_library_now();
        if let Some(process) = &self.game_overlay.process {
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

    /// Tells the overlay how its installs go, each change once.
    fn sync_game_overlay_installs(&mut self, ctx: &egui::Context) {
        let Some(process) = &self.game_overlay.process else {
            // They go on in Hestia.
            self.game_overlay.installs.clear();
            self.game_overlay.window_downloads.clear();
            return;
        };
        let game_id = process.game_id.clone();
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
