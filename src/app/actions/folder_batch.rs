use std::sync::mpsc::{self, Receiver};

/// Shared lock reason for mutation entry points that predate folder batches.  The reason is
/// intentionally a stable internal value; callers use their normal localized locked-action
/// surface when they reject a request.
const FOLDER_BATCH_BUSY_BLOCK_REASON: &str = "folder batch is running";

/// A result for one snapshotted member.  The optional detail is deliberately kept out of the
/// progress API: the UI may be configured to hide names and counts for unsafe content.
#[derive(Debug, Clone)]
enum FolderBatchItemResult {
    Completed,
    SkippedLocked,
    Unsupported,
    ChangedMissing,
    Failed(String),
}

#[derive(Debug, Clone)]
struct FolderBatchItemReport {
    id: String,
    label: String,
    unsafe_content: bool,
    result: FolderBatchItemResult,
}

#[derive(Debug, Clone)]
struct FolderBatchReport {
    action: FolderBatchAction,
    scope: FolderContentsScope,
    completed: usize,
    skipped: usize,
    unsupported: usize,
    changed_missing: usize,
    failed: usize,
    cancelled: bool,
    items: Vec<FolderBatchItemReport>,
}

#[derive(Debug)]
enum FolderBatchWorkerEvent {
    Item {
        id: String,
        expected_root: PathBuf,
        label: String,
        unsafe_content: bool,
        before: ModStatus,
        entry: Option<ModEntry>,
        result: FolderBatchItemResult,
    },
    Finished {
        cancelled: bool,
        remove_category_ids: Vec<String>,
        commit_error: Option<String>,
        commit_warnings: Vec<String>,
    },
}

/// State owned by the UI while the filesystem worker runs.  The target snapshot remains attached
/// to the job so refreshing the library or changing the selection cannot retarget it.
struct FolderBatchJob {
    action: FolderBatchAction,
    targets: FolderBatchTargets,
    destination: Option<String>,
    cancel: Arc<AtomicBool>,
    rx: Receiver<FolderBatchWorkerEvent>,
    total: usize,
    completed: usize,
    skipped: usize,
    unsupported: usize,
    changed_missing: usize,
    failed: usize,
    items: Vec<FolderBatchItemReport>,
}

fn scoped_target_ids(targets: &FolderBatchTargets) -> Vec<String> {
    let source = match targets.scope {
        FolderContentsScope::Visible => &targets.mod_ids,
        FolderContentsScope::All => &targets.all_mod_ids,
    };
    let mut seen = HashSet::new();
    source
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect()
}

fn mod_is_in_target_scope(
    categories: &[ModCategory],
    targets: &FolderBatchTargets,
    entry: &ModEntry,
) -> bool {
    entry.game_id == targets.game_id
        && (targets.category_ids.is_empty()
            || effective_category_id(categories, entry)
                .is_some_and(|id| targets.category_ids.iter().any(|target| target == id)))
}

fn target_categories_exist(categories: &[ModCategory], targets: &FolderBatchTargets) -> bool {
    targets.category_ids.iter().all(|id| {
        categories
            .iter()
            .any(|category| category.game_id == targets.game_id && category.id == *id)
    })
}

fn action_status_eligible(
    action: FolderBatchAction,
    entry: &ModEntry,
    game: Option<&GameInstall>,
) -> bool {
    match action {
        FolderBatchAction::Enable => entry.status == ModStatus::Disabled,
        FolderBatchAction::Disable => entry.status == ModStatus::Active,
        FolderBatchAction::Archive => {
            matches!(entry.status, ModStatus::Active | ModStatus::Disabled)
                && game.is_some_and(GameInstall::is_xxmi)
        }
        FolderBatchAction::Restore => {
            entry.status == ModStatus::Archived && game.is_some_and(GameInstall::is_xxmi)
        }
        FolderBatchAction::CheckUpdates => entry
            .source
            .as_ref()
            .is_some_and(|source| source.gamebanana.is_some()),
        FolderBatchAction::Update => {
            entry.update_state == ModUpdateState::UpdateAvailable
                && entry
                    .source
                    .as_ref()
                    .is_some_and(|source| source.gamebanana.is_some())
        }
        FolderBatchAction::MoveContents
        | FolderBatchAction::RemoveFolders
        | FolderBatchAction::DeleteContents
        | FolderBatchAction::DeleteFoldersAndContents => true,
    }
}

impl HestiaApp {
    /// Return the member IDs that can currently participate in an operation.  IDs are
    /// de-duplicated because a legacy category/name match can otherwise appear twice in a UI
    /// snapshot.
    fn folder_batch_action_eligible_ids(
        &self,
        targets: &FolderBatchTargets,
        action: FolderBatchAction,
    ) -> Vec<String> {
        if !target_categories_exist(&self.state.categories, targets) {
            return Vec::new();
        }
        if action == FolderBatchAction::RemoveFolders {
            return targets
                .category_ids
                .iter()
                .filter(|category_id| {
                    self.state.categories.iter().any(|category| {
                        category.game_id == targets.game_id && category.id == **category_id
                    })
                })
                .cloned()
                .collect();
        }
        let candidate_ids = scoped_target_ids(targets);
        let games = &self.state.games;
        candidate_ids
            .into_iter()
            .filter(|id| {
                self.state
                    .mods
                    .iter()
                    .find(|entry| entry.id == *id)
                    .is_some_and(|entry| {
                        mod_is_in_target_scope(&self.state.categories, targets, entry)
                            && action_status_eligible(
                                action,
                                entry,
                                games
                                    .iter()
                                    .find(|game| game.definition.id == entry.game_id),
                            )
                    })
            })
            .collect()
    }

    fn folder_batch_action_allowed(
        &self,
        targets: &FolderBatchTargets,
        action: FolderBatchAction,
    ) -> bool {
        if targets.game_id.is_empty() || !target_categories_exist(&self.state.categories, targets) {
            return false;
        }
        if action == FolderBatchAction::RemoveFolders {
            return !targets.category_ids.is_empty();
        }
        if action == FolderBatchAction::DeleteFoldersAndContents
            && targets.scope == FolderContentsScope::Visible
        {
            let visible: HashSet<_> = targets.mod_ids.iter().collect();
            let all: HashSet<_> = targets.all_mod_ids.iter().collect();
            if visible != all {
                return false;
            }
        }
        if action == FolderBatchAction::DeleteFoldersAndContents
            && targets.category_ids.iter().all(|category_id| {
                self.state.mods.iter().all(|entry| {
                    entry.game_id != targets.game_id
                        || effective_category_id(&self.state.categories, entry)
                            != Some(category_id.as_str())
                })
            })
        {
            // Empty folders are still a valid explicit folder deletion target.
            return true;
        }
        !self
            .folder_batch_action_eligible_ids(targets, action)
            .is_empty()
    }

    fn start_folder_batch(
        &mut self,
        action: FolderBatchAction,
        destination: Option<String>,
    ) -> bool {
        let Some(targets) = self.folder_batch_targets() else {
            return false;
        };
        self.start_folder_batch_targets(targets, action, destination)
    }

    fn start_folder_batch_targets(
        &mut self,
        mut targets: FolderBatchTargets,
        action: FolderBatchAction,
        destination: Option<String>,
    ) -> bool {
        let game_id = targets.game_id.as_str();
        let update_check_busy = self
            .update_check_active_items
            .iter()
            .any(|(_, item_game_id, ..)| item_game_id == game_id)
            || self.pending_update_check_game.as_deref() == Some(game_id)
            || self.pending_update_check_mods.iter().any(|mod_id| {
                self.state
                    .mods
                    .iter()
                    .find(|entry| entry.id == *mod_id)
                    .is_some_and(|entry| entry.game_id == game_id)
            });
        let install_busy = self
            .install_inflight
            .values()
            .any(|job| job.game_id == game_id)
            || self
                .mod_image_sync_inflight
                .values()
                .any(|busy_game_id| busy_game_id == game_id)
            || self.folder_batch_has_local_image_work(game_id)
            || self.manual_image_imports_pending > 0;
        let global_busy = self.startup_scan_loading
            || self.refresh_inflight
            || self.profile_operation_locks_app()
            || self.profile_operation_inflight.is_some()
            || self
                .profile_recovery_queue
                .iter()
                .any(|game| game.definition.id == game_id)
            || self.profile_reconcile_inflight.contains(game_id)
            || !self.hotkey_clear_inflight.is_empty()
            || !self.hotkey_customization_rx.is_empty()
            || self
                .hotkey_requests_inflight
                .iter()
                .any(|request_game_id| request_game_id == game_id);
        if self.folder_batch_job.is_some() {
            return false;
        }
        if global_busy || update_check_busy || install_busy {
            self.set_message_ok(self.text().folder_batch_busy_tooltip());
            return false;
        }
        if !self.folder_batch_action_allowed(&targets, action) {
            return false;
        }
        if action == FolderBatchAction::MoveContents {
            if destination.as_ref().is_some_and(|destination| {
                !self.state.categories.iter().any(|category| {
                    category.game_id == targets.game_id && category.id == *destination
                })
            }) {
                return false;
            }
        }

        // Normalize the snapshot once at the boundary.  This also prevents duplicate filesystem
        // work when overlapping folder selections are supplied by a caller.
        let eligible = self.folder_batch_action_eligible_ids(&targets, action);
        if action == FolderBatchAction::RemoveFolders {
            // RemoveFolders is intentionally independent of the Visible/All contents toggle:
            // it always unassigns the complete captured membership. Keep the original
            // all_mod_ids snapshot so a newly installed member can prevent folder removal.
            targets.mod_ids = targets.all_mod_ids.clone();
            let mut seen = HashSet::new();
            targets.mod_ids.retain(|id| seen.insert(id.clone()));
        } else if targets.scope == FolderContentsScope::Visible {
            targets.mod_ids = eligible.clone();
        }
        let ids = if action == FolderBatchAction::RemoveFolders {
            targets.mod_ids.clone()
        } else {
            scoped_target_ids(&targets)
                .into_iter()
                .filter(|id| eligible.contains(id))
                .collect::<Vec<_>>()
        };
        if ids.is_empty()
            && !matches!(
                action,
                FolderBatchAction::RemoveFolders | FolderBatchAction::DeleteFoldersAndContents
            )
        {
            return false;
        }

        let games = self.state.games.clone();
        let game = games
            .iter()
            .find(|game| game.definition.id == targets.game_id)
            .cloned();
        let Some(game) = game else { return false };
        let use_default_path = self.state.static_prefs.use_default_mods_path;
        let delete_behavior = self.state.static_prefs.delete_behavior;
        // Prepare the one settings transaction on the UI thread only for operations that change
        // the live importer set. Metadata-only moves and queued update checks do not need to
        // touch d3dx_user.ini.
        let persist_tx = matches!(
            action,
            FolderBatchAction::Enable
                | FolderBatchAction::Disable
                | FolderBatchAction::Archive
                | FolderBatchAction::Restore
                | FolderBatchAction::DeleteContents
                | FolderBatchAction::DeleteFoldersAndContents
        )
        .then(|| self.begin_xxmi_persist_tx(&game))
        .flatten();
        let categories = self.state.categories.clone();
        let snapshots: Vec<(ModEntry, ModStatus)> = self
            .state
            .mods
            .iter()
            .filter(|entry| ids.iter().any(|id| id == &entry.id))
            .filter(|entry| mod_is_in_target_scope(&categories, &targets, entry))
            .map(|entry| (entry.clone(), entry.status.clone()))
            .collect();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let worker_cancel = Arc::clone(&cancel);
        let worker_action = action;
        let worker_targets = targets.clone();
        let worker_destination = destination.clone();
        let worker_destination_label = destination.as_deref().and_then(|destination| {
            self.state
                .categories
                .iter()
                .find(|category| category.game_id == targets.game_id && category.id == destination)
                .map(|category| category.name.clone())
        });
        std::thread::Builder::new()
            .name("hestia-folder-batch".to_string())
            .spawn(move || {
                run_folder_batch_worker(
                    worker_action,
                    worker_targets,
                    worker_destination,
                    worker_destination_label,
                    categories,
                    snapshots,
                    game,
                    use_default_path,
                    delete_behavior,
                    persist_tx,
                    worker_cancel,
                    tx,
                );
            })
            .is_ok()
            .then(|| {
                self.folder_batch_job = Some(FolderBatchJob {
                    action,
                    targets,
                    destination,
                    cancel,
                    rx,
                    total: ids.len(),
                    completed: 0,
                    skipped: 0,
                    unsupported: 0,
                    changed_missing: 0,
                    failed: 0,
                    items: Vec::new(),
                });
                true
            })
            .unwrap_or(false)
    }

    fn poll_folder_batch_job(&mut self) {
        let Some(job) = self.folder_batch_job.as_ref() else {
            return;
        };
        let mut events = Vec::new();
        let mut disconnected = false;
        loop {
            match job.rx.try_recv() {
                Ok(event) => events.push(event),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        let worker_disconnected = disconnected
            && !events
                .iter()
                .any(|event| matches!(event, FolderBatchWorkerEvent::Finished { .. }));
        if worker_disconnected {
            events.push(FolderBatchWorkerEvent::Finished {
                cancelled: false,
                remove_category_ids: Vec::new(),
                commit_error: Some("folder batch worker stopped unexpectedly".to_string()),
                commit_warnings: Vec::new(),
            });
        }
        if events.is_empty() {
            return;
        }

        let mut finished = None;
        for event in events {
            match event {
                FolderBatchWorkerEvent::Item {
                    id,
                    expected_root,
                    label,
                    unsafe_content,
                    before,
                    entry,
                    result,
                } => {
                    let (action, targets, destination) = {
                        let job = self.folder_batch_job.as_ref().expect("batch job exists");
                        (job.action, job.targets.clone(), job.destination.clone())
                    };
                    let result = self.merge_folder_batch_item(
                        &targets,
                        action,
                        destination.as_deref(),
                        &id,
                        &expected_root,
                        before,
                        entry,
                        result,
                    );
                    let job = self.folder_batch_job.as_mut().expect("batch job exists");
                    match &result {
                        FolderBatchItemResult::Completed => job.completed += 1,
                        FolderBatchItemResult::SkippedLocked => job.skipped += 1,
                        FolderBatchItemResult::Unsupported => job.unsupported += 1,
                        FolderBatchItemResult::ChangedMissing => job.changed_missing += 1,
                        FolderBatchItemResult::Failed(_) => job.failed += 1,
                    }
                    job.items.push(FolderBatchItemReport {
                        id,
                        label,
                        unsafe_content,
                        result,
                    });
                }
                FolderBatchWorkerEvent::Finished {
                    cancelled,
                    remove_category_ids,
                    commit_error,
                    commit_warnings,
                } => {
                    let hide_details =
                        self.state.static_prefs.unsafe_content_mode != UnsafeContentMode::Show;
                    if let Some(error) = commit_error {
                        let warning = if hide_details {
                            self.text().folder_batch_failed().to_owned()
                        } else {
                            format!("folder batch settings commit failed: {error}")
                        };
                        self.report_warn(warning, None);
                    }
                    for warning in commit_warnings {
                        let warning = if hide_details {
                            self.text().folder_batch_failed().to_owned()
                        } else {
                            warning
                        };
                        self.report_warn(warning, None);
                    }
                    if worker_disconnected {
                        if let Some(job) = self.folder_batch_job.as_mut() {
                            job.failed += 1;
                            job.items.push(FolderBatchItemReport {
                                id: String::new(),
                                label: String::new(),
                                unsafe_content: false,
                                result: FolderBatchItemResult::Failed(
                                    "folder batch worker stopped unexpectedly".to_string(),
                                ),
                            });
                        }
                    }
                    finished = Some((cancelled, remove_category_ids));
                }
            }
        }
        if let Some((cancelled, remove_category_ids)) = finished {
            let Some(job) = self.folder_batch_job.take() else {
                return;
            };
            let mut remove_category_ids = remove_category_ids;
            if matches!(
                job.action,
                FolderBatchAction::RemoveFolders | FolderBatchAction::DeleteFoldersAndContents
            ) && job
                .items
                .iter()
                .any(|item| !matches!(item.result, FolderBatchItemResult::Completed))
            {
                // The executor can only prove the on-disk step. A merge-time revalidation may
                // still classify an item as changed, so keep every selected folder for retry
                // whenever any captured member did not complete.
                remove_category_ids.clear();
            }
            let completed_items = job.completed;
            let reload_trigger = match job.action {
                FolderBatchAction::Enable => Some(ReloadHotkeyTrigger::EnablingMods),
                FolderBatchAction::Disable => Some(ReloadHotkeyTrigger::DisablingMods),
                FolderBatchAction::Archive => Some(ReloadHotkeyTrigger::ArchivingMods),
                FolderBatchAction::Restore => Some(ReloadHotkeyTrigger::RestoringMods),
                FolderBatchAction::DeleteContents | FolderBatchAction::DeleteFoldersAndContents => {
                    Some(ReloadHotkeyTrigger::DeletingMods)
                }
                _ => None,
            };
            if completed_items > 0
                && let Some(trigger) = reload_trigger
                && let Some(game) = self
                    .state
                    .games
                    .iter()
                    .find(|game| game.definition.id == job.targets.game_id)
                    .cloned()
                && game.is_xxmi()
            {
                // The worker committed its one settings transaction already. This call only
                // performs the normal live-state refresh/reload handshake used by single actions.
                self.finish_xxmi_persist_tx(&game, None, Some(trigger));
            }
            let is_read_only_queue = matches!(
                job.action,
                FolderBatchAction::CheckUpdates | FolderBatchAction::Update
            );
            if !is_read_only_queue {
                self.mark_usage_counters_dirty();
                let old_ts: HashMap<String, DateTime<Utc>> = self
                    .state
                    .mods
                    .iter()
                    .map(|entry| (entry.id.clone(), entry.updated_at))
                    .collect();
                // Refresh the affected game before removing an empty folder. This admits a
                // member installed externally while the worker was running, even when it was
                // not in the in-memory snapshot captured at start.
                let rescanned =
                    match xxmi::refresh_state(&mut self.state, Some(&job.targets.game_id)) {
                        Ok(()) => {
                            self.restore_imported_mod_categories(Some(&job.targets.game_id));
                            true
                        }
                        Err(_) => {
                            self.report_warn(self.text().could_not_refresh_mods().to_owned(), None);
                            false
                        }
                    };
                if rescanned {
                    for category_id in &remove_category_ids {
                        let has_members = self.state.mods.iter().any(|entry| {
                            entry.game_id == job.targets.game_id
                                && effective_category_id(&self.state.categories, entry)
                                    == Some(category_id.as_str())
                        });
                        if !has_members {
                            self.state.categories.retain(|category| {
                                !(category.game_id == job.targets.game_id
                                    && category.id == *category_id)
                            });
                        }
                    }
                    self.invalidate_stale_mod_textures(&old_ts);
                    self.backfill_missing_mod_images(Some(&job.targets.game_id));
                    if self
                        .selected_game()
                        .is_some_and(|game| game.definition.id == job.targets.game_id)
                    {
                        self.sync_tools_for_selected_game();
                    }
                    self.sync_selection_after_refresh();
                }
            }
            self.save_state();
            // Resume any refresh/recovery work that was deferred while the batch lock was held.
            self.resume_folder_batch_deferred_work();
            let mut report = FolderBatchReport {
                action: job.action,
                scope: if job.action == FolderBatchAction::RemoveFolders {
                    FolderContentsScope::All
                } else {
                    job.targets.scope
                },
                completed: job.completed,
                skipped: job.skipped,
                unsupported: job.unsupported,
                changed_missing: job.changed_missing,
                failed: job.failed,
                cancelled,
                items: job.items,
            };
            // Update installs are deferred until the batch lock is released. Their existing
            // queue/finalization path can then own the game's filesystem without racing this
            // batch's worker or settings transaction.
            if report.action == FolderBatchAction::Update {
                let update_ids: Vec<String> = report
                    .items
                    .iter()
                    .filter(|item| matches!(item.result, FolderBatchItemResult::Completed))
                    .map(|item| item.id.clone())
                    .collect();
                for id in update_ids {
                    if !self.queue_update_apply(&id) {
                        report.completed = report.completed.saturating_sub(1);
                        report.failed += 1;
                        if let Some(item) = report.items.iter_mut().find(|item| item.id == id) {
                            item.result = FolderBatchItemResult::Failed(
                                "update could not be queued".to_string(),
                            );
                        }
                    }
                }
            }
            self.folder_batch_report = Some(report);
        }
    }

    fn resume_folder_batch_deferred_work(&mut self) {
        self.dispatch_next_profile_recovery();
        if let Some(game_id) = self.refresh_pending_selected_game.take() {
            self.queue_game_refresh(game_id);
        }
        if self.update_check_inflight {
            return;
        }
        if let Some(game_id) = self.pending_update_check_game.take() {
            self.queue_update_check_for_linked_mods(Some(&game_id));
        } else if !self.pending_update_check_mods.is_empty() {
            let pending_ids: Vec<_> = self.pending_update_check_mods.drain().collect();
            let items: Vec<_> = pending_ids
                .into_iter()
                .filter_map(|mod_id| self.update_check_item_for_mod(&mod_id))
                .collect();
            self.dispatch_update_check_items(items);
        }
    }

    fn merge_folder_batch_item(
        &mut self,
        targets: &FolderBatchTargets,
        action: FolderBatchAction,
        destination: Option<&str>,
        id: &str,
        expected_root: &Path,
        before: ModStatus,
        worker_entry: Option<ModEntry>,
        worker_result: FolderBatchItemResult,
    ) -> FolderBatchItemResult {
        if !matches!(worker_result, FolderBatchItemResult::Completed) {
            return worker_result;
        }
        let Some(index) = self.state.mods.iter().position(|entry| entry.id == id) else {
            return FolderBatchItemResult::ChangedMissing;
        };
        let current = &self.state.mods[index];
        if !mod_is_in_target_scope(&self.state.categories, targets, current)
            || current.status != before
            || current.root_path != expected_root
        {
            return FolderBatchItemResult::ChangedMissing;
        }
        if matches!(
            action,
            FolderBatchAction::CheckUpdates | FolderBatchAction::Update
        ) && current
            .source
            .as_ref()
            .and_then(|source| source.gamebanana.as_ref())
            .is_none()
        {
            return FolderBatchItemResult::ChangedMissing;
        }
        if action == FolderBatchAction::Update
            && current.update_state != ModUpdateState::UpdateAvailable
        {
            return FolderBatchItemResult::ChangedMissing;
        }
        if action == FolderBatchAction::MoveContents
            && destination.is_some_and(|destination| {
                !self.state.categories.iter().any(|category| {
                    category.game_id == targets.game_id && category.id == destination
                })
            })
        {
            return FolderBatchItemResult::ChangedMissing;
        }
        if action == FolderBatchAction::MoveContents
            && let (Some(destination), Some(entry)) = (destination, worker_entry.as_ref())
            && self
                .state
                .categories
                .iter()
                .find(|category| category.game_id == targets.game_id && category.id == destination)
                .is_some_and(|category| entry.metadata.user.category.as_str() != category.name)
        {
            return FolderBatchItemResult::ChangedMissing;
        }
        if worker_entry.is_none() {
            let removed = self.state.mods.remove(index);
            self.clear_mod_image_runtime_state(&removed);
            return FolderBatchItemResult::Completed;
        }
        let next = worker_entry.expect("checked above");
        let current = &mut self.state.mods[index];
        match action {
            FolderBatchAction::Enable
            | FolderBatchAction::Disable
            | FolderBatchAction::Archive
            | FolderBatchAction::Restore => {
                // The filesystem executor owns these fields. Preserve source, unsafe-content,
                // image, and update metadata that may have arrived from an unrelated event while
                // this batch was running.
                current.root_path = next.root_path;
                current.folder_name = next.folder_name;
                current.status = next.status;
                current.archive_original_path = next.archive_original_path;
                current.updated_at = next.updated_at;
            }
            FolderBatchAction::MoveContents | FolderBatchAction::RemoveFolders => {
                current.metadata.user.category_id = next.metadata.user.category_id;
                current.metadata.user.category = next.metadata.user.category;
                current.updated_at = next.updated_at;
            }
            FolderBatchAction::CheckUpdates | FolderBatchAction::Update => {}
            FolderBatchAction::DeleteContents | FolderBatchAction::DeleteFoldersAndContents => {}
        }
        if action == FolderBatchAction::CheckUpdates {
            self.queue_update_check_for_mod(id);
        }
        FolderBatchItemResult::Completed
    }

    fn cancel_folder_batch(&mut self) {
        if let Some(job) = self.folder_batch_job.as_ref() {
            job.cancel.store(true, Ordering::Release);
        }
    }

    fn folder_batch_game_busy(&self, game_id: &str) -> bool {
        self.folder_batch_job
            .as_ref()
            .is_some_and(|job| job.targets.game_id == game_id)
    }

    fn folder_batch_blocks_mod(&self, mod_id: &str) -> bool {
        let Some(job) = self.folder_batch_job.as_ref() else {
            return false;
        };
        self.state
            .mods
            .iter()
            .find(|entry| entry.id == mod_id)
            .is_some_and(|entry| entry.game_id == job.targets.game_id)
    }

    fn folder_batch_progress_state(&self) -> Option<(usize, usize, bool)> {
        let job = self.folder_batch_job.as_ref()?;
        let show_counts = if job.action == FolderBatchAction::RemoveFolders {
            self.state.static_prefs.unsafe_content_mode != UnsafeContentMode::HideNoCounter
        } else {
            self.folder_batch_show_counts(&job.targets)
        };
        Some((
            job.completed + job.skipped + job.unsupported + job.changed_missing + job.failed,
            job.total,
            show_counts,
        ))
    }

    fn folder_batch_report(&self) -> Option<&FolderBatchReport> {
        self.folder_batch_report.as_ref()
    }

    /// A redaction-aware summary for the completion surface.  Zeroes are intentional when the
    /// current preference hides counts; callers can show the generic working/result string then.
    fn folder_batch_report_summary(&self) -> Option<(usize, usize, usize, usize, usize, bool)> {
        let report = self.folder_batch_report.as_ref()?;
        let show_counts = self.state.static_prefs.unsafe_content_mode
            != UnsafeContentMode::HideNoCounter
            || report.scope != FolderContentsScope::All;
        if show_counts {
            Some((
                report.completed,
                report.skipped,
                report.unsupported,
                report.changed_missing,
                report.failed,
                report.cancelled,
            ))
        } else {
            Some((0, 0, 0, 0, 0, report.cancelled))
        }
    }

    /// Report rows with labels and failure details redacted according to the current preference.
    /// A caller gets `None` for the whole row surface when counts are hidden for an All-scope
    /// operation, so it cannot accidentally reveal the number of hidden members.
    fn folder_batch_report_items(&self) -> Option<Vec<FolderBatchItemReport>> {
        let report = self.folder_batch_report.as_ref()?;
        let hide_counts = self.state.static_prefs.unsafe_content_mode
            == UnsafeContentMode::HideNoCounter
            && report.scope == FolderContentsScope::All;
        if hide_counts {
            return None;
        }
        let hide_details = self.state.static_prefs.unsafe_content_mode != UnsafeContentMode::Show;
        Some(
            report
                .items
                .iter()
                .cloned()
                .map(|mut item| {
                    if hide_details && item.unsafe_content {
                        item.label = self.text().folder_batch_hidden_mod().to_owned();
                    }
                    if hide_details && let FolderBatchItemResult::Failed(detail) = &mut item.result
                    {
                        *detail = self.text().folder_batch_failed().to_owned();
                    }
                    item
                })
                .collect(),
        )
    }

    fn dismiss_folder_batch_report(&mut self) {
        self.folder_batch_report = None;
    }
}

fn run_folder_batch_worker(
    action: FolderBatchAction,
    targets: FolderBatchTargets,
    destination: Option<String>,
    destination_label: Option<String>,
    categories: Vec<ModCategory>,
    snapshots: Vec<(ModEntry, ModStatus)>,
    game: GameInstall,
    use_default_path: bool,
    delete_behavior: DeleteBehavior,
    mut persist_tx: Option<xxmi_persist::PersistTx>,
    cancel: Arc<AtomicBool>,
    tx: mpsc::Sender<FolderBatchWorkerEvent>,
) {
    let mut remove_category_ids = Vec::new();
    let total_items = snapshots.len();
    let mut processed_items = 0usize;
    let snapshot_ids: HashSet<String> = snapshots
        .iter()
        .map(|(entry, _)| entry.id.clone())
        .collect();
    let captured_member_ids: HashSet<String> = targets.all_mod_ids.iter().cloned().collect();
    let mut category_member_ids: HashMap<String, HashSet<String>> = HashMap::new();
    for (entry, _) in &snapshots {
        if let Some(category_id) = effective_category_id(&categories, entry) {
            category_member_ids
                .entry(category_id.to_owned())
                .or_default()
                .insert(entry.id.clone());
        }
    }
    let mut completed_member_ids = HashSet::new();
    for (mut entry, before) in snapshots {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let original_id = entry.id.clone();
        let expected_root = entry.root_path.clone();
        let label = entry
            .metadata
            .user
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&entry.folder_name)
            .to_owned();
        let unsafe_content = entry.unsafe_content;
        // Unreal's active and disabled roots are both visible to the game process.  Recheck at
        // the item boundary because a game can launch after the UI admitted the batch.
        let locked = game.is_unreal_engine()
            && xxmi_persist::game_process_running_for_reload(&game)
            && matches!(
                action,
                FolderBatchAction::Enable
                    | FolderBatchAction::Disable
                    | FolderBatchAction::DeleteContents
                    | FolderBatchAction::DeleteFoldersAndContents
            )
            && match action {
                FolderBatchAction::Enable => entry.status == ModStatus::Disabled,
                FolderBatchAction::Disable
                | FolderBatchAction::DeleteContents
                | FolderBatchAction::DeleteFoldersAndContents => entry.status == ModStatus::Active,
                _ => false,
            };
        if locked {
            if tx
                .send(FolderBatchWorkerEvent::Item {
                    id: original_id,
                    expected_root: expected_root.clone(),
                    label,
                    unsafe_content,
                    before,
                    entry: Some(entry),
                    result: FolderBatchItemResult::SkippedLocked,
                })
                .is_err()
            {
                return;
            }
            processed_items += 1;
            continue;
        }
        if !entry.root_path.exists() {
            if tx
                .send(FolderBatchWorkerEvent::Item {
                    id: original_id,
                    expected_root: expected_root.clone(),
                    label,
                    unsafe_content,
                    before,
                    entry: Some(entry),
                    result: FolderBatchItemResult::ChangedMissing,
                })
                .is_err()
            {
                return;
            }
            processed_items += 1;
            continue;
        }
        if !folder_batch_physical_item_matches(
            &entry,
            &game,
            use_default_path,
            &categories,
            &targets,
        ) {
            if tx
                .send(FolderBatchWorkerEvent::Item {
                    id: original_id,
                    expected_root: expected_root.clone(),
                    label,
                    unsafe_content,
                    before,
                    entry: Some(entry),
                    result: FolderBatchItemResult::ChangedMissing,
                })
                .is_err()
            {
                return;
            }
            processed_items += 1;
            continue;
        }
        let result = execute_folder_batch_item(
            action,
            &mut entry,
            &game,
            use_default_path,
            delete_behavior,
            &mut persist_tx,
            destination.as_deref(),
            destination_label.as_deref(),
        );
        let (entry, result) = match result {
            Ok(entry) => (entry, FolderBatchItemResult::Completed),
            Err(FolderBatchExecutionError::Unsupported) => {
                (Some(entry), FolderBatchItemResult::Unsupported)
            }
            Err(FolderBatchExecutionError::Failed(error)) => {
                (Some(entry), FolderBatchItemResult::Failed(error))
            }
        };
        if matches!(result, FolderBatchItemResult::Completed) {
            completed_member_ids.insert(original_id.clone());
        }
        if tx
            .send(FolderBatchWorkerEvent::Item {
                id: original_id,
                expected_root,
                label,
                unsafe_content,
                before,
                entry,
                result,
            })
            .is_err()
        {
            return;
        }
        processed_items += 1;
    }
    let may_remove_categories =
        !cancel.load(Ordering::Acquire) || (processed_items > 0 && processed_items == total_items);
    if may_remove_categories
        && matches!(
            action,
            FolderBatchAction::RemoveFolders | FolderBatchAction::DeleteFoldersAndContents
        )
    {
        // A category may be removed only after every captured member in it completed. If the
        // snapshot itself lost an ID before the worker began, retain all categories: a later
        // rescan cannot prove that missing member was safely handled.
        let snapshot_complete = snapshot_ids == captured_member_ids;
        remove_category_ids = targets
            .category_ids
            .iter()
            .filter(|category_id| {
                snapshot_complete
                    && category_member_ids.get(*category_id).is_none_or(|members| {
                        members
                            .iter()
                            .all(|member_id| completed_member_ids.contains(member_id))
                    })
            })
            .cloned()
            .collect();
    }
    let (commit_error, commit_warnings) = match persist_tx {
        Some(tx) => match tx.commit() {
            Ok(outcome) => (None, outcome.warnings),
            Err(error) => (Some(format!("{error:#}")), Vec::new()),
        },
        None => (None, Vec::new()),
    };
    let _ = tx.send(FolderBatchWorkerEvent::Finished {
        cancelled: cancel.load(Ordering::Acquire),
        remove_category_ids,
        commit_error,
        commit_warnings,
    });
}

fn folder_batch_physical_item_matches(
    entry: &ModEntry,
    game: &GameInstall,
    use_default_path: bool,
    categories: &[ModCategory],
    targets: &FolderBatchTargets,
) -> bool {
    if let Some(stored) = persistence::load_portable_mod_state(&entry.root_path)
        .ok()
        .flatten()
    {
        if stored.id != entry.id {
            return false;
        }
        let mut physical = entry.clone();
        physical.metadata = stored.metadata;
        if !mod_is_in_target_scope(categories, targets, &physical) {
            return false;
        }
    } else {
        // Older mods can legitimately have no portable metadata. Their path and category state
        // still get checked by the UI merge after the operation.
    }
    if game.is_unreal_engine() {
        let expected_root = match entry.status {
            ModStatus::Active => game.mods_path(use_default_path),
            ModStatus::Disabled => game.disabled_mods_path(use_default_path),
            ModStatus::Archived => None,
        };
        if let Some(expected_root) = expected_root
            && !strict_path_descendant(&entry.root_path, &expected_root)
        {
            return false;
        }
    } else {
        let Some(live_root) = game.mods_path(use_default_path) else {
            return false;
        };
        match entry.status {
            ModStatus::Active | ModStatus::Disabled => {
                // XXMI can discover nested mod roots below Mods. Keep the containment check
                // rather than requiring a direct parent, while still rejecting a stale root in
                // another game's tree. Disabled payloads stay in the same root below the real
                // model constant; the old lowercase spelling was never a valid container.
                if !entry.root_path.starts_with(&live_root) {
                    return false;
                }
                let disabled_root = entry.root_path.join(crate::model::DISABLED_CONTAINER);
                if entry.status == ModStatus::Disabled && !disabled_root.is_dir() {
                    return false;
                }
                if entry.status == ModStatus::Active && disabled_root.is_dir() {
                    return false;
                }
            }
            ModStatus::Archived => {
                let Ok(archive_root) = xxmi::archived_mods_root(game, use_default_path) else {
                    return false;
                };
                // Archive writes may preserve the nested physical organization below
                // Mods_Archived. Older archives may not have a recorded destination, and a
                // stale recorded path is harmless because restore confines its fallback to the
                // current live root.
                if !strict_path_descendant(&entry.root_path, &archive_root) {
                    return false;
                }
                if entry.archive_original_path.as_ref() == Some(&entry.root_path) {
                    return false;
                }
            }
        }
    }
    true
}

/// Return whether `path` is a strict lexical descendant of `root`.
///
/// The archive root itself is not a mod, and rejecting `ParentDir` components keeps a
/// syntactically prefixed path from escaping the archive tree without requiring the target to
/// exist on disk.
fn strict_path_descendant(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut has_normal_component = false;
    for component in relative.components() {
        match component {
            std::path::Component::ParentDir => return false,
            std::path::Component::Normal(_) => has_normal_component = true,
            _ => {}
        }
    }
    has_normal_component
}

enum FolderBatchExecutionError {
    Unsupported,
    Failed(String),
}

fn execute_folder_batch_item(
    action: FolderBatchAction,
    entry: &mut ModEntry,
    game: &GameInstall,
    use_default_path: bool,
    delete_behavior: DeleteBehavior,
    persist_tx: &mut Option<xxmi_persist::PersistTx>,
    destination: Option<&str>,
    destination_label: Option<&str>,
) -> Result<Option<ModEntry>, FolderBatchExecutionError> {
    let result = match action {
        FolderBatchAction::Enable => match game.definition.backend {
            GameBackend::Xxmi => HestiaApp::persisted_xxmi_enable(persist_tx, entry),
            GameBackend::UnrealEngine => unrealengine::enable_mod(entry, game, use_default_path),
        },
        FolderBatchAction::Disable => match game.definition.backend {
            GameBackend::Xxmi => HestiaApp::persisted_xxmi_disable(persist_tx, entry),
            GameBackend::UnrealEngine => unrealengine::disable_mod(entry, game, use_default_path),
        },
        FolderBatchAction::Archive => {
            if game.is_xxmi() {
                HestiaApp::persisted_xxmi_archive(persist_tx, entry, game, use_default_path)
            } else {
                return Err(FolderBatchExecutionError::Unsupported);
            }
        }
        FolderBatchAction::Restore => {
            if game.is_xxmi() {
                HestiaApp::persisted_xxmi_restore_from_archive(
                    persist_tx,
                    entry,
                    game,
                    use_default_path,
                )
            } else {
                return Err(FolderBatchExecutionError::Unsupported);
            }
        }
        FolderBatchAction::MoveContents => {
            entry.metadata.user.category_id = destination.map(str::to_owned);
            entry.metadata.user.category = destination_label.unwrap_or_default().to_string();
            entry.updated_at = Utc::now();
            match game.definition.backend {
                GameBackend::Xxmi => xxmi::save_mod_metadata(entry),
                GameBackend::UnrealEngine => unrealengine::write_portable_metadata(entry),
            }
        }
        FolderBatchAction::RemoveFolders => {
            entry.metadata.user.category_id = None;
            entry.metadata.user.category.clear();
            entry.updated_at = Utc::now();
            match game.definition.backend {
                GameBackend::Xxmi => xxmi::save_mod_metadata(entry),
                GameBackend::UnrealEngine => unrealengine::write_portable_metadata(entry),
            }
        }
        FolderBatchAction::DeleteContents | FolderBatchAction::DeleteFoldersAndContents => {
            match delete_behavior {
                DeleteBehavior::RecycleBin => HestiaApp::persisted_xxmi_recycle(persist_tx, entry),
                DeleteBehavior::Permanent => HestiaApp::persisted_xxmi_purge(persist_tx, entry),
            }
        }
        FolderBatchAction::CheckUpdates | FolderBatchAction::Update => Ok(()),
    };
    result
        .map(|()| match action {
            FolderBatchAction::DeleteContents | FolderBatchAction::DeleteFoldersAndContents => None,
            _ => Some(entry.clone()),
        })
        .map_err(|error| FolderBatchExecutionError::Failed(format!("{error:#}")))
}

#[cfg(test)]
mod folder_batch_tests {
    use super::*;

    fn game() -> GameInstall {
        GameInstall {
            definition: crate::model::GameDefinition {
                id: "game".into(),
                name: "Game".into(),
                backend: GameBackend::UnrealEngine,
                xxmi_code: "game".into(),
            },
            mods_path_override: None,
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: false,
            enabled: true,
        }
    }

    fn xxmi_game() -> GameInstall {
        let mut game = game();
        game.definition.backend = GameBackend::Xxmi;
        game
    }

    fn entry(id: &str, root_path: PathBuf, status: ModStatus) -> ModEntry {
        ModEntry {
            id: id.into(),
            game_id: "game".into(),
            folder_name: id.into(),
            root_path,
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
            update_state: ModUpdateState::Unlinked,
        }
    }

    #[test]
    fn scoped_ids_are_deduplicated_and_follow_contents_scope() {
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: vec!["cat".into()],
            mod_ids: vec!["one".into(), "one".into()],
            all_mod_ids: vec!["one".into(), "two".into(), "one".into()],
            scope: FolderContentsScope::Visible,
        };
        assert_eq!(scoped_target_ids(&targets), vec!["one"]);
        let all = FolderBatchTargets {
            scope: FolderContentsScope::All,
            ..targets
        };
        assert_eq!(scoped_target_ids(&all), vec!["one", "two"]);
    }

    #[test]
    fn legacy_category_name_is_a_member_of_the_selected_folder() {
        let categories = vec![ModCategory {
            id: "cat-id".into(),
            game_id: "game".into(),
            name: "Legacy Label".into(),
            order: 0,
            gamebanana_character: None,
        }];
        let mut mod_entry = entry("mod", PathBuf::from("mod"), ModStatus::Active);
        mod_entry.metadata.user.category = "legacy label".into();
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: vec!["cat-id".into()],
            mod_ids: vec!["mod".into()],
            all_mod_ids: vec!["mod".into()],
            scope: FolderContentsScope::All,
        };
        assert!(mod_is_in_target_scope(&categories, &targets, &mod_entry));
    }

    #[test]
    fn permanent_delete_removes_only_the_snapshotted_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("payload.bin"), b"payload").unwrap();
        let mut mod_entry = entry("mod", root.clone(), ModStatus::Active);
        let mut ptx = None;
        let result = execute_folder_batch_item(
            FolderBatchAction::DeleteContents,
            &mut mod_entry,
            &game(),
            false,
            DeleteBehavior::Permanent,
            &mut ptx,
            None,
            None,
        );
        assert!(matches!(result, Ok(None)));
        assert!(!root.exists());
        assert!(temp.path().exists());
    }

    #[test]
    fn remove_folder_contents_keeps_files_and_clears_category_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("payload.bin"), b"payload").unwrap();
        let mut mod_entry = entry("mod", root.clone(), ModStatus::Active);
        mod_entry.metadata.user.category_id = Some("cat".into());
        mod_entry.metadata.user.category = "Legacy".into();
        let mut ptx = None;
        let result = execute_folder_batch_item(
            FolderBatchAction::RemoveFolders,
            &mut mod_entry,
            &game(),
            false,
            DeleteBehavior::Permanent,
            &mut ptx,
            None,
            None,
        );
        assert!(matches!(result, Ok(Some(_))));
        assert!(root.join("payload.bin").exists());
        assert!(mod_entry.metadata.user.category_id.is_none());
        assert!(mod_entry.metadata.user.category.is_empty());
    }

    #[test]
    fn cancellation_before_first_item_does_not_delete_snapshotted_content() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("payload.bin"), b"payload").unwrap();
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: Vec::new(),
            mod_ids: vec!["mod".into()],
            all_mod_ids: vec!["mod".into()],
            scope: FolderContentsScope::All,
        };
        let cancel = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::channel();
        run_folder_batch_worker(
            FolderBatchAction::DeleteContents,
            targets,
            None,
            None,
            Vec::new(),
            vec![(
                entry("mod", root.clone(), ModStatus::Active),
                ModStatus::Active,
            )],
            game(),
            false,
            DeleteBehavior::Permanent,
            None,
            cancel,
            tx,
        );
        assert!(root.exists());
        assert!(matches!(
            rx.try_recv(),
            Ok(FolderBatchWorkerEvent::Finished {
                cancelled: true,
                ..
            })
        ));
    }

    #[test]
    fn changed_member_keeps_folder_for_cleanup_retry() {
        let temp = tempfile::tempdir().unwrap();
        let mut mod_entry = entry(
            "mod",
            temp.path().join("externally-removed"),
            ModStatus::Active,
        );
        mod_entry.metadata.user.category_id = Some("cat".into());
        let categories = vec![ModCategory {
            id: "cat".into(),
            game_id: "game".into(),
            name: "Category".into(),
            order: 0,
            gamebanana_character: None,
        }];
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: vec!["cat".into()],
            mod_ids: vec!["mod".into()],
            all_mod_ids: vec!["mod".into()],
            scope: FolderContentsScope::All,
        };
        let (tx, rx) = mpsc::channel();
        run_folder_batch_worker(
            FolderBatchAction::DeleteFoldersAndContents,
            targets,
            None,
            None,
            categories,
            vec![(mod_entry, ModStatus::Active)],
            game(),
            false,
            DeleteBehavior::Permanent,
            None,
            Arc::new(AtomicBool::new(false)),
            tx,
        );
        let mut retained = false;
        while let Ok(event) = rx.try_recv() {
            if let FolderBatchWorkerEvent::Finished {
                remove_category_ids,
                ..
            } = event
            {
                retained = remove_category_ids.is_empty();
            }
        }
        assert!(retained);
    }

    #[test]
    fn worker_enables_xxmi_disabled_container_with_actual_constant() {
        let temp = tempfile::tempdir().unwrap();
        let mods_root = temp.path().join("Mods");
        let root = mods_root.join("mod");
        let disabled = root.join(crate::model::DISABLED_CONTAINER);
        fs::create_dir_all(&disabled).unwrap();
        fs::write(disabled.join("payload.ini"), b"payload").unwrap();
        let mut game = xxmi_game();
        game.mods_path_override = Some(mods_root);
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: Vec::new(),
            mod_ids: vec!["mod".into()],
            all_mod_ids: vec!["mod".into()],
            scope: FolderContentsScope::All,
        };
        let (tx, rx) = mpsc::channel();
        run_folder_batch_worker(
            FolderBatchAction::Enable,
            targets,
            None,
            None,
            Vec::new(),
            vec![(
                entry("mod", root.clone(), ModStatus::Disabled),
                ModStatus::Disabled,
            )],
            game,
            false,
            DeleteBehavior::Permanent,
            None,
            Arc::new(AtomicBool::new(false)),
            tx,
        );
        let mut saw_completed = false;
        while let Ok(event) = rx.try_recv() {
            if let FolderBatchWorkerEvent::Item {
                result: FolderBatchItemResult::Completed,
                ..
            } = event
            {
                saw_completed = true;
            }
        }
        assert!(saw_completed);
        assert!(root.join("payload.ini").is_file());
        assert!(!disabled.exists());
    }

    #[test]
    fn unreal_physical_match_accepts_nested_active_and_disabled_mods() {
        let temp = tempfile::tempdir().unwrap();
        let active_root = temp.path().join("Mods");
        let mut game = game();
        game.mods_path_override = Some(active_root.clone());
        let disabled_root = game.disabled_mods_path(false).unwrap();
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: Vec::new(),
            mod_ids: vec!["mod".into()],
            all_mod_ids: vec!["mod".into()],
            scope: FolderContentsScope::All,
        };

        assert!(folder_batch_physical_item_matches(
            &entry(
                "mod",
                active_root.join("Ardelia").join("Outfit A"),
                ModStatus::Active,
            ),
            &game,
            false,
            &[],
            &targets,
        ));
        assert!(folder_batch_physical_item_matches(
            &entry(
                "mod",
                disabled_root.join("Ardelia").join("Outfit B"),
                ModStatus::Disabled,
            ),
            &game,
            false,
            &[],
            &targets,
        ));
        assert!(!folder_batch_physical_item_matches(
            &entry("mod", active_root.clone(), ModStatus::Active),
            &game,
            false,
            &[],
            &targets,
        ));
        assert!(!folder_batch_physical_item_matches(
            &entry("mod", disabled_root.clone(), ModStatus::Disabled),
            &game,
            false,
            &[],
            &targets,
        ));
        assert!(!folder_batch_physical_item_matches(
            &entry(
                "mod",
                temp.path().join("Other").join("Outfit C"),
                ModStatus::Active,
            ),
            &game,
            false,
            &[],
            &targets,
        ));
    }

    #[test]
    fn archived_physical_match_allows_missing_or_stale_original_destination() {
        let temp = tempfile::tempdir().unwrap();
        let mods_root = temp.path().join("Mods");
        let archive_root = temp.path().join("Mods_Archived");
        let root = archive_root.join("mod");
        fs::create_dir_all(&root).unwrap();
        let mut game = xxmi_game();
        game.mods_path_override = Some(mods_root);
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: Vec::new(),
            mod_ids: vec!["mod".into()],
            all_mod_ids: vec!["mod".into()],
            scope: FolderContentsScope::All,
        };
        let mut missing_original = entry("mod", root.clone(), ModStatus::Archived);
        assert!(folder_batch_physical_item_matches(
            &missing_original,
            &game,
            false,
            &[],
            &targets,
        ));
        missing_original.archive_original_path =
            Some(temp.path().join("old-game-root").join("mod"));
        assert!(folder_batch_physical_item_matches(
            &missing_original,
            &game,
            false,
            &[],
            &targets,
        ));
    }

    #[test]
    fn archived_physical_match_accepts_nested_mods_but_rejects_root_and_escape_paths() {
        let temp = tempfile::tempdir().unwrap();
        let mods_root = temp.path().join("Mods");
        let archive_root = temp.path().join("Mods_Archived");
        let nested_root = archive_root.join("Ardelia").join("Outfit A");
        let mut game = xxmi_game();
        game.mods_path_override = Some(mods_root);
        let targets = FolderBatchTargets {
            game_id: "game".into(),
            category_ids: Vec::new(),
            mod_ids: vec!["mod".into()],
            all_mod_ids: vec!["mod".into()],
            scope: FolderContentsScope::All,
        };

        assert!(folder_batch_physical_item_matches(
            &entry("mod", nested_root, ModStatus::Archived),
            &game,
            false,
            &[],
            &targets,
        ));
        assert!(!folder_batch_physical_item_matches(
            &entry("mod", archive_root.clone(), ModStatus::Archived),
            &game,
            false,
            &[],
            &targets,
        ));
        assert!(!folder_batch_physical_item_matches(
            &entry(
                "mod",
                temp.path().join("Other").join("mod"),
                ModStatus::Archived,
            ),
            &game,
            false,
            &[],
            &targets,
        ));
        assert!(!folder_batch_physical_item_matches(
            &entry(
                "mod",
                archive_root.join("..").join("Other").join("mod"),
                ModStatus::Archived,
            ),
            &game,
            false,
            &[],
            &targets,
        ));
    }
}
