use std::{
    ffi::OsStr,
    fs,
    hash::Hasher,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x00000008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use xxhash_rust::xxh3::Xxh3;

use crate::{
    model::{
        AppState, GameInstall, ModEntry, ModMetadata, ModStatus, PortableModState, MOD_META_DIR,
    },
    persistence,
};

const LEGACY_UNREAL_DISABLED_MODS_DIR: &str = "~mods-disabled";

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
    fs::create_dir_all(&disabled_root)?;
    let target = next_available_mod_path(&disabled_root, &mod_entry.folder_name);
    fs::rename(&mod_entry.root_path, &target)
        .with_context(|| format!("failed to move mod to {}", target.display()))?;
    mod_entry.root_path = target;
    mod_entry.status = ModStatus::Disabled;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
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
    fs::create_dir_all(&active_root)?;
    let target = next_available_mod_path(&active_root, &mod_entry.folder_name);
    fs::rename(&mod_entry.root_path, &target)
        .with_context(|| format!("failed to move mod to {}", target.display()))?;
    mod_entry.root_path = target;
    mod_entry.status = ModStatus::Active;
    mod_entry.updated_at = Utc::now();
    write_portable_metadata(mod_entry)?;
    Ok(())
}

fn scan_root(game: &GameInstall, root: &Path, status: ModStatus) -> Result<Vec<ModEntry>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || path.file_name() == Some(OsStr::new(MOD_META_DIR)) {
            continue;
        }
        if cleanup_metadata_only_mod_dir(&path)? {
            continue;
        }
        if !mod_dir_has_payload(&path)? {
            continue;
        }
        mods.push(scan_mod_dir(game, path, status.clone())?);
    }
    Ok(mods)
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
    let (created_at, updated_at, unsafe_content) = match &portable {
        Some(stored) => (
            stored.created_at.unwrap_or_else(Utc::now),
            stored.updated_at.unwrap_or_else(Utc::now),
            stored.unsafe_content,
        ),
        None => (Utc::now(), Utc::now(), false),
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
        let path = entry.path();
        if !path.is_dir() || path.file_name() == Some(OsStr::new(MOD_META_DIR)) {
            continue;
        }
        let Some(folder_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        fs::create_dir_all(disabled_root)
            .with_context(|| format!("failed to create {}", disabled_root.display()))?;
        let target = next_available_mod_path(disabled_root, folder_name);
        fs::rename(&path, &target)
            .with_context(|| format!("failed to move legacy disabled mod to {}", target.display()))?;
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
        created_at: Some(mod_entry.created_at),
        updated_at: Some(mod_entry.updated_at),
    };
    persistence::save_portable_mod_state(&mod_entry.root_path, &portable)
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

    bail!("NTE launch executable not found: {}", fallback_exe.display())
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
                return Err(anyhow!(err).context(format!(
                    "failed to launch {}",
                    command.executable.display()
                )));
            };
            spawn_detached(fallback).with_context(|| {
                format!(
                    "{} requires elevation; failed to launch fallback {}",
                    command.executable.display(),
                    fallback.executable.display()
                )
            })
        }
        Err(err) => Err(anyhow!(err).context(format!(
            "failed to launch {}",
            command.executable.display()
        ))),
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
            mods_path_override: Some(
                root.join("Content")
                    .join("Paks")
                    .join("~mods"),
            ),
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
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
        assert!(game
            .disabled_mods_path(false)
            .unwrap()
            .join("Spider Gwen")
            .join("spider_P.pak")
            .is_file());

        enable_mod(&mut entry, &game, false).unwrap();
        assert_eq!(entry.status, ModStatus::Active);
        assert!(active_mod.join("spider_P.pak").is_file());
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
