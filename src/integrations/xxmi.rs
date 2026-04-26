use std::{
    ffi::OsStr,
    fs,
    hash::Hasher,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, SystemTime},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x00000008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use xxhash_rust::xxh3::Xxh3;

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::{
        UI::{
            Shell::{
                FO_DELETE, FOF_ALLOWUNDO, FOF_NOERRORUI, FOF_NOCONFIRMATION, FOF_SILENT,
                SHFILEOPSTRUCTW, SHFileOperationW, ShellExecuteW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
};

use crate::{
    model::{
        AppState, DISABLED_CONTAINER, DiscoveredTool, ExtractedMetadata, GameInstall, MOD_META_DIR,
        ModEntry, ModMetadata, ModStatus, PortableModState,
    },
    persistence,
};

pub fn refresh_state(state: &mut AppState, target_game_id: Option<&str>) -> Result<()> {
    let mut newly_scanned = Vec::new();

    // Determine which games to scan
    let games_to_scan: Vec<GameInstall> = match target_game_id {
        Some(id) => state.games.iter().filter(|g| g.definition.id == id).cloned().collect(),
        None => state.games.iter().filter(|g| g.enabled).cloned().collect(),
    };

    for game in &games_to_scan {
        // Auto-create mods directory if both executables exist but folder is missing
        if let Some(mods_path) = game.mods_path(state.use_default_mods_path) {
            if !mods_path.exists() {
                let vanilla_exists = game.vanilla_exe_path().as_ref().is_some_and(|p| p.is_file());
                let modded_exists = state.modded_launcher_path_override.as_ref()
                    .or(game.modded_exe_path_override.as_ref())
                    .is_some_and(|p| p.is_file());

                if vanilla_exists && modded_exists {
                    fs::create_dir_all(&mods_path)
                        .with_context(|| format!("failed to create mod directory: {}", mods_path.display()))?;
                }
            }
        }
        newly_scanned.extend(scan_live_mods(game, state.use_default_mods_path)?);
        newly_scanned.extend(scan_archived_mods(game, state.use_default_mods_path)?);
    }

    for discovered in &mut newly_scanned {
        // Hydrate from existing memory state to preserve non-portable flags (like update_state)
        // and ensure we don't overwrite portable flags with defaults if they aren't in the JSON yet.
        hydrate_from_existing_state(discovered, state);
        write_portable_metadata(discovered)?;
    }

    if let Some(id) = target_game_id {
        // Selective refresh: only replace mods for the target game
        state.mods.retain(|m| m.game_id != id);
        state.mods.extend(newly_scanned);
    } else {
        // Full refresh (e.g. on startup)
        state.mods = newly_scanned;
    }

    state.mods.sort_by(|a, b| {
        a.game_id.cmp(&b.game_id).then_with(|| {
            a.folder_name
                .to_lowercase()
                .cmp(&b.folder_name.to_lowercase())
        })
    });
    Ok(())
}

#[allow(dead_code)]
pub fn save_mod_metadata(mod_entry: &mut ModEntry) -> Result<()> {
    write_portable_metadata(mod_entry)
}

pub fn disable_mod(mod_entry: &mut ModEntry) -> Result<()> {
    if mod_entry.status != ModStatus::Active {
        bail!("only active mods can be disabled");
    }
    let disabled_root = mod_entry.root_path.join(DISABLED_CONTAINER);
    fs::create_dir_all(&disabled_root)?;

    let entries = substantive_entries(&mod_entry.root_path)?;
    for entry in entries {
        let name = entry
            .file_name()
            .ok_or_else(|| anyhow!("mod entry missing file name"))?;
        fs::rename(&entry, disabled_root.join(name))?;
    }
    mod_entry.status = ModStatus::Disabled;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
    Ok(())
}

pub fn enable_mod(mod_entry: &mut ModEntry) -> Result<()> {
    if mod_entry.status != ModStatus::Disabled {
        bail!("only disabled mods can be enabled");
    }

    let disabled_root = mod_entry.root_path.join(DISABLED_CONTAINER);
    if !disabled_root.exists() {
        bail!("missing DISABLED_BY_HESTIA container");
    }
    for entry in fs::read_dir(&disabled_root)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        fs::rename(&path, mod_entry.root_path.join(name))?;
    }
    fs::remove_dir_all(&disabled_root)?;
    mod_entry.status = ModStatus::Active;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
    Ok(())
}

pub fn archive_mod(
    mod_entry: &mut ModEntry,
    game: &GameInstall,
    use_default_path: bool,
) -> Result<PathBuf> {
    if mod_entry.status == ModStatus::Archived {
        bail!("mod is already archived");
    }
    let archive_root = archived_mods_root(game, use_default_path)?;
    fs::create_dir_all(&archive_root)?;
    let destination = archive_root.join(&mod_entry.folder_name);
    if destination.exists() {
        bail!(
            "archive destination already exists: {}",
            destination.display()
        );
    }
    // Store the original path before archiving
    mod_entry.archive_original_path = Some(mod_entry.root_path.clone());
    fs::rename(&mod_entry.root_path, &destination)?;
    mod_entry.root_path = destination.clone();
    mod_entry.status = ModStatus::Archived;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
    Ok(destination)
}

pub fn restore_mod(mod_entry: &mut ModEntry, game: &GameInstall, use_default_path: bool) -> Result<PathBuf> {
    if mod_entry.status != ModStatus::Archived {
        bail!("only archived mods can be restored");
    }
    let live_root = game
        .mods_path(use_default_path)
        .ok_or_else(|| anyhow!("game has no live mods path"))?;
    fs::create_dir_all(&live_root)?;
    let destination = live_root.join(&mod_entry.folder_name);
    if destination.exists() {
        bail!("live mod folder already exists: {}", destination.display());
    }
    fs::rename(&mod_entry.root_path, &destination)?;
    mod_entry.root_path = destination.clone();
    mod_entry.archive_original_path = None;
    mod_entry.status = ModStatus::Active;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
    Ok(destination)
}

pub fn send_to_recycle_bin(mod_entry: &ModEntry) -> Result<()> {
    if !mod_entry.root_path.exists() {
        return Ok(());
    }

    let mut trash_err: Option<anyhow::Error> = None;
    for delay_ms in [150_u64, 400, 900] {
        match trash::delete(&mod_entry.root_path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                if !mod_entry.root_path.exists() {
                    return Ok(());
                }
                trash_err = Some(anyhow!(err));
                thread::sleep(Duration::from_millis(delay_ms));
            }
        }
    }

    #[cfg(windows)]
    {
        let mut shell_err: Option<anyhow::Error> = None;
        for delay_ms in [150_u64, 400, 900] {
            match shell_recycle_delete(&mod_entry.root_path) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    if !mod_entry.root_path.exists() {
                        return Ok(());
                    }
                    shell_err = Some(err);
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
        }

        return Err(shell_err
            .unwrap_or_else(|| anyhow!("unknown native recycle-bin failure")))
        .context(
            trash_err
                .map(|err| format!("failed to send mod to recycle bin after fallback: {err:#}"))
                .unwrap_or_else(|| "failed to send mod to recycle bin after fallback".to_string()),
        );
    }

    #[cfg(not(windows))]
    Err(trash_err.unwrap_or_else(|| anyhow!("unknown recycle-bin failure")))
        .context("failed to send mod to recycle bin")
}

#[cfg(windows)]
fn shell_recycle_delete(path: &Path) -> Result<()> {
    let mut wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .chain(std::iter::once(0))
        .collect();

    let mut op = SHFILEOPSTRUCTW::default();
    op.wFunc = FO_DELETE;
    op.pFrom = PCWSTR(wide_path.as_mut_ptr());
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16;

    let result = unsafe { SHFileOperationW(&mut op) };
    if result == 0 && !op.fAnyOperationsAborted.as_bool() {
        return Ok(());
    }

    if !path.exists() {
        return Ok(());
    }

    if op.fAnyOperationsAborted.as_bool() {
        bail!("shell recycle operation was aborted");
    }

    bail!("shell recycle operation failed with code {}", result)
}

#[allow(dead_code)]
pub fn launch_executable(path: &Path) -> Result<()> {
    launch_executable_with_args(path, &[], false, "executable")
}

#[allow(dead_code)]
pub fn launch_executable_with_raw_args(path: &Path, raw_args: &str) -> Result<()> {
    if raw_args.trim().is_empty() {
        return launch_executable(path);
    }
    let args = shlex::split(raw_args)
        .ok_or_else(|| anyhow!("invalid launch options: unmatched quotes"))?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    launch_executable_with_args(path, &arg_refs, false, "executable")
}

pub fn launch_path_with_raw_args(path: &Path, raw_args: &str) -> Result<()> {
    if !path.is_file() {
        bail!("tool not found: {}", path.display());
    }
    let args = if raw_args.trim().is_empty() {
        Vec::new()
    } else {
        shlex::split(raw_args)
            .ok_or_else(|| anyhow!("invalid launch options: unmatched quotes"))?
    };
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    #[cfg(windows)]
    {
        let working_dir = path.parent();
        return shell_execute_open(path, &arg_refs, working_dir)
            .map_err(anyhow::Error::from)
            .with_context(|| format!("failed to launch {}", path.display()));
    }

    #[allow(unreachable_code)]
    launch_executable_with_args(path, &arg_refs, false, "tool")
}

pub fn launch_vanilla_executable(path: &Path) -> Result<()> {
    launch_executable_with_args(path, &[], false, "vanilla executable")
}

fn launch_executable_with_args(
    path: &Path,
    args: &[&str],
    detached: bool,
    label: &str,
) -> Result<()> {
    if !path.is_file() {
        bail!("{label} not found: {}", path.display());
    }
    let working_dir = path.parent().map(Path::to_path_buf);
    let mut command = Command::new(path);
    command.args(args);
    if let Some(dir) = working_dir.as_ref() {
        command.current_dir(dir);
    }

    #[cfg(windows)]
    if detached {
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    match command.spawn() {
        Ok(_child) => Ok(()),
        Err(err) => {
            if err.raw_os_error() == Some(740) {
                #[cfg(windows)]
                {
                    match shell_execute_open(path, args, working_dir.as_deref()) {
                        Ok(()) => return Ok(()),
                        Err(shell_err) => {
                            if shell_err.code == 5 {
                                return shell_execute_runas(path, args, working_dir.as_deref())
                                    .with_context(|| {
                                        format!("failed to launch via shell: {}", path.display())
                                    });
                            }
                            return Err(shell_err).with_context(|| {
                                format!("failed to launch via shell: {}", path.display())
                            });
                        }
                    }
                }
            }
            Err(anyhow!(err).context(format!("failed to launch {}", path.display())))
        }
    }
}

pub fn launch_xxmi_launcher(launcher_exe: &Path, xxmi_code: &str) -> Result<()> {
    if !launcher_exe.is_file() {
        bail!("XXMI launcher executable not found: {}", launcher_exe.display());
    }

    let args = ["--nogui", "--xxmi", xxmi_code];
    launch_executable_with_args(launcher_exe, &args, true, "XXMI launcher")
}

#[cfg(windows)]
fn shell_execute_open(exe: &Path, args: &[&str], working_dir: Option<&Path>) -> Result<(), ShellExecuteError> {
    shell_execute("open", exe, args, working_dir)
}

#[cfg(windows)]
fn shell_execute_runas(exe: &Path, args: &[&str], working_dir: Option<&Path>) -> Result<(), ShellExecuteError> {
    shell_execute("runas", exe, args, working_dir)
}

#[cfg(windows)]
fn shell_execute(
    verb: &str,
    exe: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<(), ShellExecuteError> {
    let verb = wide_null(verb);
    let file = wide_null(&exe.display().to_string());
    let params = wide_null(
        &args
            .iter()
            .map(|value| quote_arg_windows(value))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let directory = working_dir
        .map(|dir| wide_null(&dir.display().to_string()))
        .unwrap_or_else(|| vec![0]);

    // SAFETY: All pointers are valid null-terminated UTF-16 strings for the duration of the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(directory.as_ptr()),
            SW_SHOWNORMAL,
        )
    };

    let code = result.0 as isize;
    if code <= 32 {
        return Err(ShellExecuteError::new(code));
    }
    Ok(())
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
struct ShellExecuteError {
    code: isize,
}

#[cfg(windows)]
impl ShellExecuteError {
    fn new(code: isize) -> Self {
        Self { code }
    }
}

#[cfg(windows)]
impl std::fmt::Display for ShellExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.code {
            2 => "file not found",
            3 => "path not found",
            5 => "access denied (UAC canceled or blocked)",
            31 => "no association",
            _ => "shell execution failed",
        };
        write!(f, "{message} (ShellExecute error {})", self.code)
    }
}

#[cfg(windows)]
impl std::error::Error for ShellExecuteError {}

#[cfg(windows)]
fn wide_null(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

#[cfg(windows)]
fn quote_arg_windows(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quotes = arg.chars().any(|c| c.is_whitespace() || c == '"');
    if !needs_quotes {
        return arg.to_string();
    }
    let mut out = String::new();
    out.push('"');
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(ch);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

fn scan_live_mods(game: &GameInstall, use_default_path: bool) -> Result<Vec<ModEntry>> {
    let Some(root) = game.mods_path(use_default_path) else {
        return Ok(Vec::new());
    };
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        mods.push(load_mod_entry(game, path, false)?);
    }
    Ok(mods)
}

fn scan_archived_mods(game: &GameInstall, use_default_path: bool) -> Result<Vec<ModEntry>> {
    let root = archived_mods_root(game, use_default_path)?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        mods.push(load_mod_entry(game, path, true)?);
    }
    Ok(mods)
}

fn archived_mods_root(game: &GameInstall, use_default_path: bool) -> Result<PathBuf> {
    let live_root = game
        .mods_path(use_default_path)
        .ok_or_else(|| anyhow!("game has no live mods path"))?;
    let parent = live_root
        .parent()
        .ok_or_else(|| anyhow!("invalid game mods path"))?;
    Ok(parent.join("Mods_Archived"))
}

fn load_mod_entry(
    game: &GameInstall,
    root_path: PathBuf,
    force_archived: bool,
) -> Result<ModEntry> {
    let folder_name = root_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("invalid mod folder name"))?
        .to_string();

    let portable = persistence::load_portable_mod_state(&root_path)?;
    let extracted = extract_metadata(&root_path)?;
    let metadata = match &portable {
        Some(stored) => ModMetadata {
            extracted,
            user: stored.metadata.user.clone(),
            prompt_for_missing_metadata: stored.metadata.prompt_for_missing_metadata,
        },
        None => ModMetadata {
            extracted,
            user: Default::default(),
            prompt_for_missing_metadata: true,
        },
    };

    let discovered_tools = metadata
        .extracted
        .discovered_executables
        .iter()
        .map(|relative| DiscoveredTool {
            label: Path::new(relative)
                .file_stem()
                .or_else(|| Path::new(relative).file_name())
                .and_then(OsStr::to_str)
                .unwrap_or("Tool")
                .to_string(),
            path: root_path.join(relative),
        })
        .collect();

    let id = portable
        .as_ref()
        .map(|stored| stored.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let (content_mtime, ini_hash) = compute_mod_fingerprint(&root_path)?;

    let (created_at, updated_at, unsafe_content) = match &portable {
        Some(stored) => (
            stored.created_at.unwrap_or_else(Utc::now),
            stored.updated_at.unwrap_or_else(Utc::now),
            derive_unsafe_content_from_portable(stored).unwrap_or(false),
        ),
        None => (Utc::now(), Utc::now(), false),
    };

    Ok(ModEntry {
        id,
        game_id: game.definition.id.clone(),
        folder_name,
        root_path: root_path.clone(),
        status: if force_archived {
            ModStatus::Archived
        } else {
            detect_status(&root_path)?
        },
        metadata,
        discovered_tools,
        archive_original_path: None,
        created_at,
        updated_at,
        content_mtime,
        ini_hash,
        unsafe_content,
        source: portable.as_ref().and_then(|stored| stored.source.clone()),
        update_state: crate::model::ModUpdateState::Unlinked,
    })
}

fn derive_unsafe_content_from_portable(stored: &PortableModState) -> Option<bool> {
    let raw = stored.source.as_ref()?.raw_profile_json.as_ref()?;
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let ratings = value.get("_aContentRatings")?;
    let map = ratings.as_object()?;
    Some(!map.is_empty())
}

fn hydrate_from_existing_state(discovered: &mut ModEntry, state: &AppState) {
    let existing = state.mods.iter()
        .find(|item| item.id == discovered.id)
        .or_else(|| state.mods.iter().find(|item| item.root_path == discovered.root_path));

    if let Some(existing) = existing {
        discovered.id = existing.id.clone(); // Preserve stable ID if matched by path
        discovered.created_at = existing.created_at;
        let has_existing_fingerprint =
            existing.content_mtime.is_some() || existing.ini_hash.is_some();
        let fingerprint_changed = has_existing_fingerprint
            && (existing.content_mtime.map(|t| t.timestamp()) != discovered.content_mtime.map(|t| t.timestamp())
                || existing.ini_hash != discovered.ini_hash);
        if fingerprint_changed {
            discovered.updated_at = Utc::now();
        } else {
            discovered.updated_at = existing.updated_at;
        }
        discovered.metadata.user = existing.metadata.user.clone();
        discovered.metadata.prompt_for_missing_metadata =
            existing.metadata.prompt_for_missing_metadata;
        discovered.source = existing.source.clone();
        discovered.update_state = existing.update_state;
    }
}

fn write_portable_metadata(mod_entry: &ModEntry) -> Result<()> {
    let portable = PortableModState {
        id: mod_entry.id.clone(),
        metadata: mod_entry.metadata.clone(),
        source: mod_entry.source.clone(),
        unsafe_content: mod_entry.unsafe_content,
        created_at: Some(mod_entry.created_at),
        updated_at: Some(mod_entry.updated_at),
    };
    persistence::save_portable_mod_state(&mod_entry.root_path, &portable)
}

fn detect_status(root: &Path) -> Result<ModStatus> {
    let disabled_root = root.join(DISABLED_CONTAINER);
    if !disabled_root.exists() {
        return Ok(ModStatus::Active);
    }

    let has_live_entries = !substantive_entries(root)?.is_empty();
    if has_live_entries {
        Ok(ModStatus::Active)
    } else {
        Ok(ModStatus::Disabled)
    }
}

fn substantive_entries(root: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name() else {
            continue;
        };
        if name == OsStr::new(MOD_META_DIR) || name == OsStr::new(DISABLED_CONTAINER) {
            continue;
        }
        entries.push(path);
    }
    Ok(entries)
}

fn compute_mod_fingerprint(root: &Path) -> Result<(Option<DateTime<Utc>>, Option<String>)> {
    let mut max_mtime: Option<SystemTime> = None;
    let mut hasher = Xxh3::new();
    let mut found_ini = false;

    for entry in walkdir::WalkDir::new(root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.components().any(|part| part.as_os_str() == MOD_META_DIR) {
            continue;
        }
        let metadata = entry.metadata()?;
        if let Ok(modified) = metadata.modified() {
            max_mtime = match max_mtime {
                Some(current) => Some(current.max(modified)),
                None => Some(modified),
            };
        }
        if is_ini_file(path) {
            found_ini = true;
            let rel = path.strip_prefix(root).unwrap_or(path);
            hasher.update(rel.to_string_lossy().as_bytes());
            let bytes = fs::read(path)?;
            hasher.update(&bytes);
        }
    }

    let mtime = max_mtime.map(DateTime::<Utc>::from);
    let hash = if found_ini {
        Some(format!("{:016x}", hasher.finish()))
    } else {
        None
    };
    Ok((mtime, hash))
}

fn is_ini_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ini"))
}

fn extract_metadata(root: &Path) -> Result<ExtractedMetadata> {
    let mut description = None;
    let mut hotkeys = Vec::new();
    let mut executables = Vec::new();
    let mut readme_path = None;
    let mut best_readme_priority = 0; // 0: none, 1: generic txt/md/json, 2: matches "readme" pattern

    for entry in walkdir::WalkDir::new(root).max_depth(3) {
        let entry = entry?;
        let path = entry.path();
        if path
            .components()
            .any(|part| part.as_os_str() == MOD_META_DIR)
        {
            continue;
        }
        if entry.file_type().is_file() {
            let extension = path
                .extension()
                .and_then(OsStr::to_str)
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if extension == "exe" {
                if let Ok(relative) = path.strip_prefix(root) {
                    executables.push(relative.to_string_lossy().to_string());
                }
            }
            if ["txt", "md", "json"].contains(&extension.as_str()) {
                let raw = fs::read_to_string(path).unwrap_or_default();
                let trimmed = raw.trim();

                let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or_default().to_lowercase();
                let is_readme_pattern = file_name.contains("readme") || file_name.contains("read me") || file_name.contains("read_me");
                let priority = if is_readme_pattern { 2 } else { 1 };

                // Update description if we found a "better" readme or haven't found any yet
                if !trimmed.is_empty() && priority > best_readme_priority {
                    description = Some(trimmed.to_string());
                    if let Ok(relative) = path.strip_prefix(root) {
                        readme_path = Some(relative.to_string_lossy().to_string());
                    }
                    best_readme_priority = priority;
                }

                // Always scan all text-like files for hotkeys
                for line in trimmed.lines() {
                    if line.to_ascii_lowercase().contains("hotkey") {
                        hotkeys.push(line.trim().to_string());
                    }
                }
            }
        }
    }

    hotkeys.sort();
    hotkeys.dedup();
    executables.sort();
    executables.dedup();

    Ok(ExtractedMetadata {
        description,
        hotkeys,
        discovered_executables: executables,
        readme_path,
    })
}
