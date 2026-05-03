use std::{collections::HashMap, env, fs, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DISABLED_CONTAINER: &str = "DISABLED_BY_HESTIA";
pub const MOD_META_DIR: &str = "⬢HESTIA";
pub const MOD_META_FILE: &str = "metadata.json";

fn serde_default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub version: u32,
    pub games: Vec<GameInstall>,
    pub library_folders: Vec<LibraryFolder>,
    #[serde(default)]
    pub mods: Vec<ModEntry>,
    #[serde(default)]
    pub tools: Vec<ToolEntry>,
    #[serde(default)]
    pub categories: Vec<ModCategory>,
    pub operations: Vec<OperationLogEntry>,
    #[serde(default)]
    pub tasks: Vec<TaskEntry>,
    pub show_log: bool,
    #[serde(default)]
    pub show_tasks: bool,
    #[serde(default)]
    pub show_tools: bool,
    #[serde(default)]
    pub tasks_layout: TasksLayout,
    #[serde(default)]
    pub tasks_order: TasksOrder,
    #[serde(default)]
    pub last_selected_game_id: Option<String>,
    #[serde(default)]
    pub auto_game_enable_done: bool,
    #[serde(default)]
    pub modded_launcher_path_override: Option<PathBuf>,
    #[serde(default = "serde_default_true")]
    pub use_default_mods_path: bool,
    #[serde(default)]
    pub hide_disabled: bool,
    #[serde(default)]
    pub hide_archived: bool,
    #[serde(default)]
    pub metadata_visibility: MetadataVisibility,
    #[serde(default)]
    pub scan_rabbitfx_requirement: bool,
    #[serde(default)]
    pub launch_behavior: LaunchBehavior,
    #[serde(default)]
    pub tool_launch_behavior: LaunchBehavior,
    #[serde(default)]
    pub after_install_behavior: AfterInstallBehavior,
    #[serde(default)]
    pub unsafe_content_mode: UnsafeContentMode,
    #[serde(default)]
    pub cache_size_tier: CacheSizeTier,
    #[serde(default)]
    pub import_resolution: ImportResolution,
    #[serde(default)]
    pub delete_behavior: DeleteBehavior,
    #[serde(default)]
    pub window_pos: Option<[f32; 2]>,
    #[serde(default)]
    pub window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub window_maximized: bool,
    #[serde(default)]
    pub browse_sort: BrowseSort,
    #[serde(default)]
    pub search_sort: SearchSort,
    #[serde(default)]
    pub library_sort: LibrarySort,
    #[serde(default)]
    pub library_group_mode: LibraryGroupMode,
    #[serde(default = "serde_default_true")]
    pub library_sort_status_first: bool,
    #[serde(default = "serde_default_true")]
    pub library_status_group_show_category: bool,
    #[serde(default = "serde_default_true")]
    pub library_category_group_show_status: bool,
    #[serde(default)]
    pub library_sort_category_first: bool,
    #[serde(default)]
    pub library_uncategorized_first: bool,
    #[serde(default)]
    pub update_check_statuses: ModStatusTargets,
    #[serde(default)]
    pub auto_update_statuses: ModStatusTargets,
    #[serde(default)]
    pub modified_update_behavior: ModifiedUpdateBehavior,
    #[serde(default = "serde_default_true")]
    pub always_replace_on_update: bool,
    #[serde(default = "serde_default_true")]
    pub automatically_check_for_update: bool,
    #[serde(default)]
    pub staged_app_update: Option<StagedAppUpdate>,
    #[serde(default)]
    pub tool_blacklist: HashMap<String, Vec<String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 7,
            games: seeded_games(),
            library_folders: Vec::new(),
            mods: Vec::new(),
            tools: Vec::new(),
            categories: Vec::new(),
            operations: Vec::new(),
            tasks: Vec::new(),
            show_log: false,
            show_tasks: false,
            show_tools: false,
            tasks_layout: TasksLayout::SingleList,
            tasks_order: TasksOrder::OldestFirst,
            last_selected_game_id: None,
            auto_game_enable_done: false,
            modded_launcher_path_override: None,
            use_default_mods_path: true,
            hide_disabled: false,
            hide_archived: false,
            metadata_visibility: MetadataVisibility::default(),
            scan_rabbitfx_requirement: false,
            launch_behavior: LaunchBehavior::default(),
            tool_launch_behavior: LaunchBehavior::default(),
            after_install_behavior: AfterInstallBehavior::default(),
            unsafe_content_mode: UnsafeContentMode::default(),
            cache_size_tier: CacheSizeTier::default(),
            import_resolution: ImportResolution::default(),
            delete_behavior: DeleteBehavior::default(),
            window_pos: None,
            window_size: None,
            window_maximized: false,
            browse_sort: BrowseSort::default(),
            search_sort: SearchSort::default(),
            library_sort: LibrarySort::default(),
            library_group_mode: LibraryGroupMode::default(),
            library_sort_status_first: true,
            library_status_group_show_category: true,
            library_category_group_show_status: true,
            library_sort_category_first: false,
            library_uncategorized_first: false,
            update_check_statuses: ModStatusTargets::default(),
            auto_update_statuses: ModStatusTargets::default(),
            modified_update_behavior: ModifiedUpdateBehavior::default(),
            always_replace_on_update: true,
            automatically_check_for_update: true,
            staged_app_update: None,
            tool_blacklist: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedAppUpdate {
    pub version: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModCategory {
    pub id: String,
    pub game_id: String,
    pub name: String,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModStatusTargets {
    pub active: bool,
    pub disabled: bool,
    pub archived: bool,
}

impl Default for ModStatusTargets {
    fn default() -> Self {
        Self {
            active: true,
            disabled: false,
            archived: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ModifiedUpdateBehavior {
    Yes,
    #[default]
    ShowButton,
    HideButton,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDefinition {
    pub id: String,
    pub name: String,
    pub xxmi_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstall {
    pub definition: GameDefinition,
    pub mods_path_override: Option<PathBuf>,
    #[serde(default)]
    pub modded_exe_path_override: Option<PathBuf>,
    #[serde(default)]
    pub vanilla_exe_path_override: Option<PathBuf>,
    pub enabled: bool,
}

impl GameInstall {
    pub fn mods_path(&self, use_default: bool) -> Option<PathBuf> {
        if use_default {
            default_mods_path(&self.definition.xxmi_code)
        } else {
            self.mods_path_override.clone()
        }
    }

    pub fn modded_exe_path(&self) -> Option<PathBuf> {
        self.modded_exe_path_override.clone()
    }

    pub fn vanilla_exe_path(&self) -> Option<PathBuf> {
        self.vanilla_exe_path_override.clone()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MetadataVisibility {
    Never,
    #[default]
    OnlyIfNoDescription,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModStatus {
    Active,
    Disabled,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractedMetadata {
    pub description: Option<String>,
    pub hotkeys: Vec<String>,
    pub discovered_executables: Vec<String>,
    pub readme_path: Option<String>,
    #[serde(default)]
    pub text_sources: Vec<ExtractedMetadataTextSource>,
    #[serde(default)]
    pub requires_rabbitfx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractedMetadataTextSource {
    pub path: String,
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub hotkeys: Vec<String>,
    pub notes: String,
    pub folder_path: String,
    pub cover_image: Option<String>,
    pub screenshots: Vec<String>,
    #[serde(default)]
    pub extracted_metadata_source_path: Option<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub card_thumb_source_kind: Option<String>,
    #[serde(default)]
    pub card_thumb_source_id: Option<String>,
    #[serde(default)]
    pub card_thumb_source_mtime: Option<i64>,
    #[serde(default)]
    pub card_thumb_source_size: Option<u64>,
    #[serde(default)]
    pub card_thumb_generated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rail_thumb_source_kind: Option<String>,
    #[serde(default)]
    pub rail_thumb_source_id: Option<String>,
    #[serde(default)]
    pub rail_thumb_source_mtime: Option<i64>,
    #[serde(default)]
    pub rail_thumb_source_size: Option<u64>,
    #[serde(default)]
    pub rail_thumb_generated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModMetadata {
    pub extracted: ExtractedMetadata,
    pub user: UserMetadata,
    pub prompt_for_missing_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    pub game_id: String,
    pub folder_name: String,
    pub root_path: PathBuf,
    pub status: ModStatus,
    pub metadata: ModMetadata,
    pub discovered_tools: Vec<DiscoveredTool>,
    #[serde(default)]
    pub archive_original_path: Option<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub content_mtime: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ini_hash: Option<String>,
    #[serde(default)]
    pub unsafe_content: bool,
    #[serde(default)]
    pub source: Option<ModSourceData>,
    #[serde(default)]
    pub update_state: ModUpdateState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableModState {
    pub id: String,
    pub metadata: ModMetadata,
    #[serde(default)]
    pub source: Option<ModSourceData>,
    #[serde(default)]
    pub unsafe_content: bool,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ModUpdateState {
    #[default]
    Unlinked,
    UpToDate,
    UpdateAvailable,
    CheckSkipped,
    MissingSource,
    ModifiedLocally,
    IgnoringUpdateOnce,
    IgnoringUpdateAlways,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BrowseSort {
    #[default]
    Popular,
    RecentUpdated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SearchSort {
    #[default]
    BestMatch,
    RecentUpdated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LibrarySort {
    #[default]
    NameAsc,
    NameDesc,
    DateDesc,
    DateAsc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LibraryGroupMode {
    #[default]
    Category,
    Status,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModSourceData {
    pub gamebanana: Option<GameBananaLink>,
    pub snapshot: Option<GameBananaSnapshot>,
    pub raw_profile_json: Option<String>,
    pub file_set: FileSetRecipe,
    pub prefs: UpdatePrefs,
    #[serde(default)]
    pub ignored_update_signature: Option<IgnoredUpdateSignature>,
    #[serde(default)]
    pub ignore_update_always: bool,
    pub history: InstallHistory,
    pub baseline_content_mtime: Option<DateTime<Utc>>,
    pub baseline_ini_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameBananaLink {
    pub mod_id: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameBananaSnapshot {
    pub title: String,
    pub authors: Vec<String>,
    pub version: Option<String>,
    pub publish_ts: Option<i64>,
    pub update_ts: Option<i64>,
    pub description: Option<String>,
    pub preview_urls: Vec<String>,
    pub files: Vec<GameBananaFileMeta>,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub is_trashed: bool,
    #[serde(default)]
    pub is_withheld: bool,
    #[serde(default)]
    pub unsafe_content: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameBananaFileMeta {
    pub file_id: u64,
    pub file_name: String,
    pub file_size: u64,
    pub date_added: i64,
    pub download_count: u64,
    pub description: Option<String>,
    pub download_url: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileSetRecipe {
    #[serde(default)]
    pub selected_file_ids: Vec<u64>,
    #[serde(default)]
    pub selected_file_names: Vec<String>,
    #[serde(default)]
    pub selected_files_meta: Vec<TrackedFileMeta>,
    #[serde(default)]
    pub selected_candidate_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TrackedFileMeta {
    pub file_id: u64,
    pub file_name: String,
    pub date_added: i64,
    pub version: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IgnoredUpdateSignature {
    #[serde(default)]
    pub files: Vec<TrackedFileMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_update_ts: Option<i64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub prearmed_next_update: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdatePrefs {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstallHistory {
    pub downloaded_at: Option<DateTime<Utc>>,
    pub installed_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryFolder {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub id: String,
    pub game_id: String,
    pub label: String,
    pub path: PathBuf,
    #[serde(default)]
    pub launch_args: String,
    #[serde(default)]
    pub source_mod_id: Option<String>,
    #[serde(default)]
    pub auto_detected: bool,
    #[serde(default)]
    pub show_in_titlebar: bool,
    #[serde(default)]
    pub window_order: i32,
    #[serde(default)]
    pub titlebar_order: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredTool {
    pub label: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct ImportCandidate {
    pub label: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum ImportSource {
    Folder(PathBuf),
    Archive(PathBuf),
}

#[derive(Debug, Clone)]
pub struct ImportInspection {
    pub game_id: String,
    pub candidates: Vec<ImportCandidate>,
    #[allow(dead_code)]
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Replace,
    Merge,
    KeepBoth,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchBehavior {
    DoNothing,
    Minimize,
    Exit,
}

impl Default for LaunchBehavior {
    fn default() -> Self {
        Self::DoNothing
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AfterInstallBehavior {
    DoNothing,
    AddToSelection,
    OpenModDetail,
}

impl Default for AfterInstallBehavior {
    fn default() -> Self {
        Self::DoNothing
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsafeContentMode {
    HideNoCounter,
    #[serde(alias = "Hide")]
    HideShowCounter,
    Censor,
    Show,
}

impl Default for UnsafeContentMode {
    fn default() -> Self {
        Self::HideShowCounter
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheSizeTier {
    Gb2,
    Gb4,
    Gb8,
    Gb16,
}

impl CacheSizeTier {
    pub fn bytes(self) -> u64 {
        match self {
            Self::Gb2 => 2 * 1024 * 1024 * 1024,
            Self::Gb4 => 4 * 1024 * 1024 * 1024,
            Self::Gb8 => 8 * 1024 * 1024 * 1024,
            Self::Gb16 => 16 * 1024 * 1024 * 1024,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gb2 => "2 GB",
            Self::Gb4 => "4 GB",
            Self::Gb8 => "8 GB",
            Self::Gb16 => "16 GB",
        }
    }
}

impl Default for CacheSizeTier {
    fn default() -> Self {
        Self::Gb4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TasksLayout {
    Sections,
    Tabbed,
    SingleList,
}

impl Default for TasksLayout {
    fn default() -> Self {
        Self::Sections
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TasksOrder {
    OldestFirst,
    NewestFirst,
}

impl Default for TasksOrder {
    fn default() -> Self {
        Self::OldestFirst
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskKind {
    Install,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Queued,
    Installing,
    Downloading,
    Canceling,
    Completed,
    Failed,
    Canceled,
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub id: u64,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub title: String,
    pub game_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub total_size: Option<u64>,
    #[serde(default)]
    pub unsafe_content: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportResolution {
    Ask,
    Replace,
    Merge,
    KeepBoth,
}

impl Default for ImportResolution {
    fn default() -> Self {
        Self::Ask
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteBehavior {
    RecycleBin,
    Permanent,
}

impl Default for DeleteBehavior {
    fn default() -> Self {
        Self::RecycleBin
    }
}

pub fn default_mods_path(xxmi_code: &str) -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("XXMI Launcher")
            .join(xxmi_code)
            .join("Mods"),
    )
}

pub fn default_modded_exe_candidates(_game_id: &str) -> Vec<PathBuf> {
    let roots = common_roots();
    build_candidates(&roots, xxmi_launcher_rels())
}

pub fn registry_modded_exe_candidates() -> Vec<PathBuf> {
    let roots = registry_game_install_roots("xxmi-launcher");
    build_candidates(&roots, xxmi_launcher_rels())
}

pub fn shortcut_modded_exe_candidates() -> Vec<PathBuf> {
    xxmi_shortcut_paths()
        .into_iter()
        .filter_map(|path| resolve_shortcut_target(&path))
        .filter(|path| path.is_file())
        .collect()
}

fn xxmi_launcher_rels() -> &'static [&'static str] {
    &[
        "Resources\\Bin\\XXMI Launcher.exe",
        "Resources\\Bin\\XXMI-Launcher.exe",
        "XXMI Launcher.exe",
        "XXMI-Launcher.exe",
        "XXMI Launcher\\Resources\\Bin\\XXMI Launcher.exe",
        "XXMI Launcher\\Resources\\Bin\\XXMI-Launcher.exe",
        "XXMI Launcher\\XXMI Launcher.exe",
        "XXMI Launcher\\XXMI-Launcher.exe",
    ]
}

pub fn default_vanilla_exe_candidates(game_id: &str) -> Vec<PathBuf> {
    let roots = common_roots();
    build_candidates(&roots, vanilla_exe_rels(game_id))
}

pub fn registry_vanilla_exe_candidates(game_id: &str) -> Vec<PathBuf> {
    let roots = registry_game_install_roots(game_id);
    build_candidates(&roots, vanilla_exe_rels(game_id))
}

fn vanilla_exe_rels(game_id: &str) -> &'static [&'static str] {
    match game_id {
        "wuwa" => &[
            "Steam\\steamapps\\common\\Wuthering Waves\\Wuthering Waves.exe",
            "Wuthering Waves.exe",
            "Wuthering Waves\\Wuthering Waves.exe",
            "Wuthering Waves Game\\WutheringWaves.exe",
            "Wuthering Waves Game\\Wuthering Waves.exe",
            "Wuthering Waves\\Wuthering Waves Game\\WutheringWaves.exe",
            "Wuthering Waves\\Wuthering Waves Game\\Wuthering Waves.exe",
            "Client\\Binaries\\Win64\\WutheringWaves.exe",
            "Wuthering Waves\\Client\\Binaries\\Win64\\WutheringWaves.exe",
        ],
        "zzz" => &[
            "ZenlessZoneZero.exe",
            "Zenless Zone Zero Game\\ZenlessZoneZero.exe",
            "ZenlessZoneZero Game\\ZenlessZoneZero.exe",
            "HoYoPlay\\games\\Zenless Zone Zero\\Zenless Zone Zero Game\\ZenlessZoneZero.exe",
            "HoYoPlay\\games\\Zenless Zone Zero\\ZenlessZoneZero Game\\ZenlessZoneZero.exe",
            "Zenless Zone Zero\\Zenless Zone Zero Game\\ZenlessZoneZero.exe",
            "Zenless Zone Zero\\ZenlessZoneZero Game\\ZenlessZoneZero.exe",
            "Zenless Zone Zero\\ZenlessZoneZero.exe",
            "ZenlessZoneZero\\ZenlessZoneZero Game\\ZenlessZoneZero.exe",
            "ZenlessZoneZero\\ZenlessZoneZero.exe",
        ],
        "endfield" => &[
            "Endfield.exe",
            "EndField Game\\Endfield.exe",
            "Arknights Endfield Game\\Endfield.exe",
            "GRYPHLINK\\games\\EndField Game\\Endfield.exe",
            "Arknights Endfield\\Arknights Endfield Game\\Endfield.exe",
            "Arknights Endfield\\ArknightsEndfield.exe",
        ],
        "starrail" => &[
            "StarRail.exe",
            "Games\\StarRail.exe",
            "Star Rail Games\\StarRail.exe",
            "HoYoPlay\\games\\Honkai Star Rail\\Games\\StarRail.exe",
            "HoYoPlay\\games\\Honkai Star Rail\\Star Rail Games\\StarRail.exe",
            "Honkai Star Rail\\Games\\StarRail.exe",
            "Honkai Star Rail\\Star Rail Games\\StarRail.exe",
            "Honkai Star Rail\\StarRail.exe",
            "Star Rail\\Games\\StarRail.exe",
            "Star Rail\\Star Rail Games\\StarRail.exe",
            "Star Rail\\StarRail.exe",
        ],
        "genshin" => &[
            "GenshinImpact.exe",
            "YuanShen.exe",
            "Genshin Impact Game\\GenshinImpact.exe",
            "Genshin Impact Game\\YuanShen.exe",
            "Genshin Impact\\Genshin Impact Game\\GenshinImpact.exe",
            "Genshin Impact\\Genshin Impact Game\\YuanShen.exe",
            "HoYoPlay\\games\\Genshin Impact\\Genshin Impact Game\\GenshinImpact.exe",
            "HoYoPlay\\games\\Genshin Impact\\Genshin Impact Game\\YuanShen.exe",
            "HoYo\\games\\Genshin Impact\\Genshin Impact Game\\GenshinImpact.exe",
            "miHoYo\\Genshin Impact\\Genshin Impact Game\\GenshinImpact.exe",
        ],
        "honkai-impact" => &[
            "BH3.exe",
            "HonkaiImpact3.exe",
            "Games\\BH3.exe",
            "Games\\HonkaiImpact3.exe",
            "Honkai Impact 3rd game\\BH3.exe",
            "Honkai Impact 3rd game\\HonkaiImpact3.exe",
            "HoYoPlay\\games\\Honkai Impact 3rd\\Games\\BH3.exe",
            "HoYoPlay\\games\\Honkai Impact 3rd\\Honkai Impact 3rd game\\BH3.exe",
            "Honkai Impact 3rd\\Games\\BH3.exe",
            "Honkai Impact 3rd\\Honkai Impact 3rd game\\BH3.exe",
            "Honkai Impact 3rd\\BH3.exe",
            "Honkai Impact 3rd\\Games\\HonkaiImpact3.exe",
            "Honkai Impact 3rd\\Honkai Impact 3rd game\\HonkaiImpact3.exe",
            "Honkai Impact 3rd\\HonkaiImpact3.exe",
        ],
        _ => &[],
    }
}

fn common_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in [
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "LOCALAPPDATA",
        "APPDATA",
    ] {
        if let Some(value) = env::var_os(key) {
            roots.push(PathBuf::from(value));
        }
    }
    roots.extend(steam_library_common_roots());
    roots.extend(epic_install_roots());
    roots.push(PathBuf::from("C:\\Games"));
    roots.push(PathBuf::from("D:\\Games"));
    roots.sort();
    roots.dedup();
    roots
}

fn steam_library_common_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for steam_root in steam_install_roots() {
        let library_file = steam_root.join("steamapps").join("libraryfolders.vdf");
        let Ok(raw) = fs::read_to_string(&library_file) else {
            continue;
        };
        for library_root in parse_steam_library_roots(&raw) {
            let common = library_root.join("steamapps").join("common");
            if common.is_dir() {
                roots.push(common);
            }
        }
    }
    roots
}

fn steam_install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in ["PROGRAMFILES(X86)", "PROGRAMFILES"] {
        if let Some(value) = env::var_os(key) {
            roots.push(PathBuf::from(value).join("Steam"));
        }
    }
    roots
}

fn parse_steam_library_roots(raw: &str) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("\"path\"") {
            continue;
        }
        let value = trimmed.trim_start_matches("\"path\"").trim_start();
        let Some(value) = value.strip_prefix('"') else {
            continue;
        };
        let Some(end) = value.find('"') else {
            continue;
        };
        let root = PathBuf::from(value[..end].replace("\\\\", "\\"));
        if root.is_dir() {
            roots.push(root);
        }
    }
    roots
}

fn epic_install_roots() -> Vec<PathBuf> {
    let program_data = env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\ProgramData"));
    let mut roots = Vec::new();

    let manifest_dir = program_data
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");
    if let Ok(entries) = fs::read_dir(manifest_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("item") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            if let Some(install_location) = value
                .get("InstallLocation")
                .and_then(|value| value.as_str())
            {
                push_install_root(&mut roots, PathBuf::from(install_location));
            }
        }
    }

    let launcher_installed = program_data
        .join("Epic")
        .join("UnrealEngineLauncher")
        .join("LauncherInstalled.dat");
    if let Ok(raw) = fs::read_to_string(launcher_installed) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(installs) = value
                .get("InstallationList")
                .and_then(|value| value.as_array())
            {
                for install in installs {
                    if let Some(install_location) = install
                        .get("InstallLocation")
                        .and_then(|value| value.as_str())
                    {
                        push_install_root(&mut roots, PathBuf::from(install_location));
                    }
                }
            }
        }
    }

    roots.sort();
    roots.dedup();
    roots
}

fn push_install_root(roots: &mut Vec<PathBuf>, root: PathBuf) {
    if !root.is_dir() {
        return;
    }
    if let Some(parent) = root.parent().filter(|parent| parent.is_dir()) {
        roots.push(parent.to_path_buf());
    }
    roots.push(root);
}

fn xxmi_shortcut_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(appdata) = env::var_os("APPDATA") {
        let appdata = PathBuf::from(appdata);
        paths.push(appdata.join("XXMI Launcher").join("XXMI Launcher.lnk"));
        paths.push(
            appdata
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("XXMI Launcher.lnk"),
        );
        paths.push(
            appdata
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("XXMI Launcher")
                .join("XXMI Launcher.lnk"),
        );
    }
    if let Some(programdata) = env::var_os("PROGRAMDATA") {
        paths.push(
            PathBuf::from(programdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("XXMI Launcher.lnk"),
        );
    }
    if let Some(profile) = env::var_os("USERPROFILE") {
        paths.push(
            PathBuf::from(profile)
                .join("Desktop")
                .join("XXMI Launcher.lnk"),
        );
    }
    paths.push(PathBuf::from(
        "C:\\Users\\Public\\Desktop\\XXMI Launcher.lnk",
    ));
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(windows)]
fn resolve_shortcut_target(path: &PathBuf) -> Option<PathBuf> {
    use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_UNCPRIORITY, ShellLink};
    use windows::core::{Interface, PCWSTR};

    struct ComApartment(bool);
    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    fn wide_null(value: &PathBuf) -> Vec<u16> {
        value
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    #[cfg(windows)]
    use std::os::windows::ffi::OsStrExt;

    if !path.is_file() {
        return None;
    }

    let com_initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().is_ok() };
    let _apartment = ComApartment(com_initialized);
    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()? };
    let persist_file: IPersistFile = shell_link.cast().ok()?;
    let shortcut_path = wide_null(path);
    unsafe {
        persist_file
            .Load(PCWSTR(shortcut_path.as_ptr()), STGM_READ)
            .ok()?;
    }

    let mut target = [0u16; 32768];
    let mut find_data = WIN32_FIND_DATAW::default();
    unsafe {
        shell_link
            .GetPath(&mut target, &mut find_data, SLGP_UNCPRIORITY.0 as u32)
            .ok()?;
    }
    let end = target
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(target.len());
    if end == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&target[..end])))
}

#[cfg(not(windows))]
fn resolve_shortcut_target(_path: &PathBuf) -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn registry_game_install_roots(game_id: &str) -> Vec<PathBuf> {
    use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        REG_EXPAND_SZ, REG_SAM_FLAGS, REG_SZ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
        RegQueryValueExW,
    };
    use windows::core::{PCWSTR, PWSTR};

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn registry_string_value(key: HKEY, name: &str) -> Option<String> {
        let name = wide_null(name);
        let mut value_type = Default::default();
        let mut byte_len = 0u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut value_type),
                None,
                Some(&mut byte_len),
            )
        };
        if status != ERROR_SUCCESS || byte_len == 0 {
            return None;
        }
        if value_type != REG_SZ && value_type != REG_EXPAND_SZ {
            return None;
        }

        let mut bytes = vec![0u8; byte_len as usize];
        let status = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut value_type),
                Some(bytes.as_mut_ptr()),
                Some(&mut byte_len),
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }

        bytes.truncate(byte_len as usize);
        let chars: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .take_while(|ch| *ch != 0)
            .collect();
        let value = String::from_utf16_lossy(&chars);
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    fn display_icon_root(value: &str) -> Option<PathBuf> {
        let value = value.trim();
        let executable = if let Some(rest) = value.strip_prefix('"') {
            let end = rest.find('"')?;
            &rest[..end]
        } else {
            value.split(',').next().unwrap_or(value).trim()
        };
        let path = PathBuf::from(executable);
        path.parent()
            .filter(|parent| parent.is_dir())
            .map(|parent| parent.to_path_buf())
    }

    fn display_name_matches(display_name: &str, needles: &[&str]) -> bool {
        let display_name = display_name.to_ascii_lowercase();
        needles.iter().any(|needle| display_name.contains(needle))
    }

    fn collect_from_uninstall_key(
        roots: &mut Vec<PathBuf>,
        hive: HKEY,
        view: REG_SAM_FLAGS,
        needles: &[&str],
    ) {
        let uninstall_key = wide_null("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall");
        let mut key = HKEY::default();
        let status = unsafe {
            RegOpenKeyExW(
                hive,
                PCWSTR(uninstall_key.as_ptr()),
                Some(0),
                KEY_READ | view,
                &mut key,
            )
        };
        if status != ERROR_SUCCESS {
            return;
        }

        let mut index = 0u32;
        loop {
            let mut name = [0u16; 256];
            let mut name_len = name.len() as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    key,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            index += 1;
            if status != ERROR_SUCCESS {
                continue;
            }

            let mut subkey = HKEY::default();
            let status = unsafe {
                RegOpenKeyExW(
                    key,
                    PCWSTR(name.as_ptr()),
                    Some(0),
                    KEY_READ | view,
                    &mut subkey,
                )
            };
            if status != ERROR_SUCCESS {
                continue;
            }

            let display_name = registry_string_value(subkey, "DisplayName");
            if display_name
                .as_deref()
                .is_some_and(|name| display_name_matches(name, needles))
            {
                if let Some(install_location) = registry_string_value(subkey, "InstallLocation") {
                    push_install_root(roots, PathBuf::from(install_location));
                }
                if let Some(display_icon) = registry_string_value(subkey, "DisplayIcon") {
                    if let Some(root) = display_icon_root(&display_icon) {
                        push_install_root(roots, root);
                    }
                }
            }
            unsafe {
                let _ = RegCloseKey(subkey);
            }
        }
        unsafe {
            let _ = RegCloseKey(key);
        }
    }

    let needles: &[&str] = match game_id {
        "wuwa" => &["wuthering waves"],
        "zzz" => &["zenless zone zero", "zenlesszonezero"],
        "endfield" => &["arknights endfield", "arknights: endfield", "endfield"],
        "starrail" => &["honkai: star rail", "honkai star rail", "star rail"],
        "genshin" => &["genshin impact"],
        "honkai-impact" => &["honkai impact 3rd", "honkai impact"],
        "xxmi-launcher" => &["xxmi launcher"],
        _ => &[],
    };
    if needles.is_empty() {
        return Vec::new();
    }

    let mut roots = Vec::new();
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            collect_from_uninstall_key(&mut roots, hive, view, needles);
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(not(windows))]
fn registry_game_install_roots(_game_id: &str) -> Vec<PathBuf> {
    Vec::new()
}

fn build_candidates(roots: &[PathBuf], rels: &[&str]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in roots {
        for rel in rels {
            paths.push(root.join(rel));
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

pub fn seeded_games() -> Vec<GameInstall> {
    [
        ("wuwa", "Wuthering Waves", "WWMI"),
        ("zzz", "Zenless Zone Zero", "ZZMI"),
        ("endfield", "Arknights Endfield", "EFMI"),
        ("starrail", "Honkai Star Rail", "SRMI"),
        ("genshin", "Genshin Impact", "GIMI"),
        ("honkai-impact", "Honkai Impact", "HIMI"),
    ]
    .into_iter()
    .map(|(id, name, xxmi_code)| GameInstall {
        definition: GameDefinition {
            id: id.to_string(),
            name: name.to_string(),
            xxmi_code: xxmi_code.to_string(),
        },
        mods_path_override: None,
        modded_exe_path_override: None,
        vanilla_exe_path_override: None,
        enabled: true,
    })
    .collect()
}
