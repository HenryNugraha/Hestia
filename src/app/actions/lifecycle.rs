impl HestiaApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        portable: PortablePaths,
        mut state: AppState,
        runtime_services: RuntimeServices,
    ) -> Self {
        install_lucide_font(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);
        let (icon_request_tx, icon_request_rx) = tokio_mpsc::unbounded_channel::<IconRequest>();
        let (icon_result_tx, icon_result_rx) = tokio_mpsc::unbounded_channel::<IconResult>();
        spawn_icon_worker(&runtime_services, icon_request_rx, icon_result_tx);
        let image_generation = Arc::new(AtomicU64::new(0));
        let (mod_image_request_tx, mod_image_request_rx) = tokio_mpsc::unbounded_channel::<LocalModImageRequest>();
        let (mod_image_result_tx, mod_image_result_rx) = tokio_mpsc::unbounded_channel::<LocalModImageResult>();
        let cache_limit_bytes = Arc::new(AtomicU64::new(state.cache_size_tier.bytes()));
        spawn_local_mod_image_worker(
            &runtime_services,
            portable.clone(),
            Arc::clone(&image_generation),
            Arc::clone(&cache_limit_bytes),
            mod_image_request_rx,
            mod_image_result_tx,
        );
        let game_icon_textures = HashMap::new();
        let (cover_request_tx, cover_request_rx) = tokio_mpsc::unbounded_channel::<CoverRequest>();
        let (cover_result_tx, cover_result_rx) = tokio_mpsc::unbounded_channel::<CoverResult>();
        spawn_cover_worker(&runtime_services, cover_request_rx, cover_result_tx);
        let (install_request_tx, install_request_rx) = tokio_mpsc::unbounded_channel::<InstallRequest>();
        let (install_event_tx, install_event_rx) = tokio_mpsc::unbounded_channel::<InstallEvent>();
        spawn_install_workers(&runtime_services, portable.clone(), install_request_rx, install_event_tx);
        let (browse_request_tx, browse_request_rx) = tokio_mpsc::unbounded_channel::<BrowseRequest>();
        let (browse_event_tx, browse_event_rx) = tokio_mpsc::unbounded_channel::<BrowseEvent>();
        spawn_browse_worker(&runtime_services, portable.clone(), browse_request_rx, browse_event_tx);
        let (browse_image_request_tx, browse_image_request_rx) = tokio_mpsc::unbounded_channel::<BrowseImageRequest>();
        let (browse_image_result_tx, browse_image_result_rx) = tokio_mpsc::unbounded_channel::<BrowseImageResult>();
        let youtube_icon_texture = load_image_texture(&cc.egui_ctx, youtube_icon_bytes(), "youtube-icon");
        spawn_browse_image_workers(
            &runtime_services,
            portable.clone(),
            Arc::clone(&cache_limit_bytes),
            browse_image_request_rx,
            browse_image_result_tx,
        );
        let (browse_download_result_tx, browse_download_event_rx) =
            tokio_mpsc::unbounded_channel::<BrowseDownloadEvent>();
        let (app_update_event_tx, app_update_event_rx) =
            tokio_mpsc::unbounded_channel::<AppUpdateEvent>();
        let (update_check_tx, update_check_worker_rx) = tokio_mpsc::unbounded_channel::<UpdateCheckRequest>();
        let (update_check_worker_tx, update_check_rx) = tokio_mpsc::unbounded_channel::<UpdateCheckResult>();
        spawn_update_check_worker(
            &runtime_services,
            portable.clone(),
            update_check_worker_rx,
            update_check_worker_tx,
        );
        let (refresh_request_tx, refresh_request_rx) =
            tokio_mpsc::unbounded_channel::<RefreshRequest>();
        let (refresh_result_tx, refresh_result_rx) =
            tokio_mpsc::unbounded_channel::<RefreshEvent>();
        spawn_selected_game_refresh_worker(&runtime_services, refresh_request_rx, refresh_result_tx);
        let app_icon_texture =
            load_title_icon_texture(&cc.egui_ctx, app_icon_bytes(), "app-icon");
        let selected_game = resolve_last_selected_game(&state).unwrap_or(0);
        let game_cover_textures = HashMap::new();
        let mod_thumbnail_placeholder =
            load_cover_texture(&cc.egui_ctx, mod_thumbnail_placeholder_bytes(), "mod-thumb-placeholder");
        let mod_cover_textures = HashMap::new();
        let mod_full_textures = HashMap::new();
        state.mods.clear();
        let (startup_scan_tx, startup_scan_rx) =
            tokio_mpsc::unbounded_channel::<StartupScanEvent>();
        let scan_runtime = runtime_services.handle();
        let mut startup_scan_state = state.clone();
        runtime_services.spawn(async move {
            let result = scan_runtime
                .spawn_blocking(move || -> Result<Vec<ModEntry>> {
                    xxmi::refresh_state(&mut startup_scan_state, None)?;
                    Ok(startup_scan_state.mods)
                })
                .await;
            match result {
                Ok(Ok(mods)) => {
                    let _ = startup_scan_tx.send(StartupScanEvent::Ready(mods));
                }
                Ok(Err(err)) => {
                    let _ = startup_scan_tx.send(StartupScanEvent::Failed(format!(
                        "Initial refresh failed: {err:#}"
                    )));
                }
                Err(err) => {
                    let _ = startup_scan_tx.send(StartupScanEvent::Failed(format!(
                        "Initial refresh join failed: {err}"
                    )));
                }
            }
        });
        state.tasks.retain(|task| task.status.is_terminal());
        state.show_log = false;
        state.show_tasks = false;
        state.show_tools = false;
        let log_scroll_to_bottom = state.show_log;
        let log_window_nonce = if state.show_log { 1 } else { 0 };
        let log_force_default_pos = state.show_log;
        let tools_window_nonce = if state.show_tools { 1 } else { 0 };
        let tools_force_default_pos = state.show_tools;
        let tasks_window_nonce = if state.show_tasks { 1 } else { 0 };
        let tasks_force_default_pos = state.show_tasks;
        let window_state_cache = Some(WindowStateSnapshot {
            pos: state.window_pos,
            size: state.window_size,
            maximized: state.window_maximized,
        });
        let window_was_maximized = state.window_maximized;
        prepare_initial_window_placement(cc, &state);
        let next_job_id = state
            .tasks
            .iter()
            .map(|task| task.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let texture_ram_budget_bytes = Self::detect_texture_ram_budget_bytes();

        let (gif_preview_request_tx, gif_preview_request_rx) =
            tokio_mpsc::unbounded_channel::<GifPreviewRequest>();
        let (gif_preview_event_tx, gif_preview_event_rx) =
            tokio_mpsc::unbounded_channel::<GifPreviewEvent>();
        spawn_gif_preview_worker(&runtime_services, gif_preview_request_rx, gif_preview_event_tx);

        let (gif_animation_request_tx, gif_animation_request_rx) =
            tokio_mpsc::unbounded_channel::<GifAnimationRequest>();
        let (gif_animation_event_tx, gif_animation_event_rx) =
            tokio_mpsc::unbounded_channel::<GifAnimationEvent>();
        spawn_gif_animation_worker(&runtime_services, gif_animation_request_rx, gif_animation_event_tx);

        let mut app = Self {
            runtime_services,
            portable,
            state,
            selected_game,
            selected_mod_id: None,
            selected_mods: HashSet::new(),
            mods_search_query: String::new(),
            mods_search_expanded: false,
            mods_search_focus_pending: false,
            show_enabled_mods: true,
            show_unlinked_mods: true,
            show_up_to_date_mods: true,
            show_update_available_mods: true,
            show_missing_source_mods: true,
            show_modified_locally_mods: true,
            current_view: ViewMode::Library,
            settings_open: false,
            mod_detail_open: false,
            browse_detail_open: false,
            settings_tab: SettingsTab::General,
            mod_detail_tab: ModDetailTab::Overview,
            last_right_pane_rect: None,
            mod_detail_focus_requested: false,
            browse_detail_focus_requested: false,
            mod_detail_editing: false,
            mod_detail_edit_target_id: None,
            mod_detail_edit_name: String::new(),
            category_rename_target_id: None,
            category_rename_name: String::new(),
            dragging_category_id: None,
            dragging_category_target_index: None,
            toasts: Vec::new(),
            pending_imports: VecDeque::new(),
            pending_conflicts: VecDeque::new(),
            log_scroll_to_bottom,
            log_window_nonce,
            log_force_default_pos,
            tools_window_nonce,
            tools_force_default_pos,
            tool_launch_options_prompt: None,
            dragging_window_tool_id: None,
            dragging_window_tool_target_index: None,
            dragging_titlebar_tool_id: None,
            dragging_titlebar_tool_target_index: None,
            tasks_window_nonce,
            tasks_force_default_pos,
            tasks_tab: TasksTab::Installs,
            tasks_scroll_to_edge: false,
            install_queue: VecDeque::new(),
            install_batch_active: false,
            install_batch_stats: InstallBatchStats::default(),
            install_inflight: HashMap::new(),
            install_next_job_id: next_job_id,
            install_request_tx,
            install_event_rx,
            browse_query: String::new(),
            browse_search_expanded: false,
            browse_search_focus_pending: false,
            pending_browse_open_mod_id: None,
            browse_state: BrowseState {
                next_page: 1,
                has_more: true,
                ..Default::default()
            },
            my_mod_overlay_images: Vec::new(),
            game_icon_textures,
            tool_icon_textures: HashMap::new(),
            game_cover_textures,
            mod_thumbnail_placeholder,
            mod_cover_textures,
            mod_full_textures,
            browse_image_textures: HashMap::new(),
            browse_thumb_textures: HashMap::new(),
            icon_request_tx,
            icon_result_rx,
            mod_image_request_tx,
            mod_image_result_rx,
            pending_mod_image_requests: HashSet::new(),
            pending_mod_image_queue: Vec::new(),
            pending_icon_requests: HashSet::new(),
            cover_request_tx,
            cover_result_rx,
            pending_cover_requests: HashSet::new(),
            youtube_icon_texture,
            app_icon_texture,
            browse_request_tx,
            browse_event_rx,
            browse_image_request_tx,
            browse_image_result_rx,
            browse_download_event_rx,
            browse_download_result_tx,
            app_update_event_tx,
            app_update_event_rx,
            app_update_download_inflight: None,
            app_update_manifest: None,
            app_update_verified_path: None,
            app_update_task_id: None,
            app_update_button_state: AppUpdateButtonState::Check,
            app_update_button_spin_until: 0.0,
            browse_image_queue: Vec::new(),
            browse_image_inflight: HashMap::new(),
            browse_image_retry_after: HashMap::new(),
            pending_texture_uploads: VecDeque::new(),
            texture_meta: HashMap::new(),
            texture_access_tick: 0,
            texture_ram_estimated_bytes: 0,
            texture_ram_budget_bytes,
            texture_evictions_window_start: 0.0,
            texture_evictions_window_count: 0,
            texture_evictions_per_minute: 0,
            browse_download_queue: VecDeque::new(),
            browse_download_inflight: HashMap::new(),
            pending_browse_install_safety: HashMap::new(),
            pending_browse_install_meta: HashMap::new(),
            browse_commonmark_cache: CommonMarkCache::default(),
            browse_request_nonce: 0,
            browse_page_generation: 0,
            browse_detail_generation: 0,
            image_generation,
            update_check_tx,
            update_check_rx,
            update_check_inflight: false,
            pending_update_check_game: None,
            pending_update_check_mods: HashSet::new(),
            refresh_request_tx,
            refresh_result_rx,
            refresh_inflight: false,
            refresh_pending_selected_game: None,
            pending_install_finalize: HashMap::new(),
            pending_known_installed_paths: HashSet::new(),
            reload_spin_until: 0.7,
            reload_was_busy: true,
            my_mod_source_expanded: false,
            cache_limit_bytes,
            usage_cache_bytes: 0,
            usage_archive_bytes: 0,
            usage_counters_last_refresh: 0.0,
            usage_counters_dirty: true,
            window_state_cache,
            window_state_last_save: 0.0,
            window_was_maximized,
            selection_empty_at: None,
            startup_scan_loading: true,
            startup_scan_rx,
            gif_preview_request_tx,
            gif_preview_event_rx,
            pending_gif_previews: HashSet::new(),
            gif_animation_request_tx,
            gif_animation_event_rx,
            pending_gif_animations: HashSet::new(),
            animated_gif_state: HashMap::new(),
        };
        Self::cleanup_runtime_temp_downloads_best_effort();
        app.set_selected_game(selected_game, &cc.egui_ctx);
        app.auto_detect_game_paths();
        app.ensure_selected_game_enabled(&cc.egui_ctx);
        app
    }

    fn save_state(&mut self) {
        if let Err(err) = persistence::save_app_state(&self.portable, &self.state) {
            self.report_error_message(
                format!("failed to save app state: {err:#}"),
                Some("Could not save settings"),
            );
        }
    }

    fn runtime_temp_downloads_dir() -> PathBuf {
        persistence::runtime_temp_downloads_dir()
    }

    fn runtime_temp_root() -> PathBuf {
        persistence::runtime_temp_root()
    }

    fn cleanup_runtime_temp_downloads_best_effort() {
        let dir = Self::runtime_temp_downloads_dir();
        let _ = fs::remove_dir_all(&dir);
    }

    fn cleanup_runtime_temp_for_source(source: &ImportSource) {
        let ImportSource::Archive(path) = source else {
            return;
        };
        let root = Self::runtime_temp_root();
        if path.starts_with(&root) {
            let _ = fs::remove_file(path);
        }
    }

    fn detect_texture_ram_budget_bytes() -> u64 {
        let adaptive = Self::detect_total_system_ram_bytes()
            .map(|total| total / 8)
            .unwrap_or(TEXTURE_RAM_BUDGET_MIN_BYTES);
        adaptive.clamp(TEXTURE_RAM_BUDGET_MIN_BYTES, TEXTURE_RAM_BUDGET_MAX_BYTES)
    }

    fn detect_total_system_ram_bytes() -> Option<u64> {
        let mut mem = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe {
            GlobalMemoryStatusEx(&mut mem).ok()?;
        }
        Some(mem.ullTotalPhys)
    }

    fn texture_key(kind: TextureKind, key: &str) -> (TextureKind, String) {
        (kind, key.to_string())
    }

    fn estimate_texture_bytes(texture: &egui::TextureHandle) -> u64 {
        let [w, h] = texture.size();
        (w as u64).saturating_mul(h as u64).saturating_mul(4)
    }

    fn bump_texture_tick(&mut self) -> u64 {
        self.texture_access_tick = self.texture_access_tick.wrapping_add(1).max(1);
        self.texture_access_tick
    }

    fn touch_texture(&mut self, kind: TextureKind, key: &str, priority: u8) {
        let tick = self.bump_texture_tick();
        if let Some(meta) = self.texture_meta.get_mut(&Self::texture_key(kind, key)) {
            meta.last_access_tick = tick;
            meta.priority = priority;
        }
    }

    fn insert_tracked_texture(
        &mut self,
        kind: TextureKind,
        key: String,
        priority: u8,
        texture: egui::TextureHandle,
    ) {
        let bytes = Self::estimate_texture_bytes(&texture);
        let tick = self.bump_texture_tick();
        let map_key = Self::texture_key(kind, &key);
        if let Some(prev) = self.texture_meta.insert(
            map_key,
            TextureEntryMeta {
                bytes,
                last_access_tick: tick,
                priority,
            },
        ) {
            self.texture_ram_estimated_bytes =
                self.texture_ram_estimated_bytes.saturating_sub(prev.bytes);
        }
        self.texture_ram_estimated_bytes = self.texture_ram_estimated_bytes.saturating_add(bytes);
        match kind {
            TextureKind::ModThumb => {
                self.mod_cover_textures.insert(key, texture);
            }
            TextureKind::ModFull => {
                self.mod_full_textures.insert(key, texture);
            }
            TextureKind::BrowseThumb => {
                self.browse_thumb_textures.insert(key, texture);
            }
            TextureKind::BrowseFull => {
                self.browse_image_textures.insert(key, texture);
            }
        }
    }

    fn remove_tracked_texture(&mut self, kind: TextureKind, key: &str) {
        let map_key = Self::texture_key(kind, key);
        if let Some(prev) = self.texture_meta.remove(&map_key) {
            self.texture_ram_estimated_bytes =
                self.texture_ram_estimated_bytes.saturating_sub(prev.bytes);
        }
        match kind {
            TextureKind::ModThumb => {
                self.mod_cover_textures.remove(key);
            }
            TextureKind::ModFull => {
                self.mod_full_textures.remove(key);
            }
            TextureKind::BrowseThumb => {
                self.browse_thumb_textures.remove(key);
            }
            TextureKind::BrowseFull => {
                self.browse_image_textures.remove(key);
            }
        }
    }

    fn rebuild_texture_tracking(&mut self) {
        let old_meta = std::mem::take(&mut self.texture_meta);
        self.texture_meta = HashMap::new();
        self.texture_ram_estimated_bytes = 0;

        for (k, t) in &self.mod_cover_textures {
            let key = Self::texture_key(TextureKind::ModThumb, k);
            let last = old_meta
                .get(&key)
                .map(|m| m.last_access_tick)
                .unwrap_or(self.texture_access_tick);
            let bytes = Self::estimate_texture_bytes(t);
            self.texture_meta.insert(
                key,
                TextureEntryMeta {
                    bytes,
                    last_access_tick: last,
                    priority: 1, // Default to background
                },
            );
            self.texture_ram_estimated_bytes = self.texture_ram_estimated_bytes.saturating_add(bytes);
        }
        for (k, t) in &self.mod_full_textures {
            let key = Self::texture_key(TextureKind::ModFull, k);
            let last = old_meta
                .get(&key)
                .map(|m| m.last_access_tick)
                .unwrap_or(self.texture_access_tick);
            let bytes = Self::estimate_texture_bytes(t);
            self.texture_meta.insert(
                key,
                TextureEntryMeta {
                    bytes,
                    last_access_tick: last,
                    priority: 0, // Default to inactive high-res
                },
            );
            self.texture_ram_estimated_bytes = self.texture_ram_estimated_bytes.saturating_add(bytes);
        }
        for (k, t) in &self.browse_thumb_textures {
            let key = Self::texture_key(TextureKind::BrowseThumb, k);
            let last = old_meta
                .get(&key)
                .map(|m| m.last_access_tick)
                .unwrap_or(self.texture_access_tick);
            let bytes = Self::estimate_texture_bytes(t);
            self.texture_meta.insert(
                key,
                TextureEntryMeta {
                    bytes,
                    last_access_tick: last,
                    priority: 1, // Default to background
                },
            );
            self.texture_ram_estimated_bytes = self.texture_ram_estimated_bytes.saturating_add(bytes);
        }
        for (k, t) in &self.browse_image_textures {
            let key = Self::texture_key(TextureKind::BrowseFull, k);
            let last = old_meta
                .get(&key)
                .map(|m| m.last_access_tick)
                .unwrap_or(self.texture_access_tick);
            let bytes = Self::estimate_texture_bytes(t);
            self.texture_meta.insert(
                key,
                TextureEntryMeta {
                    bytes,
                    last_access_tick: last,
                    priority: 0, // Default to inactive high-res
                },
            );
            self.texture_ram_estimated_bytes = self.texture_ram_estimated_bytes.saturating_add(bytes);
        }
    }

    fn evict_textures_to_budget(&mut self, now: f64) {
        // Weighted eviction: Level 0 (inactive hi-res) -> Level 1 (off-screen thumbs) -> Level 2 (on-screen/rails)
        // Level 3 (Current Full View) is protected.
        for target_priority in 0..=2 {
            while self.texture_ram_estimated_bytes > self.texture_ram_budget_bytes {
                let victim = self.texture_meta.iter()
                    .filter(|(_, meta)| meta.priority == target_priority)
                    .min_by_key(|(_, meta)| meta.last_access_tick)
                    .map(|(key, _)| key.clone());

                if let Some((kind, key)) = victim {
                    self.remove_tracked_texture(kind, &key);
                    self.texture_evictions_window_count = self.texture_evictions_window_count.saturating_add(1);
                } else {
                    break; // No more victims at this priority level
                }
            }
        }

        if now - self.texture_evictions_window_start >= 60.0 {
            self.texture_evictions_per_minute = self.texture_evictions_window_count;
            self.texture_evictions_window_count = 0;
            self.texture_evictions_window_start = now;
        }
    }

    fn clear_dynamic_textures(&mut self) {
        self.mod_cover_textures.clear();
        self.mod_full_textures.clear();
        self.browse_thumb_textures.clear();
        self.browse_image_textures.clear();
        self.pending_texture_uploads.clear();
        self.pending_mod_image_queue.clear();
        self.pending_mod_image_requests.clear();
        self.cancel_browse_full_image_requests();
        self.browse_image_queue.clear();
        self.browse_image_inflight.clear();
        self.rebuild_texture_tracking();
    }

    fn invalidate_stale_mod_textures(&mut self, old_updated_ats: &HashMap<String, DateTime<Utc>>) {
        let mut cleared_any = false;
        for m in &self.state.mods {
            if let Some(prev_ts) = old_updated_ats.get(&m.id) {
                if m.updated_at != *prev_ts {
                    let mod_id = &m.id;
                    let prefix = format!("my-mod-shot-{mod_id}-");

                    self.mod_cover_textures.remove(mod_id);
                    self.mod_cover_textures.retain(|k, _| !k.starts_with(&prefix));

                    self.mod_full_textures.remove(mod_id);
                    self.mod_full_textures.retain(|k, _| k != mod_id && !k.starts_with(&prefix));

                    self.pending_mod_image_requests.remove(mod_id);
                    self.pending_mod_image_requests.retain(|k| !k.starts_with(&prefix));

                    self.pending_mod_image_queue.retain(|req| {
                        req.texture_key != *mod_id && !req.texture_key.starts_with(&prefix)
                    });

                    cleared_any = true;
                }
            }
        }
        if cleared_any {
            self.rebuild_texture_tracking();
        }
    }

    fn get_mod_thumb_texture(&mut self, key: &str, priority: u8) -> Option<&egui::TextureHandle> {
        if self.mod_cover_textures.contains_key(key) {
            self.touch_texture(TextureKind::ModThumb, key, priority);
        }
        self.mod_cover_textures.get(key)
    }

    fn get_mod_full_texture(&mut self, key: &str, priority: u8) -> Option<&egui::TextureHandle> {
        if self.mod_full_textures.contains_key(key) {
            self.touch_texture(TextureKind::ModFull, key, priority);
        }
        self.mod_full_textures.get(key)
    }

    fn get_browse_thumb_texture(&mut self, key: &str, priority: u8) -> Option<&egui::TextureHandle> {
        if self.browse_thumb_textures.contains_key(key) {
            self.touch_texture(TextureKind::BrowseThumb, key, priority);
            return self.browse_thumb_textures.get(key);
        }

        // Fallback for thumbnails generated via high-res preloading, which may not have the "rail:" prefix.
        if let Some(stripped) = key.strip_prefix("rail:") {
            if self.browse_thumb_textures.contains_key(stripped) {
                self.touch_texture(TextureKind::BrowseThumb, stripped, priority);
                return self.browse_thumb_textures.get(stripped);
            }
        }
        None
    }

    fn get_browse_full_texture(&mut self, key: &str, priority: u8) -> Option<&egui::TextureHandle> {
        if self.browse_image_textures.contains_key(key) {
            self.touch_texture(TextureKind::BrowseFull, key, priority);
        }
        self.browse_image_textures.get(key)
    }

    fn set_message_ok(&mut self, message: impl Into<String>) {
        self.push_toast(message.into(), false);
    }

    fn log_warn(&mut self, detail: impl Into<String>) {
        let detail = sanitize_log_subject(&detail.into());
        if !detail.is_empty() {
            self.push_log(format!("Warn: {detail}"));
        }
    }

    fn log_error(&mut self, detail: impl Into<String>) {
        let detail = sanitize_log_subject(&detail.into());
        if !detail.is_empty() {
            self.push_log(format!("Error: {detail}"));
        }
    }

    fn report_warn(&mut self, detail: impl Into<String>, toast_summary: Option<&str>) {
        self.log_warn(detail);
        if let Some(summary) = toast_summary {
            self.push_toast(summary.to_string(), true);
        }
    }

    fn report_error_message(&mut self, detail: impl Into<String>, toast_summary: Option<&str>) {
        self.log_error(detail);
        if let Some(summary) = toast_summary {
            self.push_toast(summary.to_string(), true);
        }
    }

    fn report_error(&mut self, err: anyhow::Error, toast_summary: Option<&str>) {
        self.report_error_message(format!("{err:#}"), toast_summary);
    }

    fn push_toast(&mut self, message: String, is_error: bool) {
        let entry = ToastEntry {
            message,
            is_error,
            created_at: 0.0,
        };
        self.toasts.insert(0, entry);
        if self.toasts.len() > TOAST_LIMIT {
            self.toasts.truncate(TOAST_LIMIT);
        }
    }

    fn auto_detect_game_paths(&mut self) {
        let xxmi_config = load_xxmi_config();
        let xxmi_launcher_candidates = xxmi_config
            .as_ref()
            .map(|(config_path, _)| xxmi_launcher_exe_candidates(config_path))
            .unwrap_or_default();

        let mut changed = false;
        let global_modded_needs = match self.state.modded_launcher_path_override.as_ref() {
            Some(path) => !path.is_file(),
            None => true,
        };
        if global_modded_needs {
            if let Some(path) = pick_most_recent_existing(&xxmi_launcher_candidates) {
                self.state.modded_launcher_path_override = Some(path);
                changed = true;
            }
        }

        for game in &mut self.state.games {
            let modded_needs = match game.modded_exe_path_override.as_ref() {
                Some(path) => !path.is_file(),
                None => true,
            };
            if modded_needs {
                let mut candidates = xxmi_launcher_candidates.clone();
                candidates.extend(default_modded_exe_candidates(&game.definition.id));
                if let Some(path) = pick_most_recent_existing(&candidates) {
                    game.modded_exe_path_override = Some(path);
                    changed = true;
                }
            }

            let vanilla_needs = match game.vanilla_exe_path_override.as_ref() {
                Some(path) => !path.is_file(),
                None => true,
            };
            if vanilla_needs {
                let candidates_from_config = xxmi_config
                    .as_ref()
                    .map(|(_, config)| {
                        xxmi_game_exe_candidates(config, &game.definition.xxmi_code)
                    })
                    .unwrap_or_default();
                let fallback_candidates = default_vanilla_exe_candidates(&game.definition.id);
                let path = pick_most_recent_existing(&candidates_from_config)
                    .or_else(|| pick_most_recent_existing(&fallback_candidates));
                if let Some(path) = path {
                    game.vanilla_exe_path_override = Some(path);
                    changed = true;
                }
            }

            if !self.state.auto_game_enable_done {
                let vanilla_found = game
                    .vanilla_exe_path_override
                    .as_ref()
                    .is_some_and(|path| path.is_file());
                if game.enabled != vanilla_found {
                    game.enabled = vanilla_found;
                    changed = true;
                }
            }
        }

        if !self.state.auto_game_enable_done {
            self.state.auto_game_enable_done = true;
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }

    fn ensure_selected_game_enabled(&mut self, ctx: &egui::Context) {
        if self
            .state
            .games
            .get(self.selected_game)
            .is_some_and(|game| game.enabled)
        {
            return;
        }
        if let Some((index, _)) = self
            .state
            .games
            .iter()
            .enumerate()
            .find(|(_, game)| game.enabled)
        {
            self.set_selected_game(index, ctx);
        }
    }

    fn launch_selected_game(&mut self, modded: bool) {
        let Some(game) = self.selected_game().cloned() else {
            self.report_error_message("game not selected", Some("Launch failed"));
            return;
        };
        if !Self::game_install_is_configured(&game) {
            self.report_error_message(
                "Game is not installed or configured.",
                Some("Launch failed"),
            );
            return;
        }
        let Some(path) = (if modded {
            self.state
                .modded_launcher_path_override
                .clone()
                .or_else(|| game.modded_exe_path())
        } else {
            game.vanilla_exe_path()
        }) else {
            let label = if modded { "Play (Modded)" } else { "Play (Vanilla)" };
            self.report_error_message(
                format!("{label} path not set for {}", game.definition.name),
                Some("Launch path not set"),
            );
            return;
        };
        if !path.is_file() {
            self.report_error_message(
                "Game is not installed or configured.",
                Some("Launch failed"),
            );
            return;
        }
        let result = if modded {
            xxmi::launch_xxmi_launcher(&path, &game.definition.xxmi_code)
        } else {
            xxmi::launch_vanilla_executable(&path)
        };
        match result {
            Ok(()) => {
                let label = if modded { "Modded" } else { "Vanilla" };
                self.set_message_ok(format!("Launched {} ({label})", game.definition.name));
            }
            Err(err) => self.report_error(err, Some("Launch failed")),
        }
    }

    fn selected_game(&self) -> Option<&GameInstall> {
        self.state.games.get(self.selected_game)
    }

    fn enabled_games(&self) -> Vec<GameInstall> {
        self.state
            .games
            .iter()
            .filter(|game| game.enabled)
            .cloned()
            .collect()
    }

    fn has_enabled_games(&self) -> bool {
        self.state.games.iter().any(|game| game.enabled)
    }

    fn selected_game_is_installed_or_configured(&self) -> bool {
        self.selected_game()
            .is_some_and(Self::game_install_is_configured)
    }

    fn selected_game_can_launch_modded(&self) -> bool {
        self.selected_game().is_some_and(|game| {
            Self::game_install_is_configured(game)
                && self
                    .state
                    .modded_launcher_path_override
                    .as_ref()
                    .or_else(|| game.modded_exe_path_override.as_ref())
                    .is_some_and(|path| path.is_file())
        })
    }

    fn selected_game_can_launch_vanilla(&self) -> bool {
        self.selected_game()
            .is_some_and(|game| Self::game_install_is_configured(game))
    }

    fn game_is_installed_or_configured(&self, game_id: &str) -> bool {
        self.state
            .games
            .iter()
            .any(|game| game.definition.id == game_id && Self::game_install_is_configured(game))
    }

    fn game_install_is_configured(game: &GameInstall) -> bool {
        game.enabled
            && game
                .vanilla_exe_path_override
                .as_ref()
                .is_some_and(|path| path.is_file())
    }

    fn mods_for_selected_game(&self) -> Vec<&ModEntry> {
        let Some(game) = self.selected_game() else {
            return Vec::new();
        };
        if !game.enabled {
            return Vec::new();
        }
        let query_norm = normalize_lookup(&self.mods_search_query);
        let mut mods: Vec<_> = self
            .state
            .mods
            .iter()
            .filter(|item| {
                if item.game_id != game.definition.id {
                    return false;
                }
                if !self.show_enabled_mods && item.status == ModStatus::Active {
                    return false;
                }
                if self.state.hide_disabled && item.status == ModStatus::Disabled {
                    return false;
                }
                if self.state.hide_archived && item.status == ModStatus::Archived {
                    return false;
                }
                if !self.show_unlinked_mods && item.update_state == ModUpdateState::Unlinked {
                    return false;
                }
                if !self.show_up_to_date_mods && item.update_state == ModUpdateState::UpToDate {
                    return false;
                }
                if !self.show_update_available_mods
                    && item.update_state == ModUpdateState::UpdateAvailable
                {
                    return false;
                }
                if !self.show_missing_source_mods
                    && item.update_state == ModUpdateState::MissingSource
                {
                    return false;
                }
                if !self.show_modified_locally_mods
                    && item.update_state == ModUpdateState::ModifiedLocally
                {
                    return false;
                }
                if matches!(
                    self.state.unsafe_content_mode,
                    UnsafeContentMode::HideNoCounter | UnsafeContentMode::HideShowCounter
                ) && item.unsafe_content
                {
                    return false;
                }
                if !query_norm.is_empty() {
                    let title = item
                        .metadata
                        .user
                        .title
                        .as_deref()
                        .unwrap_or(&item.folder_name);
                    let mut haystacks = vec![
                        normalize_lookup(&item.folder_name),
                        normalize_lookup(title),
                        normalize_lookup(
                            item.root_path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or_default(),
                        ),
                    ];
                    if let Some(link) = item.source.as_ref().and_then(|s| s.gamebanana.as_ref()) {
                        haystacks.push(normalize_lookup(&link.url));
                        haystacks.push(link.mod_id.to_string());
                    }
                    if !haystacks.iter().any(|text| text.contains(&query_norm)) {
                        return false;
                    }
                }
                true
            })
            .collect();
        let display_name = |item: &&ModEntry| {
            item.metadata
                .user
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or(&item.folder_name)
                .to_ascii_lowercase()
        };
        let sort_date = |item: &&ModEntry| {
            item.created_at
                .timestamp()
                .max(
                    item.content_mtime
                        .map(|ts| ts.timestamp())
                        .unwrap_or(i64::MIN),
                )
                .max(item.updated_at.timestamp())
        };
        let category_order = |item: &&ModEntry| {
            let category_id = item.metadata.user.category_id.as_deref();
            category_id
                .and_then(|id| {
                    self.state
                        .categories
                        .iter()
                        .find(|category| category.id == id && category.game_id == item.game_id)
                        .map(|category| category.order)
                })
                .unwrap_or(i32::MAX / 4)
        };
        mods.sort_by(|a, b| {
            let name_cmp = display_name(a).cmp(&display_name(b));
            let status_cmp = if self.state.library_sort_status_first
                && matches!(
                    self.state.library_group_mode,
                    LibraryGroupMode::Category | LibraryGroupMode::None
                ) {
                a.status.cmp(&b.status)
            } else {
                std::cmp::Ordering::Equal
            };
            let category_cmp = if self.state.library_sort_category_first
                && !matches!(self.state.library_group_mode, LibraryGroupMode::Category)
            {
                category_order(a)
                    .cmp(&category_order(b))
                    .then_with(|| {
                        let left = a
                            .metadata
                            .user
                            .category
                            .trim()
                            .to_ascii_lowercase();
                        let right = b
                            .metadata
                            .user
                            .category
                            .trim()
                            .to_ascii_lowercase();
                        left.cmp(&right)
                    })
            } else {
                std::cmp::Ordering::Equal
            };
            let sort_cmp = match self.state.library_sort {
                LibrarySort::NameAsc => name_cmp,
                LibrarySort::NameDesc => name_cmp.reverse(),
                LibrarySort::DateDesc => sort_date(b)
                    .cmp(&sort_date(a))
                    .then_with(|| name_cmp),
                LibrarySort::DateAsc => sort_date(a)
                    .cmp(&sort_date(b))
                    .then_with(|| name_cmp),
            };
            status_cmp.then(category_cmp).then(sort_cmp)
        });
        mods
    }

    fn selected_mod_mut(&mut self) -> Option<&mut ModEntry> {
        let id = self.selected_mod_id.clone()?;
        self.state.mods.iter_mut().find(|item| item.id == id)
    }

    fn selected_mod(&self) -> Option<&ModEntry> {
        let id = self.selected_mod_id.as_ref()?;
        self.state.mods.iter().find(|item| &item.id == id)
    }

    fn toggle_tasks_window(&mut self) {
        self.state.show_tasks = !self.state.show_tasks;
        if self.state.show_tasks {
            self.tasks_window_nonce = self.tasks_window_nonce.wrapping_add(1);
            self.tasks_force_default_pos = true;
        }
        self.save_state();
    }

    fn toggle_log_window(&mut self) {
        self.state.show_log = !self.state.show_log;
        if self.state.show_log {
            self.log_scroll_to_bottom = true;
            self.log_window_nonce = self.log_window_nonce.wrapping_add(1);
            self.log_force_default_pos = true;
        }
        self.save_state();
    }

    fn toggle_primary_view(&mut self) {
        self.current_view = match self.current_view {
            ViewMode::Library => ViewMode::Browse,
            ViewMode::Browse => ViewMode::Library,
        };
        self.mod_detail_editing = false;
    }

    fn focus_active_search(&mut self, ctx: &egui::Context) {
        match self.current_view {
            ViewMode::Library => {
                self.mods_search_expanded = true;
                self.mods_search_focus_pending = true;
            }
            ViewMode::Browse => {
                self.browse_search_expanded = true;
                self.browse_search_focus_pending = true;
            }
        }
        ctx.request_repaint();
    }

    fn start_selected_mod_rename(&mut self) {
        let Some((mod_id, title)) = self.selected_mod().map(|selected| {
            (
                selected.id.clone(),
                selected
                    .metadata
                    .user
                    .title
                    .clone()
                    .unwrap_or_else(|| selected.folder_name.clone()),
            )
        }) else {
            return;
        };
        self.mod_detail_editing = true;
        self.mod_detail_edit_target_id = Some(mod_id);
        self.mod_detail_edit_name = title;
    }

    fn shortcuts_blocked_by_text_input(&self, ctx: &egui::Context) -> bool {
        if self.mod_detail_editing {
            return true;
        }

        ctx.memory(|memory| memory.focused())
            .is_some_and(|focused_id| egui::TextEdit::load_state(ctx, focused_id).is_some())
    }

    fn delete_shortcut_has_mod_context(&self) -> bool {
        !self.selected_mods.is_empty() || self.selected_mod().is_some()
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl = egui::Modifiers::CTRL;
        let ctrl_shift = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let text_input_active = self.shortcuts_blocked_by_text_input(ctx);

        if ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Tab) && !input.modifiers.shift && !input.modifiers.alt) {
            self.toggle_primary_view();
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F10)) {
            self.settings_open = !self.settings_open;
        }
        if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::L))) {
            self.toggle_log_window();
        }
        if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::T))) {
            self.toggle_tools_window();
        }
        if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::J))) {
            self.toggle_tasks_window();
        }
        if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::F))) {
            self.focus_active_search(ctx);
        }

        if self.current_view == ViewMode::Library {
            if !text_input_active
                && self.delete_shortcut_has_mod_context()
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            {
                self.delete_selected_context();
            }
            if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::E))) {
                self.enable_or_restore_selected_context();
            }
            if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::D))) {
                self.disable_selected_context();
            }
            if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::A))) {
                self.archive_selected_context();
            }
            if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::R))) {
                self.enable_or_restore_selected_context();
            }
            if !text_input_active
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F2))
                && self.mod_detail_open
                && !self.mod_detail_editing
                && self.selected_mod().is_some()
            {
                self.start_selected_mod_rename();
            }
        }

        if ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::R))) {
            match self.current_view {
                ViewMode::Library => self.refresh_with_toast(),
                ViewMode::Browse => self.restart_browse_query(),
            }
        }
    }

    fn clear_mod_image_runtime_state(&mut self, mod_entry: &ModEntry) {
        let cover_key = mod_entry.id.clone();
        let shot_prefix = format!("my-mod-shot-{}-", mod_entry.id);

        self.mod_cover_textures
            .retain(|key, _| key != &cover_key && !key.starts_with(&shot_prefix));
        self.mod_full_textures
            .retain(|key, _| key != &cover_key && !key.starts_with(&shot_prefix));
        self.pending_mod_image_requests
            .retain(|key| key != &cover_key && !key.starts_with(&shot_prefix));
        self.pending_mod_image_queue
            .retain(|req| req.texture_key != cover_key && !req.texture_key.starts_with(&shot_prefix));
        self.pending_texture_uploads.retain(|item| match item {
            PendingTextureUpload::ModThumb { texture_key, .. }
            | PendingTextureUpload::ModFull { texture_key, .. } => {
                texture_key != &cover_key && !texture_key.starts_with(&shot_prefix)
            }
            _ => true,
        });
        self.my_mod_overlay_images.retain(|item| {
            item.texture_key != cover_key && !item.texture_key.starts_with(&shot_prefix)
        });

        if self
            .browse_state
            .screenshot_overlay
            .as_ref()
            .is_some_and(|overlay| {
                overlay.texture_key == cover_key || overlay.texture_key.starts_with(&shot_prefix)
            })
        {
            self.browse_state.screenshot_overlay = None;
        }

        let mut source_urls: Vec<String> = mod_entry
            .source
            .as_ref()
            .and_then(|s| s.snapshot.as_ref())
            .map(|s| s.preview_urls.clone())
            .unwrap_or_default();
        if let Some(raw) = mod_entry
            .source
            .as_ref()
            .and_then(|s| s.raw_profile_json.as_deref())
        {
            source_urls.extend(extract_image_urls_from_profile_json(raw));
        }
        source_urls.sort();
        source_urls.dedup();

        let mut source_keys = HashSet::new();
        for url in source_urls {
            source_keys.insert(Self::browse_image_cache_key(&url));
        }
        if source_keys.is_empty() {
            self.rebuild_texture_tracking();
            return;
        }

        self.browse_image_queue
            .retain(|job| !source_keys.contains(&job.texture_key));
        for key in &source_keys {
            if let Some(inflight) = self.browse_image_inflight.remove(key) {
                inflight.cancel.store(true, Ordering::Relaxed);
            }
        }
        self.browse_image_textures
            .retain(|key, _| !source_keys.contains(key));
        self.browse_thumb_textures
            .retain(|key, _| !source_keys.contains(key));
        self.pending_texture_uploads.retain(|item| match item {
            PendingTextureUpload::BrowseThumb { texture_key, .. }
            | PendingTextureUpload::BrowseFull { texture_key, .. } => {
                !source_keys.contains(texture_key)
            }
            _ => true,
        });
        self.rebuild_texture_tracking();
    }

    fn set_selected_mod_id(&mut self, mod_id: Option<String>) {
        if self.selected_mod_id == mod_id {
            return;
        }
        self.image_generation.fetch_add(1, Ordering::Relaxed);
        self.pending_mod_image_queue.clear();
        self.pending_mod_image_requests.clear();

        // self.selected_mod_id = mod_id;
        self.selected_mod_id = mod_id.clone();
        self.mod_detail_editing = false;
        self.mod_detail_edit_target_id = None;
        self.mod_detail_edit_name.clear();
        self.my_mod_overlay_images.clear();
        self.browse_state.screenshot_overlay = None;
        if let Some(id) = mod_id {
            self.mod_detail_open = true;
            self.mod_detail_focus_requested = true;

            // Optimization: Pre-fetch full cover image for the selected mod to avoid redundant decoding later.
            // Extract necessary data before any mutable borrows of `self`
            let (mod_entry_id_clone, source_path_clone, markdown_content) = {
                if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == id) {
                let (_, source_path, _) = Self::current_card_thumb_meta(mod_entry);
                let markdown = mod_primary_description_markdown(mod_entry, &self.portable);
                    (Some(id.clone()), source_path, Some(markdown))
                } else {
                    (None, None, None)
                }
            };

            // Now perform mutable operations
            if let Some(mod_entry_id) = mod_entry_id_clone {
                if let Some(path) = source_path_clone {
                    self.queue_mod_image_full_load(mod_entry_id.clone(), path, 10);
                }
                if let Some(markdown) = markdown_content {
                    self.prewarm_markdown_images(&markdown);
                }
            }
        } else {
            self.mod_detail_open = false;
        }
    }

    fn update_main_window_state(&mut self, ctx: &egui::Context) {
        let viewport = ctx.input(|input| input.viewport().clone());
        let now = ctx.input(|input| input.time);
        if viewport.minimized.unwrap_or(false) {
            return;
        }
        let maximized = viewport.maximized.unwrap_or(false);
        let pos = if maximized {
            None
        } else {
            viewport.outer_rect.map(|rect| [rect.min.x, rect.min.y])
        };
        let size = if maximized {
            None
        } else {
            viewport
                .inner_rect
                .map(|rect| [rect.size().x, rect.size().y])
        };
        self.window_was_maximized = maximized;
        let snapshot = WindowStateSnapshot { pos, size, maximized };
        if self.window_state_cache != Some(snapshot) {
            if !maximized {
                self.state.window_pos = pos;
                self.state.window_size = size;
            }
            self.state.window_maximized = maximized;
            if now - self.window_state_last_save >= 0.5 {
                self.save_state();
                self.window_state_last_save = now;
            }
            self.window_state_cache = Some(snapshot);
        }
    }

    fn refresh(&mut self) {
        self.mark_usage_counters_dirty();
        let old_ts: HashMap<String, DateTime<Utc>> = self.state.mods.iter()
            .map(|m| (m.id.clone(), m.updated_at))
            .collect();

        let game_id = self.selected_game().map(|g| g.definition.id.clone());
        match xxmi::refresh_state(&mut self.state, game_id.as_deref()) {
            Ok(()) => {
                self.invalidate_stale_mod_textures(&old_ts);
                self.backfill_missing_mod_images(game_id.as_deref());
                self.sync_tools_for_selected_game();
                self.save_state();
                self.sync_selection_after_refresh();
                self.queue_update_check_for_linked_mods(game_id.as_deref());
                self.request_automatic_app_update_check(0.0);
            }
            Err(err) => self.report_error(err, Some("Could not refresh mods")),
        }
    }

    fn enqueue_mod_image_sync(&mut self, mod_id: &str) {
        let job_data = self.state.mods.iter().find(|m| m.id == mod_id).and_then(|m| {
            m.source.as_ref().and_then(|s| {
                s.snapshot.as_ref().map(|snap| {
                    (m.root_path.clone(), snap.clone())
                })
            })
        });

        if let Some((root_path, snapshot)) = job_data {
            let job_id = self.next_background_job_id();
            let _ = self.install_request_tx.send(InstallRequest::SyncImages {
                job_id,
                mod_entry_id: mod_id.to_string(),
                mod_root_path: root_path,
                profile: Box::new(profile_to_response(Some(&snapshot))),
            });
        }
    }

    fn is_missing_expected_source_images(mod_entry: &ModEntry, snapshot: &GameBananaSnapshot) -> bool {
        if snapshot.preview_urls.is_empty() {
            return false;
        }
        let Some(mod_id) = mod_entry
            .source
            .as_ref()
            .and_then(|s| s.gamebanana.as_ref())
            .map(|l| l.mod_id)
        else {
            return true;
        };
        let meta_dir = mod_entry.root_path.join(MOD_META_DIR);
        if !meta_dir.exists() {
            return true;
        }
        snapshot.preview_urls.iter().enumerate().any(|(idx, url)| {
            let path_no_query = url.split('?').next().unwrap_or(url);
            let ext = Path::new(path_no_query)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("jpg");
            let file_name = format!("gb_{mod_id}_{}.{ext}", idx + 1);
            !meta_dir.join(file_name).exists()
        })
    }

    fn refresh_with_toast(&mut self) {
        self.mark_usage_counters_dirty();
        let old_ts: HashMap<String, DateTime<Utc>> = self.state.mods.iter()
            .map(|m| (m.id.clone(), m.updated_at))
            .collect();
        let game_id = self.selected_game().map(|g| g.definition.id.clone());
        let before = self.capture_reload_snapshots(game_id.as_deref());

        match xxmi::refresh_state(&mut self.state, game_id.as_deref()) {
            Ok(()) => {
                let after = self.capture_reload_snapshots(game_id.as_deref());
                let summary = self.build_reload_summary(&before, &after);
                self.invalidate_stale_mod_textures(&old_ts);
                self.backfill_missing_mod_images(game_id.as_deref());
                self.sync_tools_for_selected_game();
                self.save_state();
                self.sync_selection_after_refresh();
                self.queue_update_check_for_linked_mods(game_id.as_deref());
                self.request_automatic_app_update_check(0.0);
                self.push_log(format!(
                    "Reload: {}",
                    self.reload_summary_log_text(&summary)
                ));
                for line in &summary.detail_lines {
                    self.push_log(format!("Reload: {line}"));
                }
                self.set_message_ok(self.reload_summary_toast_text(&summary));
            }
            Err(err) => self.report_error(err, Some("Could not refresh mods")),
        }
    }

    fn capture_reload_snapshots(&self, game_id: Option<&str>) -> Vec<ReloadSnapshot> {
        let mut items: Vec<_> = self
            .state
            .mods
            .iter()
            .filter(|mod_entry| game_id.is_none_or(|id| mod_entry.game_id == id))
            .map(|mod_entry| ReloadSnapshot {
                id: mod_entry.id.clone(),
                folder_name: mod_entry.folder_name.clone(),
                root_path: mod_entry.root_path.clone(),
                status: mod_entry.status.clone(),
                updated_at: mod_entry.updated_at,
            })
            .collect();
        items.sort_by(|a, b| a.folder_name.to_lowercase().cmp(&b.folder_name.to_lowercase()));
        items
    }

    fn build_reload_summary(
        &self,
        before: &[ReloadSnapshot],
        after: &[ReloadSnapshot],
    ) -> ReloadSummary {
        let before_map: HashMap<_, _> = before.iter().map(|item| (&item.id, item)).collect();
        let after_map: HashMap<_, _> = after.iter().map(|item| (&item.id, item)).collect();
        let mut detail_lines = Vec::new();
        let mut added = 0usize;
        let mut removed = 0usize;
        let mut changed = 0usize;

        let mut added_items: Vec<_> = after
            .iter()
            .filter(|item| {
                !before_map.contains_key(&item.id)
                    && !self.pending_known_installed_paths.contains(&item.root_path)
            })
            .collect();
        added_items.sort_by(|a, b| a.folder_name.to_lowercase().cmp(&b.folder_name.to_lowercase()));
        for item in added_items {
            added += 1;
            detail_lines.push(format!("added {}", item.folder_name));
        }

        let mut removed_items: Vec<_> = before
            .iter()
            .filter(|item| !after_map.contains_key(&item.id))
            .collect();
        removed_items.sort_by(|a, b| a.folder_name.to_lowercase().cmp(&b.folder_name.to_lowercase()));
        for item in removed_items {
            removed += 1;
            detail_lines.push(format!("removed {}", item.folder_name));
        }

        let mut changed_items: Vec<_> = after
            .iter()
            .filter_map(|item| {
                let previous = before_map.get(&item.id)?;
                if previous.status != item.status
                    || previous.folder_name != item.folder_name
                    || previous.root_path != item.root_path
                    || previous.updated_at != item.updated_at
                {
                    Some(item)
                } else {
                    None
                }
            })
            .collect();
        changed_items.sort_by(|a, b| a.folder_name.to_lowercase().cmp(&b.folder_name.to_lowercase()));
        for item in changed_items {
            changed += 1;
            detail_lines.push(format!("changed {}", item.folder_name));
        }

        ReloadSummary {
            total_mods: after.len(),
            added,
            removed,
            changed,
            detail_lines,
        }
    }

    fn reload_summary_log_text(&self, summary: &ReloadSummary) -> String {
        if summary.added == 0 && summary.removed == 0 && summary.changed == 0 {
            format!("{} mods scanned, no changes", summary.total_mods)
        } else {
            let mut parts = vec![format!("{} mods scanned", summary.total_mods)];
            if summary.added > 0 {
                parts.push(format!("{} added", summary.added));
            }
            if summary.removed > 0 {
                parts.push(format!("{} removed", summary.removed));
            }
            if summary.changed > 0 {
                parts.push(format!("{} changed", summary.changed));
            }
            parts.join(", ")
        }
    }

    fn reload_summary_toast_text(&self, summary: &ReloadSummary) -> String {
        if summary.added == 0 && summary.removed == 0 && summary.changed == 0 {
            format!("Reloaded: {} mods, no changes", summary.total_mods)
        } else {
            let mut parts = vec![format!("Reloaded: {} mods", summary.total_mods)];
            if summary.added > 0 {
                parts.push(format!("{} added", summary.added));
            }
            if summary.removed > 0 {
                parts.push(format!("{} removed", summary.removed));
            }
            if summary.changed > 0 {
                parts.push(format!("{} changed", summary.changed));
            }
            parts.join(", ")
        }
    }


    fn sync_selection_after_refresh(&mut self) {
        let live_ids: HashSet<_> = self.state.mods.iter().map(|item| item.id.clone()).collect();
        self.selected_mods.retain(|id| live_ids.contains(id));
        if self
            .selected_mod_id
            .as_ref()
            .is_some_and(|id| !live_ids.contains(id))
        {
            self.set_selected_mod_id(None);
        }
    }

    fn set_selected_game(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.state.games.len() {
            return;
        }
        let previous_game_id = self
            .selected_game()
            .map(|game| game.definition.id.clone());
        self.selected_game = index;
        let game_id = self.state.games[index].definition.id.clone();
        self.state.last_selected_game_id = Some(game_id.clone());
        self.save_state();

        if !self.game_icon_textures.contains_key(&game_id) {
            if let Some(bytes) = game_icon_bytes(&game_id) {
                if let Some(texture) =
                    load_title_icon_texture(ctx, bytes, &format!("game-icon-{game_id}"))
                {
                    self.game_icon_textures.insert(game_id.clone(), texture);
                }
            }
        }

        if !self.game_cover_textures.contains_key(&game_id) {
            if let Some(bytes) = game_cover_bytes(&game_id) {
                if let Some(texture) =
                    load_cover_texture(ctx, bytes, &format!("game-cover-{game_id}"))
                {
                    self.game_cover_textures.insert(game_id.clone(), texture);
                }
            }
        }

        self.enqueue_icon_preload();
        self.enqueue_cover_preload();

        if previous_game_id.as_deref() != Some(game_id.as_str()) {
            self.image_generation.fetch_add(1, Ordering::Relaxed);
            self.pending_mod_image_queue.clear();
            self.pending_mod_image_requests.clear();

            self.set_selected_mod_id(None);
            self.selected_mods.clear();
            self.clear_dynamic_textures();
            self.browse_query.clear();
            self.reset_browse_for_game(&game_id);
            let selected_mods_root = self
                .selected_game()
                .and_then(|game| game.mods_path(self.state.use_default_mods_path));
            let _ = persistence::cleanup_orphan_tmp_files(
                selected_mods_root.as_deref(),
                &HashSet::new(),
            );
            self.queue_update_check_for_linked_mods(Some(&game_id));
        }
    }

    fn next_background_job_id(&mut self) -> u64 {
        let id = self.install_next_job_id;
        self.install_next_job_id = self.install_next_job_id.wrapping_add(1);
        id
    }
}
