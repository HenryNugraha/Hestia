use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs,
    hash::Hasher,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, SystemTime},
};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x00000008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x08000000;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use uuid::Uuid;
use xxhash_rust::xxh3::Xxh3;

#[cfg(windows)]
use windows::{
    Win32::UI::{
        Shell::{
            FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
            SHFILEOPSTRUCTW, SHFileOperationW, ShellExecuteW,
        },
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
    core::PCWSTR,
};

use crate::{
    model::{
        AppState, DISABLED_CONTAINER, DiscoveredTool, ExtractedMetadata,
        ExtractedMetadataTextSource, GameInstall, MOD_META_DIR, ModEntry, ModMetadata, ModStatus,
        PERSONAL_NOTE_FILE, PortableModState,
    },
    persistence,
};

/// Read file with platform-specific optimizations.
/// On Windows, hints to the OS that sequential access is expected.
#[cfg(windows)]
fn read_file_optimized(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)
        .open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(not(windows))]
fn read_file_optimized(path: &Path) -> Result<Vec<u8>> {
    Ok(fs::read(path)?)
}

pub fn refresh_state(state: &mut AppState, target_game_id: Option<&str>) -> Result<()> {
    let mut newly_scanned = Vec::new();

    // Determine which games to scan
    let games_to_scan: Vec<GameInstall> = match target_game_id {
        Some(id) => state
            .games
            .iter()
            .filter(|g| g.definition.id == id)
            .cloned()
            .collect(),
        None => state.games.iter().filter(|g| g.enabled).cloned().collect(),
    };

    for game in &games_to_scan {
        // Auto-create mods directory if both executables exist but folder is missing
        if let Some(mods_path) = game.mods_path(state.static_prefs.use_default_mods_path) {
            if !mods_path.exists() {
                let vanilla_exists = game
                    .vanilla_exe_path()
                    .as_ref()
                    .is_some_and(|p| p.is_file());
                let modded_exists = state
                    .static_prefs.modded_launcher_path_override
                    .as_ref()
                    .or(game.modded_exe_path_override.as_ref())
                    .is_some_and(|p| p.is_file());

                if vanilla_exists && modded_exists {
                    fs::create_dir_all(&mods_path).with_context(|| {
                        format!("failed to create mod directory: {}", mods_path.display())
                    })?;
                }
            }
        }
        newly_scanned.extend(scan_live_mods(
            game,
            state.static_prefs.use_default_mods_path,
            state.static_prefs.scan_rabbitfx_requirement,
        )?);
        newly_scanned.extend(scan_archived_mods(
            game,
            state.static_prefs.use_default_mods_path,
            state.static_prefs.scan_rabbitfx_requirement,
        )?);
    }

    repair_duplicate_scanned_mod_ids(&mut newly_scanned, state, target_game_id);

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

fn repair_duplicate_scanned_mod_ids(
    newly_scanned: &mut [ModEntry],
    state: &AppState,
    target_game_id: Option<&str>,
) {
    let mut indices_by_id: HashMap<String, Vec<usize>> = HashMap::with_capacity(newly_scanned.len());
    for (index, mod_entry) in newly_scanned.iter().enumerate() {
        indices_by_id
            .entry(mod_entry.id.clone())
            .or_default()
            .push(index);
    }

    let duplicate_groups: Vec<Vec<usize>> = indices_by_id
        .into_values()
        .filter(|indices| indices.len() > 1)
        .collect();
    let state_entry_will_remain =
        |existing: &ModEntry| target_game_id.is_some_and(|game_id| existing.game_id != game_id);
    let id_collides_with_remaining_state = |id: &str| {
        state
            .mods
            .iter()
            .any(|existing| existing.id == id && state_entry_will_remain(existing))
    };

    if duplicate_groups.is_empty()
        && !newly_scanned
            .iter()
            .any(|mod_entry| id_collides_with_remaining_state(&mod_entry.id))
    {
        return;
    }

    let mut used_ids: HashSet<String> = state
        .mods
        .iter()
        .map(|mod_entry| mod_entry.id.clone())
        .collect();
    used_ids.extend(newly_scanned.iter().map(|mod_entry| mod_entry.id.clone()));
    let mut assign_new_id = |mod_entry: &mut ModEntry| {
        let new_id = loop {
            let candidate = Uuid::new_v4().to_string();
            if used_ids.insert(candidate.clone()) {
                break candidate;
            }
        };
        mod_entry.id = new_id;
    };

    for mut indices in duplicate_groups {
        indices.sort_by(|left, right| {
            newly_scanned[*left]
                .root_path
                .to_string_lossy()
                .to_lowercase()
                .cmp(
                    &newly_scanned[*right]
                        .root_path
                        .to_string_lossy()
                        .to_lowercase(),
                )
        });

        let keep_index = indices
            .iter()
            .copied()
            .find(|index| {
                if id_collides_with_remaining_state(&newly_scanned[*index].id) {
                    return false;
                }
                state.mods.iter().any(|existing| {
                    existing.id == newly_scanned[*index].id
                        && existing.root_path == newly_scanned[*index].root_path
                })
            })
            .unwrap_or(indices[0]);

        for index in indices {
            if index == keep_index {
                continue;
            }
            assign_new_id(&mut newly_scanned[index]);
        }
    }

    for mod_entry in newly_scanned {
        if id_collides_with_remaining_state(&mod_entry.id) {
            assign_new_id(mod_entry);
        }
    }
}

#[allow(dead_code)]
pub fn save_mod_metadata(mod_entry: &mut ModEntry) -> Result<()> {
    write_portable_metadata(mod_entry)
}

pub fn personal_note_relative_path() -> String {
    Path::new(MOD_META_DIR)
        .join(PERSONAL_NOTE_FILE)
        .to_string_lossy()
        .to_string()
}

pub fn personal_note_path(mod_root: &Path) -> PathBuf {
    mod_root.join(MOD_META_DIR).join(PERSONAL_NOTE_FILE)
}

pub fn sanitize_personal_note_content(raw: &str) -> Option<String> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut cleaned = String::with_capacity(normalized.len());

    for line in normalized.lines() {
        let line = line
            .chars()
            .filter(|ch| *ch == '\t' || !ch.is_control())
            .collect::<String>();
        let line = line.trim_end();
        cleaned.push_str(line);
        cleaned.push('\n');
    }

    let trimmed = cleaned.trim();
    if trimmed.chars().any(|ch| {
        !ch.is_whitespace() && !matches!(ch, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}')
    }) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub fn save_personal_note(mod_root: &Path, raw: &str) -> Result<Option<String>> {
    let path = personal_note_path(mod_root);
    if let Some(sanitized) = sanitize_personal_note_content(raw) {
        let dir = path
            .parent()
            .ok_or_else(|| anyhow!("invalid personal note path"))?;
        fs::create_dir_all(dir)?;
        persistence::write_atomic_text(&path, &sanitized)?;
        Ok(Some(sanitized))
    } else {
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(None)
    }
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

pub fn restore_mod(
    mod_entry: &mut ModEntry,
    game: &GameInstall,
    use_default_path: bool,
) -> Result<PathBuf> {
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
                if cleanup_metadata_only_mod_dir(&mod_entry.root_path)? {
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
                    if cleanup_metadata_only_mod_dir(&mod_entry.root_path)? {
                        return Ok(());
                    }
                    shell_err = Some(err);
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
        }

        return Err(shell_err.unwrap_or_else(|| anyhow!("unknown native recycle-bin failure")))
            .context(
                trash_err
                    .map(|err| format!("failed to send mod to recycle bin after fallback: {err:#}"))
                    .unwrap_or_else(|| {
                        "failed to send mod to recycle bin after fallback".to_string()
                    }),
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
        shlex::split(raw_args).ok_or_else(|| anyhow!("invalid launch options: unmatched quotes"))?
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
    #[cfg_attr(not(windows), allow(unused_variables))] detached: bool,
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
        bail!(
            "XXMI launcher executable not found: {}",
            launcher_exe.display()
        );
    }

    let args = ["--nogui", "--xxmi", xxmi_code];
    launch_executable_with_args(launcher_exe, &args, true, "XXMI launcher")
}

#[cfg(windows)]
fn shell_execute_open(
    exe: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<(), ShellExecuteError> {
    shell_execute("open", exe, args, working_dir)
}

#[cfg(windows)]
fn shell_execute_runas(
    exe: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<(), ShellExecuteError> {
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

fn scan_live_mods(
    game: &GameInstall,
    use_default_path: bool,
    scan_rabbitfx_requirement: bool,
) -> Result<Vec<ModEntry>> {
    let Some(root) = game.mods_path(use_default_path) else {
        return Ok(Vec::new());
    };
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mod_dirs = collect_scannable_mod_dirs(&root)?;

    // Process each mod directory in parallel
    let mods: Result<Vec<ModEntry>> = mod_dirs
        .par_iter()
        .map(|path| load_mod_entry(game, path.clone(), false, scan_rabbitfx_requirement))
        .collect();

    mods
}

fn scan_archived_mods(
    game: &GameInstall,
    use_default_path: bool,
    scan_rabbitfx_requirement: bool,
) -> Result<Vec<ModEntry>> {
    if game.mods_path(use_default_path).is_none() {
        return Ok(Vec::new());
    }
    let root = archived_mods_root(game, use_default_path)?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mod_dirs = collect_scannable_mod_dirs(&root)?;

    // Process each mod directory in parallel
    let mods: Result<Vec<ModEntry>> = mod_dirs
        .par_iter()
        .map(|path| load_mod_entry(game, path.clone(), true, scan_rabbitfx_requirement))
        .collect();

    mods
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

fn collect_scannable_mod_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut mod_dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        if mod_dir_has_payload(&path)? {
            mod_dirs.push(path);
        } else {
            cleanup_metadata_only_mod_dir(&path)?;
        }
    }
    Ok(mod_dirs)
}

fn load_mod_entry(
    game: &GameInstall,
    root_path: PathBuf,
    force_archived: bool,
    scan_rabbitfx_requirement: bool,
) -> Result<ModEntry> {
    let folder_name = root_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("invalid mod folder name"))?
        .to_string();

    let portable = persistence::load_portable_mod_state(&root_path)?;
    let selected_metadata_source = portable.as_ref().and_then(|stored| {
        stored
            .metadata
            .user
            .extracted_metadata_source_path
            .as_deref()
    });
    let extracted = extract_metadata(
        &root_path,
        selected_metadata_source,
        scan_rabbitfx_requirement,
    )?;
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

    let (content_mtime, ini_hash, content_size_bytes) = compute_mod_fingerprint(&root_path)?;

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
        content_size_bytes,
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
    let existing = state
        .mods
        .iter()
        .find(|item| item.root_path == discovered.root_path)
        .or_else(|| state.mods.iter().find(|item| item.id == discovered.id));

    if let Some(existing) = existing {
        discovered.id = existing.id.clone();
        discovered.created_at = existing.created_at;
        let has_existing_fingerprint =
            existing.content_mtime.is_some() || existing.ini_hash.is_some();
        let same_mtime = existing.content_mtime.map(|t| t.timestamp())
            == discovered.content_mtime.map(|t| t.timestamp());
        let legacy_disabled_hash = legacy_disabled_ini_hash(&discovered.root_path)
            .ok()
            .flatten();
        let legacy_disabled_hash_match = same_mtime
            && existing.ini_hash.as_deref() == legacy_disabled_hash.as_deref()
            && discovered.ini_hash.as_deref() != legacy_disabled_hash.as_deref();
        let fingerprint_changed = has_existing_fingerprint
            && !legacy_disabled_hash_match
            && (!same_mtime || existing.ini_hash != discovered.ini_hash);
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
        migrate_legacy_disabled_baseline(discovered);
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

fn mod_dir_has_payload(root: &Path) -> Result<bool> {
    if !substantive_entries(root)?.is_empty() {
        return Ok(true);
    }
    directory_has_entries(&root.join(DISABLED_CONTAINER))
}

fn directory_has_entries(root: &Path) -> Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(root)? {
        entry?;
        return Ok(true);
    }
    Ok(false)
}

fn cleanup_metadata_only_mod_dir(root: &Path) -> Result<bool> {
    if root.is_dir() && root.join(MOD_META_DIR).is_dir() && !mod_dir_has_payload(root)? {
        fs::remove_dir_all(root)?;
        return Ok(true);
    }
    Ok(false)
}

fn compute_mod_fingerprint(root: &Path) -> Result<(Option<DateTime<Utc>>, Option<String>, u64)> {
    let disabled_root = root.join(DISABLED_CONTAINER);
    let content_root = if disabled_root.exists() && substantive_entries(root)?.is_empty() {
        disabled_root.as_path()
    } else {
        root
    };
    let mut max_mtime: Option<SystemTime> = None;
    let mut hasher = Xxh3::new();
    let mut found_ini = false;
    let mut content_size_bytes = 0_u64;

    for entry in walkdir::WalkDir::new(content_root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .components()
            .any(|part| part.as_os_str() == MOD_META_DIR)
        {
            continue;
        }
        if content_root == root
            && path
                .components()
                .any(|part| part.as_os_str() == DISABLED_CONTAINER)
        {
            continue;
        }
        let metadata = entry.metadata()?;
        content_size_bytes = content_size_bytes.saturating_add(metadata.len());
        if let Ok(modified) = metadata.modified() {
            max_mtime = match max_mtime {
                Some(current) => Some(current.max(modified)),
                None => Some(modified),
            };
        }
        if is_ini_file(path) {
            found_ini = true;
            let rel = path.strip_prefix(content_root).unwrap_or(path);
            hasher.update(rel.to_string_lossy().as_bytes());
            let bytes = read_file_optimized(path)?;
            hasher.update(&bytes);
        }
    }

    let mtime = max_mtime.map(DateTime::<Utc>::from);
    let hash = if found_ini {
        Some(format!("{:016x}", hasher.finish()))
    } else {
        None
    };
    Ok((mtime, hash, content_size_bytes))
}

fn legacy_disabled_ini_hash(root: &Path) -> Result<Option<String>> {
    let disabled_root = root.join(DISABLED_CONTAINER);
    if !disabled_root.exists() || !substantive_entries(root)?.is_empty() {
        return Ok(None);
    }

    let mut hasher = Xxh3::new();
    let mut found_ini = false;
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .components()
            .any(|part| part.as_os_str() == MOD_META_DIR)
        {
            continue;
        }
        if is_ini_file(path) {
            found_ini = true;
            let rel = path.strip_prefix(root).unwrap_or(path);
            hasher.update(rel.to_string_lossy().as_bytes());
            let bytes = read_file_optimized(path)?;
            hasher.update(&bytes);
        }
    }

    Ok(found_ini.then(|| format!("{:016x}", hasher.finish())))
}

fn migrate_legacy_disabled_baseline(mod_entry: &mut ModEntry) {
    let Some(source) = mod_entry.source.as_mut() else {
        return;
    };
    let Some(baseline_hash) = source.baseline_ini_hash.as_deref() else {
        return;
    };
    if mod_entry.ini_hash.as_deref() == Some(baseline_hash) {
        return;
    }
    if source.baseline_content_mtime.map(|time| time.timestamp())
        != mod_entry.content_mtime.map(|time| time.timestamp())
    {
        return;
    }
    let Ok(Some(legacy_hash)) = legacy_disabled_ini_hash(&mod_entry.root_path) else {
        return;
    };
    if baseline_hash == legacy_hash {
        source.baseline_ini_hash = mod_entry.ini_hash.clone();
    }
}

fn is_ini_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ini"))
}

fn extract_metadata(
    root: &Path,
    selected_source_path: Option<&str>,
    scan_rabbitfx_requirement: bool,
) -> Result<ExtractedMetadata> {
    let mut description = None;
    let mut hotkeys = Vec::new();
    let mut executables = Vec::new();
    let mut readme_path = None;
    let mut requires_rabbitfx = false;
    let mut best_text_priority = 0;
    let mut best_text_index: Option<usize> = None;
    let mut selected_text_index: Option<usize> = None;
    let mut text_sources: Vec<ExtractedMetadataTextSource> = Vec::new();

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
            if ["txt", "md"].contains(&extension.as_str()) {
                let raw = fs::read_to_string(path).unwrap_or_default();
                let trimmed = raw.trim();

                if scan_rabbitfx_requirement && text_mentions_rabbitfx_requirement(trimmed) {
                    requires_rabbitfx = true;
                }

                let priority = text_metadata_priority(path);
                let relative = path
                    .strip_prefix(root)
                    .map(|relative| relative.to_string_lossy().to_string())
                    .ok();
                if !trimmed.is_empty() && !is_noise_metadata_text(trimmed) {
                    if let Some(relative) = relative.clone() {
                        let source_index = text_sources.len();
                        text_sources.push(ExtractedMetadataTextSource {
                            path: relative.clone(),
                            label: path
                                .file_name()
                                .and_then(OsStr::to_str)
                                .unwrap_or(relative.as_str())
                                .to_string(),
                            content: trimmed.to_string(),
                        });
                        if selected_source_path == Some(relative.as_str()) {
                            selected_text_index = Some(source_index);
                        }
                        if priority > best_text_priority {
                            best_text_priority = priority;
                            best_text_index = Some(source_index);
                        }
                    }
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

    let personal_note_relative = personal_note_relative_path();
    let personal_note_path = personal_note_path(root);
    if personal_note_path.exists() {
        let raw = fs::read_to_string(&personal_note_path).unwrap_or_default();
        if let Some(content) = sanitize_personal_note_content(&raw) {
            let source_index = text_sources.len();
            text_sources.push(ExtractedMetadataTextSource {
                path: personal_note_relative.clone(),
                label: "Personal Note".to_string(),
                content,
            });
            if selected_source_path == Some(personal_note_relative.as_str()) {
                selected_text_index = Some(source_index);
            }
        }
    }

    if let Some(source_index) = selected_text_index.or(best_text_index) {
        if let Some(source) = text_sources.get(source_index) {
            description = Some(source.content.clone());
            readme_path = Some(source.path.clone());
        }
    }

    hotkeys.sort();
    hotkeys.dedup();
    executables.sort();
    executables.dedup();
    text_sources.sort_by(|left, right| {
        let left_personal = left.path == personal_note_relative;
        let right_personal = right.path == personal_note_relative;
        if left_personal != right_personal {
            return left_personal.cmp(&right_personal);
        }
        text_metadata_priority(Path::new(&right.path))
            .cmp(&text_metadata_priority(Path::new(&left.path)))
            .then_with(|| {
                left.label
                    .to_ascii_lowercase()
                    .cmp(&right.label.to_ascii_lowercase())
            })
            .then_with(|| {
                left.path
                    .to_ascii_lowercase()
                    .cmp(&right.path.to_ascii_lowercase())
            })
    });

    Ok(ExtractedMetadata {
        description,
        hotkeys,
        discovered_executables: executables,
        readme_path,
        text_sources,
        requires_rabbitfx,
    })
}

fn text_metadata_priority(path: &Path) -> u8 {
    let file_name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if file_name.contains("readme")
        || file_name.contains("read me")
        || file_name.contains("read_me")
    {
        4
    } else if file_name.contains("toggle") || file_name.contains("key") {
        3
    } else if file_name.contains("credit") {
        2
    } else {
        1
    }
}

fn is_noise_metadata_text(text: &str) -> bool {
    let meaningful_chars = text.chars().filter(|ch| ch.is_alphanumeric()).count();
    meaningful_chars < 8
        && !text.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.contains(':') || trimmed.contains('=') || trimmed.contains(" - ")
        })
}

fn text_mentions_rabbitfx_requirement(text: &str) -> bool {
    let normalized = text
        .to_ascii_lowercase()
        .replace('’', "'")
        .replace('“', "\"")
        .replace('”', "\"");
    normalized.contains("rabbitfx is required")
        || normalized.contains("requires rabbitfx")
        || normalized.contains("rabbitfx required")
        || normalized.contains("if you don't have rabbitfx installed")
        || normalized.contains("if you dont have rabbitfx installed")
        || normalized.contains("install rabbitfx")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_note_sanitizer_trims_controls_and_empty_notes() {
        assert_eq!(
            sanitize_personal_note_content("  first line  \r\nsecond\tline\0  "),
            Some("first line\nsecond\tline".to_string())
        );
        assert_eq!(sanitize_personal_note_content(" \r\n\t \n"), None);
        assert_eq!(sanitize_personal_note_content("\u{200b}\u{200c}"), None);
    }

    #[test]
    fn personal_note_sanitizer_preserves_multiple_blank_lines() {
        assert_eq!(
            sanitize_personal_note_content("one\n\n\n\n\ntwo"),
            Some("one\n\n\n\n\ntwo".to_string())
        );
    }

    #[test]
    fn personal_note_is_metadata_source_last_and_selectable() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("README.txt"), "Readme content").unwrap();
        let note_dir = root.join(MOD_META_DIR);
        fs::create_dir_all(&note_dir).unwrap();
        fs::write(
            note_dir.join(PERSONAL_NOTE_FILE),
            "Personal note\nsecond line",
        )
        .unwrap();

        let note_path = personal_note_relative_path();
        let extracted = extract_metadata(root, Some(&note_path), false).unwrap();

        assert_eq!(
            extracted.description.as_deref(),
            Some("Personal note\nsecond line")
        );
        assert_eq!(extracted.readme_path.as_deref(), Some(note_path.as_str()));
        assert_eq!(
            extracted
                .text_sources
                .last()
                .map(|source| source.label.as_str()),
            Some("Personal Note")
        );
    }

    #[test]
    fn personal_note_does_not_change_mod_fingerprint() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("mod.ini"), "[TextureOverride]\nhash = abc").unwrap();
        let before = compute_mod_fingerprint(root).unwrap();

        let note_dir = root.join(MOD_META_DIR);
        fs::create_dir_all(&note_dir).unwrap();
        fs::write(note_dir.join(PERSONAL_NOTE_FILE), "Personal note").unwrap();
        let after = compute_mod_fingerprint(root).unwrap();

        assert_eq!(before, after);
    }

    #[test]
    fn metadata_only_folder_is_not_scannable_mod_payload() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let meta_dir = root.join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).unwrap();
        fs::write(meta_dir.join("metadata.json"), "{}").unwrap();

        assert!(!mod_dir_has_payload(root).unwrap());
    }

    #[test]
    fn disabled_container_counts_as_scannable_mod_payload() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let meta_dir = root.join(MOD_META_DIR);
        let disabled_dir = root.join(DISABLED_CONTAINER);
        fs::create_dir_all(&meta_dir).unwrap();
        fs::create_dir_all(&disabled_dir).unwrap();
        fs::write(meta_dir.join("metadata.json"), "{}").unwrap();
        fs::write(
            disabled_dir.join("mod.ini"),
            "[TextureOverride]\nhash = abc",
        )
        .unwrap();

        assert!(mod_dir_has_payload(root).unwrap());
    }

    #[test]
    fn cleanup_removes_metadata_only_mod_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Deleted Mod");
        let meta_dir = root.join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).unwrap();
        fs::write(meta_dir.join("metadata.json"), "{}").unwrap();

        assert!(cleanup_metadata_only_mod_dir(&root).unwrap());
        assert!(!root.exists());
    }

    #[test]
    fn scan_collection_deletes_metadata_only_mod_folders() {
        let temp = tempfile::tempdir().unwrap();
        let mods_root = temp.path();
        let orphan = mods_root.join("Deleted Mod");
        let meta_dir = orphan.join(MOD_META_DIR);
        fs::create_dir_all(&meta_dir).unwrap();
        fs::write(meta_dir.join("metadata.json"), "{}").unwrap();

        let mod_dirs = collect_scannable_mod_dirs(mods_root).unwrap();

        assert!(mod_dirs.is_empty());
        assert!(!orphan.exists());
    }

    #[test]
    fn scan_collection_preserves_empty_non_hestia_folders() {
        let temp = tempfile::tempdir().unwrap();
        let mods_root = temp.path();
        let empty = mods_root.join("Empty Folder");
        fs::create_dir_all(&empty).unwrap();

        let mod_dirs = collect_scannable_mod_dirs(mods_root).unwrap();

        assert!(mod_dirs.is_empty());
        assert!(empty.exists());
    }
}
