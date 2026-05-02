#[derive(Clone)]
struct ToastEntry {
    message: String,
    is_error: bool,
    created_at: f64,
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

enum InlineMarkdownEmbed {
    Image { texture_key: String },
    Youtube { url: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Library,
    Browse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Advanced,
    Path,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum ModDetailTab {
    Overview,
    Source,
    Content,
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
    show_check_skipped_mods: bool,
    show_missing_source_mods: bool,
    show_modified_locally_mods: bool,
    show_ignoring_update_mods: bool,
    current_view: ViewMode,
    settings_open: bool,
    mod_detail_open: bool,
    browse_detail_open: bool,
    settings_tab: SettingsTab,
    mod_detail_tab: ModDetailTab,
    last_right_pane_rect: Option<egui::Rect>,
    mod_detail_focus_requested: bool,
    browse_detail_focus_requested: bool,
    mod_detail_editing: bool,
    mod_detail_edit_target_id: Option<String>,
    mod_detail_edit_name: String,
    category_rename_target_id: Option<String>,
    category_rename_name: String,
    dragging_category_id: Option<String>,
    dragging_category_target_index: Option<usize>,
    toasts: Vec<ToastEntry>,
    pending_imports: VecDeque<PendingImport>,
    pending_conflicts: VecDeque<PendingConflict>,
    log_scroll_to_bottom: bool,
    log_window_nonce: u64,
    log_force_default_pos: bool,
    tools_window_nonce: u64,
    tools_force_default_pos: bool,
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
    tasks_scroll_to_edge: bool,
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
    game_icon_textures: HashMap<String, egui::TextureHandle>,
    tool_icon_textures: HashMap<String, egui::TextureHandle>,
    game_cover_textures: HashMap<String, egui::TextureHandle>,
    mod_thumbnail_placeholder: Option<egui::TextureHandle>,
    mod_cover_textures: HashMap<String, egui::TextureHandle>,
    mod_full_textures: HashMap<String, egui::TextureHandle>,
    browse_image_textures: HashMap<String, egui::TextureHandle>,
    browse_thumb_textures: HashMap<String, egui::TextureHandle>,
    icon_request_tx: WorkerTx<IconRequest>,
    icon_result_rx: WorkerRx<IconResult>,
    mod_image_request_tx: WorkerTx<LocalModImageRequest>,
    mod_image_result_rx: WorkerRx<LocalModImageResult>,
    pending_mod_image_requests: HashSet<String>,
    pending_mod_image_queue: Vec<LocalModImageRequest>,
    pending_icon_requests: HashSet<String>,
    cover_request_tx: WorkerTx<CoverRequest>,
    cover_result_rx: WorkerRx<CoverResult>,
    pending_cover_requests: HashSet<String>,
    youtube_icon_texture: Option<egui::TextureHandle>,
    app_icon_texture: Option<egui::TextureHandle>,
    browse_request_tx: WorkerTx<BrowseRequest>,
    browse_event_rx: WorkerRx<BrowseEvent>,
    browse_image_request_tx: WorkerTx<BrowseImageRequest>,
    browse_image_result_rx: WorkerRx<BrowseImageResult>,
    browse_download_event_rx: WorkerRx<BrowseDownloadEvent>,
    browse_download_result_tx: WorkerTx<BrowseDownloadEvent>,
    app_update_event_tx: WorkerTx<AppUpdateEvent>,
    app_update_event_rx: WorkerRx<AppUpdateEvent>,
    app_update_download_inflight: Option<AppUpdateDownloadInflight>,
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
    pending_browse_install_safety: HashMap<u64, bool>,
    pending_browse_install_meta: HashMap<u64, PendingBrowseInstallMeta>,
    browse_commonmark_cache: CommonMarkCache,
    browse_request_nonce: u64,
    browse_page_generation: u64,
    browse_detail_generation: u64,
    image_generation: Arc<AtomicU64>,
    update_check_tx: WorkerTx<UpdateCheckRequest>,
    update_check_rx: WorkerRx<UpdateCheckResult>,
    update_check_inflight: bool,
    pending_update_check_game: Option<String>,
    pending_update_check_mods: HashSet<String>,
    refresh_request_tx: WorkerTx<RefreshRequest>,
    refresh_result_rx: WorkerRx<RefreshEvent>,
    refresh_inflight: bool,
    refresh_pending_selected_game: Option<String>,
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
    selection_empty_at: Option<f64>,
    startup_scan_loading: bool,
    startup_scan_rx: WorkerRx<StartupScanEvent>,
    gif_preview_request_tx: WorkerTx<GifPreviewRequest>,
    gif_preview_event_rx: WorkerRx<GifPreviewEvent>,
    gif_animation_request_tx: WorkerTx<GifAnimationRequest>,
    gif_animation_event_rx: WorkerRx<GifAnimationEvent>,
    pending_gif_previews: HashSet<String>,
    pending_gif_animations: HashSet<String>,
    animated_gif_state: HashMap<String, AnimatedGifState>,
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
}

#[derive(Clone)]
struct InstallJob {
    id: u64,
    game_id: String,
    source: ImportSource,
    title: Option<String>,
    reuse_existing_task: bool,
}

#[derive(Default)]
struct BrowseState {
    active_game_id: Option<String>,
    active_query: Option<String>,
    cards: Vec<BrowseCard>,
    total_count: Option<usize>,
    next_page: usize,
    has_more: bool,
    loading_page: bool,
    refresh_page_cache_for_session: bool,
    selected_mod_id: Option<u64>,
    details: HashMap<u64, BrowseDetailCache>,
    loading_details: HashSet<u64>,
    pending_installs: Vec<PendingBrowseInstall>,
    file_prompt: Option<BrowseFilePrompt>,
    screenshot_overlay: Option<BrowseOverlayImage>,
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
    markdown: String,
    unsafe_content: bool,
    updates: BrowseUpdatesState,
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

struct PendingBrowseInstall {
    task_id: u64,
    mod_id: u64,
    game_id: String,
    update_target_id: Option<String>,
}

#[derive(Clone)]
struct PendingBrowseInstallMeta {
    mod_id: u64,
    game_id: String,
    selected_files: Vec<gamebanana::ModFile>,
    update_folder_name: Option<String>,
    update_target_mod_id: Option<String>,
    post_install_rename_to: Option<String>,
}

struct UpdateCheckRequest {
    items: Vec<(String, String, u64, Option<i64>, FileSetRecipe)>, // mod_id, game_id, gb_id, old_update_ts, file_set
}

struct UpdateCheckResult {
    states: Vec<(
        String,
        ModUpdateState,
        Option<GameBananaSnapshot>,
        Option<String>,
        Option<String>,
        Option<Box<gamebanana::ProfileResponse>>,
    )>,
}

struct BrowseFilePrompt {
    mod_id: u64,
    game_id: String,
    files: Vec<BrowseSelectableFile>,
    update_folder_name: Option<String>,
    update_target_mod_id: Option<String>,
    post_install_rename_to: Option<String>,
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

struct BrowseSelectableFile {
    file: gamebanana::ModFile,
    selected: bool,
}

struct BrowseOverlayImage {
    texture_key: String,
}

#[derive(Clone)]
struct MyModOverlayImage {
    texture_key: String,
    url: Option<String>,
    caption: Option<String>,
}

enum BrowseRequest {
    FetchPage {
        nonce: u64,
        generation: u64,
        game_id: String,
        query: Option<String>,
        page: usize,
        browse_sort: BrowseSort,
        search_sort: SearchSort,
        force_refresh: bool,
    },
    FetchDetail {
        nonce: u64,
        mod_id: u64,
        force_refresh: bool,
        cached_profile_json: Option<String>,
    },
    FetchUpdates {
        nonce: u64,
        mod_id: u64,
        force_refresh: bool,
    },
}

enum GifPreviewRequest {
    FromFile { src_path: PathBuf, out_png: PathBuf, gif_dest: String },
    FromUrl { url: String, out_png: PathBuf, gif_dest: String },
}

enum GifPreviewEvent {
    Ready { out_png: PathBuf, gif_dest: String, image: egui::ColorImage },
    Failed { out_png: PathBuf },
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

enum GifAnimationRequest {
    FromFile { src_path: PathBuf, texture_key: String },
    FromUrl { url: String, texture_key: String },
}

enum GifAnimationEvent {
    Ready { texture_key: String, animation: GifAnimation },
    Failed { texture_key: String, error: String },
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
        error: String,
    },
    DetailLoaded {
        _nonce: u64,
        mod_id: u64,
        profile: gamebanana::ProfileResponse,
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
    cancel: Arc<AtomicBool>,
    cancel_key: Option<u64>,
    skip_texture: bool,
    load_full: bool,
}

struct BrowseImageFailure {
    url: String,
    timed_out: bool,
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
}

struct DownloadProgress {
    downloaded: u64,
    total: Option<u64>,
    speed: f64, // bytes/sec
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
        preferred_names: Vec<String>,
        choice: ConflictChoice,
        target_root: PathBuf,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
    },
    SyncImages {
        job_id: u64,
        mod_entry_id: String,
        mod_root_path: PathBuf,
        profile: Box<gamebanana::ProfileResponse>,
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
    SyncImagesDone {
        _job_id: u64,
        mod_entry_id: String,
        profile: Box<gamebanana::ProfileResponse>,
        rel_paths: Vec<String>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TasksTab {
    Downloads,
    Installs,
    Completed,
    Failed,
}

struct CoverRequest {
    game_id: String,
}

struct CoverResult {
    game_id: String,
    image: egui::ColorImage,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThumbnailProfile {
    Card,
    Rail,
}

impl ThumbnailProfile {
    fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Card => (CARD_THUMBNAIL_WIDTH, CARD_THUMBNAIL_HEIGHT),
            Self::Rail => (RAIL_THUMBNAIL_WIDTH, RAIL_THUMBNAIL_HEIGHT),
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Rail => "rail",
        }
    }
}

enum PendingTextureUpload {
    ModThumb { texture_key: String, image: egui::ColorImage },
    ModFull { texture_key: String, image: egui::ColorImage },
    BrowseThumb { texture_key: String, image: egui::ColorImage },
    BrowseFull { texture_key: String, image: egui::ColorImage },
}

impl PendingTextureUpload {
    fn is_thumb(&self) -> bool {
        matches!(
            self,
            PendingTextureUpload::ModThumb { .. } | PendingTextureUpload::BrowseThumb { .. }
        )
    }

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

#[derive(Clone, Copy, PartialEq)]
struct WindowStateSnapshot {
    pos: Option<[f32; 2]>,
    size: Option<[f32; 2]>,
    maximized: bool,
}


