use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};
use once_cell::sync::Lazy;
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::model::{ConflictChoice, ImportCandidate, ImportInspection, ImportSource};
use crate::persistence;

pub const CANCELLED_ERROR: &str = "install canceled";
pub type CancelFlag = Arc<AtomicBool>;

pub struct PreparedImport {
    pub _temp_dir: Option<TempDir>,
    pub inspection: ImportInspection,
    pub source_is_archive: bool,
}

const ZIP_COPY_BUFFER_BYTES: usize = 256 * 1024;
const ZIP_EXTRACT_WORKERS: usize = 4;

static ZIP_EXTRACT_POOL: Lazy<ThreadPool> = Lazy::new(|| {
    let worker_count = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(ZIP_EXTRACT_WORKERS);
    ThreadPoolBuilder::new()
        .num_threads(worker_count)
        .thread_name(|index| format!("hestia-zip-{index}"))
        .build()
        .expect("failed to create ZIP extraction pool")
});

fn check_cancel(flag: &CancelFlag) -> Result<()> {
    if flag.load(Ordering::Relaxed) {
        bail!(CANCELLED_ERROR);
    }
    Ok(())
}

fn validate_windows_relative_path(path: &Path) -> Result<()> {
    for component in path.components() {
        let Component::Normal(name) = component else {
            bail!("import contains invalid path: {}", path.display());
        };
        validate_windows_file_name(name)
            .with_context(|| format!("import contains invalid file name: {}", path.display()))?;
    }
    Ok(())
}

fn validate_windows_file_name(name: &OsStr) -> Result<()> {
    let name = name
        .to_str()
        .ok_or_else(|| anyhow!("file name is not valid UTF-8"))?;
    if name.is_empty() {
        bail!("file name is empty");
    }
    if name.ends_with([' ', '.']) {
        bail!("file name ends with a space or dot");
    }
    if name.chars().any(|c| {
        c.is_ascii_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
    }) {
        bail!("file name contains a character Windows does not allow");
    }

    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        bail!("file name uses a reserved Windows device name");
    }

    Ok(())
}

fn sanitize_windows_file_name(name: &OsStr) -> Result<OsString> {
    let name = name
        .to_str()
        .ok_or_else(|| anyhow!("file name is not valid UTF-8"))?
        .trim();
    let mut sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_control()
                || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    while sanitized.ends_with([' ', '.']) {
        sanitized.pop();
    }
    if sanitized.is_empty() {
        sanitized = "Imported Mod".to_string();
    }

    let stem = sanitized
        .split('.')
        .next()
        .unwrap_or(&sanitized)
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        sanitized.push('_');
    }
    Ok(OsString::from(sanitized))
}

fn validate_import_tree(root: &Path, cancel: Option<&CancelFlag>) -> Result<()> {
    for entry in WalkDir::new(root) {
        if let Some(flag) = cancel {
            check_cancel(flag)?;
        }
        let entry = entry?;
        let relative = entry.path().strip_prefix(root)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        validate_windows_relative_path(relative)?;
    }
    Ok(())
}

fn validate_import_candidates(
    inspection: &ImportInspection,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    for candidate in &inspection.candidates {
        validate_import_tree(&candidate.path, cancel).with_context(|| {
            format!(
                "import candidate contains invalid file names: {}",
                candidate.label
            )
        })?;
    }
    Ok(())
}

pub fn validate_install_folder_name(name: &str) -> Result<()> {
    validate_windows_file_name(OsStr::new(name))
        .with_context(|| format!("invalid install folder name: {name}"))
}

const SUPPORTED_ARCHIVE_EXTENSIONS: [&str; 3] = ["zip", "7z", "rar"];
const SPLIT_PART_SUFFIX_MAX_DIGITS: usize = 6;

fn is_supported_archive_extension(ext: &str) -> bool {
    SUPPORTED_ARCHIVE_EXTENSIONS
        .iter()
        .any(|supported| ext.eq_ignore_ascii_case(supported))
}

/// Detects raw byte-split archive volumes like `mod.rar.0001` or `mod.7z.001`.
/// Returns the base archive path (`mod.rar`) and the part number.
fn numeric_split_part(path: &Path) -> Option<(PathBuf, u64)> {
    let suffix = path.extension()?.to_str()?;
    if suffix.is_empty()
        || suffix.len() > SPLIT_PART_SUFFIX_MAX_DIGITS
        || !suffix.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let part = suffix.parse().ok()?;
    let base = path.with_extension("");
    let base_ext = base.extension()?.to_str()?;
    is_supported_archive_extension(base_ext).then_some((base, part))
}

/// Detects native RAR multi-volume naming like `mod.part2.rar`.
/// Returns the stem before `.partN` and the part number.
fn rar_part_number(path: &Path) -> Option<(String, u64)> {
    let ext = path.extension()?.to_str()?;
    if !ext.eq_ignore_ascii_case("rar") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let idx = stem.to_ascii_lowercase().rfind(".part")?;
    let digits = &stem[idx + ".part".len()..];
    if digits.is_empty()
        || digits.len() > SPLIT_PART_SUFFIX_MAX_DIGITS
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((stem[..idx].to_string(), digits.parse().ok()?))
}

/// Detects old-style RAR volume naming (`mod.r00`, `mod.r01`, ...).
/// Returns the main `mod.rar` volume if it exists next to the part.
fn rar_old_style_main_volume(path: &Path) -> Option<PathBuf> {
    let ext = path.extension()?.to_str()?.as_bytes();
    if !(ext.len() == 3
        && ext[0].eq_ignore_ascii_case(&b'r')
        && ext[1].is_ascii_digit()
        && ext[2].is_ascii_digit())
    {
        return None;
    }
    let main = path.with_extension("rar");
    main.is_file().then_some(main)
}

fn split_part_parent(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// If `path` is one volume of a split archive, returns the volume extraction
/// should start from, so every part of a set maps to the same install source.
/// Returns `None` when `path` is not a recognized split volume.
pub fn resolve_split_archive(path: &Path) -> Option<PathBuf> {
    if let Some((base, _)) = numeric_split_part(path) {
        let first = collect_numeric_split_parts(&base)
            .ok()
            .and_then(|parts| parts.into_iter().next());
        return Some(first.unwrap_or_else(|| path.to_path_buf()));
    }
    if let Some((prefix, _)) = rar_part_number(path) {
        let mut best: Option<(u64, PathBuf)> = None;
        if let Ok(entries) = fs::read_dir(split_part_parent(path)) {
            for entry in entries.flatten() {
                let candidate = entry.path();
                let Some((candidate_prefix, number)) = rar_part_number(&candidate) else {
                    continue;
                };
                if !candidate_prefix.eq_ignore_ascii_case(&prefix) {
                    continue;
                }
                if best.as_ref().is_none_or(|(smallest, _)| number < *smallest) {
                    best = Some((number, candidate));
                }
            }
        }
        return Some(
            best.map(|(_, first)| first)
                .unwrap_or_else(|| path.to_path_buf()),
        );
    }
    rar_old_style_main_volume(path)
}

/// File-picker extensions for the archive filter: the plain archive formats
/// plus the first-part suffixes of split sets. Selecting part 1 is enough --
/// the remaining volumes are picked up from the same folder automatically.
/// The list must stay small: Windows' file dialog crashes on huge filters.
pub fn archive_picker_extensions() -> &'static [&'static str] {
    &["zip", "rar", "7z", "001", "0001"]
}

/// Display name for an import source file: split volumes show the base
/// archive name (`mod.7z.001` -> `mod.7z`, `mod.part2.rar` -> `mod.rar`).
pub fn source_display_file_name(path: &Path) -> String {
    if let Some((base, _)) = numeric_split_part(path) {
        if let Some(name) = base.file_name().and_then(OsStr::to_str) {
            return name.to_string();
        }
    }
    if let Some((prefix, _)) = rar_part_number(path)
        && !prefix.is_empty()
    {
        return format!("{prefix}.rar");
    }
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("mod")
        .to_string()
}

/// Size of an archive source on disk; for split sets, the sum of all volumes.
pub fn archive_source_total_size(path: &Path) -> Option<u64> {
    if numeric_split_part(path).is_some() {
        if let Some(parts) = resolve_split_archive(path)
            .and_then(|first| numeric_split_part(&first))
            .and_then(|(base, _)| collect_numeric_split_parts(&base).ok())
        {
            return Some(
                parts
                    .iter()
                    .filter_map(|part| fs::metadata(part).ok())
                    .map(|meta| meta.len())
                    .sum(),
            );
        }
    }
    if let Some((prefix, _)) = rar_part_number(path) {
        let mut total = 0u64;
        let mut found = false;
        if let Ok(entries) = fs::read_dir(split_part_parent(path)) {
            for entry in entries.flatten() {
                let candidate = entry.path();
                let same_set = rar_part_number(&candidate).is_some_and(|(candidate_prefix, _)| {
                    candidate_prefix.eq_ignore_ascii_case(&prefix)
                });
                if same_set && let Ok(meta) = fs::metadata(&candidate) {
                    total += meta.len();
                    found = true;
                }
            }
        }
        if found {
            return Some(total);
        }
    }
    fs::metadata(path).ok().map(|meta| meta.len())
}

/// Strip the "uuid_" prefix some download sources stamp on archive and folder
/// names ("8b7f2df5-61b7-409f-93bf-7948fd407fd4_Name" -> "Name"), so the
/// installed folder gets the human name. A 36-char parse only succeeds for the
/// hyphenated 8-4-4-4-12 form, so ordinary names are never mistaken for one.
fn strip_uuid_prefix(name: &str) -> &str {
    const UUID_PREFIX_LEN: usize = 37; // hyphenated uuid + '_'
    if name.len() > UUID_PREFIX_LEN && name.is_char_boundary(UUID_PREFIX_LEN) {
        let (prefix, rest) = name.split_at(UUID_PREFIX_LEN);
        if prefix.ends_with('_')
            && uuid::Uuid::parse_str(&prefix[..UUID_PREFIX_LEN - 1]).is_ok()
        {
            let rest = rest.trim_start();
            if !rest.is_empty() {
                return rest;
            }
        }
    }
    name
}

fn import_source_label(path: &Path) -> String {
    let label = if let Some((base, _)) = numeric_split_part(path) {
        base.file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("mod")
            .to_string()
    } else if let Some((prefix, _)) = rar_part_number(path)
        && !prefix.is_empty()
    {
        prefix
    } else {
        path.file_stem()
            .or_else(|| path.file_name())
            .and_then(OsStr::to_str)
            .unwrap_or("mod")
            .to_string()
    };
    strip_uuid_prefix(&label).to_string()
}

fn collect_numeric_split_parts(base: &Path) -> Result<Vec<PathBuf>> {
    let base_name = base
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("split archive has an invalid base name"))?;
    let mut parts: Vec<(u64, PathBuf)> = Vec::new();
    for entry in fs::read_dir(split_part_parent(base))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let Some((candidate_base, number)) = numeric_split_part(&path) else {
            continue;
        };
        let matches_base = candidate_base
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.eq_ignore_ascii_case(base_name));
        if matches_base {
            parts.push((number, path));
        }
    }
    if parts.is_empty() {
        bail!("found no volumes of split archive {base_name}");
    }
    parts.sort_by_key(|(number, _)| *number);
    for pair in parts.windows(2) {
        if pair[0].0 == pair[1].0 {
            bail!(
                "split archive has conflicting volumes: {} and {}",
                pair[0].1.display(),
                pair[1].1.display()
            );
        }
    }
    let first = parts[0].0;
    if first > 1 {
        bail!("split archive {base_name} is missing part 1 (found parts starting at {first})");
    }
    for (offset, (number, _)) in parts.iter().enumerate() {
        let expected = first + offset as u64;
        if *number != expected {
            bail!("split archive {base_name} is missing part {expected}");
        }
    }
    Ok(parts.into_iter().map(|(_, path)| path).collect())
}

fn join_split_parts(parts: &[PathBuf], joined: &Path, cancel: Option<&CancelFlag>) -> Result<()> {
    let mut output = fs::File::create(joined)
        .with_context(|| format!("failed to create joined archive {}", joined.display()))?;
    let mut buffer = vec![0u8; ZIP_COPY_BUFFER_BYTES];
    for part in parts {
        let mut input = fs::File::open(part)
            .with_context(|| format!("failed to open split volume {}", part.display()))?;
        loop {
            if let Some(flag) = cancel {
                check_cancel(flag)?;
            }
            let bytes_read = input.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            std::io::Write::write_all(&mut output, &buffer[..bytes_read])?;
        }
    }
    Ok(())
}

fn extract_split_archive(
    part: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    let (base, _) = numeric_split_part(part)
        .ok_or_else(|| anyhow!("not a split archive volume: {}", part.display()))?;
    let parts = collect_numeric_split_parts(&base)?;
    let base_name = base
        .file_name()
        .ok_or_else(|| anyhow!("split archive has an invalid base name"))?
        .to_os_string();
    let extract_root = persistence::runtime_temp_extract_dir();
    fs::create_dir_all(&extract_root)?;
    let temp_dir = tempfile::Builder::new()
        .prefix("join-")
        .tempdir_in(&extract_root)
        .context("failed to create temp dir for joining split archive")?;
    let joined = temp_dir.path().join(base_name);
    join_split_parts(&parts, &joined, cancel)?;
    extract_archive_impl(&joined, destination, cancel)
}

fn zip_top_level_sanitize_map(
    archive: &mut zip::ZipArchive<fs::File>,
) -> Result<Option<HashMap<OsString, OsString>>> {
    let mut has_top_level_dir = false;
    let mut has_top_level_file = false;
    let mut original_names = HashMap::<OsString, OsString>::new();
    let mut sanitized_keys = HashMap::<String, OsString>::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("archive contains invalid path"))?;
        let mut components = enclosed.components();
        let Some(Component::Normal(first)) = components.next() else {
            bail!("archive contains invalid path");
        };
        if first == OsStr::new("__MACOSX") {
            continue;
        }
        if components.next().is_some() || entry.name().ends_with('/') {
            has_top_level_dir = true;
            let original = first.to_os_string();
            if !original_names.contains_key(&original) {
                let sanitized = sanitize_windows_file_name(first)?;
                let key = sanitized.to_string_lossy().to_lowercase();
                if let Some(existing) = sanitized_keys.get(&key) {
                    if existing != &original {
                        bail!(
                            "archive top-level folders sanitize to the same install name: {} and {}",
                            existing.to_string_lossy(),
                            first.to_string_lossy()
                        );
                    }
                }
                sanitized_keys.insert(key, original.clone());
                original_names.insert(original, sanitized);
            }
        } else {
            has_top_level_file = true;
        }
    }
    if has_top_level_dir && !has_top_level_file {
        Ok(Some(original_names))
    } else {
        Ok(None)
    }
}

fn zip_entry_relative_path(
    enclosed: &Path,
    sanitized_top_level: Option<&HashMap<OsString, OsString>>,
) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for (index, component) in enclosed.components().enumerate() {
        let Component::Normal(name) = component else {
            bail!("archive contains invalid path: {}", enclosed.display());
        };
        if index == 0 {
            if let Some(map) = sanitized_top_level {
                let sanitized = map.get(name).ok_or_else(|| {
                    anyhow!("archive contains invalid path: {}", enclosed.display())
                })?;
                out.push(sanitized);
                continue;
            }
        }
        validate_windows_file_name(name).with_context(|| {
            format!("archive contains invalid file name: {}", enclosed.display())
        })?;
        out.push(name);
    }
    Ok(out)
}

fn zip_entry_is_ignored_metadata(path: &Path) -> bool {
    path.components()
        .next()
        .is_some_and(|component| component == Component::Normal(OsStr::new("__MACOSX")))
}

#[allow(dead_code)]
pub fn inspect_source(game_id: &str, source: ImportSource) -> Result<PreparedImport> {
    match &source {
        ImportSource::Folder(path) => {
            let inspection = inspect_directory(game_id, &source, path)?;
            validate_import_candidates(&inspection, None)?;
            Ok(PreparedImport {
                _temp_dir: None,
                inspection,
                source_is_archive: false,
            })
        }
        ImportSource::Archive(path) => {
            let extract_root = persistence::runtime_temp_extract_dir();
            fs::create_dir_all(&extract_root)?;
            let temp_dir = tempfile::Builder::new()
                .prefix("inspect-")
                .tempdir_in(&extract_root)
                .context("failed to create temp dir for archive inspection")?;
            extract_archive(path, temp_dir.path())?;
            let inspection = inspect_directory(game_id, &source, temp_dir.path())?;
            validate_import_candidates(&inspection, None)?;
            Ok(PreparedImport {
                _temp_dir: Some(temp_dir),
                inspection,
                source_is_archive: true,
            })
        }
    }
}

pub fn inspect_source_cancelable(
    game_id: &str,
    source: ImportSource,
    cancel: &CancelFlag,
) -> Result<PreparedImport> {
    check_cancel(cancel)?;
    match &source {
        ImportSource::Folder(path) => {
            let inspection = inspect_directory_cancelable(game_id, &source, path, cancel)?;
            validate_import_candidates(&inspection, Some(cancel))?;
            Ok(PreparedImport {
                _temp_dir: None,
                inspection,
                source_is_archive: false,
            })
        }
        ImportSource::Archive(path) => {
            let extract_root = persistence::runtime_temp_extract_dir();
            fs::create_dir_all(&extract_root)?;
            let temp_dir = tempfile::Builder::new()
                .prefix("inspect-")
                .tempdir_in(&extract_root)
                .context("failed to create temp dir for archive inspection")?;
            extract_archive_cancelable(path, temp_dir.path(), cancel)?;
            check_cancel(cancel)?;
            let inspection =
                inspect_directory_cancelable(game_id, &source, temp_dir.path(), cancel)?;
            validate_import_candidates(&inspection, Some(cancel))?;
            check_cancel(cancel)?;
            Ok(PreparedImport {
                _temp_dir: Some(temp_dir),
                inspection,
                source_is_archive: true,
            })
        }
    }
}

#[allow(dead_code)]
pub fn install_candidate(
    candidate_path: &Path,
    preferred_name: &str,
    target_root: &Path,
    choice: ConflictChoice,
) -> Result<Option<PathBuf>> {
    validate_install_folder_name(preferred_name)?;
    fs::create_dir_all(target_root)?;
    let initial_target = target_root.join(preferred_name);

    if initial_target.exists() {
        return match choice {
            ConflictChoice::Cancel => Ok(None),
            ConflictChoice::Replace => {
                copy_dir(candidate_path, &initial_target, true)?;
                Ok(Some(initial_target))
            }
            ConflictChoice::Merge => {
                copy_dir(candidate_path, &initial_target, false)?;
                Ok(Some(initial_target))
            }
            ConflictChoice::KeepBoth => {
                let target = next_available_name(target_root, preferred_name);
                copy_dir(candidate_path, &target, false)?;
                Ok(Some(target))
            }
        };
    }

    copy_dir(candidate_path, &initial_target, false)?;
    Ok(Some(initial_target))
}

#[allow(dead_code)]
fn inspect_directory(
    game_id: &str,
    source: &ImportSource,
    root: &Path,
) -> Result<ImportInspection> {
    let mut top_level_dirs = Vec::new();
    let mut top_level_files = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.file_name() == Some(OsStr::new("__MACOSX")) {
            continue;
        }
        if path.is_dir() {
            top_level_dirs.push(path);
        } else {
            top_level_files.push(path);
        }
    }

    let mut candidates = Vec::with_capacity(top_level_dirs.len().max(1));
    let mut notice = None;

    if top_level_dirs.len() == 1 && top_level_files.is_empty() {
        let nested = &top_level_dirs[0];
        candidates.push(ImportCandidate {
            label: strip_uuid_prefix(
                nested.file_name().and_then(OsStr::to_str).unwrap_or("mod"),
            )
            .to_string(),
            path: nested.clone(),
        });
        notice = Some("Nested top-level folder detected. Hestia will import the inner folder as the mod root.".to_string());
    } else if top_level_dirs.len() > 1 && top_level_files.is_empty() {
        for dir in top_level_dirs {
            candidates.push(ImportCandidate {
                label: strip_uuid_prefix(
                    dir.file_name().and_then(OsStr::to_str).unwrap_or("mod"),
                )
                .to_string(),
                path: dir,
            });
        }
        notice = Some("Multiple top-level folders detected. Choose which folder should be treated as the mod root.".to_string());
    } else if top_level_dirs.is_empty() && top_level_files.is_empty() {
        bail!("import source is empty");
    } else {
        let label = match source {
            ImportSource::Folder(path) | ImportSource::Archive(path) => import_source_label(path),
        };
        candidates.push(ImportCandidate {
            label,
            path: root.to_path_buf(),
        });
    }

    Ok(ImportInspection {
        game_id: game_id.to_string(),
        candidates,
        notice,
    })
}

fn inspect_directory_cancelable(
    game_id: &str,
    source: &ImportSource,
    root: &Path,
    cancel: &CancelFlag,
) -> Result<ImportInspection> {
    check_cancel(cancel)?;
    let mut top_level_dirs = Vec::new();
    let mut top_level_files = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        check_cancel(cancel)?;
        let entry = entry?;
        let path = entry.path();
        if path.file_name() == Some(OsStr::new("__MACOSX")) {
            continue;
        }
        if path.is_dir() {
            top_level_dirs.push(path);
        } else {
            top_level_files.push(path);
        }
    }

    let mut candidates = Vec::with_capacity(top_level_dirs.len().max(1));
    let mut notice = None;

    if top_level_dirs.len() == 1 && top_level_files.is_empty() {
        let nested = &top_level_dirs[0];
        candidates.push(ImportCandidate {
            label: strip_uuid_prefix(
                nested.file_name().and_then(OsStr::to_str).unwrap_or("mod"),
            )
            .to_string(),
            path: nested.clone(),
        });
        notice = Some("Nested top-level folder detected. Hestia will import the inner folder as the mod root.".to_string());
    } else if top_level_dirs.len() > 1 && top_level_files.is_empty() {
        for dir in top_level_dirs {
            candidates.push(ImportCandidate {
                label: strip_uuid_prefix(
                    dir.file_name().and_then(OsStr::to_str).unwrap_or("mod"),
                )
                .to_string(),
                path: dir,
            });
        }
        notice = Some("Multiple top-level folders detected. Choose which folder should be treated as the mod root.".to_string());
    } else if top_level_dirs.is_empty() && top_level_files.is_empty() {
        bail!("import source is empty");
    } else {
        let label = match source {
            ImportSource::Folder(path) | ImportSource::Archive(path) => import_source_label(path),
        };
        candidates.push(ImportCandidate {
            label,
            path: root.to_path_buf(),
        });
    }

    Ok(ImportInspection {
        game_id: game_id.to_string(),
        candidates,
        notice,
    })
}

#[allow(dead_code)]
fn extract_archive(archive: &Path, destination: &Path) -> Result<()> {
    extract_archive_impl(archive, destination, None)
}

fn extract_archive_cancelable(
    archive: &Path,
    destination: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
    extract_archive_impl(archive, destination, Some(cancel))
}

fn extract_archive_impl(
    archive: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    if let Some(flag) = cancel {
        check_cancel(flag)?;
    }
    if numeric_split_part(archive).is_some() {
        return extract_split_archive(archive, destination, cancel);
    }
    let extension = archive
        .extension()
        .and_then(OsStr::to_str)
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("unsupported archive with no extension"))?;

    match extension.as_str() {
        "zip" => extract_zip_with_cancel(archive, destination, cancel),
        "7z" => extract_7z_with_cancel(archive, destination, cancel),
        "rar" => extract_rar_with_cancel(archive, destination, cancel),
        _ => bail!("unsupported archive format: {}", extension),
    }
}

#[allow(dead_code)]
fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    extract_zip_with_cancel(archive, destination, None)
}

#[derive(Clone)]
struct ZipFileEntry {
    index: usize,
    outpath: PathBuf,
}

fn extract_zip_with_cancel(
    archive_path: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    if let Some(cancel) = cancel {
        check_cancel(cancel)?;
    }

    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let sanitized_top_level = zip_top_level_sanitize_map(&mut archive)?;
    let mut directories = HashSet::new();
    let mut files = Vec::new();
    let mut known_output_kinds = HashMap::<String, bool>::new();
    let mut has_conflicting_paths = false;

    for index in 0..archive.len() {
        if let Some(cancel) = cancel {
            check_cancel(cancel)?;
        }
        let entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("archive contains invalid path"))?;
        if zip_entry_is_ignored_metadata(&enclosed) {
            continue;
        }
        let relative = zip_entry_relative_path(&enclosed, sanitized_top_level.as_ref())?;
        let outpath = destination.join(relative);
        if entry.name().ends_with('/') {
            if !register_zip_output(&mut known_output_kinds, &outpath, true) {
                has_conflicting_paths = true;
            }
            directories.insert(outpath);
        } else {
            if let Some(parent) = outpath.parent() {
                directories.insert(parent.to_path_buf());
            }
            if !register_zip_output(&mut known_output_kinds, &outpath, false) {
                has_conflicting_paths = true;
            }
            files.push(ZipFileEntry { index, outpath });
        }
    }

    if zip_outputs_overlap(&known_output_kinds) {
        has_conflicting_paths = true;
    }

    if has_conflicting_paths {
        return extract_zip_serial(
            archive_path,
            destination,
            sanitized_top_level.as_ref(),
            cancel,
        );
    }

    let mut created_dirs = HashSet::new();
    ensure_directory(destination, &mut created_dirs)?;
    for directory in directories {
        ensure_directory(&directory, &mut created_dirs)?;
    }

    if files.is_empty() {
        return Ok(());
    }

    let chunk_size = (files.len() + ZIP_EXTRACT_POOL.current_num_threads() - 1)
        / ZIP_EXTRACT_POOL.current_num_threads();
    ZIP_EXTRACT_POOL.install(|| {
        files.par_chunks(chunk_size).try_for_each(|chunk| {
            let file = fs::File::open(archive_path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            for work in chunk {
                if let Some(cancel) = cancel {
                    check_cancel(cancel)?;
                }
                let mut entry = archive.by_index(work.index)?;
                copy_zip_entry(&mut entry, &work.outpath, cancel)?;
            }
            Ok(())
        })
    })
}

fn register_zip_output(
    outputs: &mut HashMap<String, bool>,
    path: &Path,
    is_directory: bool,
) -> bool {
    let key = path.to_string_lossy().to_lowercase();
    outputs.insert(key, is_directory).is_none()
}

fn zip_outputs_overlap(outputs: &HashMap<String, bool>) -> bool {
    outputs.iter().any(|(path, is_directory)| {
        let prefix = format!("{path}{}", std::path::MAIN_SEPARATOR);
        !is_directory && outputs.keys().any(|other| other.starts_with(&prefix))
    })
}

fn extract_zip_serial(
    archive_path: &Path,
    destination: &Path,
    sanitized_top_level: Option<&HashMap<OsString, OsString>>,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut created_dirs = HashSet::new();
    ensure_directory(destination, &mut created_dirs)?;
    for index in 0..archive.len() {
        if let Some(cancel) = cancel {
            check_cancel(cancel)?;
        }
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("archive contains invalid path"))?;
        if zip_entry_is_ignored_metadata(&enclosed) {
            continue;
        }
        let relative = zip_entry_relative_path(&enclosed, sanitized_top_level)?;
        let outpath = destination.join(relative);
        if entry.name().ends_with('/') {
            ensure_directory(&outpath, &mut created_dirs)?;
        } else {
            if let Some(parent) = outpath.parent() {
                ensure_directory(parent, &mut created_dirs)?;
            }
            copy_zip_entry(&mut entry, &outpath, cancel)?;
        }
    }
    Ok(())
}

fn copy_zip_entry<R: Read>(
    entry: &mut R,
    outpath: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    let mut outfile = create_file_with_parent_recovery(outpath)?;
    let mut buffer = vec![0; ZIP_COPY_BUFFER_BYTES];
    loop {
        if let Some(cancel) = cancel {
            check_cancel(cancel)?;
        }
        let bytes_read = entry.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        std::io::Write::write_all(&mut outfile, &buffer[..bytes_read])?;
    }
    Ok(())
}

fn ensure_directory(path: &Path, created_dirs: &mut HashSet<PathBuf>) -> Result<()> {
    if created_dirs.insert(path.to_path_buf()) {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn create_file_with_parent_recovery(path: &Path) -> Result<fs::File> {
    match fs::File::create(path) {
        Ok(file) => Ok(file),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow!("file has no parent directory: {}", path.display()))?;
            fs::create_dir_all(parent)?;
            Ok(fs::File::create(path)?)
        }
        Err(err) => Err(err.into()),
    }
}

fn extract_7z_with_cancel(
    archive: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    sevenz_rust::decompress_file(archive, destination).context("failed to extract .7z archive")?;
    if let Some(flag) = cancel {
        check_cancel(flag)?;
    }
    Ok(())
}

fn extract_rar_with_cancel(
    archive: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    match extract_rar_with_unrar(archive, destination, cancel) {
        Ok(()) => Ok(()),
        Err(unrar_err) => extract_rar_with_7z_fallback(archive, destination, cancel, unrar_err),
    }
}

fn extract_rar_with_unrar(
    archive: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    let mut archive = unrar::Archive::new(archive)
        .open_for_processing()
        .context("failed to open .rar archive")?;
    while let Some(header) = archive
        .read_header()
        .context("failed to read .rar header")?
    {
        if let Some(flag) = cancel {
            check_cancel(flag)?;
        }
        archive = header
            .extract_with_base(destination)
            .context("failed to extract .rar entry")?;
    }
    Ok(())
}

fn extract_rar_with_7z_fallback(
    archive: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
    unrar_error: anyhow::Error,
) -> Result<()> {
    if let Some(flag) = cancel {
        check_cancel(flag)?;
    }

    let Some(exe_path) = resolve_7z_executable() else {
        bail!(
            "{:#}\n7-Zip fallback unavailable. Install 7-Zip or add 7z/7za to PATH.",
            unrar_error
        );
    };

    fs::create_dir_all(destination)?;
    let output = Command::new(&exe_path)
        .arg("x")
        .arg("-y")
        .arg("-aoa")
        .arg(format!("-o{}", destination.display()))
        .arg(archive)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run 7-Zip fallback at {}", exe_path.display()))?;

    if let Some(flag) = cancel {
        check_cancel(flag)?;
    }

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!(
            "{:#}\n7-Zip fallback failed with status {}.",
            unrar_error,
            output.status
        );
    }
    bail!("{:#}\n7-Zip fallback failed: {}", unrar_error, stderr);
}

fn resolve_7z_executable() -> Option<PathBuf> {
    for candidate in ["7z.exe", "7z", "7za.exe", "7za"] {
        if let Some(path) = find_executable_on_path(candidate) {
            return Some(path);
        }
    }
    for path in [
        PathBuf::from(r"C:\Program Files\7-Zip\7z.exe"),
        PathBuf::from(r"C:\Program Files (x86)\7-Zip\7z.exe"),
    ] {
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_executable_on_path(exe_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn next_available_name(root: &Path, base_name: &str) -> PathBuf {
    let mut counter = 2;
    loop {
        let candidate = root.join(format!("{base_name} ({counter})"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

#[allow(dead_code)]
pub fn copy_dir(source: &Path, destination: &Path, replace_existing: bool) -> Result<()> {
    if destination.exists() && replace_existing {
        fs::remove_dir_all(destination)?;
    }
    let mut created_dirs = HashSet::new();
    ensure_directory(destination, &mut created_dirs)?;
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        validate_windows_relative_path(relative)?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            ensure_directory(&target, &mut created_dirs)?;
        } else {
            if let Some(parent) = target.parent() {
                ensure_directory(parent, &mut created_dirs)?;
            }
            copy_file_with_parent_recovery(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub fn copy_dir_cancelable(
    source: &Path,
    destination: &Path,
    replace_existing: bool,
    cancel: &CancelFlag,
) -> Result<()> {
    copy_dir_cancelable_with_progress(source, destination, replace_existing, cancel, &mut |_| {
        Ok(())
    })
}

/// `on_bytes` receives the running total of bytes copied so far, after each file. Returning an
/// error from it aborts the copy, so it can double as an extra cancellation point.
pub fn copy_dir_cancelable_with_progress(
    source: &Path,
    destination: &Path,
    replace_existing: bool,
    cancel: &CancelFlag,
    on_bytes: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<()> {
    check_cancel(cancel)?;
    if destination.exists() && replace_existing {
        fs::remove_dir_all(destination)?;
    }
    let mut created_dirs = HashSet::new();
    ensure_directory(destination, &mut created_dirs)?;
    let mut copied_bytes = 0u64;
    for entry in WalkDir::new(source) {
        check_cancel(cancel)?;
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        validate_windows_relative_path(relative)?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            ensure_directory(&target, &mut created_dirs)?;
        } else {
            if let Some(parent) = target.parent() {
                ensure_directory(parent, &mut created_dirs)?;
            }
            let bytes = copy_file_with_parent_recovery(entry.path(), &target)?;
            copied_bytes = copied_bytes.saturating_add(bytes);
            on_bytes(copied_bytes)?;
        }
    }
    Ok(())
}

fn copy_file_with_parent_recovery(source: &Path, destination: &Path) -> Result<u64> {
    match fs::copy(source, destination) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let parent = destination.parent().ok_or_else(|| {
                anyhow!("file has no parent directory: {}", destination.display())
            })?;
            fs::create_dir_all(parent)?;
            Ok(fs::copy(source, destination)?)
        }
        Err(err) => Err(err.into()),
    }
}

pub fn move_or_copy_archive_candidate_cancelable(
    source: &Path,
    destination: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
    check_cancel(cancel)?;
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device_rename_error(&err) => {
            copy_dir_cancelable(source, destination, false, cancel)
        }
        Err(err) => Err(err.into()),
    }
}

fn is_cross_device_rename_error(err: &io::Error) -> bool {
    #[cfg(windows)]
    {
        err.raw_os_error() == Some(17) // ERROR_NOT_SAME_DEVICE
    }
    #[cfg(not(windows))]
    {
        err.raw_os_error() == Some(18) // EXDEV
    }
}

pub fn install_candidate_cancelable(
    candidate_path: &Path,
    preferred_name: &str,
    target_root: &Path,
    choice: ConflictChoice,
    source_is_archive: bool,
    cancel: &CancelFlag,
) -> Result<Option<PathBuf>> {
    validate_install_folder_name(preferred_name)?;
    fs::create_dir_all(target_root)?;
    let initial_target = target_root.join(preferred_name);

    if initial_target.exists() {
        return match choice {
            ConflictChoice::Cancel => Ok(None),
            ConflictChoice::Replace => {
                // Replace removes the destination outright; carry the ⬢HESTIA settings
                // stash across so a hidden mod's saved in-game settings survive.
                let preserved_stash =
                    crate::integrations::xxmi_persist::read_stash_bytes(&initial_target);
                copy_dir_cancelable(candidate_path, &initial_target, true, cancel)?;
                crate::integrations::xxmi_persist::restore_stash_bytes(
                    &initial_target,
                    &preserved_stash,
                );
                Ok(Some(initial_target))
            }
            ConflictChoice::Merge => {
                copy_dir_cancelable(candidate_path, &initial_target, false, cancel)?;
                Ok(Some(initial_target))
            }
            ConflictChoice::KeepBoth => {
                let target = next_available_name(target_root, preferred_name);
                if source_is_archive {
                    move_or_copy_archive_candidate_cancelable(candidate_path, &target, cancel)?;
                } else {
                    copy_dir_cancelable(candidate_path, &target, false, cancel)?;
                }
                Ok(Some(target))
            }
        };
    }

    if source_is_archive {
        move_or_copy_archive_candidate_cancelable(candidate_path, &initial_target, cancel)?;
    } else {
        copy_dir_cancelable(candidate_path, &initial_target, false, cancel)?;
    }
    Ok(Some(initial_target))
}

/// How deep below the mod root a named preview file is still considered part of
/// this mod: covers `DISABLED_BY_HESTIA\<content>\preview.jpg` and a mixed-root
/// install whose actual mod lives one folder down.
const BUNDLED_PREVIEW_MAX_DEPTH: usize = 4;
const BUNDLED_PREVIEW_MAX_IMAGES: usize = 10;
const BUNDLED_PREVIEW_MAX_INI_BYTES: usize = 1024 * 1024;

/// File-name conventions mod authors and other managers use for a bundled
/// preview image: JASM's `.JASM_Cover`, MODORA's `.MODORA_Preview`, plain
/// `preview`/`cover`/`thumbnail` files.
fn is_bundled_preview_stem(stem: &str) -> bool {
    let stem = stem.trim_start_matches('.').to_ascii_lowercase();
    stem.contains("preview")
        || stem == "cover"
        || stem.ends_with("_cover")
        || stem == "thumbnail"
        || stem == "thumb"
}

fn is_bundled_preview_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff" | "bmp"
            )
        })
        .unwrap_or(false)
}

/// Whether the lowercased ini blob mentions `file_name` as a standalone token.
/// Plain `contains` would let a reference to `11.png` veto an unrelated
/// `1.png`, so both neighbors must be non-alphanumeric.
fn ini_blob_references_file(blob: &str, file_name: &str) -> bool {
    let needle = file_name.to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    let bytes = blob.as_bytes();
    for (idx, _) in blob.match_indices(&needle) {
        let end = idx + needle.len();
        let before_ok = idx == 0 || !bytes[idx - 1].is_ascii_alphanumeric();
        let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Find preview images a mod shipped with, best-candidate first.
///
/// Named preview files (`preview.*`, `.JASM_Cover.jpg`, ...) count anywhere
/// within [`BUNDLED_PREVIEW_MAX_DEPTH`]. Loose images only count directly in
/// the mod root or directly in the `DISABLED_BY_HESTIA` container, where a
/// stray image is a preview by convention rather than a texture. Any image a
/// `.ini` in the tree references is part of the mod itself and never adopted.
pub fn discover_bundled_preview_images(mod_root: &Path) -> Vec<PathBuf> {
    let mut named: Vec<(usize, u8, PathBuf)> = Vec::new();
    let mut loose: Vec<PathBuf> = Vec::new();
    let mut ini_paths: Vec<PathBuf> = Vec::new();

    let walker = WalkDir::new(mod_root)
        .max_depth(BUNDLED_PREVIEW_MAX_DEPTH)
        .into_iter()
        .filter_entry(|entry| {
            !(entry.file_type().is_dir()
                && (entry.file_name() == OsStr::new(crate::model::MOD_META_DIR)
                    || entry.file_name() == OsStr::new("__MACOSX")))
        });
    for entry in walker {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ini"))
        {
            ini_paths.push(path.to_path_buf());
            continue;
        }
        if !is_bundled_preview_extension(path) {
            continue;
        }
        let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
        if is_bundled_preview_stem(stem) {
            let rank = if stem.to_ascii_lowercase().contains("preview") {
                0
            } else {
                1
            };
            named.push((entry.depth(), rank, path.to_path_buf()));
        } else {
            let in_root = entry.depth() == 1;
            let in_disabled_container = entry.depth() == 2
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == OsStr::new(crate::model::DISABLED_CONTAINER));
            if in_root || in_disabled_container {
                loose.push(path.to_path_buf());
            }
        }
    }

    let mut ini_blob = String::new();
    for ini_path in &ini_paths {
        let Ok(mut bytes) = fs::read(ini_path) else {
            continue;
        };
        bytes.truncate(BUNDLED_PREVIEW_MAX_INI_BYTES);
        ini_blob.push_str(&String::from_utf8_lossy(&bytes).to_ascii_lowercase());
        ini_blob.push('\n');
    }
    let is_mod_asset = |path: &Path| {
        path.file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| ini_blob_references_file(&ini_blob, name))
    };
    named.retain(|(_, _, path)| !is_mod_asset(path));
    loose.retain(|path| !is_mod_asset(path));

    named.sort_by(|a, b| {
        (a.0, a.1, a.2.to_string_lossy().to_ascii_lowercase())
            .cmp(&(b.0, b.1, b.2.to_string_lossy().to_ascii_lowercase()))
    });
    // A numbered set sorts numerically (2.png before 10.png), everything else
    // falls back to name order behind it.
    loose.sort_by_key(|path| {
        let stem = path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let numeric = stem.parse::<u64>().map_or((1u8, 0u64), |value| (0, value));
        (
            numeric,
            path.file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_ascii_lowercase(),
        )
    });

    let mut result: Vec<PathBuf> = named.into_iter().map(|(_, _, path)| path).collect();
    result.extend(loose);
    result.truncate(BUNDLED_PREVIEW_MAX_IMAGES);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn uuid_prefix_is_stripped_from_labels_only_when_it_is_a_real_uuid() {
        assert_eq!(
            strip_uuid_prefix("8b7f2df5-61b7-409f-93bf-7948fd407fd4_洁尔佩塔 侦探 1.2"),
            "洁尔佩塔 侦探 1.2"
        );
        assert_eq!(
            strip_uuid_prefix("30AC753A-8368-4A58-81EB-4672DB2A7742_Name"),
            "Name"
        );
        // Not a uuid: hyphens in the wrong spots.
        assert_eq!(
            strip_uuid_prefix("8b7f2df561-b7-409f-93bf-7948fd407fd4_Name"),
            "8b7f2df561-b7-409f-93bf-7948fd407fd4_Name"
        );
        // A bare uuid keeps its name rather than becoming empty.
        assert_eq!(
            strip_uuid_prefix("8b7f2df5-61b7-409f-93bf-7948fd407fd4"),
            "8b7f2df5-61b7-409f-93bf-7948fd407fd4"
        );
        assert_eq!(
            strip_uuid_prefix("8b7f2df5-61b7-409f-93bf-7948fd407fd4_"),
            "8b7f2df5-61b7-409f-93bf-7948fd407fd4_"
        );
        assert_eq!(strip_uuid_prefix("ordinary mod name"), "ordinary mod name");
    }

    #[test]
    fn nested_uuid_folder_candidate_gets_a_clean_label() {
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path().join("outer");
        let nested = outer.join("8aca8d5d-97ba-40af-95ac-3346ebfc26d8_弧光 拉舒莎 1.2");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("mod.ini"), "x").unwrap();

        let inspection =
            inspect_directory("game", &ImportSource::Folder(outer.clone()), &outer).unwrap();
        assert_eq!(inspection.candidates.len(), 1);
        assert_eq!(inspection.candidates[0].label, "弧光 拉舒莎 1.2");
    }

    #[test]
    fn bundled_preview_discovery_orders_named_before_loose_and_skips_ini_assets() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join(".JASM_Cover.jpg"), b"img").unwrap();
        fs::write(root.join("nested").join("preview.webp"), b"img").unwrap();
        fs::write(root.join("10.png"), b"img").unwrap();
        fs::write(root.join("2.png"), b"img").unwrap();
        fs::write(root.join("tex.png"), b"img").unwrap();
        fs::write(root.join("mod.ini"), "[ResourceTex]\nfilename = tex.png\n").unwrap();

        let found = discover_bundled_preview_images(root);
        let names: Vec<String> = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec![".JASM_Cover.jpg", "preview.webp", "2.png", "10.png"]);
    }

    #[test]
    fn bundled_preview_discovery_reads_disabled_container_and_skips_meta_dir() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let disabled = root.join(crate::model::DISABLED_CONTAINER);
        fs::create_dir_all(&disabled).unwrap();
        fs::create_dir_all(root.join(crate::model::MOD_META_DIR)).unwrap();
        fs::write(disabled.join("preview.jpg"), b"img").unwrap();
        fs::write(disabled.join("1.png"), b"img").unwrap();
        fs::write(
            root.join(crate::model::MOD_META_DIR).join("manual_a.jpg"),
            b"img",
        )
        .unwrap();

        let found = discover_bundled_preview_images(root);
        let names: Vec<String> = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["preview.jpg", "1.png"]);
    }

    #[test]
    fn ini_reference_check_requires_token_boundaries() {
        assert!(ini_blob_references_file("filename = 1.png", "1.png"));
        assert!(ini_blob_references_file("filename=.\\sub\\1.png\r\n", "1.png"));
        assert!(!ini_blob_references_file("filename = 11.png", "1.png"));
        assert!(!ini_blob_references_file("filename = 1.pngx", "1.png"));
        assert!(!ini_blob_references_file("", "1.png"));
    }

    #[test]
    fn nested_folder_becomes_single_candidate() {
        let temp = tempfile::tempdir().unwrap();
        let outer = temp.path().join("outer");
        let inner = outer.join("InnerMod");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("mod.txt"), "demo").unwrap();

        let inspected = inspect_source("wuwa", ImportSource::Folder(outer)).unwrap();
        assert_eq!(inspected.inspection.candidates.len(), 1);
        assert_eq!(inspected.inspection.candidates[0].label, "InnerMod");
        assert!(inspected.inspection.notice.is_some());
    }

    #[test]
    fn mixed_root_treats_folder_as_mod_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("MixedMod");
        fs::create_dir_all(root.join("Shaders")).unwrap();
        fs::write(root.join("README.txt"), "mod").unwrap();

        let inspected = inspect_source("wuwa", ImportSource::Folder(root.clone())).unwrap();
        assert_eq!(inspected.inspection.candidates.len(), 1);
        assert_eq!(inspected.inspection.candidates[0].path, root);
    }

    #[test]
    fn windows_path_validation_rejects_invalid_names() {
        for path in [
            Path::new("Bad:Name/file.txt"),
            Path::new("Bad<Name/file.txt"),
            Path::new("BadName./file.txt"),
            Path::new("BadName /file.txt"),
            Path::new("CON/readme.txt"),
            Path::new("aux.ini"),
            Path::new("nested/LPT1.cfg"),
        ] {
            assert!(
                validate_windows_relative_path(path).is_err(),
                "{} should be rejected",
                path.display()
            );
        }

        validate_windows_relative_path(Path::new("Good Name/readme.txt")).unwrap();
    }

    #[test]
    fn zip_extract_sanitizes_top_level_candidate_folder_name() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("outer-name.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("Bad:Name/readme.txt", options).unwrap();
            writer.write_all(b"demo").unwrap();
            writer.finish().unwrap();
        }

        let destination = temp.path().join("extract");
        extract_zip(&archive_path, &destination).unwrap();
        assert!(destination.join("Bad_Name").join("readme.txt").exists());
    }

    #[test]
    fn zip_extract_ignores_macos_metadata_while_sanitizing_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("macos-metadata.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("__MACOSX/._Bad:Name", options).unwrap();
            writer.write_all(b"metadata").unwrap();
            writer.start_file("Bad:Name/readme.txt", options).unwrap();
            writer.write_all(b"demo").unwrap();
            writer.finish().unwrap();
        }

        let destination = temp.path().join("extract");
        extract_zip(&archive_path, &destination).unwrap();
        assert!(destination.join("Bad_Name").join("readme.txt").exists());
        assert!(!destination.join("__MACOSX").exists());
    }

    #[test]
    fn zip_extract_rejects_windows_invalid_payload_names() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("invalid-payload.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("GoodMod/Bad:Name/readme.txt", options)
                .unwrap();
            writer.write_all(b"demo").unwrap();
            writer.finish().unwrap();
        }

        let destination = temp.path().join("extract");
        let err = extract_zip(&archive_path, &destination).unwrap_err();
        assert!(
            err.to_string().contains("invalid file name"),
            "unexpected error: {err:#}"
        );
        assert!(!destination.join("GoodMod").join("Bad:Name").exists());
    }

    #[test]
    fn zip_extracts_independent_files_with_parallel_workers() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("parallel.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            for index in 0..24 {
                writer
                    .start_file(format!("Mod/files/{index}.txt"), options)
                    .unwrap();
                writer
                    .write_all(format!("payload-{index}").as_bytes())
                    .unwrap();
            }
            writer.finish().unwrap();
        }

        let destination = temp.path().join("extract");
        extract_zip(&archive_path, &destination).unwrap();
        for index in 0..24 {
            assert_eq!(
                fs::read_to_string(
                    destination
                        .join("Mod")
                        .join("files")
                        .join(format!("{index}.txt"))
                )
                .unwrap(),
                format!("payload-{index}")
            );
        }
    }

    #[test]
    fn archive_candidate_moves_when_target_is_on_same_volume() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("extracted");
        let target_root = temp.path().join("mods");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested").join("mod.ini"), "demo").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        let installed = install_candidate_cancelable(
            &source,
            "Installed",
            &target_root,
            ConflictChoice::KeepBoth,
            true,
            &cancel,
        )
        .unwrap()
        .unwrap();

        assert_eq!(installed, target_root.join("Installed"));
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(installed.join("nested").join("mod.ini")).unwrap(),
            "demo"
        );
    }

    fn write_demo_zip(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("Mod/readme.txt", options).unwrap();
        writer.write_all(b"split demo payload").unwrap();
        writer.finish().unwrap();
    }

    fn split_file_into_parts(source: &Path, part_paths: &[PathBuf]) {
        let bytes = fs::read(source).unwrap();
        let chunk = bytes.len().div_ceil(part_paths.len());
        for (index, part) in part_paths.iter().enumerate() {
            let start = index * chunk;
            let end = (start + chunk).min(bytes.len());
            fs::write(part, &bytes[start..end]).unwrap();
        }
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn numeric_split_part_detects_supported_bases_only() {
        let (base, part) = numeric_split_part(Path::new("mods/Foo.rar.0001")).unwrap();
        assert_eq!(base, Path::new("mods/Foo.rar"));
        assert_eq!(part, 1);
        let (base, part) = numeric_split_part(Path::new("Foo.7z.002")).unwrap();
        assert_eq!(base, Path::new("Foo.7z"));
        assert_eq!(part, 2);
        assert!(numeric_split_part(Path::new("Foo.rar")).is_none());
        assert!(numeric_split_part(Path::new("Foo.part1.rar")).is_none());
        assert!(numeric_split_part(Path::new("Foo.0001")).is_none());
        assert!(numeric_split_part(Path::new("Foo.txt.0001")).is_none());
    }

    #[test]
    fn split_zip_volumes_extract_after_joining() {
        let temp = tempfile::tempdir().unwrap();
        let whole = temp.path().join("Cool Mod.zip");
        write_demo_zip(&whole);
        let parts = [
            temp.path().join("Cool Mod.zip.001"),
            temp.path().join("Cool Mod.zip.002"),
        ];
        split_file_into_parts(&whole, &parts);

        let destination = temp.path().join("extract");
        extract_archive(&parts[0], &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("Mod").join("readme.txt")).unwrap(),
            "split demo payload"
        );

        // Any part of the set resolves to the same full extraction.
        let from_second = temp.path().join("extract-second");
        extract_archive(&parts[1], &from_second).unwrap();
        assert!(from_second.join("Mod").join("readme.txt").exists());
    }

    #[test]
    fn split_archive_with_missing_part_reports_the_gap() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Foo.zip.001"), b"a").unwrap();
        fs::write(temp.path().join("Foo.zip.003"), b"c").unwrap();

        let destination = temp.path().join("extract");
        let err = extract_archive(&temp.path().join("Foo.zip.001"), &destination).unwrap_err();
        assert!(
            err.to_string().contains("missing part 2"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn split_archive_missing_first_part_reports_it() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Foo.zip.0002"), b"b").unwrap();

        let destination = temp.path().join("extract");
        let err = extract_archive(&temp.path().join("Foo.zip.0002"), &destination).unwrap_err();
        assert!(
            err.to_string().contains("missing part 1"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn resolve_split_archive_maps_every_part_to_the_first_volume() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("Foo.rar.0001");
        let second = temp.path().join("Foo.rar.0002");
        fs::write(&first, b"a").unwrap();
        fs::write(&second, b"b").unwrap();
        assert_eq!(resolve_split_archive(&second).unwrap(), first);
        assert_eq!(resolve_split_archive(&first).unwrap(), first);

        let part1 = temp.path().join("Bar.part1.rar");
        let part2 = temp.path().join("Bar.part2.rar");
        fs::write(&part1, b"a").unwrap();
        fs::write(&part2, b"b").unwrap();
        assert_eq!(resolve_split_archive(&part2).unwrap(), part1);

        let main = temp.path().join("Old.rar");
        let old_part = temp.path().join("Old.r00");
        fs::write(&main, b"a").unwrap();
        fs::write(&old_part, b"b").unwrap();
        assert_eq!(resolve_split_archive(&old_part).unwrap(), main);

        assert!(resolve_split_archive(&temp.path().join("Plain.zip")).is_none());
    }

    #[test]
    fn source_display_file_name_strips_part_suffixes() {
        assert_eq!(
            source_display_file_name(Path::new("Cool Mod.7z.001")),
            "Cool Mod.7z"
        );
        assert_eq!(
            source_display_file_name(Path::new("Cool Mod.rar.0002")),
            "Cool Mod.rar"
        );
        assert_eq!(
            source_display_file_name(Path::new("Cool Mod.part2.rar")),
            "Cool Mod.rar"
        );
        assert_eq!(
            source_display_file_name(Path::new("Cool Mod.zip")),
            "Cool Mod.zip"
        );
    }

    #[test]
    fn archive_source_total_size_sums_all_volumes() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("Foo.7z.001");
        let second = temp.path().join("Foo.7z.002");
        fs::write(&first, vec![0u8; 10]).unwrap();
        fs::write(&second, vec![0u8; 7]).unwrap();
        assert_eq!(archive_source_total_size(&first), Some(17));
        assert_eq!(archive_source_total_size(&second), Some(17));

        let part1 = temp.path().join("Bar.part1.rar");
        let part2 = temp.path().join("Bar.part2.rar");
        fs::write(&part1, vec![0u8; 4]).unwrap();
        fs::write(&part2, vec![0u8; 3]).unwrap();
        assert_eq!(archive_source_total_size(&part1), Some(7));

        let plain = temp.path().join("Baz.zip");
        fs::write(&plain, vec![0u8; 5]).unwrap();
        assert_eq!(archive_source_total_size(&plain), Some(5));
    }

    #[test]
    fn archive_picker_extensions_cover_formats_and_first_part_suffixes() {
        let extensions = archive_picker_extensions();
        for expected in ["zip", "rar", "7z", "001", "0001"] {
            assert!(
                extensions.iter().any(|ext| *ext == expected),
                "missing extension {expected}"
            );
        }
        assert!(
            extensions.len() < 10,
            "picker filter must stay small; Windows' file dialog crashes on huge filters"
        );
    }

    #[test]
    fn import_source_label_strips_split_volume_suffixes() {
        assert_eq!(
            import_source_label(Path::new("Cool Mod.rar.0001")),
            "Cool Mod"
        );
        assert_eq!(
            import_source_label(Path::new("Cool Mod.part1.rar")),
            "Cool Mod"
        );
        assert_eq!(import_source_label(Path::new("Cool Mod.zip")), "Cool Mod");
    }

    #[test]
    fn folder_candidate_is_copied_without_moving_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target_root = temp.path().join("mods");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("mod.ini"), "demo").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        let installed = install_candidate_cancelable(
            &source,
            "Installed",
            &target_root,
            ConflictChoice::KeepBoth,
            false,
            &cancel,
        )
        .unwrap()
        .unwrap();

        assert!(source.exists());
        assert_eq!(fs::read_to_string(source.join("mod.ini")).unwrap(), "demo");
        assert_eq!(
            fs::read_to_string(installed.join("mod.ini")).unwrap(),
            "demo"
        );
    }
}
