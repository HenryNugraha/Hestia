use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use filetime::{FileTime, set_file_mtime};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use xxhash_rust::xxh3::xxh3_64;

use crate::model::{
    AppLanguage, AppState, FeedbackSurveyState, GameInstall, LibraryFolder, MOD_META_DIR,
    MOD_META_FILE, ModCategory, ModCategorySortMode, OperationLogEntry, PortableModState,
    StagedAppUpdate, StaticPreferences, TaskEntry, TaskKind, TaskRetryPayload, TaskStatus,
    TasksLayout, TasksOrder, ToolEntry,
};

#[derive(Debug, Clone)]
pub struct PortablePaths {
    pub state_archive: PathBuf,
    pub history_db: PathBuf,
}

pub const RUNTIME_TEMP_DIR_NAME: &str = "Hestia";
pub const ORPHAN_TMP_AGE_SECS: u64 = 600;
pub const LOG_HISTORY_LIMIT: usize = 30_000;
pub const LOG_HISTORY_TRIM_THRESHOLD: usize = 50_000;
pub const TASK_HISTORY_LIMIT: usize = 10_000;
pub const TASK_HISTORY_TRIM_THRESHOLD: usize = 15_000;

fn serde_default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppPreferences {
    version: u32,
    #[serde(default)]
    app_version: Option<String>,
    games: Vec<GameInstall>,
    #[serde(default)]
    library_folders: Vec<LibraryFolder>,
    #[serde(default)]
    show_log: bool,
    #[serde(default)]
    show_tasks: bool,
    #[serde(default)]
    show_tools: bool,
    #[serde(default)]
    feedback_survey: FeedbackSurveyState,
    #[serde(default = "serde_default_true")]
    startup_path_scan_completed: bool,
    #[serde(default)]
    tasks_layout: TasksLayout,
    #[serde(default)]
    tasks_order: TasksOrder,
    #[serde(default)]
    last_selected_game_id: Option<String>,
    #[serde(default)]
    auto_game_enable_done: bool,
    #[serde(default)]
    tools: Vec<ToolEntry>,
    #[serde(default)]
    categories: Vec<ModCategory>,
    #[serde(default)]
    category_sort_mode_by_game: HashMap<String, ModCategorySortMode>,
    #[serde(default)]
    create_downloaded_mod_category_by_game: HashMap<String, bool>,
    #[serde(default)]
    staged_app_update: Option<StagedAppUpdate>,
    // Flatten static preferences for backward compatibility
    #[serde(flatten)]
    static_prefs: StaticPreferences,
}

impl From<&AppState> for AppPreferences {
    fn from(state: &AppState) -> Self {
        Self {
            version: 7,
            app_version: Some(state.app_version.clone()),
            games: state.games.clone(),
            library_folders: state.library_folders.clone(),
            show_log: state.show_log,
            show_tasks: state.show_tasks,
            show_tools: state.show_tools,
            feedback_survey: state.feedback_survey.clone(),
            startup_path_scan_completed: state.startup_path_scan_completed,
            tasks_layout: state.tasks_layout,
            tasks_order: state.tasks_order,
            last_selected_game_id: state.last_selected_game_id.clone(),
            auto_game_enable_done: state.auto_game_enable_done,
            tools: state.tools.clone(),
            categories: state.categories.clone(),
            category_sort_mode_by_game: state.category_sort_mode_by_game.clone(),
            create_downloaded_mod_category_by_game: state
                .create_downloaded_mod_category_by_game
                .clone(),
            staged_app_update: state.staged_app_update.clone(),
            static_prefs: state.static_prefs.clone(),
        }
    }
}

impl PortablePaths {
    pub fn discover() -> Result<Self> {
        let exe = std::env::current_exe().context("failed to resolve current executable")?;
        let root = exe
            .parent()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .context("failed to resolve portable root")?;
        let paths = resolve_persistent_data_paths(&exe, &root)?;
        Ok(Self {
            state_archive: paths.state_archive,
            history_db: paths.history_db,
        })
    }

    pub fn ensure_layout(&self) -> Result<()> {
        if let Some(parent) = self.state_archive.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = self.history_db.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(runtime_temp_cache_dir())?;
        fs::create_dir_all(runtime_temp_downloads_partial_dir())?;
        fs::create_dir_all(runtime_temp_downloads_final_dir())?;
        fs::create_dir_all(runtime_temp_extract_dir())?;
        init_history_store(self)?;
        Ok(())
    }
}

struct PersistentDataPaths {
    state_archive: PathBuf,
    history_db: PathBuf,
}

fn resolve_persistent_data_paths(exe: &Path, root: &Path) -> Result<PersistentDataPaths> {
    let fallback_dir = appdata_fallback_dir()?;
    Ok(resolve_persistent_data_paths_with_fallback_dir(
        exe,
        root,
        &fallback_dir,
    ))
}

fn resolve_persistent_data_paths_with_fallback_dir(
    exe: &Path,
    root: &Path,
    fallback_dir: &Path,
) -> PersistentDataPaths {
    let exe_stem = exe
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("hestia");
    let portable_state = root.join(format!("{exe_stem}.toml"));
    let portable_history = root.join("hestia.dat");
    let fallback_state = fallback_dir.join("hestia.toml");
    let fallback_history = fallback_dir.join("hestia.dat");

    if portable_state.exists() || portable_history.exists() {
        return PersistentDataPaths {
            state_archive: portable_state,
            history_db: portable_history,
        };
    }

    if fallback_state.exists() || fallback_history.exists() {
        return PersistentDataPaths {
            state_archive: fallback_state,
            history_db: fallback_history,
        };
    }

    if dir_is_writable(root, exe_stem) {
        return PersistentDataPaths {
            state_archive: portable_state,
            history_db: portable_history,
        };
    }

    PersistentDataPaths {
        state_archive: fallback_state,
        history_db: fallback_history,
    }
}

fn appdata_fallback_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
        Ok(PathBuf::from(appdata).join("Hestia"))
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").context("$HOME is not set")?;
        Ok(PathBuf::from(home).join("Library/Application Support/Hestia"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var_os("HOME").expect("$HOME is not set");
                PathBuf::from(home).join(".config")
            });
        Ok(base.join("Hestia"))
    }
}

pub fn runtime_temp_root() -> PathBuf {
    std::env::temp_dir().join(RUNTIME_TEMP_DIR_NAME)
}

pub fn runtime_temp_cache_dir() -> PathBuf {
    runtime_temp_root().join("cache")
}

pub fn runtime_temp_downloads_dir() -> PathBuf {
    runtime_temp_downloads_final_dir()
}

pub fn runtime_temp_downloads_partial_dir() -> PathBuf {
    runtime_temp_root().join("download").join("partial")
}

pub fn runtime_temp_downloads_final_dir() -> PathBuf {
    runtime_temp_root().join("download").join("final")
}

pub fn runtime_temp_extract_dir() -> PathBuf {
    runtime_temp_root().join("extract")
}

pub fn load_app_state(paths: &PortablePaths) -> Result<AppState> {
    let mut state = AppState::default();
    if !paths.state_archive.exists() {
        state.show_whats_new = true;
        state.startup_path_scan_completed = false;
        return Ok(state);
    }
    let raw = fs::read_to_string(&paths.state_archive).context("failed to read hestia.toml")?;
    let prefs: AppPreferences = toml::from_str(&raw).context("failed to parse hestia.toml")?;

    let loaded_version = prefs.version;
    let previous_app_version = prefs.app_version.clone();
    let app_version_needs_save =
        app_version_needs_normalization(previous_app_version.as_deref(), env!("CARGO_PKG_VERSION"));
    
    // Check if language field was missing (needs save)
    let language_needs_save = prefs.static_prefs.language == AppLanguage::default() 
        && previous_app_version.is_some();
    
    state.version = prefs.version.max(7);
    state.show_whats_new =
        should_show_whats_new(previous_app_version.as_deref(), env!("CARGO_PKG_VERSION"));
    state.app_version = env!("CARGO_PKG_VERSION").to_string();
    state.games = prefs.games;
    state.library_folders = prefs.library_folders;
    state.show_log = prefs.show_log;
    state.show_tasks = prefs.show_tasks;
    state.show_tools = prefs.show_tools;
    state.feedback_survey = prefs.feedback_survey;
    state.startup_path_scan_completed = prefs.startup_path_scan_completed;
    state.tasks_layout = prefs.tasks_layout;
    state.tasks_order = prefs.tasks_order;
    state.last_selected_game_id = prefs.last_selected_game_id;
    state.auto_game_enable_done = prefs.auto_game_enable_done;
    state.tools = prefs.tools;
    state.categories = prefs.categories;
    state.category_sort_mode_by_game = prefs.category_sort_mode_by_game;
    state.staged_app_update = prefs.staged_app_update;
    
    // Move static preferences (single assignment, no field-by-field copying)
    state.static_prefs = prefs.static_prefs;
    
    let (create_downloaded_mod_category_by_game, preferences_need_save) =
        normalize_create_downloaded_mod_category_by_game(
            &state.games,
            &state.categories,
            prefs.create_downloaded_mod_category_by_game,
        );
    state.create_downloaded_mod_category_by_game = create_downloaded_mod_category_by_game;
    state.preferences_need_save = preferences_need_save || app_version_needs_save || language_needs_save;
    initialize_tool_orders(&mut state, loaded_version);
    Ok(state)
}

fn normalize_create_downloaded_mod_category_by_game(
    games: &[GameInstall],
    categories: &[ModCategory],
    mut saved: HashMap<String, bool>,
) -> (HashMap<String, bool>, bool) {
    let mut changed = false;
    for game in games {
        let game_id = &game.definition.id;
        if !saved.contains_key(game_id) {
            let game_has_categories = categories
                .iter()
                .any(|category| category.game_id == *game_id);
            saved.insert(game_id.clone(), !game_has_categories);
            changed = true;
        }
    }
    (saved, changed)
}

fn should_show_whats_new(previous_app_version: Option<&str>, current_app_version: &str) -> bool {
    let Some(previous_app_version) = previous_app_version else {
        return true;
    };
    let Ok(previous) = semver::Version::parse(previous_app_version.trim()) else {
        return true;
    };
    let Ok(current) = semver::Version::parse(current_app_version) else {
        return false;
    };
    previous < current
}

fn app_version_needs_normalization(
    previous_app_version: Option<&str>,
    current_app_version: &str,
) -> bool {
    match previous_app_version {
        Some(previous_app_version) => previous_app_version.trim() != current_app_version,
        None => true,
    }
}

fn initialize_tool_orders(state: &mut AppState, loaded_version: u32) {
    let game_ids: Vec<String> = state
        .games
        .iter()
        .map(|game| game.definition.id.clone())
        .collect();

    for game_id in game_ids {
        if loaded_version < 5 {
            let mut legacy_window_ids: Vec<String> = state
                .tools
                .iter()
                .filter(|tool| tool.game_id == game_id)
                .map(|tool| tool.id.clone())
                .collect();
            legacy_window_ids.sort_by(|a, b| {
                let left = state.tools.iter().find(|tool| tool.id == *a);
                let right = state.tools.iter().find(|tool| tool.id == *b);
                match (left, right) {
                    (Some(left), Some(right)) => left
                        .auto_detected
                        .cmp(&right.auto_detected)
                        .then_with(|| {
                            left.label
                                .to_ascii_lowercase()
                                .cmp(&right.label.to_ascii_lowercase())
                        })
                        .then_with(|| left.created_at.cmp(&right.created_at)),
                    _ => a.cmp(b),
                }
            });
            assign_tool_window_order(state, &game_id, &legacy_window_ids);

            let mut legacy_titlebar_ids: Vec<String> = state
                .tools
                .iter()
                .filter(|tool| tool.game_id == game_id && tool.show_in_titlebar)
                .map(|tool| tool.id.clone())
                .collect();
            legacy_titlebar_ids.sort_by(|a, b| {
                let left = state.tools.iter().find(|tool| tool.id == *a);
                let right = state.tools.iter().find(|tool| tool.id == *b);
                match (left, right) {
                    (Some(left), Some(right)) => left
                        .label
                        .to_ascii_lowercase()
                        .cmp(&right.label.to_ascii_lowercase())
                        .then_with(|| left.created_at.cmp(&right.created_at)),
                    _ => a.cmp(b),
                }
            });
            assign_tool_titlebar_order(state, &game_id, &legacy_titlebar_ids);
        } else {
            compact_tool_window_order(state, &game_id);
            compact_tool_titlebar_order(state, &game_id);
        }
    }
}

fn assign_tool_window_order(state: &mut AppState, game_id: &str, ids: &[String]) {
    let order_map: HashMap<&str, i32> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index as i32))
        .collect();
    for tool in state
        .tools
        .iter_mut()
        .filter(|tool| tool.game_id == game_id)
    {
        tool.window_order = order_map
            .get(tool.id.as_str())
            .copied()
            .unwrap_or(i32::MAX / 4);
    }
}

fn assign_tool_titlebar_order(state: &mut AppState, game_id: &str, ids: &[String]) {
    let order_map: HashMap<&str, i32> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index as i32))
        .collect();
    for tool in state
        .tools
        .iter_mut()
        .filter(|tool| tool.game_id == game_id)
    {
        if tool.show_in_titlebar {
            tool.titlebar_order = order_map.get(tool.id.as_str()).copied();
        } else {
            tool.titlebar_order = None;
        }
    }
}

fn compact_tool_window_order(state: &mut AppState, game_id: &str) {
    let mut ids: Vec<String> = state
        .tools
        .iter()
        .filter(|tool| tool.game_id == game_id)
        .map(|tool| tool.id.clone())
        .collect();
    ids.sort_by(|a, b| {
        let left = state.tools.iter().find(|tool| tool.id == *a);
        let right = state.tools.iter().find(|tool| tool.id == *b);
        match (left, right) {
            (Some(left), Some(right)) => left
                .window_order
                .cmp(&right.window_order)
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| {
                    left.label
                        .to_ascii_lowercase()
                        .cmp(&right.label.to_ascii_lowercase())
                }),
            _ => a.cmp(b),
        }
    });
    assign_tool_window_order(state, game_id, &ids);
}

fn compact_tool_titlebar_order(state: &mut AppState, game_id: &str) {
    let mut ids: Vec<String> = state
        .tools
        .iter()
        .filter(|tool| tool.game_id == game_id && tool.show_in_titlebar)
        .map(|tool| tool.id.clone())
        .collect();
    ids.sort_by(|a, b| {
        let left = state.tools.iter().find(|tool| tool.id == *a);
        let right = state.tools.iter().find(|tool| tool.id == *b);
        match (left, right) {
            (Some(left), Some(right)) => left
                .titlebar_order
                .unwrap_or(i32::MAX / 4)
                .cmp(&right.titlebar_order.unwrap_or(i32::MAX / 4))
                .then_with(|| left.window_order.cmp(&right.window_order))
                .then_with(|| left.created_at.cmp(&right.created_at)),
            _ => a.cmp(b),
        }
    });
    assign_tool_titlebar_order(state, game_id, &ids);
}

pub fn save_app_state(paths: &PortablePaths, state: &AppState) -> Result<()> {
    let prefs = AppPreferences::from(state);
    let raw = toml::to_string_pretty(&prefs).context("failed to serialize app preferences")?;
    write_atomic_text(&paths.state_archive, &raw)
}

pub fn load_history(paths: &PortablePaths, state: &mut AppState) -> Result<()> {
    match load_history_inner(paths, state) {
        Ok(()) => Ok(()),
        Err(_) => {
            reset_history_store(paths)?;
            state.operations.clear();
            state.tasks.clear();
            Ok(())
        }
    }
}

pub fn append_operation_log(paths: &PortablePaths, entry: &OperationLogEntry) -> Result<()> {
    let conn = open_history_db(paths)?;
    conn.execute(
        "INSERT OR REPLACE INTO operation_logs (id, timestamp, summary) VALUES (?1, ?2, ?3)",
        params![entry.id, entry.timestamp.to_rfc3339(), entry.summary],
    )?;
    prune_operation_logs(&conn)?;
    Ok(())
}

pub fn replace_task(paths: &PortablePaths, task: &TaskEntry) -> Result<()> {
    let conn = open_history_db(paths)?;
    conn.execute(
        "INSERT OR REPLACE INTO task_history
        (id, kind, status, title, game_id, created_at, updated_at, total_size, unsafe_content, retry_payload)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            task.id as i64,
            task_kind_to_str(task.kind),
            task_status_to_str(task.status),
            task.title,
            task.game_id,
            task.created_at.to_rfc3339(),
            task.updated_at.to_rfc3339(),
            task.total_size.map(|value| value as i64),
            if task.unsafe_content { 1_i64 } else { 0_i64 },
            serialize_task_retry_payload(task.retry_payload.as_ref())?,
        ],
    )?;
    prune_task_history(&conn)?;
    Ok(())
}

pub fn remove_task(paths: &PortablePaths, task_id: u64) -> Result<()> {
    let conn = open_history_db(paths)?;
    conn.execute(
        "DELETE FROM task_history WHERE id = ?1",
        params![task_id as i64],
    )?;
    Ok(())
}

pub fn clear_finished_tasks(paths: &PortablePaths) -> Result<()> {
    let conn = open_history_db(paths)?;
    conn.execute(
        "DELETE FROM task_history WHERE status IN ('Completed', 'Failed', 'Canceled')",
        [],
    )?;
    Ok(())
}

fn init_history_store(paths: &PortablePaths) -> Result<()> {
    let conn = open_history_db(paths)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS operation_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            summary TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS task_history (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            title TEXT NOT NULL,
            game_id TEXT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            total_size INTEGER NULL,
            unsafe_content INTEGER NOT NULL DEFAULT 0,
            retry_payload TEXT NULL
        );",
    )?;
    ensure_task_history_schema(&conn)?;
    Ok(())
}

fn open_history_db(paths: &PortablePaths) -> Result<Connection> {
    Connection::open(&paths.history_db)
        .with_context(|| format!("failed to open history db {}", paths.history_db.display()))
}

fn load_history_inner(paths: &PortablePaths, state: &mut AppState) -> Result<()> {
    let conn = open_history_db(paths)?;
    ensure_task_history_schema(&conn)?;
    normalize_interrupted_tasks(&conn)?;
    state.operations = load_operation_logs_from_conn(&conn)?;
    state.tasks = load_task_history_from_conn(&conn)?;
    Ok(())
}

fn ensure_task_history_schema(conn: &Connection) -> Result<()> {
    if !table_has_column(conn, "task_history", "unsafe_content")? {
        conn.execute(
            "ALTER TABLE task_history ADD COLUMN unsafe_content INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !table_has_column(conn, "task_history", "retry_payload")? {
        conn.execute(
            "ALTER TABLE task_history ADD COLUMN retry_payload TEXT NULL",
            [],
        )?;
    }
    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn reset_history_store(paths: &PortablePaths) -> Result<()> {
    remove_if_exists(&paths.history_db)?;
    let wal_path = PathBuf::from(format!("{}-wal", paths.history_db.display()));
    let shm_path = PathBuf::from(format!("{}-shm", paths.history_db.display()));
    remove_if_exists(&wal_path)?;
    remove_if_exists(&shm_path)?;
    init_history_store(paths)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn load_operation_logs_from_conn(conn: &Connection) -> Result<Vec<OperationLogEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, summary
        FROM operation_logs
        ORDER BY timestamp DESC, id DESC",
    )?;
    let mut rows = stmt.query([])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(OperationLogEntry {
            id: row.get(0)?,
            timestamp: parse_rfc3339_utc(&row.get::<_, String>(1)?)?,
            summary: row.get(2)?,
        });
    }
    Ok(items)
}

fn load_task_history_from_conn(conn: &Connection) -> Result<Vec<TaskEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, status, title, game_id, created_at, updated_at, total_size, unsafe_content, retry_payload
        FROM task_history
        ORDER BY created_at ASC, id ASC",
    )?;
    let mut rows = stmt.query([])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        let total_size_raw: Option<i64> = row.get(7)?;
        let retry_payload_raw: Option<String> = row.get(9)?;
        items.push(TaskEntry {
            id: row.get::<_, i64>(0)? as u64,
            kind: parse_task_kind(&row.get::<_, String>(1)?)?,
            status: parse_task_status(&row.get::<_, String>(2)?)?,
            title: row.get(3)?,
            game_id: row.get(4)?,
            created_at: parse_rfc3339_utc(&row.get::<_, String>(5)?)?,
            updated_at: parse_rfc3339_utc(&row.get::<_, String>(6)?)?,
            total_size: total_size_raw.map(|value| value as u64),
            unsafe_content: row.get::<_, i64>(8).unwrap_or(0) != 0,
            retry_payload: parse_task_retry_payload(retry_payload_raw.as_deref()),
        });
    }
    Ok(items)
}

fn serialize_task_retry_payload(payload: Option<&TaskRetryPayload>) -> Result<Option<String>> {
    payload
        .map(serde_json::to_string)
        .transpose()
        .context("failed to serialize task retry payload")
}

fn parse_task_retry_payload(raw: Option<&str>) -> Option<TaskRetryPayload> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str(raw).ok()
}

fn normalize_interrupted_tasks(conn: &Connection) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE task_history
        SET status = 'Canceled', updated_at = ?1
        WHERE status IN ('Queued', 'Installing', 'Downloading', 'Canceling')",
        params![now],
    )?;
    prune_task_history(conn)?;
    prune_operation_logs(conn)?;
    Ok(())
}

fn prune_operation_logs(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM operation_logs", [], |row| row.get(0))?;
    if count <= LOG_HISTORY_TRIM_THRESHOLD as i64 {
        return Ok(());
    }

    conn.execute(
        "DELETE FROM operation_logs
        WHERE id NOT IN (
            SELECT id FROM operation_logs
            ORDER BY timestamp DESC, id DESC
            LIMIT ?1
        )",
        params![LOG_HISTORY_LIMIT as i64],
    )?;
    Ok(())
}

fn prune_task_history(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM task_history WHERE status IN ('Completed', 'Failed', 'Canceled')",
        [],
        |row| row.get(0),
    )?;
    if count <= TASK_HISTORY_TRIM_THRESHOLD as i64 {
        return Ok(());
    }

    conn.execute(
        "DELETE FROM task_history
        WHERE status IN ('Completed', 'Failed', 'Canceled')
          AND id NOT IN (
            SELECT id FROM task_history
            WHERE status IN ('Completed', 'Failed', 'Canceled')
            ORDER BY updated_at DESC, id DESC
            LIMIT ?1
        )",
        params![TASK_HISTORY_LIMIT as i64],
    )?;
    Ok(())
}

fn parse_rfc3339_utc(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("failed to parse timestamp {value}"))?
        .with_timezone(&Utc))
}

fn task_kind_to_str(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Install => "Install",
        TaskKind::Download => "Download",
    }
}

fn task_status_to_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "Queued",
        TaskStatus::Installing => "Installing",
        TaskStatus::Downloading => "Downloading",
        TaskStatus::Canceling => "Canceling",
        TaskStatus::Completed => "Completed",
        TaskStatus::Failed => "Failed",
        TaskStatus::Canceled => "Canceled",
    }
}

fn parse_task_kind(value: &str) -> Result<TaskKind> {
    match value {
        "Install" => Ok(TaskKind::Install),
        "Download" => Ok(TaskKind::Download),
        _ => anyhow::bail!("unknown task kind {value}"),
    }
}

fn parse_task_status(value: &str) -> Result<TaskStatus> {
    match value {
        "Queued" => Ok(TaskStatus::Queued),
        "Installing" => Ok(TaskStatus::Installing),
        "Downloading" => Ok(TaskStatus::Downloading),
        "Canceling" => Ok(TaskStatus::Canceling),
        "Completed" => Ok(TaskStatus::Completed),
        "Failed" => Ok(TaskStatus::Failed),
        "Canceled" => Ok(TaskStatus::Canceled),
        _ => anyhow::bail!("unknown task status {value}"),
    }
}

fn dir_is_writable(root: &Path, exe_stem: &str) -> bool {
    let test_path = root.join(format!(".{exe_stem}.write_test"));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&test_path)?;
        file.write_all(b"test")?;
        file.flush()?;
        Ok(())
    })();
    let _ = fs::remove_file(&test_path);
    result.is_ok()
}

pub fn cache_file_path(cache_key: &str) -> PathBuf {
    let hashed = format!("{:016x}", xxh3_64(cache_key.as_bytes()));
    runtime_temp_cache_dir().join(format!("{hashed}.bin"))
}

pub fn cache_exists(_paths: &PortablePaths, cache_key: &str) -> Result<bool> {
    Ok(cache_file_path(cache_key).exists())
}

pub fn cache_get(_paths: &PortablePaths, cache_key: &str) -> Result<Option<Vec<u8>>> {
    let path = cache_file_path(cache_key);
    if !path.exists() {
        return Ok(None);
    }
    let bytes =
        fs::read(&path).with_context(|| format!("failed to read cache {}", path.display()))?;
    let now = FileTime::from_system_time(SystemTime::now());
    let _ = set_file_mtime(&path, now);
    Ok(Some(bytes))
}

pub fn cache_put(
    _paths: &PortablePaths,
    cache_key: &str,
    _cache_type: &str,
    data: &[u8],
    max_bytes: u64,
) -> Result<()> {
    let path = cache_file_path(cache_key);
    write_atomic_bytes(&path, data)?;
    evict_lru_if_needed_path(max_bytes)
}

pub fn evict_lru_if_needed(_paths: &PortablePaths, max_bytes: u64) -> Result<()> {
    evict_lru_if_needed_path(max_bytes)
}

fn evict_lru_if_needed_path(max_bytes: u64) -> Result<()> {
    if max_bytes == 0 {
        return Ok(());
    }
    let threshold = max_bytes.saturating_mul(80) / 100;
    let cache_root = runtime_temp_cache_dir();
    if !cache_root.exists() {
        return Ok(());
    }

    use rayon::prelude::*;
    
    // Collect entries first
    let entries: Vec<_> = WalkDir::new(&cache_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .collect();
    
    // Process in parallel to gather file info
    let files: Vec<_> = entries
        .par_iter()
        .filter_map(|entry| {
            let path = entry.path().to_path_buf();
            let metadata = fs::metadata(&path).ok()?;
            let size = metadata.len();
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((modified, path, size))
        })
        .collect();
    
    let usage: u64 = files.iter().map(|(_, _, size)| size).sum();

    if usage <= threshold {
        return Ok(());
    }

    let mut files = files;
    files.sort_by_key(|(modified, _, _)| *modified);
    let mut usage = usage;
    for (_, path, size) in files {
        let _ = fs::remove_file(&path);
        usage = usage.saturating_sub(size);
        if usage <= threshold {
            break;
        }
    }
    Ok(())
}

pub fn clear_cache_and_vacuum(_paths: &PortablePaths) -> Result<()> {
    let cache_dir = runtime_temp_cache_dir();
    if cache_dir.exists() {
        let _ = fs::remove_dir_all(cache_dir);
    }
    let partial = runtime_temp_downloads_partial_dir();
    if partial.exists() {
        let _ = fs::remove_dir_all(partial);
    }
    let final_dir = runtime_temp_downloads_final_dir();
    if final_dir.exists() {
        let _ = fs::remove_dir_all(final_dir);
    }
    let extract = runtime_temp_extract_dir();
    if extract.exists() {
        let _ = fs::remove_dir_all(extract);
    }
    fs::create_dir_all(runtime_temp_cache_dir())?;
    fs::create_dir_all(runtime_temp_downloads_partial_dir())?;
    fs::create_dir_all(runtime_temp_downloads_final_dir())?;
    fs::create_dir_all(runtime_temp_extract_dir())?;
    Ok(())
}

pub fn cleanup_orphan_tmp_files(
    selected_mods_root: Option<&Path>,
    active_tmp_paths: &HashSet<PathBuf>,
) -> Result<()> {
    let min_age = Duration::from_secs(ORPHAN_TMP_AGE_SECS);
    prune_tmp_files_in_dir(&runtime_temp_root(), active_tmp_paths, min_age)?;
    if let Some(mods_root) = selected_mods_root {
        prune_tmp_files_in_dir(mods_root, active_tmp_paths, min_age)?;
    }
    Ok(())
}

fn prune_tmp_files_in_dir(
    root: &Path,
    active_tmp_paths: &HashSet<PathBuf>,
    min_age: Duration,
) -> Result<()> {
    use rayon::prelude::*;
    
    if !root.exists() {
        return Ok(());
    }
    let now = SystemTime::now();
    
    // Collect entries first
    let entries: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .collect();
    
    // Process in parallel
    entries.par_iter().for_each(|entry| {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("tmp") {
            return;
        }
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if active_tmp_paths.contains(&canonical) {
            return;
        }
        let Ok(metadata) = fs::metadata(path) else {
            return;
        };
        let modified = metadata.modified().unwrap_or(now);
        if now.duration_since(modified).unwrap_or_default() < min_age {
            return;
        }
        let _ = fs::remove_file(path);
    });
    Ok(())
}

pub fn write_atomic_text(path: &Path, text: &str) -> Result<()> {
    write_atomic_bytes(path, text.as_bytes())
}

pub fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let tmp_name = format!(
        "{}.{}.tmp",
        path.file_name().and_then(|v| v.to_str()).unwrap_or("file"),
        now_ns
    );
    let tmp_path = path.with_file_name(tmp_name);

    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp_path)?;
        file.write_all(bytes)?;
        file.flush()?;
        let _ = file.sync_all();
    }

    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub fn load_portable_mod_state(mod_root: &Path) -> Result<Option<PortableModState>> {
    let path = mod_root.join(MOD_META_DIR).join(MOD_META_FILE);
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)?;
    let state = serde_json::from_str(&raw)?;
    Ok(Some(state))
}

pub fn save_portable_mod_state(mod_root: &Path, state: &PortableModState) -> Result<()> {
    let dir = mod_root.join(MOD_META_DIR);
    fs::create_dir_all(&dir)?;
    let raw = serde_json::to_string_pretty(state)?;
    write_atomic_text(&dir.join(MOD_META_FILE), &raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whats_new_opens_for_missing_invalid_and_older_app_versions() {
        assert!(should_show_whats_new(None, "1.2.3"));
        assert!(should_show_whats_new(Some("not-semver"), "1.2.3"));
        assert!(should_show_whats_new(Some("1.2.2"), "1.2.3"));
        assert!(should_show_whats_new(Some("1.2.3-alpha.1"), "1.2.3"));
        assert!(should_show_whats_new(Some("1.2.3-string.4"), "1.2.4"));
    }

    #[test]
    fn whats_new_stays_closed_for_same_or_newer_app_versions() {
        assert!(!should_show_whats_new(Some("1.2.3"), "1.2.3"));
        assert!(!should_show_whats_new(Some("1.2.4"), "1.2.3"));
    }

    #[test]
    fn newer_app_version_stays_closed_but_normalizes_preferences() {
        assert!(!should_show_whats_new(Some("9.0.0"), "1.2.3"));
        assert!(app_version_needs_normalization(Some("9.0.0"), "1.2.3"));
    }

    #[test]
    fn downloaded_mod_category_defaults_true_when_game_has_no_categories() {
        let state = AppState::default();
        let game_id = state.games[0].definition.id.clone();
        let (prefs, changed) =
            normalize_create_downloaded_mod_category_by_game(&state.games, &[], HashMap::new());

        assert!(changed);
        assert_eq!(prefs.get(&game_id), Some(&true));
    }

    #[test]
    fn downloaded_mod_category_defaults_false_only_for_games_with_categories() {
        let state = AppState::default();
        let game_with_category = state.games[0].definition.id.clone();
        let game_without_category = state.games[1].definition.id.clone();
        let categories = vec![ModCategory {
            id: "category-1".to_string(),
            game_id: game_with_category.clone(),
            name: "Existing".to_string(),
            order: 0,
        }];

        let (prefs, changed) = normalize_create_downloaded_mod_category_by_game(
            &state.games,
            &categories,
            HashMap::new(),
        );

        assert!(changed);
        assert_eq!(prefs.get(&game_with_category), Some(&false));
        assert_eq!(prefs.get(&game_without_category), Some(&true));
    }

    #[test]
    fn downloaded_mod_category_preserves_saved_values() {
        let state = AppState::default();
        let game_id = state.games[0].definition.id.clone();
        let mut saved = HashMap::with_capacity(1);
        saved.insert(game_id.clone(), false);

        let (prefs, changed) =
            normalize_create_downloaded_mod_category_by_game(&state.games[0..1], &[], saved);

        assert!(!changed);
        assert_eq!(prefs.get(&game_id), Some(&false));
    }

    #[test]
    fn downloaded_mod_category_only_defaults_missing_game_entries() {
        let state = AppState::default();
        let saved_game = state.games[0].definition.id.clone();
        let missing_game_with_category = state.games[1].definition.id.clone();
        let missing_game_without_category = state.games[2].definition.id.clone();
        let categories = vec![ModCategory {
            id: "category-1".to_string(),
            game_id: missing_game_with_category.clone(),
            name: "Existing".to_string(),
            order: 0,
        }];
        let mut saved = HashMap::with_capacity(1);
        saved.insert(saved_game.clone(), true);

        let (prefs, changed) =
            normalize_create_downloaded_mod_category_by_game(&state.games, &categories, saved);

        assert!(changed);
        assert_eq!(prefs.get(&saved_game), Some(&true));
        assert_eq!(prefs.get(&missing_game_with_category), Some(&false));
        assert_eq!(prefs.get(&missing_game_without_category), Some(&true));
    }

    #[test]
    fn always_translate_mod_details_defaults_false_and_roundtrips() {
        let old_config: AppPreferences = toml::from_str("version = 7\ngames = []\n").unwrap();
        assert!(!old_config.static_prefs.always_translate_mod_details);

        let mut state = AppState::default();
        state.static_prefs.always_translate_mod_details = true;
        let raw = toml::to_string(&AppPreferences::from(&state)).unwrap();
        let saved: AppPreferences = toml::from_str(&raw).unwrap();

        assert!(saved.static_prefs.always_translate_mod_details);
    }

    #[test]
    fn persistent_paths_prefer_existing_fallback_before_new_portable_files() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path().join("install");
        let fallback_dir = temp.path().join("appdata").join("Hestia");
        fs::create_dir_all(&install_dir).unwrap();
        fs::create_dir_all(&fallback_dir).unwrap();
        fs::write(fallback_dir.join("hestia.toml"), "state").unwrap();
        fs::write(fallback_dir.join("hestia.dat"), "history").unwrap();

        let paths = resolve_persistent_data_paths_with_fallback_dir(
            &install_dir.join("hestia.exe"),
            &install_dir,
            &fallback_dir,
        );

        assert_eq!(paths.state_archive, fallback_dir.join("hestia.toml"));
        assert_eq!(paths.history_db, fallback_dir.join("hestia.dat"));
    }

    #[test]
    fn persistent_paths_keep_existing_portable_files_first() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path().join("install");
        let fallback_dir = temp.path().join("appdata").join("Hestia");
        fs::create_dir_all(&install_dir).unwrap();
        fs::create_dir_all(&fallback_dir).unwrap();
        fs::write(install_dir.join("hestia.toml"), "portable state").unwrap();
        fs::write(fallback_dir.join("hestia.toml"), "fallback state").unwrap();
        fs::write(fallback_dir.join("hestia.dat"), "fallback history").unwrap();

        let paths = resolve_persistent_data_paths_with_fallback_dir(
            &install_dir.join("hestia.exe"),
            &install_dir,
            &fallback_dir,
        );

        assert_eq!(paths.state_archive, install_dir.join("hestia.toml"));
        assert_eq!(paths.history_db, install_dir.join("hestia.dat"));
    }

    #[test]
    fn persistent_paths_create_portable_files_when_no_existing_state_and_writable() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path().join("install");
        let fallback_dir = temp.path().join("appdata").join("Hestia");
        fs::create_dir_all(&install_dir).unwrap();
        fs::create_dir_all(&fallback_dir).unwrap();

        let paths = resolve_persistent_data_paths_with_fallback_dir(
            &install_dir.join("hestia.exe"),
            &install_dir,
            &fallback_dir,
        );

        assert_eq!(paths.state_archive, install_dir.join("hestia.toml"));
        assert_eq!(paths.history_db, install_dir.join("hestia.dat"));
    }
}
