// Font and shared icon resources.
const LUCIDE_FAMILY: &str = "lucide";

// Main workspace and reusable card layout sizing.
const CARD_WIDTH: f32 = 220.0;
const WORKSPACE_LEFT_PANE_RATIO: f32 = 0.515;

// Outer chrome spacing and major titlebar/game art sizing.
const WINDOW_INSET: i8 = 6;
const GAME_ICON_TEXTURE_SIZE: u32 = 256;
const TOOL_ICON_TEXTURE_SIZE: u32 = 96;
const TITLEBAR_GAME_ICON_SIZE: f32 = 96.0;
const GAME_SWITCHER_GRID_ICON_SIZE: f32 = 256.0;
const COVER_RIGHT_EXTEND: f32 = 2.0;  // Adjust this to extend cover to right edge
const COVER_BOTTOM_EXTEND: f32 = 2.0; // Adjust this to extend cover to bottom edge

// Toast notification limits, timing, and positioning.
const TOAST_LIMIT: usize = 5;
const TOAST_DURATION: f64 = 4.0;
const TOAST_SPACING: f32 = 6.0;
const TOAST_OFFSET: f32 = -108.0;
const TOAST_MAX_WIDTH: f32 = CARD_WIDTH * 3.0;

// Settings and background worker refresh intervals.
const SETTINGS_USAGE_REFRESH_SECS: f64 = 5.0;

// Worker pool and concurrency limits for install, network JSON, images, and texture uploads.
const INSTALL_WORKER_COUNT: usize = 6;
const FULL_IMAGE_LIMIT: usize = 6;
const THUMB_IMAGE_LIMIT: usize = 12;
const BROWSE_IMAGE_RETRY_COOLDOWN_SECS: u64 = 15;
const THUMBNAIL_BYTE_CACHE_CAPACITY: usize = 256;
const LOCAL_MOD_IMAGE_WORKERS: usize = 8; 
const JSON_LIMIT: usize = 6;
const TEXTURE_UPLOADS_PER_FRAME: usize = 32;
const LOCAL_IMAGE_DISPATCH_BATCH: usize = 64; 
const FULL_IMAGE_DECODE_LIMIT: usize = 3;
const TEXTURE_RAM_BUDGET_MIN_BYTES: u64 = 1024 * 1024 * 1024;
const TEXTURE_RAM_BUDGET_MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

// App identity and stable internal IDs.
const APP_NAME: &str = "Hestia";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const APP_ICON_ID: &str = "__app__";

// Persistent egui widget/window IDs used for focus, popups, and window state.
const BROWSE_DETAIL_WINDOW_ID: &str = "browse_detail_window";
const BROWSE_FILE_PICKER_WINDOW_ID: &str = "browse_file_picker_window";
const MODS_SEARCH_INPUT_ID: &str = "mods_search_input";
const BROWSE_SEARCH_INPUT_ID: &str = "browse_search_input";
const MOD_DETAIL_RENAME_INPUT_ID: &str = "mod_detail_rename_input";

// Browse view card/detail layout sizing.
const BROWSE_PANEL_CARD_WIDTH: f32 = 220.0;
const BROWSE_THUMBNAIL_HEIGHT: f32 = 130.0;
const BROWSE_DETAIL_SIZE: egui::Vec2 = egui::vec2(420.0, 560.0);

// Batch action bar layout.
const MAX_OPERATIONAL_BUTTONS_PER_ROW: usize = 6; // exceeding will go to next row
const MAX_OPERATIONAL_BUTTONS_PER_ROW_WITH_SEARCHBAR: usize = 3;

// Cached thumbnail output sizes by destination surface.
const CARD_THUMBNAIL_WIDTH: u32 = 220;
const CARD_THUMBNAIL_HEIGHT: u32 = 130;
const RAIL_THUMBNAIL_WIDTH: u32 = 585;
const RAIL_THUMBNAIL_HEIGHT: u32 = 330;

// App worker channel aliases.
type WorkerTx<T> = tokio_mpsc::UnboundedSender<T>;
type WorkerRx<T> = tokio_mpsc::UnboundedReceiver<T>;

// Markdown image extraction for GameBanana descriptions.
static MARKDOWN_IMAGE_DEST_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches ![alt](dest) or ![alt](<dest>) handling nested parentheses and spaces
    Regex::new(r"!\[([^\]]*)\]\(\s*(<[^>]+>|[^\s)]+)\s*\)").unwrap()
});
