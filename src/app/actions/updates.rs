fn tracked_file_meta_from_mod_file(file: &gamebanana::ModFile) -> TrackedFileMeta {
    TrackedFileMeta {
        file_id: file.id,
        file_name: file.file_name.clone(),
        date_added: file.date_added,
        version: file.version.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| v.to_string()),
        archived: file.is_archived,
    }
}

fn selected_file_baseline_ts(file_set: &FileSetRecipe) -> Option<i64> {
    file_set.selected_files_meta.iter().map(|file| file.date_added).max()
}

fn candidate_signature(candidates: Vec<TrackedFileMeta>) -> Option<IgnoredUpdateSignature> {
    if candidates.is_empty() {
        None
    } else {
        Some(IgnoredUpdateSignature {
            files: candidates,
            profile_update_ts: None,
            prearmed_next_update: false,
        })
    }
}

fn tracked_update_signature(
    tracked_files: &[TrackedFileMeta],
    all_remote_files: &[&gamebanana::ModFile],
) -> (ModUpdateState, Option<IgnoredUpdateSignature>) {
    let installed_baseline_ts = tracked_files.iter().map(|file| file.date_added).max();
    let latest_remote_ts = all_remote_files.iter().map(|file| file.date_added).max();

    let tracked_files_still_exist = tracked_files.iter().all(|tracked| {
        all_remote_files.iter().any(|file| {
            file.id == tracked.file_id
                || (file.file_name == tracked.file_name && file.date_added == tracked.date_added)
        })
    });

    match (installed_baseline_ts, latest_remote_ts) {
        (Some(installed), Some(latest)) if latest > installed => {
            let candidates = all_remote_files
                .iter()
                .copied()
                .filter(|file| file.date_added == latest)
                .map(tracked_file_meta_from_mod_file)
                .collect();
            (ModUpdateState::UpdateAvailable, candidate_signature(candidates))
        }
        (_, Some(_)) if tracked_files_still_exist => (ModUpdateState::UpToDate, None),
        (_, Some(_)) => (ModUpdateState::MissingSource, None),
        _ => (ModUpdateState::MissingSource, None),
    }
}

fn determine_tracked_meta_update_state(
    tracked_files: &[TrackedFileMeta],
    all_remote_files: &[&gamebanana::ModFile],
) -> ModUpdateState {
    tracked_update_signature(tracked_files, all_remote_files).0
}

fn compute_update_signature(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    let all_remote_files: Vec<&gamebanana::ModFile> = profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| file.download_url.is_some())
        .collect();
    if !file_set.selected_files_meta.is_empty() {
        return tracked_update_signature(&file_set.selected_files_meta, &all_remote_files).1;
    }
    if !file_set.selected_file_ids.is_empty() {
        let selected_ids: HashSet<u64> = file_set.selected_file_ids.iter().copied().collect();
        let tracked_files: Vec<_> = all_remote_files
            .iter()
            .copied()
            .filter(|file| selected_ids.contains(&file.id))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        if tracked_files.len() == selected_ids.len() {
            return tracked_update_signature(&tracked_files, &all_remote_files).1;
        }
    }
    if !file_set.selected_file_names.is_empty() {
        let selected_names: HashSet<&str> = file_set
            .selected_file_names
            .iter()
            .map(String::as_str)
            .collect();
        let tracked_files: Vec<_> = all_remote_files
            .iter()
            .copied()
            .filter(|file| selected_names.contains(file.file_name.as_str()))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        if tracked_files.len() == selected_names.len() {
            return tracked_update_signature(&tracked_files, &all_remote_files).1;
        }
    }
    None
}

fn profile_update_signature(
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    profile
        .date_updated
        .or(Some(profile.date_modified))
        .filter(|update_ts| *update_ts > 0)
        .map(|update_ts| IgnoredUpdateSignature {
            files: Vec::new(),
            profile_update_ts: Some(update_ts),
            prearmed_next_update: false,
        })
}

fn prearm_next_update_signature(
    mut signature: IgnoredUpdateSignature,
) -> IgnoredUpdateSignature {
    signature.prearmed_next_update = true;
    signature
}

fn current_remote_signature(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    let all_remote_files: Vec<&gamebanana::ModFile> = profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| file.download_url.is_some())
        .collect();
    if !file_set.selected_files_meta.is_empty() {
        let tracked_files: Vec<_> = file_set
            .selected_files_meta
            .iter()
            .filter_map(|tracked| {
                all_remote_files
                    .iter()
                    .copied()
                    .find(|file| {
                        file.id == tracked.file_id
                            || (file.file_name == tracked.file_name
                                && file.date_added == tracked.date_added)
                    })
                    .map(tracked_file_meta_from_mod_file)
            })
            .collect();
        if tracked_files.len() == file_set.selected_files_meta.len() {
            return candidate_signature(tracked_files).map(prearm_next_update_signature);
        }
    }
    if !file_set.selected_file_ids.is_empty() {
        let selected_ids: HashSet<u64> = file_set.selected_file_ids.iter().copied().collect();
        let tracked_files: Vec<_> = all_remote_files
            .iter()
            .copied()
            .filter(|file| selected_ids.contains(&file.id))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        if tracked_files.len() == selected_ids.len() {
            return candidate_signature(tracked_files).map(prearm_next_update_signature);
        }
    }
    if !file_set.selected_file_names.is_empty() {
        let selected_names: HashSet<&str> = file_set
            .selected_file_names
            .iter()
            .map(String::as_str)
            .collect();
        let tracked_files: Vec<_> = all_remote_files
            .iter()
            .copied()
            .filter(|file| selected_names.contains(file.file_name.as_str()))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        if tracked_files.len() == selected_names.len() {
            return candidate_signature(tracked_files).map(prearm_next_update_signature);
        }
    }
    profile_update_signature(profile).map(prearm_next_update_signature)
}

fn current_update_signature_for_state(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
    raw_state: ModUpdateState,
) -> Option<IgnoredUpdateSignature> {
    compute_update_signature(file_set, profile).or_else(|| {
        matches!(raw_state, ModUpdateState::UpdateAvailable)
            .then(|| profile_update_signature(profile))
            .flatten()
    })
}

fn source_profile_for_compare(source: &ModSourceData) -> Option<gamebanana::ProfileResponse> {
    source
        .raw_profile_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<gamebanana::ProfileResponse>(raw).ok())
        .or_else(|| source.snapshot.as_ref().map(|snapshot| profile_to_response(Some(snapshot))))
}

fn compute_raw_update_state(mod_entry: &ModEntry) -> Option<ModUpdateState> {
    let source = mod_entry.source.as_ref()?;
    let profile = source_profile_for_compare(source)?;
    let has_local_changes = source.baseline_content_mtime.map(|t| t.timestamp())
        != mod_entry.content_mtime.map(|t| t.timestamp())
        || source.baseline_ini_hash != mod_entry.ini_hash;
    if has_local_changes {
        Some(ModUpdateState::ModifiedLocally)
    } else {
        let local_sync_ts = selected_file_baseline_ts(&source.file_set)
            .or(profile.date_updated.or(Some(profile.date_modified)));
        Some(determine_file_set_update_state(&source.file_set, local_sync_ts, &profile))
    }
}

fn mod_has_local_changes_for_update_check(mod_entry: &ModEntry) -> bool {
    let Some(source) = mod_entry.source.as_ref() else {
        return false;
    };
    source.baseline_content_mtime.map(|t| t.timestamp())
        != mod_entry.content_mtime.map(|t| t.timestamp())
        || source.baseline_ini_hash != mod_entry.ini_hash
}

fn apply_ignored_update_override(
    source: &mut ModSourceData,
    raw_state: ModUpdateState,
    profile: Option<&gamebanana::ProfileResponse>,
) -> ModUpdateState {
    if source.ignore_update_always {
        source.ignored_update_signature = None;
        return ModUpdateState::IgnoringUpdateAlways;
    }
    let current_signature =
        profile.and_then(|profile| current_update_signature_for_state(&source.file_set, profile, raw_state));
    match raw_state {
        ModUpdateState::UpdateAvailable => {
            if let Some(current) = current_signature.as_ref() {
                if source
                    .ignored_update_signature
                    .as_ref()
                    .is_some_and(|ignored| ignored.prearmed_next_update)
                {
                    let mut ignored = current.clone();
                    ignored.prearmed_next_update = false;
                    source.ignored_update_signature = Some(ignored);
                    return ModUpdateState::IgnoringUpdateOnce;
                }
                if source
                    .ignored_update_signature
                    .as_ref()
                    .is_some_and(|ignored| ignored == current)
                {
                    return ModUpdateState::IgnoringUpdateOnce;
                }
            }
        }
        ModUpdateState::UpToDate
        | ModUpdateState::CheckSkipped
        => {
            if !source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|ignored| ignored.prearmed_next_update)
            {
                source.ignored_update_signature = None;
            }
        }
        ModUpdateState::Unlinked
        | ModUpdateState::MissingSource
        | ModUpdateState::IgnoringUpdateOnce
        | ModUpdateState::IgnoringUpdateAlways => {
            source.ignored_update_signature = None;
        }
        ModUpdateState::ModifiedLocally => {}
    }

    if matches!(raw_state, ModUpdateState::ModifiedLocally) {
        if let Some(current) = current_signature.as_ref() {
            if source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|ignored| ignored.prearmed_next_update)
            {
                let mut ignored = current.clone();
                ignored.prearmed_next_update = false;
                source.ignored_update_signature = Some(ignored);
            }
        }
    }

    if source
        .ignored_update_signature
        .as_ref()
        .is_some_and(|ignored| {
            !ignored.prearmed_next_update && current_signature.as_ref() != Some(ignored)
        })
    {
        source.ignored_update_signature = None;
    }

    raw_state
}

fn ignore_once_signature_for_mod(mod_entry: &ModEntry) -> Option<IgnoredUpdateSignature> {
    let source = mod_entry.source.as_ref()?;
    let profile = source_profile_for_compare(source)?;
    let raw_state = if matches!(mod_entry.update_state, ModUpdateState::ModifiedLocally) {
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or_else(|| source.snapshot.as_ref().and_then(|snapshot| snapshot.update_ts))
                .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));
        determine_file_set_update_state(&source.file_set, local_sync_ts, &profile)
    } else {
        mod_entry.update_state
    };
    current_update_signature_for_state(&source.file_set, &profile, raw_state)
        .or_else(|| current_remote_signature(&source.file_set, &profile))
}

fn determine_file_set_update_state(
    file_set: &FileSetRecipe,
    local_ts: Option<i64>,
    profile: &gamebanana::ProfileResponse,
) -> ModUpdateState {
    if gamebanana::is_unavailable(profile) {
        return ModUpdateState::MissingSource;
    }

    let all_remote_files: Vec<&gamebanana::ModFile> = profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| file.download_url.is_some())
        .collect();

    if !file_set.selected_files_meta.is_empty() {
        return determine_tracked_meta_update_state(&file_set.selected_files_meta, &all_remote_files);
    }

    if !file_set.selected_file_ids.is_empty()
        && file_set
            .selected_file_ids
            .iter()
            .any(|id| !all_remote_files.iter().any(|file| file.id == *id))
    {
        return ModUpdateState::MissingSource;
    }

    if !file_set.selected_file_names.is_empty()
        && file_set
            .selected_file_names
            .iter()
            .any(|name| !all_remote_files.iter().any(|file| &file.file_name == name))
    {
        return ModUpdateState::MissingSource;
    }

    determine_update_state(local_ts, profile)
}

fn backfill_selected_files_meta(file_set: &mut FileSetRecipe, profile: &gamebanana::ProfileResponse) -> bool {
    if !file_set.selected_files_meta.is_empty() || file_set.selected_file_ids.is_empty() {
        return false;
    }
    let matched: Vec<_> = profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| file_set.selected_file_ids.contains(&file.id))
        .map(tracked_file_meta_from_mod_file)
        .collect();
    if matched.is_empty() {
        return false;
    }
    file_set.selected_files_meta = matched;
    true
}

fn determine_update_state(local_ts: Option<i64>, profile: &gamebanana::ProfileResponse) -> ModUpdateState {
    if gamebanana::is_unavailable(profile) {
        return ModUpdateState::MissingSource;
    }
    let remote_ts = profile.date_updated.or(Some(profile.date_modified));
    if let (Some(local), Some(remote)) = (local_ts, remote_ts) {
        if remote > local {
            return ModUpdateState::UpdateAvailable;
        }
    }
    ModUpdateState::UpToDate
}

fn profile_to_response(snapshot: Option<&GameBananaSnapshot>) -> gamebanana::ProfileResponse {
    snapshot
        .map(|s| gamebanana::ProfileResponse {
            is_private: s.is_private,
            is_deleted: s.is_deleted,
            is_trashed: s.is_trashed,
            is_withheld: s.is_withheld,
            date_updated: s.update_ts,
            ..Default::default()
        })
        .unwrap_or_default()
}

fn profile_to_snapshot(profile: &gamebanana::ProfileResponse) -> GameBananaSnapshot {
    let mut files = Vec::new();
    for file in &profile.files {
        files.push(GameBananaFileMeta {
            file_id: file.id,
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            date_added: file.date_added,
            download_count: file.download_count,
            description: file.description.clone(),
            download_url: file.download_url.clone(),
            archived: false,
        });
    }
    for file in &profile.archived_files {
        files.push(GameBananaFileMeta {
            file_id: file.id,
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            date_added: file.date_added,
            download_count: file.download_count,
            description: file.description.clone(),
            download_url: file.download_url.clone(),
            archived: true,
        });
    }
    GameBananaSnapshot {
        title: profile.name.clone(),
        authors: gamebanana::all_authors(profile),
        version: None,
        publish_ts: Some(profile.date_added),
        update_ts: profile.date_updated.or(Some(profile.date_modified)),
        description: profile.short_description.clone(),
        preview_urls: profile
            .preview_media
            .as_ref()
            .map(|preview| preview.images.iter().map(gamebanana::full_image_url).collect())
            .unwrap_or_default(),
        files,
        is_private: profile.is_private,
        is_deleted: profile.is_deleted,
        is_trashed: profile.is_trashed,
        is_withheld: profile.is_withheld,
        unsafe_content: !profile.content_ratings.is_empty(),
    }
}

impl HestiaApp {
    fn update_check_item_for_mod(
        &self,
        mod_entry_id: &str,
    ) -> Option<(String, String, u64, Option<i64>, FileSetRecipe)> {
        let mod_entry = self.state.mods.iter().find(|m| m.id == mod_entry_id)?;
        let source = mod_entry.source.as_ref()?;
        let link = source.gamebanana.as_ref()?;
        let local_sync_ts = selected_file_baseline_ts(&source.file_set)
            .or_else(|| source.snapshot.as_ref().and_then(|s| s.update_ts))
            .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));
        Some((
            mod_entry.id.clone(),
            mod_entry.game_id.clone(),
            link.mod_id,
            local_sync_ts,
            source.file_set.clone(),
        ))
    }

    fn dispatch_update_check_items(
        &mut self,
        items: Vec<(String, String, u64, Option<i64>, FileSetRecipe)>,
    ) {
        if items.is_empty() {
            return;
        }
        if self.update_check_tx.send(UpdateCheckRequest { items }).is_ok() {
            self.update_check_inflight = true;
        }
    }

    fn queue_update_check_for_mod(&mut self, mod_entry_id: &str) {
        if self.update_check_inflight {
            self.pending_update_check_mods
                .insert(mod_entry_id.to_string());
            return;
        }
        self.pending_update_check_mods.remove(mod_entry_id);
        let Some(item) = self.update_check_item_for_mod(mod_entry_id) else {
            return;
        };
        self.pending_update_check_game = None;
        self.dispatch_update_check_items(vec![item]);
    }

    fn queue_update_check_for_linked_mods(&mut self, target_game_id: Option<&str>) {
        if self.update_check_inflight {
            self.pending_update_check_game = target_game_id.map(|id| id.to_string());
            return;
        }
        self.pending_update_check_game = None;
        let mut items = Vec::new();
        let update_check_statuses = self.state.update_check_statuses;
        let modified_update_behavior = self.state.modified_update_behavior;
        let mut state_changed_without_fetch = false;
        for mod_entry in &mut self.state.mods {
            if let Some(id) = target_game_id {
                if mod_entry.game_id != id { continue; }
            }
            if mod_entry
                .source
                .as_ref()
                .and_then(|source| source.gamebanana.as_ref())
                .is_none()
            {
                continue;
            }
            if !Self::status_target_enabled(&mod_entry.status, update_check_statuses) {
                if mod_entry.update_state != ModUpdateState::CheckSkipped {
                    mod_entry.update_state = ModUpdateState::CheckSkipped;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            let Some(source) = &mod_entry.source else {
                continue;
            };
            let Some(link) = &source.gamebanana else {
                continue;
            };
            if source.ignore_update_always {
                if mod_entry.update_state != ModUpdateState::IgnoringUpdateAlways {
                    mod_entry.update_state = ModUpdateState::IgnoringUpdateAlways;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            if modified_update_behavior == ModifiedUpdateBehavior::HideButton
                && mod_has_local_changes_for_update_check(mod_entry)
            {
                if mod_entry.update_state != ModUpdateState::ModifiedLocally {
                    mod_entry.update_state = ModUpdateState::ModifiedLocally;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            // Prefer the exact GameBanana file(s) this mod was installed from.
            // Fall back to the profile snapshot timestamp for older metadata.
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or_else(|| source.snapshot.as_ref().and_then(|s| s.update_ts))
                .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));

            items.push((
                mod_entry.id.clone(),
                mod_entry.game_id.clone(),
                link.mod_id,
                local_sync_ts,
                source.file_set.clone(),
            ));
        }
        if state_changed_without_fetch {
            self.save_state();
        }
        self.dispatch_update_check_items(items);
    }

    fn consume_update_check_results(&mut self) {
        while let Ok(result) = self.update_check_rx.try_recv() {
            self.update_check_inflight = false;
            let mut warn_lines: Vec<String> = Vec::new();
            let mut auto_update_ids: Vec<String> = Vec::new();
            let active_update_tasks: HashSet<(String, String)> = self
                .state
                .tasks
                .iter()
                .filter(|task| {
                    matches!(
                        task.status,
                        TaskStatus::Queued
                            | TaskStatus::Downloading
                            | TaskStatus::Installing
                            | TaskStatus::Canceling
                    )
                })
                .filter_map(|task| {
                    task.game_id
                        .as_ref()
                        .map(|game_id| (task.title.clone(), game_id.clone()))
                })
                .collect();
            for (mod_id, state, snapshot, err, raw_json, profile) in result.states {
                let mut mod_updated = false;
                let mut should_sync_images = false;
                let mut sync_profile: Option<Box<gamebanana::ProfileResponse>> = None;
                let fetch_failed = err.is_some()
                    && snapshot.is_none()
                    && raw_json.is_none()
                    && profile.is_none();
                if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
                    let old_preview_urls = mod_entry
                        .source
                        .as_ref()
                        .and_then(|s| s.snapshot.as_ref())
                        .map(|s| s.preview_urls.clone())
                        .unwrap_or_default();
                    let has_local_changes = mod_entry
                        .source
                        .as_ref()
                        .is_some_and(|source| {
                            source.baseline_content_mtime.map(|t| t.timestamp()) != mod_entry.content_mtime.map(|t| t.timestamp())
                                || source.baseline_ini_hash != mod_entry.ini_hash
                        });
                    if fetch_failed && !has_local_changes {
                        warn_lines.push(format!(
                            "{} (update check failed; keeping previous state: {})",
                            mod_entry.folder_name,
                            err.as_deref().unwrap_or("unknown error"),
                        ));
                    }
                    mod_entry.unsafe_content = snapshot
                        .as_ref()
                        .map_or(mod_entry.unsafe_content, |s| s.unsafe_content);
                    if let Some(snap) = snapshot.as_ref() {
                        should_sync_images = old_preview_urls != snap.preview_urls
                            || Self::is_missing_expected_source_images(mod_entry, snap);
                    }
                    if let Some(source) = mod_entry.source.as_mut() {
                        if let Some(profile) = profile.as_deref() {
                            let _ = backfill_selected_files_meta(&mut source.file_set, profile);
                        }
                        if let Some(s) = snapshot {
                            source.snapshot = Some(s);
                        }
                        if let Some(raw) = raw_json {
                            source.raw_profile_json = Some(raw);
                        }
                        let raw_state = if has_local_changes {
                            ModUpdateState::ModifiedLocally
                        } else {
                            state
                        };
                        if !fetch_failed {
                            mod_entry.update_state =
                                apply_ignored_update_override(source, raw_state, profile.as_deref());
                        } else if has_local_changes {
                            mod_entry.update_state = ModUpdateState::ModifiedLocally;
                        }
                        if let Some(message) = err {
                            if !fetch_failed {
                                warn_lines.push(format!("{} ({message})", mod_entry.folder_name));
                            }
                        }
                    }
                    if !fetch_failed || has_local_changes {
                        let _ = xxmi::save_mod_metadata(mod_entry);
                        mod_updated = true;
                    }
                    if should_sync_images {
                        sync_profile = profile;
                    }
                    let modified_update_available = Self::has_modified_update_available(mod_entry);
                    let auto_update_allowed = mod_entry.update_state == ModUpdateState::UpdateAvailable
                        || (self.state.modified_update_behavior == ModifiedUpdateBehavior::Yes
                            && modified_update_available);
                    let should_auto_apply = !fetch_failed
                        && auto_update_allowed
                        && Self::status_target_enabled(&mod_entry.status, self.state.auto_update_statuses)
                        && !active_update_tasks.contains(&(
                            format!(
                                "Updating: {}",
                                mod_entry
                                    .metadata
                                    .user
                                    .title
                                    .as_ref()
                                    .unwrap_or(&mod_entry.folder_name)
                            ),
                            mod_entry.game_id.clone(),
                        ));
                    if should_auto_apply {
                        auto_update_ids.push(mod_entry.id.clone());
                    }
                }
                if mod_updated && should_sync_images {
                    if let Some(p) = sync_profile {
                        if let Some(mod_root_path) = self
                            .state
                            .mods
                            .iter()
                            .find(|m| m.id == mod_id)
                            .map(|m| m.root_path.clone())
                        {
                            let job_id = self.next_background_job_id();
                            let _ = self.install_request_tx.send(InstallRequest::SyncImages {
                                job_id,
                                mod_entry_id: mod_id.clone(),
                                mod_root_path,
                                profile: p,
                            });
                        }
                    } else {
                        self.enqueue_mod_image_sync(&mod_id);
                    }
                }
            }
            for line in warn_lines {
                self.log_warn(format!("update check: {line}"));
            }
            self.save_state();
            for mod_id in auto_update_ids {
                self.queue_update_apply(&mod_id);
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
    }

    fn status_target_enabled(status: &ModStatus, targets: ModStatusTargets) -> bool {
        match status {
            ModStatus::Active => targets.active,
            ModStatus::Disabled => targets.disabled,
            ModStatus::Archived => targets.archived,
        }
    }

    fn should_show_local_change_update_prefs(mod_entry: &ModEntry) -> bool {
        matches!(mod_entry.update_state, ModUpdateState::ModifiedLocally)
    }

    fn should_auto_replace_update(&self, job_id: u64) -> bool {
        if self.state.always_replace_on_update {
            return true;
        }
        let _ = job_id;
        false
    }

    fn configured_existing_target_choice(&self) -> Option<ConflictChoice> {
        match self.state.import_resolution {
            ImportResolution::Ask => None,
            ImportResolution::Replace => Some(ConflictChoice::Replace),
            ImportResolution::Merge => Some(ConflictChoice::Merge),
            ImportResolution::KeepBoth => Some(ConflictChoice::KeepBoth),
        }
    }

    fn resolve_update_existing_target_choice(&self, job_id: u64) -> Option<ConflictChoice> {
        if self.should_auto_replace_update(job_id) {
            Some(ConflictChoice::Replace)
        } else {
            self.configured_existing_target_choice()
        }
    }

    fn consume_startup_scan_events(&mut self) {
        while let Ok(event) = self.startup_scan_rx.try_recv() {
            match event {
                StartupScanEvent::Ready(mods) => {
                    self.state.mods = mods;
                    self.restore_imported_mod_categories(None);
                    self.sync_selection_after_refresh();
                    self.backfill_missing_mod_images(None);
                    self.sync_tools_for_selected_game();
                    self.save_state();
                    self.startup_scan_loading = false;
                    let launch_game_id = self.selected_game().map(|g| g.definition.id.clone());
                    self.queue_update_check_for_linked_mods(launch_game_id.as_deref());
                    self.request_automatic_app_update_check(0.0);
                }
                StartupScanEvent::Failed(error) => {
                    self.startup_scan_loading = false;
                    self.report_error_message(error, None);
                }
            }
        }
    }

    fn queue_game_refresh(&mut self, game_id: String) {
        if self.refresh_inflight {
            self.refresh_pending_selected_game = Some(game_id);
            return;
        }
        self.dispatch_selected_game_refresh(game_id);
    }

    fn dispatch_selected_game_refresh(&mut self, game_id: String) {
        let request = RefreshRequest {
            game_id: game_id.clone(),
            games: self.state.games.clone(),
            use_default_mods_path: self.state.use_default_mods_path,
            existing_mods: self.state.mods.clone(),
        };
        if self.refresh_request_tx.send(request).is_ok() {
            self.refresh_inflight = true;
        } else {
            self.refresh_inflight = false;
            self.report_error_message(
                format!("failed to queue selected-game refresh for {game_id}"),
                None,
            );
        }
    }

    fn consume_refresh_events(&mut self) {
        while let Ok(event) = self.refresh_result_rx.try_recv() {
            self.refresh_inflight = false;
            match event {
                RefreshEvent::Ready { game_id, mods } => {
                    let is_current = self
                        .selected_game()
                        .is_some_and(|g| g.definition.id == game_id);
                    let old_ts: HashMap<String, DateTime<Utc>> = self.state.mods.iter()
                        .map(|m| (m.id.clone(), m.updated_at))
                        .collect();
                    self.state.mods.retain(|m| m.game_id != game_id);
                    self.state.mods.extend(mods);
                    self.state.mods.sort_by(|a, b| {
                        a.game_id.cmp(&b.game_id).then_with(|| {
                            a.folder_name
                                .to_lowercase()
                                .cmp(&b.folder_name.to_lowercase())
                        })
                    });
                    self.restore_imported_mod_categories(Some(&game_id));
                    if is_current {
                        self.invalidate_stale_mod_textures(&old_ts);
                        self.sync_selection_after_refresh();
                        self.backfill_missing_mod_images(Some(&game_id));
                        self.sync_tools_for_selected_game();
                    }
                    let finalized_install =
                        self.resolve_pending_install_finalization_for_game(&game_id);
                    self.save_state();
                    if !finalized_install {
                        self.queue_update_check_for_linked_mods(Some(&game_id));
                    }
                }
                RefreshEvent::Failed { game_id, error } => {
                    let is_current = self
                        .selected_game()
                        .is_some_and(|g| g.definition.id == game_id);
                    if is_current {
                    self.report_error_message(
                        format!("selected-game refresh failed for {game_id}: {error}"),
                        Some("Could not refresh mods"),
                    );
                }
                }
            }
            if let Some(next_game_id) = self.refresh_pending_selected_game.take() {
                self.dispatch_selected_game_refresh(next_game_id);
            }
        }
    }

    fn resolve_pending_install_finalization_for_game(&mut self, game_id: &str) -> bool {
        let mut finalized_any = false;
        let job_ids: Vec<u64> = self.pending_install_finalize.keys().copied().collect();
        for job_id in job_ids {
            let Some(payload) = self.pending_install_finalize.remove(&job_id) else {
                continue;
            };
            let belongs_to_game = payload.installed_paths.iter().any(|path| {
                self.state
                    .mods
                    .iter()
                    .find(|m| m.root_path == *path)
                    .is_some_and(|m| m.game_id == game_id)
            });
            if !belongs_to_game {
                continue;
            }
            self.finalize_install_after_refresh(job_id, payload);
            finalized_any = true;
        }
        finalized_any
    }

    fn finalize_install_after_refresh(&mut self, _job_id: u64, payload: PendingInstallFinalize) {
        let PendingInstallFinalize {
            installed_paths,
            installed_candidate_labels,
            gb_profile,
            rel_paths,
            pending_meta,
            pending_unsafe,
        } = payload;
        let post_install_rename = pending_meta
            .as_ref()
            .and_then(|meta| {
                meta.update_target_mod_id
                    .as_ref()
                    .zip(meta.post_install_rename_to.as_ref())
                    .map(|(mod_id, name)| (mod_id.clone(), name.clone()))
            });
        for path in &installed_paths {
            self.pending_known_installed_paths.remove(path);
        }
        let mut first_mod_name = String::new();
        let mut primary_id = None;
        let mut newly_installed_ids = Vec::new();

        for (i, path) in installed_paths.iter().enumerate() {
            if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.root_path == *path) {
                if i == 0 {
                    first_mod_name = mod_entry.folder_name.clone();
                    primary_id = Some(mod_entry.id.clone());
                }
                if pending_unsafe {
                    mod_entry.unsafe_content = true;
                }
                if pending_meta.is_none() {
                    let _ = xxmi::save_mod_metadata(mod_entry);
                }
                newly_installed_ids.push(mod_entry.id.clone());
            }
        }

        for id in &newly_installed_ids {
            let candidate_labels = self
                .state
                .mods
                .iter()
                .find(|m| m.id == *id)
                .map(|m| {
                    installed_candidate_labels
                        .iter()
                        .filter(|(path, _)| path == &m.root_path)
                        .map(|(_, label)| label.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.apply_sync_metadata(
                id,
                pending_meta.clone(),
                gb_profile.clone(),
                rel_paths.clone(),
                candidate_labels,
            );
        }

        if let Some((target_mod_id, rename_to)) = post_install_rename {
            match self.rename_mod_folder(&target_mod_id, &rename_to) {
                Ok(()) => {
                    self.log_action("Renamed", &rename_to);
                    self.save_state();
                    if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == target_mod_id) {
                        first_mod_name = mod_entry.folder_name.clone();
                    }
                }
                Err(err) => {
                    self.report_warn(
                        format!("post-install rename failed: {err:#}"),
                        Some("Rename failed"),
                    );
                }
            }
        }

        if let (Some(profile), Some(first_path)) = (gb_profile.clone(), installed_paths.first()) {
            let mod_id = pending_meta
                .as_ref()
                .and_then(|meta| meta.update_target_mod_id.clone())
                .or_else(|| {
                    self.state
                        .mods
                        .iter()
                        .find(|m| m.root_path == *first_path)
                        .map(|m| m.id.clone())
                });
            if let Some(mod_entry_id) = mod_id {
                let image_job_id = self.next_background_job_id();
                let mod_root_path = self
                    .state
                    .mods
                    .iter()
                    .find(|m| m.id == mod_entry_id)
                    .map(|m| m.root_path.clone())
                    .unwrap_or_else(|| first_path.clone());
                let _ = self.install_request_tx.send(InstallRequest::SyncImages {
                    job_id: image_job_id,
                    mod_entry_id,
                    mod_root_path,
                    profile,
                });
            }
        }

        if let Some(id) = primary_id {
            match self.state.after_install_behavior {
                AfterInstallBehavior::DoNothing => {}
                AfterInstallBehavior::AddToSelection => {
                    self.selected_mods.insert(id.clone());
                }
                AfterInstallBehavior::OpenModDetail => {
                    self.set_selected_mod_id(Some(id.clone()));
                }
            }
            let count = installed_paths.len();
            if count > 1 {
                self.log_action("Installed", &format!("{count} mods from archive"));
                self.set_message_ok(format!("Installed {count} mods"));
            } else if !first_mod_name.is_empty() {
                self.log_action("Installed", &first_mod_name);
                self.set_message_ok(format!("Installed: {first_mod_name}"));
            }
        } else if let Some(first_path) = installed_paths.first() {
            let fallback_name = first_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("mod");
            self.log_action("Installed", fallback_name);
            self.set_message_ok(format!("Installed: {fallback_name}"));
        }
    }

    fn apply_sync_metadata(
        &mut self,
        mod_entry_id: &str,
        pending_meta: Option<PendingBrowseInstallMeta>,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
        rel_paths: Vec<String>,
        selected_candidate_labels: Vec<String>,
    ) {
        let Some(meta) = pending_meta else { return; };

        // Identify all mods sharing this ID to keep them in sync
        let target_indices: Vec<usize> = self.state.mods.iter().enumerate()
            .filter(|(_, m)| m.id == mod_entry_id && m.game_id == meta.game_id)
            .map(|(i, _)| i)
            .collect();

        for idx in target_indices {
            let mod_entry = &mut self.state.mods[idx];
            let now = Utc::now();
            let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
            source.gamebanana = Some(GameBananaLink {
                mod_id: meta.mod_id,
                url: gamebanana::browser_url(meta.mod_id),
            });
            source.file_set = FileSetRecipe {
                selected_file_ids: meta.selected_files.iter().map(|f| f.id).collect(),
                selected_file_names: meta
                    .selected_files
                    .iter()
                    .map(|f| f.file_name.clone())
                    .collect(),
                selected_files_meta: meta
                    .selected_files
                    .iter()
                    .map(tracked_file_meta_from_mod_file)
                    .collect(),
                selected_candidate_labels: selected_candidate_labels.clone(),
            };
            source.history.downloaded_at = Some(now);
            source.history.installed_at = Some(now);
            source.history.updated_at = Some(now);
            source.ignored_update_signature = None;
            source.baseline_content_mtime = mod_entry.content_mtime;
            source.baseline_ini_hash = mod_entry.ini_hash.clone();
            
            let profile_compare = if let Some(p) = gb_profile.as_ref() {
                (**p).clone()
            } else {
                profile_to_response(source.snapshot.as_ref())
            };
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or(profile_compare.date_updated.or(Some(profile_compare.date_modified)));
            let raw_state = determine_file_set_update_state(&source.file_set, local_sync_ts, &profile_compare);
            mod_entry.update_state =
                apply_ignored_update_override(source, raw_state, gb_profile.as_deref().or(Some(&profile_compare)));
            mod_entry.unsafe_content = gb_profile.as_ref().is_some_and(|p| !p.content_ratings.is_empty());

            if let Some(profile) = gb_profile.as_ref() {
                source.snapshot = Some(profile_to_snapshot(profile));
                source.raw_profile_json = serde_json::to_string(profile).ok();
                if !rel_paths.is_empty() {
                    mod_entry.metadata.user.screenshots = rel_paths.clone();
                    if mod_entry.metadata.user.cover_image.as_ref().map_or(true, |s| s.trim().is_empty()) {
                        mod_entry.metadata.user.cover_image = rel_paths.first().cloned();
                    }
                }
            }
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
    }

    fn backfill_missing_mod_images(&mut self, target_game_id: Option<&str>) {
        if let Some(id) = self.selected_mod_id.clone() {
            let needs_sync = self.state.mods.iter().find(|m| m.id == id).map_or(false, |m| {
                if let Some(game_id) = target_game_id {
                    if m.game_id != game_id {
                        return false;
                    }
                }
                m.source.as_ref().is_some_and(|s| s.gamebanana.is_some())
                    && m.metadata.user.screenshots.is_empty()
            });

            if needs_sync {
                self.enqueue_mod_image_sync(&id);
            }
        }
    }

    fn apply_mod_sync_result(
        &mut self,
        mod_entry_id: &str,
        profile: gamebanana::ProfileResponse,
        rel_paths: Vec<String>,
    ) {
        let target_indices: Vec<usize> = self.state.mods.iter().enumerate()
            .filter(|(_, m)| m.id == mod_entry_id)
            .map(|(i, _)| i)
            .collect();

        if target_indices.is_empty() { return; }

        let mut first_folder_name = String::new();
        for (i, idx) in target_indices.into_iter().enumerate() {
            let mod_entry = &mut self.state.mods[idx];
            if i == 0 { first_folder_name = mod_entry.folder_name.clone(); }

            if !rel_paths.is_empty() {
                mod_entry.metadata.user.screenshots = rel_paths.clone();
                if mod_entry
                    .metadata
                    .user
                    .cover_image
                    .as_deref()
                    .map(|s| s.trim().is_empty() || s.contains("gb_"))
                    .unwrap_or(true)
                {
                    mod_entry.metadata.user.cover_image = rel_paths.first().cloned();
                }
            }
            let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
            source.snapshot = Some(profile_to_snapshot(&profile));
            source.raw_profile_json = serde_json::to_string(&profile).ok();
            mod_entry.unsafe_content = !profile.content_ratings.is_empty();
            source.baseline_content_mtime = mod_entry.content_mtime;
            source.baseline_ini_hash = mod_entry.ini_hash.clone();
            let local_sync_ts = profile.date_updated.or(Some(profile.date_modified));
            mod_entry.update_state = determine_update_state(local_sync_ts, &profile);
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
        
        let folder_name = first_folder_name;

        self.save_state();

        let prefix = format!("my-mod-shot-{mod_entry_id}-");
        self.mod_cover_textures.retain(|key, _| key != mod_entry_id && !key.starts_with(&prefix));
        self.mod_full_textures.retain(|key, _| key != mod_entry_id && !key.starts_with(&prefix));
        self.pending_mod_image_requests
            .retain(|key| key != mod_entry_id && !key.starts_with(&prefix));
        self.rebuild_texture_tracking();
        self.log_action("Synced", &folder_name);
    }

    fn queue_update_apply(&mut self, mod_entry_id: &str) {
        let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_entry_id).cloned() else { return; };
        let Some(source) = &mod_entry.source else { return; };
        let Some(link) = &source.gamebanana else { return; };

        let mod_id = link.mod_id;
        let game_id = mod_entry.game_id.clone();
        if !self.game_is_installed_or_configured(&game_id) {
            self.report_warn(
                "Game is not installed or configured.",
                Some("Update unavailable"),
            );
            return;
        }
        let title = mod_entry.metadata.user.title.as_ref().unwrap_or(&mod_entry.folder_name).clone();

        let task_id = self.next_background_job_id();
        self.add_task(
            task_id,
            TaskKind::Download,
            TaskStatus::Queued,
            format!("Updating: {title}"),
            Some(game_id.clone()),
            None,
            mod_entry.unsafe_content,
        );

        self.request_browse_detail(mod_id);
        self.resolve_browse_install_after_detail(PendingBrowseInstall {
            task_id,
            mod_id,
            game_id,
            update_target_id: Some(mod_entry_id.to_string()),
        });
    }

    fn cancel_update_process_for_mod(&mut self, mod_entry: &ModEntry) {
        let title = mod_entry
            .metadata
            .user
            .title
            .as_ref()
            .unwrap_or(&mod_entry.folder_name)
            .clone();
        let task_title = format!("Updating: {title}");
        let task_ids: Vec<u64> = self
            .state
            .tasks
            .iter()
            .filter(|task| {
                task.title == task_title
                    && task.game_id.as_deref() == Some(mod_entry.game_id.as_str())
                    && matches!(
                        task.status,
                        TaskStatus::Queued
                            | TaskStatus::Downloading
                            | TaskStatus::Installing
                            | TaskStatus::Canceling
                    )
            })
            .map(|task| task.id)
            .collect();
        for task_id in task_ids {
            self.cancel_task(task_id);
        }
    }

}

#[cfg(test)]
mod update_signature_tests {
    use super::*;

    fn mod_file(id: u64, file_name: &str, date_added: i64) -> gamebanana::ModFile {
        gamebanana::ModFile {
            id,
            file_name: file_name.to_string(),
            file_size: 1,
            date_added,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some(format!("https://example.com/{file_name}")),
            is_archived: false,
        }
    }

    fn profile(files: Vec<gamebanana::ModFile>, update_ts: i64) -> gamebanana::ProfileResponse {
        gamebanana::ProfileResponse {
            id: 1,
            date_modified: update_ts,
            date_updated: Some(update_ts),
            files,
            ..Default::default()
        }
    }

    #[test]
    fn update_signature_uses_legacy_selected_file_ids() {
        let profile = profile(vec![mod_file(10, "old.zip", 100), mod_file(20, "new.zip", 200)], 200);
        let file_set = FileSetRecipe {
            selected_file_ids: vec![10],
            ..Default::default()
        };

        let signature = compute_update_signature(&file_set, &profile).unwrap();

        assert!(!signature.prearmed_next_update);
        assert_eq!(signature.profile_update_ts, None);
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 20);
    }

    #[test]
    fn update_signature_falls_back_to_profile_timestamp_for_update_available() {
        let profile = profile(Vec::new(), 200);
        let signature =
            current_update_signature_for_state(&FileSetRecipe::default(), &profile, ModUpdateState::UpdateAvailable)
                .unwrap();

        assert!(signature.files.is_empty());
        assert_eq!(signature.profile_update_ts, Some(200));
        assert!(!signature.prearmed_next_update);
    }

    #[test]
    fn update_signature_does_not_use_profile_timestamp_without_current_update() {
        let profile = profile(Vec::new(), 200);

        assert!(
            current_update_signature_for_state(&FileSetRecipe::default(), &profile, ModUpdateState::UpToDate)
                .is_none()
        );
    }

    #[test]
    fn prearmed_ignore_once_persists_while_up_to_date() {
        let current = mod_file(10, "current.zip", 100);
        let profile = profile(vec![current.clone()], 100);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&current)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set: file_set.clone(),
            ignored_update_signature: current_remote_signature(&file_set, &profile),
            ..Default::default()
        };

        let state = apply_ignored_update_override(&mut source, ModUpdateState::UpToDate, Some(&profile));

        assert_eq!(state, ModUpdateState::UpToDate);
        assert!(
            source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|signature| signature.prearmed_next_update)
        );
    }

    #[test]
    fn prearmed_ignore_once_converts_to_next_update_signature() {
        let current = mod_file(10, "current.zip", 100);
        let update = mod_file(20, "update.zip", 200);
        let current_profile = profile(vec![current.clone()], 100);
        let update_profile = profile(vec![current.clone(), update], 200);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&current)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set: file_set.clone(),
            ignored_update_signature: current_remote_signature(&file_set, &current_profile),
            ..Default::default()
        };

        let state =
            apply_ignored_update_override(&mut source, ModUpdateState::UpdateAvailable, Some(&update_profile));

        let signature = source.ignored_update_signature.as_ref().unwrap();
        assert_eq!(state, ModUpdateState::IgnoringUpdateOnce);
        assert!(!signature.prearmed_next_update);
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 20);
    }

    #[test]
    fn prearmed_ignore_once_converts_for_modified_local_update() {
        let current = mod_file(10, "current.zip", 100);
        let update = mod_file(20, "update.zip", 200);
        let current_profile = profile(vec![current.clone()], 100);
        let update_profile = profile(vec![current.clone(), update], 200);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&current)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set: file_set.clone(),
            ignored_update_signature: current_remote_signature(&file_set, &current_profile),
            ..Default::default()
        };

        let state =
            apply_ignored_update_override(&mut source, ModUpdateState::ModifiedLocally, Some(&update_profile));

        let signature = source.ignored_update_signature.as_ref().unwrap();
        assert_eq!(state, ModUpdateState::ModifiedLocally);
        assert!(!signature.prearmed_next_update);
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 20);
    }

    #[test]
    fn normal_ignore_once_clears_on_subsequent_update() {
        let installed = mod_file(10, "installed.zip", 100);
        let ignored = mod_file(20, "ignored.zip", 200);
        let newer = mod_file(30, "newer.zip", 300);
        let update_profile = profile(vec![installed.clone(), ignored.clone(), newer], 300);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&installed)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set,
            ignored_update_signature: Some(IgnoredUpdateSignature {
                files: vec![tracked_file_meta_from_mod_file(&ignored)],
                profile_update_ts: None,
                prearmed_next_update: false,
            }),
            ..Default::default()
        };

        let state =
            apply_ignored_update_override(&mut source, ModUpdateState::UpdateAvailable, Some(&update_profile));

        assert_eq!(state, ModUpdateState::UpdateAvailable);
        assert!(source.ignored_update_signature.is_none());
    }
}
