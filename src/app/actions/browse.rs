const BROWSE_DOWNLOAD_RETRY_INITIAL_DELAY: Duration = Duration::from_secs(1);
const BROWSE_DOWNLOAD_RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
const BROWSE_DOWNLOAD_MAX_ATTEMPTS: usize = 3;
const BROWSE_PROFILE_FRESH_TTL: Duration = Duration::from_secs(15 * 60);
const BROWSE_PROFILE_RETRY_COOLDOWN: Duration = Duration::from_secs(30);

fn browse_profile_is_fresh(cached_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    cached_at
        .and_then(|cached_at| (now - cached_at).to_std().ok())
        .is_some_and(|age| age < BROWSE_PROFILE_FRESH_TTL)
}

fn browse_detail_attempt_is_recent(attempted_at: Option<Instant>, now: Instant) -> bool {
    attempted_at
        .and_then(|attempted_at| now.checked_duration_since(attempted_at))
        .is_some_and(|age| age < BROWSE_PROFILE_RETRY_COOLDOWN)
}

fn thalia_merge_recommendation_id(
    recommendation: Option<&ThaliaRecommendation>,
    review_files: &[gamebanana::ModFile],
) -> Option<u64> {
    let recommendation = recommendation?;
    (recommendation.action == ThaliaRecommendationAction::Merge)
        .then_some(recommendation.id?)
        .filter(|id| review_files.iter().any(|file| file.id == *id))
}

fn browse_selected_files_for_queued_download(
    file: &gamebanana::ModFile,
    selected_files: &[gamebanana::ModFile],
    update_target_mod_id: Option<&str>,
) -> Vec<gamebanana::ModFile> {
    if update_target_mod_id.is_some() {
        selected_files.to_vec()
    } else {
        vec![file.clone()]
    }
}

fn browse_exact_source_matches(
    file_set: &FileSetRecipe,
    selectable: &[gamebanana::ModFile],
) -> Vec<gamebanana::ModFile> {
    let mut matches = Vec::new();
    let mut add_match = |candidate: &gamebanana::ModFile| {
        if !matches
            .iter()
            .any(|matched: &gamebanana::ModFile| matched.id == candidate.id)
        {
            matches.push(candidate.clone());
        }
    };

    for tracked in &file_set.selected_files_meta {
        if let Some(found) = selectable
            .iter()
            .find(|file| remote_file_matches_tracked(file, tracked))
        {
            add_match(found);
        }
    }
    for old_id in &file_set.selected_file_ids {
        if let Some(found) = selectable.iter().find(|file| file.id == *old_id) {
            add_match(found);
        }
    }
    for old_name in &file_set.selected_file_names {
        if let Some(found) = selectable.iter().find(|file| file.file_name == *old_name) {
            add_match(found);
        }
    }
    matches
}

fn browse_review_file_is_installed(file: &gamebanana::ModFile, file_set: &FileSetRecipe) -> bool {
    if file.id != 0
        && (file_set.selected_file_ids.contains(&file.id)
            || file_set
                .selected_files_meta
                .iter()
                .any(|installed| installed.file_id != 0 && installed.file_id == file.id))
    {
        return true;
    }
    let has_authoritative_ids = file_set.selected_file_ids.iter().any(|id| *id != 0)
        || file_set
            .selected_files_meta
            .iter()
            .any(|installed| installed.file_id != 0);
    !has_authoritative_ids
        && (!file.file_name.is_empty()
            && (file_set.selected_file_names.contains(&file.file_name)
                || file_set
                    .selected_files_meta
                    .iter()
                    .any(|installed| {
                        installed.file_id == 0 && installed.file_name == file.file_name
                    })))
}

fn browse_card_indices_for_display(
    cards: &[BrowseCard],
    unsafe_content_mode: UnsafeContentMode,
) -> Vec<usize> {
    cards
        .iter()
        .enumerate()
        .filter_map(|(index, card)| {
            let hidden = matches!(
                unsafe_content_mode,
                UnsafeContentMode::HideNoCounter | UnsafeContentMode::HideShowCounter
            ) && card.unsafe_content;
            (!hidden).then_some(index)
        })
        .collect()
}

trait BrowseInstalledModSource {
    fn len(&self) -> usize;
    fn gamebanana_entry(&self, index: usize) -> Option<(&str, u64)>;
}

impl BrowseInstalledModSource for [ModEntry] {
    fn len(&self) -> usize {
        <[ModEntry]>::len(self)
    }

    fn gamebanana_entry(&self, index: usize) -> Option<(&str, u64)> {
        let mod_entry = self.get(index)?;
        let link = mod_entry
            .source
            .as_ref()
            .and_then(|source| source.gamebanana.as_ref())?;
        Some((mod_entry.game_id.as_str(), link.mod_id))
    }
}

#[cfg(test)]
impl BrowseInstalledModSource for [(String, u64)] {
    fn len(&self) -> usize {
        <[(String, u64)]>::len(self)
    }

    fn gamebanana_entry(&self, index: usize) -> Option<(&str, u64)> {
        let (game_id, mod_id) = self.get(index)?;
        Some((game_id.as_str(), *mod_id))
    }
}

#[cfg(test)]
impl<'a> BrowseInstalledModSource for [(&'a str, u64)] {
    fn len(&self) -> usize {
        <[(&'a str, u64)]>::len(self)
    }

    fn gamebanana_entry(&self, index: usize) -> Option<(&str, u64)> {
        self.get(index).copied()
    }
}

#[derive(Default)]
struct BrowseInstalledModSnapshot {
    seen_by_game: HashMap<String, HashSet<u64>>,
    scanned_until: usize,
}

impl BrowseInstalledModSnapshot {
    fn lookup<S: BrowseInstalledModSource + ?Sized>(
        &mut self,
        card: &BrowseCard,
        source: &S,
    ) -> bool {
        if self
            .seen_by_game
            .get(&card.game_id)
            .is_some_and(|mod_ids| mod_ids.contains(&card.id))
        {
            return true;
        }

        while self.scanned_until < source.len() {
            let index = self.scanned_until;
            self.scanned_until += 1;
            let Some((game_id, mod_id)) = source.gamebanana_entry(index) else {
                continue;
            };

            if let Some(mod_ids) = self.seen_by_game.get_mut(game_id) {
                mod_ids.insert(mod_id);
            } else {
                self.seen_by_game
                    .insert(game_id.to_string(), HashSet::from([mod_id]));
            }

            if game_id == card.game_id && mod_id == card.id {
                return true;
            }
        }
        false
    }
}

fn browse_detail_profile(detail: &BrowseDetailCache) -> &gamebanana::ProfileResponse {
    detail
        .translated_profile
        .as_ref()
        .unwrap_or(&detail.profile)
}

impl HestiaApp {
    fn is_imported_mod_placeholder_name(&self, sanitized: &str) -> bool {
        AppLanguage::ALL
            .iter()
            .map(|language| sanitize_folder_name(TextCatalog::new(*language).imported_mod()))
            .any(|placeholder| sanitized == placeholder)
    }

    fn sanitized_preferred_browse_title_name(&self, raw_title: Option<&str>) -> Option<String> {
        let title = raw_title?.trim();
        if title.is_empty() {
            return None;
        }
        let sanitized = sanitize_folder_name(title);
        if self.is_imported_mod_placeholder_name(&sanitized) || sanitized.chars().all(|c| c == '_')
        {
            None
        } else {
            Some(sanitized)
        }
    }

    fn preferred_browse_folder_name(&self, raw_title: Option<&str>, fallback_name: &str) -> String {
        self.sanitized_preferred_browse_title_name(raw_title)
            .unwrap_or_else(|| sanitize_folder_name(fallback_name))
    }

    fn browse_mod_title_for_install(
        &self,
        mod_id: u64,
        update_target_mod_id: Option<&str>,
    ) -> Option<String> {
        if let Some(target_id) = update_target_mod_id {
            if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == target_id) {
                if let Some(title) = mod_entry.metadata.user.title.as_deref() {
                    if !title.trim().is_empty() {
                        return Some(title.to_string());
                    }
                }
            }
        }
        self.browse_state
            .details
            .get(&mod_id)
            .map(|detail| detail.profile.name.clone())
            .or_else(|| {
                self.browse_state
                    .cards
                    .iter()
                    .find(|card| card.id == mod_id)
                    .map(|card| card.name.clone())
            })
    }

    fn reset_browse_for_game(&mut self, game_id: &str) {
        self.browse_state.active_game_id = Some(game_id.to_string());
        self.browse_state.active_query = None;
        self.browse_state.active_character_category_id = None;
        self.browse_state.cards.clear();
        self.browse_state.total_count = None;
        self.browse_state.next_page = 1;
        self.browse_state.has_more = true;
        self.browse_state.loading_page = false;
        self.browse_state.page_request_nonce = None;
        self.browse_state.page_request_started_at = None;
        self.browse_state.page_error = None;
        self.browse_state.refresh_page_cache_for_session = false;
        self.browse_state.selected_mod_id = None;
        self.browse_state.details.clear();
        self.browse_state.detail_request_attempts.clear();
        self.browse_state.character_categories_game_id = None;
        self.browse_state.character_categories.clear();
        self.browse_state.character_categories_loading = false;
        self.browse_state.selected_character_category = None;
        self.browse_detail_open = false;
        self.browse_state.file_prompt = None;
        self.browse_state.pending_file_review = None;
        self.cancel_browse_full_image_requests();
    }

    fn browse_image_cache_key(url: &str) -> String {
        format!("img:{}", hash64_hex(url.as_bytes()))
    }

    fn browse_download_cache_key(mod_id: u64, file_name: &str, file_size: u64) -> String {
        let safe_name = sanitize_folder_name(file_name);
        format!("dl:{mod_id}:{safe_name}:{file_size}")
    }

    fn browse_download_task_file_from_mod_file(
        file: &gamebanana::ModFile,
    ) -> BrowseDownloadTaskFile {
        BrowseDownloadTaskFile {
            id: file.id,
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            date_added: file.date_added,
            download_count: file.download_count,
            description: file.description.clone(),
            version: file.version.clone(),
            download_url: file.download_url.clone(),
            is_archived: file.is_archived,
            md5: file.md5.clone(),
        }
    }

    fn browse_download_mod_file_from_task_file(
        file: &BrowseDownloadTaskFile,
    ) -> gamebanana::ModFile {
        gamebanana::ModFile {
            id: file.id,
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            date_added: file.date_added,
            download_count: file.download_count,
            description: file.description.clone(),
            version: file.version.clone(),
            download_url: file.download_url.clone(),
            is_archived: file.is_archived,
            md5: file.md5.clone(),
        }
    }

    fn browse_download_retry_payload(
        &self,
        game_id: String,
        mod_id: u64,
        file: &gamebanana::ModFile,
        selected_files: &[gamebanana::ModFile],
        unsafe_content: bool,
        update_folder_name: Option<String>,
        update_target_mod_id: Option<String>,
        install_state: ModInstallState,
        preserve_existing_state: bool,
        post_install_rename_to: Option<String>,
    ) -> BrowseDownloadTaskPayload {
        self.browse_download_retry_payload_with_resolution(
            game_id,
            mod_id,
            file,
            selected_files,
            unsafe_content,
            update_folder_name,
            update_target_mod_id,
            install_state,
            preserve_existing_state,
            post_install_rename_to,
            None,
        )
    }

    fn browse_download_retry_payload_with_resolution(
        &self,
        game_id: String,
        mod_id: u64,
        file: &gamebanana::ModFile,
        selected_files: &[gamebanana::ModFile],
        unsafe_content: bool,
        update_folder_name: Option<String>,
        update_target_mod_id: Option<String>,
        install_state: ModInstallState,
        preserve_existing_state: bool,
        post_install_rename_to: Option<String>,
        explicit_resolution: Option<ImportResolution>,
    ) -> BrowseDownloadTaskPayload {
        let selected_files = if selected_files.is_empty() {
            vec![Self::browse_download_task_file_from_mod_file(file)]
        } else {
            selected_files
                .iter()
                .map(Self::browse_download_task_file_from_mod_file)
                .collect()
        };
        BrowseDownloadTaskPayload {
            game_id,
            mod_id,
            file: Self::browse_download_task_file_from_mod_file(file),
            selected_files,
            unsafe_content,
            update_folder_name,
            update_target_mod_id,
            install_state,
            install_disabled: None,
            preserve_existing_state,
            post_install_rename_to,
            profile_json: self
                .browse_state
                .details
                .get(&mod_id)
                .and_then(|detail| serde_json::to_string(&detail.profile).ok()),
            profile_cached_at: self
                .browse_state
                .details
                .get(&mod_id)
                .and_then(|detail| detail.profile_cached_at),
            is_tool: self
                .browse_state
                .details
                .get(&mod_id)
                .is_some_and(|detail| detail.is_tool),
            explicit_resolution,
            thalia_bundle: false,
        }
    }

    fn browse_download_job_from_retry_payload(
        task_id: u64,
        payload: &BrowseDownloadTaskPayload,
        cache_limit_bytes: u64,
    ) -> BrowseDownloadJob {
        BrowseDownloadJob {
            task_id,
            title: payload.file.file_name.clone(),
            url: payload.file.download_url.clone().unwrap_or_default(),
            cache_key: Self::browse_download_cache_key(
                payload.mod_id,
                &payload.file.file_name,
                payload.file.file_size,
            ),
            file_name: payload.file.file_name.clone(),
            cache_limit_bytes,
            total_size: Some(payload.file.file_size),
            md5: payload.file.md5.clone(),
        }
    }

    fn pending_browse_install_meta_from_retry_payload(
        &self,
        payload: &BrowseDownloadTaskPayload,
    ) -> PendingBrowseInstallMeta {
        let update_target_was_disabled = payload
            .update_target_mod_id
            .as_deref()
            .and_then(|target_id| self.state.mods.iter().find(|m| m.id == target_id))
            .is_some_and(|mod_entry| mod_entry.status == ModStatus::Disabled);
        PendingBrowseInstallMeta {
            mod_id: payload.mod_id,
            game_id: payload.game_id.clone(),
            selected_files: payload
                .selected_files
                .iter()
                .map(Self::browse_download_mod_file_from_task_file)
                .collect(),
            update_folder_name: payload.update_folder_name.clone(),
            update_target_mod_id: payload.update_target_mod_id.clone(),
            update_target_was_disabled,
            install_state: payload.effective_install_state(),
            preserve_existing_state: payload.preserve_existing_state,
            post_install_rename_to: payload.post_install_rename_to.clone(),
            profile_cached_at: payload.profile_cached_at,
            is_tool: payload.is_tool,
            explicit_resolution: payload.explicit_resolution,
        }
    }

    fn retry_profile_for_task(&self, task_id: u64) -> Option<Box<gamebanana::ProfileResponse>> {
        self.state
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| match task.retry_payload.as_ref()? {
                TaskRetryPayload::BrowseDownload(payload) => payload.profile_json.as_deref(),
            })
            .and_then(|raw| serde_json::from_str::<gamebanana::ProfileResponse>(raw).ok())
            .map(Box::new)
    }

    fn browse_download_retry_payload_for_task(
        task: &TaskEntry,
    ) -> Option<&BrowseDownloadTaskPayload> {
        match task.retry_payload.as_ref()? {
            TaskRetryPayload::BrowseDownload(payload) => Some(payload),
        }
    }

    fn browse_download_partial_path_for_payload(payload: &BrowseDownloadTaskPayload) -> PathBuf {
        let cache_key = Self::browse_download_cache_key(
            payload.mod_id,
            &payload.file.file_name,
            payload.file.file_size,
        );
        persistence::runtime_temp_downloads_partial_dir()
            .join(format!("{}.partial", hash64_hex(cache_key.as_bytes())))
    }

    fn task_has_resumable_browse_download(&self, task: &TaskEntry) -> bool {
        let Some(payload) = Self::browse_download_retry_payload_for_task(task) else {
            return false;
        };
        Self::browse_download_partial_path_for_payload(payload)
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0)
    }

    fn task_is_retryable_browse_download(task: &TaskEntry) -> bool {
        matches!(task.status, TaskStatus::Failed | TaskStatus::Canceled)
            && Self::browse_download_retry_payload_for_task(task)
                .is_some_and(|payload| !payload.thalia_bundle)
    }

    fn retry_browse_download_task(&mut self, task_id: u64) -> bool {
        self.requeue_browse_download_task(task_id, false)
    }

    fn requeue_browse_download_task(&mut self, task_id: u64, in_bundle: bool) -> bool {
        let Some(task) = self.state.tasks.iter().find(|task| task.id == task_id) else {
            return false;
        };
        let retryable = matches!(task.status, TaskStatus::Failed | TaskStatus::Canceled)
            && Self::browse_download_retry_payload_for_task(task)
                .is_some_and(|payload| !payload.thalia_bundle || in_bundle);
        if !retryable {
            return false;
        }
        let Some(payload) = Self::browse_download_retry_payload_for_task(task).cloned() else {
            return false;
        };
        if self
            .browse_download_queue
            .iter()
            .any(|job| job.task_id == task_id)
            || self.browse_download_inflight.contains_key(&task_id)
            || self.install_queue.iter().any(|job| job.id == task_id)
            || self.install_inflight.contains_key(&task_id)
        {
            return true;
        }
        if !self.game_can_download_mods(&payload.game_id) {
            self.report_warn(
                self.game_mod_setup_message(&payload.game_id),
                Some(self.text().install_unavailable()),
            );
            return true;
        }

        self.pending_browse_install_safety
            .insert(task_id, payload.unsafe_content);
        self.pending_browse_install_meta.insert(
            task_id,
            self.pending_browse_install_meta_from_retry_payload(&payload),
        );
        self.browse_download_queue
            .push_back(Self::browse_download_job_from_retry_payload(
                task_id,
                &payload,
                self.cache_limit_bytes.load(Ordering::Relaxed),
            ));
        if let Some(task) = self.state.tasks.iter_mut().find(|task| task.id == task_id) {
            task.kind = TaskKind::Download;
        }
        self.update_task_status(task_id, TaskStatus::Queued);
        self.set_message_ok(self.text().download_queued());
        true
    }

    fn ensure_browse_bootstrap(&mut self) {
        if self.current_view != ViewMode::Browse || !self.has_enabled_games() {
            return;
        }
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        let current_game_id = game.definition.id.clone();
        if self.browse_state.active_game_id.as_deref() != Some(current_game_id.as_str()) {
            self.reset_browse_for_game(&current_game_id);
        }
        if self.browse_state.loading_page || self.browse_state.page_error.is_some() {
            return;
        }
        if self.browse_state.cards.is_empty() {
            self.request_browse_page_with_mode(1, true);
        } else if !self.browse_state.refresh_page_cache_for_session {
            self.restart_browse_query();
        }
    }

    fn current_browse_query(&self) -> Option<String> {
        let trimmed = self.browse_query.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn request_browse_page(&mut self, page: usize) {
        self.request_browse_page_with_mode(page, self.browse_state.refresh_page_cache_for_session);
    }

    fn request_browse_page_with_mode(&mut self, page: usize, force_refresh: bool) {
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        let Some(_gamebanana_id) = gamebanana::game_id_for_hestia(&game.definition.id) else {
            return;
        };
        if page == 1 && force_refresh {
            self.browse_state.refresh_page_cache_for_session = true;
            self.browse_page_generation = self.browse_page_generation.wrapping_add(1);
            self.browse_state.page_error = None;
        }
        let query = self.current_browse_query();
        let character_category_id = self
            .browse_state
            .selected_character_category
            .as_ref()
            .map(|category| category.id);
        if page == 1 {
            self.browse_state.active_game_id = Some(game.definition.id.clone());
            self.browse_state.active_query = query.clone();
            self.browse_state.active_character_category_id = character_category_id;
        }
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        self.browse_state.page_request_nonce = Some(self.browse_request_nonce);
        self.browse_state.loading_page = true;
        self.browse_state.page_request_started_at = Some(Instant::now());
        let request = BrowseRequest::FetchPage {
            nonce: self.browse_request_nonce,
            generation: self.browse_page_generation,
            game_id: game.definition.id.clone(),
            query,
            character_category_id,
            page,
            browse_sort: self.state.static_prefs.browse_sort,
            search_sort: self.state.static_prefs.search_sort,
            force_refresh,
        };
        if self.browse_request_tx.send(request).is_err() {
            self.browse_state.loading_page = false;
            self.browse_state.page_request_nonce = None;
            self.browse_state.page_request_started_at = None;
            self.browse_state.has_more = false;
            self.browse_state.page_error = Some("browse worker unavailable".to_string());
        }
    }

    fn restart_browse_query(&mut self) {
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        self.browse_state.active_game_id = Some(game.definition.id);
        self.browse_state.refresh_page_cache_for_session = true;
        self.browse_state.active_query = self.current_browse_query();
        self.browse_state.active_character_category_id = self
            .browse_state
            .selected_character_category
            .as_ref()
            .map(|category| category.id);
        self.browse_state.cards.clear();
        self.browse_state.total_count = None;
        self.browse_state.next_page = 1;
        self.browse_state.has_more = true;
        self.browse_state.loading_page = false;
        self.browse_state.page_request_nonce = None;
        self.browse_state.page_request_started_at = None;
        self.browse_state.page_error = None;
        self.browse_state.selected_mod_id = None;
        self.browse_state.details.clear();
        self.browse_state.detail_request_attempts.clear();
        self.browse_detail_open = false;
        self.browse_state.file_prompt = None;
        self.browse_state.pending_file_review = None;
        self.cancel_browse_full_image_requests();
        self.clear_translation_caches();
        self.request_browse_page_with_mode(1, true);
    }

    fn enforce_browse_page_timeout(&mut self) {
        const BROWSE_PAGE_HARD_TIMEOUT: Duration = Duration::from_secs(7);

        let Some(started_at) = self.browse_state.page_request_started_at else {
            return;
        };
        if !self.browse_state.loading_page || started_at.elapsed() < BROWSE_PAGE_HARD_TIMEOUT {
            return;
        }

        self.browse_page_generation = self.browse_page_generation.wrapping_add(1);
        self.browse_state.loading_page = false;
        self.browse_state.page_request_nonce = None;
        self.browse_state.page_request_started_at = None;
        self.browse_state.has_more = false;
        let error = self.text().connection_timed_out().to_string();
        self.browse_state.page_error = Some(error.clone());
        let _ = self.browse_request_tx.send(BrowseRequest::CancelPage);
        self.report_error_message(
            self.text().browse_page_failed_message(&error),
            Some(self.text().browse_failed()),
        );
    }

    fn enforce_browse_request_timeouts(&mut self) {
        const BROWSE_REQUEST_HARD_TIMEOUT: Duration = Duration::from_secs(90);

        let timed_out: Vec<u64> = self
            .browse_state
            .loading_details
            .iter()
            .filter(|(_, started_at)| started_at.elapsed() >= BROWSE_REQUEST_HARD_TIMEOUT)
            .map(|(mod_id, _)| *mod_id)
            .collect();
        for mod_id in timed_out {
            self.browse_state.loading_details.remove(&mod_id);
            self.browse_detail_request_nonces.remove(&mod_id);
            let mut i = 0;
            while i < self.browse_state.pending_installs.len() {
                if self.browse_state.pending_installs[i].mod_id == mod_id {
                    let pending = self.browse_state.pending_installs.remove(i);
                    self.update_task_status(pending.task_id, TaskStatus::Failed);
                } else {
                    i += 1;
                }
            }
            if self
                .browse_state
                .pending_file_review
                .as_ref()
                .is_some_and(|pending| pending.gb_id == mod_id)
            {
                self.browse_state.pending_file_review = None;
                self.report_error_message(
                    self.text().browse_detail_failed_message(mod_id, "request timed out"),
                    Some(self.text().browse_detail_failed()),
                );
            }
            self.log_warn(format!(
                "detail request timed out without a worker response; dropping it: mod {mod_id}"
            ));
        }

        if self.browse_state.character_categories_loading
            && self
                .browse_state
                .character_categories_started_at
                .is_some_and(|started_at| started_at.elapsed() >= BROWSE_REQUEST_HARD_TIMEOUT)
        {
            self.browse_state.character_categories_loading = false;
            self.log_warn(
                "character category request timed out without a worker response; dropping it"
                    .to_string(),
            );
        }
    }

    fn enforce_browse_image_timeouts(&mut self) {
        const BROWSE_IMAGE_HARD_TIMEOUT: Duration = Duration::from_secs(120);

        let timed_out: Vec<String> = self
            .browse_image_inflight
            .iter()
            .filter(|(_, inflight)| inflight.started_at.elapsed() >= BROWSE_IMAGE_HARD_TIMEOUT)
            .map(|(texture_key, _)| texture_key.clone())
            .collect();
        for texture_key in timed_out {
            if let Some(inflight) = self.browse_image_inflight.remove(&texture_key) {
                inflight.cancel.store(true, Ordering::Relaxed);
                self.browse_image_retry_after.insert(
                    texture_key.clone(),
                    Instant::now() + Duration::from_secs(BROWSE_IMAGE_RETRY_COOLDOWN_SECS),
                );
                self.log_warn(format!(
                    "image request stalled without a worker response; dropping it: {}",
                    inflight.request.url
                ));
            }
        }
    }

    fn request_browse_character_categories(&mut self, force_refresh: bool) {
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        let Some(super_category_id) =
            gamebanana::character_super_category_id_for_hestia(&game.definition.id)
        else {
            self.report_warn(
                self.text().no_configured_character_category_list(),
                Some(self.text().characters_unavailable()),
            );
            return;
        };
        if self.browse_state.character_categories_loading {
            return;
        }
        if !force_refresh
            && self.browse_state.character_categories_game_id.as_deref()
                == Some(game.definition.id.as_str())
            && !self.browse_state.character_categories.is_empty()
        {
            return;
        }
        self.browse_state.character_categories_game_id = Some(game.definition.id.clone());
        self.browse_state.character_categories_loading = true;
        self.browse_state.character_categories_started_at = Some(Instant::now());
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        if self
            .browse_request_tx
            .send(BrowseRequest::FetchCharacterCategories {
                nonce: self.browse_request_nonce,
                game_id: game.definition.id,
                super_category_id,
                force_refresh,
            })
            .is_err()
        {
            self.browse_state.character_categories_loading = false;
        }
    }

    fn select_browse_character_category(&mut self, category: BrowseCharacterCategory) {
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        self.browse_query.clear();
        self.browse_state.active_game_id = Some(game.definition.id);
        self.browse_state.active_query = None;
        self.browse_state.active_character_category_id = Some(category.id);
        self.browse_state.selected_character_category = Some(category);
        self.browse_state.cards.clear();
        self.browse_state.total_count = None;
        self.browse_state.next_page = 1;
        self.browse_state.has_more = true;
        self.browse_state.loading_page = false;
        self.browse_state.page_request_started_at = None;
        self.browse_state.selected_mod_id = None;
        self.browse_detail_open = false;
        self.browse_state.file_prompt = None;
        self.browse_state.pending_file_review = None;
        self.browse_state.refresh_page_cache_for_session = true;
        self.cancel_browse_full_image_requests();
        self.request_browse_page_with_mode(1, true);
    }

    fn clear_browse_character_category(&mut self) {
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        self.browse_state.active_game_id = Some(game.definition.id);
        self.browse_state.active_query = None;
        self.browse_state.active_character_category_id = None;
        self.browse_state.selected_character_category = None;
        self.browse_state.cards.clear();
        self.browse_state.total_count = None;
        self.browse_state.next_page = 1;
        self.browse_state.has_more = true;
        self.browse_state.loading_page = false;
        self.browse_state.page_request_started_at = None;
        self.browse_state.selected_mod_id = None;
        self.browse_detail_open = false;
        self.browse_state.file_prompt = None;
        self.browse_state.pending_file_review = None;
        self.browse_state.refresh_page_cache_for_session = true;
        self.cancel_browse_full_image_requests();
        self.request_browse_page_with_mode(1, true);
    }

    fn request_browse_detail(&mut self, mod_id: u64) {
        self.request_browse_detail_with_mode(mod_id, false);
    }

    fn request_browse_detail_revalidate(&mut self, mod_id: u64) {
        let now = Utc::now();
        if self.browse_state.details.get(&mod_id).is_some_and(|detail| {
            !detail.is_tool && browse_profile_is_fresh(detail.profile_cached_at, now)
        }) {
            return;
        }
        let has_usable_detail = self.browse_state.details.get(&mod_id).is_some_and(|detail| {
            !detail.is_tool && detail.profile.id == mod_id
        });
        if has_usable_detail && self.detail_request_attempt_is_recent(mod_id) {
            return;
        }
        self.request_browse_detail_with_mode_internal(mod_id, true, true);
    }

    fn detail_request_attempt_is_recent(&mut self, mod_id: u64) -> bool {
        let now = Instant::now();
        self.browse_state
            .detail_request_attempts
            .retain(|_, attempted_at| !now
                .checked_duration_since(*attempted_at)
                .is_some_and(|age| age >= BROWSE_PROFILE_RETRY_COOLDOWN));
        let is_recent = self
            .browse_state
            .detail_request_attempts
            .get(&mod_id)
            .copied()
            .is_some_and(|attempted_at| browse_detail_attempt_is_recent(Some(attempted_at), now));
        if self.browse_state.detail_request_attempts.len() > 256 {
            if let Some(oldest_id) = self
                .browse_state
                .detail_request_attempts
                .iter()
                .min_by_key(|(_, attempted_at)| *attempted_at)
                .map(|(id, _)| *id)
            {
                self.browse_state.detail_request_attempts.remove(&oldest_id);
            }
        }
        is_recent
    }

    fn request_browse_detail_with_mode(&mut self, mod_id: u64, force_refresh: bool) {
        self.request_browse_detail_with_mode_internal(mod_id, force_refresh, false);
    }

    fn request_browse_detail_with_mode_internal(
        &mut self,
        mod_id: u64,
        force_refresh: bool,
        preserve_existing_detail: bool,
    ) {
        if force_refresh {
            if !preserve_existing_detail {
                self.browse_state.details.remove(&mod_id);
                self.browse_state.loading_details.remove(&mod_id);
            } else if self.browse_state.loading_details.contains_key(&mod_id) {
                return;
            }
        } else if self.browse_state.details.contains_key(&mod_id)
            || self.browse_state.loading_details.contains_key(&mod_id)
        {
            return;
        }
        if !force_refresh && self.detail_request_attempt_is_recent(mod_id) {
            return;
        }
        if force_refresh || self.browse_state.refresh_page_cache_for_session {
            self.browse_state
                .detail_request_attempts
                .insert(mod_id, Instant::now());
        }
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        self.browse_detail_request_nonces
            .insert(mod_id, self.browse_request_nonce);
        self.browse_state
            .loading_details
            .insert(mod_id, Instant::now());
        let update_source = self.browse_state.pending_installs.iter().find_map(|pending| {
            if pending.mod_id != mod_id {
                return None;
            }
            let target = pending.update_target_id.as_deref()?;
            self.state.mods.iter().find(|entry| entry.id == target)?.source.as_ref()
        }).or_else(|| {
            let pending = self.browse_state.pending_file_review.as_ref()?;
            if pending.gb_id != mod_id {
                return None;
            }
            self.state
                .mods
                .iter()
                .find(|entry| entry.id == pending.mod_entry_id)
                .and_then(|entry| entry.source.as_ref())
        });
        let is_tool = self
            .browse_state
            .pending_file_review
            .as_ref()
            .filter(|pending| pending.gb_id == mod_id)
            .map(|pending| pending.is_tool)
            .or_else(|| {
                update_source
                    .and_then(|source| source.gamebanana.as_ref())
                    .map(|link| gamebanana::is_tool_url(&link.url))
            })
            .unwrap_or(false);
        let include_update_history = update_source.is_some() && !is_tool;
        let existing_profile_json = preserve_existing_detail.then(|| {
            self.browse_state
                .details
                .get(&mod_id)
                .filter(|detail| detail.is_tool == is_tool)
                .and_then(|detail| serde_json::to_string(&detail.profile).ok())
        }).flatten();
        let cached_profile_json =
            if let Some(source) = update_source {
                source_profile_for_compare(source)
                    .and_then(|profile| serde_json::to_string(&profile).ok())
            } else if force_refresh {
                existing_profile_json.or_else(|| {
                    self.state.mods.iter().find_map(|mod_entry| {
                        let source = mod_entry.source.as_ref()?;
                        let link = source.gamebanana.as_ref()?;
                        (link.mod_id == mod_id && gamebanana::is_tool_url(&link.url) == is_tool)
                            .then(|| source.raw_profile_json.clone())
                            .flatten()
                    })
                })
            } else if self.browse_state.refresh_page_cache_for_session {
                None
            } else {
                self.state.mods.iter().find_map(|mod_entry| {
                    let source = mod_entry.source.as_ref()?;
                    let link = source.gamebanana.as_ref()?;
                    (link.mod_id == mod_id)
                        .then(|| source.raw_profile_json.clone())
                        .flatten()
                })
            };
        if self
            .browse_request_tx
            .send(BrowseRequest::FetchDetail {
                nonce: self.browse_request_nonce,
                mod_id,
                is_tool,
                force_refresh: force_refresh || self.browse_state.refresh_page_cache_for_session,
                include_update_history,
                cached_profile_json,
            })
            .is_err()
        {
            self.browse_detail_request_nonces.remove(&mod_id);
            self.browse_state.loading_details.remove(&mod_id);
            let mut i = 0;
            while i < self.browse_state.pending_installs.len() {
                if self.browse_state.pending_installs[i].mod_id == mod_id {
                    let pending = self.browse_state.pending_installs.remove(i);
                    self.update_task_status(pending.task_id, TaskStatus::Failed);
                } else {
                    i += 1;
                }
            }
            if self
                .browse_state
                .pending_file_review
                .as_ref()
                .is_some_and(|pending| pending.gb_id == mod_id)
            {
                self.browse_state.pending_file_review = None;
                self.report_error_message(
                    self.text().browse_detail_failed_message(mod_id, "request unavailable"),
                    Some(self.text().browse_detail_failed()),
                );
            }
        }
    }

    fn request_browse_detail_refresh(&mut self, mod_id: u64) {
        self.request_browse_detail_with_mode(mod_id, true);
    }

    fn request_browse_updates(&mut self, mod_id: u64) {
        let Some(detail) = self.browse_state.details.get(&mod_id) else {
            return;
        };
        if !matches!(detail.updates, BrowseUpdatesState::Unrequested) {
            return;
        }
        let is_tool = detail.is_tool;
        if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
            Arc::make_mut(detail).updates = BrowseUpdatesState::Loading;
        }
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        if self
            .browse_request_tx
            .send(BrowseRequest::FetchUpdates {
                nonce: self.browse_request_nonce,
                mod_id,
                is_tool,
                force_refresh: self.browse_state.refresh_page_cache_for_session,
            })
            .is_err()
        {
            if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
                Arc::make_mut(detail).updates =
                    BrowseUpdatesState::Failed("browse worker unavailable".to_string());
            }
        }
    }

    fn updates_to_entries(
        &self,
        updates: gamebanana::ApiEnvelope<gamebanana::UpdateRecord>,
        mod_id: u64,
    ) -> Vec<BrowseUpdateEntry> {
        updates
            .records
            .into_iter()
            .filter(|record| !record.is_private && !record.is_trashed && !record.is_withheld)
            .map(|record| {
                let update_time = if record.date_modified > 0 {
                    record.date_modified
                } else {
                    record.date_added
                };
                BrowseUpdateEntry {
                    name: record.name.clone(),
                    version: record
                        .version
                        .as_deref()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(|v| v.to_string()),
                    updated_at: timestamp_to_utc(update_time),
                    markdown: prepare_markdown_for_display(
                        record.html_text.as_deref().unwrap_or_default(),
                        None,
                        Some(mod_id),
                        &self.portable,
                    ),
                }
            })
            .collect()
    }

    fn ensure_my_mod_updates(&mut self, mod_id: u64) {
        if mod_id == 0 {
            return;
        }
        let now = Utc::now();
        let should_fetch = match self.my_mod_updates.get(&mod_id) {
            None => true,
            Some(MyModUpdatesState::Loading) => false,
            Some(MyModUpdatesState::Ready { fetched_at, .. })
            | Some(MyModUpdatesState::Empty { fetched_at }) => {
                now.signed_duration_since(*fetched_at) >= chrono::Duration::minutes(30)
            }
            Some(MyModUpdatesState::Failed { attempted_at, .. }) => {
                now.signed_duration_since(*attempted_at) >= chrono::Duration::seconds(60)
            }
        };
        if !should_fetch {
            return;
        }
        if !self.my_mod_updates_inflight.insert(mod_id) {
            return;
        }
        if !matches!(
            self.my_mod_updates.get(&mod_id),
            Some(MyModUpdatesState::Ready { .. }) | Some(MyModUpdatesState::Empty { .. })
        ) {
            self.my_mod_updates
                .insert(mod_id, MyModUpdatesState::Loading);
        }
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        if self
            .browse_request_tx
            .send(BrowseRequest::FetchMyModUpdates {
                nonce: self.browse_request_nonce,
                mod_id,
            })
            .is_err()
        {
            self.my_mod_updates_inflight.remove(&mod_id);
            let message = self.text().could_not_load_updates().to_string();
            self.my_mod_updates.insert(
                mod_id,
                MyModUpdatesState::Failed {
                    message,
                    attempted_at: now,
                },
            );
        }
    }

    fn reset_browse_detail_render_state(&mut self) {
        self.browse_commonmark_cache = CommonMarkCache::default();
        self.gif_rewritten_markdown_cache.clear();
        self.render_safe_markdown_cache.clear();
        self.gif_dest_by_texture_key.clear();
        self.cancel_all_gif_work();
    }

    fn prune_browse_detail_cache(&mut self) {
        if self.browse_state.details.len() <= BROWSE_DETAIL_CACHE_LIMIT {
            return;
        }

        let selected = self.browse_state.selected_mod_id;
        let loading = self.browse_state.loading_details.clone();
        let pending_installs: HashSet<u64> = self
            .browse_state
            .pending_installs
            .iter()
            .map(|pending| pending.mod_id)
            .collect();
        let pending_file_review = self
            .browse_state
            .pending_file_review
            .as_ref()
            .map(|pending| pending.gb_id);
        let mut removable = self
            .browse_state
            .details
            .keys()
            .copied()
            .filter(|mod_id| Some(*mod_id) != selected)
            .filter(|mod_id| !loading.contains_key(mod_id))
            .filter(|mod_id| !pending_installs.contains(mod_id))
            .filter(|mod_id| Some(*mod_id) != pending_file_review)
            .collect::<Vec<_>>();
        while self.browse_state.details.len() > BROWSE_DETAIL_CACHE_LIMIT {
            let Some(mod_id) = removable.pop() else {
                break;
            };
            self.browse_state.details.remove(&mod_id);
        }
    }

    fn open_browse_detail(&mut self, mod_id: u64) {
        let switching_detail = self.browse_state.selected_mod_id != Some(mod_id);
        if switching_detail {
            self.reset_browse_detail_render_state();
        }
        self.browse_state.selected_mod_id = Some(mod_id);
        self.browse_detail_open = true;
        self.browse_detail_focus_requested = true;
        self.request_browse_detail_revalidate(mod_id);
        self.request_browse_updates(mod_id);
        if self.browse_state.details.contains_key(&mod_id) {
            self.maybe_translate_gamebanana_mod_details(mod_id);
        }
        self.queue_detail_preview_images(mod_id);
        self.begin_full_image_prefetch(mod_id);
        self.prune_browse_detail_cache();
    }

    fn open_linked_mod_in_browse(&mut self, mod_id: u64) {
        self.current_view = ViewMode::Browse;
        self.clear_mod_detail_rename();
        self.pending_browse_open_mod_id = Some(mod_id);
    }

    fn open_mod_file_review(&mut self, mod_entry_id: &str) {
        let Some(mod_entry) = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == mod_entry_id)
            .cloned()
        else {
            return;
        };
        let Some(source) = mod_entry.source.as_ref() else {
            return;
        };
        let Some(link) = source.gamebanana.as_ref() else {
            return;
        };
        if link.mod_id == 0 || mod_entry.game_id.is_empty() {
            return;
        }

        self.browse_state.file_prompt = None;
        self.browse_state.pending_file_review = None;

        if let Some(evidence) = self.cached_grouped_evidence_for_mod(&mod_entry) {
            if let Some(profile) = evidence.profile {
                self.seed_browse_detail_profile(
                    link.mod_id,
                    (*profile).clone(),
                    evidence.profile_cached_at,
                    gamebanana::is_tool_url(&link.url),
                );
                self.resolve_mod_file_review(PendingModFileReview {
                    mod_entry_id: mod_entry.id,
                    game_id: mod_entry.game_id,
                    gb_id: link.mod_id,
                    is_tool: gamebanana::is_tool_url(&link.url),
                    source_url: link.url.clone(),
                });
                return;
            }
        }

        let is_tool = gamebanana::is_tool_url(&link.url);
        let detail_kind_matches = self
            .browse_state
            .details
            .get(&link.mod_id)
            .is_some_and(|detail| detail.is_tool == is_tool);
        if detail_kind_matches {
            self.resolve_mod_file_review(PendingModFileReview {
                mod_entry_id: mod_entry.id,
                game_id: mod_entry.game_id,
                gb_id: link.mod_id,
                is_tool,
                source_url: link.url.clone(),
            });
        } else {
            self.browse_state.pending_file_review = Some(PendingModFileReview {
                mod_entry_id: mod_entry.id,
                game_id: mod_entry.game_id,
                gb_id: link.mod_id,
                is_tool,
                source_url: link.url.clone(),
            });
            if is_tool
                || self.browse_state.details.contains_key(&link.mod_id)
                || self.browse_state.loading_details.contains_key(&link.mod_id)
            {
                self.request_browse_detail_refresh(link.mod_id);
            } else {
                self.request_browse_detail(link.mod_id);
            }
        }
    }

    fn seed_browse_detail_profile(
        &mut self,
        mod_id: u64,
        profile: gamebanana::ProfileResponse,
        profile_cached_at: Option<DateTime<Utc>>,
        is_tool: bool,
    ) {
        let markdown = prepare_markdown_for_display(
            profile.html_text.as_deref().unwrap_or_default(),
            None,
            Some(mod_id),
            &self.portable,
        );
        self.browse_state.details.insert(
            mod_id,
            Arc::new(BrowseDetailCache {
                unsafe_content: !profile.content_ratings.is_empty(),
                profile,
                is_tool,
                profile_cached_at,
                markdown,
                updates: BrowseUpdatesState::Unrequested,
                translated_profile: None,
                translation_lang: None,
                translation_loading: false,
            }),
        );
    }

    fn resolve_mod_file_review(&mut self, pending: PendingModFileReview) {
        let Some(target) = self
            .state
            .mods
            .iter()
            .find(|entry| entry.id == pending.mod_entry_id)
            .cloned()
        else {
            self.browse_state.pending_file_review = None;
            return;
        };
        let Some(source) = target.source.as_ref() else {
            self.browse_state.pending_file_review = None;
            return;
        };
        let source_matches = source
            .gamebanana
            .as_ref()
            .is_some_and(|link| {
                link.mod_id == pending.gb_id
                    && link.url == pending.source_url
                    && gamebanana::is_tool_url(&link.url) == pending.is_tool
            });
        if target.game_id != pending.game_id || !source_matches {
            self.browse_state.pending_file_review = None;
            self.report_warn(
                self.text().file_review_target_changed(),
                Some(self.text().update_unavailable()),
            );
            return;
        }
        let Some(detail) = self.browse_state.details.get(&pending.gb_id).cloned() else {
            let gb_id = pending.gb_id;
            self.browse_state.pending_file_review = Some(pending);
            self.request_browse_detail(gb_id);
            return;
        };
        if detail.is_tool != pending.is_tool {
            self.browse_state.pending_file_review = None;
            self.report_warn(
                self.text().file_review_target_changed(),
                Some(self.text().update_unavailable()),
            );
            return;
        }

        let canonical_profile = matcher_adapter::canonicalize_profile(&detail.profile).profile;
        let evidence = self.cached_grouped_evidence_for_mod(&target);
        let other_mods_files: HashSet<u64> = evidence
            .iter()
            .flat_map(|evidence| evidence.other_mods_files.iter().copied())
            .collect();
        let mut other_mod_titles: Vec<(u64, String)> = Vec::new();
        for peer in self
            .state
            .mods
            .iter()
            .filter(|peer| peer.id != target.id && peer.game_id == target.game_id)
        {
            let Some(peer_source) = peer.source.as_ref() else {
                continue;
            };
            let same_page = peer_source.gamebanana.as_ref().is_some_and(|link| {
                link.mod_id == pending.gb_id
                    && gamebanana::is_tool_url(&link.url) == pending.is_tool
            });
            if !same_page {
                continue;
            }
            let title = peer
                .metadata
                .user
                .title
                .as_ref()
                .unwrap_or(&peer.folder_name);
            for id in installed_file_ids(&peer_source.file_set) {
                if !other_mod_titles.iter().any(|(known, _)| *known == id) {
                    other_mod_titles.push((id, title.clone()));
                }
            }
        }
        let factual_newer_files = available_newer_files(&source.file_set, &canonical_profile);
        let newer_file_ids: HashSet<u64> = factual_newer_files
            .iter()
            .map(|file| file.id)
            .filter(|id| {
                !other_mods_files.contains(id)
                    && !other_mod_titles.iter().any(|(known, _)| known == id)
            })
            .collect();
        let mut review_files: Vec<_> = canonical_profile
            .files
            .iter()
            .filter(|file| {
                file.id != 0
                    && !browse_review_file_is_installed(file, &source.file_set)
                    && profile_file_is_active_downloadable(&canonical_profile, file)
            })
            .cloned()
            .collect();
        review_files.sort_by(|left, right| {
            newer_file_ids
                .contains(&right.id)
                .cmp(&newer_file_ids.contains(&left.id))
                .then_with(|| right.date_added.cmp(&left.date_added))
                .then_with(|| right.id.cmp(&left.id))
        });

        let recommendation =
            self.thalia_recommendation_for_profile(&pending.mod_entry_id, &canonical_profile);
        let recommended_merge_id =
            thalia_merge_recommendation_id(recommendation.as_ref(), &review_files);
        let recommended_action = recommended_merge_id.map(|_| FileReviewAction::InstallMerge);

        let dismissible_file_ids = evidence
            .as_ref()
            .filter(|evidence| {
                Self::has_newer_files_notice_from_evidence(
                    &target,
                    evidence,
                    self.state.static_prefs.update_check_statuses,
                )
            })
            .map(|evidence| {
                let ignored_once: HashSet<u64> = source
                    .ignored_update_signature
                    .as_ref()
                    .map(|signature| {
                        signature
                            .files
                            .iter()
                            .filter_map(|file| (file.file_id != 0).then_some(file.file_id))
                            .collect()
                    })
                    .unwrap_or_default();
                evidence
                    .newer_files
                    .iter()
                    .filter_map(|file| {
                        (file.id != 0 && !ignored_once.contains(&file.id)).then_some(file.id)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let installed_files = evidence
            .map(|evidence| evidence.installed_files)
            .filter(|files| !files.is_empty())
            .unwrap_or_else(|| source.file_set.selected_files_meta.clone());
        self.browse_state.pending_file_review = None;
        self.browse_state.file_prompt = Some(BrowseFilePrompt {
            mod_id: pending.gb_id,
            game_id: target.game_id,
            files: review_files
                .into_iter()
                .map(|file| {
                    let selected = recommended_merge_id == Some(file.id);
                    BrowseSelectableFile { file, selected }
                })
                .collect(),
            update_folder_name: Some(target.folder_name.clone()),
            update_target_mod_id: Some(target.id.clone()),
            post_install_rename_to: None,
            install_state: ModInstallState::Auto,
            preserve_existing_state: true,
            file_review: Some(FileReviewContext {
                recommended_action,
                installed_files,
                newer_file_ids: newer_file_ids.into_iter().collect(),
                other_mod_titles,
                dismissible_file_ids,
                expected_file_set: source.file_set.clone(),
                target_root_path: target.root_path,
                expected_source_url: pending.source_url,
                expected_is_tool: pending.is_tool,
            }),
        });
        if !pending.is_tool {
            self.maybe_translate_gamebanana_mod_details(pending.gb_id);
        }
    }

    fn process_pending_browse_open(&mut self, ctx: &egui::Context) {
        let Some(mod_id) = self.pending_browse_open_mod_id.take() else {
            return;
        };
        if self.current_view != ViewMode::Browse {
            self.pending_browse_open_mod_id = Some(mod_id);
            return;
        }
        self.open_browse_detail(mod_id);
        ctx.request_repaint();
    }

    fn queue_detail_preview_images(&mut self, mod_id: u64) {
        let Some(detail) = self.browse_state.details.get(&mod_id) else {
            return;
        };
        let urls: Vec<String> = detail
            .profile
            .preview_media
            .as_ref()
            .map(|preview| {
                preview
                    .images
                    .iter()
                    .filter_map(gamebanana::thumbnail_url)
                    .collect()
            })
            .unwrap_or_default();
        for url in urls {
            self.queue_browse_image_with_profile(url, None, false, ThumbnailProfile::Rail, 30);
        }
    }

    fn begin_full_image_prefetch(&mut self, mod_id: u64) {
        self.cancel_browse_full_image_requests();
        self.browse_detail_generation = self.browse_detail_generation.wrapping_add(1);

        let urls: Vec<String> = self
            .browse_state
            .details
            .get(&mod_id)
            .and_then(|detail| detail.profile.preview_media.as_ref())
            .map(|preview| {
                preview
                    .images
                    .iter()
                    .take(3)
                    .map(gamebanana::full_image_url)
                    .collect()
            })
            .unwrap_or_default();

        for (idx, full_url) in urls.into_iter().enumerate() {
            self.queue_browse_image_full(
                full_url,
                Some(self.browse_detail_generation),
                5 + (idx as u32),
            );
        }
    }

    fn cancel_browse_full_image_requests(&mut self) {
        self.browse_image_queue
            .retain(|job| job.cancel_key.is_none());
        for inflight in self.browse_image_inflight.values() {
            if inflight.cancel_key.is_some() {
                inflight.cancel.store(true, Ordering::Relaxed);
            }
        }
        self.browse_image_inflight
            .retain(|_, inflight| inflight.cancel_key.is_none());
        self.browse_state.screenshot_overlay = None;
    }

    fn queue_browse_image(
        &mut self,
        url: String,
        cancel_key: Option<u64>,
        skip_texture: bool,
        priority: u32,
    ) {
        self.queue_browse_image_with_profile(
            url,
            cancel_key,
            skip_texture,
            ThumbnailProfile::Card,
            priority,
        );
    }

    fn browse_thumb_texture_key(url: &str, profile: ThumbnailProfile) -> String {
        format!("{}:{}", profile.suffix(), hash64_hex(url.as_bytes()))
    }

    fn queue_browse_image_with_profile(
        &mut self,
        url: String,
        cancel_key: Option<u64>,
        skip_texture: bool,
        thumb_profile: ThumbnailProfile,
        priority: u32,
    ) {
        let hash_key = hash64_hex(url.as_bytes());
        let thumb_texture_key = Self::browse_thumb_texture_key(&url, thumb_profile);
        let cache_key = Self::browse_image_cache_key(&url);
        let now = Instant::now();

        if let Some(retry_after) = self.browse_image_retry_after.get(&hash_key).copied() {
            if retry_after > now {
                return;
            }
            self.browse_image_retry_after.remove(&hash_key);
        }

        if !skip_texture && self.browse_thumb_textures.contains_key(&thumb_texture_key) {
            return;
        }

        if skip_texture && persistence::cache_exists(&self.portable, &cache_key).unwrap_or(false) {
            return;
        }

        if let Some(inflight) = self.browse_image_inflight.get(&hash_key)
            && (skip_texture || (!inflight.skip_texture && !inflight.load_full))
        {
            return;
        }

        if let Some(existing) = self
            .browse_image_queue
            .iter_mut()
            .find(|j| j.texture_key == hash_key)
        {
            if !skip_texture && existing.skip_texture {
                existing.skip_texture = false;
            }
            existing.priority = existing.priority.min(priority);
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.browse_image_queue.push(BrowseImageRequest {
            texture_key: hash_key,
            thumb_texture_key,
            url,
            cache_key,
            cancel_key,
            cancel,
            skip_texture,
            load_full: false,
            priority,
            thumb_profile,
        });
    }

    fn queue_browse_image_full(&mut self, url: String, cancel_key: Option<u64>, priority: u32) {
        let texture_key = hash64_hex(url.as_bytes());
        let thumb_texture_key = Self::browse_thumb_texture_key(&url, ThumbnailProfile::Rail);
        let now = Instant::now();
        if let Some(retry_after) = self.browse_image_retry_after.get(&texture_key).copied() {
            if retry_after > now {
                return;
            }
            self.browse_image_retry_after.remove(&texture_key);
        }
        if self.browse_image_textures.contains_key(&texture_key) {
            return;
        }
        if let Some(inflight) = self.browse_image_inflight.get(&texture_key) {
            if inflight.load_full {
                return;
            }
        }
        if self
            .browse_image_queue
            .iter()
            .any(|job| job.texture_key == texture_key && job.load_full)
        {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.browse_image_queue.push(BrowseImageRequest {
            texture_key,
            thumb_texture_key,
            cache_key: Self::browse_image_cache_key(&url),
            url,
            cancel_key,
            cancel,
            skip_texture: false,
            load_full: true,
            priority,
            thumb_profile: ThumbnailProfile::Rail,
        });
    }

    fn queue_overlay_full_texture(&mut self, texture_key: &str) {
        if self.mod_full_textures.contains_key(texture_key) {
            self.touch_texture(TextureKind::ModFull, texture_key, 3);
            return;
        }
        if self.browse_image_textures.contains_key(texture_key) {
            self.touch_texture(TextureKind::BrowseFull, texture_key, 3);
            return;
        }

        let mut local_load = None;
        if let Some(mod_id) = self.selected_mod_id.clone() {
            if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_id) {
                let prefix = format!("my-mod-shot-{}-", mod_entry.id);
                if texture_key.starts_with(&prefix) {
                    if let Some(rel) = mod_entry.metadata.user.screenshots.iter().find(|rel| {
                        Self::my_mod_screenshot_texture_key(&mod_entry.id, rel) == texture_key
                    }) {
                        local_load = Some((texture_key.to_string(), mod_entry.root_path.join(rel)));
                    }
                }
            }
        }
        if let Some((key, path)) = local_load {
            self.queue_mod_image_full_load(key, path, 1);
            return;
        }

        let mut remote_url = None;
        if let Some(mod_id) = self.browse_state.selected_mod_id {
            if let Some(detail) = self.browse_state.details.get(&mod_id) {
                if let Some(preview) = &detail.profile.preview_media {
                    remote_url = preview
                        .images
                        .iter()
                        .map(gamebanana::full_image_url)
                        .find(|url| hash64_hex(url.as_bytes()) == texture_key);
                }
            }
        }
        if let Some(full_url) = remote_url {
            self.queue_browse_image_full(full_url, Some(self.browse_detail_generation), 1);
            return;
        }

        if let Some(url) = self.my_mod_overlay_images.iter().find_map(|item| {
            (item.texture_key == texture_key)
                .then(|| item.url.as_ref())
                .flatten()
        }) {
            self.queue_browse_image_full(url.clone(), Some(self.browse_detail_generation), 1);
        }
    }

    fn browse_card_indices_for_display(&self) -> Vec<usize> {
        browse_card_indices_for_display(
            &self.browse_state.cards,
            self.state.static_prefs.unsafe_content_mode,
        )
    }

    fn is_browse_mod_installed(&self, card: &BrowseCard) -> bool {
        self.state.mods.iter().any(|mod_entry| {
            mod_entry.game_id == card.game_id
                && mod_entry
                    .source
                    .as_ref()
                    .and_then(|source| source.gamebanana.as_ref())
                    .is_some_and(|link| link.mod_id == card.id)
        })
    }

    fn hidden_unsafe_browse_count(&self) -> usize {
        if self.state.static_prefs.unsafe_content_mode != UnsafeContentMode::HideShowCounter {
            return 0;
        }
        self.browse_state
            .cards
            .iter()
            .filter(|card| card.unsafe_content)
            .count()
    }

    fn should_censor_unsafe(&self) -> bool {
        self.state.static_prefs.unsafe_content_mode == UnsafeContentMode::Censor
    }

    fn browse_thumbnail_parallelism(&self) -> usize {
        THUMB_IMAGE_LIMIT
    }

    fn browse_download_parallelism(&self) -> usize {
        FULL_IMAGE_LIMIT
    }

    fn consume_browse_events(&mut self) {
        while let Ok(event) = self.browse_event_rx.try_recv() {
            match event {
                BrowseEvent::PageLoaded {
                    _nonce: nonce,
                    generation,
                    game_id,
                    query,
                    character_category_id,
                    page,
                    payload,
                } => {
                    if generation != self.browse_page_generation {
                        continue;
                    }
                    if self.browse_state.page_request_nonce != Some(nonce) {
                        continue;
                    }
                    self.browse_state.loading_page = false;
                    self.browse_state.page_request_started_at = None;
                    if page == 1 {
                        self.browse_state.page_error = None;
                    }
                    self.browse_state.active_query = query.clone();
                    self.browse_state.active_character_category_id = character_category_id;
                    self.browse_state.total_count = Some(payload.metadata.record_count);
                    self.browse_state.has_more =
                        !payload.metadata.is_complete && !payload.records.is_empty();
                    self.browse_state.next_page = page + 1;
                    if page == 1 {
                        self.browse_state.cards.clear();
                    }
                    for record in payload.records {
                        if self
                            .browse_state
                            .cards
                            .iter()
                            .any(|item| item.id == record.id)
                        {
                            continue;
                        }
                        let thumbnail_url = gamebanana::record_thumbnail_image(&record).map(
                            |image| {
                                gamebanana::thumbnail_url(image)
                                    .unwrap_or_else(|| gamebanana::full_image_url(image))
                            });
                        if let Some(url) = thumbnail_url.clone() {
                            self.queue_browse_image_with_profile(
                                url,
                                None,
                                false,
                                ThumbnailProfile::Card,
                                50,
                            );
                        }
                        self.browse_state.cards.push(BrowseCard {
                            id: record.id,
                            game_id: game_id.clone(),
                            name: record.name,
                            author_name: record.submitter.name,
                            like_count: record.like_count,
                            download_count: self
                                .browse_state
                                .details
                                .get(&record.id)
                                .map(|detail| detail.profile.download_count),
                            updated_at: timestamp_to_utc(
                                record.date_updated.unwrap_or(record.date_modified),
                            ),
                            thumbnail_url,
                            has_files: record.has_files,
                            unsafe_content: record.has_content_ratings,
                        });
                    }
                }
                BrowseEvent::PageWarning {
                    _nonce: nonce,
                    generation,
                    warning,
                    ..
                } => {
                    if generation != self.browse_page_generation {
                        continue;
                    }
                    if self.browse_state.page_request_nonce != Some(nonce) {
                        continue;
                    }
                    self.report_warn(
                        self.text().browse_page_warning(&warning),
                        Some(self.text().connection_failed()),
                    );
                }
                BrowseEvent::PageFailed {
                    _nonce: nonce,
                    generation,
                    page,
                    error,
                    ..
                } => {
                    if generation != self.browse_page_generation {
                        continue;
                    }
                    if self.browse_state.page_request_nonce != Some(nonce) {
                        continue;
                    }
                    self.browse_state.loading_page = false;
                    self.browse_state.page_request_nonce = None;
                    self.browse_state.page_request_started_at = None;
                    if page > 1 {
                        self.browse_state.has_more = false;
                        self.log_warn(format!(
                            "browse page {page} failed; stopped loading more results: {error}"
                        ));
                        continue;
                    }
                    self.browse_state.page_error = Some(error.clone());
                    self.browse_state.has_more = false;
                    self.log_warn(format!(
                        "browse page {page} refresh failed; waiting for manual retry: {error}"
                    ));
                    self.report_error_message(
                        self.text().browse_page_failed_message(&error),
                        Some(self.text().browse_failed()),
                    );
                }
                BrowseEvent::CharacterCategoriesLoaded {
                    game_id,
                    categories,
                    ..
                } => {
                    if self.browse_state.active_game_id.as_deref() != Some(game_id.as_str()) {
                        continue;
                    }
                    self.browse_state.character_categories_loading = false;
                    self.browse_state.character_categories_game_id = Some(game_id);
                    self.browse_state.character_categories = categories
                        .into_iter()
                        .filter(|category| !category.is_obsolete)
                        .map(|category| BrowseCharacterCategory {
                            id: category.id,
                            name: category.name,
                            item_count: category.item_count,
                            icon_url: category.icon_url.filter(|url| !url.trim().is_empty()),
                        })
                        .collect();
                }
                BrowseEvent::CharacterCategoriesWarning {
                    game_id, warning, ..
                } => {
                    if self.browse_state.active_game_id.as_deref() != Some(game_id.as_str()) {
                        continue;
                    }
                    self.report_warn(
                        self.text().character_categories_warning(&warning),
                        Some(self.text().connection_failed()),
                    );
                }
                BrowseEvent::CharacterCategoriesFailed { game_id, error, .. } => {
                    if self.browse_state.active_game_id.as_deref() != Some(game_id.as_str()) {
                        continue;
                    }
                    self.browse_state.character_categories_loading = false;
                    self.report_error_message(
                        self.text().character_categories_failed_message(&error),
                        Some(self.text().characters_failed()),
                    );
                }
                BrowseEvent::DetailLoaded {
                    _nonce,
                    mod_id,
                    is_tool,
                    profile,
                    profile_cached_at,
                } => {
                    if self.browse_detail_request_nonces.get(&mod_id) != Some(&_nonce) {
                        continue;
                    }
                    self.browse_detail_request_nonces.remove(&mod_id);
                    self.browse_state.loading_details.remove(&mod_id);
                    let markdown = prepare_markdown_for_display(
                        profile.html_text.as_deref().unwrap_or_default(),
                        None,
                        Some(mod_id),
                        &self.portable,
                    );
                    self.prewarm_markdown_images(&markdown);

                    let mut description_image_keys = HashSet::with_capacity(16);
                    for cap in MARKDOWN_IMAGE_URL_RE.captures_iter(&markdown) {
                        if let Some(url_match) = cap.get(2) {
                            let url = url_match.as_str();
                            if url.starts_with("http") && !is_gif_dest(url) {
                                description_image_keys.insert(hash64_hex(url.as_bytes()));
                                description_image_keys.insert(Self::browse_thumb_texture_key(
                                    url,
                                    ThumbnailProfile::Card,
                                ));
                                self.queue_browse_image(
                                    url.to_string(),
                                    Some(self.browse_detail_generation),
                                    true,
                                    100,
                                );
                            }
                        }
                    }

                    let cache = BrowseDetailCache {
                        profile: profile.clone(),
                        is_tool,
                        profile_cached_at,
                        markdown,
                        unsafe_content: !profile.content_ratings.is_empty(),
                        updates: BrowseUpdatesState::Unrequested,
                        translated_profile: None,
                        translation_lang: None,
                        translation_loading: false,
                    };
                    self.browse_state.details.insert(mod_id, Arc::new(cache));
                    self.prune_browse_detail_cache();
                    if let Some(card) = self
                        .browse_state
                        .cards
                        .iter_mut()
                        .find(|card| card.id == mod_id)
                    {
                        card.download_count = Some(profile.download_count);
                        card.unsafe_content = !profile.content_ratings.is_empty();
                    }
                    self.queue_detail_preview_images(mod_id);
                    if self.browse_state.selected_mod_id == Some(mod_id) {
                        self.begin_full_image_prefetch(mod_id);
                        if self.browse_detail_open {
                            self.maybe_translate_gamebanana_mod_details(mod_id);
                        }
                    }
                    self.request_browse_updates(mod_id);

                    let mut i = 0;
                    while i < self.browse_state.pending_installs.len() {
                        if self.browse_state.pending_installs[i].mod_id == mod_id {
                            let pending = self.browse_state.pending_installs.remove(i);
                            self.resolve_browse_install_after_detail(pending);
                        } else {
                            i += 1;
                        }
                    }
                    if self
                        .browse_state
                        .pending_file_review
                        .as_ref()
                        .is_some_and(|pending| pending.gb_id == mod_id)
                    {
                        if let Some(pending) = self.browse_state.pending_file_review.take() {
                            self.resolve_mod_file_review(pending);
                        }
                    }
                }
                BrowseEvent::DetailWarning {
                    _nonce,
                    mod_id,
                    warning,
                } => {
                    if self.browse_detail_request_nonces.get(&mod_id) != Some(&_nonce) {
                        continue;
                    }
                    if self.browse_state.selected_mod_id == Some(mod_id) {
                        self.report_warn(
                            self.text().browse_detail_warning(mod_id, &warning),
                            Some(self.text().connection_failed()),
                        );
                    } else {
                        self.report_warn(self.text().browse_detail_warning(mod_id, &warning), None);
                    }
                }
                BrowseEvent::DetailFailed {
                    _nonce,
                    mod_id,
                    error,
                } => {
                    if self.browse_detail_request_nonces.get(&mod_id) != Some(&_nonce) {
                        continue;
                    }
                    self.browse_detail_request_nonces.remove(&mod_id);
                    self.browse_state.loading_details.remove(&mod_id);
                    let mut i = 0;
                    while i < self.browse_state.pending_installs.len() {
                        if self.browse_state.pending_installs[i].mod_id == mod_id {
                            let pending = self.browse_state.pending_installs.remove(i);
                            self.update_task_status(pending.task_id, TaskStatus::Failed);
                        } else {
                            i += 1;
                        }
                    }
                    if self
                        .browse_state
                        .pending_file_review
                        .as_ref()
                        .is_some_and(|pending| pending.gb_id == mod_id)
                    {
                        self.browse_state.pending_file_review = None;
                    }
                    self.report_error_message(
                        self.text().browse_detail_failed_message(mod_id, &error),
                        Some(self.text().browse_detail_failed()),
                    );
                }
                BrowseEvent::UpdatesLoaded {
                    mod_id, updates, ..
                } => {
                    let entries = self.updates_to_entries(updates, mod_id);
                    if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
                        let detail = Arc::make_mut(detail);
                        detail.updates = if entries.is_empty() {
                            BrowseUpdatesState::Empty
                        } else {
                            BrowseUpdatesState::Loaded(entries)
                        };
                    }
                }
                BrowseEvent::MyModUpdatesLoaded {
                    mod_id, updates, ..
                } => {
                    self.my_mod_updates_inflight.remove(&mod_id);
                    let entries = self.updates_to_entries(updates, mod_id);
                    let state = if entries.is_empty() {
                        MyModUpdatesState::Empty {
                            fetched_at: Utc::now(),
                        }
                    } else {
                        MyModUpdatesState::Ready {
                            entries,
                            fetched_at: Utc::now(),
                        }
                    };
                    self.my_mod_updates.insert(mod_id, state);
                }
                BrowseEvent::UpdatesWarning {
                    mod_id, warning, ..
                } => {
                    self.report_warn(self.text().browse_updates_warning(mod_id, &warning), None);
                }
                BrowseEvent::UpdatesFailed { mod_id, error, .. } => {
                    let failed_label = self.text().could_not_load_updates().to_string();
                    if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
                        let detail = Arc::make_mut(detail);
                        detail.updates = BrowseUpdatesState::Failed(failed_label);
                    }
                    self.report_error_message(
                        self.text().browse_updates_failed_message(mod_id, &error),
                        None,
                    );
                }
                BrowseEvent::MyModUpdatesFailed { mod_id, error, .. } => {
                    self.my_mod_updates_inflight.remove(&mod_id);
                    match self.my_mod_updates.remove(&mod_id) {
                        Some(MyModUpdatesState::Ready { entries, .. }) => {
                            self.my_mod_updates.insert(
                                mod_id,
                                MyModUpdatesState::Ready {
                                    entries,
                                    fetched_at: Utc::now(),
                                },
                            );
                        }
                        Some(MyModUpdatesState::Empty { .. }) => {
                            self.my_mod_updates.insert(
                                mod_id,
                                MyModUpdatesState::Empty {
                                    fetched_at: Utc::now(),
                                },
                            );
                        }
                        _ => {
                            let message = self.text().could_not_load_updates().to_string();
                            self.my_mod_updates.insert(
                                mod_id,
                                MyModUpdatesState::Failed {
                                    message,
                                    attempted_at: Utc::now(),
                                },
                            );
                        }
                    }
                    self.log_warn(format!(
                        "MY MODS updates fetch failed for mod {mod_id}: {error}"
                    ));
                }
            }
        }
    }

    fn process_browse_image_queue(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.browse_image_retry_after
            .retain(|_, retry_after| *retry_after > now);
        let mut allowed_keys = HashSet::with_capacity(32);
        let mut focus_mode = false;
        let pointer_motion_throttle = Self::pointer_motion_image_throttle_active(ctx);

        if let Some(overlay) = &self.browse_state.screenshot_overlay {
            focus_mode = true;
            allowed_keys.insert(overlay.texture_key.clone());

            if let Some(mod_id) = self.browse_state.selected_mod_id {
                if let Some(detail) = self.browse_state.details.get(&mod_id) {
                    if let Some(preview) = &detail.profile.preview_media {
                        let images = &preview.images;
                        if let Some(idx) = images.iter().position(|img| {
                            hash64_hex(gamebanana::full_image_url(img).as_bytes())
                                == overlay.texture_key
                        }) {
                            if idx + 1 < images.len() {
                                allowed_keys.insert(hash64_hex(
                                    gamebanana::full_image_url(&images[idx + 1]).as_bytes(),
                                ));
                            }
                            if idx > 0 {
                                allowed_keys.insert(hash64_hex(
                                    gamebanana::full_image_url(&images[idx - 1]).as_bytes(),
                                ));
                            }
                        }
                    }
                }
            }
            if let Some(mod_id) = &self.selected_mod_id {
                let prefix = format!("my-mod-shot-{mod_id}-");
                if overlay.texture_key.starts_with(&prefix) {
                    if let Some(mod_entry) = self.state.mods.iter().find(|m| &m.id == mod_id) {
                        if let Some(idx) =
                            mod_entry.metadata.user.screenshots.iter().position(|rel| {
                                Self::my_mod_screenshot_texture_key(&mod_entry.id, rel)
                                    == overlay.texture_key
                            })
                        {
                            if idx + 1 < mod_entry.metadata.user.screenshots.len() {
                                if let Some(rel) = mod_entry.metadata.user.screenshots.get(idx + 1)
                                {
                                    allowed_keys.insert(Self::my_mod_screenshot_texture_key(
                                        &mod_entry.id,
                                        rel,
                                    ));
                                }
                            }
                            if idx > 0 {
                                if let Some(rel) = mod_entry.metadata.user.screenshots.get(idx - 1)
                                {
                                    allowed_keys.insert(Self::my_mod_screenshot_texture_key(
                                        &mod_entry.id,
                                        rel,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        self.browse_image_queue.sort_by_key(|j| {
            let class = if j.skip_texture {
                2_u8
            } else if j.load_full {
                1
            } else {
                0
            };
            (class, j.priority)
        });
        let max_parallel = self.browse_thumbnail_parallelism();
        let mut i = 0;
        let mut deferred_for_pointer_motion = false;
        while self.browse_image_inflight.len() < max_parallel && i < self.browse_image_queue.len() {
            let job = &self.browse_image_queue[i];
            let job_key = &job.texture_key;
            if focus_mode && !allowed_keys.contains(job_key) {
                i += 1;
                continue;
            }
            let pointer_motion_allowed = if job.load_full {
                allowed_keys.contains(job_key) || job.priority <= 1
            } else {
                !job.skip_texture && job.priority <= 20
            };
            if pointer_motion_throttle && !pointer_motion_allowed {
                deferred_for_pointer_motion = true;
                i += 1;
                continue;
            }

            let job = self.browse_image_queue.remove(i);
            self.browse_image_inflight.insert(
                job.texture_key.clone(),
                BrowseImageInflight {
                    request: job.clone(),
                    cancel: job.cancel.clone(),
                    cancel_key: job.cancel_key,
                    skip_texture: job.skip_texture,
                    load_full: job.load_full,
                    started_at: Instant::now(),
                },
            );
            if self.browse_image_request_tx.send(job.clone()).is_err() {
                self.browse_image_inflight.remove(&job.texture_key);
                self.browse_image_queue.insert(i, job);
                break;
            }
        }
        if deferred_for_pointer_motion {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    fn consume_browse_image_results(&mut self) {
        while let Ok(result) = self.browse_image_result_rx.try_recv() {
            self.browse_image_inflight.remove(&result.texture_key);
            if let Some(cancel_key) = result.cancel_key {
                if cancel_key != self.browse_detail_generation {
                    continue;
                }
            }
            if let Some(failure) = result.failure {
                self.browse_image_retry_after.insert(
                    result.texture_key.clone(),
                    Instant::now() + Duration::from_secs(BROWSE_IMAGE_RETRY_COOLDOWN_SECS),
                );
                if failure.timed_out {
                    self.log_warn(format!(
                        "image request timed out; retrying in {}s: {} ({})",
                        BROWSE_IMAGE_RETRY_COOLDOWN_SECS, failure.url, failure.error
                    ));
                } else {
                    self.log_warn(format!(
                        "image request failed; retrying in {}s: {} ({})",
                        BROWSE_IMAGE_RETRY_COOLDOWN_SECS, failure.url, failure.error
                    ));
                }
                continue;
            }
            self.browse_image_retry_after.remove(&result.texture_key);
            if let Some(image_thumb) = result.image_thumb {
                self.pending_texture_uploads
                    .push_back(PendingTextureUpload::BrowseThumb {
                        texture_key: result.thumb_texture_key,
                        image: image_thumb,
                    });
            }
            if let Some(image_full) = result.image_full {
                self.pending_texture_uploads
                    .push_back(PendingTextureUpload::BrowseFull {
                        texture_key: result.texture_key,
                        image: image_full,
                    });
            }
        }
    }

    fn queue_browse_download(
        &mut self,
        game_id: String,
        mod_id: u64,
        file: gamebanana::ModFile,
        selected_files: Vec<gamebanana::ModFile>,
        task_id: Option<u64>,
        unsafe_content: bool,
        update_folder_name: Option<String>,
        update_target_mod_id: Option<String>,
        install_state: ModInstallState,
        preserve_existing_state: bool,
        post_install_rename_to: Option<String>,
    ) {
        self.queue_browse_download_with_resolution(
            game_id,
            mod_id,
            file,
            selected_files,
            task_id,
            unsafe_content,
            update_folder_name,
            update_target_mod_id,
            install_state,
            preserve_existing_state,
            post_install_rename_to,
            None,
        );
    }

    fn queue_browse_download_with_resolution(
        &mut self,
        game_id: String,
        mod_id: u64,
        file: gamebanana::ModFile,
        selected_files: Vec<gamebanana::ModFile>,
        task_id: Option<u64>,
        unsafe_content: bool,
        update_folder_name: Option<String>,
        update_target_mod_id: Option<String>,
        install_state: ModInstallState,
        preserve_existing_state: bool,
        post_install_rename_to: Option<String>,
        explicit_resolution: Option<ImportResolution>,
    ) {
        if !self.game_can_download_mods(&game_id) {
            if let Some(task_id) = task_id {
                self.update_task_status(task_id, TaskStatus::Failed);
            }
            self.report_warn(
                self.game_mod_setup_message(&game_id),
                Some(self.text().install_unavailable()),
            );
            return;
        }
        let task_id = task_id.unwrap_or_else(|| self.next_background_job_id());
        let title = file.file_name.clone();
        let size = Some(file.file_size);
        let retry_payload = match explicit_resolution {
            Some(explicit_resolution) => self.browse_download_retry_payload_with_resolution(
                game_id.clone(),
                mod_id,
                &file,
                &selected_files,
                unsafe_content,
                update_folder_name,
                update_target_mod_id,
                install_state,
                preserve_existing_state,
                post_install_rename_to,
                Some(explicit_resolution),
            ),
            None => self.browse_download_retry_payload(
                game_id.clone(),
                mod_id,
                &file,
                &selected_files,
                unsafe_content,
                update_folder_name,
                update_target_mod_id,
                install_state,
                preserve_existing_state,
                post_install_rename_to,
            ),
        };
        if task_id == self.install_next_job_id {
            self.install_next_job_id = self.install_next_job_id.wrapping_add(1);
        }
        if !self.state.tasks.iter().any(|task| task.id == task_id) {
            self.add_task(
                task_id,
                TaskKind::Download,
                TaskStatus::Queued,
                title.clone(),
                Some(game_id.clone()),
                size,
                unsafe_content,
            );
        } else {
            if let Some(task) = self.state.tasks.iter_mut().find(|task| task.id == task_id) {
                task.title = title.clone();
                task.game_id = Some(game_id.clone());
                task.updated_at = Utc::now();
                if task.total_size.is_none() {
                    task.total_size = size;
                }
            }
            self.update_task_status(task_id, TaskStatus::Queued);
        }
        self.update_task_retry_payload(
            task_id,
            Some(TaskRetryPayload::BrowseDownload(retry_payload.clone())),
        );
        self.pending_browse_install_safety
            .insert(task_id, unsafe_content);
        self.pending_browse_install_meta.insert(
            task_id,
            self.pending_browse_install_meta_from_retry_payload(&retry_payload),
        );
        self.browse_download_queue
            .push_back(Self::browse_download_job_from_retry_payload(
                task_id,
                &retry_payload,
                self.cache_limit_bytes.load(Ordering::Relaxed),
            ));
    }

    fn process_browse_download_queue(&mut self) {
        let max_parallel = self.browse_download_parallelism();
        while self.browse_download_inflight.len() < max_parallel {
            let Some(job) = self.browse_download_queue.pop_front() else {
                break;
            };
            if job.url.is_empty() {
                self.update_task_status(job.task_id, TaskStatus::Failed);
                continue;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            self.browse_download_inflight.insert(
                job.task_id,
                BrowseDownloadInflight {
                    progress: Arc::new(RwLock::new(DownloadProgress {
                        downloaded: 0,
                        total: job.total_size,
                        speed: 0.0,
                        last_update: std::time::Instant::now(),
                        bytes_since_last: 0,
                    })),
                    cancel: Arc::clone(&cancel),
                },
            );
            self.update_task_status(job.task_id, TaskStatus::Downloading);
            let progress = self
                .browse_download_inflight
                .get(&job.task_id)
                .unwrap()
                .progress
                .clone();
            let tx = self.browse_download_result_tx.clone();
            let portable = self.portable.clone();
            let client = self.runtime_services.download_http_client();
            let full_limiter = Arc::clone(&self.runtime_services.full_image_limiter);
            let handle = self.runtime_services.handle();
            self.runtime_services.spawn(async move {
                let _lane = full_limiter.acquire().await.ok();
                let result = async {
                    let key = job.cache_key.clone();
                    let portable_for_get = portable.clone();
                    if let Ok(Ok(Some(bytes))) = handle
                        .spawn_blocking(move || persistence::cache_get(&portable_for_get, &key))
                        .await
                    {
                        let byte_size = bytes.len() as u64;
                        if job.total_size.is_none_or(|expected| byte_size == expected)
                            && job
                                .md5
                                .as_deref()
                                .is_none_or(|md5| importing::bundle::bytes_match_md5(&bytes, md5))
                        {
                            return Ok::<u64, anyhow::Error>(byte_size);
                        }
                    }
                    let byte_size = download_browse_file_to_cache_with_retries(
                        &client,
                        &portable,
                        &job,
                        &cancel,
                        progress,
                        handle.clone(),
                    )
                    .await?;
                    verify_browse_download_md5(&portable, &job, &cancel, &handle).await?;
                    Ok(byte_size)
                }
                .await;
                match result {
                    Ok(byte_size) => {
                        let _ = tx.send(BrowseDownloadEvent::Done {
                            task_id: job.task_id,
                            title: job.title,
                            cache_key: job.cache_key,
                            file_name: job.file_name,
                            byte_size,
                        });
                    }
                    Err(err) if err.to_string() == importing::CANCELLED_ERROR => {
                        let _ = tx.send(BrowseDownloadEvent::Canceled {
                            task_id: job.task_id,
                            title: job.title,
                        });
                    }
                    Err(err) => {
                        let _ = tx.send(BrowseDownloadEvent::Failed {
                            task_id: job.task_id,
                            title: job.title,
                            error: format!("{err:#}"),
                        });
                    }
                }
            });
        }
    }

    fn consume_browse_download_events(&mut self) {
        while let Ok(event) = self.browse_download_event_rx.try_recv() {
            match event {
                BrowseDownloadEvent::Done {
                    task_id,
                    title,
                    cache_key,
                    file_name,
                    byte_size,
                } => {
                    self.browse_download_inflight.remove(&task_id);
                    self.mark_usage_counters_dirty();
                    if let Some(task) = self.state.tasks.iter_mut().find(|t| t.id == task_id) {
                        task.total_size = Some(byte_size);
                    }
                    self.update_task_status(task_id, TaskStatus::Installing);
                    self.set_message_ok(self.text().downloaded(&title));
                    let install_game_id = self
                        .pending_browse_install_meta
                        .get(&task_id)
                        .map(|meta| meta.game_id.clone());
                    match self
                        .write_cached_download_to_temp_archive(task_id, &cache_key, &file_name)
                    {
                        Ok(temp_archive) => {
                            if let Some(game_id) = install_game_id.as_deref()
                                && self.hold_thalia_bundle_download(
                                    task_id,
                                    game_id,
                                    &title,
                                    temp_archive.clone(),
                                )
                            {
                                continue;
                            }
                            let Some(game_id) = install_game_id else {
                                self.update_task_status(task_id, TaskStatus::Failed);
                                self.report_error_message(
                                    "Missing game context for install".to_string(),
                                    Some(self.text().could_not_prepare_install()),
                                );
                                continue;
                            };
                            self.enqueue_install_source_for_existing_task(
                                task_id,
                                game_id,
                                title,
                                ImportSource::Archive(temp_archive),
                            );
                        }
                        Err(err) => {
                            self.update_task_status(task_id, TaskStatus::Failed);
                            self.report_error(err, Some(self.text().could_not_prepare_install()));
                        }
                    }
                }
                BrowseDownloadEvent::Failed {
                    task_id,
                    title,
                    error,
                } => {
                    self.browse_download_inflight.remove(&task_id);
                    self.mark_usage_counters_dirty();
                    self.update_task_status(task_id, TaskStatus::Failed);
                    self.report_error_message(
                        self.text().browse_download_failed_message(&title, &error),
                        Some(self.text().download_failed()),
                    );
                }
                BrowseDownloadEvent::Canceled { task_id, title } => {
                    self.browse_download_inflight.remove(&task_id);
                    self.mark_usage_counters_dirty();
                    self.update_task_status(task_id, TaskStatus::Canceled);
                    if self.proxy_requeue_browse_downloads.remove(&task_id) {
                        let in_bundle = self
                            .thalia_install_bundles
                            .iter()
                            .any(|bundle| bundle.members.contains(&task_id));
                        let _ = self.requeue_browse_download_task(task_id, in_bundle);
                    } else {
                        self.set_message_ok(self.text().download_canceled(&title));
                    }
                }
            }
        }
    }

    fn fail_browse_update(&mut self, task_id: u64) {
        self.update_task_status(task_id, TaskStatus::Failed);
        let message = self.text().update_unavailable().to_string();
        self.report_warn(message, Some(self.text().update_unavailable()));
    }

    fn prepare_browse_update_folder_name(
        &mut self,
        target_id: Option<&str>,
        current_name: Option<String>,
        preferred_name: Option<&str>,
    ) -> (Option<String>, Option<String>) {
        let (Some(target_id), Some(preferred_name)) = (target_id, preferred_name) else {
            return (current_name, None);
        };
        match self.rename_mod_folder(target_id, preferred_name) {
            Ok(()) => {
                self.save_state();
                (Some(preferred_name.to_string()), None)
            }
            Err(_) => (current_name, Some(preferred_name.to_string())),
        }
    }

    fn resolve_browse_install_after_detail(&mut self, pending: PendingBrowseInstall) {
        if !self.game_can_install_mods(&pending.game_id) {
            self.update_task_status(pending.task_id, TaskStatus::Failed);
            self.report_warn(
                self.game_mod_setup_message(&pending.game_id),
                Some(self.text().install_unavailable()),
            );
            return;
        }
        let Some(detail) = self.browse_state.details.get(&pending.mod_id).cloned() else {
            self.browse_state.pending_installs.push(pending);
            return;
        };
        let update_folder_name = pending.update_target_id.as_ref().and_then(|id| {
            self.state
                .mods
                .iter()
                .find(|m| &m.id == id)
                .map(|m| m.folder_name.clone())
        });
        let preferred_update_folder_name =
            if let Some(target_id) = pending.update_target_id.as_deref() {
                let preferred_name = self.preferred_browse_folder_name(
                    self.browse_mod_title_for_install(pending.mod_id, Some(target_id))
                        .as_deref(),
                    update_folder_name
                        .as_deref()
                        .unwrap_or(self.text().imported_mod()),
                );
                (update_folder_name.as_deref() != Some(preferred_name.as_str()))
                    .then_some(preferred_name)
            } else {
                None
            };

        let selectable: Vec<_> = detail
            .profile
            .files
            .iter()
            .filter(|file| file.download_url.is_some())
            .cloned()
            .collect();
        let existing_mod = pending.update_target_id.as_deref().and_then(|target_id| {
            self.state
                .mods
                .iter()
                .find(|m| {
                    m.id == target_id
                        && m.game_id == pending.game_id
                        && m.source
                            .as_ref()
                            .and_then(|s| s.gamebanana.as_ref())
                            .is_some_and(|l| l.mod_id == pending.mod_id)
                })
                .cloned()
        });
        if pending.update_target_id.is_some() && existing_mod.is_none() {
            self.fail_browse_update(pending.task_id);
            return;
        }
        if let Some(existing_mod) = existing_mod {
            if let Some(plan) = pending.thalia_plan.as_ref()
                && !plan.matches_target(
                    &existing_mod,
                    self.active_profile_id(&existing_mod.game_id),
                )
            {
                self.remove_task(pending.task_id);
                self.open_mod_file_review(&existing_mod.id);
                return;
            }
            let existing_is_tool = existing_mod
                .source
                .as_ref()
                .and_then(|source| source.gamebanana.as_ref())
                .is_some_and(|link| gamebanana::is_tool_url(&link.url));
            let file_set = existing_mod.source.as_ref().map(|source| &source.file_set);
            if let Some(file_set) = file_set {
                let mut group_items = Vec::new();
                let mut target_index = None;
                for mod_entry in self.state.mods.iter().filter(|m| {
                    m.game_id == pending.game_id
                        && m.source
                            .as_ref()
                            .and_then(|s| s.gamebanana.as_ref())
                            .is_some_and(|l| {
                                l.mod_id == pending.mod_id
                                    && gamebanana::is_tool_url(&l.url) == existing_is_tool
                            })
                }) {
                    let Some(source) = mod_entry.source.as_ref() else {
                        continue;
                    };
                    if mod_entry.id == existing_mod.id {
                        target_index = Some(group_items.len());
                    }
                    group_items.push((
                        selected_file_baseline_ts(&source.file_set),
                        source.file_set.clone(),
                    ));
                }
                let evaluations = evaluate_file_set_update_group(&group_items, &detail.profile);
                let update_files = target_index
                    .and_then(|idx| evaluations.get(idx))
                    .map(|evaluation| {
                        update_candidate_files_from_signature(
                            evaluation.signature.as_ref(),
                            &selectable,
                        )
                    })
                    .unwrap_or_default();
                let tracked_update = file_set_tracks_remote_files(file_set);
                let evaluation_state = target_index
                    .and_then(|idx| evaluations.get(idx))
                    .map(|evaluation| evaluation.state);

                let validated_thalia_file = pending.thalia_plan.as_ref().and_then(|plan| {
                    self.validate_thalia_install_plan(plan, &existing_mod, &detail.profile)
                });
                if pending.thalia_plan.is_some()
                    && evaluation_state != Some(ModUpdateState::UpdateAvailable)
                    && update_files.is_empty()
                {
                    if let Some(validated) = validated_thalia_file {
                        let (update_folder_name, post_install_rename_to) =
                            self.prepare_browse_update_folder_name(
                                pending.update_target_id.as_deref(),
                                update_folder_name.clone(),
                                preferred_update_folder_name.as_deref(),
                            );
                        let merge_needs_stable_folder = post_install_rename_to.is_some()
                            && validated.requires.iter().any(|(action, _)| {
                                *action == thalia::UpdateMatchRequiredAction::Merge
                            });
                        if !Self::should_show_local_change_update_prefs(&existing_mod)
                            && !merge_needs_stable_folder
                        {
                            self.queue_thalia_install(
                                &pending,
                                validated,
                                detail.unsafe_content,
                                update_folder_name,
                                post_install_rename_to,
                            );
                        } else {
                            self.remove_task(pending.task_id);
                            self.browse_state.file_prompt = Some(BrowseFilePrompt {
                                mod_id: pending.mod_id,
                                game_id: pending.game_id.clone(),
                                files: selectable
                                    .into_iter()
                                    .map(|candidate| BrowseSelectableFile {
                                        selected: validated
                                            .file
                                            .as_ref()
                                            .is_some_and(|file| file.id == candidate.id),
                                        file: candidate,
                                    })
                                    .collect(),
                                update_folder_name,
                                update_target_mod_id: pending.update_target_id.clone(),
                                post_install_rename_to,
                                install_state: pending.install_state,
                                preserve_existing_state: pending.preserve_existing_state,
                                file_review: None,
                            });
                        }
                        return;
                    }

                    self.remove_task(pending.task_id);
                    if let Some(target_id) = pending.update_target_id.as_deref() {
                        self.open_mod_file_review(target_id);
                    }
                    return;
                }
                if tracked_update
                    && evaluation_state == Some(ModUpdateState::UpdateAvailable)
                    && update_files.is_empty()
                {
                    self.fail_browse_update(pending.task_id);
                    return;
                }
                if !update_files.is_empty() {
                    let (update_folder_name, post_install_rename_to) =
                        self.prepare_browse_update_folder_name(
                            pending.update_target_id.as_deref(),
                            update_folder_name.clone(),
                            preferred_update_folder_name.as_deref(),
                        );
                    if !Self::should_show_local_change_update_prefs(&existing_mod) {
                        for (i, file) in update_files.iter().cloned().enumerate() {
                            let tid = if i == 0 { Some(pending.task_id) } else { None };
                            self.queue_browse_download(
                                pending.game_id.clone(),
                                pending.mod_id,
                                file,
                                update_files.clone(),
                                tid,
                                detail.unsafe_content,
                                update_folder_name.clone(),
                                pending.update_target_id.clone(),
                                pending.install_state,
                                pending.preserve_existing_state,
                                post_install_rename_to.clone(),
                            );
                        }
                    } else {
                        self.remove_task(pending.task_id);
                        self.browse_state.file_prompt = Some(BrowseFilePrompt {
                            mod_id: pending.mod_id,
                            game_id: pending.game_id,
                            files: selectable
                                .into_iter()
                                .map(|file| BrowseSelectableFile {
                                    selected: update_files
                                        .iter()
                                        .any(|update| update.id == file.id),
                                    file,
                                })
                                .collect(),
                            update_folder_name,
                            update_target_mod_id: pending.update_target_id.clone(),
                            post_install_rename_to: post_install_rename_to.clone(),
                            install_state: pending.install_state,
                            preserve_existing_state: pending.preserve_existing_state,
                            file_review: None,
                        });
                    }
                    return;
                }
                if tracked_update
                    && existing_mod.update_state != ModUpdateState::UpdateAvailable
                {
                    let matches = browse_exact_source_matches(file_set, &selectable);
                    if !matches.is_empty() {
                        let (update_folder_name, post_install_rename_to) =
                            self.prepare_browse_update_folder_name(
                                pending.update_target_id.as_deref(),
                                update_folder_name.clone(),
                                preferred_update_folder_name.as_deref(),
                            );
                        if !Self::should_show_local_change_update_prefs(&existing_mod) {
                            let selected_set = matches.clone();
                            for (i, file) in matches.into_iter().enumerate() {
                                let tid = if i == 0 { Some(pending.task_id) } else { None };
                                self.queue_browse_download(
                                    pending.game_id.clone(),
                                    pending.mod_id,
                                    file,
                                    selected_set.clone(),
                                    tid,
                                    detail.unsafe_content,
                                    update_folder_name.clone(),
                                    pending.update_target_id.clone(),
                                    pending.install_state,
                                    pending.preserve_existing_state,
                                    post_install_rename_to.clone(),
                                );
                            }
                        } else {
                            self.remove_task(pending.task_id);
                            self.browse_state.file_prompt = Some(BrowseFilePrompt {
                                mod_id: pending.mod_id,
                                game_id: pending.game_id,
                                files: selectable
                                    .into_iter()
                                    .map(|file| BrowseSelectableFile {
                                        selected: matches
                                            .iter()
                                            .any(|matched| matched.id == file.id),
                                        file,
                                    })
                                    .collect(),
                                update_folder_name,
                                update_target_mod_id: pending.update_target_id.clone(),
                                post_install_rename_to: post_install_rename_to.clone(),
                                install_state: pending.install_state,
                                preserve_existing_state: pending.preserve_existing_state,
                                file_review: None,
                            });
                        }
                        return;
                    }
                }
                if tracked_update {
                    self.fail_browse_update(pending.task_id);
                    return;
                }
            }
        }
        let (update_folder_name, post_install_rename_to) =
            self.prepare_browse_update_folder_name(
                pending.update_target_id.as_deref(),
                update_folder_name,
                preferred_update_folder_name.as_deref(),
            );
        match selectable.len() {
            0 => {
                self.update_task_status(pending.task_id, TaskStatus::Failed);
                self.report_warn(
                    self.text().no_downloadable_files_found(),
                    Some(self.text().no_downloadable_files_found()),
                );
            }
            1 => {
                let selected_files = vec![selectable[0].clone()];
                self.queue_browse_download(
                    pending.game_id,
                    pending.mod_id,
                    selectable[0].clone(),
                    selected_files,
                    Some(pending.task_id),
                    detail.unsafe_content,
                    update_folder_name,
                    pending.update_target_id,
                    pending.install_state,
                    pending.preserve_existing_state,
                    post_install_rename_to,
                );
            }
            _ => {
                if self.ask_game_overlay_for_file(
                    pending.task_id,
                    &selectable,
                    detail.unsafe_content,
                ) {
                    return;
                }
                self.remove_task(pending.task_id);
                self.browse_state.file_prompt = Some(BrowseFilePrompt {
                    mod_id: pending.mod_id,
                    game_id: pending.game_id,
                    files: selectable
                        .into_iter()
                        .map(|file| BrowseSelectableFile {
                            file,
                            selected: false,
                        })
                        .collect(),
                    update_folder_name,
                    update_target_mod_id: pending.update_target_id,
                    post_install_rename_to,
                    install_state: pending.install_state,
                    preserve_existing_state: pending.preserve_existing_state,
                    file_review: None,
                });
            }
        }
    }

    fn queue_install_for_browse_mod(
        &mut self,
        mod_id: u64,
        explicit_state: Option<ModInstallState>,
    ) {
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            return;
        };
        if !self.selected_game_can_download_mods() {
            self.report_warn(
                self.selected_game_mod_setup_message(),
                Some(self.text().install_unavailable()),
            );
            return;
        }
        let unsafe_content = self
            .browse_state
            .cards
            .iter()
            .find(|card| card.id == mod_id)
            .map(|card| card.unsafe_content)
            .or_else(|| {
                self.browse_state
                    .details
                    .get(&mod_id)
                    .map(|detail| detail.unsafe_content)
            })
            .unwrap_or(false);
        let title = self
            .browse_state
            .cards
            .iter()
            .find(|card| card.id == mod_id)
            .map(|card| card.name.clone())
            .unwrap_or_else(|| format!("Mod {mod_id}"));
        let (install_state, preserve_existing_state) = match explicit_state {
            Some(state) => (state, false),
            None => (self.state.static_prefs.mod_install_state, true),
        };
        let task_id = self.next_background_job_id();
        self.add_task(
            task_id,
            TaskKind::Download,
            TaskStatus::Queued,
            title.clone(),
            Some(game_id.clone()),
            None,
            unsafe_content,
        );
        self.request_browse_detail(mod_id);
        self.resolve_browse_install_after_detail(PendingBrowseInstall {
            task_id,
            mod_id,
            game_id,
            update_target_id: None,
            thalia_plan: None,
            install_state,
            preserve_existing_state,
        });
        self.set_message_ok(self.text().resolving_download(&title));
    }

    fn queue_thalia_install(
        &mut self,
        pending: &PendingBrowseInstall,
        validated: ValidatedThaliaInstall,
        unsafe_content: bool,
        update_folder_name: Option<String>,
        post_install_rename_to: Option<String>,
    ) {
        let ValidatedThaliaInstall {
            action,
            file,
            requires,
        } = validated;
        let main_resolution = (action == ThaliaRecommendationAction::Merge)
            .then_some(ImportResolution::Merge);
        if requires.is_empty() && file.is_none() {
            self.remove_task(pending.task_id);
            return;
        }
        if let Some(file) = file.clone()
            && (requires.is_empty() || !self.game_can_download_mods(&pending.game_id))
        {
            self.queue_browse_download_with_resolution(
                pending.game_id.clone(),
                pending.mod_id,
                file.clone(),
                vec![file],
                Some(pending.task_id),
                unsafe_content,
                update_folder_name,
                pending.update_target_id.clone(),
                pending.install_state,
                pending.preserve_existing_state,
                post_install_rename_to,
                main_resolution,
            );
            return;
        }
        if !self.game_can_download_mods(&pending.game_id) {
            self.update_task_status(pending.task_id, TaskStatus::Failed);
            self.report_warn(
                self.game_mod_setup_message(&pending.game_id),
                Some(self.text().install_unavailable()),
            );
            return;
        }
        let title = self
            .browse_mod_title_for_install(pending.mod_id, pending.update_target_id.as_deref())
            .or_else(|| file.as_ref().map(|file| file.file_name.clone()))
            .unwrap_or_default();
        let mut members = Vec::new();
        let mut separate = HashSet::new();
        let mut main = None;
        for (_, required) in requires
            .iter()
            .filter(|(action, _)| *action == thalia::UpdateMatchRequiredAction::Separate)
        {
            let task_id = self.next_background_job_id();
            self.thalia_separate_install_jobs.insert(task_id);
            separate.insert(task_id);
            self.queue_browse_download_with_resolution(
                pending.game_id.clone(),
                pending.mod_id,
                required.clone(),
                vec![required.clone()],
                Some(task_id),
                unsafe_content,
                None,
                None,
                self.state.static_prefs.mod_install_state,
                false,
                None,
                Some(ImportResolution::KeepBoth),
            );
            members.push(task_id);
        }
        if let Some(file) = file {
            self.queue_browse_download_with_resolution(
                pending.game_id.clone(),
                pending.mod_id,
                file.clone(),
                vec![file],
                Some(pending.task_id),
                unsafe_content,
                update_folder_name.clone(),
                pending.update_target_id.clone(),
                pending.install_state,
                pending.preserve_existing_state,
                post_install_rename_to,
                Some(main_resolution.unwrap_or(ImportResolution::Replace)),
            );
            members.push(pending.task_id);
            main = Some(pending.task_id);
        }
        let mut spare_task_id = (!members.contains(&pending.task_id)).then_some(pending.task_id);
        for (_, required) in requires
            .iter()
            .filter(|(action, _)| *action == thalia::UpdateMatchRequiredAction::Merge)
        {
            let task_id = spare_task_id
                .take()
                .unwrap_or_else(|| self.next_background_job_id());
            self.queue_browse_download_with_resolution(
                pending.game_id.clone(),
                pending.mod_id,
                required.clone(),
                vec![required.clone()],
                Some(task_id),
                unsafe_content,
                update_folder_name.clone(),
                pending.update_target_id.clone(),
                pending.install_state,
                pending.preserve_existing_state,
                None,
                Some(ImportResolution::Merge),
            );
            members.push(task_id);
        }
        if let Some(task_id) = spare_task_id {
            self.remove_task(task_id);
        }
        for &member in &members {
            self.mark_thalia_bundle_payload(member);
        }
        let target = pending.update_target_id.clone().map(|mod_id| {
            let state = self.thalia_bundle_target_state(&mod_id);
            (mod_id, state)
        });
        let answer = pending.thalia_plan.as_ref().map(|plan| {
            (
                plan.context.local_mod_id.clone(),
                plan.recommendation.clone(),
            )
        });
        let key = self.next_background_job_id();
        self.thalia_install_bundles.push(ThaliaInstallBundle {
            key,
            title,
            game_id: pending.game_id.clone(),
            members,
            separate,
            main,
            target,
            answer,
            downloaded: HashMap::new(),
            preparing: false,
            prepared_root: None,
            target_changed: false,
            finished: HashSet::new(),
            installed_any: false,
        });
    }

    fn mark_thalia_bundle_payload(&mut self, task_id: u64) {
        let Some(TaskRetryPayload::BrowseDownload(mut payload)) = self
            .state
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| task.retry_payload.clone())
        else {
            return;
        };
        payload.thalia_bundle = true;
        self.update_task_retry_payload(task_id, Some(TaskRetryPayload::BrowseDownload(payload)));
    }

    fn thalia_bundle_target_state(&self, mod_id: &str) -> Option<u64> {
        let entry = self.state.mods.iter().find(|entry| entry.id == mod_id)?;
        importing::bundle::mod_folder_state(&entry.root_path)
    }

    fn thalia_bundle_target_unchanged(&self, index: usize) -> bool {
        self.thalia_install_bundles[index]
            .target
            .as_ref()
            .is_none_or(|(mod_id, state)| {
                state.is_some() && self.thalia_bundle_target_state(mod_id) == *state
            })
    }

    fn thalia_bundle_target_changed_before_install(&mut self, job_id: u64) -> bool {
        let Some(index) = self.thalia_install_bundles.iter().position(|bundle| {
            bundle.members.contains(&job_id) && !bundle.separate.contains(&job_id)
        }) else {
            return false;
        };
        if self.thalia_bundle_target_unchanged(index) {
            return false;
        }
        self.thalia_install_bundles[index].target_changed = true;
        true
    }

    fn task_is_running(&self, task_id: u64) -> bool {
        self.browse_download_queue
            .iter()
            .any(|job| job.task_id == task_id)
            || self.browse_download_inflight.contains_key(&task_id)
            || self.install_queue.iter().any(|job| job.id == task_id)
            || self.install_inflight.contains_key(&task_id)
    }

    fn hold_thalia_bundle_download(
        &mut self,
        task_id: u64,
        game_id: &str,
        title: &str,
        archive: PathBuf,
    ) -> bool {
        if self.thalia_canceled_bundle_tasks.remove(&task_id) {
            Self::cleanup_runtime_temp_for_source(&ImportSource::Archive(archive));
            self.pending_browse_install_meta.remove(&task_id);
            self.pending_browse_install_safety.remove(&task_id);
            self.update_task_status(task_id, TaskStatus::Canceled);
            return true;
        }
        let Some(index) = self
            .thalia_install_bundles
            .iter()
            .position(|bundle| bundle.members.contains(&task_id))
        else {
            return false;
        };
        let bundle = &mut self.thalia_install_bundles[index];
        bundle
            .downloaded
            .insert(task_id, (game_id.to_string(), title.to_string(), archive));
        if bundle.downloaded.len() < bundle.members.len() {
            self.update_task_status(task_id, TaskStatus::Queued);
            return true;
        }
        self.start_thalia_bundle_prepare(index);
        true
    }

    fn start_thalia_bundle_prepare(&mut self, index: usize) {
        if !self.thalia_bundle_target_unchanged(index) {
            self.thalia_install_bundles[index].target_changed = true;
            self.fail_thalia_bundle(index, None, None);
            return;
        }
        let bundle = &self.thalia_install_bundles[index];
        let parts = bundle
            .members
            .iter()
            .filter_map(|member| {
                let (_, title, archive) = bundle.downloaded.get(member)?;
                let meta = self.pending_browse_install_meta.get(member);
                let replaces = meta.is_none_or(|meta| {
                    meta.explicit_resolution != Some(ImportResolution::Merge)
                });
                let role = if bundle.separate.contains(member) {
                    importing::bundle::BundlePartRole::Separate
                } else if bundle.main == Some(*member) && replaces {
                    importing::bundle::BundlePartRole::Main
                } else {
                    importing::bundle::BundlePartRole::Merge
                };
                let md5 = meta
                    .and_then(|meta| meta.selected_files.first())
                    .and_then(|file| file.md5.clone());
                Some(importing::bundle::BundlePart {
                    job_id: *member,
                    file_name: title.clone(),
                    archive: archive.clone(),
                    md5,
                    role,
                })
            })
            .collect();
        let key = bundle.key;
        let game_id = bundle.game_id.clone();
        let members = bundle.members.clone();
        self.thalia_install_bundles[index].preparing = true;
        for member in members {
            self.update_task_status(member, TaskStatus::Installing);
        }
        let _ = self.install_request_tx.send(InstallRequest::PrepareThaliaBundle {
            job_id: key,
            game_id,
            parts,
        });
    }

    fn thalia_bundle_prepared(
        &mut self,
        key: u64,
        root: PathBuf,
        folders: Vec<importing::bundle::PreparedBundleFolder>,
    ) {
        let Some(index) = self
            .thalia_install_bundles
            .iter()
            .position(|bundle| bundle.key == key && bundle.preparing)
        else {
            let _ = fs::remove_dir_all(&root);
            return;
        };
        let downloaded = {
            let bundle = &mut self.thalia_install_bundles[index];
            bundle.preparing = false;
            bundle.prepared_root = Some(root);
            std::mem::take(&mut bundle.downloaded)
        };
        for (_, _, archive) in downloaded.values() {
            Self::cleanup_runtime_temp_for_source(&ImportSource::Archive(archive.clone()));
        }
        if !self.thalia_bundle_target_unchanged(index) {
            self.thalia_install_bundles[index].target_changed = true;
            self.fail_thalia_bundle(index, None, None);
            return;
        }
        let bundle = &self.thalia_install_bundles[index];
        let lead = folders
            .iter()
            .map(|folder| folder.job_id)
            .find(|job_id| !bundle.separate.contains(job_id));
        let folded: Vec<u64> = bundle
            .members
            .iter()
            .copied()
            .filter(|member| !bundle.separate.contains(member) && Some(*member) != lead)
            .collect();
        let game_id = bundle.game_id.clone();
        let mut merged_files = Vec::new();
        for member in &folded {
            if let Some(meta) = self.pending_browse_install_meta.remove(member) {
                merged_files.extend(meta.selected_files);
            }
            self.pending_browse_install_safety.remove(member);
        }
        if let Some(meta) = lead.and_then(|lead| self.pending_browse_install_meta.get_mut(&lead)) {
            meta.selected_files.extend(merged_files);
        }
        self.thalia_install_bundles[index]
            .members
            .retain(|member| !folded.contains(member));
        for member in folded {
            self.remove_task(member);
        }
        for folder in folders {
            let title = downloaded
                .get(&folder.job_id)
                .map(|(_, title, _)| title.clone())
                .unwrap_or_default();
            self.thalia_bundle_manifests
                .insert(folder.job_id, Arc::new(folder.manifest));
            self.enqueue_install_source_for_existing_task(
                folder.job_id,
                game_id.clone(),
                title,
                ImportSource::Folder(folder.source),
            );
        }
    }

    fn thalia_bundle_prepare_failed(&mut self, key: u64, failure: importing::bundle::BundleFailure) {
        let Some(index) = self
            .thalia_install_bundles
            .iter()
            .position(|bundle| bundle.key == key && bundle.preparing)
        else {
            return;
        };
        self.thalia_install_bundles[index].preparing = false;
        self.fail_thalia_bundle(index, None, Some(failure));
    }

    fn cancel_waiting_thalia_bundle_member(&mut self, task_id: u64) -> bool {
        let waiting = self.thalia_install_bundles.iter().any(|bundle| {
            bundle.members.contains(&task_id)
                && (bundle.preparing || bundle.downloaded.contains_key(&task_id))
        });
        if waiting {
            self.update_task_status(task_id, TaskStatus::Canceled);
        }
        waiting
    }

    fn note_thalia_bundle_task_finished(&mut self, task_id: u64, status: TaskStatus) {
        if status == TaskStatus::Canceled && self.proxy_requeue_browse_downloads.contains(&task_id)
        {
            return;
        }
        let Some(index) = self
            .thalia_install_bundles
            .iter()
            .position(|bundle| bundle.members.contains(&task_id))
        else {
            self.thalia_separate_install_jobs.remove(&task_id);
            self.thalia_canceled_bundle_tasks.remove(&task_id);
            return;
        };
        if status == TaskStatus::Completed {
            let bundle = &mut self.thalia_install_bundles[index];
            if bundle.finished.insert(task_id) {
                bundle.installed_any = true;
            }
            self.thalia_separate_install_jobs.remove(&task_id);
            if bundle.finished.len() == bundle.members.len() {
                let bundle = self.thalia_install_bundles.remove(index);
                if let Some(root) = bundle.prepared_root {
                    let _ = fs::remove_dir_all(root);
                }
            }
            return;
        }
        self.fail_thalia_bundle(index, Some(task_id), None);
    }

    fn fail_thalia_bundle(
        &mut self,
        index: usize,
        ended_task: Option<u64>,
        failure: Option<importing::bundle::BundleFailure>,
    ) {
        use importing::bundle::BundleFailure;
        let bundle = self.thalia_install_bundles.remove(index);
        if bundle.preparing {
            let _ = self.install_request_tx.send(InstallRequest::Cancel {
                job_id: bundle.key,
            });
        }
        for (_, _, archive) in bundle.downloaded.values() {
            Self::cleanup_runtime_temp_for_source(&ImportSource::Archive(archive.clone()));
        }
        let task_title = |task_id: u64| {
            self.state
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| task.title.clone())
        };
        let failed_file = match &failure {
            Some(
                BundleFailure::Checksum { file_name }
                | BundleFailure::Layout { file_name }
                | BundleFailure::Other {
                    file_name: Some(file_name),
                    ..
                },
            ) => Some(file_name.clone()),
            _ => None,
        }
        .or_else(|| ended_task.and_then(task_title))
        .or_else(|| bundle.members.first().copied().and_then(task_title))
        .unwrap_or_default();
        let stopped_status = if failure.is_some() || bundle.target_changed {
            TaskStatus::Failed
        } else {
            TaskStatus::Canceled
        };
        for member in bundle.members.iter().copied() {
            self.update_task_retry_payload(member, None);
            self.thalia_separate_install_jobs.remove(&member);
            self.thalia_bundle_manifests.remove(&member);
            if bundle.finished.contains(&member) {
                continue;
            }
            if !self.task_is_running(member) {
                self.pending_browse_install_meta.remove(&member);
                self.pending_browse_install_safety.remove(&member);
                if Some(member) != ended_task {
                    self.update_task_status(member, stopped_status);
                }
                continue;
            }
            if Some(member) == ended_task {
                continue;
            }
            if self.browse_download_inflight.contains_key(&member) {
                self.thalia_canceled_bundle_tasks.insert(member);
            }
            self.cancel_task(member);
        }
        if let Some(root) = bundle.prepared_root.as_ref() {
            let _ = fs::remove_dir_all(root);
        }
        if matches!(
            failure,
            Some(BundleFailure::Layout { .. } | BundleFailure::Clash { .. })
        ) && let Some((mod_id, answer)) = bundle.answer.as_ref()
        {
            self.remember_refused_thalia_answer(mod_id, answer);
        }
        let message = if bundle.target_changed {
            self.text().thalia_bundle_target_changed(&bundle.title)
        } else if let Some(BundleFailure::Clash { path }) = &failure {
            self.text().thalia_bundle_clash(&bundle.title, path)
        } else if bundle.installed_any {
            self.text()
                .thalia_bundle_partly_updated(&bundle.title, &failed_file)
        } else {
            self.text()
                .thalia_bundle_not_updated(&bundle.title, &failed_file)
        };
        self.report_warn(message, Some(self.text().update_unavailable()));
    }

    fn confirm_browse_file_prompt(&mut self) {
        let action = self
            .browse_state
            .file_prompt
            .as_ref()
            .and_then(|prompt| prompt.file_review.as_ref())
            .map(|review| review.recommended_action.unwrap_or(FileReviewAction::Update));
        self.confirm_browse_file_prompt_action(action);
    }

    fn confirm_browse_file_prompt_with_action(&mut self, action: FileReviewAction) {
        self.confirm_browse_file_prompt_action(Some(action));
    }

    fn confirm_browse_file_prompt_action(&mut self, review_action: Option<FileReviewAction>) {
        let Some(prompt) = self.browse_state.file_prompt.take() else {
            return;
        };
        let review_action = review_action.filter(|_| prompt.file_review.is_some());
        if let Some(review) = prompt.file_review.as_ref() {
            let detail_matches_review = self
                .browse_state
                .details
                .get(&prompt.mod_id)
                .is_some_and(|detail| {
                    detail.profile.id == prompt.mod_id
                        && detail.is_tool == review.expected_is_tool
                });
            if !detail_matches_review {
                self.report_warn(
                    self.text().file_review_target_changed(),
                    Some(self.text().update_unavailable()),
                );
                self.browse_state.file_prompt = Some(prompt);
                return;
            }
            if !self.game_can_download_mods(&prompt.game_id) {
                self.report_warn(
                    self.game_mod_setup_message(&prompt.game_id),
                    Some(self.text().install_unavailable()),
                );
                self.browse_state.file_prompt = Some(prompt);
                return;
            }
            if review_action != Some(FileReviewAction::InstallSeparate)
                && let Some(target_id) = prompt.update_target_mod_id.as_deref()
            {
                if let Some(lock_reason) =
                    self.mod_action_lock_reason_by_id(target_id, ModMutationKind::UpdateExisting)
                {
                    self.report_warn(lock_reason, Some(self.text().update_unavailable()));
                    self.browse_state.file_prompt = Some(prompt);
                    return;
                }
            }
            let valid_target = prompt
                .update_target_mod_id
                .as_deref()
                .and_then(|target_id| self.state.mods.iter().find(|entry| entry.id == target_id))
                .is_some_and(|target| {
                    target.game_id == prompt.game_id
                        && target.root_path == review.target_root_path
                        && target
                            .source
                            .as_ref()
                            .and_then(|source| source.gamebanana.as_ref())
                            .is_some_and(|link| {
                                link.mod_id == prompt.mod_id
                                    && link.url == review.expected_source_url
                                    && gamebanana::is_tool_url(&link.url) == review.expected_is_tool
                            })
                        && target
                            .source
                            .as_ref()
                            .is_some_and(|source| source.file_set == review.expected_file_set)
                });
            if !valid_target {
                self.report_warn(
                    self.text().file_review_target_changed(),
                    Some(self.text().update_unavailable()),
                );
                return;
            }
        }
        let (update_folder_name, update_target_mod_id, install_state, preserve_existing_state, post_install_rename_to, explicit_resolution) =
            match review_action {
                Some(FileReviewAction::Update) => (
                    prompt.update_folder_name.clone(),
                    prompt.update_target_mod_id.clone(),
                    prompt.install_state,
                    prompt.preserve_existing_state,
                    prompt.post_install_rename_to.clone(),
                    Some(ImportResolution::Replace),
                ),
                Some(FileReviewAction::InstallSeparate) => (
                    None,
                    None,
                    self.state.static_prefs.mod_install_state,
                    false,
                    None,
                    Some(ImportResolution::KeepBoth),
                ),
                Some(FileReviewAction::InstallMerge) => (
                    prompt.update_folder_name.clone(),
                    prompt.update_target_mod_id.clone(),
                    prompt.install_state,
                    prompt.preserve_existing_state,
                    prompt.post_install_rename_to.clone(),
                    Some(ImportResolution::Merge),
                ),
                None => (
                    prompt.update_folder_name.clone(),
                    prompt.update_target_mod_id.clone(),
                    prompt.install_state,
                    prompt.preserve_existing_state,
                    prompt.post_install_rename_to.clone(),
                    None,
                ),
            };
        let selected_files: Vec<_> = prompt
            .files
            .into_iter()
            .filter(|file| file.selected)
            .map(|file| file.file)
            .collect();
        if selected_files.is_empty() {
            self.set_message_ok(self.text().no_files_selected());
            return;
        }
        for file in selected_files.iter().cloned() {
            let unsafe_content = self
                .browse_state
                .details
                .get(&prompt.mod_id)
                .map(|detail| detail.unsafe_content)
                .unwrap_or(false);
            let selected_set = if explicit_resolution == Some(ImportResolution::Merge) {
                vec![file.clone()]
            } else {
                browse_selected_files_for_queued_download(
                    &file,
                    &selected_files,
                    update_target_mod_id.as_deref(),
                )
            };
            self.queue_browse_download_with_resolution(
                prompt.game_id.clone(),
                prompt.mod_id,
                file,
                selected_set.clone(),
                None,
                unsafe_content,
                update_folder_name.clone(),
                update_target_mod_id.clone(),
                install_state,
                preserve_existing_state,
                post_install_rename_to.clone(),
                explicit_resolution,
            );
        }
        self.set_message_ok(self.text().download_queued());
    }
}

#[cfg(test)]
mod browse_file_prompt_tests {
    use super::*;

    #[test]
    fn browse_profile_freshness_requires_recent_past_observation() {
        let now = Utc::now();
        assert!(browse_profile_is_fresh(
            Some(now - chrono::Duration::seconds(899)),
            now,
        ));
        assert!(!browse_profile_is_fresh(
            Some(now - chrono::Duration::seconds(900)),
            now,
        ));
        assert!(!browse_profile_is_fresh(None, now));
        assert!(!browse_profile_is_fresh(
            Some(now + chrono::Duration::seconds(1)),
            now,
        ));
    }

    #[test]
    fn browse_detail_retry_cooldown_only_allows_attempts_after_thirty_seconds() {
        let now = Instant::now();
        assert!(browse_detail_attempt_is_recent(
            Some(now.checked_sub(Duration::from_secs(29)).unwrap()),
            now,
        ));
        assert!(!browse_detail_attempt_is_recent(
            Some(now.checked_sub(BROWSE_PROFILE_RETRY_COOLDOWN).unwrap()),
            now,
        ));
        assert!(!browse_detail_attempt_is_recent(None, now));
    }

    #[test]
    fn thalia_merge_recommendation_selects_only_a_present_candidate() {
        let files = vec![mod_file(11, "patch.zip"), mod_file(12, "other.zip")];
        let recommendation = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Merge,
            id: Some(11),
            requires: Vec::new(),
        };
        assert_eq!(
            thalia_merge_recommendation_id(Some(&recommendation), &files),
            Some(11)
        );

        let stale = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Merge,
            id: Some(99),
            requires: Vec::new(),
        };
        assert_eq!(thalia_merge_recommendation_id(Some(&stale), &files), None);
        let update = ThaliaRecommendation {
            action: ThaliaRecommendationAction::Update,
            id: Some(11),
            requires: Vec::new(),
        };
        assert_eq!(thalia_merge_recommendation_id(Some(&update), &files), None);
    }

    fn browse_card(id: u64, unsafe_content: bool) -> BrowseCard {
        BrowseCard {
            id,
            game_id: "game".to_string(),
            name: format!("mod-{id}"),
            author_name: "author".to_string(),
            like_count: id,
            download_count: None,
            updated_at: Utc::now(),
            thumbnail_url: None,
            has_files: true,
            unsafe_content,
        }
    }

    fn mod_file(id: u64, file_name: &str) -> gamebanana::ModFile {
        gamebanana::ModFile {
            id,
            file_name: file_name.to_string(),
            file_size: 1,
            date_added: id as i64,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some(format!("https://example.com/{file_name}")),
            is_archived: false,
            md5: None,
        }
    }

    #[test]
    fn review_file_identity_prefers_ids_over_reused_names() {
        let installed = mod_file(7, "same-name.zip");
        let replacement = mod_file(8, "same-name.zip");
        let file_set = FileSetRecipe {
            selected_file_ids: vec![installed.id],
            selected_file_names: vec![installed.file_name.clone()],
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&installed)],
            ..Default::default()
        };

        assert!(browse_review_file_is_installed(&installed, &file_set));
        assert!(!browse_review_file_is_installed(&replacement, &file_set));
    }

    #[test]
    fn fresh_multi_file_prompt_tracks_only_each_queued_file() {
        let body = mod_file(1, "covencarlottabody.zip");
        let hair = mod_file(2, "covencarlottahair.zip");
        let selected = vec![body.clone(), hair.clone()];

        let tracked = browse_selected_files_for_queued_download(&hair, &selected, None);

        assert_eq!(
            tracked
                .iter()
                .map(|file| (file.id, file.file_name.as_str()))
                .collect::<Vec<_>>(),
            vec![(hair.id, hair.file_name.as_str())]
        );
    }

    #[test]
    fn update_prompt_keeps_full_selected_file_set_for_target_entry() {
        let body = mod_file(1, "covencarlottabody.zip");
        let hair = mod_file(2, "covencarlottahair.zip");
        let selected = vec![body.clone(), hair.clone()];

        let tracked =
            browse_selected_files_for_queued_download(&hair, &selected, Some("local-mod-id"));

        assert_eq!(
            tracked
                .iter()
                .map(|file| (file.id, file.file_name.as_str()))
                .collect::<Vec<_>>(),
            selected
                .iter()
                .map(|file| (file.id, file.file_name.as_str()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn exact_source_matching_supports_metadata_only_file_sets() {
        let source = mod_file(7, "author_hash_v1.zip");
        let unrelated = mod_file(8, "different_mod.zip");
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&source)],
            ..Default::default()
        };

        let matches = browse_exact_source_matches(&file_set, &[unrelated, source.clone()]);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, source.id);
        assert_eq!(matches[0].file_name, source.file_name);
    }

    #[test]
    fn exact_source_matching_does_not_accept_an_unrelated_sole_file() {
        let source = mod_file(7, "author_hash_v1.zip");
        let unrelated = mod_file(8, "different_mod.zip");
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&source)],
            ..Default::default()
        };

        assert!(browse_exact_source_matches(&file_set, &[unrelated]).is_empty());
    }

    #[test]
    fn browse_display_indices_preserve_order_and_unsafe_filtering() {
        let cards = vec![
            browse_card(11, false),
            browse_card(22, true),
            browse_card(33, false),
            browse_card(44, true),
        ];

        assert_eq!(
            browse_card_indices_for_display(&cards, UnsafeContentMode::HideShowCounter),
            vec![0, 2]
        );
        assert_eq!(
            browse_card_indices_for_display(&cards, UnsafeContentMode::Show),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn installed_mod_snapshot_is_game_specific_and_refreshable() {
        let cards = vec![
            browse_card(101, false),
            BrowseCard {
                game_id: "game-b".to_string(),
                ..browse_card(101, false)
            },
        ];
        let before_links = [("game", 101), ("game-b", 101)];
        let mut before = BrowseInstalledModSnapshot::default();
        assert!(before.lookup(&cards[0], before_links.as_slice()));
        assert!(before.lookup(&cards[1], before_links.as_slice()));
        assert!(before
            .seen_by_game
            .get("game")
            .is_some_and(|ids| ids.contains(&101)));
        assert!(before
            .seen_by_game
            .get("game-b")
            .is_some_and(|ids| ids.contains(&101)));
        assert!(!before
            .seen_by_game
            .get("game")
            .is_some_and(|ids| ids.contains(&202)));

        let refreshed_cards = vec![BrowseCard {
            id: 202,
            ..cards[0].clone()
        }];
        let refreshed_links = [("game", 202), ("game-b", 101)];
        let mut after = BrowseInstalledModSnapshot::default();
        assert!(after.lookup(&refreshed_cards[0], refreshed_links.as_slice()));
        assert!(after
            .seen_by_game
            .get("game")
            .is_some_and(|ids| ids.contains(&202)));
        assert!(!after
            .seen_by_game
            .get("game")
            .is_some_and(|ids| ids.contains(&101)));
    }

    #[test]
    fn installed_mod_snapshot_deduplicates_candidates_and_stops_after_matches() {
        let cards = vec![
            browse_card(3, false),
            browse_card(3, false),
            BrowseCard {
                game_id: "other-game".to_string(),
                ..browse_card(8, false)
            },
        ];
        let links: Vec<(String, u64)> = (0..5_000)
            .map(|id| ("game".to_string(), id))
            .collect();
        let mut links = links;
        links.insert(8, ("other-game".to_string(), 8));
        let mut snapshot = BrowseInstalledModSnapshot::default();
        assert!(snapshot.lookup(&cards[0], links.as_slice()));
        let scanned_after_first = snapshot.scanned_until;
        assert!(snapshot.lookup(&cards[1], links.as_slice()));
        assert_eq!(snapshot.scanned_until, scanned_after_first);
        assert!(snapshot.lookup(&cards[2], links.as_slice()));

        assert_eq!(snapshot.scanned_until, 9);
        assert!(snapshot
            .seen_by_game
            .get("game")
            .is_some_and(|ids| ids == &HashSet::from([0, 1, 2, 3, 4, 5, 6, 7])));
        assert!(snapshot
            .seen_by_game
            .get("other-game")
            .is_some_and(|ids| ids == &HashSet::from([8])));
    }

    #[test]
    fn installed_mod_snapshot_scans_all_links_when_candidates_are_missing() {
        let cards = vec![browse_card(50_000, false)];
        let links: Vec<(String, u64)> = (0..128)
            .map(|id| ("game".to_string(), id))
            .collect();
        let mut snapshot = BrowseInstalledModSnapshot::default();
        assert!(!snapshot.lookup(&cards[0], links.as_slice()));

        assert_eq!(snapshot.scanned_until, links.len());
        assert!(snapshot.seen_by_game.contains_key("game"));
    }

    #[test]
    fn arc_detail_clone_shares_heavy_profile_until_translation_updates_it() {
        let original = BrowseDetailCache {
            profile: gamebanana::ProfileResponse {
                id: 77,
                name: "Original".to_string(),
                html_text: Some("long original description".to_string()),
                ..Default::default()
            },
            is_tool: false,
            profile_cached_at: None,
            markdown: "long original description".to_string(),
            unsafe_content: false,
            updates: BrowseUpdatesState::Unrequested,
            translated_profile: None,
            translation_lang: None,
            translation_loading: false,
        };
        let mut writable = Arc::new(original);
        let reader = Arc::clone(&writable);
        let original_ptr = Arc::as_ptr(&writable);

        let translated = gamebanana::ProfileResponse {
            id: 77,
            name: "Translated".to_string(),
            html_text: Some("translated description".to_string()),
            ..Default::default()
        };
        let detail = Arc::make_mut(&mut writable);
        detail.translated_profile = Some(translated);
        detail.translation_lang = Some("id".to_string());
        detail.updates = BrowseUpdatesState::Loading;

        assert_eq!(Arc::as_ptr(&reader), original_ptr);
        assert_ne!(Arc::as_ptr(&writable), original_ptr);
        assert_eq!(browse_detail_profile(&reader).name.as_str(), "Original");
        assert_eq!(browse_detail_profile(&writable).name.as_str(), "Translated");
        assert_eq!(writable.translation_lang.as_deref(), Some("id"));
        assert!(matches!(&reader.updates, BrowseUpdatesState::Unrequested));
        assert!(matches!(&writable.updates, BrowseUpdatesState::Loading));
    }

    #[test]
    #[ignore = "manual release performance comparison"]
    fn browse_grid_copy_and_installed_membership_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;

        let cards: Vec<BrowseCard> = (0..1_000)
            .map(|id| browse_card(id, id % 17 == 0))
            .collect();
        let page_cards: Vec<BrowseCard> = cards.iter().take(24).cloned().collect();

        fn old_pass(cards: &[BrowseCard], local_links: &[(String, u64)]) -> (Vec<u64>, Vec<bool>) {
            let displayed: Vec<BrowseCard> = cards
                .iter()
                .filter(|card| !card.unsafe_content)
                .cloned()
                .collect();
            let visible = displayed.iter().take(24);
            let ids = visible.clone().map(|card| card.id).collect();
            let installed = visible
                .map(|card| {
                    local_links
                        .iter()
                        .any(|(game_id, mod_id)| game_id == &card.game_id && *mod_id == card.id)
                })
                .collect();
            black_box(&displayed);
            (ids, installed)
        }

        fn new_pass(cards: &[BrowseCard], local_links: &[(String, u64)]) -> (Vec<u64>, Vec<bool>) {
            let indices = browse_card_indices_for_display(cards, UnsafeContentMode::HideShowCounter);
            let visible_cards: Vec<BrowseCard> = indices
                .iter()
                .take(24)
                .filter_map(|index| cards.get(*index).cloned())
                .collect();
            let mut snapshot = BrowseInstalledModSnapshot::default();
            let output = (
                visible_cards.iter().map(|card| card.id).collect(),
                visible_cards
                    .iter()
                    .map(|card| snapshot.lookup(card, local_links))
                    .collect(),
            );
            black_box((&indices, &visible_cards, &snapshot));
            output
        }

        let early_links: Vec<(String, u64)> = (0..5_000)
            .map(|id| ("game".to_string(), id))
            .collect();
        let missing_links: Vec<(String, u64)> = (10_000..15_000)
            .map(|id| ("game".to_string(), id))
            .collect();
        let mut late_links: Vec<(String, u64)> = (10_000..14_976)
            .map(|id| ("game".to_string(), id))
            .collect();
        late_links.extend((0..24).map(|id| ("game".to_string(), id)));

        let scenarios = [
            ("early-hit", &early_links),
            ("late-hit", &late_links),
            ("missing", &missing_links),
        ];
        const WARMUP_PASSES: usize = 10;
        const MEASURED_PASSES: usize = 100;

        for (shape, benchmark_cards) in [
            ("1000-card", cards.as_slice()),
            ("24-card-page", page_cards.as_slice()),
        ] {
            for &(name, local_links) in &scenarios {
                for _ in 0..WARMUP_PASSES {
                    black_box(old_pass(benchmark_cards, local_links));
                    black_box(new_pass(benchmark_cards, local_links));
                }

                let old_started = Instant::now();
                let mut old_output = None;
                for _ in 0..MEASURED_PASSES {
                    old_output = Some(black_box(old_pass(benchmark_cards, local_links)));
                }
                let old_elapsed = old_started.elapsed();

                let new_started = Instant::now();
                let mut new_output = None;
                for _ in 0..MEASURED_PASSES {
                    new_output = Some(black_box(new_pass(benchmark_cards, local_links)));
                }
                let new_elapsed = new_started.elapsed();

                assert_eq!(old_output, new_output);
                println!(
                    "browse benchmark {shape}/{name}: old {:.2} us/pass, new {:.2} us/pass",
                    old_elapsed.as_secs_f64() * 1_000_000.0 / MEASURED_PASSES as f64,
                    new_elapsed.as_secs_f64() * 1_000_000.0 / MEASURED_PASSES as f64,
                );
            }
        }

        let detail = BrowseDetailCache {
            profile: gamebanana::ProfileResponse {
                id: 77,
                name: "benchmark".to_string(),
                html_text: Some("description ".repeat(1_000)),
                ..Default::default()
            },
            is_tool: false,
            profile_cached_at: None,
            markdown: "markdown ".repeat(1_000),
            unsafe_content: false,
            updates: BrowseUpdatesState::Unrequested,
            translated_profile: None,
            translation_lang: None,
            translation_loading: false,
        };
        let detail = Arc::new(detail);
        for _ in 0..WARMUP_PASSES {
            black_box((*detail).clone());
            black_box(Arc::clone(&detail));
        }
        let deep_started = Instant::now();
        for _ in 0..MEASURED_PASSES {
            black_box((*detail).clone());
        }
        let deep_elapsed = deep_started.elapsed();
        let shared_started = Instant::now();
        for _ in 0..MEASURED_PASSES {
            black_box(Arc::clone(&detail));
        }
        let shared_elapsed = shared_started.elapsed();
        println!(
            "browse detail benchmark: deep clone {:.2} us/pass, Arc clone {:.2} us/pass",
            deep_elapsed.as_secs_f64() * 1_000_000.0 / MEASURED_PASSES as f64,
            shared_elapsed.as_secs_f64() * 1_000_000.0 / MEASURED_PASSES as f64,
        );
    }
}

async fn download_browse_file_to_cache_with_retries(
    client: &ClientWithMiddleware,
    portable: &PortablePaths,
    job: &BrowseDownloadJob,
    cancel: &Arc<AtomicBool>,
    progress: Arc<RwLock<DownloadProgress>>,
    handle: tokio::runtime::Handle,
) -> Result<u64> {
    let mut retry_delay = BROWSE_DOWNLOAD_RETRY_INITIAL_DELAY;
    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 1..=BROWSE_DOWNLOAD_MAX_ATTEMPTS {
        if cancel.load(Ordering::Relaxed) {
            bail!(importing::CANCELLED_ERROR);
        }

        match download_browse_file_to_cache_once(
            client,
            portable,
            job,
            cancel,
            Arc::clone(&progress),
            handle.clone(),
        )
        .await
        {
            Ok(byte_size) => return Ok(byte_size),
            Err(err) if err.to_string() == importing::CANCELLED_ERROR => return Err(err),
            Err(err) => {
                last_error = Some(err);
                if attempt >= BROWSE_DOWNLOAD_MAX_ATTEMPTS {
                    break;
                }
                if let Ok(mut guard) = progress.write() {
                    guard.speed = 0.0;
                    guard.bytes_since_last = 0;
                    guard.last_update = std::time::Instant::now();
                }
                sleep_cancelable(retry_delay, cancel).await?;
                retry_delay = (retry_delay * 2).min(BROWSE_DOWNLOAD_RETRY_MAX_DELAY);
            }
        }
    }
    let err = last_error
        .map(|err| {
            err.context(format!(
                "download failed after {BROWSE_DOWNLOAD_MAX_ATTEMPTS} attempts"
            ))
        })
        .unwrap_or_else(|| {
            anyhow!("download failed after {BROWSE_DOWNLOAD_MAX_ATTEMPTS} attempts")
        });
    Err(err)
}

async fn download_browse_file_to_cache_once(
    client: &ClientWithMiddleware,
    portable: &PortablePaths,
    job: &BrowseDownloadJob,
    cancel: &Arc<AtomicBool>,
    progress: Arc<RwLock<DownloadProgress>>,
    handle: tokio::runtime::Handle,
) -> Result<u64> {
    let partial_path = browse_download_partial_path(job);
    if let Some(parent) = partial_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let expected_size = job.total_size;
    let mut resume_from = tokio::fs::metadata(&partial_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    if expected_size.is_some_and(|expected| resume_from > expected) {
        let _ = tokio::fs::remove_file(&partial_path).await;
        resume_from = 0;
    }

    if expected_size.is_some_and(|expected| resume_from == expected) && resume_from > 0 {
        return cache_completed_browse_download(
            portable,
            &job.cache_key,
            &partial_path,
            job.cache_limit_bytes,
            expected_size,
            cancel,
            handle,
        )
        .await;
    }

    if let Ok(mut guard) = progress.write() {
        guard.downloaded = resume_from;
        guard.total = expected_size;
        guard.speed = 0.0;
        guard.bytes_since_last = 0;
        guard.last_update = std::time::Instant::now();
    }

    let mut request = client.get(&job.url);
    if resume_from > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }

    let response = request.send().await?;
    let status = response.status();
    let mut append = false;

    if resume_from > 0 {
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            validate_content_range(response.headers(), resume_from, expected_size)?;
            append = true;
        } else if status == reqwest::StatusCode::OK {
            let _ = tokio::fs::remove_file(&partial_path).await;
            resume_from = 0;
            if let Ok(mut guard) = progress.write() {
                guard.downloaded = 0;
            }
        } else {
            let _ = tokio::fs::remove_file(&partial_path).await;
            response.error_for_status()?;
            bail!("server rejected resumed download with status {status}");
        }
    }

    let response = response.error_for_status()?;
    let response_len = response.content_length();
    if let Ok(mut guard) = progress.write() {
        guard.total = if append {
            response_len
                .map(|len| resume_from.saturating_add(len))
                .or(expected_size)
        } else {
            response_len.or(expected_size)
        };
    }

    let mut file = if append {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial_path)
            .await?
    } else {
        tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&partial_path)
            .await?
    };

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            bail!(importing::CANCELLED_ERROR);
        }
        let chunk = chunk?;
        if chunk.is_empty() {
            continue;
        }
        file.write_all(&chunk).await?;

        if let Ok(mut guard) = progress.write() {
            let read = chunk.len() as u64;
            guard.downloaded = guard.downloaded.saturating_add(read);
            guard.bytes_since_last = guard.bytes_since_last.saturating_add(read);
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(guard.last_update);
            if elapsed.as_millis() >= 500 {
                guard.speed = guard.bytes_since_last as f64 / elapsed.as_secs_f64();
                guard.bytes_since_last = 0;
                guard.last_update = now;
            }
        }
    }
    file.flush().await?;

    let actual_size = tokio::fs::metadata(&partial_path).await?.len();
    if let Some(expected) = expected_size {
        if actual_size != expected {
            bail!("download ended at {actual_size} bytes, expected {expected}");
        }
    }

    cache_completed_browse_download(
        portable,
        &job.cache_key,
        &partial_path,
        job.cache_limit_bytes,
        expected_size,
        cancel,
        handle,
    )
    .await
}

async fn cache_completed_browse_download(
    portable: &PortablePaths,
    cache_key: &str,
    partial_path: &Path,
    cache_limit_bytes: u64,
    expected_size: Option<u64>,
    cancel: &Arc<AtomicBool>,
    handle: tokio::runtime::Handle,
) -> Result<u64> {
    if cancel.load(Ordering::Relaxed) {
        bail!(importing::CANCELLED_ERROR);
    }

    let byte_size = tokio::fs::metadata(partial_path).await?.len();
    if let Some(expected) = expected_size {
        if byte_size != expected {
            bail!("download cache size mismatch: expected {expected}, got {byte_size}");
        }
    }

    let portable_for_promotion = portable.clone();
    let key = cache_key.to_string();
    let partial_path = partial_path.to_path_buf();
    handle
        .spawn_blocking(move || {
            persistence::cache_promote_file(
                &portable_for_promotion,
                &key,
                &partial_path,
                cache_limit_bytes,
            )
        })
        .await
        .map_err(|err| anyhow!("download cache promotion join error: {err}"))??;
    Ok(byte_size)
}

async fn sleep_cancelable(delay: Duration, cancel: &Arc<AtomicBool>) -> Result<()> {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!(importing::CANCELLED_ERROR);
        }
        tokio::select! {
            _ = &mut sleep => return Ok(()),
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
}

async fn verify_browse_download_md5(
    portable: &PortablePaths,
    job: &BrowseDownloadJob,
    cancel: &Arc<AtomicBool>,
    handle: &tokio::runtime::Handle,
) -> Result<()> {
    let Some(expected) = job.md5.clone().filter(|md5| !md5.trim().is_empty()) else {
        return Ok(());
    };
    let path = persistence::cache_file_path(&job.cache_key);
    if !path.is_file() {
        return Ok(());
    }
    let cancel_for_hash = Arc::clone(cancel);
    let actual = handle
        .spawn_blocking(move || importing::bundle::file_md5_hex(&path, &cancel_for_hash))
        .await??;
    if actual.eq_ignore_ascii_case(expected.trim()) {
        return Ok(());
    }
    let portable = portable.clone();
    let key = job.cache_key.clone();
    let partial = browse_download_partial_path(job);
    let _ = handle
        .spawn_blocking(move || {
            let _ = std::fs::remove_file(partial);
            persistence::cache_remove(&portable, &key)
        })
        .await;
    bail!("the downloaded file does not match GameBanana's checksum, try again")
}

fn browse_download_partial_path(job: &BrowseDownloadJob) -> PathBuf {
    persistence::runtime_temp_downloads_partial_dir()
        .join(format!("{}.partial", hash64_hex(job.cache_key.as_bytes())))
}

fn validate_content_range(
    headers: &reqwest::header::HeaderMap,
    expected_start: u64,
    expected_total: Option<u64>,
) -> Result<()> {
    let Some(value) = headers.get(reqwest::header::CONTENT_RANGE) else {
        bail!("resumed download response did not include Content-Range");
    };
    let value = value
        .to_str()
        .map_err(|_| anyhow!("resumed download returned invalid Content-Range"))?;
    let Some(range) = value.strip_prefix("bytes ") else {
        bail!("resumed download returned unsupported Content-Range: {value}");
    };
    let Some((span, total)) = range.split_once('/') else {
        bail!("resumed download returned malformed Content-Range: {value}");
    };
    let Some((start, _end)) = span.split_once('-') else {
        bail!("resumed download returned malformed Content-Range: {value}");
    };
    let start = start
        .parse::<u64>()
        .map_err(|_| anyhow!("resumed download returned invalid Content-Range start"))?;
    if start != expected_start {
        bail!("resumed download started at byte {start}, expected {expected_start}");
    }
    if let Some(expected_total) = expected_total {
        let total = total
            .parse::<u64>()
            .map_err(|_| anyhow!("resumed download returned invalid Content-Range total"))?;
        if total != expected_total {
            bail!("resumed download total was {total}, expected {expected_total}");
        }
    }
    Ok(())
}
