use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use url::{Host, Url};
use uuid::Uuid;

pub const DISABLED_CONTAINER: &str = "DISABLED_BY_HESTIA";
pub const UNREAL_DISABLED_MODS_DIR: &str = "~mods-disabledByHestia";
pub const MOD_META_DIR: &str = "⬢HESTIA";
pub const MOD_META_FILE: &str = "metadata.json";
pub const PERSONAL_NOTE_FILE: &str = "Personal Note.txt";
pub const MODS_PROFILES_DIR: &str = "Mods_Profiles";

fn serde_default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadHotkeyTrigger {
    EnablingMods,
    DisablingMods,
    InstallingMods,
    DeletingMods,
    UpdatingMods,
    RenamingMods,
    ArchivingMods,
    RestoringMods,
    CustomizingMods,
    ProfileSwitch,
}

/// Operations that should ask XXMI to reload while the game is already running.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadHotkeyTriggers {
    #[serde(default = "serde_default_true")]
    pub enabling_mods: bool,
    #[serde(default = "serde_default_true")]
    pub disabling_mods: bool,
    #[serde(default = "serde_default_true")]
    pub installing_mods: bool,
    #[serde(default = "serde_default_true")]
    pub deleting_mods: bool,
    #[serde(default = "serde_default_true")]
    pub updating_mods: bool,
    #[serde(default = "serde_default_true")]
    pub renaming_mods: bool,
    #[serde(default = "serde_default_true")]
    pub archiving_mods: bool,
    #[serde(default = "serde_default_true")]
    pub restoring_mods: bool,
    #[serde(default = "serde_default_true")]
    pub customizing_mods: bool,
    #[serde(default = "serde_default_true")]
    pub profile_switch: bool,
}

impl Default for ReloadHotkeyTriggers {
    fn default() -> Self {
        Self {
            enabling_mods: true,
            disabling_mods: true,
            installing_mods: true,
            deleting_mods: true,
            updating_mods: true,
            renaming_mods: true,
            archiving_mods: true,
            restoring_mods: true,
            customizing_mods: true,
            profile_switch: true,
        }
    }
}

impl ReloadHotkeyTriggers {
    pub fn enabled(&self, trigger: ReloadHotkeyTrigger) -> bool {
        match trigger {
            ReloadHotkeyTrigger::EnablingMods => self.enabling_mods,
            ReloadHotkeyTrigger::DisablingMods => self.disabling_mods,
            ReloadHotkeyTrigger::InstallingMods => self.installing_mods,
            ReloadHotkeyTrigger::DeletingMods => self.deleting_mods,
            ReloadHotkeyTrigger::UpdatingMods => self.updating_mods,
            ReloadHotkeyTrigger::RenamingMods => self.renaming_mods,
            ReloadHotkeyTrigger::ArchivingMods => self.archiving_mods,
            ReloadHotkeyTrigger::RestoringMods => self.restoring_mods,
            ReloadHotkeyTrigger::CustomizingMods => self.customizing_mods,
            ReloadHotkeyTrigger::ProfileSwitch => self.profile_switch,
        }
    }
}

/// Static preferences that rarely change during runtime.
/// Deserializing these separately reduces overhead when loading AppState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticPreferences {
    #[serde(default)]
    pub modded_launcher_path_override: Option<PathBuf>,
    #[serde(default = "serde_default_true")]
    pub use_default_mods_path: bool,
    #[serde(default)]
    pub hide_disabled: bool,
    #[serde(default)]
    pub hide_archived: bool,
    /// Inline Hotkeys view mode: false = Raw (per-file sections), true = List
    /// (simplified). Defaults to List.
    #[serde(default = "serde_default_true")]
    pub hotkeys_simplified: bool,
    #[serde(default)]
    pub scan_rabbitfx_requirement: bool,
    #[serde(default)]
    pub font_style: AppFontStyle,
    #[serde(default)]
    pub language: AppLanguage,
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
    pub renderer: RendererPreference,
    #[serde(default)]
    pub import_resolution: ImportResolution,
    #[serde(default)]
    pub delete_behavior: DeleteBehavior,
    /// Preserve in-game XXMI mod settings (3DMigoto persistent variables) across rename,
    /// disable/enable, archive/restore, delete, import-replace, and profile switches.
    #[serde(default = "serde_default_true")]
    pub preserve_mod_settings: bool,
    /// Legacy global default for per-game XXMI reload settings. Kept for migration and
    /// backward-compatible config parsing; live reload enablement is stored on `GameInstall`.
    #[serde(default = "serde_default_true")]
    pub send_reload_hotkey: bool,
    #[serde(default)]
    pub reload_hotkey_triggers: ReloadHotkeyTriggers,
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
    #[serde(default)]
    pub library_category_display_mode: LibraryCategoryDisplayMode,
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
    #[serde(default = "serde_default_true")]
    pub library_show_empty_category_folders: bool,
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
    pub always_translate_mod_details: bool,
    #[serde(default)]
    pub use_custom_proxy: bool,
    #[serde(default)]
    pub custom_proxy_url: String,
    /// Internally selected endpoint for a bare proxy address. This never changes the text the
    /// user entered, but lets the same detected protocol be used after a restart.
    #[serde(default)]
    pub custom_proxy_resolved_url: String,
    #[serde(default)]
    pub tool_blacklist: HashMap<String, Vec<String>>,
}

impl Default for StaticPreferences {
    fn default() -> Self {
        Self {
            modded_launcher_path_override: None,
            use_default_mods_path: true,
            hide_disabled: false,
            hide_archived: false,
            hotkeys_simplified: true,
            scan_rabbitfx_requirement: false,
            font_style: AppFontStyle::default(),
            language: AppLanguage::detect_system_supported().unwrap_or_default(),
            launch_behavior: LaunchBehavior::default(),
            tool_launch_behavior: LaunchBehavior::default(),
            after_install_behavior: AfterInstallBehavior::default(),
            unsafe_content_mode: UnsafeContentMode::default(),
            cache_size_tier: CacheSizeTier::default(),
            renderer: RendererPreference::default(),
            import_resolution: ImportResolution::default(),
            delete_behavior: DeleteBehavior::default(),
            preserve_mod_settings: true,
            send_reload_hotkey: true,
            reload_hotkey_triggers: ReloadHotkeyTriggers::default(),
            window_pos: None,
            window_size: None,
            window_maximized: false,
            browse_sort: BrowseSort::default(),
            search_sort: SearchSort::default(),
            library_sort: LibrarySort::default(),
            library_group_mode: LibraryGroupMode::default(),
            library_category_display_mode: LibraryCategoryDisplayMode::default(),
            library_sort_status_first: true,
            library_status_group_show_category: true,
            library_category_group_show_status: true,
            library_sort_category_first: false,
            library_uncategorized_first: false,
            library_show_empty_category_folders: true,
            update_check_statuses: ModStatusTargets::default(),
            auto_update_statuses: ModStatusTargets::default(),
            modified_update_behavior: ModifiedUpdateBehavior::default(),
            always_replace_on_update: true,
            automatically_check_for_update: true,
            always_translate_mod_details: false,
            use_custom_proxy: false,
            custom_proxy_url: String::new(),
            custom_proxy_resolved_url: String::new(),
            tool_blacklist: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomProxyConfig {
    endpoint: String,
}

impl CustomProxyConfig {
    const AUTO_DETECT_PORTS: [u16; 11] = [
        80, 8080, 8085, 7890, 7891, 3128, 999, 1080, 3129, 5678, 8089,
    ];
    const AUTO_DETECT_SCHEMES: [&str; 6] =
        ["socks5h", "socks5", "socks4a", "socks4", "http", "https"];
    pub fn from_preferences(preferences: &StaticPreferences) -> Result<Option<Self>, String> {
        if !preferences.use_custom_proxy {
            return Ok(None);
        }
        if !preferences.custom_proxy_url.contains("://")
            && !preferences.custom_proxy_resolved_url.trim().is_empty()
        {
            Self::parse(&preferences.custom_proxy_resolved_url).map(Some)
        } else {
            Self::parse(&preferences.custom_proxy_url).map(Some)
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        let (host, scheme, port, explicit_scheme) = Self::parse_template(value)?;
        let scheme = scheme.as_deref().unwrap_or("http");
        let port = port.unwrap_or_else(|| {
            if explicit_scheme {
                Self::default_port_for_scheme(scheme)
            } else {
                80
            }
        });
        Ok(Self {
            endpoint: format!("{scheme}://{host}:{port}"),
        })
    }

    pub fn parse_candidates(value: &str) -> Result<Vec<Self>, String> {
        let (host, scheme, port, _explicit_scheme) = Self::parse_template(value)?;
        let schemes: Vec<String> = match scheme {
            Some(scheme) => vec![scheme],
            None => Self::AUTO_DETECT_SCHEMES
                .iter()
                .map(|scheme| (*scheme).to_string())
                .collect(),
        };
        let ports: Vec<u16> = match port {
            Some(port) => vec![port],
            None => Self::AUTO_DETECT_PORTS.to_vec(),
        };
        let mut candidates = Vec::with_capacity(schemes.len() * ports.len());
        // Port order is user-visible policy. Within each port, keep the DNS-safe SOCKS
        // variants ahead of HTTP(S). An explicitly supplied scheme has one entry per port.
        for port in ports {
            for scheme in &schemes {
                candidates.push(Self {
                    endpoint: format!("{scheme}://{host}:{port}"),
                });
            }
        }
        Ok(candidates)
    }

    fn parse_template(value: &str) -> Result<(String, Option<String>, Option<u16>, bool), String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("proxy address is required".to_string());
        }
        if value.contains('\\') {
            return Err("proxy address must not contain a backslash".to_string());
        }
        let explicit_scheme = value.contains("://");
        let candidate = if explicit_scheme {
            value.to_string()
        } else {
            format!("http://{value}")
        };
        let url = Url::parse(&candidate).map_err(|_| "proxy address is invalid".to_string())?;
        let scheme = url.scheme();
        if !matches!(
            scheme,
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
        ) {
            return Err("proxy protocol is unsupported".to_string());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("proxy credentials are unsupported".to_string());
        }
        if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
            return Err("proxy address must not include a path, query, or fragment".to_string());
        }
        let host = match url.host() {
            Some(Host::Domain(host)) => host.to_string(),
            Some(Host::Ipv4(host)) => host.to_string(),
            Some(Host::Ipv6(host)) => format!("[{host}]"),
            None => return Err("proxy host is required".to_string()),
        };
        Ok((
            host,
            explicit_scheme.then(|| scheme.to_string()),
            url.port(),
            explicit_scheme,
        ))
    }

    fn default_port_for_scheme(scheme: &str) -> u16 {
        match scheme {
            "http" => 80,
            "https" => 443,
            _ => 1080,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub version: u32,
    #[serde(default)]
    pub app_version: String,
    pub games: Vec<GameInstall>,
    pub library_folders: Vec<LibraryFolder>,
    #[serde(default)]
    pub mods: Vec<ModEntry>,
    #[serde(default)]
    pub tools: Vec<ToolEntry>,
    #[serde(default)]
    pub categories: Vec<ModCategory>,
    #[serde(default)]
    pub category_sort_mode_by_game: HashMap<String, ModCategorySortMode>,
    #[serde(default)]
    pub create_downloaded_mod_category_by_game: HashMap<String, bool>,
    pub operations: Vec<OperationLogEntry>,
    #[serde(default)]
    pub tasks: Vec<TaskEntry>,
    pub show_log: bool,
    #[serde(default)]
    pub show_tasks: bool,
    #[serde(default)]
    pub show_tools: bool,
    #[serde(default)]
    pub show_whats_new: bool,
    #[serde(default)]
    pub show_feedback_survey: bool,
    #[serde(default)]
    pub feedback_survey: FeedbackSurveyState,
    #[serde(default = "serde_default_true")]
    pub startup_path_scan_completed: bool,
    #[serde(default)]
    pub tasks_layout: TasksLayout,
    #[serde(default)]
    pub tasks_order: TasksOrder,
    #[serde(default)]
    pub last_selected_game_id: Option<String>,
    #[serde(default)]
    pub auto_game_enable_done: bool,
    #[serde(default)]
    pub staged_app_update: Option<StagedAppUpdate>,
    #[serde(skip)]
    pub preferences_need_save: bool,
    #[serde(default)]
    pub last_update_check_time_by_game: HashMap<String, DateTime<Utc>>,
    /// Profile catalogs are keyed by stable game definition id.  Payloads live in the game's
    /// `Mods_Profiles` directory and are not embedded in the application state.
    #[serde(default)]
    pub profiles_by_game: HashMap<String, ProfileCatalog>,
    // Static preferences inlined for backward compatibility
    #[serde(flatten)]
    pub static_prefs: StaticPreferences,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: 7,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            games: seeded_games(),
            library_folders: Vec::new(),
            mods: Vec::new(),
            tools: Vec::new(),
            categories: Vec::new(),
            category_sort_mode_by_game: HashMap::new(),
            create_downloaded_mod_category_by_game: HashMap::new(),
            operations: Vec::new(),
            tasks: Vec::new(),
            show_log: false,
            show_tasks: false,
            show_tools: false,
            show_whats_new: false,
            show_feedback_survey: false,
            feedback_survey: FeedbackSurveyState::default(),
            startup_path_scan_completed: true,
            tasks_layout: TasksLayout::SingleList,
            tasks_order: TasksOrder::OldestFirst,
            last_selected_game_id: None,
            auto_game_enable_done: false,
            staged_app_update: None,
            preferences_need_save: false,
            last_update_check_time_by_game: HashMap::new(),
            profiles_by_game: HashMap::new(),
            static_prefs: StaticPreferences::default(),
        }
    }
}

/// A profile's identity: 32 random bits, rendered as the 8 hex digits that appear in its storage
/// name.
///
/// Deliberately short rather than a UUID. The value people actually see is the one in
/// `Patch 1.4 [1fe9ec7a]`, and it exists to keep two profiles apart — most importantly two shared
/// profiles that are both called "Default" — so the identity and the visible token are the same
/// thing rather than one being a truncation of the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileId(u32);

impl ProfileId {
    pub fn random() -> Self {
        // Uuid v4 is already a dependency and is a cryptographically-seeded source; take 32 bits.
        Self(uuid::Uuid::new_v4().as_u128() as u32)
    }

    /// A fresh id that no profile in `taken` is using, so ids never collide within one game even
    /// though 32 bits alone would make it merely unlikely.
    pub fn random_unused(taken: &[ProfileId]) -> Self {
        loop {
            let candidate = Self::random();
            if !taken.contains(&candidate) {
                return candidate;
            }
        }
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08x}", self.0)
    }
}

impl std::str::FromStr for ProfileId {
    type Err = ();

    /// Accepts the canonical 8 hex digits, and also a full UUID so profiles written before ids were
    /// shortened keep the identity their storage names already show.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let head: String = value
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(8)
            .collect();
        if head.len() != 8 {
            return Err(());
        }
        u32::from_str_radix(&head, 16).map(Self).map_err(|_| ())
    }
}

impl Serialize for ProfileId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ProfileId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse()
            .map_err(|_| serde::de::Error::custom(format!("not a profile id: {raw}")))
    }
}

#[cfg(test)]
mod profile_id_tests {
    use super::*;

    #[test]
    fn a_uuid_from_older_state_keeps_the_id_its_storage_name_already_shows() {
        // Files were already named from the leading hex, so reading a UUID this way leaves every
        // existing profile pointing at the archive it always pointed at.
        let migrated: ProfileId = "1fe9ec7a-6ad1-46b6-9558-e8483dc62bcf".parse().unwrap();

        assert_eq!(migrated.to_string(), "1fe9ec7a");
        assert_eq!("1fe9ec7a".parse::<ProfileId>().unwrap(), migrated);
        assert_eq!(
            "7a15244d-90cc-45d9-aca0-244405f229b6"
                .parse::<ProfileId>()
                .unwrap()
                .to_string(),
            "7a15244d"
        );
    }

    #[test]
    fn ids_render_as_eight_hex_digits_including_leading_zeroes() {
        assert_eq!(
            "00000abc".parse::<ProfileId>().unwrap().to_string(),
            "00000abc"
        );
        assert_eq!(
            "1FE9EC7A".parse::<ProfileId>().unwrap().to_string(),
            "1fe9ec7a"
        );
        assert!(
            "1fe9ec".parse::<ProfileId>().is_err(),
            "too short to be an id"
        );
        assert!("zzzzzzzz".parse::<ProfileId>().is_err());
    }

    #[test]
    fn a_fresh_id_never_reuses_one_already_taken() {
        let taken: Vec<ProfileId> = (0..64).map(|_| ProfileId::random()).collect();
        let fresh = ProfileId::random_unused(&taken);

        assert!(!taken.contains(&fresh));
    }

    #[test]
    fn ids_round_trip_through_serde_as_their_visible_form() {
        let id: ProfileId = "1fe9ec7a".parse().unwrap();
        let encoded = serde_json::to_string(&id).unwrap();

        assert_eq!(encoded, "\"1fe9ec7a\"");
        assert_eq!(serde_json::from_str::<ProfileId>(&encoded).unwrap(), id);
        // And still reads state written before ids were shortened.
        assert_eq!(
            serde_json::from_str::<ProfileId>("\"1fe9ec7a-6ad1-46b6-9558-e8483dc62bcf\"").unwrap(),
            id
        );
    }
}

/// Persisted profile catalog for one game. UUIDs are independent from display names so a rename
/// never changes the archive identity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileCatalog {
    #[serde(default)]
    pub active_profile_id: Option<ProfileId>,
    #[serde(default)]
    pub profiles: Vec<ProfileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileRecord {
    pub id: ProfileId,
    pub display_name: String,
    #[serde(default)]
    pub archive_size: Option<u64>,
    #[serde(default)]
    pub uncompressed_size: Option<u64>,
    #[serde(default)]
    pub file_count: Option<u64>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub portable_metadata: HashMap<String, serde_json::Value>,
    /// Category definitions captured for this profile. `None` means this record predates
    /// profile-scoped categories and should be migrated from the game's legacy global list.
    #[serde(default)]
    pub categories: Option<Vec<ModCategory>>,
    /// Tools captured for this profile, including their launch options, titlebar pins, and
    /// ordering. Every tool belongs to a profile regardless of where its executable lives, so
    /// switching restores exactly the set that was active last time. `None` means this record
    /// predates profile-scoped tools and should be migrated from the game's legacy global list.
    #[serde(default)]
    pub tools: Option<Vec<ToolEntry>>,
    /// Auto-detected tool paths the user removed in this profile. Profile-scoped so hiding a
    /// tool in one profile never hides a same-pathed tool in another.
    #[serde(default)]
    pub tool_blacklist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct L10n {
    pub en_us: &'static str,
    pub id_id: &'static str,
    pub zh_cn: &'static str,
    pub ru_ru: &'static str,
}

pub(crate) const fn l10n(
    en_us: &'static str,
    id_id: &'static str,
    zh_cn: &'static str,
    ru_ru: &'static str,
) -> L10n {
    L10n {
        en_us,
        id_id,
        zh_cn,
        ru_ru,
    }
}

impl L10n {
    pub(crate) fn get(&self, language: AppLanguage) -> &'static str {
        let localized = match language {
            AppLanguage::Indonesian => self.id_id,
            AppLanguage::ChineseSimplified => self.zh_cn,
            AppLanguage::Russian => self.ru_ru,
            AppLanguage::English => self.en_us,
        };

        if localized.trim().is_empty() {
            self.en_us
        } else {
            localized
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContentSurveyQuestion {
    pub id: &'static str,
    pub prompt: L10n,
    pub answers: &'static [ContentSurveyAnswer],
}

pub(crate) const fn q(
    id: &'static str,
    prompt: L10n,
    answers: &'static [ContentSurveyAnswer],
) -> ContentSurveyQuestion {
    ContentSurveyQuestion {
        id,
        prompt,
        answers,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ContentSurveyAnswer {
    pub id: u8,
    pub label: L10n,
}

pub(crate) const fn a(id: u8, label: L10n) -> ContentSurveyAnswer {
    ContentSurveyAnswer { id, label }
}

#[derive(Debug, Clone)]
pub struct SurveyDefinition {
    pub id: &'static str,
    pub version: &'static str,
    pub launch_delay: u32,
    pub later_delay: u32,
    pub title: &'static L10n,
    pub questions: &'static [ContentSurveyQuestion],
    pub message_label: &'static L10n,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeedbackSurveyState {
    #[serde(default, deserialize_with = "deserialize_optional_uuid_lossy")]
    pub client_id: Option<Uuid>,
    #[serde(default)]
    pub never_show: bool,
    #[serde(default)]
    pub surveys: HashMap<String, FeedbackSurveyVersionState>,
}

fn deserialize_optional_uuid_lossy<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.and_then(|value| Uuid::parse_str(value.trim()).ok()))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeedbackSurveyVersionState {
    #[serde(default)]
    pub launches_seen: u32,
    #[serde(default)]
    pub next_prompt_launch: u32,
    #[serde(default)]
    pub later_deferrals: u32,
    #[serde(default)]
    pub submitted: bool,
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub submit_pending: bool,
    #[serde(default)]
    pub submit_discarded: bool,
}

impl SurveyDefinition {
    pub fn key(&self) -> String {
        format!("{}:{}", self.version, self.id)
    }
}

const FEEDBACK_SURVEY: SurveyDefinition = SurveyDefinition {
    id: "feedback",
    version: env!("CARGO_PKG_VERSION"),
    launch_delay: crate::app::content::FEEDBACK_SURVEY_LAUNCH_DELAY,
    later_delay: 5,
    title: &crate::app::content::FEEDBACK_SURVEY_TITLE,
    questions: crate::app::content::FEEDBACK_SURVEY_QUESTIONS,
    message_label: &crate::app::content::FEEDBACK_SURVEY_MESSAGE_LABEL,
};

pub(crate) fn feedback_survey() -> Option<&'static SurveyDefinition> {
    if !crate::app::content::FEEDBACK_SURVEY_ENABLED {
        return None;
    }
    if FEEDBACK_SURVEY.questions.is_empty()
        && FEEDBACK_SURVEY
            .message_label
            .get(AppLanguage::English)
            .trim()
            .is_empty()
    {
        return None;
    }
    Some(&FEEDBACK_SURVEY)
}

impl AppState {
    pub fn prepare_feedback_survey_on_launch(&mut self, survey: Option<&SurveyDefinition>) -> bool {
        let Some(survey) = survey else {
            let changed = self.show_feedback_survey;
            self.show_feedback_survey = false;
            return changed;
        };

        let mut changed = false;
        if self.feedback_survey.client_id.is_none() {
            self.feedback_survey.client_id = Some(Uuid::new_v4());
            changed = true;
        }

        if self.feedback_survey.never_show {
            if self.show_feedback_survey {
                self.show_feedback_survey = false;
                changed = true;
            }
            return changed;
        }

        let key = survey.key();
        let survey_state = self.feedback_survey.surveys.entry(key).or_default();
        if survey_state.submitted
            || survey_state.skipped
            || survey_state.submit_pending
            || survey_state.submit_discarded
        {
            if self.show_feedback_survey {
                self.show_feedback_survey = false;
                changed = true;
            }
            return changed;
        }

        if self.show_feedback_survey {
            return changed;
        }

        survey_state.launches_seen = survey_state.launches_seen.saturating_add(1);
        changed = true;

        let next_prompt_launch = survey_state
            .next_prompt_launch
            .max(survey.launch_delay.max(1));
        if survey_state.launches_seen >= next_prompt_launch {
            self.show_feedback_survey = true;
        }

        changed
    }

    pub fn defer_feedback_survey(&mut self, survey: &SurveyDefinition) {
        let key = survey.key();
        let survey_state = self.feedback_survey.surveys.entry(key).or_default();
        survey_state.later_deferrals = survey_state.later_deferrals.saturating_add(1);
        let defer_delay = survey
            .later_delay
            .max(1)
            .saturating_mul(survey_state.later_deferrals.max(1));
        survey_state.next_prompt_launch = survey_state.launches_seen.saturating_add(defer_delay);
        self.show_feedback_survey = false;
    }

    pub fn skip_feedback_survey(&mut self, survey: &SurveyDefinition) {
        let key = survey.key();
        let survey_state = self.feedback_survey.surveys.entry(key).or_default();
        survey_state.skipped = true;
        survey_state.submit_pending = false;
        self.show_feedback_survey = false;
    }

    pub fn mark_feedback_survey_submit_pending(&mut self, survey: &SurveyDefinition) {
        let key = survey.key();
        let survey_state = self.feedback_survey.surveys.entry(key).or_default();
        survey_state.submit_pending = true;
        survey_state.submit_discarded = false;
        self.show_feedback_survey = false;
    }

    pub fn mark_feedback_survey_submitted(&mut self, survey: &SurveyDefinition) {
        let key = survey.key();
        let survey_state = self.feedback_survey.surveys.entry(key).or_default();
        survey_state.submitted = true;
        survey_state.submit_pending = false;
        survey_state.submit_discarded = false;
        self.show_feedback_survey = false;
    }

    pub fn discard_pending_feedback_survey(&mut self, survey: &SurveyDefinition) {
        let key = survey.key();
        let survey_state = self.feedback_survey.surveys.entry(key).or_default();
        survey_state.submit_pending = false;
        survey_state.submit_discarded = true;
        self.show_feedback_survey = false;
    }

    pub fn disable_feedback_surveys(&mut self) {
        self.feedback_survey.never_show = true;
        self.show_feedback_survey = false;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedAppUpdate {
    pub version: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModCategory {
    pub id: String,
    pub game_id: String,
    pub name: String,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ModCategorySortMode {
    #[default]
    Manual,
    ByNameAsc,
    ByModCountAsc,
    ByModCountDesc,
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
    #[serde(default)]
    pub backend: GameBackend,
    pub xxmi_code: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum GameBackend {
    #[default]
    Xxmi,
    UnrealEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstall {
    pub definition: GameDefinition,
    pub mods_path_override: Option<PathBuf>,
    #[serde(default)]
    pub modded_exe_path_override: Option<PathBuf>,
    #[serde(default)]
    pub vanilla_exe_path_override: Option<PathBuf>,
    #[serde(default = "serde_default_true")]
    pub apply_mod_changes_in_game: bool,
    pub enabled: bool,
}

impl GameInstall {
    pub fn mods_path(&self, use_default: bool) -> Option<PathBuf> {
        match self.definition.backend {
            GameBackend::Xxmi => {
                if use_default {
                    self.modded_exe_path_override
                        .as_ref()
                        .and_then(|path| {
                            default_mods_path_from_launcher(path, &self.definition.xxmi_code)
                        })
                        .or_else(|| default_mods_path(&self.definition.xxmi_code))
                } else {
                    self.mods_path_override.clone()
                }
            }
            GameBackend::UnrealEngine => self.mods_path_override.clone().or_else(|| {
                self.vanilla_exe_path_override.as_ref().and_then(|path| {
                    default_unreal_pak_mods_path_from_exe(&self.definition.id, path)
                })
            }),
        }
    }

    pub fn disabled_mods_path(&self, use_default: bool) -> Option<PathBuf> {
        match self.definition.backend {
            GameBackend::Xxmi => None,
            GameBackend::UnrealEngine => self
                .mods_path(use_default)
                .map(|path| default_unreal_disabled_mods_path_from_mods_path(&path)),
        }
    }

    pub fn modded_exe_path(&self) -> Option<PathBuf> {
        self.modded_exe_path_override.clone()
    }

    pub fn vanilla_exe_path(&self) -> Option<PathBuf> {
        self.vanilla_exe_path_override.clone()
    }

    pub fn is_xxmi(&self) -> bool {
        self.definition.backend == GameBackend::Xxmi
    }

    pub fn is_unreal_engine(&self) -> bool {
        self.definition.backend == GameBackend::UnrealEngine
    }

    pub fn backend_label(&self) -> &str {
        match self.definition.backend {
            GameBackend::Xxmi => self.definition.xxmi_code.as_str(),
            GameBackend::UnrealEngine => "Unreal Engine",
        }
    }
}

fn default_unreal_disabled_mods_path_from_mods_path(mods_path: &Path) -> PathBuf {
    if let Some(paks_dir) = mods_path.parent() {
        if paks_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Paks"))
        {
            if let Some(content_dir) = paks_dir.parent() {
                return content_dir.join(UNREAL_DISABLED_MODS_DIR);
            }
        }
        return paks_dir.join(UNREAL_DISABLED_MODS_DIR);
    }
    mods_path.with_file_name(UNREAL_DISABLED_MODS_DIR)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AppFontStyle {
    Classic,
    #[default]
    Modern,
    Elegant,
    Traditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AppLanguage {
    #[default]
    English,
    Indonesian,
    ChineseSimplified,
    Russian,
}

impl AppLanguage {
    pub const ALL: [Self; 4] = [
        Self::English,
        Self::Indonesian,
        Self::ChineseSimplified,
        Self::Russian,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Indonesian => "Indonesian",
            Self::ChineseSimplified => "Chinese (Simplified)",
            Self::Russian => "Russian",
        }
    }

    pub fn native_label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Indonesian => "Bahasa Indonesia",
            Self::ChineseSimplified => "简体中文",
            Self::Russian => "Русский",
        }
    }

    pub fn from_locale_tag(tag: &str) -> Option<Self> {
        let normalized = tag.trim().replace('_', "-").to_ascii_lowercase();
        let language = normalized
            .split(['.', '@'])
            .next()
            .unwrap_or(normalized.as_str());
        if language == "en" || language.starts_with("en-") {
            return Some(Self::English);
        }
        if language == "zh-cn"
            || language == "zh-hans"
            || language.starts_with("zh-cn-")
            || language.starts_with("zh-hans-")
            || language == "zh-sg"
            || language.starts_with("zh-sg-")
        {
            return Some(Self::ChineseSimplified);
        }
        if language == "id"
            || language.starts_with("id-")
            || language == "in"
            || language.starts_with("in-")
        {
            return Some(Self::Indonesian);
        }
        if language == "ru" || language == "ru-ru" || language.starts_with("ru-") {
            return Some(Self::Russian);
        }
        None
    }

    pub fn detect_system_supported() -> Option<Self> {
        sys_locale::get_locales().find_map(|tag| Self::from_locale_tag(&tag))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModStatus {
    Active,
    Disabled,
    Archived,
}

/// Which metadata source the mod-detail pane is currently showing. Persisted per
/// mod so reopening restores the last-viewed source. `TextFile` is backed by the
/// existing `extracted.readme_path` / `user.extracted_metadata_source_path`
/// (personal note included); the others are self-describing. Resolved against live
/// availability at render time, so a stale pick falls back gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetadataSourceKind {
    Description,
    TextFile,
    Hotkeys,
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
    pub selected_metadata_source: Option<MetadataSourceKind>,
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
    pub content_size_bytes: u64,
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
    SizeAsc,
    SizeDesc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LibraryGroupMode {
    #[default]
    Category,
    Status,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum LibraryCategoryDisplayMode {
    GroupedSections,
    #[default]
    Folders,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModSourceData {
    pub gamebanana: Option<GameBananaLink>,
    pub snapshot: Option<GameBananaSnapshot>,
    pub raw_profile_json: Option<String>,
    /// The earliest time a transient GameBanana request failure may be retried
    /// automatically. Terminal source states are represented by `MissingSource`
    /// and are not retried by bulk update checks.
    #[serde(default)]
    pub update_check_retry_after: Option<DateTime<Utc>>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackedFileMeta {
    pub file_id: u64,
    pub file_name: String,
    pub date_added: i64,
    pub version: Option<String>,
    pub archived: bool,
    /// GameBanana file label (`_sDescription`, e.g. "Main File" / "Experimental").
    /// Labels are the only stable lineage signal when authors upload files with
    /// meaningless names (RabbitFX ships `v24_<hash>.zip`-style names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// `label` is matching metadata, not file identity: persisted
/// `ignored_update_signature`s predate the field and must keep comparing equal
/// to freshly computed signatures that now carry labels.
impl PartialEq for TrackedFileMeta {
    fn eq(&self, other: &Self) -> bool {
        self.file_id == other.file_id
            && self.file_name == other.file_name
            && self.date_added == other.date_added
            && self.version == other.version
            && self.archived == other.archived
    }
}

impl Eq for TrackedFileMeta {}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolEntry {
    pub id: String,
    pub game_id: String,
    pub label: String,
    pub path: PathBuf,
    /// Path relative to the game's mods root, when the tool lives inside it. `path` is absolute and
    /// therefore only valid for one install; this is what lets a tool re-match itself after the
    /// mods folder moves or a profile is carried to another machine.
    #[serde(default)]
    pub relative_path: Option<PathBuf>,
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

/// Rendering backend preference. The renderer is fixed at window creation, so
/// a change takes effect on the next launch. Picks that make no sense on the
/// current platform (e.g. Dx12 on Linux) behave like Auto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererPreference {
    Auto,
    Dx12,
    Vulkan,
    Metal,
    OpenGl,
}

impl RendererPreference {
    /// Proper API names; not translated.
    pub fn api_label(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Dx12 => Some("DirectX 12"),
            Self::Vulkan => Some("Vulkan"),
            Self::Metal => Some("Metal"),
            Self::OpenGl => Some("OpenGL"),
        }
    }

    pub fn valid_on_current_platform(self) -> bool {
        match self {
            Self::Auto | Self::OpenGl => true,
            Self::Dx12 => cfg!(windows),
            Self::Vulkan => cfg!(any(windows, target_os = "linux")),
            Self::Metal => cfg!(target_os = "macos"),
        }
    }
}

impl Default for RendererPreference {
    fn default() -> Self {
        Self::Auto
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
    #[serde(default)]
    pub retry_payload: Option<TaskRetryPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskRetryPayload {
    BrowseDownload(BrowseDownloadTaskPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseDownloadTaskPayload {
    pub game_id: String,
    pub mod_id: u64,
    pub file: BrowseDownloadTaskFile,
    #[serde(default)]
    pub selected_files: Vec<BrowseDownloadTaskFile>,
    #[serde(default)]
    pub unsafe_content: bool,
    #[serde(default)]
    pub update_folder_name: Option<String>,
    #[serde(default)]
    pub update_target_mod_id: Option<String>,
    #[serde(default)]
    pub install_disabled: bool,
    #[serde(default)]
    pub post_install_rename_to: Option<String>,
    #[serde(default)]
    pub profile_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseDownloadTaskFile {
    pub id: u64,
    pub file_name: String,
    pub file_size: u64,
    pub date_added: i64,
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub is_archived: bool,
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

pub fn default_mods_path_from_launcher(launcher_exe: &Path, xxmi_code: &str) -> Option<PathBuf> {
    let launcher_dir = launcher_exe.parent()?;
    let root = if launcher_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Bin"))
        && launcher_dir
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Resources"))
    {
        launcher_dir.parent()?.parent()?
    } else {
        launcher_dir
    };
    Some(root.join(xxmi_code).join("Mods"))
}

pub fn default_unreal_pak_mods_path_from_exe(game_id: &str, game_exe: &Path) -> Option<PathBuf> {
    match game_id {
        "nte" => default_nte_pak_mods_path_from_exe(game_exe),
        _ => None,
    }
}

pub fn default_unreal_bypasser_paths_from_exe(game_id: &str, game_exe: &Path) -> Vec<PathBuf> {
    match game_id {
        "nte" => default_nte_bypasser_paths_from_exe(game_exe),
        _ => Vec::new(),
    }
}

fn default_nte_pak_mods_path_from_exe(game_exe: &Path) -> Option<PathBuf> {
    for ancestor in game_exe.ancestors() {
        if ancestor
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("HT"))
        {
            return Some(ancestor.join("Content").join("Paks").join("~mods"));
        }
    }

    let root = game_exe
        .ancestors()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Neverness To Everness"))
        })
        .or_else(|| {
            game_exe.parent().and_then(|parent| {
                parent
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("NTEGlobal"))
                    .then(|| parent.parent())
                    .flatten()
            })
        })
        .or_else(|| game_exe.parent())?;
    Some(
        root.join("Client")
            .join("WindowsNoEditor")
            .join("HT")
            .join("Content")
            .join("Paks")
            .join("~mods"),
    )
}

fn default_nte_bypasser_paths_from_exe(game_exe: &Path) -> Vec<PathBuf> {
    let Some(pak_mods) = default_nte_pak_mods_path_from_exe(game_exe) else {
        return Vec::new();
    };
    let Some(ht_root) = pak_mods
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
    else {
        return Vec::new();
    };
    let dir = ht_root.join("Binaries").join("Win64");
    vec![
        dir.join("AyakaNTEModLoader.asi"),
        dir.join("UniversalSigBypasser.asi"),
    ]
}

/// Whether the process may create files or subdirectories inside `path`, walking up to the
/// deepest existing ancestor when `path` itself does not exist yet (e.g. `~mods` before the
/// first install).  Side-effect free: nothing is created on disk.
pub fn path_allows_dir_creation(path: &Path) -> bool {
    for ancestor in path.ancestors() {
        if ancestor.is_dir() {
            return dir_allows_creation(ancestor);
        }
    }
    false
}

#[cfg(windows)]
fn dir_allows_creation(dir: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;

    // FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY ask only for create rights, and with no create
    // disposition set the open maps to OPEN_EXISTING, so this evaluates the directory's ACL
    // without touching its contents.  FILE_FLAG_BACKUP_SEMANTICS is required to open a
    // directory handle at all.
    const FILE_ADD_FILE: u32 = 0x0002;
    const FILE_ADD_SUBDIRECTORY: u32 = 0x0004;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    std::fs::OpenOptions::new()
        .access_mode(FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(dir)
        .is_ok()
}

#[cfg(not(windows))]
fn dir_allows_creation(_dir: &Path) -> bool {
    true
}

pub fn xxmi_launcher_file_names() -> &'static [&'static str] {
    &["XXMI Launcher.exe", "XXMI-Launcher.exe"]
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
        "nte" => &[
            "NevernessToEverness.exe",
            "Neverness To Everness.exe",
            "NTE.exe",
            "NTEGame.exe",
            "NTEGlobalGame.exe",
            "NTEGlobalLauncher.exe",
            "HT.exe",
            "HT-Win64-Shipping.exe",
            "HTGame.exe",
            "NevernessToEverness\\NevernessToEverness.exe",
            "Neverness To Everness\\Neverness To Everness.exe",
            "NevernessToEverness\\NTE.exe",
            "Neverness To Everness\\NTE.exe",
            "NevernessToEverness\\NTEGame.exe",
            "Neverness To Everness\\NTEGame.exe",
            "NevernessToEverness\\NTEGlobalGame.exe",
            "Neverness To Everness\\NTEGlobalGame.exe",
            "NevernessToEverness\\NTEGlobalLauncher.exe",
            "Neverness To Everness\\NTEGlobalLauncher.exe",
            "NevernessToEverness\\NTEGlobal\\NTEGame.exe",
            "Neverness To Everness\\NTEGlobal\\NTEGame.exe",
            "NevernessToEverness\\NTEGlobal\\NTEGlobalGame.exe",
            "Neverness To Everness\\NTEGlobal\\NTEGlobalGame.exe",
            "NevernessToEverness\\NTEGlobal\\NTEGlobalLauncher.exe",
            "Neverness To Everness\\NTEGlobal\\NTEGlobalLauncher.exe",
            "NTEGlobal\\NTEGlobalGame.exe",
            "NTEGlobal\\NTEGlobalLauncher.exe",
            "Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HT-Win64-Shipping.exe",
            "Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HTGame.exe",
            "Neverness To Everness\\Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HT-Win64-Shipping.exe",
            "Neverness To Everness\\Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HTGame.exe",
            "NevernessToEverness\\Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HT-Win64-Shipping.exe",
            "NevernessToEverness\\Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HTGame.exe",
            "HT\\Binaries\\Win64\\HT-Win64-Shipping.exe",
            "HT\\Binaries\\Win64\\HTGame.exe",
        ],
        _ => &[],
    }
}

pub fn vanilla_exe_file_names(game_id: &str) -> Vec<&'static str> {
    let rels = vanilla_exe_rels(game_id);
    let mut names = Vec::with_capacity(rels.len());
    for rel in rels {
        let Some(name) = Path::new(rel).file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
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
        ("wuwa", "Wuthering Waves", GameBackend::Xxmi, "WWMI"),
        ("zzz", "Zenless Zone Zero", GameBackend::Xxmi, "ZZMI"),
        ("endfield", "Arknights Endfield", GameBackend::Xxmi, "EFMI"),
        ("starrail", "Honkai Star Rail", GameBackend::Xxmi, "SRMI"),
        ("genshin", "Genshin Impact", GameBackend::Xxmi, "GIMI"),
        ("honkai-impact", "Honkai Impact", GameBackend::Xxmi, "HIMI"),
        (
            "nte",
            "Neverness To Everness",
            GameBackend::UnrealEngine,
            "",
        ),
    ]
    .into_iter()
    .map(|(id, name, backend, xxmi_code)| GameInstall {
        definition: GameDefinition {
            id: id.to_string(),
            name: name.to_string(),
            backend,
            xxmi_code: xxmi_code.to_string(),
        },
        mods_path_override: None,
        modded_exe_path_override: None,
        vanilla_exe_path_override: None,
        apply_mod_changes_in_game: true,
        enabled: true,
    })
    .collect()
}

#[cfg(test)]
mod dir_creation_probe_tests {
    use super::*;

    #[test]
    fn writable_dir_allows_creation() {
        let temp = tempfile::tempdir().unwrap();
        assert!(path_allows_dir_creation(temp.path()));
    }

    #[test]
    fn missing_path_probes_deepest_existing_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("Content").join("Paks").join("~mods");
        assert!(path_allows_dir_creation(&missing));
    }

}

#[cfg(test)]
mod unreal_path_tests {
    use super::*;

    #[test]
    fn nte_pak_path_derives_from_ht_game_exe() {
        let exe = Path::new(
            r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Binaries\Win64\HTGame.exe",
        );
        let pak = default_unreal_pak_mods_path_from_exe("nte", exe).unwrap();
        assert_eq!(
            pak,
            Path::new(
                r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Content\Paks\~mods"
            )
        );
    }

    #[test]
    fn nte_disabled_path_derives_outside_paks_from_pak_mods_path() {
        let pak = Path::new(
            r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Content\Paks\~mods",
        );
        let disabled = default_unreal_disabled_mods_path_from_mods_path(pak);
        assert_eq!(
            disabled,
            Path::new(
                r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Content\~mods-disabledByHestia"
            )
        );
    }

    #[test]
    fn nte_bypasser_paths_derive_from_ht_game_exe() {
        let exe = Path::new(
            r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Binaries\Win64\HTGame.exe",
        );
        let bypassers = default_unreal_bypasser_paths_from_exe("nte", exe);
        assert_eq!(
            bypassers,
            vec![
                PathBuf::from(
                    r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Binaries\Win64\AyakaNTEModLoader.asi"
                ),
                PathBuf::from(
                    r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Binaries\Win64\UniversalSigBypasser.asi"
                ),
            ]
        );
    }

    #[test]
    fn nte_pak_path_derives_from_global_launcher_exe() {
        let exe = Path::new(r"D:\Games\Neverness To Everness\NTEGlobal\NTEGlobalGame.exe");
        let pak = default_unreal_pak_mods_path_from_exe("nte", exe).unwrap();
        assert_eq!(
            pak,
            Path::new(
                r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Content\Paks\~mods"
            )
        );
    }

    #[test]
    fn nte_bypasser_paths_derive_from_global_launcher_exe() {
        let exe = Path::new(r"D:\Games\Neverness To Everness\NTEGlobal\NTEGlobalGame.exe");
        let bypassers = default_unreal_bypasser_paths_from_exe("nte", exe);
        assert_eq!(
            bypassers,
            vec![
                PathBuf::from(
                    r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Binaries\Win64\AyakaNTEModLoader.asi"
                ),
                PathBuf::from(
                    r"D:\Games\Neverness To Everness\Client\WindowsNoEditor\HT\Binaries\Win64\UniversalSigBypasser.asi"
                ),
            ]
        );
    }
}

#[cfg(test)]
mod feedback_survey_tests {
    use super::*;

    const SURVEY_TITLE: L10n = l10n("Survey", "Survey", "Survey", "Survey");
    const SURVEY_MESSAGE_LABEL: L10n = l10n(
        "Anything else?",
        "Anything else?",
        "Anything else?",
        "Anything else?",
    );
    const ANSWERS: &[ContentSurveyAnswer] = &[
        ContentSurveyAnswer {
            id: 1,
            label: l10n("Yes", "Yes", "Yes", "Yes"),
        },
        ContentSurveyAnswer {
            id: 2,
            label: l10n("No", "No", "No", "No"),
        },
    ];
    const QUESTIONS: &[ContentSurveyQuestion] = &[ContentSurveyQuestion {
        id: "q1",
        prompt: l10n("Question?", "Question?", "Question?", "Question?"),
        answers: ANSWERS,
    }];
    const SURVEY: SurveyDefinition = SurveyDefinition {
        id: "survey",
        version: "1.2.3",
        launch_delay: 5,
        later_delay: 3,
        title: &SURVEY_TITLE,
        questions: QUESTIONS,
        message_label: &SURVEY_MESSAGE_LABEL,
    };

    #[test]
    fn feedback_survey_opens_after_configured_launches() {
        let mut state = AppState::default();

        for _ in 0..4 {
            assert!(state.prepare_feedback_survey_on_launch(Some(&SURVEY)));
            assert!(!state.show_feedback_survey);
        }

        assert!(state.prepare_feedback_survey_on_launch(Some(&SURVEY)));
        assert!(state.show_feedback_survey);
    }

    #[test]
    fn feedback_survey_open_window_does_not_advance_launch_delay() {
        let mut state = AppState::default();
        for _ in 0..5 {
            state.prepare_feedback_survey_on_launch(Some(&SURVEY));
        }
        assert!(state.show_feedback_survey);

        assert!(!state.prepare_feedback_survey_on_launch(Some(&SURVEY)));
        let survey_state = state.feedback_survey.surveys.get(&SURVEY.key()).unwrap();
        assert_eq!(survey_state.launches_seen, 5);
        assert!(state.show_feedback_survey);
    }

    #[test]
    fn feedback_survey_maybe_later_delay_increases_by_base_delay() {
        let mut state = AppState::default();
        for _ in 0..5 {
            state.prepare_feedback_survey_on_launch(Some(&SURVEY));
        }

        state.defer_feedback_survey(&SURVEY);
        assert!(!state.show_feedback_survey);
        assert_eq!(
            state
                .feedback_survey
                .surveys
                .get(&SURVEY.key())
                .unwrap()
                .later_deferrals,
            1
        );
        for _ in 0..2 {
            state.prepare_feedback_survey_on_launch(Some(&SURVEY));
            assert!(!state.show_feedback_survey);
        }

        state.prepare_feedback_survey_on_launch(Some(&SURVEY));
        assert!(state.show_feedback_survey);

        state.defer_feedback_survey(&SURVEY);
        assert!(!state.show_feedback_survey);
        assert_eq!(
            state
                .feedback_survey
                .surveys
                .get(&SURVEY.key())
                .unwrap()
                .later_deferrals,
            2
        );
        for _ in 0..5 {
            state.prepare_feedback_survey_on_launch(Some(&SURVEY));
            assert!(!state.show_feedback_survey);
        }

        state.prepare_feedback_survey_on_launch(Some(&SURVEY));
        assert!(state.show_feedback_survey);

        state.defer_feedback_survey(&SURVEY);
        assert!(!state.show_feedback_survey);
        assert_eq!(
            state
                .feedback_survey
                .surveys
                .get(&SURVEY.key())
                .unwrap()
                .later_deferrals,
            3
        );
        for _ in 0..8 {
            state.prepare_feedback_survey_on_launch(Some(&SURVEY));
            assert!(!state.show_feedback_survey);
        }

        state.prepare_feedback_survey_on_launch(Some(&SURVEY));
        assert!(state.show_feedback_survey);
    }

    #[test]
    fn feedback_survey_skip_and_never_show_suppress_future_prompts() {
        let mut skipped = AppState::default();
        skipped.skip_feedback_survey(&SURVEY);
        for _ in 0..6 {
            skipped.prepare_feedback_survey_on_launch(Some(&SURVEY));
        }
        assert!(!skipped.show_feedback_survey);

        let mut disabled = AppState::default();
        disabled.disable_feedback_surveys();
        for _ in 0..6 {
            disabled.prepare_feedback_survey_on_launch(Some(&SURVEY));
        }
        assert!(!disabled.show_feedback_survey);
    }

    #[test]
    fn feedback_survey_pending_and_discarded_suppress_future_prompts() {
        let mut pending = AppState::default();
        pending.mark_feedback_survey_submit_pending(&SURVEY);
        for _ in 0..6 {
            pending.prepare_feedback_survey_on_launch(Some(&SURVEY));
        }
        assert!(!pending.show_feedback_survey);
        let pending_state = pending.feedback_survey.surveys.get(&SURVEY.key()).unwrap();
        assert!(pending_state.submit_pending);
        assert!(!pending_state.submitted);

        let mut discarded = AppState::default();
        discarded.discard_pending_feedback_survey(&SURVEY);
        for _ in 0..6 {
            discarded.prepare_feedback_survey_on_launch(Some(&SURVEY));
        }
        assert!(!discarded.show_feedback_survey);
        let discarded_state = discarded
            .feedback_survey
            .surveys
            .get(&SURVEY.key())
            .unwrap();
        assert!(discarded_state.submit_discarded);
        assert!(!discarded_state.submitted);
    }

    #[test]
    fn feedback_survey_invalid_client_uuid_deserializes_as_missing() {
        let raw = r#"
            client_id = "not-a-uuid"
            never_show = false
        "#;

        let state: FeedbackSurveyState = toml::from_str(raw).unwrap();
        assert!(state.client_id.is_none());
    }
}

#[cfg(test)]
mod custom_proxy_tests {
    use super::*;

    #[test]
    fn normalizes_a_bare_proxy_endpoint_to_http() {
        let proxy = CustomProxyConfig::parse("127.0.0.1:7891").unwrap();
        assert_eq!(proxy.endpoint(), "http://127.0.0.1:7891");
    }

    #[test]
    fn preserves_supported_proxy_protocols() {
        for scheme in ["socks4", "socks4a", "socks5", "socks5h"] {
            let proxy = CustomProxyConfig::parse(&format!("{scheme}://[::1]:7891")).unwrap();
            assert_eq!(proxy.endpoint(), format!("{scheme}://[::1]:7891"));
        }
    }

    #[test]
    fn bare_proxy_candidates_follow_the_protocol_priority() {
        let candidates = CustomProxyConfig::parse_candidates("127.0.0.1:7891").unwrap();
        let endpoints: Vec<_> = candidates.iter().map(|proxy| proxy.endpoint()).collect();
        assert_eq!(
            endpoints,
            [
                "socks5h://127.0.0.1:7891",
                "socks5://127.0.0.1:7891",
                "socks4a://127.0.0.1:7891",
                "socks4://127.0.0.1:7891",
                "http://127.0.0.1:7891",
                "https://127.0.0.1:7891",
            ]
        );
    }

    #[test]
    fn missing_proxy_port_scans_the_configured_port_order() {
        let candidates = CustomProxyConfig::parse_candidates("proxy.example").unwrap();
        assert_eq!(candidates.len(), 66);
        assert_eq!(candidates[0].endpoint(), "socks5h://proxy.example:80");
        assert_eq!(candidates[5].endpoint(), "https://proxy.example:80");
        assert_eq!(candidates[6].endpoint(), "socks5h://proxy.example:8080");

        let socks5 = CustomProxyConfig::parse_candidates("socks5://proxy.example").unwrap();
        assert_eq!(socks5.len(), 11);
        assert_eq!(socks5[0].endpoint(), "socks5://proxy.example:80");
        assert_eq!(socks5[1].endpoint(), "socks5://proxy.example:8080");
    }

    #[test]
    fn rejects_proxy_credentials_and_url_suffixes() {
        for value in [
            "http://user:password@127.0.0.1:7891",
            "http://127.0.0.1:7891/path",
            "http://127.0.0.1:7891?query=value",
            "18.139.224.252\\",
            "ftp://127.0.0.1:7891",
        ] {
            assert!(CustomProxyConfig::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn disabled_proxy_does_not_require_an_endpoint() {
        let preferences = StaticPreferences::default();
        assert_eq!(CustomProxyConfig::from_preferences(&preferences), Ok(None));
    }
}
