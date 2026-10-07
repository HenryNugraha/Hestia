use std::{
    ffi::OsStr,
    fs,
    hash::Hasher,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
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

use crate::{
    model::{
        AppState, GameInstall, MOD_META_DIR, ModEntry, ModMetadata, ModStatus, PortableModState,
    },
    persistence,
};

const LEGACY_UNREAL_DISABLED_MODS_DIR: &str = "~mods-disabled";
const MOD_ROOT_MARKER: &str = "mod-root";
const STALE_STAGING_DIR_AGE: Duration = Duration::from_secs(6 * 60 * 60);

pub fn scan_game_mods(game: &GameInstall, use_default_path: bool) -> Result<Vec<ModEntry>> {
    let mut mods = Vec::new();
    let active_root = game.mods_path(use_default_path);
    let disabled_root = game.disabled_mods_path(use_default_path);

    if let (Some(active_root), Some(disabled_root)) = (&active_root, &disabled_root) {
        migrate_legacy_disabled_mods(active_root, disabled_root)?;
    }

    if let Some(root) = active_root {
        mods.extend(scan_root(game, &root, ModStatus::Active)?);
    }
    if let Some(root) = disabled_root {
        mods.extend(scan_root(game, &root, ModStatus::Disabled)?);
    }
    Ok(mods)
}

pub fn launch_game(game: &GameInstall) -> Result<()> {
    let command = resolve_launch_command(game)?;
    launch_unreal_command(&command)
}

pub fn disable_mod(
    mod_entry: &mut ModEntry,
    game: &GameInstall,
    use_default_path: bool,
) -> Result<()> {
    if mod_entry.status != ModStatus::Active {
        bail!("only active mods can be disabled");
    }
    let disabled_root = game
        .disabled_mods_path(use_default_path)
        .ok_or_else(|| anyhow!("disabled mods path is not configured"))?;
    let active_root = game
        .mods_path(use_default_path)
        .ok_or_else(|| anyhow!("mods path is not configured"))?;
    let relative = relative_mod_path(&active_root, &mod_entry.root_path, &mod_entry.folder_name);
    let target = next_available_relative_mod_path(&disabled_root, &relative)?;
    fs::rename(&mod_entry.root_path, &target)
        .with_context(|| format!("failed to move mod to {}", target.display()))?;
    mod_entry.root_path = target;
    mod_entry.folder_name = relative_file_name(&mod_entry.root_path)?;
    mod_entry.status = ModStatus::Disabled;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
    mark_mod_root(&mod_entry.root_path)?;
    Ok(())
}

pub fn enable_mod(
    mod_entry: &mut ModEntry,
    game: &GameInstall,
    use_default_path: bool,
) -> Result<()> {
    if mod_entry.status != ModStatus::Disabled {
        bail!("only disabled mods can be enabled");
    }
    let active_root = game
        .mods_path(use_default_path)
        .ok_or_else(|| anyhow!("mods path is not configured"))?;
    let disabled_root = game
        .disabled_mods_path(use_default_path)
        .ok_or_else(|| anyhow!("disabled mods path is not configured"))?;
    let relative = relative_mod_path(&disabled_root, &mod_entry.root_path, &mod_entry.folder_name);
    let target = next_available_relative_mod_path(&active_root, &relative)?;
    fs::rename(&mod_entry.root_path, &target)
        .with_context(|| format!("failed to move mod to {}", target.display()))?;
    mod_entry.root_path = target;
    mod_entry.folder_name = relative_file_name(&mod_entry.root_path)?;
    mod_entry.status = ModStatus::Active;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
    mark_mod_root(&mod_entry.root_path)?;
    Ok(())
}

fn scan_root(game: &GameInstall, root: &Path, status: ModStatus) -> Result<Vec<ModEntry>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
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
            if has_real_scan_payload(&path)? {
                explicit_roots.push(path);
            } else {
                cleanup_metadata_only_mod_dir(&path)?;
            }
        }
        for mod_path in explicit_roots {
            mods.push(scan_mod_dir(game, mod_path, status.clone())?);
        }
    }
    Ok(mods)
}

fn is_package_file(path: &Path) -> bool {
    path.extension().and_then(OsStr::to_str).is_some_and(|ext| {
        ext.eq_ignore_ascii_case("pak")
            || ext.eq_ignore_ascii_case("utoc")
            || ext.eq_ignore_ascii_case("ucas")
    })
}

fn is_scan_helper_dir(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == OsStr::new(MOD_META_DIR))
}

fn install_scratch_kind(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            name.starts_with(".hestia-install-")
                || name.starts_with(".hestia_tmp_")
                || name.starts_with(".hestia_old_")
                || name.starts_with(".hestia-retired-")
        })
}

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

fn is_retired_install_scratch(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            name.starts_with(".hestia_old_") || name.starts_with(".hestia-retired-")
        })
}

fn skip_install_scratch_dir(path: &Path) -> bool {
    if !install_scratch_kind(path) {
        return false;
    }
    if is_retired_install_scratch(path) {
        if !retired_install_has_live_stage(path) {
            let _ = fs::remove_dir_all(path);
        }
    } else if staging_dir_is_abandoned(path) {
        let _ = fs::remove_dir_all(path);
    }
    true
}

fn path_is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(true)
}

fn entry_is_link(entry: &fs::DirEntry) -> Result<bool> {
    Ok(entry.file_type()?.is_symlink())
}

fn managed_mod_root(path: &Path) -> bool {
    let marker = path.join(MOD_META_DIR).join(MOD_ROOT_MARKER);
    !path_is_link(&marker) && marker.is_file()
}

fn directory_has_direct_package(path: &Path) -> Result<bool> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry_is_link(&entry)? {
            continue;
        }
        if entry.file_type()?.is_file() && is_package_file(&entry.path()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn collect_explicit_mod_roots(path: &Path, roots: &mut Vec<PathBuf>) -> Result<()> {
    if is_scan_helper_dir(path) || skip_install_scratch_dir(path) || path_is_link(path) {
        return Ok(());
    }
    if managed_mod_root(path) || directory_has_direct_package(path)? {
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
        if is_scan_helper_dir(&path) || skip_install_scratch_dir(&path) {
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

fn scan_mod_dir(game: &GameInstall, root_path: PathBuf, status: ModStatus) -> Result<ModEntry> {
    let folder_name = root_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("invalid mod folder name"))?
        .to_string();
    let portable = persistence::load_portable_mod_state(&root_path)?;
    let metadata = match &portable {
        Some(stored) => ModMetadata {
            extracted: Default::default(),
            user: stored.metadata.user.clone(),
            prompt_for_missing_metadata: stored.metadata.prompt_for_missing_metadata,
        },
        None => ModMetadata {
            extracted: Default::default(),
            user: Default::default(),
            prompt_for_missing_metadata: true,
        },
    };
    let id = portable
        .as_ref()
        .map(|stored| stored.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let (content_mtime, content_hash, content_size_bytes) = compute_mod_fingerprint(&root_path)?;
    let (created_at, updated_at, unsafe_content, unsafe_content_auto, unsafe_content_preference) =
        match &portable {
            Some(stored) => {
                let auto_unsafe = stored.unsafe_content_auto.unwrap_or(stored.unsafe_content);
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
        root_path,
        status,
        metadata,
        discovered_tools: Vec::new(),
        archive_original_path: None,
        created_at,
        updated_at,
        content_mtime,
        ini_hash: content_hash,
        content_size_bytes,
        unsafe_content,
        unsafe_content_auto,
        unsafe_content_preference,
        source: portable.as_ref().and_then(|stored| stored.source.clone()),
        update_state: crate::model::ModUpdateState::Unlinked,
    })
}

fn migrate_legacy_disabled_mods(active_root: &Path, disabled_root: &Path) -> Result<()> {
    let Some(legacy_root) = active_root
        .parent()
        .map(|parent| parent.join(LEGACY_UNREAL_DISABLED_MODS_DIR))
    else {
        return Ok(());
    };
    if legacy_root == disabled_root || !legacy_root.is_dir() {
        return Ok(());
    }

    let mut moved_any = false;
    for entry in fs::read_dir(&legacy_root)
        .with_context(|| format!("failed to read {}", legacy_root.display()))?
    {
        let entry = entry?;
        if entry_is_link(&entry)? {
            continue;
        }
        let path = entry.path();
        if !entry.file_type()?.is_dir()
            || is_scan_helper_dir(&path)
            || skip_install_scratch_dir(&path)
        {
            continue;
        }
        let Some(folder_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        fs::create_dir_all(disabled_root)
            .with_context(|| format!("failed to create {}", disabled_root.display()))?;
        let target = next_available_mod_path(disabled_root, folder_name);
        fs::rename(&path, &target).with_context(|| {
            format!("failed to move legacy disabled mod to {}", target.display())
        })?;
        moved_any = true;
    }

    if moved_any && directory_is_empty(&legacy_root)? {
        fs::remove_dir(&legacy_root)
            .with_context(|| format!("failed to remove {}", legacy_root.display()))?;
    }
    Ok(())
}

fn directory_is_empty(root: &Path) -> Result<bool> {
    Ok(fs::read_dir(root)?.next().transpose()?.is_none())
}

pub fn hydrate_from_existing_state(discovered: &mut ModEntry, state: &AppState) {
    let existing = state
        .mods
        .iter()
        .find(|item| item.root_path == discovered.root_path)
        .or_else(|| state.mods.iter().find(|item| item.id == discovered.id));

    if let Some(existing) = existing {
        discovered.id = existing.id.clone();
        discovered.created_at = existing.created_at;
        let same_mtime = existing.content_mtime.map(|t| t.timestamp())
            == discovered.content_mtime.map(|t| t.timestamp());
        let same_hash = existing.ini_hash == discovered.ini_hash;
        discovered.updated_at = if same_mtime && same_hash {
            existing.updated_at
        } else {
            Utc::now()
        };
        discovered.metadata.user = existing.metadata.user.clone();
        discovered.metadata.prompt_for_missing_metadata =
            existing.metadata.prompt_for_missing_metadata;
        discovered.unsafe_content_auto = existing.unsafe_content_auto;
        discovered.unsafe_content_preference = existing.unsafe_content_preference;
        discovered.unsafe_content = existing.unsafe_content;
        discovered.source = existing.source.clone();
        discovered.update_state = existing.update_state;
    }
}

pub fn write_portable_metadata(mod_entry: &ModEntry) -> Result<()> {
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
    if managed_mod_root(&mod_entry.root_path)
        || directory_has_direct_package(&mod_entry.root_path)?
    {
        mark_mod_root(&mod_entry.root_path)?;
    }
    Ok(())
}

pub(crate) fn mark_mod_root(root: &Path) -> Result<()> {
    let metadata_dir = root.join(MOD_META_DIR);
    fs::create_dir_all(&metadata_dir)?;
    let marker = metadata_dir.join(MOD_ROOT_MARKER);
    if !marker.exists() {
        fs::write(marker, b"managed mod root\n")?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnrealLaunchCommand {
    executable: PathBuf,
    args: Vec<String>,
    working_dir: PathBuf,
    fallback_on_elevation: Option<Box<UnrealLaunchCommand>>,
}

fn resolve_launch_command(game: &GameInstall) -> Result<UnrealLaunchCommand> {
    match game.definition.id.as_str() {
        "nte" => resolve_nte_launch_command(game),
        _ => bail!(
            "unsupported Unreal Engine game launch: {}",
            game.definition.name
        ),
    }
}

fn resolve_nte_launch_command(game: &GameInstall) -> Result<UnrealLaunchCommand> {
    let configured_path = game
        .vanilla_exe_path_override
        .as_deref()
        .ok_or_else(|| anyhow!("NTE executable path is not configured"))?;
    if !configured_path.is_file() {
        bail!("NTE executable not found: {}", configured_path.display());
    }

    let root = nte_install_root_from_path(configured_path).ok_or_else(|| {
        anyhow!(
            "failed to derive NTE install root from {}",
            configured_path.display()
        )
    })?;
    let fallback_on_elevation = nte_launcher_command(&root).map(Box::new);
    let global_dir = root.join("NTEGlobal");
    let config_path = global_dir.join("Config").join("Config.ini");
    if let Some(command) = read_nte_launch_command_from_config(&config_path, &global_dir, &root)? {
        if command.executable.is_file() {
            return Ok(command.with_fallback(fallback_on_elevation));
        }
    }

    let fallback_exe = global_dir.join("NTEGlobalGame.exe");
    if fallback_exe.is_file() {
        return Ok(UnrealLaunchCommand {
            executable: fallback_exe,
            args: vec!["/launcher".to_string()],
            working_dir: global_dir,
            fallback_on_elevation,
        });
    }

    if let Some(command) = fallback_on_elevation.map(|command| *command) {
        return Ok(command);
    }

    bail!(
        "NTE launch executable not found: {}",
        fallback_exe.display()
    )
}

fn read_nte_launch_command_from_config(
    config_path: &Path,
    global_dir: &Path,
    root: &Path,
) -> Result<Option<UnrealLaunchCommand>> {
    if !config_path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(config_path).with_context(|| {
        format!(
            "failed to read NTE launcher config {}",
            config_path.display()
        )
    })?;
    let Some(launch_file) = ini_value(&contents, "UPDATE_CONFIG", "LaunchFilePath") else {
        return Ok(None);
    };
    let executable = resolve_nte_launch_file(&launch_file, global_dir, root);
    let args = ini_value(&contents, "UPDATE_CONFIG", "LaunchCmdLine")
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            shlex::split(&value)
                .ok_or_else(|| anyhow!("invalid NTE launch arguments in {}", config_path.display()))
        })
        .transpose()?
        .unwrap_or_default();
    let working_dir = executable
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| global_dir.to_path_buf());
    Ok(Some(UnrealLaunchCommand {
        executable,
        args,
        working_dir,
        fallback_on_elevation: None,
    }))
}

fn resolve_nte_launch_file(launch_file: &str, global_dir: &Path, root: &Path) -> PathBuf {
    let path = PathBuf::from(launch_file);
    if path.is_absolute() {
        return path;
    }

    let from_global = global_dir.join(&path);
    if from_global.exists() {
        return from_global;
    }

    let from_root = root.join(path);
    if from_root.exists() {
        return from_root;
    }

    from_global
}

fn ini_value(contents: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let current = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            in_section = current.eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(trim_ini_value(value));
        }
    }
    None
}

fn trim_ini_value(value: &str) -> String {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(trimmed)
        .to_string()
}

fn nte_install_root_from_path(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        let Some(name) = ancestor.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.eq_ignore_ascii_case("Neverness To Everness")
            || name.eq_ignore_ascii_case("NevernessToEverness")
        {
            return Some(ancestor.to_path_buf());
        }
        if name.eq_ignore_ascii_case("NTEGlobal") {
            return ancestor.parent().map(Path::to_path_buf);
        }
        if name.eq_ignore_ascii_case("HT") {
            let windows_no_editor = ancestor.parent()?;
            let client = windows_no_editor.parent()?;
            if windows_no_editor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("WindowsNoEditor"))
                && client
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("Client"))
            {
                return client.parent().map(Path::to_path_buf);
            }
        }
    }

    let parent = path.parent()?;
    if parent.join("NTEGlobal").is_dir() || parent.join("Client").is_dir() {
        return Some(parent.to_path_buf());
    }
    None
}

impl UnrealLaunchCommand {
    fn with_fallback(mut self, fallback_on_elevation: Option<Box<UnrealLaunchCommand>>) -> Self {
        self.fallback_on_elevation = fallback_on_elevation;
        self
    }
}

fn nte_launcher_command(root: &Path) -> Option<UnrealLaunchCommand> {
    let launcher = [
        root.join("NTEGlobalLauncher.exe"),
        root.join("NTEGlobal").join("NTEGlobalLauncher.exe"),
    ]
    .into_iter()
    .find(|path| path.is_file())?;
    Some(UnrealLaunchCommand {
        executable: launcher,
        args: Vec::new(),
        working_dir: root.to_path_buf(),
        fallback_on_elevation: None,
    })
}

fn launch_unreal_command(command: &UnrealLaunchCommand) -> Result<()> {
    match spawn_detached(command) {
        Ok(()) => Ok(()),
        Err(err) if err.raw_os_error() == Some(740) => {
            let Some(fallback) = command.fallback_on_elevation.as_deref() else {
                return Err(anyhow!(err)
                    .context(format!("failed to launch {}", command.executable.display())));
            };
            spawn_detached(fallback).with_context(|| {
                format!(
                    "{} requires elevation; failed to launch fallback {}",
                    command.executable.display(),
                    fallback.executable.display()
                )
            })
        }
        Err(err) => {
            Err(anyhow!(err).context(format!("failed to launch {}", command.executable.display())))
        }
    }
}

fn spawn_detached(command: &UnrealLaunchCommand) -> std::io::Result<()> {
    if !command.executable.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Unreal Engine launch executable not found: {}",
                command.executable.display()
            ),
        ));
    }

    let mut process = Command::new(&command.executable);
    process.args(&command.args);
    process.current_dir(&command.working_dir);

    #[cfg(windows)]
    {
        process.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    process.spawn().map(|_| ())
}

fn substantive_entries(root: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.file_name() == Some(OsStr::new(MOD_META_DIR)) {
            continue;
        }
        entries.push(path);
    }
    Ok(entries)
}

fn mod_dir_has_payload(root: &Path) -> Result<bool> {
    Ok(!substantive_entries(root)?.is_empty())
}

fn cleanup_metadata_only_mod_dir(root: &Path) -> Result<bool> {
    if root.is_dir() && root.join(MOD_META_DIR).is_dir() && !mod_dir_has_payload(root)? {
        fs::remove_dir_all(root)?;
        return Ok(true);
    }
    Ok(false)
}

fn compute_mod_fingerprint(root: &Path) -> Result<(Option<DateTime<Utc>>, Option<String>, u64)> {
    let mut max_mtime: Option<SystemTime> = None;
    let mut hasher = Xxh3::new();
    let mut content_size_bytes = 0_u64;
    let mut found_payload = false;

    let walker = walkdir::WalkDir::new(root).into_iter().filter_entry(|entry| {
        if entry.depth() == 0 || entry.file_type().is_symlink() {
            return entry.depth() == 0;
        }
        let path = entry.path();
        !(entry.file_type().is_dir()
            && (is_scan_helper_dir(path) || install_scratch_kind(path)))
    });
    for entry in walker {
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
        let metadata = entry.metadata()?;
        content_size_bytes = content_size_bytes.saturating_add(metadata.len());
        if let Ok(modified) = metadata.modified() {
            max_mtime = match max_mtime {
                Some(current) => Some(current.max(modified)),
                None => Some(modified),
            };
        }
        let rel = path.strip_prefix(root).unwrap_or(path);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(&metadata.len().to_le_bytes());
        found_payload = true;
    }

    let mtime = max_mtime.map(DateTime::<Utc>::from);
    let hash = found_payload.then(|| format!("{:016x}", hasher.finish()));
    Ok((mtime, hash, content_size_bytes))
}

fn next_available_mod_path(root: &Path, folder_name: &str) -> PathBuf {
    let initial = root.join(folder_name);
    if !initial.exists() {
        return initial;
    }
    for index in 2.. {
        let candidate = root.join(format!("{folder_name} ({index})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn relative_mod_path(base: &Path, root_path: &Path, folder_name: &str) -> PathBuf {
    root_path
        .strip_prefix(base)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(folder_name))
}

fn relative_file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("invalid mod folder name"))
}

fn next_available_relative_mod_path(base: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_mod_path(relative)?;
    reject_mod_root_ancestors(base, relative)?;
    let parent = relative
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| base.join(parent))
        .unwrap_or_else(|| base.to_path_buf());
    fs::create_dir_all(&parent)?;
    let folder_name = relative_file_name(relative)?;
    Ok(next_available_mod_path(&parent, &folder_name))
}

fn validate_relative_mod_path(relative: &Path) -> Result<()> {
    if relative.as_os_str().is_empty() {
        bail!("mod relative path is empty");
    }
    for component in relative.components() {
        if matches!(
            component,
            Component::Prefix(..) | Component::RootDir | Component::ParentDir
        ) {
            bail!("mod relative path escapes its storage root");
        }
    }
    Ok(())
}

fn reject_mod_root_ancestors(base: &Path, relative: &Path) -> Result<()> {
    let Some(parent) = relative.parent() else {
        return Ok(());
    };
    let mut current = base.to_path_buf();
    for component in parent.components() {
        current.push(component.as_os_str());
        if !current.exists() {
            break;
        }
        if path_is_link(&current) {
            bail!("cannot place mod below linked directory: {}", current.display());
        }
        if managed_mod_root(&current) || directory_has_direct_package(&current)? {
            bail!(
                "cannot place mod below existing Unreal mod root: {}",
                current.display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GameBackend, GameDefinition};

    fn nte_game(root: &Path) -> GameInstall {
        GameInstall {
            definition: GameDefinition {
                id: "nte".to_string(),
                name: "Neverness To Everness".to_string(),
                backend: GameBackend::UnrealEngine,
                xxmi_code: String::new(),
            },
            mods_path_override: Some(root.join("Content").join("Paks").join("~mods")),
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: true,
            enabled: true,
        }
    }

    #[test]
    fn scans_active_and_disabled_unreal_mod_folders() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active_mod = game.mods_path(false).unwrap().join("Active Mod");
        let disabled_mod = game.disabled_mods_path(false).unwrap().join("Disabled Mod");
        fs::create_dir_all(&active_mod).unwrap();
        fs::create_dir_all(&disabled_mod).unwrap();
        fs::write(active_mod.join("active_P.pak"), "active").unwrap();
        fs::write(disabled_mod.join("disabled_P.pak"), "disabled").unwrap();

        let mut scanned = scan_game_mods(&game, false).unwrap();
        scanned.sort_by(|a, b| a.folder_name.cmp(&b.folder_name));

        assert_eq!(scanned.len(), 2);
        assert_eq!(scanned[0].folder_name, "Active Mod");
        assert_eq!(scanned[0].status, ModStatus::Active);
        assert_eq!(scanned[1].folder_name, "Disabled Mod");
        assert_eq!(scanned[1].status, ModStatus::Disabled);
    }

    #[test]
    fn scans_nested_active_and_disabled_mods_by_direct_package_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active = game
            .mods_path(false)
            .unwrap()
            .join("Characters")
            .join("Ardelia")
            .join("Outfit A");
        let disabled = game
            .disabled_mods_path(false)
            .unwrap()
            .join("Characters")
            .join("Ardelia")
            .join("Outfit B");
        fs::create_dir_all(active.join("components")).unwrap();
        fs::create_dir_all(&disabled).unwrap();
        fs::write(active.join("main.PAK"), "active").unwrap();
        fs::write(active.join("components").join("part.ucas"), "component").unwrap();
        fs::write(disabled.join("main.UTOC"), "disabled").unwrap();
        fs::write(disabled.join("main.ucas"), "disabled").unwrap();

        let mut scanned = scan_game_mods(&game, false).unwrap();
        scanned.sort_by(|a, b| a.root_path.cmp(&b.root_path));

        assert_eq!(scanned.len(), 2);
        assert_eq!(scanned[0].root_path, active);
        assert_eq!(scanned[0].status, ModStatus::Active);
        assert_eq!(scanned[1].root_path, disabled);
        assert_eq!(scanned[1].status, ModStatus::Disabled);
    }

    #[test]
    fn direct_package_root_keeps_nested_components_atomic() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let root = game.mods_path(false).unwrap().join("Multipart");
        fs::create_dir_all(root.join("components")).unwrap();
        fs::write(root.join("main.pak"), "main").unwrap();
        fs::write(root.join("components").join("part.pak"), "part").unwrap();

        let scanned = scan_game_mods(&game, false).unwrap();

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].root_path, root);
    }

    #[test]
    fn legacy_metadata_container_splits_into_nested_package_mods() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let grouping = game.mods_path(false).unwrap().join("Ardelia");
        let first = grouping.join("Outfit A");
        let second = grouping.join("Outfit B");
        fs::create_dir_all(grouping.join(MOD_META_DIR)).unwrap();
        fs::write(
            grouping.join(MOD_META_DIR).join("metadata.json"),
            "{}",
        )
        .unwrap();
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("outfit.pak"), "first").unwrap();
        fs::write(second.join("outfit.pak"), "second").unwrap();

        let mut scanned = scan_game_mods(&game, false).unwrap();
        scanned.sort_by(|a, b| a.root_path.cmp(&b.root_path));

        assert_eq!(scanned.len(), 2);
        assert_eq!(scanned[0].root_path, first);
        assert_eq!(scanned[1].root_path, second);
    }

    #[test]
    fn managed_marker_keeps_multipart_unreal_root_atomic() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let root = game.mods_path(false).unwrap().join("Multipart");
        fs::create_dir_all(root.join("components")).unwrap();
        fs::write(root.join("components").join("one.pak"), "one").unwrap();
        fs::create_dir_all(root.join(MOD_META_DIR)).unwrap();
        fs::write(root.join(MOD_META_DIR).join(MOD_ROOT_MARKER), b"managed\n").unwrap();

        let scanned = scan_game_mods(&game, false).unwrap();

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].root_path, root);
    }

    #[test]
    fn legacy_non_package_fallback_does_not_block_later_nested_discovery() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let grouping = game.mods_path(false).unwrap().join("Ardelia");
        fs::create_dir_all(&grouping).unwrap();
        fs::write(grouping.join("cover.jpg"), "cover").unwrap();

        let entry = scan_game_mods(&game, false).unwrap().remove(0);
        write_portable_metadata(&entry).unwrap();
        assert!(!grouping.join(MOD_META_DIR).join(MOD_ROOT_MARKER).exists());

        let child = grouping.join("Outfit");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("outfit.pak"), "pak").unwrap();

        let scanned = scan_game_mods(&game, false).unwrap();
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].root_path, child);
    }

    #[test]
    fn scratch_and_helper_trees_are_ignored_without_deleting_active_staging() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let grouping = game.mods_path(false).unwrap().join("Ardelia");
        let actual = grouping.join("Outfit");
        fs::create_dir_all(&actual).unwrap();
        fs::write(actual.join("outfit.pak"), "actual").unwrap();
        fs::create_dir_all(grouping.join(".hestia-install-live").join("nested")).unwrap();
        fs::write(
            grouping
                .join(".hestia-install-live")
                .join("nested")
                .join("scratch.pak"),
            "scratch",
        )
        .unwrap();
        let live_retired = grouping.join(".hestia-retired-.hestia-install-live");
        fs::create_dir_all(&live_retired).unwrap();
        fs::write(live_retired.join("retired.pak"), "retired").unwrap();
        let orphan_retired = grouping.join(".hestia-retired-after-failed-dispose");
        fs::create_dir_all(&orphan_retired).unwrap();
        fs::write(orphan_retired.join("retired.pak"), "retired").unwrap();
        fs::create_dir_all(grouping.join(MOD_META_DIR)).unwrap();
        fs::write(
            grouping.join(MOD_META_DIR).join("helper.pak"),
            "helper",
        )
        .unwrap();

        let scanned = scan_game_mods(&game, false).unwrap();

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].root_path, actual);
        assert!(grouping.join(".hestia-install-live").exists());
        assert!(live_retired.exists());
        assert!(!orphan_retired.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_unreal_mod_directories_are_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active_root = game.mods_path(false).unwrap();
        let actual = active_root.join("Actual");
        let link = active_root.join("Linked");
        fs::create_dir_all(&actual).unwrap();
        fs::write(actual.join("actual.pak"), "actual").unwrap();
        std::os::unix::fs::symlink(&actual, &link).unwrap();

        let scanned = scan_game_mods(&game, false).unwrap();

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].root_path, actual);
    }

    #[test]
    fn disable_and_enable_move_whole_mod_folder_between_unreal_roots() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active_mod = game.mods_path(false).unwrap().join("Spider Gwen");
        fs::create_dir_all(&active_mod).unwrap();
        fs::write(active_mod.join("spider_P.pak"), "pak").unwrap();

        let mut entry = scan_game_mods(&game, false).unwrap().remove(0);
        disable_mod(&mut entry, &game, false).unwrap();
        assert_eq!(entry.status, ModStatus::Disabled);
        assert!(!active_mod.exists());
        assert!(
            game.disabled_mods_path(false)
                .unwrap()
                .join("Spider Gwen")
                .join("spider_P.pak")
                .is_file()
        );

        enable_mod(&mut entry, &game, false).unwrap();
        assert_eq!(entry.status, ModStatus::Active);
        assert!(active_mod.join("spider_P.pak").is_file());
    }

    #[test]
    fn nested_disable_enable_preserves_relative_path_and_identity() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active_mod = game
            .mods_path(false)
            .unwrap()
            .join("Ardelia")
            .join("Outfit");
        fs::create_dir_all(&active_mod).unwrap();
        fs::write(active_mod.join("outfit.pak"), "pak").unwrap();

        let mut entry = scan_game_mods(&game, false).unwrap().remove(0);
        let id = entry.id.clone();
        disable_mod(&mut entry, &game, false).unwrap();
        let disabled_mod = game
            .disabled_mods_path(false)
            .unwrap()
            .join("Ardelia")
            .join("Outfit");
        assert_eq!(entry.root_path, disabled_mod);
        assert!(!active_mod.exists());

        let rescanned_disabled = scan_game_mods(&game, false).unwrap();
        assert_eq!(rescanned_disabled.len(), 1);
        assert_eq!(rescanned_disabled[0].id, id);
        assert_eq!(rescanned_disabled[0].root_path, disabled_mod);

        enable_mod(&mut entry, &game, false).unwrap();
        assert_eq!(entry.id, id);
        assert_eq!(entry.root_path, active_mod);

        let rescanned_active = scan_game_mods(&game, false).unwrap();
        assert_eq!(rescanned_active.len(), 1);
        assert_eq!(rescanned_active[0].id, id);
        assert_eq!(rescanned_active[0].root_path, active_mod);
    }

    #[test]
    fn same_basename_categories_remain_separate_and_collision_stays_in_category() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active_root = game.mods_path(false).unwrap();
        let disabled_root = game.disabled_mods_path(false).unwrap();
        let ardelia = active_root.join("Ardelia").join("Same");
        let beatrice = active_root.join("Beatrice").join("Same");
        let disabled_same = disabled_root.join("Ardelia").join("Same");
        fs::create_dir_all(&ardelia).unwrap();
        fs::create_dir_all(&beatrice).unwrap();
        fs::create_dir_all(&disabled_same).unwrap();
        fs::write(ardelia.join("ardelia.pak"), "active").unwrap();
        fs::write(beatrice.join("beatrice.pak"), "active").unwrap();
        fs::write(disabled_same.join("disabled.pak"), "disabled").unwrap();

        let mut scanned = scan_game_mods(&game, false).unwrap();
        scanned.sort_by(|a, b| a.root_path.cmp(&b.root_path));
        assert_eq!(scanned.len(), 3);
        let mut disabled_entry = scanned
            .into_iter()
            .find(|entry| entry.status == ModStatus::Disabled)
            .unwrap();
        let id = disabled_entry.id.clone();

        enable_mod(&mut disabled_entry, &game, false).unwrap();
        let expected = active_root.join("Ardelia").join("Same (2)");
        assert_eq!(disabled_entry.root_path, expected);
        assert_eq!(disabled_entry.folder_name, "Same (2)");

        let rescanned = scan_game_mods(&game, false).unwrap();
        assert_eq!(
            rescanned
                .iter()
                .filter(|entry| entry.root_path.starts_with(active_root.join("Ardelia")))
                .count(),
            2
        );
        assert_eq!(
            rescanned
                .iter()
                .find(|entry| entry.id == id)
                .map(|entry| entry.root_path.clone()),
            Some(expected)
        );
        assert!(
            rescanned
                .iter()
                .any(|entry| entry.root_path == beatrice)
        );
    }

    #[test]
    fn moving_below_existing_mod_root_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active_root = game.mods_path(false).unwrap();
        let disabled_root = game.disabled_mods_path(false).unwrap();
        let source = active_root.join("Ardelia").join("Outfit");
        let blocking_parent = disabled_root.join("Ardelia");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&blocking_parent).unwrap();
        fs::write(source.join("outfit.pak"), "source").unwrap();
        fs::write(blocking_parent.join("whole.pak"), "blocking").unwrap();

        let mut entry = scan_game_mods(&game, false)
            .unwrap()
            .into_iter()
            .find(|entry| entry.status == ModStatus::Active)
            .unwrap();
        let error = disable_mod(&mut entry, &game, false).unwrap_err();

        assert!(error.to_string().contains("existing Unreal mod root"));
        assert!(source.join("outfit.pak").is_file());
    }

    #[test]
    fn relative_move_target_rejects_parent_traversal_without_moving_source() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let source = temp.path().join("External Mod");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("mod.pak"), "source").unwrap();
        let mut entry = scan_mod_dir(&game, source.clone(), ModStatus::Active).unwrap();
        entry.folder_name = "../outside/mod".to_string();

        let error = disable_mod(&mut entry, &game, false).unwrap_err();

        assert!(error
            .to_string()
            .contains("mod relative path escapes its storage root"));
        assert!(source.join("mod.pak").is_file());
        assert!(!temp.path().join("outside").exists());
    }

    #[test]
    fn scan_migrates_legacy_unreal_disabled_folder_out_of_paks() {
        let temp = tempfile::tempdir().unwrap();
        let game = nte_game(temp.path());
        let active_root = game.mods_path(false).unwrap();
        let old_disabled_root = active_root.parent().unwrap().join("~mods-disabled");
        let old_mod = old_disabled_root.join("Legacy Disabled");
        fs::create_dir_all(&old_mod).unwrap();
        fs::write(old_mod.join("legacy_P.pak"), "legacy").unwrap();

        let scanned = scan_game_mods(&game, false).unwrap();
        let new_mod = game
            .disabled_mods_path(false)
            .unwrap()
            .join("Legacy Disabled");

        assert!(!old_mod.exists());
        assert!(!old_disabled_root.exists());
        assert!(new_mod.join("legacy_P.pak").is_file());
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].folder_name, "Legacy Disabled");
        assert_eq!(scanned[0].status, ModStatus::Disabled);
    }

    #[test]
    fn nte_launch_command_uses_update_config_from_ht_game_path() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Neverness To Everness");
        let ht_game = root
            .join("Client")
            .join("WindowsNoEditor")
            .join("HT")
            .join("Binaries")
            .join("Win64")
            .join("HTGame.exe");
        let launcher = root.join("NTEGlobalLauncher.exe");
        let global_game = root.join("NTEGlobal").join("NTEGlobalGame.exe");
        let config = root.join("NTEGlobal").join("Config").join("Config.ini");
        fs::create_dir_all(ht_game.parent().unwrap()).unwrap();
        fs::create_dir_all(global_game.parent().unwrap()).unwrap();
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&ht_game, "").unwrap();
        fs::write(&launcher, "").unwrap();
        fs::write(&global_game, "").unwrap();
        fs::write(
            &config,
            "[UPDATE_CONFIG]\nLaunchFilePath=NTEGlobalGame.exe\nLaunchCmdLine=/launcher\n",
        )
        .unwrap();

        let mut game = nte_game(temp.path());
        game.vanilla_exe_path_override = Some(ht_game);

        let command = resolve_launch_command(&game).unwrap();
        assert_eq!(command.executable, global_game);
        assert_eq!(command.args, vec!["/launcher"]);
        assert_eq!(command.working_dir, root.join("NTEGlobal"));
        let fallback = command.fallback_on_elevation.as_deref().unwrap();
        assert_eq!(fallback.executable, launcher);
        assert_eq!(fallback.working_dir, root);
    }

    #[test]
    fn nte_launch_command_falls_back_to_global_game_launcher_arg() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Custom NTE Root");
        let selected_launcher = root.join("NTEGlobalLauncher.exe");
        let global_game = root.join("NTEGlobal").join("NTEGlobalGame.exe");
        fs::create_dir_all(global_game.parent().unwrap()).unwrap();
        fs::write(&selected_launcher, "").unwrap();
        fs::write(&global_game, "").unwrap();

        let mut game = nte_game(temp.path());
        game.vanilla_exe_path_override = Some(selected_launcher.clone());

        let command = resolve_launch_command(&game).unwrap();
        assert_eq!(command.executable, global_game);
        assert_eq!(command.args, vec!["/launcher"]);
        assert_eq!(command.working_dir, root.join("NTEGlobal"));
        let fallback = command.fallback_on_elevation.as_deref().unwrap();
        assert_eq!(fallback.executable, selected_launcher);
        assert_eq!(fallback.working_dir, root);
    }
}
