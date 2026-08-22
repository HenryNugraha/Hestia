const PROXY_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

async fn validate_proxy_manifest(proxy: &Option<CustomProxyConfig>) -> Result<()> {
    let client = RuntimeServices::http_client_for(proxy)?;
    RuntimeServices::async_client_builder_for(proxy)
        .timeout(PROXY_PROBE_TIMEOUT)
        .build()
        .map_err(|err| anyhow!("failed to create async proxy client: {err}"))?;
    tokio::time::timeout(PROXY_PROBE_TIMEOUT, fetch_app_update_manifest(&client))
        .await
        .map_err(|_| anyhow!("proxy validation timed out"))??;
    Ok(())
}

async fn validate_blocking_proxy_client(proxy: &Option<CustomProxyConfig>) -> Result<()> {
    let proxy = proxy.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("hestia-proxy-client-check".to_string())
        .spawn(move || {
            let result = (|| -> Result<()> {
                let builder = reqwest::blocking::Client::builder().timeout(Duration::from_secs(30));
                let builder = match &proxy {
                    Some(proxy) => builder.proxy(reqwest::Proxy::all(proxy.endpoint())?),
                    None => builder,
                };
                let _client = builder
                    .build()
                    .map_err(|err| anyhow!("failed to create blocking proxy client: {err}"))?;
                Ok(())
            })();
            let _ = tx.send(result);
        })
        .map_err(|err| anyhow!("failed to start blocking proxy validation thread: {err}"))?;
    rx.await
        .map_err(|_| anyhow!("blocking proxy validation thread stopped unexpectedly"))?
}

fn proxy_socket_address(proxy: &CustomProxyConfig) -> Option<String> {
    let url = url::Url::parse(proxy.endpoint()).ok()?;
    let host = url.host_str()?;
    let port = url.port()?;
    Some(if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    })
}

async fn probe_proxy_ports(candidates: &[Option<CustomProxyConfig>]) -> HashSet<String> {
    let mut addresses = HashSet::new();
    let mut probes = futures_util::stream::FuturesUnordered::new();
    for address in candidates.iter().flatten().filter_map(proxy_socket_address) {
        if !addresses.insert(address.clone()) {
            continue;
        }
        probes.push(async move {
            let open = tokio::time::timeout(
                PROXY_PROBE_TIMEOUT,
                tokio::net::TcpStream::connect(&address),
            )
            .await
            .is_ok_and(|result| result.is_ok());
            (address, open)
        });
    }

    let mut open = HashSet::new();
    while let Some((endpoint, is_open)) = probes.next().await {
        if is_open {
            open.insert(endpoint);
        }
    }
    open
}

impl HestiaApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        portable: PortablePaths,
        mut state: AppState,
        runtime_services: RuntimeServices,
        startup_path_scan_due: bool,
        auto_renderer_label: &'static str,
    ) -> Self {
        install_app_fonts(&cc.egui_ctx, state.static_prefs.font_style);
        apply_theme(&cc.egui_ctx);
        let _ = UI_REPAINT_CONTEXT.set(cc.egui_ctx.clone());
        let (icon_request_tx, icon_request_rx) = worker_channel::<IconRequest>();
        let (icon_result_tx, icon_result_rx) = worker_channel::<IconResult>();
        spawn_icon_worker(&runtime_services, icon_request_rx, icon_result_tx);
        let image_generation = Arc::new(AtomicU64::new(0));
        let (mod_image_request_tx, mod_image_request_rx) =
            worker_channel::<LocalModImageRequest>();
        let (mod_image_result_tx, mod_image_result_rx) =
            worker_channel::<LocalModImageResult>();
        let (manual_image_event_tx, manual_image_event_rx) =
            worker_channel::<ManualImageEvent>();
        let (overlay_copy_event_tx, overlay_copy_event_rx) =
            worker_channel::<OverlayImageCopyEvent>();
        let cache_limit_bytes =
            Arc::new(AtomicU64::new(state.static_prefs.cache_size_tier.bytes()));
        spawn_local_mod_image_worker(
            &runtime_services,
            portable.clone(),
            Arc::clone(&image_generation),
            Arc::clone(&cache_limit_bytes),
            mod_image_request_rx,
            mod_image_result_tx,
        );
        let game_icon_textures = HashMap::new();
        let (cover_request_tx, cover_request_rx) = worker_channel::<CoverRequest>();
        let (cover_result_tx, cover_result_rx) = worker_channel::<CoverResult>();
        spawn_cover_worker(&runtime_services, cover_request_rx, cover_result_tx);
        let (install_request_tx, install_request_rx) =
            worker_channel::<InstallRequest>();
        let (install_event_tx, install_event_rx) = worker_channel::<InstallEvent>();
        spawn_install_workers(
            &runtime_services,
            portable.clone(),
            install_request_rx,
            install_event_tx,
        );
        let (browse_request_tx, browse_request_rx) =
            worker_channel::<BrowseRequest>();
        let (browse_event_tx, browse_event_rx) = worker_channel::<BrowseEvent>();
        spawn_browse_worker(
            &runtime_services,
            portable.clone(),
            browse_request_rx,
            browse_event_tx,
        );
        let (browse_image_request_tx, browse_image_request_rx) =
            worker_channel::<BrowseImageRequest>();
        let (browse_image_result_tx, browse_image_result_rx) =
            worker_channel::<BrowseImageResult>();
        let youtube_icon_texture =
            load_image_texture(&cc.egui_ctx, youtube_icon_bytes(), "youtube-icon");
        spawn_browse_image_workers(
            &runtime_services,
            portable.clone(),
            Arc::clone(&cache_limit_bytes),
            browse_image_request_rx,
            browse_image_result_tx,
        );
        let (browse_download_result_tx, browse_download_event_rx) =
            worker_channel::<BrowseDownloadEvent>();
        let (app_update_event_tx, app_update_event_rx) =
            worker_channel::<AppUpdateEvent>();
        let (proxy_apply_tx, proxy_apply_rx) = worker_channel::<ProxyApplyEvent>();
        let (feedback_survey_submit_tx, feedback_survey_worker_rx) =
            worker_channel::<FeedbackSurveySubmitRequest>();
        let (feedback_survey_worker_tx, feedback_survey_submit_rx) =
            worker_channel::<FeedbackSurveySubmitEvent>();
        spawn_feedback_survey_submit_worker(
            &runtime_services,
            feedback_survey_worker_rx,
            feedback_survey_worker_tx,
        );
        let (translation_request_tx, translation_request_rx) =
            worker_channel::<TranslationRequest>();
        let (translation_event_tx, translation_event_rx) =
            worker_channel::<TranslationEvent>();
        spawn_translation_worker(
            &runtime_services,
            &portable,
            translation_request_rx,
            translation_event_tx,
        );
        let (update_check_tx, update_check_worker_rx) =
            worker_channel::<UpdateCheckRequest>();
        let (update_check_worker_tx, update_check_rx) =
            worker_channel::<UpdateCheckResult>();
        spawn_update_check_worker(
            &runtime_services,
            portable.clone(),
            update_check_worker_rx,
            update_check_worker_tx,
        );
        let (refresh_request_tx, refresh_request_rx) =
            worker_channel::<RefreshRequest>();
        let (refresh_result_tx, refresh_result_rx) =
            worker_channel::<RefreshEvent>();
        spawn_selected_game_refresh_worker(
            &runtime_services,
            refresh_request_rx,
            refresh_result_tx,
        );
        let (xxmi_reload_event_tx, xxmi_reload_event_rx) = worker_channel::<XxmiReloadEvent>();
        let (grant_access_event_tx, grant_access_event_rx) = worker_channel::<GrantAccessEvent>();
        let (hotkey_customization_tx, hotkey_customization_request_rx) =
            worker_channel::<HotkeyCustomizationRequest>();
        let (hotkey_customization_event_tx, hotkey_customization_rx) =
            worker_channel::<HotkeyCustomizationEvent>();
        spawn_hotkey_customization_worker(
            &runtime_services,
            hotkey_customization_request_rx,
            hotkey_customization_event_tx,
        );
        let (profile_request_tx, profile_request_rx) = worker_channel::<ProfileRequest>();
        let (profile_reconcile_request_tx, profile_reconcile_request_rx) =
            worker_channel::<ProfileReconcileSpec>();
        let (profile_archive_tx, profile_archive_rx) = worker_channel::<ProfileArchiveJob>();
        let (profile_event_tx, profile_event_rx) = worker_channel::<ProfileEvent>();
        let profile_archive_coordinator = Arc::new(ProfileArchiveCoordinator::default());
        spawn_profile_archive_worker(
            &runtime_services,
            profile_archive_rx,
            profile_event_tx.clone(),
            Arc::clone(&profile_archive_coordinator),
        );
        spawn_profile_worker(
            &runtime_services,
            profile_request_rx,
            profile_event_tx.clone(),
            profile_archive_tx,
            profile_archive_coordinator,
        );
        spawn_profile_reconcile_worker(
            &runtime_services,
            profile_reconcile_request_rx,
            profile_event_tx,
        );
        let app_icon_texture = load_title_icon_texture(&cc.egui_ctx, app_icon_bytes(), "app-icon");
        let selected_game = resolve_last_selected_game(&state).unwrap_or(0);
        let game_cover_textures = HashMap::new();
        let mod_thumbnail_placeholder = load_cover_texture(
            &cc.egui_ctx,
            mod_thumbnail_placeholder_bytes(),
            "mod-thumb-placeholder",
        );
        let mod_cover_textures = HashMap::new();
        let mod_full_textures = HashMap::new();
        state.mods.clear();
        Self::auto_detect_game_paths(&mut state);
        let (startup_scan_tx, startup_scan_rx) =
            worker_channel::<StartupScanEvent>();
        let (startup_path_scan_tx, startup_path_scan_rx) =
            worker_channel::<StartupPathScanEvent>();
        let startup_path_targets = if startup_path_scan_due {
            Self::startup_path_scan_targets(&state, false)
        } else {
            Vec::new()
        };
        if startup_path_scan_due && startup_path_targets.is_empty() {
            state.startup_path_scan_completed = true;
        }
        let startup_path_scan = Self::build_startup_path_scan_state(&startup_path_targets, true);
        state.tasks.retain(|task| task.status.is_terminal());
        state.show_log = false;
        state.show_tasks = false;
        state.show_tools = false;
        let show_whats_new = state.show_whats_new;
        let log_scroll_to_bottom = state.show_log;
        let log_window_nonce = if state.show_log { 1 } else { 0 };
        let log_force_default_pos = state.show_log;
        let whats_new_window_nonce = if show_whats_new { 1 } else { 0 };
        let whats_new_force_default_pos = show_whats_new;
        let show_feedback_survey = state.show_feedback_survey;
        let feedback_survey_window_nonce = if show_feedback_survey { 1 } else { 0 };
        let feedback_survey_force_default_pos = show_feedback_survey;
        let tools_window_nonce = if state.show_tools { 1 } else { 0 };
        let tools_force_default_pos = state.show_tools;
        let tasks_window_nonce = if state.show_tasks { 1 } else { 0 };
        let tasks_force_default_pos = state.show_tasks;
        let window_state_cache = Some(WindowStateSnapshot {
            pos: state.static_prefs.window_pos,
            size: state.static_prefs.window_size,
            maximized: state.static_prefs.window_maximized,
        });
        let window_was_maximized = state.static_prefs.window_maximized;
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
            worker_channel::<GifPreviewRequest>();
        let (gif_preview_event_tx, gif_preview_event_rx) =
            worker_channel::<GifPreviewEvent>();
        spawn_gif_preview_worker(
            &runtime_services,
            gif_preview_request_rx,
            gif_preview_event_tx,
        );
        let (gif_animation_request_tx, gif_animation_request_rx) =
            worker_channel::<GifAnimationRequest>();
        let (gif_animation_event_tx, gif_animation_event_rx) =
            worker_channel::<GifAnimationEvent>();
        spawn_gif_animation_worker(
            &runtime_services,
            gif_animation_request_rx,
            gif_animation_event_tx,
        );

        let applied_custom_proxy = runtime_services.custom_proxy();
        let proxy_url_draft = state.static_prefs.custom_proxy_url.clone();
        // `wgpu_render_state` is None exactly when eframe runs the glow backend.
        let active_renderer_label = match cc
            .wgpu_render_state
            .as_ref()
            .map(|render_state| render_state.adapter.get_info().backend)
        {
            Some(eframe::wgpu::Backend::Dx12) => "DirectX 12",
            Some(eframe::wgpu::Backend::Vulkan) => "Vulkan",
            Some(eframe::wgpu::Backend::Metal) => "Metal",
            Some(eframe::wgpu::Backend::Gl) => "OpenGL (wgpu)",
            Some(_) => "wgpu",
            None => "OpenGL",
        };
        let boot_renderer_pref = state.static_prefs.renderer;
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
            show_check_skipped_mods: true,
            show_missing_source_mods: true,
            show_modified_locally_mods: true,
            show_ignoring_update_mods: true,
            selected_category_folder_id: None,
            library_scroll_to_category_id: None,
            library_card_cache: LibraryCardCache::default(),
            dragging_mod_ids: Vec::new(),
            current_view: ViewMode::Library,
            settings_open: false,
            settings_window_nonce: 0,
            mod_detail_window_nonce: 0,
            browse_detail_window_nonce: 0,
            active_renderer_label,
            auto_renderer_label,
            boot_renderer_pref,
            proxy_url_draft,
            proxy_url_validation_error: None,
            applied_custom_proxy,
            proxy_apply_inflight: false,
            proxy_apply_locks_input: false,
            proxy_apply_silent_success: false,
            proxy_apply_tx,
            proxy_apply_rx,
            mod_detail_open: false,
            browse_detail_open: false,
            settings_tab: SettingsTab::General,
            mod_detail_tab: ModDetailTab::Overview,
            last_titlebar_rect: None,
            last_right_pane_rect: None,
            mod_detail_focus_requested: false,
            browse_detail_focus_requested: false,
            mod_detail_editing: false,
            mod_detail_edit_target_id: None,
            mod_detail_rename_focus_target_id: None,
            mod_detail_edit_name: String::new(),
            mod_keybinds_available_cache: HashMap::new(),
            metadata_hotkeys_view: None,
            mod_hotkey_values_cache: HashMap::new(),
            mod_hotkey_values_loading: HashSet::new(),
            live_state_watch: None,
            hotkeys_write_block_cache: None,
            hotkey_customization_tx,
            hotkey_customization_rx,
            hotkey_clear_inflight: HashSet::new(),
            hotkey_clear_confirm_target_id: None,
            personal_note_edit_target_id: None,
            personal_note_edit_text: String::new(),
            #[cfg(windows)]
            clipboard_image_paste_held: false,
            overlay_copy_ctrl_c_held: false,
            category_rename_target_id: None,
            category_rename_focus_target_id: None,
            category_rename_surface: None,
            category_rename_name: String::new(),
            dragging_category_id: None,
            dragging_category_target_index: None,
            selected_category_ids: HashSet::new(),
            settings_dragging_category_ids: Vec::new(),
            settings_dragging_category_target_index: None,
            toasts: Vec::new(),
            pending_imports: VecDeque::new(),
            pending_conflicts: VecDeque::new(),
            whats_new_window_nonce,
            whats_new_force_default_pos,
            feedback_survey_window_nonce,
            feedback_survey_force_default_pos,
            feedback_survey_answers: HashMap::new(),
            feedback_survey_message: String::new(),
            feedback_survey_privacy_expanded: false,
            feedback_survey_submitting: false,
            feedback_survey_cancellation: None,
            feedback_survey_active_request: None,
            pending_proxy_survey_resume: None,
            feedback_survey_submit_tx,
            feedback_survey_submit_rx,
            log_scroll_to_bottom,
            log_window_nonce,
            log_force_default_pos,
            log_revision: 0,
            log_display_cache: LogDisplayCache::default(),
            tools_window_nonce,
            tools_force_default_pos,
            new_tool_ids: HashSet::new(),
            tools_new_badges_shown: false,
            tool_launch_options_prompt: None,
            dragging_window_tool_id: None,
            dragging_window_tool_target_index: None,
            dragging_titlebar_tool_id: None,
            dragging_titlebar_tool_target_index: None,
            dragging_game_id: None,
            dragging_game_target_index: None,
            tasks_window_nonce,
            tasks_force_default_pos,
            tasks_tab: TasksTab::Installs,
            tasks_scroll_to_edge: false,
            task_row_advance_cache: HashMap::new(),
            task_row_advance_cache_width: 0.0,
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
            tool_icon_texture_failures: HashMap::new(),
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
            manual_image_event_tx,
            manual_image_event_rx,
            overlay_copy_event_tx,
            overlay_copy_event_rx,
            manual_image_imports_pending: 0,
            pending_mod_image_requests: HashSet::new(),
            pending_mod_image_queue: Vec::new(),
            pending_image_loads: HashSet::new(),
            inflight_full_image_requests: HashSet::new(),
            mod_thumb_unavailable: HashMap::new(),
            image_queue_len_after_logic: 0,
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
            pending_proxy_app_update_resume: None,
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
            proxy_requeue_browse_downloads: HashSet::new(),
            pending_browse_install_safety: HashMap::new(),
            pending_browse_install_meta: HashMap::new(),
            gif_rewritten_markdown_cache: HashMap::new(),
            markdown_dependency_signature_cache: HashMap::new(),
            render_safe_markdown_cache: HashMap::new(),
            path_file_status_cache: Mutex::new(HashMap::new()),
            path_write_status_cache: Mutex::new(HashMap::new()),
            browse_commonmark_cache: CommonMarkCache::default(),
            browse_request_nonce: 0,
            browse_page_generation: 0,
            browse_detail_generation: 0,
            browse_detail_request_nonces: HashMap::new(),
            image_generation,
            translation_request_tx,
            translation_event_rx,
            translation_inflight: HashMap::new(),
            unlinked_translation_inflight: HashMap::new(),
            unlinked_translation_cancellations: HashMap::new(),
            translation_request_nonce: 0,
            cancelled_translation_requests: HashSet::new(),
            my_mods_translation_state: HashMap::new(),
            my_mod_updates: HashMap::new(),
            my_mod_updates_inflight: HashSet::new(),
            update_check_tx,
            update_check_rx,
            update_check_inflight: false,
            update_check_generation: 0,
            update_check_active_items: Vec::new(),
            pending_update_check_game: None,
            pending_update_check_mods: HashSet::new(),
            refresh_request_tx,
            refresh_result_rx,
            refresh_inflight: false,
            refresh_pending_selected_game: None,
            xxmi_reload_event_tx,
            xxmi_reload_event_rx,
            grant_access_event_tx,
            grant_access_event_rx,
            grant_access_inflight: false,
            xxmi_reload_inflight: HashSet::new(),
            xxmi_reload_pending: HashSet::new(),
            xxmi_namespace_cache: HashMap::new(),
            profile_request_tx,
            profile_reconcile_request_tx,
            profile_event_rx,
            profile_operation_inflight: None,
            profile_compression_states: HashMap::new(),
            profile_next_operation_id: 1,
            profile_recovery_queue: VecDeque::new(),
            profile_recovery_failed_games: HashSet::new(),
            profile_reconcile_inflight: HashSet::new(),
            profile_selector_popup_open_last_frame: false,
            profile_name_prompt: None,
            profile_name_target_id: None,
            profile_name_draft: String::new(),
            pending_profile_delete_id: None,
            pending_d3dx_foreground_conflict: None,
            pending_reload_summary: None,
            pending_install_finalize: HashMap::new(),
            pending_known_installed_paths: HashSet::new(),
            reload_spin_until: 0.7,
            reload_was_busy: true,
            my_mod_source_expanded: false,
            mod_detail_source_focus_pending: false,
            mod_detail_source_glow_start: None,
            cache_limit_bytes,
            usage_cache_bytes: 0,
            usage_archive_bytes: 0,
            usage_counters_last_refresh: 0.0,
            usage_counters_dirty: true,
            window_state_cache,
            window_state_last_save: 0.0,
            floating_window_save_due: None,
            window_was_maximized,
            selection_empty_at: None,
            startup_scan_loading: true,
            startup_launch_pending: true,
            startup_selected_game: selected_game,
            startup_path_targets_pending: startup_path_targets,
            startup_scan_tx,
            startup_scan_rx,
            startup_path_scan,
            startup_path_scan_tx,
            startup_path_scan_rx,
            gif_preview_request_tx,
            gif_preview_event_rx,
            gif_animation_request_tx,
            gif_animation_event_rx,
            gif_dest_by_texture_key: HashMap::new(),
            pending_gif_previews: HashMap::new(),
            gif_preview_requests_in_flight: 0,
            pending_gif_animations: HashMap::new(),
            gif_animation_requests_in_flight: 0,
            animated_gif_state: HashMap::new(),
            visible_gif_process_texture_keys: HashSet::new(),
            visible_gif_texture_keys: HashSet::new(),
            last_visible_gif_texture_keys: HashSet::new(),
            pending_events: PendingEventsFlags::default(),
        };
        Self::cleanup_runtime_temp_downloads_best_effort();
        app.release_stuck_xxmi_reload_hotkeys();
        app.dispatch_profile_recovery();
        if app.state.static_prefs.use_custom_proxy
            && !app.state.static_prefs.custom_proxy_url.trim().is_empty()
        {
            app.request_startup_custom_proxy_check();
        }
        app
    }

    fn complete_startup_launch(&mut self, ctx: &egui::Context) {
        // Only wait while recovery is still *running*. A recovery that failed must not hold
        // the launch: the failure is per-game (tracked in `profile_recovery_failed_games`,
        // which blocks profile operations for that game alone), and keeping the whole app in
        // the startup-loading state would freeze every other game's library over one
        // inaccessible install.
        let profile_recovery_pending = self
            .profile_operation_inflight
            .as_ref()
            .is_some_and(|operation| operation.kind == ProfileOperationKind::Recover)
            || !self.profile_recovery_queue.is_empty();
        if !self.startup_launch_pending || self.proxy_apply_inflight || profile_recovery_pending {
            return;
        }
        self.startup_launch_pending = false;
        self.retry_pending_feedback_survey_on_launch();
        self.set_selected_game(self.startup_selected_game, ctx);
        self.ensure_selected_game_enabled(ctx);
        let _ = self.ensure_selected_game_default_profile();
        let startup_path_targets = std::mem::take(&mut self.startup_path_targets_pending);
        if startup_path_targets.is_empty() {
            self.dispatch_startup_mod_scan();
        } else {
            self.startup_scan_loading = false;
            self.dispatch_startup_path_scan(startup_path_targets);
        }
    }

    fn save_state(&mut self) {
        if let Err(err) = persistence::save_app_state(&self.portable, &self.state) {
            self.report_error_message(
                format!("failed to save app state: {err:#}"),
                Some(self.text().could_not_save_settings()),
            );
        }
    }

    fn request_custom_proxy_apply(&mut self) {
        if self.proxy_apply_inflight {
            return;
        }
        if !self.state.static_prefs.use_custom_proxy {
            self.apply_direct_networking();
            return;
        }
        let mut has_saved_proxy = false;
        let proxy_candidates = CustomProxyConfig::parse_candidates(
            &self.state.static_prefs.custom_proxy_url,
        )
        .map(|mut candidates| {
            if let Ok(saved) =
                CustomProxyConfig::parse(&self.state.static_prefs.custom_proxy_resolved_url)
            {
                has_saved_proxy = true;
                candidates.retain(|candidate| candidate != &saved);
                candidates.insert(0, saved);
            }
            candidates.into_iter().map(Some).collect::<Vec<_>>()
        });
        let proxy_candidates = match proxy_candidates {
            Ok(candidates) => candidates,
            Err(error) => {
                self.proxy_url_validation_error = Some(error);
                return;
            }
        };
        self.proxy_apply_locks_input = true;
        self.proxy_apply_inflight = true;
        let tx = self.proxy_apply_tx.clone();
        self.runtime_services.spawn(async move {
            let result = async {
                let mut candidates = proxy_candidates;

                if has_saved_proxy {
                    let saved = candidates.remove(0);
                    match validate_proxy_manifest(&saved).await {
                        Ok(()) => {
                            validate_blocking_proxy_client(&saved).await?;
                            return Ok(saved);
                        }
                        Err(_) => {}
                    }
                }

                if candidates.len() <= 1 {
                    let proxy = candidates.pop().expect("proxy candidate must exist");
                    validate_proxy_manifest(&proxy).await?;
                    validate_blocking_proxy_client(&proxy).await?;
                    return Ok(proxy);
                }

                let open_endpoints = probe_proxy_ports(&candidates).await;
                let candidates: Vec<_> = candidates
                    .into_iter()
                    .filter(|proxy| {
                        proxy
                            .as_ref()
                            .and_then(proxy_socket_address)
                            .is_some_and(|address| open_endpoints.contains(&address))
                    })
                    .collect();
                if candidates.is_empty() {
                    bail!("none of the configured proxy ports accepted a TCP connection");
                }

                let mut validations = futures_util::stream::FuturesUnordered::new();
                for proxy in &candidates {
                    let proxy = proxy.clone();
                    validations
                        .push(async move { validate_proxy_manifest(&proxy).await.map(|_| proxy) });
                }
                let mut validated = HashSet::new();
                while let Some(result) = validations.next().await {
                    if let Ok(Some(proxy)) = result {
                        validated.insert(proxy.endpoint().to_string());
                    }
                }
                let Some(proxy) = candidates.into_iter().find(|proxy| {
                    proxy
                        .as_ref()
                        .is_some_and(|proxy| validated.contains(proxy.endpoint()))
                }) else {
                    bail!("none of the reachable proxy ports passed manifest validation");
                };
                validate_blocking_proxy_client(&proxy).await?;
                Ok(proxy)
            }
            .await;
            let event = match result {
                Ok(proxy) => ProxyApplyEvent::Validated { proxy },
                Err(err) => ProxyApplyEvent::Failed {
                    error: format!("{err:#}"),
                },
            };
            let _ = tx.send(event);
        });
    }

    fn apply_direct_networking(&mut self) {
        if let Err(err) = self.runtime_services.replace_custom_proxy(None) {
            self.report_error_message(
                format!("failed to disable proxy: {err:#}"),
                Some(self.text().proxy_connection_failed()),
            );
            return;
        }
        self.applied_custom_proxy = None;
        self.state.static_prefs.custom_proxy_resolved_url.clear();
        self.save_state();
        self.restart_network_work_for_proxy_change();
        self.set_message_ok(self.text().proxy_disabled());
    }

    fn request_startup_custom_proxy_check(&mut self) {
        if CustomProxyConfig::parse_candidates(&self.state.static_prefs.custom_proxy_url).is_err() {
            self.disable_proxy_after_startup_check_failure("saved proxy address is invalid");
            return;
        }
        self.proxy_apply_silent_success = true;
        self.request_custom_proxy_apply();
    }

    fn disable_proxy_after_startup_check_failure(&mut self, detail: impl Into<String>) {
        self.state.static_prefs.use_custom_proxy = false;
        self.state.static_prefs.custom_proxy_resolved_url.clear();
        self.runtime_services
            .replace_custom_proxy(None)
            .expect("direct HTTP client must be constructible");
        self.applied_custom_proxy = None;
        self.settings_open = true;
        self.settings_tab = SettingsTab::Advanced;
        self.save_state();
        self.report_error_message(detail, Some(self.text().proxy_connection_failed()));
    }

    fn consume_proxy_apply_events(&mut self) {
        while let Ok(event) = self.proxy_apply_rx.try_recv() {
            self.proxy_apply_inflight = false;
            self.proxy_apply_locks_input = false;
            let silent_success = std::mem::take(&mut self.proxy_apply_silent_success);
            match event {
                ProxyApplyEvent::Validated { proxy } => {
                    if let Err(err) = self.runtime_services.replace_custom_proxy(proxy.clone()) {
                        self.report_error_message(
                            format!("failed to activate proxy: {err:#}"),
                            Some(self.text().proxy_connection_failed()),
                        );
                        continue;
                    }
                    self.applied_custom_proxy = proxy;
                    self.state.static_prefs.custom_proxy_resolved_url = self
                        .applied_custom_proxy
                        .as_ref()
                        .map(|proxy| proxy.endpoint().to_string())
                        .unwrap_or_default();
                    self.save_state();
                    self.restart_network_work_for_proxy_change();
                    if silent_success {
                        continue;
                    }
                    if self.applied_custom_proxy.is_some() {
                        self.set_message_ok(self.text().proxy_enabled());
                    } else {
                        self.set_message_ok(self.text().proxy_disabled());
                    }
                }
                ProxyApplyEvent::Failed { error } => {
                    if silent_success {
                        self.disable_proxy_after_startup_check_failure(format!(
                            "proxy startup check failed: {error}"
                        ));
                        continue;
                    }
                    // The visible switch must describe the still-active client after a failed
                    // candidate probe. Keep the draft text so the user can correct it.
                    self.state.static_prefs.use_custom_proxy = self.applied_custom_proxy.is_some();
                    self.report_error_message(
                        format!("proxy validation failed: {error}"),
                        Some(self.text().proxy_connection_failed()),
                    );
                }
            }
        }
    }

    /// Relaunches the app so a changed renderer preference takes effect: the
    /// renderer is fixed at window creation. `--after-proxy-restart` only skips
    /// the single-instance guard, which the new process needs while this one is
    /// still shutting down.
    fn restart_for_renderer_change(&mut self) {
        if self.has_active_mod_tasks() {
            self.report_warn(
                "renderer restart blocked while tasks are active",
                Some(self.text().app_update_wait_for_active_tasks()),
            );
            return;
        }
        self.save_state();
        crate::integrations::xxmi_persist::release_synthetic_keys_for_shutdown();
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe)
                .arg("--after-proxy-restart")
                .spawn();
        }
        std::process::exit(0);
    }

    /// Relaunches the app elevated via a UAC prompt. Session-only: elevation is not
    /// remembered across launches, and Explorer drag-and-drop cannot reach an elevated
    /// window (UIPI), which the warning copy tells the user about.
    fn restart_as_administrator(&mut self) {
        if self.has_active_mod_tasks() {
            self.report_warn(
                "elevated restart blocked while tasks are active",
                Some(self.text().app_update_wait_for_active_tasks()),
            );
            return;
        }
        self.save_state();
        match launch_self_elevated("--after-elevated-restart") {
            Ok(()) => {
                crate::integrations::xxmi_persist::release_synthetic_keys_for_shutdown();
                std::process::exit(0)
            }
            Err(err) => self.report_error(err, Some(self.text().restart_as_admin())),
        }
    }

    /// The directory whose ACL governs every mod operation for this game: the parent of the
    /// profile storage dir, which contains the mods dir, the disabled-mods dir, and profile
    /// archives. Granting access here (with inheritance) unblocks all of them.
    fn game_write_scope_dir(&self, game: &GameInstall) -> Option<PathBuf> {
        let use_default = self.state.static_prefs.use_default_mods_path;
        if let Ok(roots) = profiles::profile_roots(game, use_default) {
            if let Some(parent) = roots.profiles_dir.parent() {
                return Some(parent.to_path_buf());
            }
        }
        game.mods_path(use_default)
            .and_then(|path| path.parent().map(Path::to_path_buf))
    }

    fn start_grant_game_dir_access(&mut self, game_id: &str, dir: PathBuf) {
        if self.grant_access_inflight {
            return;
        }
        self.grant_access_inflight = true;
        let event_tx = self.grant_access_event_tx.clone();
        let event_game_id = game_id.to_string();
        let spawned = std::thread::Builder::new()
            .name(format!("hestia-grant-access-{game_id}"))
            .spawn(move || {
                let error = elevated_grant_dir_access(&dir)
                    .err()
                    .map(|err| format!("{err:#}"));
                let _ = event_tx.send(GrantAccessEvent {
                    game_id: event_game_id,
                    error,
                });
            });
        if let Err(err) = spawned {
            self.grant_access_inflight = false;
            self.report_error(
                anyhow!("failed to start access grant thread: {err}"),
                Some(self.text().grant_access()),
            );
        }
    }

    fn consume_grant_access_events(&mut self) {
        while let Ok(event) = self.grant_access_event_rx.try_recv() {
            self.grant_access_inflight = false;
            match event.error {
                None => {
                    let game_id = event.game_id;
                    self.push_log(format!("granted write access to the game folder ({game_id})"));
                    self.invalidate_path_write_status_cache();
                    // Storage recovery and the mod scan were skipped or degraded while the
                    // directory was read-only; rerun both now that writes work.
                    self.queue_profile_recovery_for_game(&game_id);
                    self.queue_game_refresh(game_id);
                }
                Some(error) => {
                    self.report_error_message(error, Some(self.text().grant_access()));
                }
            }
        }
    }

    fn restart_network_work_for_proxy_change(&mut self) {
        // Never touch extraction, installation, archive moves, or scans. Only the requests
        // that own network sockets are cancelled, and resumable transfers retain their partials.
        if let Some(inflight) = &self.app_update_download_inflight {
            self.pending_proxy_app_update_resume = Some(inflight.manifest.clone());
            inflight.cancel.store(true, Ordering::Relaxed);
        }

        for task_id in self
            .browse_download_inflight
            .keys()
            .copied()
            .collect::<Vec<_>>()
        {
            self.proxy_requeue_browse_downloads.insert(task_id);
            if let Some(inflight) = self.browse_download_inflight.get(&task_id) {
                inflight.cancel.store(true, Ordering::Relaxed);
            }
        }

        let image_requests: Vec<_> = self
            .browse_image_inflight
            .drain()
            .map(|(_, inflight)| {
                inflight.cancel.store(true, Ordering::Relaxed);
                let mut request = inflight.request;
                request.cancel = Arc::new(AtomicBool::new(false));
                request
            })
            .collect();
        self.browse_image_queue.extend(image_requests);
        for request in &mut self.browse_image_queue {
            if request.cancel.load(Ordering::Relaxed) {
                request.cancel = Arc::new(AtomicBool::new(false));
            }
        }

        self.browse_request_tx.send(BrowseRequest::CancelPage).ok();
        self.browse_page_generation = self.browse_page_generation.wrapping_add(1);
        if self.browse_state.loading_page || self.current_view == ViewMode::Browse {
            let page = self.browse_state.next_page.saturating_sub(1).max(1);
            self.request_browse_page_with_mode(page, true);
        }
        if self.browse_state.character_categories_loading {
            self.request_browse_character_categories(true);
        }
        if let Some(mod_id) = self.browse_state.selected_mod_id.filter(|mod_id| {
            self.browse_detail_open && self.browse_state.loading_details.contains_key(mod_id)
        }) {
            self.browse_state.loading_details.remove(&mod_id);
            self.request_browse_detail(mod_id);
        }

        self.restart_active_translations_for_proxy_change();
        self.restart_active_update_check_for_proxy_change();
        if let (Some(cancel), Some(request)) = (
            self.feedback_survey_cancellation.as_ref(),
            self.feedback_survey_active_request.clone(),
        ) {
            self.pending_proxy_survey_resume = Some(request);
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn dispatch_startup_mod_scan(&mut self) {
        self.startup_scan_loading = true;
        let scan_tx = self.startup_scan_tx.clone();
        let scan_runtime = self.runtime_services.handle();
        let mut startup_scan_state = self.state.clone();
        self.runtime_services.spawn(async move {
            let result = scan_runtime
                .spawn_blocking(move || -> Result<Vec<ModEntry>> {
                    xxmi::refresh_state(&mut startup_scan_state, None)?;
                    Ok(startup_scan_state.mods)
                })
                .await;
            match result {
                Ok(Ok(mods)) => {
                    let _ = scan_tx.send(StartupScanEvent::Ready(mods));
                }
                Ok(Err(err)) => {
                    let _ = scan_tx.send(StartupScanEvent::Failed(format!(
                        "Initial refresh failed: {err:#}"
                    )));
                }
                Err(err) => {
                    let _ = scan_tx.send(StartupScanEvent::Failed(format!(
                        "Initial refresh join failed: {err}"
                    )));
                }
            }
        });
    }

    fn start_manual_path_scan(&mut self) {
        if self.startup_path_scan.is_some() {
            return;
        }
        let targets = Self::startup_path_scan_targets(&self.state, true);
        self.startup_path_scan = Self::build_startup_path_scan_state(&targets, true);
        self.dispatch_startup_path_scan(targets);
    }

    fn build_startup_path_scan_state(
        targets: &[StartupPathScanTarget],
        run_initial_mod_scan_after: bool,
    ) -> Option<StartupPathScanState> {
        if targets.is_empty() {
            return None;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let statuses = targets
            .iter()
            .map(|target| StartupPathScanStatus {
                kind: target.kind.clone(),
                label: target.label.clone(),
                candidates: target.initial_candidates.clone(),
                selected_candidate: None,
                choosing: false,
            })
            .collect();
        Some(StartupPathScanState {
            statuses,
            cancel,
            cancel_requested: false,
            stopped: false,
            finished: false,
            run_initial_mod_scan_after,
        })
    }

    fn dispatch_startup_path_scan(&self, targets: Vec<StartupPathScanTarget>) {
        let cancel = self
            .startup_path_scan
            .as_ref()
            .map(|scan| Arc::clone(&scan.cancel))
            .unwrap_or_else(|| Arc::new(AtomicBool::new(true)));
        spawn_startup_path_scan_worker(
            &self.runtime_services,
            targets,
            cancel,
            self.startup_path_scan_tx.clone(),
        );
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
        use sysinfo::{MemoryRefreshKind, RefreshKind, System};
        let sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        let total = sys.total_memory();
        (total > 0).then_some(total)
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
        self.texture_meta = HashMap::with_capacity(64);
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
            self.texture_ram_estimated_bytes =
                self.texture_ram_estimated_bytes.saturating_add(bytes);
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
            self.texture_ram_estimated_bytes =
                self.texture_ram_estimated_bytes.saturating_add(bytes);
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
            self.texture_ram_estimated_bytes =
                self.texture_ram_estimated_bytes.saturating_add(bytes);
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
            self.texture_ram_estimated_bytes =
                self.texture_ram_estimated_bytes.saturating_add(bytes);
        }
    }

    fn evict_textures_to_budget(&mut self, now: f64) {
        // Weighted eviction: Level 0 (inactive hi-res) -> Level 1 (off-screen thumbs) -> Level 2 (on-screen/rails)
        // Level 3 (Current Full View) is protected.
        for target_priority in 0..=2 {
            while self.texture_ram_estimated_bytes > self.texture_ram_budget_bytes {
                let victim = self
                    .texture_meta
                    .iter()
                    .filter(|(_, meta)| meta.priority == target_priority)
                    .min_by_key(|(_, meta)| meta.last_access_tick)
                    .map(|(key, _)| key.clone());

                if let Some((kind, key)) = victim {
                    self.remove_tracked_texture(kind, &key);
                    self.texture_evictions_window_count =
                        self.texture_evictions_window_count.saturating_add(1);
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
        // Dropping the queue means no worker result will ever clear these, and a
        // card whose id is still listed here refuses to re-request its thumbnail.
        self.pending_image_loads.clear();
        self.inflight_full_image_requests.clear();
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
                    self.mod_cover_textures
                        .retain(|k, _| !k.starts_with(&prefix));

                    self.mod_full_textures.remove(mod_id);
                    self.mod_full_textures
                        .retain(|k, _| k != mod_id && !k.starts_with(&prefix));

                    self.pending_mod_image_requests.remove(mod_id);
                    self.pending_mod_image_requests
                        .retain(|k| !k.starts_with(&prefix));

                    self.pending_image_loads.remove(mod_id);
                    self.pending_image_loads.retain(|k| !k.starts_with(&prefix));

                    self.inflight_full_image_requests.remove(mod_id);
                    self.inflight_full_image_requests
                        .retain(|k| !k.starts_with(&prefix));

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

    fn get_browse_thumb_texture(
        &mut self,
        key: &str,
        priority: u8,
    ) -> Option<&egui::TextureHandle> {
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
            self.push_log(self.text().log_warn(&detail));
        }
    }

    fn log_error(&mut self, detail: impl Into<String>) {
        let detail = sanitize_log_subject(&detail.into());
        if !detail.is_empty() {
            self.push_log(self.text().log_error(&detail));
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
        self.push_toast_with_action(message, is_error, None);
    }

    fn push_toast_with_action(
        &mut self,
        message: String,
        is_error: bool,
        action: Option<ToastAction>,
    ) {
        let entry = ToastEntry {
            message,
            is_error,
            created_at: 0.0,
            action,
        };
        self.toasts.insert(0, entry);
        if self.toasts.len() > TOAST_LIMIT {
            self.toasts.truncate(TOAST_LIMIT);
        }
    }

    pub fn auto_detect_game_paths(state: &mut AppState) -> bool {
        let xxmi_config = load_xxmi_config();
        let xxmi_launcher_candidates = xxmi_config
            .as_ref()
            .map(|(config_path, _)| xxmi_launcher_exe_candidates(config_path))
            .unwrap_or_default();

        let mut changed = false;
        let has_enabled_xxmi_games = state
            .games
            .iter()
            .any(|game| game.enabled && game.is_xxmi());
        let global_modded_needs = match state.static_prefs.modded_launcher_path_override.as_ref() {
            Some(path) => !path.is_file(),
            None => true,
        };
        if has_enabled_xxmi_games && global_modded_needs {
            let registry_candidates = registry_modded_exe_candidates();
            let shortcut_candidates = shortcut_modded_exe_candidates();
            let fallback_candidates = default_modded_exe_candidates("");
            if let Some(path) = pick_most_recent_existing(&xxmi_launcher_candidates)
                .or_else(|| pick_most_recent_existing(&registry_candidates))
                .or_else(|| pick_most_recent_existing(&shortcut_candidates))
                .or_else(|| pick_most_recent_existing(&fallback_candidates))
            {
                state.static_prefs.modded_launcher_path_override = Some(path);
                changed = true;
            }
        }

        let use_default_mods_path = state.static_prefs.use_default_mods_path;
        for game in &mut state.games {
            changed |= Self::auto_detect_single_game_paths(
                game,
                xxmi_config.as_ref(),
                &xxmi_launcher_candidates,
                use_default_mods_path,
            );

            if !state.auto_game_enable_done {
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

        if !state.auto_game_enable_done {
            state.auto_game_enable_done = true;
            changed = true;
        }
        changed
    }

    fn startup_path_scan_targets(
        state: &AppState,
        include_all: bool,
    ) -> Vec<StartupPathScanTarget> {
        let xxmi_existing = state
            .static_prefs
            .modded_launcher_path_override
            .as_ref()
            .filter(|path| path.is_file())
            .cloned()
            .into_iter()
            .collect::<Vec<_>>();
        let has_xxmi_game_targets = state.games.iter().any(|game| game.is_xxmi());
        let has_enabled_xxmi_games = state
            .games
            .iter()
            .any(|game| game.enabled && game.is_xxmi());
        let has_missing_xxmi = has_enabled_xxmi_games
            && state
                .static_prefs
                .modded_launcher_path_override
                .as_ref()
                .is_none_or(|path| !path.is_file());

        let mut has_missing_game = false;
        let mut game_targets = Vec::new();
        for game in &state.games {
            let game_existing = game
                .vanilla_exe_path_override
                .as_ref()
                .filter(|path| path.is_file())
                .cloned()
                .into_iter()
                .collect::<Vec<_>>();
            let game_missing = game
                .vanilla_exe_path_override
                .as_ref()
                .is_none_or(|path| !path.is_file());
            has_missing_game |= game_missing;
            let file_names = vanilla_exe_file_names(&game.definition.id)
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !file_names.is_empty() {
                game_targets.push(StartupPathScanTarget {
                    kind: StartupPathTargetKind::Game(game.definition.id.clone()),
                    label: game.definition.name.clone(),
                    file_names,
                    initial_candidates: game_existing,
                });
            }
        }

        if !include_all && !has_missing_xxmi && !has_missing_game {
            return Vec::new();
        }

        let mut targets = Vec::new();
        if has_xxmi_game_targets {
            targets.push(StartupPathScanTarget {
                kind: StartupPathTargetKind::Xxmi,
                label: "XXMI Launcher".to_string(),
                file_names: xxmi_launcher_file_names()
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect(),
                initial_candidates: xxmi_existing,
            });
        }
        targets.extend(game_targets);

        targets
    }

    fn auto_detect_enabled_game_paths(&mut self, game_id: &str) -> bool {
        let xxmi_config = load_xxmi_config();
        let xxmi_launcher_candidates = xxmi_config
            .as_ref()
            .map(|(config_path, _)| xxmi_launcher_exe_candidates(config_path))
            .unwrap_or_default();

        let mut changed = false;
        let has_enabled_xxmi_games = self
            .state
            .games
            .iter()
            .any(|game| game.enabled && game.is_xxmi());
        let global_modded_needs = match self
            .state
            .static_prefs
            .modded_launcher_path_override
            .as_ref()
        {
            Some(path) => !path.is_file(),
            None => true,
        };
        if has_enabled_xxmi_games && global_modded_needs {
            let registry_candidates = registry_modded_exe_candidates();
            let shortcut_candidates = shortcut_modded_exe_candidates();
            let fallback_candidates = default_modded_exe_candidates("");
            if let Some(path) = pick_most_recent_existing(&xxmi_launcher_candidates)
                .or_else(|| pick_most_recent_existing(&registry_candidates))
                .or_else(|| pick_most_recent_existing(&shortcut_candidates))
                .or_else(|| pick_most_recent_existing(&fallback_candidates))
            {
                self.state.static_prefs.modded_launcher_path_override = Some(path);
                changed = true;
            }
        }

        if let Some(game) = self
            .state
            .games
            .iter_mut()
            .find(|game| game.definition.id == game_id)
        {
            changed |= Self::auto_detect_single_game_paths(
                game,
                xxmi_config.as_ref(),
                &xxmi_launcher_candidates,
                self.state.static_prefs.use_default_mods_path,
            );
        }

        if changed {
            self.save_state();
        }
        changed
    }

    fn auto_detect_single_game_paths(
        game: &mut GameInstall,
        xxmi_config: Option<&(PathBuf, serde_json::Value)>,
        xxmi_launcher_candidates: &[PathBuf],
        use_default_mods_path: bool,
    ) -> bool {
        let mut changed = false;

        if game.is_unreal_engine() {
            let mods_needs = game
                .mods_path_override
                .as_ref()
                .is_none_or(|path| !path.is_dir());
            if mods_needs {
                if let Some(path) = game
                    .vanilla_exe_path_override
                    .as_ref()
                    .and_then(|path| {
                        default_unreal_pak_mods_path_from_exe(&game.definition.id, path)
                    })
                    .filter(|path| path.is_dir())
                {
                    game.mods_path_override = Some(path);
                    changed = true;
                }
            }
        } else if !use_default_mods_path {
            let mods_needs = match game.mods_path_override.as_ref() {
                Some(path) => !path.is_dir(),
                None => true,
            };
            if mods_needs {
                if let Some(path) =
                    default_mods_path(&game.definition.xxmi_code).filter(|path| path.is_dir())
                {
                    game.mods_path_override = Some(path);
                    changed = true;
                }
            }
        }

        let modded_needs = game.is_xxmi()
            && match game.modded_exe_path_override.as_ref() {
                Some(path) => !path.is_file(),
                None => true,
            };
        if modded_needs {
            let registry_candidates = registry_modded_exe_candidates();
            let shortcut_candidates = shortcut_modded_exe_candidates();
            let fallback_candidates = default_modded_exe_candidates(&game.definition.id);
            if let Some(path) = pick_most_recent_existing(xxmi_launcher_candidates)
                .or_else(|| pick_most_recent_existing(&registry_candidates))
                .or_else(|| pick_most_recent_existing(&shortcut_candidates))
                .or_else(|| pick_most_recent_existing(&fallback_candidates))
            {
                game.modded_exe_path_override = Some(path);
                changed = true;
            }
        }

        let vanilla_needs = match game.vanilla_exe_path_override.as_ref() {
            Some(path) => !path.is_file(),
            None => true,
        };
        if vanilla_needs {
            let candidates_from_config = if game.is_xxmi() {
                xxmi_config
                    .map(|(_, config)| xxmi_game_exe_candidates(config, &game.definition.xxmi_code))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let fallback_candidates = default_vanilla_exe_candidates(&game.definition.id);
            let registry_candidates = registry_vanilla_exe_candidates(&game.definition.id);
            let path = pick_most_recent_existing(&candidates_from_config)
                .or_else(|| pick_most_recent_existing(&registry_candidates))
                .or_else(|| pick_most_recent_existing(&fallback_candidates));
            if let Some(path) = path {
                game.vanilla_exe_path_override = Some(path);
                if game.is_unreal_engine() && game.mods_path_override.is_none() {
                    game.mods_path_override =
                        game.vanilla_exe_path_override.as_ref().and_then(|path| {
                            default_unreal_pak_mods_path_from_exe(&game.definition.id, path)
                        });
                }
                changed = true;
            }
        }

        changed
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

    fn launch_selected_game(&mut self, ctx: &egui::Context, modded: bool) {
        let Some(game) = self.selected_game().cloned() else {
            self.report_error_message(
                self.text().game_not_selected(),
                Some(self.text().launch_failed()),
            );
            return;
        };
        if !Self::game_install_is_configured(&game) {
            self.report_error_message(
                self.text().game_not_installed(),
                Some(self.text().launch_failed()),
            );
            return;
        }
        let launch_modded = modded && game.is_xxmi();
        let Some(path) = (if launch_modded {
            self.state
                .static_prefs
                .modded_launcher_path_override
                .clone()
                .or_else(|| game.modded_exe_path())
        } else {
            game.vanilla_exe_path()
        }) else {
            let text = self.text();
            let label = if launch_modded {
                text.play_modded()
            } else {
                text.play_vanilla()
            };
            self.report_error_message(
                text.launch_path_not_set_for_game(label, &game.definition.name),
                Some(text.launch_path_not_set()),
            );
            return;
        };
        if !path.is_file() {
            self.report_error_message(
                self.text().game_not_installed(),
                Some(self.text().launch_failed()),
            );
            return;
        }
        let result = if launch_modded {
            xxmi::launch_xxmi_launcher(&path, &game.definition.xxmi_code)
        } else if game.is_unreal_engine() {
            unrealengine::launch_game(&game)
        } else {
            xxmi::launch_vanilla_executable(&path)
        };
        match result {
            Ok(()) => {
                let text = self.text();
                let message = if game.is_xxmi() {
                    let label = if launch_modded {
                        text.modded()
                    } else {
                        text.vanilla()
                    };
                    text.launched_game_mode(&game.definition.name, label)
                } else {
                    text.launched_game(&game.definition.name)
                };
                self.set_message_ok(message);
                Self::apply_launch_behavior(ctx, self.state.static_prefs.launch_behavior);
            }
            Err(err) => self.report_error(err, Some(self.text().launch_failed())),
        }
    }

    fn apply_launch_behavior(ctx: &egui::Context, behavior: LaunchBehavior) {
        match behavior {
            LaunchBehavior::DoNothing => {}
            LaunchBehavior::Minimize => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            LaunchBehavior::Exit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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

    fn selected_game_readiness(&self) -> Option<GameReadiness> {
        self.selected_game()
            .map(|game| self.game_readiness_for(game))
    }

    fn game_readiness(&self, game_id: &str) -> Option<GameReadiness> {
        self.state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .map(|game| self.game_readiness_for(game))
    }

    fn selected_game_can_install_mods(&self) -> bool {
        self.selected_game_readiness()
            .is_some_and(|readiness| readiness.can_install_mods)
    }

    fn selected_game_can_download_mods(&self) -> bool {
        self.selected_game_readiness()
            .is_some_and(|readiness| readiness.can_download_mods)
    }

    fn selected_game_mod_setup_issue(&self) -> Option<GameSetupIssue> {
        self.selected_game_readiness()
            .and_then(|readiness| readiness.primary_issue)
    }

    fn selected_game_can_launch_modded(&self) -> bool {
        self.selected_game_readiness()
            .is_some_and(|readiness| readiness.can_launch_modded)
    }

    fn selected_game_can_launch_vanilla(&self) -> bool {
        self.selected_game_readiness()
            .is_some_and(|readiness| readiness.can_launch_vanilla)
    }

    fn game_can_install_mods(&self, game_id: &str) -> bool {
        self.game_readiness(game_id)
            .is_some_and(|readiness| readiness.can_install_mods)
    }

    fn game_can_download_mods(&self, game_id: &str) -> bool {
        self.game_readiness(game_id)
            .is_some_and(|readiness| readiness.can_download_mods)
    }

    fn game_mod_setup_issue(&self, game_id: &str) -> Option<GameSetupIssue> {
        self.game_readiness(game_id)
            .and_then(|readiness| readiness.primary_issue)
    }

    fn selected_game_mod_setup_message(&self) -> String {
        self.selected_game_mod_setup_issue()
            .map(|issue| self.game_setup_issue_message(issue))
            .unwrap_or_else(|| self.text().game_not_installed().to_string())
    }

    fn game_mod_setup_message(&self, game_id: &str) -> String {
        self.game_mod_setup_issue(game_id)
            .map(|issue| self.game_setup_issue_message(issue))
            .unwrap_or_else(|| self.text().game_not_installed().to_string())
    }

    fn game_setup_issue_message(&self, issue: GameSetupIssue) -> String {
        let text = self.text();
        match issue {
            GameSetupIssue::MissingGamePath => text.game_not_installed().to_string(),
            GameSetupIssue::MissingModFolder => text.games_path_not_found().to_string(),
            GameSetupIssue::NoGameDirAccess => text.protected_path_title().to_string(),
            GameSetupIssue::MissingXxmiLauncher => text.install_xxmi_description().to_string(),
            GameSetupIssue::MissingNteBypasser => text.nte_bypasser_missing_description().to_string(),
            GameSetupIssue::MissingUnrealRequirement => text.install_unavailable().to_string(),
        }
    }

    fn game_readiness_for(&self, game: &GameInstall) -> GameReadiness {
        const PATH_STATUS_TTL: Duration = Duration::from_secs(1);
        let game_present = game.enabled
            && game
                .vanilla_exe_path_override
                .as_ref()
                .is_some_and(|path| self.cached_path_is_file(path, PATH_STATUS_TTL));
        let mods_path = game.mods_path(self.state.static_prefs.use_default_mods_path);
        let mod_root_ready = game_present && mods_path.is_some();
        let mods_dir_writable = mod_root_ready
            && mods_path
                .as_deref()
                .is_some_and(|path| self.cached_path_allows_creation(path, PATH_STATUS_TTL));
        let mut mod_loader_ready = false;
        let mut primary_issue = None;

        if !game_present {
            primary_issue = Some(GameSetupIssue::MissingGamePath);
        } else if !mod_root_ready {
            primary_issue = Some(GameSetupIssue::MissingModFolder);
        } else if !mods_dir_writable {
            // Windows blocks unelevated writes into the game directory (e.g. Program Files
            // installs).  This outranks loader issues: installing a loader would fail too.
            primary_issue = Some(GameSetupIssue::NoGameDirAccess);
        }

        if game_present && mod_root_ready {
            match game.definition.backend {
                GameBackend::Xxmi => {
                    let launcher_exists = self
                        .state
                        .static_prefs
                        .modded_launcher_path_override
                        .as_deref()
                        .or_else(|| game.modded_exe_path_override.as_deref())
                        .is_some_and(|path| self.cached_path_is_file(path, PATH_STATUS_TTL));
                    mod_loader_ready = launcher_exists;
                    if !launcher_exists && primary_issue.is_none() {
                        primary_issue = Some(GameSetupIssue::MissingXxmiLauncher);
                    }
                }
                GameBackend::UnrealEngine => {
                    mod_loader_ready = match game.definition.id.as_str() {
                        "nte" => game
                            .vanilla_exe_path_override
                            .as_ref()
                            .map(|exe| default_unreal_bypasser_paths_from_exe(&game.definition.id, exe))
                            .is_some_and(|paths| {
                                !paths.is_empty()
                                    && paths
                                        .iter()
                                        .any(|path| self.cached_path_is_file(path, PATH_STATUS_TTL))
                            }),
                        _ => false,
                    };
                    if !mod_loader_ready && primary_issue.is_none() {
                        primary_issue = match game.definition.id.as_str() {
                            "nte" => Some(GameSetupIssue::MissingNteBypasser),
                            _ => Some(GameSetupIssue::MissingUnrealRequirement),
                        };
                    }
                }
            }
        }

        let can_install_mods = game_present && mod_root_ready && mods_dir_writable && mod_loader_ready;
        GameReadiness {
            game_present,
            can_launch_vanilla: game_present,
            can_launch_modded: game.is_xxmi() && game_present && mod_loader_ready,
            can_open_mods_folder: game_present && mod_root_ready,
            can_install_mods,
            can_download_mods: can_install_mods,
            primary_issue,
        }
    }

    fn cached_path_is_file(&self, path: &Path, ttl: Duration) -> bool {
        let now = Instant::now();
        if let Ok(mut cache) = self.path_file_status_cache.lock() {
            if let Some((exists, checked_at)) = cache.get(path) {
                if now.duration_since(*checked_at) < ttl {
                    return *exists;
                }
            }
            if cache.len() >= 512 {
                cache.retain(|_, (_, checked_at)| now.duration_since(*checked_at) < ttl);
                if cache.len() >= 512 {
                    cache.clear();
                }
            }
            let exists = path.is_file();
            cache.insert(path.to_path_buf(), (exists, now));
            return exists;
        }
        path.is_file()
    }

    fn cached_path_allows_creation(&self, path: &Path, ttl: Duration) -> bool {
        let now = Instant::now();
        if let Ok(mut cache) = self.path_write_status_cache.lock() {
            if let Some((writable, checked_at)) = cache.get(path) {
                if now.duration_since(*checked_at) < ttl {
                    return *writable;
                }
            }
            if cache.len() >= 512 {
                cache.retain(|_, (_, checked_at)| now.duration_since(*checked_at) < ttl);
                if cache.len() >= 512 {
                    cache.clear();
                }
            }
            let writable = path_allows_dir_creation(path);
            cache.insert(path.to_path_buf(), (writable, now));
            return writable;
        }
        path_allows_dir_creation(path)
    }

    /// Drop cached write-probe results so the next readiness pass re-checks ACLs, e.g. right
    /// after a grant-access elevation finishes.
    fn invalidate_path_write_status_cache(&self) {
        if let Ok(mut cache) = self.path_write_status_cache.lock() {
            cache.clear();
        }
    }

    #[cfg(test)]
    fn compute_game_readiness(
        game: &GameInstall,
        use_default_mods_path: bool,
        global_modded_launcher: Option<&Path>,
        mods_dir_writable: bool,
    ) -> GameReadiness {
        let game_present = Self::game_install_is_configured(game);
        let mods_path = game.mods_path(use_default_mods_path);
        let mod_root_ready = game_present && mods_path.is_some();
        let mods_dir_writable = mod_root_ready && mods_dir_writable;
        let mut mod_loader_ready = false;
        let mut primary_issue = None;

        if !game_present {
            primary_issue = Some(GameSetupIssue::MissingGamePath);
        } else if !mod_root_ready {
            primary_issue = Some(GameSetupIssue::MissingModFolder);
        } else if !mods_dir_writable {
            primary_issue = Some(GameSetupIssue::NoGameDirAccess);
        }

        if game_present && mod_root_ready {
            match game.definition.backend {
                GameBackend::Xxmi => {
                    let launcher_exists = global_modded_launcher
                        .or_else(|| game.modded_exe_path_override.as_deref())
                        .is_some_and(|path| path.is_file());
                    mod_loader_ready = launcher_exists;
                    if !launcher_exists && primary_issue.is_none() {
                        primary_issue = Some(GameSetupIssue::MissingXxmiLauncher);
                    }
                }
                GameBackend::UnrealEngine => {
                    mod_loader_ready = Self::unreal_game_mod_loader_ready(game);
                    if !mod_loader_ready && primary_issue.is_none() {
                        primary_issue = match game.definition.id.as_str() {
                            "nte" => Some(GameSetupIssue::MissingNteBypasser),
                            _ => Some(GameSetupIssue::MissingUnrealRequirement),
                        };
                    }
                }
            }
        }

        let can_install_mods =
            game_present && mod_root_ready && mods_dir_writable && mod_loader_ready;
        GameReadiness {
            game_present,
            can_launch_vanilla: game_present,
            can_launch_modded: game.is_xxmi() && game_present && mod_loader_ready,
            can_open_mods_folder: game_present && mod_root_ready,
            can_install_mods,
            can_download_mods: can_install_mods,
            primary_issue,
        }
    }

    #[cfg(test)]
    fn unreal_game_mod_loader_ready(game: &GameInstall) -> bool {
        match game.definition.id.as_str() {
            "nte" => game
                .vanilla_exe_path_override
                .as_ref()
                .map(|exe| default_unreal_bypasser_paths_from_exe(&game.definition.id, exe))
                .is_some_and(|paths| !paths.is_empty() && paths.iter().any(|path| path.is_file())),
            _ => false,
        }
    }

    fn game_install_is_configured(game: &GameInstall) -> bool {
        game.enabled
            && game
                .vanilla_exe_path_override
                .as_ref()
                .is_some_and(|path| path.is_file())
    }

    fn library_cards_for_selected_game(&mut self) -> Arc<Vec<LibraryCardRow>> {
        let key = self.library_card_cache_key();
        if self.library_card_cache.key != Some(key) {
            let rows: Vec<LibraryCardRow> = self
                .mods_for_selected_game()
                .into_iter()
                .map(|mod_entry| {
                    (
                        mod_entry.id.clone(),
                        mod_entry.folder_name.clone(),
                        mod_entry.metadata.user.title.clone(),
                        mod_entry.metadata.user.cover_image.clone(),
                        mod_entry.root_path.clone(),
                        mod_entry.status.clone(),
                        mod_entry.updated_at,
                        mod_entry.unsafe_content,
                        mod_entry.update_state,
                        mod_entry
                            .source
                            .as_ref()
                            .and_then(|source| source.gamebanana.as_ref())
                            .map(|link| link.mod_id > 0 || !link.url.trim().is_empty())
                            .unwrap_or(false),
                        Self::has_modified_update_available(mod_entry),
                        mod_has_local_changes_for_update_check(mod_entry),
                        Self::ignored_update_kind(mod_entry),
                        self.effective_mod_category_id(mod_entry),
                        self.mod_category_label(mod_entry),
                        mod_entry.content_size_bytes,
                        // Precomputed date sort key mirroring the global `sort_date` closure
                        // (max of created_at / content_mtime / updated_at) so the uncategorized
                        // pile's independent Date sort matches the mods list exactly.
                        mod_entry
                            .created_at
                            .timestamp()
                            .max(
                                mod_entry
                                    .content_mtime
                                    .map(|ts| ts.timestamp())
                                    .unwrap_or(i64::MIN),
                            )
                            .max(mod_entry.updated_at.timestamp()),
                    )
                })
                .collect();
            self.library_card_cache = LibraryCardCache {
                key: Some(key),
                rows: Arc::new(rows),
            };
        }
        Arc::clone(&self.library_card_cache.rows)
    }

    fn library_card_cache_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();

        self.selected_game.hash(&mut hasher);
        self.mods_search_query.hash(&mut hasher);
        self.show_enabled_mods.hash(&mut hasher);
        self.show_unlinked_mods.hash(&mut hasher);
        self.show_up_to_date_mods.hash(&mut hasher);
        self.show_update_available_mods.hash(&mut hasher);
        self.show_check_skipped_mods.hash(&mut hasher);
        self.show_missing_source_mods.hash(&mut hasher);
        self.show_modified_locally_mods.hash(&mut hasher);
        self.show_ignoring_update_mods.hash(&mut hasher);

        let prefs = &self.state.static_prefs;
        prefs.hide_disabled.hash(&mut hasher);
        prefs.hide_archived.hash(&mut hasher);
        prefs.library_sort_status_first.hash(&mut hasher);
        prefs.library_sort_category_first.hash(&mut hasher);
        prefs.library_uncategorized_first.hash(&mut hasher);
        prefs.library_status_group_show_category.hash(&mut hasher);
        prefs.library_category_group_show_status.hash(&mut hasher);
        Self::hash_discriminant_for_library_cache(&mut hasher, &prefs.unsafe_content_mode);
        Self::hash_discriminant_for_library_cache(&mut hasher, &prefs.library_sort);
        Self::hash_discriminant_for_library_cache(&mut hasher, &prefs.library_group_mode);
        Self::hash_discriminant_for_library_cache(
            &mut hasher,
            &prefs.library_category_display_mode,
        );
        Self::hash_discriminant_for_library_cache(&mut hasher, &prefs.modified_update_behavior);
        Self::hash_discriminant_for_library_cache(&mut hasher, &prefs.language);

        let selected_game_id = self.selected_game().map(|game| {
            game.definition.id.hash(&mut hasher);
            game.definition.name.hash(&mut hasher);
            game.enabled.hash(&mut hasher);
            game.definition.id.as_str()
        });

        if let Some(game_id) = selected_game_id {
            let mut category_count = 0usize;
            for category in self
                .state
                .categories
                .iter()
                .filter(|category| category.game_id == game_id)
            {
                category_count += 1;
                category.id.hash(&mut hasher);
                category.game_id.hash(&mut hasher);
                category.name.hash(&mut hasher);
                category.order.hash(&mut hasher);
            }
            category_count.hash(&mut hasher);

            let mut mod_count = 0usize;
            for mod_entry in self
                .state
                .mods
                .iter()
                .filter(|mod_entry| mod_entry.game_id == game_id)
            {
                mod_count += 1;
                Self::hash_mod_for_library_cache(&mut hasher, mod_entry);
            }
            mod_count.hash(&mut hasher);
        }

        hasher.finish()
    }

    fn hash_discriminant_for_library_cache<T>(hasher: &mut DefaultHasher, value: &T) {
        std::mem::discriminant(value).hash(hasher);
    }

    fn hash_datetime_for_library_cache(hasher: &mut DefaultHasher, value: &DateTime<Utc>) {
        value.timestamp().hash(hasher);
        value.timestamp_subsec_nanos().hash(hasher);
    }

    fn hash_optional_datetime_for_library_cache(
        hasher: &mut DefaultHasher,
        value: Option<&DateTime<Utc>>,
    ) {
        value.is_some().hash(hasher);
        if let Some(value) = value {
            Self::hash_datetime_for_library_cache(hasher, value);
        }
    }

    fn hash_mod_for_library_cache(hasher: &mut DefaultHasher, mod_entry: &ModEntry) {
        mod_entry.id.hash(hasher);
        mod_entry.game_id.hash(hasher);
        mod_entry.folder_name.hash(hasher);
        mod_entry.root_path.hash(hasher);
        Self::hash_discriminant_for_library_cache(hasher, &mod_entry.status);
        mod_entry.metadata.user.title.hash(hasher);
        mod_entry.metadata.user.cover_image.hash(hasher);
        mod_entry.metadata.user.category.hash(hasher);
        mod_entry.metadata.user.category_id.hash(hasher);
        Self::hash_datetime_for_library_cache(hasher, &mod_entry.created_at);
        Self::hash_datetime_for_library_cache(hasher, &mod_entry.updated_at);
        Self::hash_optional_datetime_for_library_cache(hasher, mod_entry.content_mtime.as_ref());
        mod_entry.ini_hash.hash(hasher);
        mod_entry.content_size_bytes.hash(hasher);
        mod_entry.unsafe_content.hash(hasher);
        Self::hash_discriminant_for_library_cache(hasher, &mod_entry.update_state);
        mod_has_local_changes_for_update_check(mod_entry).hash(hasher);
        if let Some(ignored_kind) = Self::ignored_update_kind(mod_entry) {
            true.hash(hasher);
            Self::hash_discriminant_for_library_cache(hasher, &ignored_kind);
        } else {
            false.hash(hasher);
        }
        Self::hash_mod_source_for_library_cache(hasher, mod_entry.source.as_ref());
    }

    fn hash_mod_source_for_library_cache(
        hasher: &mut DefaultHasher,
        source: Option<&ModSourceData>,
    ) {
        let Some(source) = source else {
            false.hash(hasher);
            return;
        };
        true.hash(hasher);

        if let Some(link) = source.gamebanana.as_ref() {
            true.hash(hasher);
            link.mod_id.hash(hasher);
            link.url.hash(hasher);
        } else {
            false.hash(hasher);
        }

        if let Some(snapshot) = source.snapshot.as_ref() {
            true.hash(hasher);
            snapshot.title.hash(hasher);
            snapshot.authors.hash(hasher);
            snapshot.version.hash(hasher);
            snapshot.publish_ts.hash(hasher);
            snapshot.update_ts.hash(hasher);
            snapshot.description.hash(hasher);
            snapshot.preview_urls.hash(hasher);
            snapshot.is_private.hash(hasher);
            snapshot.is_deleted.hash(hasher);
            snapshot.is_trashed.hash(hasher);
            snapshot.is_withheld.hash(hasher);
            snapshot.unsafe_content.hash(hasher);
            snapshot.files.len().hash(hasher);
            for file in &snapshot.files {
                file.file_id.hash(hasher);
                file.file_name.hash(hasher);
                file.file_size.hash(hasher);
                file.date_added.hash(hasher);
                file.download_count.hash(hasher);
                file.description.hash(hasher);
                file.download_url.hash(hasher);
                file.archived.hash(hasher);
            }
        } else {
            false.hash(hasher);
        }

        source
            .raw_profile_json
            .as_ref()
            .map(|raw| (raw.len(), xxh3_64(raw.as_bytes())))
            .hash(hasher);
        Self::hash_optional_datetime_for_library_cache(
            hasher,
            source.update_check_retry_after.as_ref(),
        );
        source.file_set.selected_file_ids.hash(hasher);
        source.file_set.selected_file_names.hash(hasher);
        source.file_set.selected_candidate_labels.hash(hasher);
        source.file_set.selected_files_meta.len().hash(hasher);
        for file in &source.file_set.selected_files_meta {
            file.file_id.hash(hasher);
            file.file_name.hash(hasher);
            file.date_added.hash(hasher);
            file.version.hash(hasher);
            file.archived.hash(hasher);
        }
        if let Some(signature) = source.ignored_update_signature.as_ref() {
            true.hash(hasher);
            signature.profile_update_ts.hash(hasher);
            signature.prearmed_next_update.hash(hasher);
            signature.files.len().hash(hasher);
            for file in &signature.files {
                file.file_id.hash(hasher);
                file.file_name.hash(hasher);
                file.date_added.hash(hasher);
                file.version.hash(hasher);
                file.archived.hash(hasher);
            }
        } else {
            false.hash(hasher);
        }
        source.ignore_update_always.hash(hasher);
        Self::hash_optional_datetime_for_library_cache(
            hasher,
            source.baseline_content_mtime.as_ref(),
        );
        source.baseline_ini_hash.hash(hasher);
        source.accepted_local_changes.is_some().hash(hasher);
        if let Some(accepted) = source.accepted_local_changes.as_ref() {
            Self::hash_optional_datetime_for_library_cache(hasher, accepted.content_mtime.as_ref());
            accepted.ini_hash.hash(hasher);
        }
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
                if self.state.static_prefs.hide_disabled && item.status == ModStatus::Disabled {
                    return false;
                }
                if self.state.static_prefs.hide_archived && item.status == ModStatus::Archived {
                    return false;
                }
                if !self.show_unlinked_mods && item.update_state == ModUpdateState::Unlinked {
                    return false;
                }
                if !self.show_up_to_date_mods && item.update_state == ModUpdateState::UpToDate {
                    return false;
                }
                if !self.show_ignoring_update_mods
                    && matches!(
                        item.update_state,
                        ModUpdateState::IgnoringUpdateOnce | ModUpdateState::IgnoringUpdateAlways
                    )
                {
                    return false;
                }
                let has_modified_update_available = Self::has_modified_update_available(item);
                if has_modified_update_available
                    && !self.show_update_available_mods
                    && !self.show_modified_locally_mods
                {
                    return false;
                }
                if !self.show_update_available_mods
                    && item.update_state == ModUpdateState::UpdateAvailable
                {
                    return false;
                }
                if !self.show_check_skipped_mods
                    && item.update_state == ModUpdateState::CheckSkipped
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
                    && !has_modified_update_available
                {
                    return false;
                }
                if matches!(
                    self.state.static_prefs.unsafe_content_mode,
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
            let status_cmp = if self.state.static_prefs.library_sort_status_first
                && matches!(
                    self.state.static_prefs.effective_library_group_mode(),
                    LibraryGroupMode::Category | LibraryGroupMode::None
                ) {
                a.status.cmp(&b.status)
            } else {
                std::cmp::Ordering::Equal
            };
            let category_cmp = if self.state.static_prefs.library_sort_category_first
                && !matches!(
                    self.state.static_prefs.effective_library_group_mode(),
                    LibraryGroupMode::Category
                ) {
                category_order(a).cmp(&category_order(b)).then_with(|| {
                    let left = a.metadata.user.category.trim().to_ascii_lowercase();
                    let right = b.metadata.user.category.trim().to_ascii_lowercase();
                    left.cmp(&right)
                })
            } else {
                std::cmp::Ordering::Equal
            };
            let sort_cmp = match self.state.static_prefs.library_sort {
                LibrarySort::NameAsc => name_cmp,
                LibrarySort::NameDesc => name_cmp.reverse(),
                LibrarySort::DateDesc => sort_date(b).cmp(&sort_date(a)).then_with(|| name_cmp),
                LibrarySort::DateAsc => sort_date(a).cmp(&sort_date(b)).then_with(|| name_cmp),
                LibrarySort::SizeAsc => a
                    .content_size_bytes
                    .cmp(&b.content_size_bytes)
                    .then_with(|| name_cmp),
                LibrarySort::SizeDesc => b
                    .content_size_bytes
                    .cmp(&a.content_size_bytes)
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

    fn toggle_whats_new_window(&mut self) {
        self.state.show_whats_new = !self.state.show_whats_new;
        if self.state.show_whats_new {
            self.whats_new_window_nonce = self.whats_new_window_nonce.wrapping_add(1);
            self.whats_new_force_default_pos = true;
        }
    }

    fn open_feedback_survey_window(&mut self) {
        if feedback_survey().is_none() {
            self.set_message_ok(self.text().no_feedback_survey_configured());
            return;
        }
        self.state.show_feedback_survey = true;
        self.feedback_survey_window_nonce = self.feedback_survey_window_nonce.wrapping_add(1);
        self.feedback_survey_force_default_pos = true;
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab: move to the next / previous primary tab,
    /// wrapping at both ends. Tab order is `ViewMode::TAB_ORDER`; add new
    /// views there and this picks them up automatically.
    fn cycle_primary_view(&mut self, forward: bool) {
        let order = ViewMode::TAB_ORDER;
        let Some(idx) = order.iter().position(|&v| v == self.current_view) else {
            return;
        };
        let next = if forward {
            (idx + 1) % order.len()
        } else {
            (idx + order.len() - 1) % order.len()
        };
        if order[next] == self.current_view {
            return;
        }
        self.current_view = order[next];
        self.clear_mod_detail_rename();
    }

    fn leave_category_folder_view(&mut self) -> bool {
        if self.selected_category_folder_id.is_none() {
            return false;
        }
        self.selected_category_folder_id = None;
        self.selected_mods.clear();
        true
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
        self.mod_detail_rename_focus_target_id = Some(mod_id.clone());
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

    fn current_view_detail_can_translate(&self) -> bool {
        match self.current_view {
            ViewMode::Browse => {
                self.browse_detail_open
                    && self
                        .browse_state
                        .selected_mod_id
                        .is_some_and(|mod_id| self.browse_state.details.contains_key(&mod_id))
            }
            ViewMode::Library => {
                self.mod_detail_open
                    && self.selected_mod().is_some_and(|mod_entry| {
                        mod_entry
                            .source
                            .as_ref()
                            .and_then(|source| source.gamebanana.as_ref())
                            .is_some()
                            || !self.unlinked_texts_to_translate(&mod_entry.id).is_empty()
                    })
            }
        }
    }

    fn toggle_visible_detail_translation(&mut self) {
        match self.current_view {
            ViewMode::Browse => {
                let Some(mod_id) = self.browse_state.selected_mod_id else {
                    return;
                };
                if self.browse_detail_open && self.browse_state.details.contains_key(&mod_id) {
                    self.toggle_browse_translation(mod_id);
                }
            }
            ViewMode::Library => {
                let mod_id = self.selected_mod().map(|mod_entry| mod_entry.id.clone());
                if self.mod_detail_open {
                    if let Some(mod_id) = mod_id {
                        self.toggle_my_mods_translation(mod_id);
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    fn poll_windows_ctrl_v_edge(&mut self, ctx: &egui::Context) -> bool {
        let ctrl_down = unsafe { GetAsyncKeyState(i32::from(VK_CONTROL.0)) } < 0;
        let v_down = unsafe { GetAsyncKeyState(i32::from(VK_V.0)) } < 0;
        let down = ctrl_down && v_down;
        let window_focused =
            ctx.input(|input| input.focused && input.viewport().focused.unwrap_or(input.focused));
        if !window_focused {
            self.clipboard_image_paste_held = down;
            return false;
        }
        let pressed = down && !self.clipboard_image_paste_held;
        self.clipboard_image_paste_held = down;
        pressed
    }

    #[cfg(not(windows))]
    fn poll_windows_ctrl_v_edge(&mut self, _ctx: &egui::Context) -> bool {
        false
    }

    // Fallback for the fullview overlay's copy shortcut, mirroring
    // poll_windows_ctrl_v_edge: egui's synthesized Copy event does not arrive
    // reliably on Windows, so poll the real key state instead.
    #[cfg(windows)]
    fn poll_windows_ctrl_c_edge(&mut self, ctx: &egui::Context) -> bool {
        let ctrl_down = unsafe { GetAsyncKeyState(i32::from(VK_CONTROL.0)) } < 0;
        let c_down = unsafe { GetAsyncKeyState(i32::from(VK_C.0)) } < 0;
        let down = ctrl_down && c_down;
        let window_focused =
            ctx.input(|input| input.focused && input.viewport().focused.unwrap_or(input.focused));
        if !window_focused {
            self.overlay_copy_ctrl_c_held = down;
            return false;
        }
        let pressed = down && !self.overlay_copy_ctrl_c_held;
        self.overlay_copy_ctrl_c_held = down;
        pressed
    }

    #[cfg(not(windows))]
    fn poll_windows_ctrl_c_edge(&mut self, _ctx: &egui::Context) -> bool {
        false
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl = egui::Modifiers::CTRL;
        let ctrl_shift = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let text_input_active = self.shortcuts_blocked_by_text_input(ctx);

        // Ctrl+Tab cycles forward through the primary tabs, Ctrl+Shift+Tab
        // cycles backward (same convention as browser tab switching).
        if !text_input_active {
            let tab_cycle = ctx.input(|input| {
                if input.modifiers.ctrl && !input.modifiers.alt && input.key_pressed(egui::Key::Tab) {
                    Some(!input.modifiers.shift)
                } else {
                    None
                }
            });
            if let Some(forward) = tab_cycle {
                self.cycle_primary_view(forward);
            }
        }
        // Ctrl+P — the settings/preferences shortcut. Deliberately NOT a bare
        // function key like F10: while the XXMI foreground grant is active, any bare key
        // Hestia handles also bleeds to the game (F10 = XXMI reload), so settings lives on
        // a Ctrl combo that no XXMI mod binds.
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::P))
        }) {
            self.settings_open = !self.settings_open;
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::L))
        }) {
            self.toggle_log_window();
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::T))
        }) {
            self.toggle_tools_window();
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::J))
        }) {
            self.toggle_tasks_window();
        }
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::F))
        }) {
            self.focus_active_search(ctx);
        }
        if self.current_view == ViewMode::Browse
            && ctx.input_mut(|input| {
                input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::D))
            })
        {
            self.browse_state.toggle_character_picker_requested = true;
        }
        let app_window_focused =
            ctx.input(|input| input.focused && input.viewport().focused.unwrap_or(input.focused));
        if app_window_focused
            && !text_input_active
            && self.current_view_detail_can_translate()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F7))
        {
            self.toggle_visible_detail_translation();
        }
        if app_window_focused
            && !text_input_active
            && self.settings_open
            && self.settings_tab == SettingsTab::Categories
            && ctx.input_mut(|input| {
                input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::N))
            })
        {
            if let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) {
                self.create_category_for_game(&game_id, CategoryRenameSurface::Settings);
            }
        }

        if self.current_view == ViewMode::Library {
            let alt = egui::Modifiers {
                alt: true,
                ..Default::default()
            };
            let folder_back_requested = !text_input_active
                && self.selected_category_folder_id.is_some()
                && (ctx.input_mut(|input| {
                    input.consume_shortcut(&egui::KeyboardShortcut::new(alt, egui::Key::ArrowLeft))
                        || input
                            .consume_shortcut(&egui::KeyboardShortcut::new(alt, egui::Key::ArrowUp))
                        || input.consume_key(egui::Modifiers::NONE, egui::Key::BrowserBack)
                }) || ctx
                    .input(|input| input.pointer.button_clicked(egui::PointerButton::Extra1)));
            if folder_back_requested {
                self.leave_category_folder_view();
            }
            if !text_input_active
                && self.delete_shortcut_has_mod_context()
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
            {
                self.delete_selected_context();
            }
            if ctx.input_mut(|input| {
                input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::E))
            }) {
                self.enable_or_restore_selected_context();
            }
            if ctx.input_mut(|input| {
                input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::D))
            }) {
                self.disable_selected_context();
            }
            if ctx.input_mut(|input| {
                input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::A))
            }) {
                self.archive_selected_context();
            }
            if ctx.input_mut(|input| {
                input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::R))
            }) {
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
            if !text_input_active
                && self.selected_unlinked_mod_context().is_some()
                && (ctx.input_mut(|input| {
                    input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::V))
                }) || self.poll_windows_ctrl_v_edge(ctx))
            {
                match self.enqueue_clipboard_image_to_selected_unlinked_mod() {
                    Ok(()) => self.set_message_ok(self.text().adding_clipboard_image()),
                    Err(err) => self.report_error(err, Some(self.text().could_not_paste_image())),
                }
            }
        }

        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::R))
        }) {
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
        self.pending_image_loads
            .retain(|key| key != &cover_key && !key.starts_with(&shot_prefix));
        self.inflight_full_image_requests
            .retain(|key| key != &cover_key && !key.starts_with(&shot_prefix));
        self.pending_mod_image_queue.retain(|req| {
            req.texture_key != cover_key && !req.texture_key.starts_with(&shot_prefix)
        });
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

        let mut source_keys = HashSet::with_capacity(32);
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
        // Same reason as in clear_dynamic_textures: requests that never reach a
        // worker produce no result, so these have to be released by hand or the
        // cards they belong to stay blank for the rest of the session.
        self.pending_image_loads.clear();
        self.inflight_full_image_requests.clear();

        // self.selected_mod_id = mod_id;
        self.selected_mod_id = mod_id.clone();
        self.clear_mod_detail_rename();
        self.my_mod_overlay_images.clear();
        self.browse_state.screenshot_overlay = None;
        if let Some(id) = mod_id {
            self.mod_detail_open = true;
            self.mod_detail_focus_requested = true;
            self.maybe_translate_my_mod_details(&id);

            // Optimization: Pre-fetch full cover image for the selected mod to avoid redundant decoding later.
            // Extract necessary data before any mutable borrows of `self`
            let (mod_entry_id_clone, source_path_clone) = {
                if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == id) {
                    let (_, source_path, _) = Self::current_card_thumb_meta(mod_entry);
                    (Some(id.clone()), source_path)
                } else {
                    (None, None)
                }
            };

            // Now perform mutable operations
            if let Some(mod_entry_id) = mod_entry_id_clone {
                if let Some(path) = source_path_clone {
                    self.queue_mod_image_full_load(mod_entry_id.clone(), path, 10);
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
        let snapshot = WindowStateSnapshot {
            pos,
            size,
            maximized,
        };
        if self.window_state_cache != Some(snapshot) {
            if !maximized {
                self.state.static_prefs.window_pos = pos;
                self.state.static_prefs.window_size = size;
            }
            self.state.static_prefs.window_maximized = maximized;
            if now - self.window_state_last_save >= 0.5 {
                self.save_state();
                self.window_state_last_save = now;
            }
            self.window_state_cache = Some(snapshot);
        }
        self.flush_floating_window_layouts(ctx, now);
    }

    /// Applies a change to the remembered floating-window geometry and arms the
    /// trailing debounce if anything actually changed. Call every frame a window is
    /// shown; identical geometry is a no-op so this never writes while idle.
    fn remember_floating_window_layout(
        &mut self,
        ctx: &egui::Context,
        apply: impl FnOnce(&mut FloatingWindowLayouts),
    ) {
        let layouts = &mut self.state.static_prefs.floating_windows;
        let before = layouts.clone();
        apply(layouts);
        if *layouts != before {
            let now = ctx.input(|input| input.time);
            self.floating_window_save_due = Some(now + FLOATING_WINDOW_SAVE_DEBOUNCE_SECS);
        }
    }

    /// Writes pending floating-window geometry once the user has stopped dragging
    /// for `FLOATING_WINDOW_SAVE_DEBOUNCE_SECS`; keeps a repaint scheduled so the
    /// write happens even if the app goes idle right after the drag.
    fn flush_floating_window_layouts(&mut self, ctx: &egui::Context, now: f64) {
        let Some(due) = self.floating_window_save_due else {
            return;
        };
        if now >= due {
            self.floating_window_save_due = None;
            self.save_state();
        } else {
            ctx.request_repaint_after(Duration::from_secs_f64(due - now));
        }
    }

    fn refresh(&mut self) {
        self.mark_usage_counters_dirty();
        let old_ts: HashMap<String, DateTime<Utc>> = self
            .state
            .mods
            .iter()
            .map(|m| (m.id.clone(), m.updated_at))
            .collect();

        let game_id = self.selected_game().map(|g| g.definition.id.clone());
        match xxmi::refresh_state(&mut self.state, game_id.as_deref()) {
            Ok(()) => {
                self.restore_imported_mod_categories(game_id.as_deref());
                // Settings baseline/reroute must cover this synchronous path too — the
                // first scan after an upgrade often runs here, not in the async worker.
                let scan_game_ids: Vec<String> = match game_id.as_deref() {
                    Some(id) => vec![id.to_string()],
                    None => self
                        .state
                        .games
                        .iter()
                        .filter(|game| game.is_xxmi())
                        .map(|game| game.definition.id.clone())
                        .collect(),
                };
                for scan_game_id in scan_game_ids {
                    self.run_xxmi_persist_scan_pass(&scan_game_id);
                }
                self.invalidate_stale_mod_textures(&old_ts);
                self.backfill_missing_mod_images(game_id.as_deref());
                self.sync_tools_for_selected_game();
                self.save_state();
                self.sync_selection_after_refresh();
                self.queue_update_check_for_linked_mods(game_id.as_deref());
                self.request_automatic_app_update_check(0.0);
            }
            Err(err) => self.report_error(err, Some(self.text().could_not_refresh_mods())),
        }
    }

    fn enqueue_mod_image_sync(&mut self, mod_id: &str) {
        let job_data = self
            .state
            .mods
            .iter()
            .find(|m| m.id == mod_id)
            .and_then(|m| {
                m.source.as_ref().and_then(|s| {
                    s.snapshot
                        .as_ref()
                        .map(|snap| (m.root_path.clone(), snap.clone()))
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

    fn is_missing_expected_source_images(
        mod_entry: &ModEntry,
        snapshot: &GameBananaSnapshot,
    ) -> bool {
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
        if self.startup_scan_loading || self.refresh_inflight {
            return;
        }
        self.mark_usage_counters_dirty();
        self.clear_translation_caches();
        let Some(game_id) = self.selected_game().map(|game| game.definition.id.clone()) else {
            return;
        };
        let before = self.capture_reload_snapshots(Some(&game_id));

        // Do this before the background library scan so newly added tool executables appear
        // as soon as the UI repaints after the reload click.
        self.sync_tools_for_selected_game();
        self.save_state();
        self.pending_reload_summary = Some((game_id.clone(), before));
        self.queue_profile_recovery_for_game(&game_id);
        self.queue_game_refresh(game_id);
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
        items.sort_by(|a, b| {
            a.folder_name
                .to_lowercase()
                .cmp(&b.folder_name.to_lowercase())
        });
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
        added_items.sort_by(|a, b| {
            a.folder_name
                .to_lowercase()
                .cmp(&b.folder_name.to_lowercase())
        });
        for item in added_items {
            added += 1;
            detail_lines.push(format!("added {}", item.folder_name));
        }

        let mut removed_items: Vec<_> = before
            .iter()
            .filter(|item| !after_map.contains_key(&item.id))
            .collect();
        removed_items.sort_by(|a, b| {
            a.folder_name
                .to_lowercase()
                .cmp(&b.folder_name.to_lowercase())
        });
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
        changed_items.sort_by(|a, b| {
            a.folder_name
                .to_lowercase()
                .cmp(&b.folder_name.to_lowercase())
        });
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
        let text = self.text();
        if summary.added == 0 && summary.removed == 0 && summary.changed == 0 {
            text.mods_scanned_no_changes(summary.total_mods)
        } else {
            let mut parts = vec![text.mods_scanned(summary.total_mods)];
            if summary.added > 0 {
                parts.push(text.reload_added(summary.added));
            }
            if summary.removed > 0 {
                parts.push(text.reload_removed(summary.removed));
            }
            if summary.changed > 0 {
                parts.push(text.reload_changed(summary.changed));
            }
            parts.join(", ")
        }
    }

    fn reload_summary_toast_text(&self, summary: &ReloadSummary) -> String {
        let text = self.text();
        if summary.added == 0 && summary.removed == 0 && summary.changed == 0 {
            text.reloaded_no_changes(summary.total_mods)
        } else {
            let mut parts = vec![text.reloaded(summary.total_mods)];
            if summary.added > 0 {
                parts.push(text.reload_added(summary.added));
            }
            if summary.removed > 0 {
                parts.push(text.reload_removed(summary.removed));
            }
            if summary.changed > 0 {
                parts.push(text.reload_changed(summary.changed));
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
        let previous_game_id = self.selected_game().map(|game| game.definition.id.clone());
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
                .and_then(|game| game.mods_path(self.state.static_prefs.use_default_mods_path));
            let _ = persistence::cleanup_orphan_tmp_files(
                selected_mods_root.as_deref(),
                &HashSet::new(),
            );
            self.queue_game_refresh(game_id.clone());
            self.queue_update_check_for_linked_mods(Some(&game_id));
        }
    }

    fn move_game_order_to_slot(&mut self, game_id: &str, slot_index: usize) -> bool {
        let Some(from_index) = self
            .state
            .games
            .iter()
            .position(|game| game.definition.id == game_id)
        else {
            return false;
        };
        let selected_game_id = self.selected_game().map(|game| game.definition.id.clone());
        let slot_index = slot_index.min(self.state.games.len());
        let adjusted_index = if slot_index > from_index {
            slot_index.saturating_sub(1)
        } else {
            slot_index
        };
        if from_index == adjusted_index {
            return false;
        }
        let game = self.state.games.remove(from_index);
        self.state.games.insert(adjusted_index, game);
        if let Some(selected_game_id) = selected_game_id {
            if let Some(index) = self
                .state
                .games
                .iter()
                .position(|game| game.definition.id == selected_game_id)
            {
                self.selected_game = index;
            }
        }
        true
    }

    fn next_background_job_id(&mut self) -> u64 {
        let id = self.install_next_job_id;
        self.install_next_job_id = self.install_next_job_id.wrapping_add(1);
        id
    }

    fn check_pending_worker_events(&mut self) -> bool {
        // Check if any worker channels have pending events
        // This uses try_recv's non-blocking peek behavior
        let has_events = !self.icon_result_rx.is_empty()
            || !self.mod_image_result_rx.is_empty()
            || !self.manual_image_event_rx.is_empty()
            || !self.overlay_copy_event_rx.is_empty()
            || !self.gif_preview_event_rx.is_empty()
            || !self.gif_animation_event_rx.is_empty()
            || !self.cover_result_rx.is_empty()
            || !self.browse_event_rx.is_empty()
            || !self.browse_image_result_rx.is_empty()
            || !self.browse_download_event_rx.is_empty()
            || !self.app_update_event_rx.is_empty()
            || !self.proxy_apply_rx.is_empty()
            || !self.feedback_survey_submit_rx.is_empty()
            || !self.update_check_rx.is_empty()
            || !self.startup_path_scan_rx.is_empty()
            || !self.startup_scan_rx.is_empty()
            || !self.translation_event_rx.is_empty()
            || !self.install_event_rx.is_empty()
            || !self.refresh_result_rx.is_empty()
            || !self.xxmi_reload_event_rx.is_empty()
            || !self.grant_access_event_rx.is_empty()
            || !self.hotkey_customization_rx.is_empty()
            || !self.profile_event_rx.is_empty();

        self.pending_events.has_worker_events = has_events;
        has_events
    }

    fn check_pending_process_work(&mut self) -> bool {
        // Check if any processing queues have work
        let has_work = !self.pending_mod_image_queue.is_empty()
            || !self.pending_texture_uploads.is_empty()
            || (self.current_view == ViewMode::Browse
                && self.has_enabled_games()
                && self.browse_state.cards.is_empty()
                && !self.browse_state.loading_page
                && self.browse_state.page_error.is_none())
            || self.pending_browse_open_mod_id.is_some()
            || !self.browse_image_queue.is_empty()
            || !self.browse_download_queue.is_empty()
            || self.app_update_download_inflight.is_some()
            || !self.install_queue.is_empty();

        self.pending_events.has_process_work = has_work;
        has_work
    }
}

/// The floating windows whose geometry is remembered; see `FloatingWindowLayouts`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FloatingWindow {
    Log,
    Tasks,
    Tools,
    Settings,
    LibraryDetail,
    BrowseDetail,
}

impl FloatingWindow {
    fn id_salt(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Tasks => "tasks",
            Self::Tools => "tools",
            Self::Settings => "settings",
            Self::LibraryDetail => "library_detail",
            Self::BrowseDetail => "browse_detail",
        }
    }
}

impl HestiaApp {
    /// Puts a floating window back at its default size and position and forgets
    /// the saved layout. Works by bumping the window's id nonce so egui starts a
    /// fresh area (the same trick the corner-anchored windows use on every open);
    /// the recorder then stores the default rect again on the next frame.
    fn reset_floating_window_layout(&mut self, ctx: &egui::Context, window: FloatingWindow) {
        let layouts = &mut self.state.static_prefs.floating_windows;
        match window {
            FloatingWindow::Log => {
                layouts.log_size = None;
                self.log_window_nonce = self.log_window_nonce.wrapping_add(1);
                self.log_force_default_pos = true;
            }
            FloatingWindow::Tasks => {
                layouts.tasks_size = None;
                self.tasks_window_nonce = self.tasks_window_nonce.wrapping_add(1);
                self.tasks_force_default_pos = true;
            }
            FloatingWindow::Tools => {
                layouts.tools_size = None;
                self.tools_window_nonce = self.tools_window_nonce.wrapping_add(1);
                self.tools_force_default_pos = true;
            }
            FloatingWindow::Settings => {
                layouts.settings = None;
                self.settings_window_nonce = self.settings_window_nonce.wrapping_add(1);
            }
            FloatingWindow::LibraryDetail => {
                layouts.library_detail = None;
                self.mod_detail_window_nonce = self.mod_detail_window_nonce.wrapping_add(1);
            }
            FloatingWindow::BrowseDetail => {
                layouts.browse_detail = None;
                self.browse_detail_window_nonce = self.browse_detail_window_nonce.wrapping_add(1);
            }
        }
        let now = ctx.input(|input| input.time);
        self.floating_window_save_due = Some(now + FLOATING_WINDOW_SAVE_DEBOUNCE_SECS);
        ctx.request_repaint();
    }

    /// Title-bar layout controls shared by the resizable floating windows: a
    /// "reset size and position" icon just left of the close button, visible only
    /// while the window differs from `default_rect`, plus a right-click menu on the
    /// title bar offering the same action. The icon is drawn in a sublayer of the
    /// window so it stacks with it and never floats over other windows.
    /// `inner_margin` is the window frame's inner margin (the title bar inherits it).
    /// Returns true when a reset was requested; the caller applies it after its own
    /// post-show bookkeeping so `force_default_pos` flags aren't cleared again.
    fn floating_window_layout_controls<R>(
        &self,
        ctx: &egui::Context,
        window: FloatingWindow,
        response: &Option<egui::InnerResponse<Option<R>>>,
        default_rect: egui::Rect,
        inner_margin: f32,
    ) -> bool {
        let Some(inner) = response else {
            return false;
        };
        let window_rect = inner.response.rect;
        let window_layer = inner.response.layer_id;
        let text = self.text();
        let style = ctx.style_of(ctx.theme());
        let stroke_width = style.visuals.window_stroke.width;
        // The title bar's row height: egui allocates its buttons at the heading row
        // height and our 14pt titles fit inside that.
        let row_height =
            ctx.fonts_mut(|fonts| fonts.row_height(&egui::TextStyle::Heading.resolve(&style)));
        let title_rect = egui::Rect::from_min_size(
            window_rect.min,
            egui::vec2(
                window_rect.width(),
                stroke_width + 2.0 * inner_margin + row_height,
            ),
        );
        let title_center_y = window_rect.min.y + stroke_width + inner_margin + row_height / 2.0;
        // egui allocates the close button a heading-row-high square and paints the
        // "X" at `icon_width` in its middle; sit one icon plus a small gap to its left,
        // with a hit box just a touch larger than the glyph so it can't steal the
        // close button's edge.
        let icon_width = style.spacing.icon_width;
        let close_button_center_x =
            window_rect.max.x - stroke_width - inner_margin - row_height / 2.0;
        let button_size = egui::Vec2::splat(icon_width + 4.0);
        let button_rect = egui::Rect::from_center_size(
            egui::pos2(close_button_center_x - icon_width - 6.0, title_center_y),
            button_size,
        );

        let overlay_id = egui::Id::new(("floating_window_layout_controls", window.id_salt()));
        let overlay_layer = egui::LayerId::new(egui::Order::Foreground, overlay_id);
        ctx.set_sublayer(window_layer, overlay_layer);

        let at_default = shown_floating_window_rect(response).is_none_or(|shown| {
            const TOLERANCE: f32 = 1.5;
            (shown.min - default_rect.min).abs().max_elem() <= TOLERANCE
                && (shown.size() - default_rect.size()).abs().max_elem() <= TOLERANCE
        });

        let mut reset_requested = false;
        if !at_default {
            egui::Area::new(overlay_id)
                .order(egui::Order::Foreground)
                .fixed_pos(button_rect.min)
                .interactable(true)
                .show(ctx, |ui| {
                    let (rect, button) = ui.allocate_exact_size(button_size, Sense::click());
                    let visuals = ui.style().interact(&button);
                    // Same nominal size and hover growth as egui's close "X" next door
                    // (icon_width, expanded by the interact visuals when hovered).
                    let icon_size = icon_width + 2.0 * visuals.expansion;
                    paint_lucide_icon_centered(
                        ui.painter(),
                        rect.center(),
                        Icon::PictureInPicture2,
                        icon_size,
                        visuals.fg_stroke.color,
                    );
                    let button = button
                        .on_hover_text(text.reset_window_layout())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if button.clicked() {
                        reset_requested = true;
                    }
                });
        }

        // Right-click on the title bar (including the control above) opens a small
        // menu with the same action, so there is a way in even while the window is
        // collapsed or already at its default layout.
        let title_bar_right_clicked = ctx.input(|input| {
            input.pointer.secondary_clicked()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| title_rect.contains(pos))
        }) && ctx
            .input(|input| input.pointer.interact_pos())
            .and_then(|pos| ctx.layer_id_at(pos))
            .is_some_and(|layer| layer == window_layer || layer == overlay_layer);
        let popup_id = overlay_id.with("context_menu");
        egui::Popup::new(
            popup_id,
            ctx.clone(),
            egui::PopupAnchor::PointerFixed,
            window_layer,
        )
        .kind(egui::PopupKind::Menu)
        .layout(egui::Layout::top_down_justified(egui::Align::Min))
        .gap(0.0)
        .frame(egui::Frame::menu(&style).inner_margin(egui::Margin::same(8)))
        .open_memory(title_bar_right_clicked.then_some(egui::SetOpenCommand::Bool(true)))
        .show(|ui| {
            let radius = egui::CornerRadius::same(3);
            ui.style_mut().visuals.widgets.inactive.corner_radius = radius;
            ui.style_mut().visuals.widgets.hovered.corner_radius = radius;
            ui.style_mut().visuals.widgets.active.corner_radius = radius;
            if ui
                .add(
                    egui::Button::new(icon_text_sized(
                        Icon::PictureInPicture2,
                        text.reset_window_layout(),
                        13.0,
                        13.0,
                    ))
                    .corner_radius(radius),
                )
                .clicked()
            {
                reset_requested = true;
                ui.close();
            }
        });

        reset_requested
    }
}

const FLOATING_WINDOW_SAVE_DEBOUNCE_SECS: f64 = 0.5;

/// Paints a lucide glyph with its ink box centered on `center`. Painting text with
/// `Align2::CENTER_CENTER` centers the font row, and icon fonts don't sit centered
/// in their row, so the glyph would drift a pixel or two against egui's
/// geometrically painted close "X" that this is meant to line up with.
fn paint_lucide_icon_centered(
    painter: &egui::Painter,
    center: egui::Pos2,
    icon: Icon,
    size: f32,
    color: Color32,
) {
    let font_id = egui::FontId::new(size, FontFamily::Name(LUCIDE_FAMILY.into()));
    let galley = painter.layout_no_wrap(icon_char(icon).to_string(), font_id, color);
    let ink_center = galley
        .rows
        .first()
        .and_then(|row| {
            let glyph = row.glyphs.first()?;
            let min = row.pos + glyph.pos.to_vec2() + glyph.uv_rect.offset;
            Some(egui::Rect::from_min_size(min, glyph.uv_rect.size).center())
        })
        .unwrap_or_else(|| galley.rect.center());
    painter.galley(center - ink_center.to_vec2(), galley, color);
}

/// Outer size of a floating window after `show()`, rounded to whole points so the
/// frame-stroke half-pixel doesn't drift across save/restore cycles. `None` when the
/// window is collapsed (its rect is just the title bar then) or wasn't drawn at all.
fn shown_floating_window_rect<R>(
    response: &Option<egui::InnerResponse<Option<R>>>,
) -> Option<egui::Rect> {
    let inner = response.as_ref()?;
    inner.inner.as_ref()?;
    let rect = inner.response.rect;
    let finite = rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite();
    (finite && rect.width() > 0.0 && rect.height() > 0.0).then(|| {
        egui::Rect::from_min_size(rect.min.round(), rect.size().round())
    })
}

/// Converts a shown window rect into the pane-relative form we persist.
fn floating_window_rect_relative_to(
    rect: egui::Rect,
    inset_rect: egui::Rect,
) -> FloatingWindowRect {
    let offset = rect.min - inset_rect.min;
    FloatingWindowRect {
        offset: [offset.x, offset.y],
        size: [rect.width(), rect.height()],
    }
}

/// A remembered size that is safe to feed back into `default_size`: finite and
/// positive, capped to the pane it has to fit in. Falls back to `default` otherwise.
fn restore_floating_window_size(
    saved: Option<[f32; 2]>,
    default: egui::Vec2,
    inset_rect: egui::Rect,
) -> egui::Vec2 {
    let size = saved
        .map(|[w, h]| egui::vec2(w, h))
        .filter(|size| size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0)
        .unwrap_or(default);
    size.min(inset_rect.size()).max(egui::Vec2::ZERO)
}

/// Resolves a remembered pane-relative rect against the current pane: the size is
/// capped to the pane and the top-left is pulled back inside it, so a layout saved
/// on a bigger window (or another monitor) never restores half off-pane. Falls back
/// to `default_pos`/`default_size` when nothing usable is saved.
fn restore_floating_window_rect(
    saved: Option<FloatingWindowRect>,
    default_pos: egui::Pos2,
    default_size: egui::Vec2,
    inset_rect: egui::Rect,
) -> (egui::Pos2, egui::Vec2) {
    let size = restore_floating_window_size(saved.map(|rect| rect.size), default_size, inset_rect);
    let pos = saved
        .map(|rect| inset_rect.min + egui::vec2(rect.offset[0], rect.offset[1]))
        .filter(|pos| pos.x.is_finite() && pos.y.is_finite())
        .unwrap_or(default_pos);
    let max_pos = (inset_rect.max - size).max(inset_rect.min);
    let pos = egui::pos2(
        pos.x.clamp(inset_rect.min.x, max_pos.x),
        pos.y.clamp(inset_rect.min.y, max_pos.y),
    );
    (pos, size)
}

#[cfg(test)]
mod floating_window_layout_tests {
    use super::*;

    fn pane() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(800.0, 600.0))
    }

    #[test]
    fn size_falls_back_to_default_when_unsaved_or_invalid() {
        let default = egui::vec2(460.0, 420.0);
        assert_eq!(restore_floating_window_size(None, default, pane()), default);
        assert_eq!(
            restore_floating_window_size(Some([f32::NAN, 10.0]), default, pane()),
            default
        );
        assert_eq!(
            restore_floating_window_size(Some([0.0, 10.0]), default, pane()),
            default
        );
    }

    #[test]
    fn size_is_capped_to_the_pane() {
        let size = restore_floating_window_size(
            Some([2000.0, 300.0]),
            egui::vec2(460.0, 420.0),
            pane(),
        );
        assert_eq!(size, egui::vec2(800.0, 300.0));
    }

    #[test]
    fn rect_restores_pane_relative_and_round_trips() {
        let pane = pane();
        let shown = egui::Rect::from_min_size(egui::pos2(140.0, 90.0), egui::vec2(420.0, 560.0));
        let saved = floating_window_rect_relative_to(shown, pane);
        assert_eq!(saved.offset, [40.0, 40.0]);
        assert_eq!(saved.size, [420.0, 560.0]);

        // Same offset, different pane origin: the window follows the pane.
        let moved_pane = pane.translate(egui::vec2(-50.0, 200.0));
        let (pos, size) = restore_floating_window_rect(
            Some(saved),
            moved_pane.min,
            egui::vec2(1.0, 1.0),
            moved_pane,
        );
        assert_eq!(pos, moved_pane.min + egui::vec2(40.0, 40.0));
        assert_eq!(size, egui::vec2(420.0, 560.0));
    }

    #[test]
    fn rect_is_pulled_back_inside_a_smaller_pane() {
        let pane = pane();
        let saved = FloatingWindowRect {
            offset: [700.0, 500.0],
            size: [400.0, 900.0],
        };
        let (pos, size) =
            restore_floating_window_rect(Some(saved), pane.min, egui::vec2(1.0, 1.0), pane);
        // Height capped to the pane, then position clamped so the rect fits.
        assert_eq!(size, egui::vec2(400.0, 600.0));
        assert_eq!(pos, egui::pos2(pane.max.x - 400.0, pane.min.y));
    }

    #[test]
    fn rect_falls_back_to_defaults_when_unsaved() {
        let pane = pane();
        let default_pos = pane.min + egui::vec2(0.0, 32.0);
        let (pos, size) =
            restore_floating_window_rect(None, default_pos, egui::vec2(420.0, 560.0), pane);
        assert_eq!(pos, default_pos);
        assert_eq!(size, egui::vec2(420.0, 560.0));
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;

    fn game_install(id: &str, backend: GameBackend, exe: PathBuf) -> GameInstall {
        GameInstall {
            definition: crate::model::GameDefinition {
                id: id.to_string(),
                name: id.to_string(),
                backend,
                xxmi_code: if backend == GameBackend::Xxmi {
                    "TESTMI".to_string()
                } else {
                    String::new()
                },
            },
            mods_path_override: None,
            modded_exe_path_override: None,
            vanilla_exe_path_override: Some(exe),
            apply_mod_changes_in_game: true,
            enabled: true,
        }
    }

    #[test]
    fn xxmi_game_present_without_launcher_blocks_mod_actions() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("Game.exe");
        let mods = temp.path().join("Mods");
        std::fs::write(&exe, []).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        let mut game = game_install("xxmi-test", GameBackend::Xxmi, exe);
        game.mods_path_override = Some(mods);

        let readiness = HestiaApp::compute_game_readiness(&game, false, None, true);

        assert!(readiness.game_present);
        assert!(readiness.can_launch_vanilla);
        assert!(!readiness.can_launch_modded);
        assert!(!readiness.can_install_mods);
        assert!(!readiness.can_download_mods);
        assert_eq!(
            readiness.primary_issue,
            Some(GameSetupIssue::MissingXxmiLauncher)
        );
    }

    #[test]
    fn xxmi_game_with_launcher_allows_mod_actions() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("Game.exe");
        let launcher = temp.path().join("XXMI Launcher.exe");
        let mods = temp.path().join("Mods");
        std::fs::write(&exe, []).unwrap();
        std::fs::write(&launcher, []).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        let mut game = game_install("xxmi-test", GameBackend::Xxmi, exe);
        game.mods_path_override = Some(mods);

        let readiness = HestiaApp::compute_game_readiness(&game, false, Some(&launcher), true);

        assert!(readiness.game_present);
        assert!(readiness.can_launch_vanilla);
        assert!(readiness.can_launch_modded);
        assert!(readiness.can_install_mods);
        assert!(readiness.can_download_mods);
        assert_eq!(readiness.primary_issue, None);
    }

    #[test]
    fn nte_without_bypasser_blocks_mod_actions() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp
            .path()
            .join("Neverness To Everness")
            .join("Client")
            .join("WindowsNoEditor")
            .join("HT")
            .join("Binaries")
            .join("Win64")
            .join("HTGame.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, []).unwrap();
        let game = game_install("nte", GameBackend::UnrealEngine, exe);

        let readiness = HestiaApp::compute_game_readiness(&game, false, None, true);

        assert!(readiness.game_present);
        assert!(readiness.can_launch_vanilla);
        assert!(!readiness.can_install_mods);
        assert!(!readiness.can_download_mods);
        assert_eq!(
            readiness.primary_issue,
            Some(GameSetupIssue::MissingNteBypasser)
        );
    }

    #[test]
    fn unwritable_game_dir_blocks_mod_actions_and_outranks_loader_issues() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp
            .path()
            .join("Neverness To Everness")
            .join("Client")
            .join("WindowsNoEditor")
            .join("HT")
            .join("Binaries")
            .join("Win64")
            .join("HTGame.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, []).unwrap();
        let game = game_install("nte", GameBackend::UnrealEngine, exe);

        // No bypasser installed either; the access issue must still win.
        let readiness = HestiaApp::compute_game_readiness(&game, false, None, false);

        assert!(readiness.game_present);
        assert!(readiness.can_launch_vanilla);
        assert!(readiness.can_open_mods_folder);
        assert!(!readiness.can_install_mods);
        assert!(!readiness.can_download_mods);
        assert_eq!(
            readiness.primary_issue,
            Some(GameSetupIssue::NoGameDirAccess)
        );
    }

    #[test]
    fn nte_with_bypasser_allows_mod_actions() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp
            .path()
            .join("Neverness To Everness")
            .join("Client")
            .join("WindowsNoEditor")
            .join("HT")
            .join("Binaries")
            .join("Win64")
            .join("HTGame.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, []).unwrap();
        std::fs::write(exe.parent().unwrap().join("UniversalSigBypasser.asi"), []).unwrap();
        let game = game_install("nte", GameBackend::UnrealEngine, exe);

        let readiness = HestiaApp::compute_game_readiness(&game, false, None, true);

        assert!(readiness.game_present);
        assert!(readiness.can_launch_vanilla);
        assert!(readiness.can_install_mods);
        assert!(readiness.can_download_mods);
        assert_eq!(readiness.primary_issue, None);
    }
}
