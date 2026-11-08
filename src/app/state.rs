#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastAction {
    OpenTools,
}

#[derive(Clone)]
struct ToastEntry {
    message: String,
    is_error: bool,
    created_at: f64,
    action: Option<ToastAction>,
}

#[derive(Default)]
struct InstallBatchStats {
    installed: usize,
    failed: usize,
    unsupported: usize,
    skipped: usize,
}

#[derive(Clone)]
struct ReloadSnapshot {
    id: String,
    folder_name: String,
    root_path: PathBuf,
    status: ModStatus,
    updated_at: DateTime<Utc>,
}

struct ReloadSummary {
    total_mods: usize,
    added: usize,
    removed: usize,
    changed: usize,
    detail_lines: Vec<String>,
}

struct ToolLaunchOptionsPrompt {
    tool_id: String,
    launch_args: String,
}

enum XxmiReloadEvent {
    Finished { game_id: String, message: String },
    Failed { game_id: String, message: String },
}

struct LiveStateWatch {
    mod_id: String,
    game_id: String,
    use_default: bool,
    mods_path: Option<PathBuf>,
    importer_root: Option<PathBuf>,
    token: Option<(std::time::SystemTime, u64)>,
    next_poll_at: f64,
}

#[derive(Clone, PartialEq, Eq)]
struct D3dxReloadPromptToken {
    token: Option<(SystemTime, u64)>,
}

#[derive(Clone, PartialEq, Eq)]
struct D3dxReloadPromptFingerprint {
    error: Option<String>,
    fields: Vec<(String, String)>,
}

#[derive(Clone, PartialEq, Eq)]
struct D3dxReloadPromptCandidate {
    token: D3dxReloadPromptToken,
    fingerprint: D3dxReloadPromptFingerprint,
    confirmations: u8,
}

struct D3dxReloadConfigWatch {
    game_id: String,
    use_default: bool,
    mods_path: Option<PathBuf>,
    importer_root: Option<PathBuf>,
    token: Option<(SystemTime, u64)>,
    next_poll_at: f64,
    pending_unhealthy: Option<D3dxReloadPromptCandidate>,
    prompted_unhealthy: Option<D3dxReloadPromptCandidate>,
}

#[derive(Clone)]
struct D3dxReloadStatusCache {
    game_id: String,
    status: xxmi_persist::D3dxReloadConfigStatus,
}

struct GameProcessRunningCache {
    running: bool,
    next_check_at: f64,
}

struct ProfileProcessBlockCache {
    game_id: Option<String>,
    reason: Option<&'static str>,
    next_check_at: f64,
}

#[derive(Clone, Copy)]
struct ProfileStorageStatusSnapshot {
    loose_exists: bool,
    archive_exists: bool,
    archive_part_exists: bool,
    archive_size: Option<u64>,
}

struct ProfileStorageStatusCacheEntry {
    snapshot: ProfileStorageStatusSnapshot,
    checked_at: Instant,
}

enum HotkeyCustomizationRequest {
    LoadValues {
        game: GameInstall,
        use_default: bool,
        entry: ModEntry,
        mirrors: Vec<xxmi_persist::MirrorReadback>,
    },
    SetValue {
        game: GameInstall,
        use_default: bool,
        entry: ModEntry,
        ini_rel_path: String,
        var_name: String,
        value: String,
        key_spec: String,
        steps: Option<usize>,
        reload_after_write: bool,
    },
    Clear {
        game: GameInstall,
        use_default: bool,
        entry: ModEntry,
        live: bool,
        reload_after_clear: bool,
    },
    RunCommand {
        game: GameInstall,
        use_default: bool,
        mod_id: String,
        key_spec: String,
        label: String,
    },
}

enum HotkeyCustomizationEvent {
    ValuesLoaded {
        mod_id: String,
        ini_hash: Option<String>,
        values: HashMap<String, String>,
    },
    ValueFinished {
        game_id: String,
        folder_name: String,
        var_name: String,
        value: String,
        status: String,
    },
    ClearFinished {
        mod_id: String,
        game_id: String,
        folder_name: String,
        status: String,
        message: String,
    },
    CommandFinished {
        game_id: String,
        label: String,
        message: String,
    },
    Failed {
        mod_id: Option<String>,
        message: String,
    },
}

struct XxmiNamespaceCacheEntry {
    root_path: PathBuf,
    ini_hash: Option<String>,
    content_mtime: Option<DateTime<Utc>>,
    prefixes: Vec<String>,
}

enum InlineMarkdownEmbed {
    Image { texture_key: String },
    Youtube { url: String },
}

#[derive(Clone, Copy)]
enum IgnoredUpdateKind {
    Once,
    Always,
}

type LibraryCardRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    PathBuf,
    ModStatus,
    DateTime<Utc>,
    bool,
    ModUpdateState,
    bool,
    bool,
    bool,
    Option<IgnoredUpdateKind>,
    Option<String>,
    String,
);

struct LibraryCardCache {
    key: Option<u64>,
    rows: Arc<Vec<LibraryCardRow>>,
    evidence: HashMap<String, CachedUpdateEvidence>,
    evidence_key: Option<u64>,
}

#[derive(Clone)]
struct CachedUpdateEvidence {
    state: ModUpdateState,
    signature: Option<IgnoredUpdateSignature>,
    prearmed_signature: Option<IgnoredUpdateSignature>,
    newer_files: Vec<gamebanana::ModFile>,
    installed_files: Vec<TrackedFileMeta>,
    other_mods_files: Vec<u64>,
    profile: Option<Arc<gamebanana::ProfileResponse>>,
    profile_cached_at: Option<DateTime<Utc>>,
    thalia_pending: bool,
}

enum LogDisplayRow {
    Date { text: String, gap_before: bool },
    Entry { text: String },
}

#[derive(Default)]
struct LogDisplayCache {
    key: Option<(u64, bool)>,
    rows: Vec<LogDisplayRow>,
}

impl Default for LibraryCardCache {
    fn default() -> Self {
        Self {
            key: None,
            rows: Arc::new(Vec::new()),
            evidence: HashMap::new(),
            evidence_key: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Library,
    Browse,
}

impl ViewMode {
    /// Left-to-right order of the primary tabs, used by Ctrl+Tab /
    /// Ctrl+Shift+Tab cycling. Keep in sync with the tab bar layout.
    const TAB_ORDER: &'static [ViewMode] = &[ViewMode::Library, ViewMode::Browse];
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Categories,
    Advanced,
    Games,
    About,
}

impl SettingsTab {
    const TAB_ORDER: &'static [SettingsTab] = &[
        SettingsTab::General,
        SettingsTab::Categories,
        SettingsTab::Advanced,
        SettingsTab::Games,
        SettingsTab::About,
    ];

    fn scroll_index(self) -> usize {
        match self {
            SettingsTab::General => 0,
            SettingsTab::Categories => 1,
            SettingsTab::Advanced => 2,
            SettingsTab::Games => 3,
            SettingsTab::About => 4,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ModDetailTab {
    Overview,
    Source,
    Content,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CategoryRenameSurface {
    LibraryPopup,
    LibraryFolder,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppUpdateButtonState {
    Check,
    Checking,
    UpToDate,
    Failed,
    ManualRequired,
    UpdateAvailable,
}

pub struct HestiaApp {
    runtime_services: RuntimeServices,
    portable: PortablePaths,
    state: AppState,
    selected_game: usize,
    selected_mod_id: Option<String>,
    selected_mods: HashSet<String>,
    mods_search_query: String,
    mods_search_expanded: bool,
    mods_search_focus_pending: bool,
    show_enabled_mods: bool,
    show_unlinked_mods: bool,
    show_up_to_date_mods: bool,
    show_update_available_mods: bool,
    show_new_files_mods: bool,
    show_check_skipped_mods: bool,
    show_missing_source_mods: bool,
    show_modified_locally_mods: bool,
    show_ignoring_update_mods: bool,
    selected_category_folder_id: Option<String>,
    category_folder_forward_id: Option<String>,
    selected_library_folder_ids: HashSet<String>,
    library_folder_selection_anchor: Option<String>,
    library_folder_contents_scope: FolderContentsScope,
    library_selection_section: LibrarySelectionSection,
    library_visible_folder_ids: Vec<String>,
    library_visible_mod_ids: Vec<String>,
    library_folder_selection_context: Option<u64>,
    dragging_library_folder_ids: Vec<String>,
    folder_delete_menu_requested: bool,
    folder_batch_job: Option<FolderBatchJob>,
    folder_batch_report: Option<FolderBatchReport>,
    mod_image_sync_inflight: HashMap<u64, String>,
    local_mod_image_inflight: HashMap<String, usize>,
    library_scroll_to_category_id: Option<String>,
    library_card_cache: LibraryCardCache,
    dragging_mod_ids: Vec<String>,
    current_view: ViewMode,
    settings_open: bool,
    settings_window_nonce: u64,
    mod_detail_window_nonce: u64,
    browse_detail_window_nonce: u64,
    active_renderer_label: &'static str,
    active_renderer_device: Option<String>,
    active_renderer_gpu: Option<GpuChoice>,
    auto_renderer_label: &'static str,
    boot_renderer_pref: RendererPreference,
    renderer_boot_unconfirmed: bool,
    proxy_url_draft: String,
    proxy_url_validation_error: Option<String>,
    applied_custom_proxy: Option<CustomProxyConfig>,
    proxy_apply_inflight: bool,
    proxy_apply_locks_input: bool,
    proxy_apply_silent_success: bool,
    proxy_apply_tx: WorkerTx<ProxyApplyEvent>,
    proxy_apply_rx: WorkerRx<ProxyApplyEvent>,
    mod_detail_open: bool,
    browse_detail_open: bool,
    settings_tab: SettingsTab,
    settings_tab_scroll_offsets: [f32; SettingsTab::TAB_ORDER.len()],
    mod_detail_tab: ModDetailTab,
    last_titlebar_rect: Option<egui::Rect>,
    last_right_pane_rect: Option<egui::Rect>,
    mod_detail_focus_requested: bool,
    mod_detail_category_picker_pending: Option<String>,
    browse_detail_focus_requested: bool,
    mod_detail_editing: bool,
    selected_mod_detail_snapshot: Option<Arc<ModEntry>>,
    library_detail_content_cache: Option<LibraryDetailContentCache>,
    mod_detail_edit_target_id: Option<String>,
    mod_detail_rename_focus_target_id: Option<String>,
    mod_detail_edit_name: String,
    mod_keybinds_available_cache: HashMap<String, (Option<String>, bool)>,
    metadata_hotkeys_view: Option<(String, Vec<ModConfigIni>)>,
    mod_hotkey_values_cache: HashMap<String, (Option<String>, HashMap<String, String>)>,
    mod_hotkey_values_loading: HashSet<String>,
    live_state_watch: Option<LiveStateWatch>,
    game_overlay: GameOverlay,
    d3dx_reload_status_cache: Option<D3dxReloadStatusCache>,
    d3dx_reload_config_watch: Option<D3dxReloadConfigWatch>,
    game_process_running_cache: HashMap<String, GameProcessRunningCache>,
    profile_process_block_cache: Option<ProfileProcessBlockCache>,
    profile_storage_status_cache:
        HashMap<(String, ProfileId, PathBuf, PathBuf), ProfileStorageStatusCacheEntry>,
    hotkeys_write_block_cache: Option<(String, bool, f64)>,
    hotkey_customization_tx: WorkerTx<HotkeyCustomizationRequest>,
    hotkey_requests_inflight: VecDeque<(String, Option<u64>)>,
    hotkey_customization_rx: WorkerRx<HotkeyCustomizationEvent>,
    hotkey_clear_inflight: HashSet<String>,
    hotkey_clear_confirm_target_id: Option<String>,
    personal_note_edit_target_id: Option<String>,
    personal_note_edit_text: String,
    #[cfg(windows)]
    clipboard_image_paste_held: bool,
    overlay_copy_ctrl_c_held: bool,
    category_rename_target_id: Option<String>,
    category_rename_focus_target_id: Option<String>,
    category_rename_surface: Option<CategoryRenameSurface>,
    category_rename_name: String,
    dragging_category_id: Option<String>,
    dragging_category_target_index: Option<usize>,
    selected_category_ids: HashSet<String>,
    settings_dragging_category_ids: Vec<String>,
    settings_dragging_category_target_index: Option<usize>,
    toasts: Vec<ToastEntry>,
    pending_imports: VecDeque<PendingImport>,
    pending_conflicts: VecDeque<PendingConflict>,
    whats_new_window_nonce: u64,
    whats_new_force_default_pos: bool,
    feedback_survey_window_nonce: u64,
    feedback_survey_force_default_pos: bool,
    feedback_survey_answers: HashMap<String, u8>,
    feedback_survey_message: String,
    feedback_survey_privacy_expanded: bool,
    feedback_survey_submitting: bool,
    feedback_survey_cancellation: Option<Arc<AtomicBool>>,
    feedback_survey_active_request: Option<FeedbackSurveySubmitRequest>,
    pending_proxy_survey_resume: Option<FeedbackSurveySubmitRequest>,
    feedback_survey_submit_tx: WorkerTx<FeedbackSurveySubmitRequest>,
    feedback_survey_submit_rx: WorkerRx<FeedbackSurveySubmitEvent>,
    log_scroll_to_bottom: bool,
    log_window_nonce: u64,
    log_force_default_pos: bool,
    log_revision: u64,
    log_display_cache: LogDisplayCache,
    tools_window_nonce: u64,
    tools_force_default_pos: bool,
    new_tool_ids: HashSet<String>,
    tools_new_badges_shown: bool,
    tool_launch_options_prompt: Option<ToolLaunchOptionsPrompt>,
    dragging_window_tool_id: Option<String>,
    dragging_window_tool_target_index: Option<usize>,
    dragging_titlebar_tool_id: Option<String>,
    dragging_titlebar_tool_target_index: Option<usize>,
    dragging_game_id: Option<String>,
    dragging_game_target_index: Option<usize>,
    tasks_window_nonce: u64,
    tasks_force_default_pos: bool,
    tasks_tab: TasksTab,
    tasks_tab_scroll_offsets: [f32; TasksTab::TAB_ORDER.len()],
    tasks_scroll_to_edge: bool,
    task_row_advance_cache: HashMap<u64, (TaskStatus, f32)>,
    task_row_advance_cache_width: f32,
    install_queue: VecDeque<InstallJob>,
    install_batch_active: bool,
    install_batch_stats: InstallBatchStats,
    install_inflight: HashMap<u64, InstallJob>,
    install_next_job_id: u64,
    install_request_tx: WorkerTx<InstallRequest>,
    install_event_rx: WorkerRx<InstallEvent>,
    browse_query: String,
    browse_search_expanded: bool,
    browse_search_focus_pending: bool,
    pending_browse_open_mod_id: Option<u64>,
    browse_state: BrowseState,
    my_mod_overlay_images: Vec<MyModOverlayImage>,
    my_mod_source_expanded: bool,
    mod_detail_source_focus_pending: bool,
    mod_detail_source_glow_start: Option<f64>,
    game_icon_textures: HashMap<String, egui::TextureHandle>,
    tool_icon_textures: HashMap<String, egui::TextureHandle>,
    game_cover_textures: HashMap<String, egui::TextureHandle>,
    game_cover_backdrop_textures: HashMap<String, egui::TextureHandle>,
    mod_thumbnail_placeholder: Option<egui::TextureHandle>,
    mod_cover_textures: HashMap<String, egui::TextureHandle>,
    mod_full_textures: HashMap<String, egui::TextureHandle>,
    browse_image_textures: HashMap<String, egui::TextureHandle>,
    browse_thumb_textures: HashMap<String, egui::TextureHandle>,
    censored_copies: HashMap<egui::TextureId, egui::TextureHandle>,
    icon_request_tx: WorkerTx<IconRequest>,
    icon_result_rx: WorkerRx<IconResult>,
    mod_image_request_tx: WorkerTx<LocalModImageRequest>,
    mod_image_result_rx: WorkerRx<LocalModImageResult>,
    manual_image_event_tx: WorkerTx<ManualImageEvent>,
    manual_image_event_rx: WorkerRx<ManualImageEvent>,
    overlay_copy_event_tx: WorkerTx<OverlayImageCopyEvent>,
    overlay_copy_event_rx: WorkerRx<OverlayImageCopyEvent>,
    manual_image_imports_pending: usize,
    pending_mod_image_requests: HashSet<String>,
    pending_mod_image_queue: Vec<LocalModImageRequest>,
    pending_image_loads: HashSet<String>,
    inflight_full_image_requests: HashSet<String>,
    mod_thumb_unavailable: HashMap<String, ModThumbFailure>,
    image_queue_len_after_logic: usize,
    pending_icon_requests: HashSet<String>,
    cover_request_tx: WorkerTx<CoverRequest>,
    cover_result_rx: WorkerRx<CoverResult>,
    pending_cover_requests: HashSet<String>,
    youtube_icon_texture: Option<egui::TextureHandle>,
    app_icon_texture: Option<egui::TextureHandle>,
    tool_icon_texture_failures: HashMap<String, Instant>,
    browse_request_tx: WorkerTx<BrowseRequest>,
    browse_event_rx: WorkerRx<BrowseEvent>,
    browse_image_request_tx: WorkerTx<BrowseImageRequest>,
    browse_image_result_rx: WorkerRx<BrowseImageResult>,
    browse_download_event_rx: WorkerRx<BrowseDownloadEvent>,
    browse_download_result_tx: WorkerTx<BrowseDownloadEvent>,
    app_update_event_tx: WorkerTx<AppUpdateEvent>,
    app_update_event_rx: WorkerRx<AppUpdateEvent>,
    app_update_download_inflight: Option<AppUpdateDownloadInflight>,
    pending_proxy_app_update_resume: Option<AppUpdateManifest>,
    app_update_manifest: Option<AppUpdateManifest>,
    app_update_verified_path: Option<PathBuf>,
    app_update_task_id: Option<u64>,
    app_update_button_state: AppUpdateButtonState,
    app_update_button_spin_until: f64,
    browse_image_queue: Vec<BrowseImageRequest>,
    browse_image_inflight: HashMap<String, BrowseImageInflight>,
    browse_image_retry_after: HashMap<String, Instant>,
    pending_texture_uploads: VecDeque<PendingTextureUpload>,
    texture_meta: HashMap<(TextureKind, String), TextureEntryMeta>,
    texture_access_tick: u64,
    texture_ram_estimated_bytes: u64,
    texture_ram_budget_bytes: u64,
    texture_evictions_window_start: f64,
    texture_evictions_window_count: u64,
    texture_evictions_per_minute: u64,
    browse_download_queue: VecDeque<BrowseDownloadJob>,
    browse_download_inflight: HashMap<u64, BrowseDownloadInflight>,
    proxy_requeue_browse_downloads: HashSet<u64>,
    pending_browse_install_safety: HashMap<u64, bool>,
    pending_browse_install_meta: HashMap<u64, PendingBrowseInstallMeta>,
    thalia_install_bundles: Vec<ThaliaInstallBundle>,
    thalia_separate_install_jobs: HashSet<u64>,
    thalia_canceled_bundle_tasks: HashSet<u64>,
    thalia_bundle_manifests: HashMap<u64, Arc<importing::bundle::FolderManifest>>,
    gif_rewritten_markdown_cache: HashMap<String, String>,
    markdown_dependency_signature_cache: HashMap<String, (String, Instant)>,
    render_safe_markdown_cache: HashMap<String, String>,
    path_file_status_cache: Mutex<HashMap<PathBuf, (bool, Instant)>>,
    path_dir_status_cache: Mutex<HashMap<PathBuf, (bool, Instant)>>,
    path_write_status_cache: Mutex<HashMap<PathBuf, (bool, Instant)>>,
    browse_commonmark_cache: CommonMarkCache,
    browse_request_nonce: u64,
    browse_page_generation: u64,
    browse_detail_generation: u64,
    browse_detail_request_nonces: HashMap<u64, u64>,
    image_generation: Arc<AtomicU64>,
    translation_request_tx: WorkerTx<TranslationRequest>,
    translation_event_rx: WorkerRx<TranslationEvent>,
    translation_inflight: HashMap<(u64, String, String), u64>,
    unlinked_translation_inflight: HashMap<(String, String, String), u64>,
    unlinked_translation_cancellations: HashMap<u64, Arc<AtomicBool>>,
    translation_request_nonce: u64,
    cancelled_translation_requests: HashSet<u64>,
    my_mods_translation_state: HashMap<String, MyModTranslationState>,
    my_mod_updates: HashMap<u64, MyModUpdatesState>,
    my_mod_updates_inflight: HashSet<u64>,
    update_check_tx: WorkerTx<UpdateCheckRequest>,
    update_check_rx: WorkerRx<UpdateCheckResult>,
    update_check_inflight: bool,
    update_check_generation: u64,
    update_check_active_items: Vec<UpdateCheckItem>,
    pending_update_check_game: Option<String>,
    pending_update_check_mods: HashSet<String>,
    pending_link_category_mods: HashSet<String>,
    thalia_matching: ThaliaMatchingState,
    refresh_request_tx: WorkerTx<RefreshRequest>,
    refresh_result_rx: WorkerRx<RefreshEvent>,
    refresh_inflight: bool,
    refresh_pending_selected_game: Option<String>,
    xxmi_reload_event_tx: WorkerTx<XxmiReloadEvent>,
    xxmi_reload_event_rx: WorkerRx<XxmiReloadEvent>,
    xxmi_reload_inflight: HashSet<String>,
    grant_access_event_tx: WorkerTx<GrantAccessEvent>,
    grant_access_event_rx: WorkerRx<GrantAccessEvent>,
    grant_access_inflight: bool,
    xxmi_reload_pending: HashSet<String>,
    xxmi_namespace_cache: HashMap<String, XxmiNamespaceCacheEntry>,
    profile_request_tx: WorkerTx<ProfileRequest>,
    profile_reconcile_request_tx: WorkerTx<ProfileReconcileSpec>,
    profile_event_rx: WorkerRx<ProfileEvent>,
    profile_operation_inflight: Option<ProfileOperationInflight>,
    profile_compression_states: HashMap<(String, ProfileId), ProfileCompressionUiState>,
    profile_next_operation_id: u64,
    profile_recovery_queue: VecDeque<GameInstall>,
    profile_recovery_failed_games: HashSet<String>,
    profile_reconcile_inflight: HashSet<String>,
    profile_selector_popup_open_last_frame: bool,
    profile_name_prompt: Option<ProfileOperationKind>,
    profile_name_target_id: Option<ProfileId>,
    profile_name_draft: String,
    pending_profile_delete_id: Option<ProfileId>,
    pending_d3dx_foreground_conflict: Option<D3dxForegroundConflictPrompt>,
    pending_reload_summary: Option<(String, Vec<ReloadSnapshot>)>,
    pending_install_finalize: HashMap<u64, PendingInstallFinalize>,
    pending_known_installed_paths: HashSet<PathBuf>,
    reload_spin_until: f64,
    reload_was_busy: bool,
    cache_limit_bytes: Arc<std::sync::atomic::AtomicU64>,
    usage_cache_bytes: u64,
    usage_archive_bytes: u64,
    usage_counters_last_refresh: f64,
    usage_counters_dirty: bool,
    window_state_cache: Option<WindowStateSnapshot>,
    window_state_last_save: f64,
    window_was_maximized: bool,
    window_zoom: f32,
    window_min_size: Option<egui::Vec2>,
    floating_window_save_due: Option<f64>,
    selection_empty_at: Option<f64>,
    startup_scan_loading: bool,
    startup_launch_pending: bool,
    startup_selected_game: usize,
    startup_path_targets_pending: Vec<StartupPathScanTarget>,
    startup_scan_tx: WorkerTx<StartupScanEvent>,
    startup_scan_rx: WorkerRx<StartupScanEvent>,
    startup_path_scan: Option<StartupPathScanState>,
    startup_path_scan_tx: WorkerTx<StartupPathScanEvent>,
    startup_path_scan_rx: WorkerRx<StartupPathScanEvent>,
    gif_preview_request_tx: WorkerTx<GifPreviewRequest>,
    gif_preview_event_rx: WorkerRx<GifPreviewEvent>,
    gif_animation_request_tx: WorkerTx<GifAnimationRequest>,
    gif_animation_event_rx: WorkerRx<GifAnimationEvent>,
    gif_dest_by_texture_key: HashMap<String, String>,
    pending_gif_previews: HashMap<String, PendingGifPreview>,
    gif_preview_requests_in_flight: usize,
    pending_gif_animations: HashMap<String, PendingGifAnimation>,
    gif_animation_requests_in_flight: usize,
    animated_gif_state: HashMap<String, AnimatedGifState>,
    visible_gif_process_texture_keys: HashSet<String>,
    visible_gif_texture_keys: HashSet<String>,
    last_visible_gif_texture_keys: HashSet<String>,
    pending_events: PendingEventsFlags,
}

#[derive(Default)]
struct PendingEventsFlags {
    has_worker_events: bool,
    has_process_work: bool,
}

#[derive(Clone)]
struct PendingConflict {
    job_id: u64,
    candidate_indices: Vec<usize>,
    preferred_name: String,
    target_root: PathBuf,
    existing_target: PathBuf,
    gb_profile: Option<Box<gamebanana::ProfileResponse>>,
}

#[derive(Clone)]
struct D3dxForegroundConflictPrompt {
    game_id: String,
    game_name: String,
    path: PathBuf,
    fields: Vec<(String, String)>,
}

#[derive(Clone)]
struct PendingImport {
    job_id: u64,
    inspection: ImportInspection,
    gb_profile: Option<Box<gamebanana::ProfileResponse>>,
}

#[derive(Clone)]
struct PendingInstallFinalize {
    installed_paths: Vec<PathBuf>,
    installed_candidate_labels: Vec<(PathBuf, String)>,
    gb_profile: Option<Box<gamebanana::ProfileResponse>>,
    rel_paths: Vec<String>,
    pending_meta: Option<PendingBrowseInstallMeta>,
    pending_unsafe: bool,
    install_state: ModInstallState,
    preserve_existing_state: bool,
    preserved_states: Vec<(PathBuf, ModStatus)>,
    target_category_id: Option<String>,
}

#[derive(Clone)]
struct InstallJob {
    id: u64,
    game_id: String,
    source: ImportSource,
    title: Option<String>,
    reuse_existing_task: bool,
    install_state: ModInstallState,
    preserve_existing_state: bool,
    preserved_states: Vec<(PathBuf, ModStatus)>,
    category_id: Option<String>,
}

#[derive(Default)]
struct BrowseState {
    active_game_id: Option<String>,
    active_query: Option<String>,
    active_character_category_id: Option<u64>,
    cards: Vec<BrowseCard>,
    total_count: Option<usize>,
    next_page: usize,
    has_more: bool,
    loading_page: bool,
    page_request_nonce: Option<u64>,
    page_request_started_at: Option<Instant>,
    page_error: Option<String>,
    refresh_page_cache_for_session: bool,
    selected_mod_id: Option<u64>,
    character_categories_game_id: Option<String>,
    character_categories: Vec<BrowseCharacterCategory>,
    character_categories_loading: bool,
    character_categories_started_at: Option<Instant>,
    character_filter_query: String,
    character_filter_focus_pending: bool,
    toggle_character_picker_requested: bool,
    selected_character_category: Option<BrowseCharacterCategory>,
    details: HashMap<u64, Arc<BrowseDetailCache>>,
    loading_details: HashMap<u64, Instant>,
    detail_request_attempts: HashMap<u64, Instant>,
    pending_installs: Vec<PendingBrowseInstall>,
    pending_file_review: Option<PendingModFileReview>,
    file_prompt: Option<BrowseFilePrompt>,
    screenshot_overlay: Option<BrowseOverlayImage>,
    overlay_scroll_accum: f32,
    overlay_scroll_last_event_at: f64,
    overlay_scroll_last_step_at: f64,
}

#[derive(Clone)]
struct BrowseCharacterCategory {
    id: u64,
    name: String,
    item_count: u64,
    icon_url: Option<String>,
}

#[derive(Clone)]
struct BrowseCard {
    id: u64,
    game_id: String,
    name: String,
    author_name: String,
    like_count: u64,
    download_count: Option<u64>,
    updated_at: DateTime<Utc>,
    thumbnail_url: Option<String>,
    has_files: bool,
    unsafe_content: bool,
}

#[derive(Clone)]
struct BrowseDetailCache {
    profile: gamebanana::ProfileResponse,
    is_tool: bool,
    profile_cached_at: Option<DateTime<Utc>>,
    markdown: String,
    unsafe_content: bool,
    updates: BrowseUpdatesState,
    translated_profile: Option<gamebanana::ProfileResponse>,
    translation_lang: Option<String>,
    translation_loading: bool,
}

#[derive(Clone, Default)]
struct MyModTranslationState {
    translated_profile: Option<gamebanana::ProfileResponse>,
    translation_lang: Option<String>,
    translation_source_hash: Option<String>,
    translation_loading: bool,
    unlinked_translations: HashMap<String, String>,
    unlinked_loading: HashSet<String>,
    unlinked_translation_enabled: bool,
}

#[derive(Clone)]
struct BrowseUpdateEntry {
    name: String,
    version: Option<String>,
    updated_at: DateTime<Utc>,
    markdown: String,
}

#[derive(Clone)]
enum BrowseUpdatesState {
    Unrequested,
    Loading,
    Loaded(Vec<BrowseUpdateEntry>),
    Empty,
    Failed(String),
}

#[derive(Clone)]
enum MyModUpdatesState {
    Loading,
    Ready {
        entries: Vec<BrowseUpdateEntry>,
        fetched_at: DateTime<Utc>,
    },
    Empty {
        fetched_at: DateTime<Utc>,
    },
    Failed {
        message: String,
        attempted_at: DateTime<Utc>,
    },
}

struct PendingBrowseInstall {
    task_id: u64,
    mod_id: u64,
    game_id: String,
    update_target_id: Option<String>,
    install_state: ModInstallState,
    preserve_existing_state: bool,
    thalia_plan: Option<ThaliaInstallPlan>,
}

struct ThaliaInstallBundle {
    key: u64,
    title: String,
    game_id: String,
    members: Vec<u64>,
    separate: HashSet<u64>,
    main: Option<u64>,
    target: Option<(String, Option<u64>)>,
    answer: Option<(String, ThaliaRecommendation)>,
    downloaded: HashMap<u64, (String, String, PathBuf)>,
    preparing: bool,
    prepared_root: Option<PathBuf>,
    target_changed: bool,
    finished: HashSet<u64>,
    installed_any: bool,
}

#[derive(Clone)]
struct PendingModFileReview {
    mod_entry_id: String,
    game_id: String,
    gb_id: u64,
    is_tool: bool,
    source_url: String,
}

#[derive(Clone)]
struct FileReviewContext {
    recommended_action: Option<FileReviewAction>,
    installed_files: Vec<TrackedFileMeta>,
    newer_file_ids: Vec<u64>,
    other_mod_titles: Vec<(u64, String)>,
    dismissible_file_ids: Vec<u64>,
    expected_file_set: FileSetRecipe,
    target_root_path: PathBuf,
    expected_source_url: String,
    expected_is_tool: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileReviewAction {
    Update,
    InstallSeparate,
    InstallMerge,
}

#[derive(Clone)]
struct PendingBrowseInstallMeta {
    mod_id: u64,
    game_id: String,
    selected_files: Vec<gamebanana::ModFile>,
    update_folder_name: Option<String>,
    update_target_mod_id: Option<String>,
    update_target_was_disabled: bool,
    install_state: ModInstallState,
    preserve_existing_state: bool,
    post_install_rename_to: Option<String>,
    profile_cached_at: Option<DateTime<Utc>>,
    is_tool: bool,
    explicit_resolution: Option<ImportResolution>,
}

type UpdateCheckItem = (String, String, u64, Option<i64>, FileSetRecipe, bool);

fn update_check_group_fingerprint(items: &[UpdateCheckItem]) -> u64 {
    let mut entries: Vec<_> = items
        .iter()
        .map(|item| {
            serde_json::to_string(&(
                &item.0, &item.1, item.2, item.3, &item.4, item.5,
            ))
            .unwrap_or_default()
        })
        .collect();
    entries.sort_unstable();
    let mut hasher = DefaultHasher::new();
    entries.hash(&mut hasher);
    hasher.finish()
}

fn update_check_group_fingerprint_for_group(
    items: &[UpdateCheckItem],
    game_id: &str,
    gb_id: u64,
    gb_is_tool: bool,
) -> u64 {
    let group: Vec<_> = items
        .iter()
        .filter(|item| item.1 == game_id && item.2 == gb_id && item.5 == gb_is_tool)
        .cloned()
        .collect();
    update_check_group_fingerprint(&group)
}

struct UpdateCheckRequest {
    generation: u64,
    items: Vec<UpdateCheckItem>,
    peer_items: Vec<UpdateCheckItem>,
}

struct UpdateCheckOutcome {
    mod_id: String,
    game_id: String,
    gb_id: u64,
    gb_is_tool: bool,
    file_set: FileSetRecipe,
    state: ModUpdateState,
    signature: Option<IgnoredUpdateSignature>,
    snapshot: Option<GameBananaSnapshot>,
    error: Option<String>,
    raw_json: Option<String>,
    profile: Option<Box<gamebanana::ProfileResponse>>,
    profile_cached_at: Option<DateTime<Utc>>,
    peer_fingerprint: u64,
}

struct UpdateCheckResult {
    generation: u64,
    states: Vec<UpdateCheckOutcome>,
}

struct BrowseFilePrompt {
    mod_id: u64,
    game_id: String,
    files: Vec<BrowseSelectableFile>,
    update_folder_name: Option<String>,
    update_target_mod_id: Option<String>,
    post_install_rename_to: Option<String>,
    install_state: ModInstallState,
    preserve_existing_state: bool,
    file_review: Option<FileReviewContext>,
}

#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct AppUpdateManifest {
    app: String,
    version: String,
    url: String,
    download: Vec<String>,
    bytes: u64,
    sha256: String,
    signature: String,
}

#[derive(serde::Serialize)]
struct AppUpdateManifestPayload<'a> {
    app: &'a str,
    version: &'a str,
    url: &'a str,
    download: &'a [String],
    bytes: u64,
    sha256: &'a str,
}

impl<'a> From<&'a AppUpdateManifest> for AppUpdateManifestPayload<'a> {
    fn from(manifest: &'a AppUpdateManifest) -> Self {
        Self {
            app: &manifest.app,
            version: &manifest.version,
            url: &manifest.url,
            download: &manifest.download,
            bytes: manifest.bytes,
            sha256: &manifest.sha256,
        }
    }
}

#[derive(Clone)]
struct AppUpdateDownloadInflight {
    task_id: u64,
    destination: PathBuf,
    manifest: AppUpdateManifest,
    cancel: Arc<AtomicBool>,
    progress: Arc<RwLock<DownloadProgress>>,
}

enum AppUpdateEvent {
    CheckDone {
        manifest: AppUpdateManifest,
        verified_path: Option<PathBuf>,
    },
    UpToDate,
    CheckFailed {
        error: String,
    },
    DownloadDone {
        task_id: u64,
        manifest: AppUpdateManifest,
        path: PathBuf,
        bytes: u64,
    },
    DownloadFailed {
        task_id: u64,
        error: String,
    },
    DownloadCanceled {
        task_id: u64,
    },
}

enum ProxyApplyEvent {
    Validated { proxy: Option<CustomProxyConfig> },
    Failed { error: String },
}

#[derive(Clone)]
struct FeedbackSurveySubmitRequest {
    version: String,
    payload_json: String,
    pending_path: PathBuf,
    discard_on_failure: bool,
    cancel: Arc<AtomicBool>,
}

enum FeedbackSurveySubmitEvent {
    Canceled,
    Submitted {
        version: String,
        pending_path: PathBuf,
    },
    Failed {
        version: String,
        pending_path: PathBuf,
        error: String,
        discard_on_failure: bool,
    },
}

struct BrowseSelectableFile {
    file: gamebanana::ModFile,
    selected: bool,
}

struct BrowseOverlayImage {
    texture_key: String,
    caption: Option<String>,
}

#[derive(Clone)]
struct MyModOverlayImage {
    texture_key: String,
    url: Option<String>,
    caption: Option<String>,
}

enum BrowseRequest {
    CancelPage,
    FetchPage {
        nonce: u64,
        generation: u64,
        game_id: String,
        query: Option<String>,
        character_category_id: Option<u64>,
        page: usize,
        browse_sort: BrowseSort,
        search_sort: SearchSort,
        force_refresh: bool,
    },
    FetchCharacterCategories {
        nonce: u64,
        game_id: String,
        super_category_id: u64,
        force_refresh: bool,
    },
    FetchDetail {
        nonce: u64,
        mod_id: u64,
        is_tool: bool,
        force_refresh: bool,
        include_update_history: bool,
        cached_profile_json: Option<String>,
    },
    FetchUpdates {
        nonce: u64,
        mod_id: u64,
        is_tool: bool,
        force_refresh: bool,
    },
    FetchMyModUpdates {
        nonce: u64,
        mod_id: u64,
    },
}

#[derive(Clone)]
struct GifAnimationFrame {
    image: egui::ColorImage,
    delay_ms: u32,
}

#[derive(Clone)]
struct GifAnimation {
    frames: Vec<GifAnimationFrame>,
}

struct PendingGifPreview {
    texture_key: String,
    cancel: Arc<AtomicBool>,
    started_at: Instant,
}

struct PendingGifAnimation {
    cancel: Arc<AtomicBool>,
    started_at: Instant,
}

enum GifPreviewRequest {
    FromFile {
        src_path: PathBuf,
        out_png: PathBuf,
        gif_dest: String,
        max_width: u32,
        cancel: Arc<AtomicBool>,
    },
    FromUrl {
        url: String,
        out_png: PathBuf,
        gif_dest: String,
        max_width: u32,
        cancel: Arc<AtomicBool>,
    },
}

enum GifPreviewEvent {
    Ready {
        out_png: PathBuf,
        gif_dest: String,
        image: egui::ColorImage,
    },
    Failed {
        out_png: PathBuf,
    },
}

enum GifAnimationRequest {
    FromFile {
        src_path: PathBuf,
        texture_key: String,
        max_size: [u32; 2],
        cancel: Arc<AtomicBool>,
    },
    FromUrl {
        url: String,
        texture_key: String,
        max_size: [u32; 2],
        cancel: Arc<AtomicBool>,
    },
}

enum GifAnimationEvent {
    Ready {
        texture_key: String,
        animation: GifAnimation,
    },
    Failed {
        texture_key: String,
        error: String,
        cancelled: bool,
    },
}

#[derive(Clone)]
struct AnimatedGifState {
    animation: GifAnimation,
    current_frame: usize,
    frame_start_time: f64,
}

enum BrowseEvent {
    PageLoaded {
        _nonce: u64,
        generation: u64,
        game_id: String,
        query: Option<String>,
        character_category_id: Option<u64>,
        page: usize,
        payload: gamebanana::ApiEnvelope<gamebanana::BrowseRecord>,
    },
    PageWarning {
        _nonce: u64,
        generation: u64,
        warning: String,
    },
    PageFailed {
        _nonce: u64,
        generation: u64,
        page: usize,
        error: String,
    },
    CharacterCategoriesLoaded {
        _nonce: u64,
        game_id: String,
        categories: Vec<gamebanana::CharacterCategory>,
    },
    CharacterCategoriesWarning {
        _nonce: u64,
        game_id: String,
        warning: String,
    },
    CharacterCategoriesFailed {
        _nonce: u64,
        game_id: String,
        error: String,
    },
    DetailLoaded {
        _nonce: u64,
        mod_id: u64,
        is_tool: bool,
        profile: gamebanana::ProfileResponse,
        profile_cached_at: Option<DateTime<Utc>>,
    },
    DetailWarning {
        _nonce: u64,
        mod_id: u64,
        warning: String,
    },
    DetailFailed {
        _nonce: u64,
        mod_id: u64,
        error: String,
    },
    UpdatesLoaded {
        _nonce: u64,
        mod_id: u64,
        updates: gamebanana::ApiEnvelope<gamebanana::UpdateRecord>,
    },
    UpdatesWarning {
        _nonce: u64,
        mod_id: u64,
        warning: String,
    },
    UpdatesFailed {
        _nonce: u64,
        mod_id: u64,
        error: String,
    },
    MyModUpdatesLoaded {
        _nonce: u64,
        mod_id: u64,
        updates: gamebanana::ApiEnvelope<gamebanana::UpdateRecord>,
    },
    MyModUpdatesFailed {
        _nonce: u64,
        mod_id: u64,
        error: String,
    },
}

#[derive(Clone)]
struct BrowseImageRequest {
    texture_key: String,
    thumb_texture_key: String,
    url: String,
    cache_key: String,
    cancel_key: Option<u64>,
    cancel: Arc<AtomicBool>,
    skip_texture: bool,
    load_full: bool,
    priority: u32,
    thumb_profile: ThumbnailProfile,
}

struct BrowseImageResult {
    texture_key: String,
    thumb_texture_key: String,
    image_full: Option<egui::ColorImage>,
    image_thumb: Option<egui::ColorImage>,
    cancel_key: Option<u64>,
    failure: Option<BrowseImageFailure>,
}

struct BrowseImageInflight {
    request: BrowseImageRequest,
    cancel: Arc<AtomicBool>,
    cancel_key: Option<u64>,
    skip_texture: bool,
    load_full: bool,
    started_at: Instant,
}

struct BrowseImageFailure {
    url: String,
    timed_out: bool,
    error: String,
}

#[derive(Clone)]
struct BrowseDownloadJob {
    task_id: u64,
    title: String,
    url: String,
    cache_key: String,
    file_name: String,
    cache_limit_bytes: u64,
    total_size: Option<u64>,
    md5: Option<String>,
}

struct DownloadProgress {
    downloaded: u64,
    total: Option<u64>,
    speed: f64,
    last_update: std::time::Instant,
    bytes_since_last: u64,
}

struct BrowseDownloadInflight {
    cancel: Arc<AtomicBool>,
    progress: Arc<RwLock<DownloadProgress>>,
}

enum BrowseDownloadEvent {
    Done {
        task_id: u64,
        title: String,
        cache_key: String,
        file_name: String,
        byte_size: u64,
    },
    Failed {
        task_id: u64,
        title: String,
        error: String,
    },
    Canceled {
        task_id: u64,
        title: String,
    },
}

enum InstallRequest {
    Inspect {
        job_id: u64,
        game_id: String,
        source: ImportSource,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
    },
    Install {
        job_id: u64,
        candidate_indices: Vec<usize>,
        candidate_install_states: Vec<ModStatus>,
        preferred_names: Vec<String>,
        choice: ConflictChoice,
        target_root: PathBuf,
        game_backend: GameBackend,
        disabled_target_root: Option<PathBuf>,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
        expected_manifest: Option<Arc<importing::bundle::FolderManifest>>,
    },
    PrepareThaliaBundle {
        job_id: u64,
        game_id: String,
        parts: Vec<importing::bundle::BundlePart>,
    },
    SyncImages {
        job_id: u64,
        mod_entry_id: String,
        mod_root_path: PathBuf,
        profile: Box<gamebanana::ProfileResponse>,
        profile_cached_at: Option<DateTime<Utc>>,
    },
    Cancel {
        job_id: u64,
    },
    Drop {
        job_id: u64,
    },
}

enum InstallEvent {
    InspectReady {
        job_id: u64,
        inspection: ImportInspection,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
    },
    InspectFailed {
        job_id: u64,
        error: String,
    },
    InstallDone {
        job_id: u64,
        installed_paths: Vec<PathBuf>,
        installed_candidate_labels: Vec<(PathBuf, String)>,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
        rel_paths: Vec<String>,
    },
    InstallFailed {
        job_id: u64,
        preferred_name: String,
        error: String,
    },
    InstallCanceled {
        job_id: u64,
    },
    ThaliaBundlePrepared {
        job_id: u64,
        root: PathBuf,
        folders: Vec<importing::bundle::PreparedBundleFolder>,
    },
    ThaliaBundlePrepareFailed {
        job_id: u64,
        failure: importing::bundle::BundleFailure,
    },
    SyncImagesDone {
        _job_id: u64,
        mod_entry_id: String,
        profile: Box<gamebanana::ProfileResponse>,
        rel_paths: Vec<String>,
        profile_cached_at: Option<DateTime<Utc>>,
    },
    SyncImagesCover {
        mod_entry_id: String,
        cover_rel_path: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TasksTab {
    Downloads,
    Installs,
    Completed,
    Failed,
}

impl TasksTab {
    const TAB_ORDER: &'static [TasksTab] = &[
        TasksTab::Downloads,
        TasksTab::Installs,
        TasksTab::Completed,
        TasksTab::Failed,
    ];

    fn scroll_index(self) -> usize {
        match self {
            TasksTab::Downloads => 0,
            TasksTab::Installs => 1,
            TasksTab::Completed => 2,
            TasksTab::Failed => 3,
        }
    }
}

struct CoverRequest {
    game_id: String,
}

struct CoverResult {
    game_id: String,
    image: egui::ColorImage,
    backdrop: egui::ColorImage,
}

struct IconRequest {
    game_id: String,
}

struct IconResult {
    game_id: String,
    image: egui::ColorImage,
}

struct LocalModImageRequest {
    texture_key: String,
    mode: LocalModImageMode,
    priority: u32,
    generation: u64,
    payload: LocalModImagePayload,
}

struct LocalModImageResult {
    texture_key: String,
    image_full: Option<egui::ColorImage>,
    image_thumb: Option<egui::ColorImage>,
    done: bool,
    thumb_generated: bool,
    thumb_meta: Option<CardThumbMeta>,
}

enum ManualImageEvent {
    Added {
        mod_id: String,
        folder_name: String,
        rel_paths: Vec<String>,
    },
    BundledAdopted {
        mod_id: String,
        folder_name: String,
        rel_paths: Vec<String>,
    },
    Failed {
        folder_name: String,
        error: String,
    },
}

enum OverlayImageCopySource {
    File(PathBuf),
    Url(String),
}

struct OverlayImageCopyEvent {
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LocalModImageMode {
    CardThumbOnly,
    ThumbOnly,
    FullOnly,
}

#[derive(Clone)]
enum LocalModImagePayload {
    Path {
        path: PathBuf,
        thumb_profile: ThumbnailProfile,
    },
    CardThumb {
        mod_root: PathBuf,
        source_path: Option<PathBuf>,
        source_url: Option<String>,
        expected_meta: CardThumbMeta,
        force_regen: bool,
    },
}

#[derive(Clone)]
struct CardThumbMeta {
    kind: String,
    id: String,
    mtime: Option<i64>,
    size: Option<u64>,
}

struct ModThumbFailure {
    kind: String,
    id: String,
    mtime: Option<i64>,
    size: Option<u64>,
    at: Instant,
}

impl ModThumbFailure {
    fn still_applies(&self, meta: &CardThumbMeta) -> bool {
        if self.kind != meta.kind || self.id != meta.id {
            return false;
        }
        if self.mtime != meta.mtime || self.size != meta.size {
            return false;
        }
        self.kind == "none"
            || self.at.elapsed() < Duration::from_secs(MOD_THUMB_RETRY_COOLDOWN_SECS)
    }

    fn remaining_cooldown(&self) -> Duration {
        Duration::from_secs(MOD_THUMB_RETRY_COOLDOWN_SECS).saturating_sub(self.at.elapsed())
    }
}

enum CardThumbQueueOutcome {
    Requested,
    NotNeeded,
    CoolingDown(Duration),
    NoSource,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThumbnailProfile {
    Card,
    Rail,
    Icon,
}

impl ThumbnailProfile {
    fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Card => (CARD_THUMBNAIL_WIDTH, CARD_THUMBNAIL_HEIGHT),
            Self::Rail => (RAIL_THUMBNAIL_WIDTH, RAIL_THUMBNAIL_HEIGHT),
            Self::Icon => (40, 40),
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Rail => "rail",
            Self::Icon => "icon",
        }
    }
}

enum PendingTextureUpload {
    ModThumb {
        texture_key: String,
        image: egui::ColorImage,
    },
    ModFull {
        texture_key: String,
        image: egui::ColorImage,
    },
    BrowseThumb {
        texture_key: String,
        image: egui::ColorImage,
    },
    BrowseFull {
        texture_key: String,
        image: egui::ColorImage,
    },
}

impl PendingTextureUpload {
    fn priority_class(&self) -> u8 {
        match self {
            PendingTextureUpload::BrowseThumb { .. } | PendingTextureUpload::ModThumb { .. } => 0,
            PendingTextureUpload::BrowseFull { .. } | PendingTextureUpload::ModFull { .. } => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TextureKind {
    ModThumb,
    ModFull,
    BrowseThumb,
    BrowseFull,
}

#[derive(Clone, Copy)]
struct TextureEntryMeta {
    bytes: u64,
    last_access_tick: u64,
    priority: u8,
}

enum StartupScanEvent {
    Ready(Vec<ModEntry>),
    Failed(String),
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum StartupPathTargetKind {
    Xxmi,
    Game(String),
}

#[derive(Clone)]
struct StartupPathScanTarget {
    kind: StartupPathTargetKind,
    label: String,
    file_names: Vec<String>,
    initial_candidates: Vec<PathBuf>,
}

struct StartupPathScanStatus {
    kind: StartupPathTargetKind,
    label: String,
    candidates: Vec<PathBuf>,
    selected_candidate: Option<PathBuf>,
    choosing: bool,
}

struct StartupPathScanState {
    statuses: Vec<StartupPathScanStatus>,
    cancel: Arc<AtomicBool>,
    cancel_requested: bool,
    stopped: bool,
    finished: bool,
    run_initial_mod_scan_after: bool,
}

impl StartupPathScanState {
    fn request_stop(&mut self) {
        self.cancel_requested = true;
        self.stopped = true;
        self.finished = true;
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn accepts_worker_event(&self) -> bool {
        !self.finished && !self.stopped
    }
}

enum StartupPathScanEvent {
    Found {
        kind: StartupPathTargetKind,
        path: PathBuf,
    },
    Finished {
        stopped: bool,
    },
}

fn reset_startup_path_scan_channel(
    tx: &mut WorkerTx<StartupPathScanEvent>,
    rx: &mut WorkerRx<StartupPathScanEvent>,
) {
    let (new_tx, new_rx) = worker_channel();
    *tx = new_tx;
    *rx = new_rx;
}

#[cfg(test)]
mod startup_path_scan_state_tests {
    use super::*;

    fn test_scan_state() -> StartupPathScanState {
        let candidate = PathBuf::from(r"C:\Games\test.exe");
        StartupPathScanState {
            statuses: vec![StartupPathScanStatus {
                kind: StartupPathTargetKind::Game("test-game".to_string()),
                label: "Test game".to_string(),
                candidates: vec![candidate.clone()],
                selected_candidate: Some(candidate),
                choosing: false,
            }],
            cancel: Arc::new(AtomicBool::new(false)),
            cancel_requested: false,
            stopped: false,
            finished: false,
            run_initial_mod_scan_after: true,
        }
    }

    #[test]
    fn request_stop_finishes_scan_and_preserves_results() {
        let mut scan = test_scan_state();
        let candidates = scan.statuses[0].candidates.clone();
        let selected_candidate = scan.statuses[0].selected_candidate.clone();

        scan.request_stop();

        assert!(scan.cancel_requested);
        assert!(scan.stopped);
        assert!(scan.finished);
        assert!(scan.cancel.load(Ordering::Relaxed));
        assert_eq!(scan.statuses[0].candidates, candidates);
        assert_eq!(scan.statuses[0].selected_candidate, selected_candidate);
        assert!(!scan.accepts_worker_event());
    }

    #[test]
    fn reset_scan_channel_drops_events_from_previous_scan() {
        let (mut tx, mut rx) = worker_channel::<StartupPathScanEvent>();
        let old_tx = tx.clone();
        old_tx
            .send(StartupPathScanEvent::Found {
                kind: StartupPathTargetKind::Game("old-game".to_string()),
                path: PathBuf::from(r"C:\Games\old.exe"),
            })
            .unwrap();
        old_tx
            .send(StartupPathScanEvent::Finished { stopped: false })
            .unwrap();

        reset_startup_path_scan_channel(&mut tx, &mut rx);

        assert!(rx.try_recv().is_err());
        assert!(
            old_tx
                .send(StartupPathScanEvent::Finished { stopped: false })
                .is_err()
        );
        tx.send(StartupPathScanEvent::Finished { stopped: false })
            .unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(StartupPathScanEvent::Finished { stopped: false })
        ));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameSetupIssue {
    MissingGamePath,
    MissingModFolder,
    NoGameDirAccess,
    MissingXxmiLauncher,
    MissingNteBypasser,
    MissingUnrealRequirement,
}

#[derive(Clone, Debug)]
struct GameReadiness {
    game_present: bool,
    can_launch_vanilla: bool,
    can_launch_modded: bool,
    can_open_mods_folder: bool,
    can_install_mods: bool,
    can_download_mods: bool,
    primary_issue: Option<GameSetupIssue>,
}

#[derive(Clone)]
struct RefreshRequest {
    game_id: String,
    games: Vec<GameInstall>,
    use_default_mods_path: bool,
    existing_mods: Vec<ModEntry>,
}

enum RefreshEvent {
    Ready {
        game_id: String,
        mods: Vec<ModEntry>,
    },
    Failed {
        game_id: String,
        error: String,
    },
}

struct GrantAccessEvent {
    game_id: String,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileOperationKind {
    Create,
    Duplicate,
    Switch,
    Rename,
    Delete,
    Recover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileCompressionUiState {
    Queued,
    Running,
    Failed,
}

#[derive(Clone)]
struct ProfileOperationInflight {
    operation_id: u64,
    kind: ProfileOperationKind,
    source_display_name: Option<String>,
    target_display_name: Option<String>,
    prepares_before_activating: bool,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    stage: Arc<RwLock<String>>,
}

#[derive(Default)]
struct ProfileArchiveCoordinator {
    state: Mutex<ProfileArchiveCoordinatorState>,
    changed: std::sync::Condvar,
}

#[derive(Default)]
struct ProfileArchiveCoordinatorState {
    foreground_active: bool,
    archive_running: bool,
    archive_not_before: Option<Instant>,
}

#[derive(Clone)]
struct ProfileArchiveJob {
    game_id: String,
    game: GameInstall,
    use_default_mods_path: bool,
    profile_id: ProfileId,
}

#[derive(Clone)]
struct ProfileOperationSpec {
    operation_id: u64,
    game_id: String,
    game: GameInstall,
    use_default_mods_path: bool,
    kind: ProfileOperationKind,
    profile_id: Option<ProfileId>,
    source_profile_id: Option<ProfileId>,
    target_profile_id: Option<ProfileId>,
    display_name: Option<String>,
    target_display_name: Option<String>,
    source_profile: Option<PathBuf>,
    target_profile: Option<PathBuf>,
    source_archive: Option<PathBuf>,
    target_archive: Option<PathBuf>,
    target_categories: Option<Vec<ModCategory>>,
    target_tools: Option<Vec<ToolEntry>>,
    target_tool_blacklist: Option<Vec<String>>,
    known_profile_ids: Vec<ProfileId>,
    known_profile_storage_file_names: Vec<String>,
    metadata: Option<crate::integrations::profiles::ProfileArchiveMetadata>,
    preserve_mod_settings: bool,
    send_reload_hotkey: bool,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    stage: Arc<RwLock<String>>,
}

#[derive(Clone, Default)]
struct ProfileToolSnapshot {
    tools: Vec<ToolEntry>,
    blacklist: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ActiveProfileMarker {
    profile_id: ProfileId,
    display_name: String,
    #[serde(default)]
    categories: Option<Vec<ModCategory>>,
    #[serde(default)]
    tools: Option<Vec<ToolEntry>>,
    #[serde(default)]
    tool_blacklist: Option<Vec<String>>,
}

enum ProfileRequest {
    Execute(ProfileOperationSpec),
}

#[derive(Clone)]
struct ProfileReconcileSpec {
    game_id: String,
    game: GameInstall,
    use_default_mods_path: bool,
    known_profile_ids: Vec<ProfileId>,
    known_profile_storage_file_names: Vec<String>,
}

struct ProfileCompleted {
    game_id: String,
    kind: ProfileOperationKind,
    profile_id: Option<ProfileId>,
    target_profile_id: Option<ProfileId>,
    display_name: Option<String>,
    archive: Option<crate::integrations::profiles::ArchiveResult>,
    active_profile_marker: Option<ActiveProfileMarker>,
    orphaned_profiles: Vec<OrphanedProfile>,
    renamed_profiles: Vec<RecoveredProfileLabel>,
    warnings: Vec<String>,
}

enum ProfileEvent {
    ReadyForReload {
        operation_id: u64,
        game_id: String,
        message: String,
    },
    Completed {
        operation_id: u64,
        completed: Box<ProfileCompleted>,
    },
    ArchiveCompleted {
        game_id: String,
        profile_id: ProfileId,
        archive: crate::integrations::profiles::ArchiveResult,
    },
    ArchiveQueued {
        game_id: String,
        profile_id: ProfileId,
        delay_seconds: u64,
    },
    ArchiveStarted {
        game_id: String,
        profile_id: ProfileId,
    },
    ArchiveSkipped {
        game_id: String,
        profile_id: ProfileId,
    },
    ArchiveFailed {
        game_id: String,
        profile_id: ProfileId,
        error: String,
    },
    Failed {
        operation_id: u64,
        game_id: String,
        error: String,
        recovery_blocking: bool,
        files_in_use: Option<OpenHandleReport>,
    },
    Canceled {
        operation_id: u64,
    },
    ReconcileCompleted {
        game_id: String,
        orphaned_profiles: Vec<OrphanedProfile>,
        renamed_profiles: Vec<RecoveredProfileLabel>,
        warnings: Vec<String>,
    },
    ReconcileFailed {
        game_id: String,
        error: String,
    },
}

#[derive(Clone, Copy, PartialEq)]
struct WindowStateSnapshot {
    pos: Option<[f32; 2]>,
    size: Option<[f32; 2]>,
    maximized: bool,
}
