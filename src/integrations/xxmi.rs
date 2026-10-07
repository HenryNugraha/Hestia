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
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x00000008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x08000000;

/// A marker in the existing Hestia metadata directory identifying a directory
/// that Hestia already treats as one mod root.  The marker is needed for
/// multipart mods whose active payload has no `.ini` directly at the root.
const MOD_ROOT_MARKER: &str = "mod-root";

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
    integrations::unrealengine,
    model::{
        AppState, DISABLED_CONTAINER, DiscoveredTool, ExtractedMetadata,
        ExtractedMetadataTextSource, GameBackend, GameInstall, MOD_META_DIR, ModEntry, ModMetadata,
        ModStatus, PERSONAL_NOTE_FILE, PortableModState,
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
        // Auto-create mods directory if the backend's required executable exists but folder is missing
        if let Some(mods_path) = game.mods_path(state.static_prefs.use_default_mods_path) {
            if !mods_path.exists() {
                let vanilla_exists = game
                    .vanilla_exe_path()
                    .as_ref()
                    .is_some_and(|p| p.is_file());
                let modded_exists = match game.definition.backend {
                    GameBackend::Xxmi => state
                        .static_prefs
                        .modded_launcher_path_override
                        .as_ref()
                        .or(game.modded_exe_path_override.as_ref())
                        .is_some_and(|p| p.is_file()),
                    GameBackend::UnrealEngine => true,
                };

                if vanilla_exists && modded_exists {
                    fs::create_dir_all(&mods_path).with_context(|| {
                        format!("failed to create mod directory: {}", mods_path.display())
                    })?;
                }
            }
        }
        match game.definition.backend {
            GameBackend::Xxmi => {
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
            GameBackend::UnrealEngine => {
                newly_scanned.extend(unrealengine::scan_game_mods(
                    game,
                    state.static_prefs.use_default_mods_path,
                )?);
            }
        }
    }

    repair_duplicate_scanned_mod_ids(&mut newly_scanned, state, target_game_id);

    for discovered in &mut newly_scanned {
        // Hydrate from existing memory state to preserve non-portable flags (like update_state)
        // and ensure we don't overwrite portable flags with defaults if they aren't in the JSON yet.
        let backend = state
            .games
            .iter()
            .find(|game| game.definition.id == discovered.game_id)
            .map(|game| game.definition.backend)
            .unwrap_or_default();
        match backend {
            GameBackend::Xxmi => {
                hydrate_from_existing_state(discovered, state);
                write_portable_metadata(discovered)?;
            }
            GameBackend::UnrealEngine => {
                unrealengine::hydrate_from_existing_state(discovered, state);
                unrealengine::write_portable_metadata(discovered)?;
            }
        }
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
    let mut indices_by_id: HashMap<String, Vec<usize>> =
        HashMap::with_capacity(newly_scanned.len());
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
    move_disabled_contents_to_active(&mod_entry.root_path)?;
    let old_updated_at = mod_entry.updated_at;
    mod_entry.status = ModStatus::Active;
    mod_entry.updated_at = Utc::now();
    if let Err(err) = write_portable_metadata(mod_entry) {
        let rollback = move_active_contents_to_disabled(&mod_entry.root_path);
        mod_entry.status = ModStatus::Disabled;
        mod_entry.updated_at = old_updated_at;
        if let Err(rollback_err) = rollback {
            tracing::error!(
                "could not roll back enabled mod {} after metadata failure: {rollback_err}",
                mod_entry.root_path.display()
            );
        }
        return Err(err);
    }
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
    let live_root = game
        .mods_path(use_default_path)
        .ok_or_else(|| anyhow!("game has no live mods path"))?;
    let original_path = mod_entry.root_path.clone();
    let relative_path = original_path
        .strip_prefix(&live_root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&mod_entry.folder_name));
    fs::create_dir_all(
        archive_root.join(relative_path.parent().unwrap_or_else(|| Path::new("."))),
    )?;
    let destination = archive_root.join(&relative_path);
    if destination.exists() {
        bail!(
            "archive destination already exists: {}",
            destination.display()
        );
    }
    let original_status = mod_entry.status.clone();
    let original_updated_at = mod_entry.updated_at;
    fs::rename(&original_path, &destination)?;
    mod_entry.root_path = destination.clone();
    mod_entry.archive_original_path = Some(original_path.clone());
    mod_entry.status = ModStatus::Archived;
    mod_entry.updated_at = Utc::now();
    let metadata_result = (|| {
        write_portable_metadata(mod_entry)?;
        // Archiving is an explicit selection of this directory as one mod,
        // including legacy payloads without a direct ini.  Keep that choice
        // stable if the archive is scanned after a restart.
        mark_mod_root(&destination)
    })();
    if let Err(err) = metadata_result {
        mod_entry.root_path = original_path.clone();
        mod_entry.archive_original_path = None;
        mod_entry.status = original_status;
        mod_entry.updated_at = original_updated_at;
        if let Err(rollback_err) = fs::rename(&destination, &original_path) {
            tracing::error!(
                "could not roll back archived mod {} after metadata failure: {rollback_err}",
                destination.display()
            );
        }
        return Err(err);
    }
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
    let archived_path = mod_entry.root_path.clone();
    let archive_root = archived_mods_root(game, use_default_path)?;
    let configured_destination = live_root.join(&mod_entry.folder_name);
    let derived_destination =
        archived_path_relative_destination(&archive_root, &live_root, &archived_path);
    let destination = mod_entry
        .archive_original_path
        .as_deref()
        .filter(|path| path_is_within(&live_root, path))
        .map(Path::to_path_buf)
        .or(derived_destination)
        .unwrap_or(configured_destination);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if destination.exists() {
        bail!("live mod folder already exists: {}", destination.display());
    }

    let archived_was_disabled = archived_path.join(DISABLED_CONTAINER).is_dir();
    fs::rename(&archived_path, &destination)?;
    if let Err(err) = move_disabled_contents_to_active(&destination) {
        if let Err(rollback_err) = fs::rename(&destination, &archived_path) {
            tracing::error!(
                "could not roll back restore of {} after normalization failure: {rollback_err}",
                archived_path.display()
            );
        }
        return Err(err);
    }

    let old_updated_at = mod_entry.updated_at;
    let old_archive_original_path = mod_entry.archive_original_path.clone();
    mod_entry.root_path = destination.clone();
    mod_entry.archive_original_path = None;
    mod_entry.status = ModStatus::Active;
    mod_entry.updated_at = Utc::now();
    let metadata_result = (|| {
        write_portable_metadata(mod_entry)?;
        // Restore is also an explicit selection of the archived directory as
        // one mod, including legacy non-ini payloads.
        mark_mod_root(&destination)
    })();
    if let Err(err) = metadata_result {
        let rollback_layout = if archived_was_disabled {
            Some(move_active_contents_to_disabled(&destination))
        } else {
            None
        };
        let rollback_move = fs::rename(&destination, &archived_path);
        mod_entry.root_path = archived_path;
        mod_entry.archive_original_path = old_archive_original_path;
        mod_entry.status = ModStatus::Archived;
        mod_entry.updated_at = old_updated_at;
        if let Some(Err(rollback_err)) = rollback_layout {
            tracing::error!(
                "could not roll back restored mod layout after metadata failure: {rollback_err}"
            );
        }
        if let Err(rollback_err) = rollback_move {
            tracing::error!("could not roll back restored mod move after metadata failure: {rollback_err}");
        }
        return Err(err);
    }
    Ok(destination)
}

/// Move an XXMI mod's disabled payload into its live root.  Every destination is checked before
/// the first move, and a failed move is rolled back so callers never observe a half-enabled tree.
fn move_disabled_contents_to_active(root: &Path) -> Result<()> {
    let disabled_root = root.join(DISABLED_CONTAINER);
    if !disabled_root.is_dir() {
        return Ok(());
    }
    if !substantive_entries(root)?.is_empty() {
        bail!("cannot enable mod with content already in its live root");
    }

    // A disabled imported multipart mod exposes no direct ini while its
    // payload is in DISABLED_BY_HESTIA.  Preserve that root when enabling so
    // nested discovery does not reinterpret its component folders as mods.
    if !directory_has_direct_ini(&disabled_root)? {
        mark_mod_root(root)?;
    }

    let entries = fs::read_dir(&disabled_root)?.collect::<std::result::Result<Vec<_>, _>>()?;
    for entry in &entries {
        let destination = root.join(entry.file_name());
        if destination.exists() || entry.file_name() == OsStr::new(MOD_META_DIR) {
            bail!("cannot enable mod because destination already exists: {}", destination.display());
        }
    }

    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let source = entry.path();
        let destination = root.join(entry.file_name());
        if let Err(err) = fs::rename(&source, &destination) {
            for (source, destination) in moved.into_iter().rev() {
                if let Err(rollback_err) = fs::rename(destination, &source) {
                    tracing::error!(
                        "could not roll back XXMI enable move {}: {rollback_err}",
                        source.display()
                    );
                }
            }
            return Err(err.into());
        }
        moved.push((source, destination));
    }
    if let Err(err) = fs::remove_dir_all(&disabled_root) {
        for (source, destination) in moved.into_iter().rev() {
            if let Err(rollback_err) = fs::rename(destination, &source) {
                tracing::error!(
                    "could not roll back XXMI enable move {}: {rollback_err}",
                    source.display()
                );
            }
        }
        return Err(err.into());
    }
    Ok(())
}

fn path_is_within(root: &Path, candidate: &Path) -> bool {
    if candidate.starts_with(root) {
        return true;
    }
    #[cfg(windows)]
    {
        let root_components = root
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>();
        let candidate_components = candidate
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>();
        return candidate_components.len() >= root_components.len()
            && root_components
                .iter()
                .zip(candidate_components.iter())
                .all(|(root, candidate)| root == candidate);
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Recreate an XXMI disabled container from a live root.  This is used only to roll back a
/// successful filesystem move when writing portable metadata fails.
fn move_active_contents_to_disabled(root: &Path) -> Result<()> {
    let disabled_root = root.join(DISABLED_CONTAINER);
    fs::create_dir_all(&disabled_root)?;
    let entries = substantive_entries(root)?;
    for source in &entries {
        let name = source
            .file_name()
            .ok_or_else(|| anyhow!("mod entry missing file name"))?;
        let destination = disabled_root.join(name);
        if destination.exists() {
            bail!("cannot roll back enabled mod because destination already exists: {}", destination.display());
        }
    }
    let mut moved = Vec::with_capacity(entries.len());
    for source in entries {
        let name = source
            .file_name()
            .ok_or_else(|| anyhow!("mod entry missing file name"))?;
        let destination = disabled_root.join(name);
        if let Err(err) = fs::rename(&source, &destination) {
            for (source, destination) in moved.into_iter().rev() {
                let _ = fs::rename(destination, source);
            }
            return Err(err.into());
        }
        moved.push((source, destination));
    }
    Ok(())
}

pub fn send_to_recycle_bin(mod_entry: &ModEntry) -> Result<()> {
    recycle_path(&mod_entry.root_path)
}

/// Send `path` to the recycle bin, retrying briefly and then falling back to
/// the native shell operation on Windows.
///
/// `trash` collapses every shell-side refusal into a bare "Some operations
/// were aborted": a file the game still holds open, a volume whose recycle bin
/// is disabled or too small for the folder, an over-long path. A single
/// attempt is therefore never a reliable verdict, and callers that can keep
/// working without the bin should treat a failure here as non-fatal.
pub fn recycle_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let mut trash_err: Option<anyhow::Error> = None;
    for delay_ms in [150_u64, 400, 900] {
        match trash::delete(path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                if !path.exists() {
                    return Ok(());
                }
                if cleanup_metadata_only_mod_dir(path)? {
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
            match shell_recycle_delete(path) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    if !path.exists() {
                        return Ok(());
                    }
                    if cleanup_metadata_only_mod_dir(path)? {
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

    launch_executable_with_args(path, &arg_refs, true, "tool")
}

pub fn launch_vanilla_executable(path: &Path) -> Result<()> {
    launch_executable_with_args(path, &[], true, "vanilla executable")
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

pub(crate) fn scan_live_mods(
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
    let live_root = game
        .mods_path(use_default_path)
        .ok_or_else(|| anyhow!("game has no live mods path"))?;

    // Process each mod directory in parallel
    let mods: Result<Vec<ModEntry>> = mod_dirs
        .par_iter()
        .map(|path| {
            let mut mod_entry =
                load_mod_entry(game, path.clone(), true, scan_rabbitfx_requirement)?;
            mod_entry.archive_original_path =
                archived_path_relative_destination(&root, &live_root, path);
            Ok(mod_entry)
        })
        .collect();

    mods
}

pub(crate) fn archived_mods_root(game: &GameInstall, use_default_path: bool) -> Result<PathBuf> {
    let live_root = game
        .mods_path(use_default_path)
        .ok_or_else(|| anyhow!("game has no live mods path"))?;
    let parent = live_root
        .parent()
        .ok_or_else(|| anyhow!("invalid game mods path"))?;
    Ok(parent.join("Mods_Archived"))
}

fn archived_path_relative_destination(
    archive_root: &Path,
    live_root: &Path,
    archived_path: &Path,
) -> Option<PathBuf> {
    let relative = archived_path
        .strip_prefix(archive_root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())?;
    Some(live_root.join(relative))
}

/// Scratch folders an install leaves behind when it is interrupted or when the
/// old folder could not be disposed of. They still hold a full mod payload, so
/// without this they would scan as duplicate mods — and 3dmigoto would load
/// them alongside the real one.
fn install_scratch_kind(path: &Path) -> Option<InstallScratch> {
    let name = path.file_name().and_then(OsStr::to_str)?;
    if name.starts_with(".hestia_old_") || name.starts_with(".hestia-retired-") {
        Some(InstallScratch::Retired)
    } else if name.starts_with(".hestia_tmp_") || name.starts_with(".hestia-install-") {
        Some(InstallScratch::Staging)
    } else {
        None
    }
}

enum InstallScratch {
    /// The old payload of a Replace install. Its matching stage may still
    /// need it for rollback; disposal can finish once that stage is gone.
    Retired,
    /// An install may still be copying into it right now, so only sweep it once
    /// it has clearly been abandoned.
    Staging,
}

const STALE_STAGING_DIR_AGE: Duration = Duration::from_secs(6 * 60 * 60);

fn staging_dir_is_abandoned(path: &Path) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|meta| meta.modified()) else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age >= STALE_STAGING_DIR_AGE)
}

fn retired_stage_sibling(path: &Path) -> Option<PathBuf> {
    let name = path.file_name().and_then(OsStr::to_str)?;
    let stage_name = name.strip_prefix(".hestia-retired-")?;
    if !stage_name.starts_with(".hestia-install-") {
        return None;
    }
    Some(path.parent()?.join(stage_name))
}

fn retired_install_has_live_stage(path: &Path) -> bool {
    retired_stage_sibling(path)
        .is_some_and(|stage| stage.is_dir() && !path_is_link(&stage))
}

fn collect_scannable_mod_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut mod_dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry_is_link(&entry)? {
            continue;
        }
        let path = entry.path();
        if !entry.file_type()?.is_dir() || is_scan_helper_dir(&path) {
            continue;
        }
        if skip_install_scratch_dir(&path) {
            continue;
        }

        let mut explicit_roots = Vec::new();
        collect_explicit_mod_roots(&path, &mut explicit_roots)?;
        if explicit_roots.is_empty() {
            // Legacy XXMI mods did not require an ini at the root.  Keep the
            // entire top-level directory together when no stronger nested
            // boundaries identify child mods.
            if has_real_scan_payload(&path)? {
                mod_dirs.push(path);
            } else {
                cleanup_metadata_only_mod_dir(&path)?;
            }
        } else {
            mod_dirs.extend(explicit_roots);
        }
    }
    Ok(mod_dirs)
}

/// Find boundaries that identify actual mod roots.  A directory with no
/// direct ini is an organizational container, so its non-ini child folders
/// are not independently treated as mods merely because they contain files.
fn collect_explicit_mod_roots(path: &Path, roots: &mut Vec<PathBuf>) -> Result<()> {
    if is_scan_helper_dir(path) || skip_install_scratch_dir(path) || path_is_link(path) {
        return Ok(());
    }
    if managed_mod_root(path)? || directory_has_direct_ini(path)? || has_disabled_container(path) {
        if has_real_scan_payload(path)? {
            roots.push(path.to_path_buf());
        } else {
            cleanup_metadata_only_mod_dir(path)?;
        }
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry_is_link(&entry)? || !entry.file_type()?.is_dir() {
            continue;
        }
        let child = entry.path();
        if is_scan_helper_dir(&child) || skip_install_scratch_dir(&child) {
            continue;
        }
        collect_explicit_mod_roots(&child, roots)?;
    }
    Ok(())
}

fn is_scan_helper_dir(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        name == OsStr::new(MOD_META_DIR) || name == OsStr::new(DISABLED_CONTAINER)
    })
}

fn has_disabled_container(path: &Path) -> bool {
    let disabled = path.join(DISABLED_CONTAINER);
    disabled.is_dir() && !path_is_link(&disabled)
}

fn directory_has_direct_ini(path: &Path) -> Result<bool> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry_is_link(&entry)? {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_file() && is_ini_file(&entry.path()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn path_is_link(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return true;
    };
    metadata.file_type().is_symlink()
}

fn entry_is_link(entry: &fs::DirEntry) -> Result<bool> {
    Ok(entry.file_type()?.is_symlink())
}

fn managed_mod_root(path: &Path) -> Result<bool> {
    let marker = path.join(MOD_META_DIR).join(MOD_ROOT_MARKER);
    Ok(!path_is_link(&marker) && marker.is_file())
}

fn has_real_scan_payload(root: &Path) -> Result<bool> {
    if !root.is_dir() || path_is_link(root) {
        return Ok(false);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry_is_link(&entry)? {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        if name == OsStr::new(DISABLED_CONTAINER) {
            if has_real_scan_payload(&path)? {
                return Ok(true);
            }
            continue;
        }
        if is_scan_helper_dir(&path) || install_scratch_kind(&path).is_some() {
            continue;
        }
        if entry.file_type()?.is_file()
            || (entry.file_type()?.is_dir() && has_real_scan_payload(&path)?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn skip_install_scratch_dir(path: &Path) -> bool {
    match install_scratch_kind(path) {
        Some(InstallScratch::Retired) => {
            if !retired_install_has_live_stage(path) {
                let _ = fs::remove_dir_all(path);
            }
            true
        }
        Some(InstallScratch::Staging) => {
            if staging_dir_is_abandoned(path) {
                let _ = fs::remove_dir_all(path);
            }
            true
        }
        None => false,
    }
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

    let (created_at, updated_at, unsafe_content, unsafe_content_auto, unsafe_content_preference) =
        match &portable {
            Some(stored) => {
                let auto_unsafe = derive_unsafe_content_from_portable(stored)
                    .or(stored.unsafe_content_auto)
                    .unwrap_or(stored.unsafe_content);
                (
                    stored.created_at.unwrap_or_else(Utc::now),
                    stored.updated_at.unwrap_or_else(Utc::now),
                    stored.unsafe_content_preference.resolve(auto_unsafe),
                    auto_unsafe,
                    stored.unsafe_content_preference,
                )
            }
            None => (Utc::now(), Utc::now(), false, false, Default::default()),
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
        unsafe_content_auto,
        unsafe_content_preference,
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
        if existing.archive_original_path.is_some() || discovered.archive_original_path.is_none() {
            discovered.archive_original_path = existing.archive_original_path.clone();
        }
        discovered.unsafe_content_auto = existing.unsafe_content_auto;
        discovered.unsafe_content_preference = existing.unsafe_content_preference;
        discovered.unsafe_content = existing.unsafe_content;
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
        unsafe_content_auto: Some(mod_entry.unsafe_content_auto),
        unsafe_content_preference: mod_entry.unsafe_content_preference,
        created_at: Some(mod_entry.created_at),
        updated_at: Some(mod_entry.updated_at),
    };
    persistence::save_portable_mod_state(&mod_entry.root_path, &portable)?;
    if managed_mod_root(&mod_entry.root_path)?
        || directory_has_direct_ini(&mod_entry.root_path)?
        || has_disabled_container(&mod_entry.root_path)
    {
        mark_mod_root(&mod_entry.root_path)?;
    }
    Ok(())
}

/// Mark a completed XXMI payload as one managed mod root.  This is separate
/// from the portable JSON because a multipart root can have no direct `.ini`,
/// while legacy container metadata must remain insufficient to suppress
/// nested discovery.
pub(crate) fn mark_mod_root(root: &Path) -> Result<()> {
    let metadata_dir = root.join(MOD_META_DIR);
    fs::create_dir_all(&metadata_dir)?;
    let marker = metadata_dir.join(MOD_ROOT_MARKER);
    if !marker.exists() {
        fs::write(marker, b"managed mod root\n")?;
    }
    Ok(())
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
    if let Some(entry) = fs::read_dir(root)?.next() {
        entry?;
        return Ok(true);
    }
    Ok(false)
}

fn cleanup_metadata_only_mod_dir(root: &Path) -> Result<bool> {
    if root.is_dir()
        && root.join(MOD_META_DIR).is_dir()
        && !mod_dir_has_payload(root)?
        && !has_install_scratch_child(root)?
    {
        fs::remove_dir_all(root)?;
        return Ok(true);
    }
    Ok(false)
}

fn has_install_scratch_child(root: &Path) -> Result<bool> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry_is_link(&entry)? {
            continue;
        }
        if entry.file_type()?.is_dir() && install_scratch_kind(&entry.path()).is_some() {
            return Ok(true);
        }
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
                if !trimmed.is_empty()
                    && !is_noise_metadata_text(trimmed)
                    && !is_generated_text_payload(path, trimmed)
                {
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

fn is_generated_text_payload(path: &Path, text: &str) -> bool {
    has_generated_text_name(path) && text.lines().take(80).any(is_shader_payload_line)
}

fn has_generated_text_name(path: &Path) -> bool {
    path.file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .split(['-', '_', '.', ' '])
        .any(|part| part.len() >= 12 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn is_shader_payload_line(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("//")
        || line.starts_with("ps_")
        || line.starts_with("vs_")
        || line.starts_with("dcl_")
        || line.starts_with("def ")
        || line.starts_with("mov ")
        || line.starts_with("mul ")
        || line.starts_with("texld")
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
    use crate::model::GameDefinition;

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
    fn generated_shader_text_is_not_metadata_source() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(
            root.join("f86e7a367fd097f0-ps_replace.txt"),
            "// Resource replacement\nps_5_0\ndef c0, 1, 0, 0, 0",
        )
        .unwrap();

        let extracted = extract_metadata(root, None, false).unwrap();

        assert!(extracted.description.is_none());
        assert!(extracted.text_sources.is_empty());
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
    fn scan_collection_ignores_top_level_hestia_helper_folder() {
        let temp = tempfile::tempdir().unwrap();
        let mods_root = temp.path();
        let helper = mods_root.join(MOD_META_DIR);
        fs::create_dir_all(&helper).unwrap();
        fs::write(helper.join("hestia.ini"), "[Constants]\n").unwrap();

        let mod_dirs = collect_scannable_mod_dirs(mods_root).unwrap();

        assert!(mod_dirs.is_empty());
        assert!(helper.exists());
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

    #[test]
    fn scan_collection_discovers_nested_ini_mods_without_splitting_loose_files() {
        let temp = tempfile::tempdir().unwrap();
        let mods_root = temp.path();
        fs::create_dir_all(mods_root.join("Ardelia").join("Outfit 1")).unwrap();
        fs::create_dir_all(mods_root.join("Ardelia").join("Outfit 2")).unwrap();
        fs::write(
            mods_root
                .join("Ardelia")
                .join("Outfit 1")
                .join("mod.INI"),
            "[TextureOverride]\n",
        )
        .unwrap();
        fs::write(
            mods_root
                .join("Ardelia")
                .join("Outfit 2")
                .join("mod.ini"),
            "[TextureOverride]\n",
        )
        .unwrap();
        fs::create_dir_all(mods_root.join("Ardelia").join("images")).unwrap();
        fs::write(
            mods_root.join("Ardelia").join("images").join("cover.png"),
            b"cover",
        )
        .unwrap();

        let mut mod_dirs = collect_scannable_mod_dirs(mods_root).unwrap();
        mod_dirs.sort();

        assert_eq!(
            mod_dirs,
            vec![
                mods_root.join("Ardelia").join("Outfit 1"),
                mods_root.join("Ardelia").join("Outfit 2"),
            ]
        );
    }

    #[test]
    fn scan_collection_stops_at_direct_ini_mod_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Multipart");
        fs::create_dir_all(root.join("components")).unwrap();
        fs::write(root.join("main.ini"), "[TextureOverride]\n").unwrap();
        fs::write(root.join("components").join("part.ini"), "[TextureOverride]\n").unwrap();

        assert_eq!(collect_scannable_mod_dirs(temp.path()).unwrap(), vec![root]);
    }

    #[test]
    fn scan_collection_handles_multiple_nesting_and_disabled_children() {
        let temp = tempfile::tempdir().unwrap();
        let active = temp
            .path()
            .join("Characters")
            .join("Ardelia")
            .join("Outfit 1");
        let disabled = temp
            .path()
            .join("Characters")
            .join("Ardelia")
            .join("Outfit 2")
            .join(DISABLED_CONTAINER);
        fs::create_dir_all(&active).unwrap();
        fs::create_dir_all(&disabled).unwrap();
        fs::write(active.join("mod.ini"), "[TextureOverride]\n").unwrap();
        fs::write(disabled.join("mod.ini"), "[TextureOverride]\n").unwrap();

        let mut mod_dirs = collect_scannable_mod_dirs(temp.path()).unwrap();
        mod_dirs.sort();

        assert_eq!(
            mod_dirs,
            vec![
                active,
                temp.path()
                    .join("Characters")
                    .join("Ardelia")
                    .join("Outfit 2"),
            ]
        );
    }

    #[test]
    fn scan_collection_preserves_marked_multipart_root_and_ignores_nested_scratch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Multipart");
        fs::create_dir_all(root.join(MOD_META_DIR)).unwrap();
        fs::write(
            root.join(MOD_META_DIR).join(MOD_ROOT_MARKER),
            b"managed mod root\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("components")).unwrap();
        fs::write(root.join("components").join("part.ini"), "[TextureOverride]\n").unwrap();
        fs::create_dir_all(root.join(".hestia-install-live")).unwrap();
        fs::write(
            root.join(".hestia-install-live").join("scratch.ini"),
            "[TextureOverride]\n",
        )
        .unwrap();

        assert_eq!(collect_scannable_mod_dirs(temp.path()).unwrap(), vec![root]);
    }

    #[test]
    fn scan_collection_does_not_promote_scratch_or_helpers_to_grouping_payload() {
        let temp = tempfile::tempdir().unwrap();
        let grouping = temp.path().join("Ardelia");
        let actual_mod = grouping.join("Actual Mod");
        fs::create_dir_all(&actual_mod).unwrap();
        fs::write(actual_mod.join("mod.ini"), "[TextureOverride]\n").unwrap();
        fs::create_dir_all(grouping.join(".hestia-install-live")).unwrap();
        fs::write(
            grouping.join(".hestia-install-live").join("scratch.ini"),
            "[TextureOverride]\n",
        )
        .unwrap();
        fs::create_dir_all(grouping.join("Nested").join(".hestia-install-live")).unwrap();
        fs::write(
            grouping
                .join("Nested")
                .join(".hestia-install-live")
                .join("scratch.ini"),
            "[TextureOverride]\n",
        )
        .unwrap();
        let live_retired = grouping.join(".hestia-retired-.hestia-install-live");
        fs::create_dir_all(&live_retired).unwrap();
        fs::write(live_retired.join("retired.ini"), "[TextureOverride]\n").unwrap();
        let orphan_retired = grouping.join(".hestia-retired-after-failed-dispose");
        fs::create_dir_all(&orphan_retired).unwrap();
        fs::write(orphan_retired.join("retired.ini"), "[TextureOverride]\n").unwrap();
        fs::create_dir_all(grouping.join(MOD_META_DIR)).unwrap();
        fs::write(
            grouping.join(MOD_META_DIR).join("old-metadata.ini"),
            "[Constants]\n",
        )
        .unwrap();

        let mod_dirs = collect_scannable_mod_dirs(temp.path()).unwrap();

        assert_eq!(mod_dirs, vec![actual_mod]);
        assert!(grouping.exists());
        assert!(live_retired.exists());
        assert!(!orphan_retired.exists());
    }

    #[test]
    fn legacy_non_ini_fallback_does_not_block_later_nested_discovery() {
        let temp = tempfile::tempdir().unwrap();
        let grouping = temp.path().join("Ardelia");
        fs::create_dir_all(&grouping).unwrap();
        fs::write(grouping.join("cover.png"), b"cover").unwrap();

        let entry = archive_test_entry(&grouping, ModStatus::Active);
        write_portable_metadata(&entry).unwrap();
        assert!(!grouping.join(MOD_META_DIR).join(MOD_ROOT_MARKER).exists());

        let child = grouping.join("Actual Mod");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("mod.ini"), "[TextureOverride]\n").unwrap();

        assert_eq!(collect_scannable_mod_dirs(temp.path()).unwrap(), vec![child]);
    }

    #[test]
    fn enabling_disabled_multipart_root_keeps_its_managed_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Imported Multipart");
        fs::create_dir_all(root.join("Part A")).unwrap();
        fs::create_dir_all(root.join("Part B")).unwrap();
        fs::write(root.join("Part A").join("part.ini"), "[TextureOverride]\n").unwrap();
        fs::write(root.join("Part B").join("part.ini"), "[TextureOverride]\n").unwrap();
        let mut entry = archive_test_entry(&root, ModStatus::Active);

        disable_mod(&mut entry).unwrap();
        assert_eq!(entry.status, ModStatus::Disabled);
        enable_mod(&mut entry).unwrap();
        assert_eq!(entry.status, ModStatus::Active);

        assert_eq!(collect_scannable_mod_dirs(temp.path()).unwrap(), vec![root]);
    }

    #[cfg(unix)]
    #[test]
    fn scan_collection_ignores_symlinked_mod_directories() {
        let temp = tempfile::tempdir().unwrap();
        let actual = temp.path().join("Actual");
        let link = temp.path().join("Linked");
        fs::create_dir_all(&actual).unwrap();
        fs::write(actual.join("mod.ini"), "[TextureOverride]\n").unwrap();

        std::os::unix::fs::symlink(&actual, &link).unwrap();

        let mod_dirs = collect_scannable_mod_dirs(temp.path()).unwrap();
        assert_eq!(mod_dirs, vec![actual]);
    }

    fn archive_test_game(mods_path: &Path) -> GameInstall {
        GameInstall {
            definition: GameDefinition {
                id: "archive-test".to_string(),
                name: "Archive Test".to_string(),
                backend: GameBackend::Xxmi,
                xxmi_code: "archive-test".to_string(),
            },
            mods_path_override: Some(mods_path.to_path_buf()),
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: false,
            enabled: true,
        }
    }

    fn archive_test_entry(root: &Path, status: ModStatus) -> ModEntry {
        ModEntry {
            id: "archive-test-mod".to_string(),
            game_id: "archive-test".to_string(),
            folder_name: "Test Mod".to_string(),
            root_path: root.to_path_buf(),
            status,
            metadata: ModMetadata::default(),
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
            source: None,
            update_state: crate::model::ModUpdateState::Unlinked,
        }
    }

    #[test]
    fn restore_archived_disabled_xxmi_payload_as_active_layout() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let root = mods.join("Test Mod");
        let disabled = root.join(DISABLED_CONTAINER);
        fs::create_dir_all(&disabled).unwrap();
        fs::write(disabled.join("mod.ini"), "[TextureOverride]\nhash = abc").unwrap();
        let game = archive_test_game(&mods);
        let mut entry = archive_test_entry(&root, ModStatus::Disabled);

        archive_mod(&mut entry, &game, false).unwrap();
        let original_path = entry.archive_original_path.clone().unwrap();
        restore_mod(&mut entry, &game, false).unwrap();

        assert_eq!(entry.status, ModStatus::Active);
        assert_eq!(entry.root_path, original_path);
        assert!(entry.archive_original_path.is_none());
        assert!(entry.root_path.join("mod.ini").is_file());
        assert!(!entry.root_path.join(DISABLED_CONTAINER).exists());
        assert!(entry.root_path.join(MOD_META_DIR).is_dir());
    }

    #[test]
    fn restore_archived_active_xxmi_payload_stays_active() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let root = mods.join("Test Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("mod.ini"), "[TextureOverride]\nhash = abc").unwrap();
        let game = archive_test_game(&mods);
        let mut entry = archive_test_entry(&root, ModStatus::Active);

        archive_mod(&mut entry, &game, false).unwrap();
        restore_mod(&mut entry, &game, false).unwrap();

        assert_eq!(entry.status, ModStatus::Active);
        assert!(entry.root_path.join("mod.ini").is_file());
        assert!(!entry.root_path.join(DISABLED_CONTAINER).exists());
        assert!(entry.archive_original_path.is_none());
    }

    #[test]
    fn restore_uses_current_live_root_when_archive_original_path_is_stale() {
        let temp = tempfile::tempdir().unwrap();
        let old_mods = temp.path().join("OldGame").join("Mods");
        let new_mods = temp.path().join("NewGame").join("Mods");
        let root = old_mods.join("Test Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("mod.ini"), "[TextureOverride]\nhash = abc").unwrap();
        let old_game = archive_test_game(&old_mods);
        let mut entry = archive_test_entry(&root, ModStatus::Active);
        archive_mod(&mut entry, &old_game, false).unwrap();

        let new_game = archive_test_game(&new_mods);
        let restored = restore_mod(&mut entry, &new_game, false).unwrap();

        assert_eq!(restored, new_mods.join("Test Mod"));
        assert!(restored.join("mod.ini").is_file());
        assert!(!old_mods.join("Test Mod").exists());
        assert!(entry.archive_original_path.is_none());
    }

    #[test]
    fn archive_and_restore_preserve_nested_relative_paths_after_rescan() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let root = mods.join("Ardelia").join("Same");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("mod.ini"), "[TextureOverride]\nhash = abc").unwrap();
        let game = archive_test_game(&mods);
        let mut entry = archive_test_entry(&root, ModStatus::Active);

        let archived = archive_mod(&mut entry, &game, false).unwrap();
        assert_eq!(
            archived,
            temp.path()
                .join("Mods_Archived")
                .join("Ardelia")
                .join("Same")
        );
        assert!(!root.exists());

        let mut rescanned = scan_archived_mods(&game, false, false).unwrap();
        assert_eq!(rescanned.len(), 1);
        assert_eq!(
            rescanned[0].archive_original_path,
            Some(root.clone())
        );
        rescanned[0].archive_original_path = None;
        let restored = restore_mod(&mut rescanned.remove(0), &game, false).unwrap();

        assert_eq!(restored, root);
        assert!(restored.join("mod.ini").is_file());
    }

    #[test]
    fn archive_duplicate_folder_names_keep_category_paths() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let first_root = mods.join("Ardelia").join("Same");
        let second_root = mods.join("Beatrice").join("Same");
        fs::create_dir_all(&first_root).unwrap();
        fs::create_dir_all(&second_root).unwrap();
        fs::write(first_root.join("mod.ini"), "[TextureOverride]\nhash = first").unwrap();
        fs::write(second_root.join("mod.ini"), "[TextureOverride]\nhash = second").unwrap();
        let game = archive_test_game(&mods);
        let mut first = archive_test_entry(&first_root, ModStatus::Active);
        let mut second = archive_test_entry(&second_root, ModStatus::Active);

        archive_mod(&mut first, &game, false).unwrap();
        archive_mod(&mut second, &game, false).unwrap();
        assert!(
            temp.path()
                .join("Mods_Archived")
                .join("Ardelia")
                .join("Same")
                .is_dir()
        );
        assert!(
            temp.path()
                .join("Mods_Archived")
                .join("Beatrice")
                .join("Same")
                .is_dir()
        );

        first.archive_original_path = None;
        second.archive_original_path = None;
        assert_eq!(restore_mod(&mut first, &game, false).unwrap(), first_root);
        assert_eq!(restore_mod(&mut second, &game, false).unwrap(), second_root);
    }

    #[test]
    fn restore_collision_and_layout_failure_preserve_archive_state() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let root = mods.join("Test Mod");
        let disabled = root.join(DISABLED_CONTAINER);
        fs::create_dir_all(&disabled).unwrap();
        fs::write(disabled.join("mod.ini"), "[TextureOverride]\nhash = abc").unwrap();
        let game = archive_test_game(&mods);
        let mut entry = archive_test_entry(&root, ModStatus::Disabled);
        archive_mod(&mut entry, &game, false).unwrap();
        let archived_path = entry.root_path.clone();
        let original_path = entry.archive_original_path.clone();

        fs::create_dir_all(&root).unwrap();
        assert!(restore_mod(&mut entry, &game, false).is_err());
        assert_eq!(entry.status, ModStatus::Archived);
        assert_eq!(entry.root_path, archived_path);
        assert_eq!(entry.archive_original_path, original_path);
        fs::remove_dir_all(&root).unwrap();

        // A conflicting live payload inside the archived tree must not be partially normalized.
        fs::write(archived_path.join("mod.ini"), "conflict").unwrap();
        assert!(restore_mod(&mut entry, &game, false).is_err());
        assert_eq!(entry.status, ModStatus::Archived);
        assert_eq!(entry.root_path, archived_path);
        assert_eq!(entry.archive_original_path, original_path);
        assert!(archived_path.join(DISABLED_CONTAINER).join("mod.ini").is_file());
        assert!(archived_path.join("mod.ini").is_file());
    }
}
