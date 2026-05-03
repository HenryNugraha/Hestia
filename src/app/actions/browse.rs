impl HestiaApp {
    fn sanitized_preferred_browse_title_name(&self, raw_title: Option<&str>) -> Option<String> {
        let title = raw_title?.trim();
        if title.is_empty() {
            return None;
        }
        let sanitized = sanitize_folder_name(title);
        if sanitized == "Imported Mod" || sanitized.chars().all(|c| c == '_') {
            None
        } else {
            Some(sanitized)
        }
    }

    fn preferred_browse_folder_name(
        &self,
        raw_title: Option<&str>,
        fallback_name: &str,
    ) -> String {
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
        self.browse_state.cards.clear();
        self.browse_state.total_count = None;
        self.browse_state.next_page = 1;
        self.browse_state.has_more = true;
        self.browse_state.loading_page = false;
        self.browse_state.refresh_page_cache_for_session = false;
        self.browse_state.selected_mod_id = None;
        self.browse_detail_open = false;
        self.browse_state.file_prompt = None;
        self.cancel_browse_full_image_requests();
    }

    fn browse_image_cache_key(url: &str) -> String {
        format!("img:{}", hash64_hex(url.as_bytes()))
    }

    fn browse_download_cache_key(mod_id: u64, file_name: &str) -> String {
        let safe_name = sanitize_folder_name(file_name);
        format!("dl:{mod_id}:{safe_name}")
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
        if self.browse_state.cards.is_empty() && !self.browse_state.loading_page {
            self.request_browse_page_with_mode(1, true);
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
        }
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        self.browse_state.loading_page = true;
        let request = BrowseRequest::FetchPage {
            nonce: self.browse_request_nonce,
            generation: self.browse_page_generation,
            game_id: game.definition.id.clone(),
            query: self.current_browse_query(),
            page,
            browse_sort: self.state.browse_sort,
            search_sort: self.state.search_sort,
            force_refresh,
        };
        let _ = self.browse_request_tx.send(request);
    }

    fn restart_browse_query(&mut self) {
        let Some(game) = self.selected_game().cloned() else {
            return;
        };
        self.reset_browse_for_game(&game.definition.id);
        self.browse_state.refresh_page_cache_for_session = true;
        self.browse_state.active_query = self.current_browse_query();
        self.request_browse_page_with_mode(1, true);
    }

    fn request_browse_detail(&mut self, mod_id: u64) {
        if self.browse_state.details.contains_key(&mod_id)
            || self.browse_state.loading_details.contains(&mod_id)
        {
            return;
        }
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        self.browse_state.loading_details.insert(mod_id);
        let cached_profile_json = if self.browse_state.refresh_page_cache_for_session {
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
        let _ = self.browse_request_tx.send(BrowseRequest::FetchDetail {
            nonce: self.browse_request_nonce,
            mod_id,
            force_refresh: self.browse_state.refresh_page_cache_for_session,
            cached_profile_json,
        });
    }

    fn request_browse_updates(&mut self, mod_id: u64) {
        let Some(detail) = self.browse_state.details.get_mut(&mod_id) else {
            return;
        };
        if !matches!(detail.updates, BrowseUpdatesState::Unrequested) {
            return;
        }
        detail.updates = BrowseUpdatesState::Loading;
        self.browse_request_nonce = self.browse_request_nonce.wrapping_add(1);
        let _ = self.browse_request_tx.send(BrowseRequest::FetchUpdates {
            nonce: self.browse_request_nonce,
            mod_id,
            force_refresh: self.browse_state.refresh_page_cache_for_session,
        });
    }

    fn open_browse_detail(&mut self, mod_id: u64) {
        self.browse_state.selected_mod_id = Some(mod_id);
        self.browse_detail_open = true;
        self.browse_detail_focus_requested = true;
        self.request_browse_detail(mod_id);
        self.request_browse_updates(mod_id);
        self.queue_detail_preview_images(mod_id);
        self.begin_full_image_prefetch(mod_id);
    }

    fn open_linked_mod_in_browse(&mut self, mod_id: u64) {
        self.current_view = ViewMode::Browse;
        self.mod_detail_editing = false;
        self.pending_browse_open_mod_id = Some(mod_id);
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

        let urls: Vec<String> = self.browse_state.details.get(&mod_id)
            .and_then(|detail| detail.profile.preview_media.as_ref())
            .map(|preview| {
                preview.images.iter().take(3)
                    .map(gamebanana::full_image_url)
                    .collect()
            })
            .unwrap_or_default();

        for (idx, full_url) in urls.into_iter().enumerate() {
            self.queue_browse_image_full(full_url, Some(self.browse_detail_generation), 5 + (idx as u32));
        }
    }

    fn cancel_browse_full_image_requests(&mut self) {
        self.browse_image_queue.retain(|job| job.cancel_key.is_none());
        for inflight in self.browse_image_inflight.values() {
            if inflight.cancel_key.is_some() {
                inflight.cancel.store(true, Ordering::Relaxed);
            }
        }
        self.browse_state.screenshot_overlay = None;
    }

fn queue_browse_image(&mut self, url: String, cancel_key: Option<u64>, skip_texture: bool, priority: u32) {
    self.queue_browse_image_with_profile(url, cancel_key, skip_texture, ThumbnailProfile::Card, priority);
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
        
        // 1. If we need texture and thumb already exists, done.
        if !skip_texture && self.browse_thumb_textures.contains_key(&thumb_texture_key) {
            return;
        }

        // 2. If we just want cache and file exists, done.
        if skip_texture
            && persistence::cache_exists(&self.portable, &cache_key).unwrap_or(false)
        {
            return;
        }

        // 3. Check inflight
        if let Some(inflight) = self.browse_image_inflight.get(&hash_key) {
            if skip_texture {
                // Any inflight job will download the file.
                return;
            } else if !inflight.skip_texture && !inflight.load_full {
                // We need texture, and inflight job is loading texture.
                return;
            }
            // If we need texture but inflight is download-only, we must proceed to queue/add
            // to ensure we get a loaded texture result.
        }

        // 4. Check queue
        if let Some(existing) = self.browse_image_queue.iter_mut().find(|j| j.texture_key == hash_key) {
            if !skip_texture && existing.skip_texture {
                // Upgrade existing download-only job to load texture
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
        if self.mod_full_textures.contains_key(texture_key)
            || self.browse_image_textures.contains_key(texture_key)
        {
            self.touch_texture(TextureKind::ModFull, texture_key, 3);
            return;
        }

        let mut local_load = None;
        if let Some(mod_id) = self.selected_mod_id.clone() {
            if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_id) {
                let prefix = format!("my-mod-shot-{}-", mod_entry.id);
                if let Some(suffix) = texture_key.strip_prefix(&prefix) {
                    if let Ok(index) = suffix.parse::<usize>() {
                        if let Some(rel) = mod_entry.metadata.user.screenshots.get(index) {
                            local_load = Some((texture_key.to_string(), mod_entry.root_path.join(rel)));
                        }
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

        if let Some(url) = self
            .my_mod_overlay_images
            .iter()
            .find_map(|item| (item.texture_key == texture_key).then(|| item.url.as_ref()).flatten())
        {
            self.queue_browse_image_full(url.clone(), Some(self.browse_detail_generation), 1);
        }
    }

    fn browse_cards_for_display(&self) -> Vec<&BrowseCard> {
        self.browse_state
            .cards
            .iter()
            .filter(|card| {
                !(matches!(
                    self.state.unsafe_content_mode,
                    UnsafeContentMode::HideNoCounter | UnsafeContentMode::HideShowCounter
                ) && card.unsafe_content)
            })
            .collect()
    }

    fn is_browse_mod_installed(&self, card: &BrowseCard) -> bool {
        let game_id = &card.game_id;
        self.state.mods.iter().any(|m| {
            m.game_id == *game_id
                && m
                    .source
                    .as_ref()
                    .and_then(|s| s.gamebanana.as_ref())
                    .is_some_and(|link| link.mod_id == card.id)
        })
    }

    fn hidden_unsafe_browse_count(&self) -> usize {
        if self.state.unsafe_content_mode != UnsafeContentMode::HideShowCounter {
            return 0;
        }
        self.browse_state
            .cards
            .iter()
            .filter(|card| card.unsafe_content)
            .count()
    }

    fn should_censor_unsafe(&self) -> bool {
        self.state.unsafe_content_mode == UnsafeContentMode::Censor
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
                    _nonce: _,
                    generation,
                    game_id,
                    query,
                    page,
                    payload,
                } => {
                    if generation != self.browse_page_generation {
                        continue;
                    }
                    if self.browse_state.active_game_id.as_deref() != Some(game_id.as_str()) {
                        continue;
                    }
                    if self.current_browse_query() != query {
                        continue;
                    }
                    self.browse_state.loading_page = false;
                    self.browse_state.active_query = query.clone();
                    self.browse_state.total_count = Some(payload.metadata.record_count);
                    self.browse_state.has_more =
                        !payload.metadata.is_complete && !payload.records.is_empty();
                    self.browse_state.next_page = page + 1;
                    if page == 1 {
                        self.browse_state.cards.clear();
                    }
                    for record in payload.records {
                        if self.browse_state.cards.iter().any(|item| item.id == record.id) {
                            continue;
                        }
                        let thumbnail_url = record
                            .preview_media
                            .as_ref()
                            .and_then(|media| media.images.first())
                            .and_then(gamebanana::thumbnail_url);
                        if let Some(url) = thumbnail_url.clone() {
                            self.queue_browse_image_with_profile(
                                url,
                                None,
                                false,
                                ThumbnailProfile::Card,
                            50, // Base grid priority
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
                BrowseEvent::PageWarning { generation, warning, .. } => {
                    if generation != self.browse_page_generation {
                        continue;
                    }
                    self.report_warn(
                        format!("browse page refresh failed; using cached results: {warning}"),
                        Some("Connection failed"),
                    );
                }
                BrowseEvent::PageFailed { generation, error, .. } => {
                    if generation != self.browse_page_generation {
                        continue;
                    }
                    self.browse_state.loading_page = false;
                    self.report_error_message(
                        format!("browse page failed: {error}"),
                        Some("Browse failed"),
                    );
                }
                BrowseEvent::DetailLoaded { mod_id, profile, .. } => {
                    self.browse_state.loading_details.remove(&mod_id);
                    // let html = profile.html_text.as_deref().unwrap_or_default();
                    // // Clean up HTML: Remove detective placeholder and strip attributes from img tags to ensure markdown conversion
                    // let placeholder_re = Regex::new(r#"<img[^>]+src="https://images.gamebanana.com/static/img/mascots/detective.png"[^>]*>"#).unwrap();
                    // let img_attr_re = Regex::new(r#"(?i)<img\s+[^>]*?src=(["'])(.+?)\1[^>]*?>"#).unwrap();
                    // let sanitized_html = img_attr_re.replace_all(&placeholder_re.replace_all(html, ""), r#"<img src="$2">"#);
                    // let markdown = html2md::parse_html(&sanitized_html);
                    let markdown = prepare_markdown_for_display(
                        profile.html_text.as_deref().unwrap_or_default(),
                        None,
                        Some(mod_id),
                        &self.portable,
                    );
                    self.prewarm_markdown_images(&markdown);

                    let mut description_image_keys = HashSet::new();
                    if let Ok(image_regex) = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)") {
                        for cap in image_regex.captures_iter(&markdown) {
                            if let Some(url_match) = cap.get(2) {
                                let url = url_match.as_str();
                                if url.starts_with("http") {
                                    description_image_keys.insert(hash64_hex(url.as_bytes()));
                                    description_image_keys.insert(Self::browse_thumb_texture_key(url, ThumbnailProfile::Card));
                                    self.queue_browse_image(
                                        url.to_string(),
                                        Some(self.browse_detail_generation),
                                        true,
                                    100, // Description images background
                                    );
                                }
                            }
                        }
                    }

                    let cache = BrowseDetailCache {
                        profile: profile.clone(),
                        markdown,
                        unsafe_content: !profile.content_ratings.is_empty(),
                        updates: BrowseUpdatesState::Unrequested,
                    };
                    self.browse_state.details.insert(mod_id, cache);
                    if let Some(card) = self.browse_state.cards.iter_mut().find(|card| card.id == mod_id) {
                        card.download_count = Some(profile.download_count);
                        card.unsafe_content = !profile.content_ratings.is_empty();
                    }
                    self.queue_detail_preview_images(mod_id);
                    if self.browse_state.selected_mod_id == Some(mod_id) {
                        self.begin_full_image_prefetch(mod_id);
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
                }
                BrowseEvent::DetailWarning { mod_id, warning, .. } => {
                    if self.browse_state.selected_mod_id == Some(mod_id) {
                        self.report_warn(
                            format!("browse detail refresh failed for mod {mod_id}; using cached details: {warning}"),
                            Some("Connection failed"),
                        );
                    } else {
                        self.report_warn(
                            format!("browse detail refresh failed for mod {mod_id}; using cached details: {warning}"),
                            None,
                        );
                    }
                }
                BrowseEvent::DetailFailed { mod_id, error, .. } => {
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
                    self.report_error_message(
                        format!("browse detail failed for mod {mod_id}: {error}"),
                        Some("Browse detail failed"),
                    );
                }
                BrowseEvent::UpdatesLoaded { mod_id, updates, .. } => {
                    if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
                        let entries = updates
                            .records
                            .into_iter()
                            .filter(|record| {
                                !record.is_private && !record.is_trashed && !record.is_withheld
                            })
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
                            .collect::<Vec<_>>();
                        detail.updates = if entries.is_empty() {
                            BrowseUpdatesState::Empty
                        } else {
                            BrowseUpdatesState::Loaded(entries)
                        };
                    }
                }
                BrowseEvent::UpdatesWarning { mod_id, warning, .. } => {
                    self.report_warn(
                        format!("browse updates refresh failed for mod {mod_id}; using cached updates: {warning}"),
                        None,
                    );
                }
                BrowseEvent::UpdatesFailed { mod_id, error, .. } => {
                    if let Some(detail) = self.browse_state.details.get_mut(&mod_id) {
                        detail.updates = BrowseUpdatesState::Failed("Could not load updates".to_string());
                    }
                    self.report_error_message(
                        format!("browse updates failed for mod {mod_id}: {error}"),
                        None,
                    );
                }
            }
        }
    }

    fn process_browse_image_queue(&mut self) {
        // CONTEXTUAL THROTTLING: Suspend background work to prioritize current user focus
        let now = Instant::now();
        self.browse_image_retry_after
            .retain(|_, retry_after| *retry_after > now);
        let mut allowed_keys = HashSet::new();
        let mut focus_mode = false;

        if let Some(overlay) = &self.browse_state.screenshot_overlay {
            focus_mode = true;
            allowed_keys.insert(overlay.texture_key.clone());

            // Allow neighbors for hi-res preloading (Browse view)
            if let Some(mod_id) = self.browse_state.selected_mod_id {
                if let Some(detail) = self.browse_state.details.get(&mod_id) {
                    if let Some(preview) = &detail.profile.preview_media {
                        let images = &preview.images;
                        if let Some(idx) = images.iter().position(|img| hash64_hex(gamebanana::full_image_url(img).as_bytes()) == overlay.texture_key) {
                            if idx + 1 < images.len() {
                                allowed_keys.insert(hash64_hex(gamebanana::full_image_url(&images[idx + 1]).as_bytes()));
                            }
                            if idx > 0 {
                                allowed_keys.insert(hash64_hex(gamebanana::full_image_url(&images[idx - 1]).as_bytes()));
                            }
                        }
                    }
                }
            }
            // Allow neighbors for local hi-res preloading (My Mods view)
            if let Some(mod_id) = &self.selected_mod_id {
                let prefix = format!("my-mod-shot-{mod_id}-");
                if overlay.texture_key.starts_with(&prefix) {
                    if let Some(mod_entry) = self.state.mods.iter().find(|m| &m.id == mod_id) {
                        if let Some(suffix) = overlay.texture_key.strip_prefix(&prefix) {
                            if let Ok(idx) = suffix.parse::<usize>() {
                                if idx + 1 < mod_entry.metadata.user.screenshots.len() {
                                    allowed_keys.insert(format!("{prefix}{}", idx + 1));
                                }
                                if idx > 0 {
                                    allowed_keys.insert(format!("{prefix}{}", idx - 1));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Prioritize the queue based on UI proximity
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
        while self.browse_image_inflight.len() < max_parallel && i < self.browse_image_queue.len() {
            let job = &self.browse_image_queue[i];
            let job_key = &job.texture_key;
            if focus_mode && !allowed_keys.contains(job_key) {
                i += 1;
                continue;
            }

            let job = self.browse_image_queue.remove(i);
            self.browse_image_inflight.insert(
                job.texture_key.clone(),
                BrowseImageInflight {
                    cancel: job.cancel.clone(),
                    cancel_key: job.cancel_key,
                    skip_texture: job.skip_texture,
                    load_full: job.load_full,
                },
            );
            let _ = self.browse_image_request_tx.send(job);
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
                        "image request timed out; retrying in {}s: {}",
                        BROWSE_IMAGE_RETRY_COOLDOWN_SECS,
                        failure.url
                    ));
                } else {
                    self.log_warn(format!(
                        "image request failed; retrying in {}s: {}",
                        BROWSE_IMAGE_RETRY_COOLDOWN_SECS,
                        failure.url
                    ));
                }
                continue;
            }
            self.browse_image_retry_after.remove(&result.texture_key);
            if let Some(image_thumb) = result.image_thumb {
                self.pending_texture_uploads.push_back(PendingTextureUpload::BrowseThumb {
                    texture_key: result.thumb_texture_key,
                    image: image_thumb,
                });
            }
            if let Some(image_full) = result.image_full {
                self.pending_texture_uploads.push_back(PendingTextureUpload::BrowseFull {
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
        post_install_rename_to: Option<String>,
    ) {
        if !self.game_is_installed_or_configured(&game_id) {
            if let Some(task_id) = task_id {
                self.update_task_status(task_id, TaskStatus::Failed);
            }
            self.report_warn(
                "Game is not installed or configured.",
                Some("Install unavailable"),
            );
            return;
        }
        let task_id = task_id.unwrap_or_else(|| self.next_background_job_id());
        let title = file.file_name.clone();
        let size = Some(file.file_size);
        let cache_key = Self::browse_download_cache_key(mod_id, &file.file_name);
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
                if task.total_size.is_none() { task.total_size = size; }
            }
            self.update_task_status(task_id, TaskStatus::Queued);
        }
        self.pending_browse_install_safety
            .insert(task_id, unsafe_content);
        self.pending_browse_install_meta.insert(
            task_id,
            PendingBrowseInstallMeta {
                mod_id,
                game_id: game_id.clone(),
                selected_files,
                update_folder_name,
                update_target_mod_id,
                post_install_rename_to,
            },
        );
        self.browse_download_queue.push_back(BrowseDownloadJob {
            task_id,
            title,
            url: file.download_url.unwrap_or_default(),
            cache_key,
            file_name: file.file_name,
            cache_limit_bytes: self.cache_limit_bytes.load(Ordering::Relaxed),
            total_size: Some(file.file_size),
        });
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
            let progress = self.browse_download_inflight.get(&job.task_id).unwrap().progress.clone();
            let tx = self.browse_download_result_tx.clone();
            let portable = self.portable.clone();
            let client = self.runtime_services.http_client.clone();
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
                        return Ok::<u64, anyhow::Error>(bytes.len() as u64);
                    }
                    let bytes = download_to_bytes_async(&client, &job.url, &cancel, progress).await?;
                    let byte_size = bytes.len() as u64;
                    let portable_for_put = portable.clone();
                    let key = job.cache_key.clone();
                    let bytes_for_put = bytes.clone();
                    let max_bytes = job.cache_limit_bytes;
                    let _ = handle
                        .spawn_blocking(move || {
                            persistence::cache_put(
                                &portable_for_put,
                                &key,
                                "download",
                                &bytes_for_put,
                                max_bytes,
                            )
                        })
                        .await
                        .map_err(|err| anyhow!("download cache write join error: {err}"))??;
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
                    self.set_message_ok(format!("Downloaded: {title}"));
                    let install_game_id = self
                        .pending_browse_install_meta
                        .get(&task_id)
                        .map(|meta| meta.game_id.clone());
                    let gb_profile = if let Some(meta) = self.pending_browse_install_meta.get(&task_id) {
                        self.browse_state.details.get(&meta.mod_id).map(|d| Box::new(d.profile.clone()))
                    } else {
                        None
                    };
                    match self.write_cached_download_to_temp_archive(task_id, &cache_key, &file_name)
                    {
                        Ok(temp_archive) => {
                            let Some(game_id) = install_game_id else {
                                self.update_task_status(task_id, TaskStatus::Failed);
                                self.report_error_message(
                                    "Missing game context for install".to_string(),
                                    Some("Could not prepare install"),
                                );
                                continue;
                            };
                            self.enqueue_install_source_for_existing_task(
                                task_id,
                                game_id,
                                title,
                                ImportSource::Archive(temp_archive),
                                gb_profile,
                            );
                        }
                        Err(err) => {
                            self.update_task_status(task_id, TaskStatus::Failed);
                            self.report_error(err, Some("Could not prepare install"));
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
                        format!("download failed for {title}: {error}"),
                        Some("Download failed"),
                    );
                }
                BrowseDownloadEvent::Canceled { task_id, title } => {
                    self.browse_download_inflight.remove(&task_id);
                    self.mark_usage_counters_dirty();
                    self.update_task_status(task_id, TaskStatus::Canceled);
                    self.set_message_ok(format!("Download canceled: {title}"));
                }
            }
        }
    }

    fn resolve_browse_install_after_detail(&mut self, pending: PendingBrowseInstall) {
        if !self.game_is_installed_or_configured(&pending.game_id) {
            self.update_task_status(pending.task_id, TaskStatus::Failed);
            self.report_warn(
                "Game is not installed or configured.",
                Some("Install unavailable"),
            );
            return;
        }
        let Some(detail) = self.browse_state.details.get(&pending.mod_id).cloned() else {
            self.browse_state.pending_installs.push(pending);
            return;
        };
        let mut update_folder_name = pending.update_target_id.as_ref().and_then(|id| {
            self.state.mods.iter().find(|m| &m.id == id).map(|m| m.folder_name.clone())
        });
        let mut post_install_rename_to = None;
        if let Some(target_id) = pending.update_target_id.as_deref() {
            let preferred_name = self.preferred_browse_folder_name(
                self.browse_mod_title_for_install(pending.mod_id, Some(target_id)).as_deref(),
                update_folder_name.as_deref().unwrap_or("Imported Mod"),
            );
            if update_folder_name.as_deref() != Some(preferred_name.as_str()) {
                match self.rename_mod_folder(target_id, &preferred_name) {
                    Ok(()) => {
                        update_folder_name = Some(preferred_name);
                        self.save_state();
                    }
                    Err(_) => {
                        post_install_rename_to = Some(preferred_name);
                    }
                }
            }
        }

        let selectable: Vec<_> = detail
            .profile
            .files
            .iter()
            .filter(|file| file.download_url.is_some())
            .cloned()
            .collect();
                // Check if this is an update for an existing mod with a file set
        let existing_mod = self.state.mods.iter()
            .find(|m| {
                m.game_id == pending.game_id &&
                m.source.as_ref().and_then(|s| s.gamebanana.as_ref()).is_some_and(|l| l.mod_id == pending.mod_id)
            })
            .cloned();
        if let Some(existing_mod) = existing_mod {
            let file_set = existing_mod.source.as_ref().map(|source| &source.file_set);
            if let Some(file_set) = file_set {
            let baseline_ts = file_set.selected_files_meta.iter().map(|file| file.date_added).max();
            let newer_files: Vec<_> = baseline_ts
                .map(|baseline| {
                    selectable
                        .iter()
                        .filter(|file| file.date_added > baseline)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !newer_files.is_empty() {
                let mut newer_selected = newer_files.clone();
                newer_selected.sort_by_key(|file| std::cmp::Reverse(file.date_added));
                if !Self::should_show_local_change_update_prefs(&existing_mod) {
                    let selected_set = vec![newer_selected[0].clone()];
                    self.queue_browse_download(
                        pending.game_id,
                        pending.mod_id,
                        newer_selected[0].clone(),
                        selected_set,
                        Some(pending.task_id),
                        detail.unsafe_content,
                        update_folder_name,
                        pending.update_target_id.clone(),
                        post_install_rename_to.clone(),
                    );
                } else {
                    self.remove_task(pending.task_id);
                    self.browse_state.file_prompt = Some(BrowseFilePrompt {
                        mod_id: pending.mod_id,
                        game_id: pending.game_id,
                        files: selectable
                            .into_iter()
                            .map(|file| BrowseSelectableFile {
                                selected: file.date_added > baseline_ts.unwrap_or_default(),
                                file,
                            })
                            .collect(),
                        update_folder_name,
                        update_target_mod_id: pending.update_target_id.clone(),
                        post_install_rename_to: post_install_rename_to.clone(),
                    });
                }
                return;
            }
            if !file_set.selected_file_ids.is_empty() || !file_set.selected_file_names.is_empty() {
                let mut matches = Vec::new();
                // 1. Try matching by ID (strongest match)
                for old_id in &file_set.selected_file_ids {
                    if let Some(found) = selectable.iter().find(|f| f.id == *old_id) {
                        matches.push(found.clone());
                    }
                }
                // 2. For remaining names, try exact string match
                for old_name in &file_set.selected_file_names {
                    if !matches.iter().any(|m| &m.file_name == old_name) {
                        if let Some(found) = selectable.iter().find(|f| &f.file_name == old_name) {
                            matches.push(found.clone());
                        }
                    }
                }

                if !matches.is_empty() {
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
                                    selected: matches.iter().any(|matched| matched.id == file.id),
                                    file,
                                })
                                .collect(),
                            update_folder_name,
                            update_target_mod_id: pending.update_target_id.clone(),
                            post_install_rename_to: post_install_rename_to.clone(),
                        });
                    }
                    return;
                }
            }
            }
        }
        match selectable.len() {
            0 => {
                self.update_task_status(pending.task_id, TaskStatus::Failed);
                self.report_warn("no downloadable files found", Some("No downloadable files found"));
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
                    post_install_rename_to,
                );
            }
            _ => {
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
                });
            }
        }
    }

    fn queue_install_for_browse_mod(&mut self, mod_id: u64) {
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            return;
        };
        if !self.selected_game_is_installed_or_configured() {
            self.report_warn(
                "Game is not installed or configured.",
                Some("Install unavailable"),
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
        });
        self.set_message_ok(format!("Resolving download: {title}"));
    }

    fn confirm_browse_file_prompt(&mut self) {
        let Some(prompt) = self.browse_state.file_prompt.take() else {
            return;
        };
        let selected_files: Vec<_> = prompt
            .files
            .into_iter()
            .filter(|file| file.selected)
            .map(|file| file.file)
            .collect();
        if selected_files.is_empty() {
            self.set_message_ok("No files selected");
            return;
        }
        let selected_set = selected_files.clone();
        for file in selected_files {
            let unsafe_content = self
                .browse_state
                .details
                .get(&prompt.mod_id)
                .map(|detail| detail.unsafe_content)
                .unwrap_or(false);
            self.queue_browse_download(
                prompt.game_id.clone(),
                prompt.mod_id,
                file,
                selected_set.clone(),
                None,
                unsafe_content,
                prompt.update_folder_name.clone(),
                prompt.update_target_mod_id.clone(),
                prompt.post_install_rename_to.clone(),
            );
        }
        self.set_message_ok("Download queued");
    }
}
