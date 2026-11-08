use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow, bail};
use once_cell::sync::Lazy;
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tempfile::TempDir;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::model::{
    ConflictChoice, DISABLED_CONTAINER, GameBackend, ImportCandidate, ImportInspection,
    ImportSource, MOD_META_DIR, ModStatus,
};
use crate::persistence;

pub mod bundle;

pub const CANCELLED_ERROR: &str = "install canceled";
pub type CancelFlag = Arc<AtomicBool>;

pub struct PreparedImport {
    pub _temp_dir: Option<TempDir>,
    pub inspection: ImportInspection,
    pub source_is_archive: bool,
}

const ZIP_COPY_BUFFER_BYTES: usize = 256 * 1024;
const ZIP_EXTRACT_WORKERS: usize = 4;
const UNPACK_RATIO_LIMIT: u64 = 100;
const UNPACK_ALWAYS_ALLOWED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const RAR_HEADER_NAME_CAPACITY: usize = 1024;
const RAR_REDIR_NAME_CAPACITY: usize = 2048;
const RAR_REDIR_NONE: u32 = 0;
const RAR_REDIR_HARDLINK: u32 = 4;
const RAR_REDIR_FILECOPY: u32 = 5;

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

pub fn archive_picker_extensions() -> &'static [&'static str] {
    &["zip", "rar", "7z", "001", "0001"]
}

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

fn strip_uuid_prefix(name: &str) -> &str {
    const UUID_PREFIX_LEN: usize = 37;
    if name.len() > UUID_PREFIX_LEN && name.is_char_boundary(UUID_PREFIX_LEN) {
        let (prefix, rest) = name.split_at(UUID_PREFIX_LEN);
        if prefix.ends_with('_') && uuid::Uuid::parse_str(&prefix[..UUID_PREFIX_LEN - 1]).is_ok() {
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

#[derive(Clone)]
struct ArchiveListing {
    name: String,
    is_directory: bool,
    size: u64,
}

struct PlannedArchiveEntry {
    index: usize,
    outpath: PathBuf,
    is_directory: bool,
    size: u64,
}

struct ArchivePlan {
    entries: Vec<PlannedArchiveEntry>,
    unpacked_bytes: u64,
    has_conflicting_paths: bool,
}

fn plan_archive_extraction(listing: &[ArchiveListing], destination: &Path) -> Result<ArchivePlan> {
    let sanitized_top_level = archive_top_level_sanitize_map(listing)?;
    let mut entries = Vec::new();
    let mut unpacked_bytes = 0u64;
    let mut known_output_kinds = HashMap::<String, bool>::new();
    let mut has_conflicting_paths = false;
    for (index, item) in listing.iter().enumerate() {
        let parts = archive_entry_parts(&item.name)?;
        if archive_entry_is_ignored_metadata(&parts) {
            continue;
        }
        let relative =
            archive_entry_relative_path(&item.name, &parts, sanitized_top_level.as_ref())?;
        let outpath = destination.join(relative);
        if !register_archive_output(&mut known_output_kinds, &outpath, item.is_directory) {
            has_conflicting_paths = true;
        }
        if !item.is_directory {
            unpacked_bytes = unpacked_bytes.saturating_add(item.size);
        }
        entries.push(PlannedArchiveEntry {
            index,
            outpath,
            is_directory: item.is_directory,
            size: item.size,
        });
    }
    if archive_outputs_overlap(&known_output_kinds) {
        has_conflicting_paths = true;
    }
    Ok(ArchivePlan {
        entries,
        unpacked_bytes,
        has_conflicting_paths,
    })
}

fn archive_top_level_sanitize_map(
    listing: &[ArchiveListing],
) -> Result<Option<HashMap<OsString, OsString>>> {
    let mut has_top_level_dir = false;
    let mut has_top_level_file = false;
    let mut original_names = HashMap::<OsString, OsString>::new();
    let mut sanitized_keys = HashMap::<String, OsString>::new();
    for item in listing {
        let parts = archive_entry_parts(&item.name)?;
        if archive_entry_is_ignored_metadata(&parts) {
            continue;
        }
        let first = OsStr::new(parts[0]);
        if parts.len() > 1 || item.is_directory {
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

fn archive_entry_parts(name: &str) -> Result<Vec<&str>> {
    let invalid = || anyhow!("archive contains invalid path: {name}");
    if name.contains('\0') {
        return Err(invalid());
    }
    let mut parts = Vec::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or_else(invalid)?;
            }
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(invalid());
    }
    Ok(parts)
}

fn archive_entry_relative_path(
    name: &str,
    parts: &[&str],
    sanitized_top_level: Option<&HashMap<OsString, OsString>>,
) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for (index, part) in parts.iter().enumerate() {
        let part = OsStr::new(part);
        if index == 0 {
            if let Some(map) = sanitized_top_level {
                let sanitized = map
                    .get(part)
                    .ok_or_else(|| anyhow!("archive contains invalid path: {name}"))?;
                out.push(sanitized);
                continue;
            }
        }
        validate_windows_file_name(part)
            .with_context(|| format!("archive contains invalid file name: {name}"))?;
        out.push(part);
    }
    Ok(out)
}

fn archive_entry_is_ignored_metadata(parts: &[&str]) -> bool {
    parts.first() == Some(&"__MACOSX")
}

fn check_unpacked_size(packed_bytes: u64, unpacked_bytes: u64, destination: &Path) -> Result<()> {
    let probe = destination
        .ancestors()
        .find(|path| path.exists())
        .unwrap_or(destination);
    let free_bytes = fs2::available_space(probe).ok();
    check_unpack_limits(packed_bytes, unpacked_bytes, free_bytes)
}

fn check_unpack_limits(
    packed_bytes: u64,
    unpacked_bytes: u64,
    free_bytes: Option<u64>,
) -> Result<()> {
    let ratio_limit = packed_bytes
        .saturating_mul(UNPACK_RATIO_LIMIT)
        .max(UNPACK_ALWAYS_ALLOWED_BYTES);
    if unpacked_bytes > ratio_limit {
        bail!(
            "archive would unpack to {} from {}, more than {UNPACK_RATIO_LIMIT} times its size",
            crate::app::format_file_size(unpacked_bytes),
            crate::app::format_file_size(packed_bytes)
        );
    }
    if let Some(free_bytes) = free_bytes
        && unpacked_bytes > free_bytes
    {
        bail!(
            "not enough free disk space: unpacking needs {}, only {} is free",
            crate::app::format_file_size(unpacked_bytes),
            crate::app::format_file_size(free_bytes)
        );
    }
    Ok(())
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
            label: strip_uuid_prefix(nested.file_name().and_then(OsStr::to_str).unwrap_or("mod"))
                .to_string(),
            path: nested.clone(),
        });
        notice = Some("Nested top-level folder detected. Hestia will import the inner folder as the mod root.".to_string());
    } else if top_level_dirs.len() > 1 && top_level_files.is_empty() {
        for dir in top_level_dirs {
            candidates.push(ImportCandidate {
                label: strip_uuid_prefix(dir.file_name().and_then(OsStr::to_str).unwrap_or("mod"))
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
            label: strip_uuid_prefix(nested.file_name().and_then(OsStr::to_str).unwrap_or("mod"))
                .to_string(),
            path: nested.clone(),
        });
        notice = Some("Nested top-level folder detected. Hestia will import the inner folder as the mod root.".to_string());
    } else if top_level_dirs.len() > 1 && top_level_files.is_empty() {
        for dir in top_level_dirs {
            candidates.push(ImportCandidate {
                label: strip_uuid_prefix(dir.file_name().and_then(OsStr::to_str).unwrap_or("mod"))
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
    let listing = zip_listing(&mut archive, cancel)?;
    let plan = plan_archive_extraction(&listing, destination)?;
    check_unpacked_size(
        fs::metadata(archive_path)?.len(),
        plan.unpacked_bytes,
        destination,
    )?;

    let mut created_dirs = HashSet::new();
    ensure_directory(destination, &mut created_dirs)?;
    if plan.has_conflicting_paths {
        return extract_zip_serial(&mut archive, &plan.entries, &mut created_dirs, cancel);
    }

    for planned in &plan.entries {
        let directory = if planned.is_directory {
            Some(planned.outpath.as_path())
        } else {
            planned.outpath.parent()
        };
        if let Some(directory) = directory {
            ensure_directory(directory, &mut created_dirs)?;
        }
    }

    let files: Vec<&PlannedArchiveEntry> = plan
        .entries
        .iter()
        .filter(|planned| !planned.is_directory)
        .collect();
    if files.is_empty() {
        return Ok(());
    }

    let chunk_size = (files.len() + ZIP_EXTRACT_POOL.current_num_threads() - 1)
        / ZIP_EXTRACT_POOL.current_num_threads();
    ZIP_EXTRACT_POOL.install(|| {
        files.par_chunks(chunk_size).try_for_each(|chunk| {
            let file = fs::File::open(archive_path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            for planned in chunk {
                if let Some(cancel) = cancel {
                    check_cancel(cancel)?;
                }
                let mut entry = archive.by_index(planned.index)?;
                copy_archive_entry(&mut entry, planned, cancel)?;
            }
            Ok(())
        })
    })
}

fn zip_listing(
    archive: &mut zip::ZipArchive<fs::File>,
    cancel: Option<&CancelFlag>,
) -> Result<Vec<ArchiveListing>> {
    let mut listing = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        if let Some(cancel) = cancel {
            check_cancel(cancel)?;
        }
        let entry = archive.by_index(index)?;
        listing.push(ArchiveListing {
            name: entry.name()?.into_owned(),
            is_directory: entry.name_raw().ends_with(b"/"),
            size: entry.size(),
        });
    }
    Ok(listing)
}

fn register_archive_output(
    outputs: &mut HashMap<String, bool>,
    path: &Path,
    is_directory: bool,
) -> bool {
    let key = path.to_string_lossy().to_lowercase();
    outputs.insert(key, is_directory).is_none()
}

fn archive_outputs_overlap(outputs: &HashMap<String, bool>) -> bool {
    outputs.iter().any(|(path, is_directory)| {
        let prefix = format!("{path}{}", std::path::MAIN_SEPARATOR);
        !is_directory && outputs.keys().any(|other| other.starts_with(&prefix))
    })
}

fn extract_zip_serial(
    archive: &mut zip::ZipArchive<fs::File>,
    entries: &[PlannedArchiveEntry],
    created_dirs: &mut HashSet<PathBuf>,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    for planned in entries {
        if let Some(cancel) = cancel {
            check_cancel(cancel)?;
        }
        if planned.is_directory {
            ensure_directory(&planned.outpath, created_dirs)?;
            continue;
        }
        if let Some(parent) = planned.outpath.parent() {
            ensure_directory(parent, created_dirs)?;
        }
        let mut entry = archive.by_index(planned.index)?;
        copy_archive_entry(&mut entry, planned, cancel)?;
    }
    Ok(())
}

fn copy_archive_entry<R: Read + ?Sized>(
    entry: &mut R,
    planned: &PlannedArchiveEntry,
    cancel: Option<&CancelFlag>,
) -> Result<fs::File> {
    let mut outfile = create_file_with_parent_recovery(&planned.outpath)?;
    let mut buffer = vec![0; ZIP_COPY_BUFFER_BYTES];
    let mut written = 0u64;
    loop {
        if let Some(cancel) = cancel {
            check_cancel(cancel)?;
        }
        let bytes_read = entry.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        written = written.saturating_add(bytes_read as u64);
        if written > planned.size {
            bail!("archive entry is larger than its listed size");
        }
        std::io::Write::write_all(&mut outfile, &buffer[..bytes_read])?;
    }
    Ok(outfile)
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
    archive_path: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    if let Some(flag) = cancel {
        check_cancel(flag)?;
    }
    let mut reader =
        sevenz_rust2::ArchiveReader::open(archive_path, sevenz_rust2::Password::empty())
            .context("failed to open .7z archive")?;
    let listing: Vec<ArchiveListing> = reader
        .archive()
        .files
        .iter()
        .filter(|entry| !entry.is_anti_item())
        .map(|entry| ArchiveListing {
            name: entry.name().to_string(),
            is_directory: entry.is_directory(),
            size: entry.size(),
        })
        .collect();
    let plan = plan_archive_extraction(&listing, destination)?;
    check_unpacked_size(
        fs::metadata(archive_path)?.len(),
        plan.unpacked_bytes,
        destination,
    )?;
    let planned_by_name: HashMap<&str, &PlannedArchiveEntry> = plan
        .entries
        .iter()
        .map(|planned| (listing[planned.index].name.as_str(), planned))
        .collect();

    let mut created_dirs = HashSet::new();
    ensure_directory(destination, &mut created_dirs)?;
    let mut failure = None;
    let result = reader.for_each_entries_with_decision(|entry, data| {
        if entry.is_anti_item() {
            return Ok(sevenz_rust2::EntryDecision::Continue);
        }
        let Some(planned) = planned_by_name.get(entry.name()) else {
            return Ok(sevenz_rust2::EntryDecision::Continue);
        };
        match write_7z_entry(entry, data, planned, &mut created_dirs, cancel) {
            Ok(()) => Ok(sevenz_rust2::EntryDecision::Continue),
            Err(err) => {
                failure = Some(err);
                Ok(sevenz_rust2::EntryDecision::Stop)
            }
        }
    });
    if let Some(err) = failure {
        return Err(err);
    }
    result.context("failed to extract .7z archive")?;
    Ok(())
}

fn write_7z_entry(
    entry: &sevenz_rust2::ArchiveEntry,
    data: &mut dyn Read,
    planned: &PlannedArchiveEntry,
    created_dirs: &mut HashSet<PathBuf>,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    if planned.is_directory {
        return ensure_directory(&planned.outpath, created_dirs);
    }
    if let Some(parent) = planned.outpath.parent() {
        ensure_directory(parent, created_dirs)?;
    }
    let file = copy_archive_entry(data, planned, cancel)?;
    let _ = file.set_times(seven_zip_file_times(entry));
    Ok(())
}

fn seven_zip_file_times(entry: &sevenz_rust2::ArchiveEntry) -> fs::FileTimes {
    let mut times = fs::FileTimes::new();
    if entry.has_access_date
        && let Some(time) = nt_time_to_system_time(entry.access_date())
    {
        times = times.set_accessed(time);
    }
    if entry.has_last_modified_date
        && let Some(time) = nt_time_to_system_time(entry.last_modified_date())
    {
        times = times.set_modified(time);
    }
    #[cfg(windows)]
    if entry.has_creation_date
        && let Some(time) = nt_time_to_system_time(entry.creation_date())
    {
        times = std::os::windows::fs::FileTimesExt::set_created(times, time);
    }
    times
}

fn nt_time_to_system_time(time: sevenz_rust2::NtTime) -> Option<SystemTime> {
    const TICKS_PER_SECOND: u64 = 10_000_000;
    const SECONDS_FROM_1601_TO_1970: u64 = 11_644_473_600;
    let ticks = u64::from(time);
    let since_1601 = Duration::new(
        ticks / TICKS_PER_SECOND,
        ((ticks % TICKS_PER_SECOND) * 100) as u32,
    );
    SystemTime::UNIX_EPOCH
        .checked_sub(Duration::from_secs(SECONDS_FROM_1601_TO_1970))?
        .checked_add(since_1601)
}

fn extract_rar_with_cancel(
    archive: &Path,
    destination: &Path,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    if let Some(flag) = cancel {
        check_cancel(flag)?;
    }
    let entries = match rar_listing(archive) {
        Ok(entries) => entries,
        Err(unrar_err) => {
            return extract_rar_with_7z_fallback(archive, destination, cancel, unrar_err);
        }
    };
    let plan = plan_rar_extraction(&entries, destination)?;
    check_unpacked_size(
        rar_packed_size(archive)?,
        plan.archive.unpacked_bytes,
        destination,
    )?;
    match extract_rar_with_unrar(archive, &entries, &plan, cancel) {
        Ok(()) => Ok(()),
        Err(unrar_err) => extract_rar_with_7z_fallback(archive, destination, cancel, unrar_err),
    }
}

enum RarEntryKind {
    Regular,
    CopyOf(String),
    Link,
}

struct RarListedEntry {
    listing: ArchiveListing,
    kind: RarEntryKind,
}

struct RarCopy {
    source: PathBuf,
    target: PathBuf,
}

struct RarPlan {
    archive: ArchivePlan,
    planned_index_by_entry: Vec<Option<usize>>,
    copies: Vec<RarCopy>,
}

struct RarArchiveHandle(*const unrar_ng_sys::Handle);

impl Drop for RarArchiveHandle {
    fn drop(&mut self) {
        unsafe {
            unrar_ng_sys::RARCloseArchive(self.0);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "netbsd"))]
fn rar_native_path(path: &Path) -> Result<std::ffi::CString> {
    std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| anyhow!("archive path contains a nul character"))
}

#[cfg(not(any(target_os = "linux", target_os = "netbsd")))]
fn rar_native_path(path: &Path) -> Result<widestring::WideCString> {
    widestring::WideCString::from_os_str(path)
        .map_err(|_| anyhow!("archive path contains a nul character"))
}

fn rar_wide_text<T>(text: *const T, capacity: usize) -> Result<String> {
    let text = unsafe { widestring::WideCString::from_ptr_truncate(text.cast(), capacity) };
    text.to_string()
        .map_err(|_| anyhow!("archive contains invalid path: {}", text.display()))
}

fn rar_listing(archive: &Path) -> Result<Vec<RarListedEntry>> {
    let archive_name = rar_native_path(archive)?;
    let mut open_data = unrar_ng_sys::OpenArchiveDataEx::new(
        archive_name.as_ptr().cast(),
        unrar_ng_sys::RAR_OM_LIST,
    );
    let handle = unsafe { unrar_ng_sys::RAROpenArchiveEx(&raw mut open_data) };
    let open_result = open_data.open_result;
    if handle.is_null() {
        bail!("failed to open .rar archive (unrar code {open_result})");
    }
    let handle = RarArchiveHandle(handle);
    if open_result != unrar_ng_sys::ERAR_SUCCESS as u32 {
        bail!("failed to open .rar archive (unrar code {open_result})");
    }

    let mut redir_name = vec![0 as widestring::WideChar; RAR_REDIR_NAME_CAPACITY];
    let mut entries = Vec::new();
    loop {
        let mut header = Box::new(unrar_ng_sys::HeaderDataEx::default());
        header.redir_name = redir_name.as_mut_ptr().cast();
        header.redir_name_size = RAR_REDIR_NAME_CAPACITY as u32;
        let read_result = unsafe { unrar_ng_sys::RARReadHeaderEx(handle.0, &raw mut *header) };
        if read_result == unrar_ng_sys::ERAR_END_ARCHIVE {
            break;
        }
        if read_result != unrar_ng_sys::ERAR_SUCCESS {
            bail!("failed to read .rar header (unrar code {read_result})");
        }
        let name = rar_wide_text(&raw const header.filename_w, RAR_HEADER_NAME_CAPACITY)?;
        let kind = match header.redir_type {
            RAR_REDIR_NONE => RarEntryKind::Regular,
            RAR_REDIR_HARDLINK | RAR_REDIR_FILECOPY => RarEntryKind::CopyOf(rar_wide_text(
                redir_name.as_ptr(),
                RAR_REDIR_NAME_CAPACITY,
            )?),
            _ => RarEntryKind::Link,
        };
        entries.push(RarListedEntry {
            listing: ArchiveListing {
                name,
                is_directory: header.flags & unrar_ng_sys::RHDF_DIRECTORY != 0,
                size: (u64::from(header.unp_size_high) << 32) | u64::from(header.unp_size),
            },
            kind,
        });
        let skip_result = unsafe {
            unrar_ng_sys::RARProcessFile(
                handle.0,
                unrar_ng_sys::RAR_SKIP,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if skip_result != unrar_ng_sys::ERAR_SUCCESS {
            bail!("failed to read .rar archive (unrar code {skip_result})");
        }
    }
    Ok(entries)
}

fn plan_rar_extraction(entries: &[RarListedEntry], destination: &Path) -> Result<RarPlan> {
    let plannable: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !matches!(entry.kind, RarEntryKind::Link))
        .map(|(index, _)| index)
        .collect();
    let listing: Vec<ArchiveListing> = plannable
        .iter()
        .map(|&index| entries[index].listing.clone())
        .collect();
    let archive = plan_archive_extraction(&listing, destination)?;

    let mut planned_index_by_entry = vec![None; entries.len()];
    let mut outpath_by_name = HashMap::new();
    for (planned_index, planned) in archive.entries.iter().enumerate() {
        let entry_index = plannable[planned.index];
        planned_index_by_entry[entry_index] = Some(planned_index);
        let key = archive_entry_parts(&entries[entry_index].listing.name)?.join("/");
        outpath_by_name.insert(key, &planned.outpath);
    }
    let mut copies = Vec::new();
    for (entry, planned_index) in entries.iter().zip(&planned_index_by_entry) {
        let (RarEntryKind::CopyOf(source_name), Some(planned_index)) = (&entry.kind, planned_index)
        else {
            continue;
        };
        let source = outpath_by_name
            .get(&archive_entry_parts(source_name)?.join("/"))
            .ok_or_else(|| anyhow!("archive refers to a missing file: {source_name}"))?;
        copies.push(RarCopy {
            source: source.to_path_buf(),
            target: archive.entries[*planned_index].outpath.clone(),
        });
    }
    Ok(RarPlan {
        archive,
        planned_index_by_entry,
        copies,
    })
}

fn rar_packed_size(archive: &Path) -> Result<u64> {
    let own_size = fs::metadata(archive)?.len();
    let Some(volumes) = unrar_ng::Archive::new(archive).all_parts_option() else {
        return Ok(own_size);
    };
    let (Some(folder), Some(pattern)) = (
        volumes.parent(),
        volumes.file_name().and_then(OsStr::to_str),
    ) else {
        return Ok(own_size);
    };
    let mut total = 0u64;
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| matches_rar_volume_pattern(name, pattern))
        {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total.max(own_size))
}

fn matches_rar_volume_pattern(name: &str, pattern: &str) -> bool {
    name.chars().count() == pattern.chars().count()
        && name
            .chars()
            .zip(pattern.chars())
            .all(|(name_char, pattern_char)| {
                pattern_char == '?' || name_char.eq_ignore_ascii_case(&pattern_char)
            })
}

fn listing_with_7z(exe_path: &Path, archive: &Path) -> Result<Vec<ArchiveListing>> {
    let output = Command::new(exe_path)
        .arg("l")
        .arg("-slt")
        .arg("-sccUTF-8")
        .arg(archive)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .with_context(|| format!("failed to run 7-Zip at {}", exe_path.display()))?;
    if !output.status.success() {
        bail!("7-Zip could not list the archive (status {})", output.status);
    }
    Ok(parse_7z_technical_listing(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn parse_7z_technical_listing(text: &str) -> Vec<ArchiveListing> {
    let mut listing: Vec<ArchiveListing> = Vec::new();
    let mut lines = text.lines().skip_while(|line| *line != "----------");
    lines.next();
    for line in lines {
        if let Some(path) = line.strip_prefix("Path = ") {
            listing.push(ArchiveListing {
                name: path.to_string(),
                is_directory: false,
                size: 0,
            });
        } else if let Some(entry) = listing.last_mut() {
            if line == "Folder = +" {
                entry.is_directory = true;
            } else if let Some(size) = line.strip_prefix("Size = ") {
                entry.size = size.trim().parse().unwrap_or(0);
            }
        }
    }
    listing
}

fn extract_rar_with_unrar(
    archive_path: &Path,
    entries: &[RarListedEntry],
    plan: &RarPlan,
    cancel: Option<&CancelFlag>,
) -> Result<()> {
    let mut created_dirs = HashSet::new();
    let mut archive = unrar_ng::Archive::new(archive_path)
        .open_for_processing()
        .context("failed to open .rar archive")?;
    let mut entry_index = 0;
    while let Some(header) = archive
        .read_header()
        .context("failed to read .rar header")?
    {
        if let Some(flag) = cancel {
            check_cancel(flag)?;
        }
        let Some(entry) = entries
            .get(entry_index)
            .filter(|entry| OsStr::new(&entry.listing.name) == header.entry().filename.as_os_str())
        else {
            bail!(".rar entries did not match their listing");
        };
        let planned = plan.planned_index_by_entry[entry_index].map(|index| &plan.archive.entries[index]);
        entry_index += 1;
        archive = match planned {
            Some(planned) if planned.is_directory => {
                ensure_directory(&planned.outpath, &mut created_dirs)?;
                header.skip().context("failed to read .rar entry")?
            }
            Some(planned) if matches!(entry.kind, RarEntryKind::Regular) => {
                if let Some(parent) = planned.outpath.parent() {
                    ensure_directory(parent, &mut created_dirs)?;
                }
                let next = header
                    .extract_to(&planned.outpath)
                    .context("failed to extract .rar entry")?;
                if fs::metadata(&planned.outpath)?.len() > planned.size {
                    bail!("archive entry is larger than its listed size");
                }
                next
            }
            _ => header.skip().context("failed to read .rar entry")?,
        };
    }
    if entry_index != entries.len() {
        bail!(".rar entries did not match their listing");
    }
    for copy in &plan.copies {
        if let Some(parent) = copy.target.parent() {
            ensure_directory(parent, &mut created_dirs)?;
        }
        fs::copy(&copy.source, &copy.target).with_context(|| {
            format!(
                "failed to copy {} inside the .rar archive",
                copy.source.display()
            )
        })?;
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

    let listing = listing_with_7z(&exe_path, archive)
        .map_err(|list_err| anyhow!("{unrar_error:#}\n7-Zip fallback failed: {list_err:#}"))?;
    let plan = plan_archive_extraction(&listing, destination)?;
    check_unpacked_size(rar_packed_size(archive)?, plan.unpacked_bytes, destination)?;

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
        err.raw_os_error() == Some(17)
    }
    #[cfg(not(windows))]
    {
        err.raw_os_error() == Some(18)
    }
}

#[cfg(test)]
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

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub fn install_candidate_with_state_cancelable(
    candidate_path: &Path,
    preferred_name: &str,
    target_root: &Path,
    disabled_target_root: Option<&Path>,
    choice: ConflictChoice,
    source_is_archive: bool,
    backend: GameBackend,
    status: ModStatus,
    cancel: &CancelFlag,
) -> Result<Option<PathBuf>> {
    install_candidate_with_state_verified_cancelable(
        candidate_path,
        preferred_name,
        target_root,
        disabled_target_root,
        choice,
        source_is_archive,
        backend,
        status,
        None,
        cancel,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn install_candidate_with_state_verified_cancelable(
    candidate_path: &Path,
    preferred_name: &str,
    target_root: &Path,
    disabled_target_root: Option<&Path>,
    choice: ConflictChoice,
    source_is_archive: bool,
    backend: GameBackend,
    status: ModStatus,
    expected: Option<&bundle::FolderManifest>,
    cancel: &CancelFlag,
) -> Result<Option<PathBuf>> {
    validate_install_folder_name(preferred_name)?;
    if !matches!(status, ModStatus::Active | ModStatus::Disabled) {
        bail!("install candidates must be active or disabled");
    }
    if choice == ConflictChoice::Cancel {
        return Ok(None);
    }

    let destination_root = match (backend, &status) {
        (GameBackend::UnrealEngine, ModStatus::Disabled) => disabled_target_root
            .ok_or_else(|| anyhow!("disabled Unreal mod path is not configured"))?,
        _ => target_root,
    };
    fs::create_dir_all(target_root)?;
    if let Some(root) = disabled_target_root {
        fs::create_dir_all(root)?;
    }
    let destination = destination_root.join(preferred_name);
    let alternate_root = match backend {
        GameBackend::UnrealEngine => {
            if destination_root == target_root {
                disabled_target_root
            } else {
                Some(target_root)
            }
        }
        GameBackend::Xxmi => None,
    };
    let alternate = alternate_root.map(|root| root.join(preferred_name));

    let stage_parent = if backend == GameBackend::UnrealEngine {
        disabled_target_root
            .and_then(Path::parent)
            .or_else(|| destination_root.parent())
    } else {
        destination_root.parent()
    }
    .ok_or_else(|| anyhow!("destination root has no parent"))?;
    fs::create_dir_all(stage_parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".hestia-install-")
        .tempdir_in(stage_parent)
        .context("failed to create install staging directory")?;
    let staged_candidate = stage.path().join("candidate");
    stage_candidate_payload(candidate_path, &staged_candidate, source_is_archive, cancel)?;
    if let Some(expected) = expected {
        bundle::verify_folder_manifest(&staged_candidate, expected, cancel)?;
    }

    let existing_destination = destination.is_dir().then_some(destination.as_path());
    let existing_alternate = alternate.as_deref().filter(|path| path.is_dir());
    let existing = existing_destination.or(existing_alternate);

    let actual_destination = match choice {
        ConflictChoice::Replace => {
            let staged_final = stage.path().join("final");
            prepare_candidate_stage(&staged_candidate, &staged_final, backend, &status, cancel)?;
            check_cancel(cancel)?;
            let stash = (backend == GameBackend::Xxmi)
                .then(|| existing.and_then(crate::integrations::xxmi_persist::read_stash_bytes))
                .flatten();
            let preserved_state = existing.and_then(read_portable_state);
            atomic_publish_staged_dir(
                &staged_final,
                &destination,
                existing,
                stash,
                preserved_state,
            )?;
            destination
        }
        ConflictChoice::Merge => {
            if let Some(existing) = existing_destination {
                if backend == GameBackend::Xxmi {
                    let staged_final = stage.path().join("final");
                    copy_dir_cancelable(existing, &staged_final, false, cancel)?;
                    normalize_existing_xxmi_stage(&staged_final, &status, cancel)?;
                    merge_candidate_stage(
                        &staged_candidate,
                        &staged_final,
                        backend,
                        &status,
                        cancel,
                    )?;
                    check_cancel(cancel)?;
                    let stash = crate::integrations::xxmi_persist::read_stash_bytes(existing);
                    let preserved_state = read_portable_state(existing);
                    atomic_publish_staged_dir(
                        &staged_final,
                        &destination,
                        Some(existing),
                        stash,
                        preserved_state,
                    )?;
                } else {
                    let staged_final = stage.path().join("final");
                    copy_dir_cancelable(existing, &staged_final, false, cancel)?;
                    merge_candidate_stage(
                        &staged_candidate,
                        &staged_final,
                        backend,
                        &status,
                        cancel,
                    )?;
                    check_cancel(cancel)?;
                    let preserved_state = read_portable_state(existing);
                    atomic_publish_staged_dir(
                        &staged_final,
                        &destination,
                        Some(existing),
                        None,
                        preserved_state,
                    )?;
                }
                destination
            } else if let Some(existing) = existing_alternate {
                let staged_final = stage.path().join("final");
                copy_dir_cancelable(existing, &staged_final, false, cancel)?;
                merge_candidate_stage(&staged_candidate, &staged_final, backend, &status, cancel)?;
                check_cancel(cancel)?;
                let stash = (backend == GameBackend::Xxmi)
                    .then(|| crate::integrations::xxmi_persist::read_stash_bytes(existing))
                    .flatten();
                let preserved_state = read_portable_state(existing);
                atomic_publish_staged_dir(
                    &staged_final,
                    &destination,
                    Some(existing),
                    stash,
                    preserved_state,
                )?;
                destination
            } else {
                let staged_final = stage.path().join("final");
                prepare_candidate_stage(
                    &staged_candidate,
                    &staged_final,
                    backend,
                    &status,
                    cancel,
                )?;
                check_cancel(cancel)?;
                fs::rename(&staged_final, &destination)?;
                destination
            }
        }
        ConflictChoice::KeepBoth => {
            let destination =
                next_available_name_across_roots(destination_root, alternate_root, preferred_name);
            let staged_final = stage.path().join("final");
            prepare_candidate_stage(&staged_candidate, &staged_final, backend, &status, cancel)?;
            check_cancel(cancel)?;
            let portable_state = staged_final
                .join(MOD_META_DIR)
                .join(crate::model::MOD_META_FILE);
            if portable_state.exists() {
                match crate::persistence::load_portable_mod_state(&staged_final) {
                    Ok(Some(mut state)) => {
                        state.id = Uuid::new_v4().to_string();
                        crate::persistence::save_portable_mod_state(&staged_final, &state)?;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::warn!(
                            "could not read cloned portable mod state at {}: {err}",
                            portable_state.display()
                        );
                        fs::remove_file(portable_state)?;
                    }
                }
            }
            check_cancel(cancel)?;
            fs::rename(&staged_final, &destination)?;
            destination
        }
        ConflictChoice::Cancel => unreachable!(),
    };

    Ok(Some(actual_destination))
}

fn stage_candidate_payload(
    source: &Path,
    destination: &Path,
    source_is_archive: bool,
    cancel: &CancelFlag,
) -> Result<()> {
    if source_is_archive {
        move_or_copy_archive_candidate_cancelable(source, destination, cancel)
    } else {
        copy_dir_cancelable(source, destination, false, cancel)
    }
}

fn next_available_name_across_roots(
    primary: &Path,
    alternate: Option<&Path>,
    base_name: &str,
) -> PathBuf {
    let occupied = |name: &str| {
        primary.join(name).exists() || alternate.is_some_and(|root| root.join(name).exists())
    };
    if !occupied(base_name) {
        return primary.join(base_name);
    }
    let mut counter = 2;
    loop {
        let name = format!("{base_name} ({counter})");
        if !occupied(&name) {
            return primary.join(name);
        }
        counter += 1;
    }
}

fn copy_entry_cancelable(source: &Path, destination: &Path, cancel: &CancelFlag) -> Result<()> {
    check_cancel(cancel)?;
    if source.is_dir() {
        copy_dir_cancelable(source, destination, false, cancel)
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_file_with_parent_recovery(source, destination)?;
        Ok(())
    }
}

fn prepare_candidate_stage(
    source: &Path,
    destination: &Path,
    backend: GameBackend,
    status: &ModStatus,
    cancel: &CancelFlag,
) -> Result<()> {
    check_cancel(cancel)?;
    fs::rename(source, destination)?;
    if backend == GameBackend::Xxmi {
        normalize_existing_xxmi_stage(destination, status, cancel)?;
        crate::integrations::xxmi::mark_mod_root(destination)?;
    } else {
        crate::integrations::unrealengine::mark_mod_root(destination)?;
    }
    check_cancel(cancel)?;
    Ok(())
}

fn copy_candidate_into_xxmi_disabled_container(
    source: &Path,
    destination_root: &Path,
    disabled_root: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
    fs::create_dir_all(destination_root)?;
    fs::create_dir_all(disabled_root)?;
    for entry in fs::read_dir(source)? {
        check_cancel(cancel)?;
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == OsStr::new(DISABLED_CONTAINER) && path.is_dir() {
            copy_dir_cancelable(&path, disabled_root, false, cancel)?;
        } else if name == OsStr::new(MOD_META_DIR) {
            copy_entry_cancelable(&path, &destination_root.join(&name), cancel)?;
        } else {
            copy_entry_cancelable(&path, &disabled_root.join(&name), cancel)?;
        }
    }
    Ok(())
}

fn copy_candidate_into_xxmi_active_root(
    source: &Path,
    destination_root: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
    fs::create_dir_all(destination_root)?;
    for entry in fs::read_dir(source)? {
        check_cancel(cancel)?;
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == OsStr::new(DISABLED_CONTAINER) && path.is_dir() {
            copy_dir_cancelable(&path, destination_root, false, cancel)?;
        } else {
            copy_entry_cancelable(&path, &destination_root.join(&name), cancel)?;
        }
    }
    if destination_root.join(DISABLED_CONTAINER).exists() {
        normalize_existing_xxmi_stage(destination_root, &ModStatus::Active, cancel)?;
    }
    Ok(())
}

fn normalize_existing_xxmi_stage(
    root: &Path,
    desired_status: &ModStatus,
    cancel: &CancelFlag,
) -> Result<()> {
    if *desired_status == ModStatus::Disabled {
        let disabled_root = root.join(DISABLED_CONTAINER);
        fs::create_dir_all(&disabled_root)?;
        let entries = fs::read_dir(root)?.collect::<std::result::Result<Vec<_>, _>>()?;
        for entry in entries {
            check_cancel(cancel)?;
            let path = entry.path();
            let name = entry.file_name();
            if name == OsStr::new(DISABLED_CONTAINER) || name == OsStr::new(MOD_META_DIR) {
                continue;
            }
            move_entry_into(&path, &disabled_root.join(name), cancel)?;
        }
    } else {
        let disabled_root = root.join(DISABLED_CONTAINER);
        if disabled_root.is_dir() {
            let entries = fs::read_dir(&disabled_root)
                .and_then(|entries| entries.collect::<std::result::Result<Vec<_>, _>>())?;
            for entry in entries {
                check_cancel(cancel)?;
                let path = entry.path();
                let name = entry.file_name();
                move_entry_into(&path, &root.join(name), cancel)?;
            }
            fs::remove_dir_all(disabled_root)?;
        }
    }
    Ok(())
}

fn move_entry_into(source: &Path, destination: &Path, cancel: &CancelFlag) -> Result<()> {
    check_cancel(cancel)?;
    if destination.exists() {
        if source.is_dir() && destination.is_dir() {
            copy_dir_cancelable(source, destination, true, cancel)?;
            fs::remove_dir_all(source)?;
        } else {
            fs::remove_dir_all(source).or_else(|_| fs::remove_file(source))?;
        }
    } else {
        fs::rename(source, destination)?;
    }
    Ok(())
}

fn merge_candidate_stage(
    source: &Path,
    destination: &Path,
    backend: GameBackend,
    status: &ModStatus,
    cancel: &CancelFlag,
) -> Result<()> {
    if backend == GameBackend::Xxmi && *status == ModStatus::Disabled {
        let disabled_root = destination.join(DISABLED_CONTAINER);
        fs::create_dir_all(&disabled_root)?;
        copy_candidate_into_xxmi_disabled_container(source, destination, &disabled_root, cancel)?;
    } else if backend == GameBackend::Xxmi && *status == ModStatus::Active {
        normalize_existing_xxmi_stage(destination, &ModStatus::Active, cancel)?;
        copy_candidate_into_xxmi_active_root(source, destination, cancel)?;
    } else {
        copy_dir_cancelable(source, destination, false, cancel)?;
    }
    if backend == GameBackend::Xxmi {
        crate::integrations::xxmi::mark_mod_root(destination)?;
    } else {
        crate::integrations::unrealengine::mark_mod_root(destination)?;
    }
    Ok(())
}

fn atomic_publish_staged_dir(
    staged: &Path,
    destination: &Path,
    existing: Option<&Path>,
    preserved_stash: Option<Vec<u8>>,
    preserved_state: Option<crate::model::PortableModState>,
) -> Result<()> {
    crate::integrations::xxmi_persist::restore_stash_bytes(staged, &preserved_stash);
    if let Some(state) = &preserved_state {
        crate::persistence::save_portable_mod_state(staged, state).with_context(|| {
            format!(
                "could not preserve portable mod state at {}",
                staged.display()
            )
        })?;
    }
    let staging_parent = staged
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("staging directory has no external parent"))?;
    let retired = existing.map(|_| {
        let stage_name = staged
            .parent()
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .unwrap_or("install");
        staging_parent.join(format!(".hestia-retired-{stage_name}"))
    });
    if let Some(existing) = existing {
        let retired = retired.as_ref().unwrap();
        if retired.exists() {
            let _ = fs::remove_dir_all(retired);
        }
        fs::rename(existing, retired).with_context(|| {
            format!("could not move existing mod aside: {}", existing.display())
        })?;
        if let Err(err) = fs::rename(staged, destination) {
            if let Err(rollback_err) = fs::rename(retired, existing) {
                tracing::error!(
                    "could not roll back replaced mod {} after publish failure: {rollback_err}",
                    existing.display()
                );
            }
            return Err(err.into());
        }
        dispose_replaced_install_path(retired);
    } else {
        fs::rename(staged, destination)?;
    }
    Ok(())
}

fn read_portable_state(root: &Path) -> Option<crate::model::PortableModState> {
    match crate::persistence::load_portable_mod_state(root) {
        Ok(state) => state,
        Err(err) => {
            tracing::warn!(
                "could not read portable mod state from {}: {err}",
                root.display()
            );
            None
        }
    }
}

fn dispose_replaced_install_path(retired: &Path) {
    if crate::integrations::xxmi::recycle_path(retired).is_ok() {
        return;
    }
    if let Err(err) = fs::remove_dir_all(retired) {
        tracing::warn!(
            "replaced mod folder left behind at {}: {err}",
            retired.display()
        );
    }
}

const BUNDLED_PREVIEW_MAX_DEPTH: usize = 4;
const BUNDLED_PREVIEW_MAX_IMAGES: usize = 10;
const BUNDLED_PREVIEW_MAX_INI_BYTES: usize = 1024 * 1024;

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
        (a.0, a.1, a.2.to_string_lossy().to_ascii_lowercase()).cmp(&(
            b.0,
            b.1,
            b.2.to_string_lossy().to_ascii_lowercase(),
        ))
    });
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
        assert_eq!(
            strip_uuid_prefix("8b7f2df561-b7-409f-93bf-7948fd407fd4_Name"),
            "8b7f2df561-b7-409f-93bf-7948fd407fd4_Name"
        );
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
        assert_eq!(
            names,
            vec![".JASM_Cover.jpg", "preview.webp", "2.png", "10.png"]
        );
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
        assert!(ini_blob_references_file(
            "filename=.\\sub\\1.png\r\n",
            "1.png"
        ));
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

    fn write_test_zip(path: &Path, names: &[&str]) {
        let file = fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for name in names {
            writer.start_file(*name, options).unwrap();
            writer.write_all(b"demo").unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn archive_entry_parts_refuse_names_that_climb_out() {
        for name in [
            "../evil.txt",
            "Mod/../../evil.txt",
            "Mod\\..\\..\\evil.txt",
            "Mod/..",
            "/",
            ".",
            "Mod/a\0b.txt",
        ] {
            assert!(archive_entry_parts(name).is_err(), "{name:?} should be refused");
        }
        assert_eq!(
            archive_entry_parts("/Mod/./Sub/../readme.txt").unwrap(),
            ["Mod", "readme.txt"]
        );
        assert_eq!(
            archive_entry_parts("Mod\\Sub\\readme.txt").unwrap(),
            ["Mod", "Sub", "readme.txt"]
        );
    }

    #[test]
    fn zip_extract_refuses_entries_that_climb_out() {
        for name in ["../evil.txt", "Mod/../../evil.txt", "Mod\\..\\..\\evil.txt"] {
            let temp = tempfile::tempdir().unwrap();
            let archive_path = temp.path().join("climb.zip");
            write_test_zip(&archive_path, &["Mod/readme.txt", name]);

            let destination = temp.path().join("extract");
            let err = extract_zip(&archive_path, &destination).unwrap_err();
            assert!(
                err.to_string().contains("invalid path"),
                "unexpected error for {name:?}: {err:#}"
            );
            assert!(!temp.path().join("evil.txt").exists());
        }
    }

    #[test]
    fn zip_extract_keeps_drive_and_rooted_names_inside_the_destination() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("drive.zip");
        write_test_zip(&archive_path, &["C:/Mod/readme.txt", "/C:/Mod/other.txt"]);

        let destination = temp.path().join("extract");
        extract_zip(&archive_path, &destination).unwrap();
        assert!(destination.join("C_").join("Mod").join("readme.txt").exists());
        assert!(destination.join("C_").join("Mod").join("other.txt").exists());

        let mixed_path = temp.path().join("mixed-drive.zip");
        write_test_zip(&mixed_path, &["readme.txt", "C:/evil.txt"]);
        let mixed_destination = temp.path().join("mixed");
        let err = extract_zip(&mixed_path, &mixed_destination).unwrap_err();
        assert!(
            err.to_string().contains("invalid file name"),
            "unexpected error: {err:#}"
        );
    }

    const TEST_MIB: u64 = 1024 * 1024;

    #[test]
    fn unpack_limits_allow_anything_under_two_gib() {
        check_unpack_limits(TEST_MIB, 1500 * TEST_MIB, None).unwrap();
        check_unpack_limits(1, UNPACK_ALWAYS_ALLOWED_BYTES, None).unwrap();
        let err = check_unpack_limits(TEST_MIB, UNPACK_ALWAYS_ALLOWED_BYTES + 1, None).unwrap_err();
        assert!(
            err.to_string().contains("times its size"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn unpack_limits_allow_up_to_a_hundred_times_the_archive() {
        let packed = 50 * TEST_MIB;
        check_unpack_limits(packed, packed * UNPACK_RATIO_LIMIT, None).unwrap();
        assert!(check_unpack_limits(packed, packed * UNPACK_RATIO_LIMIT + 1, None).is_err());
        assert!(check_unpack_limits(u64::MAX, u64::MAX, None).is_ok());
    }

    #[test]
    fn unpack_limits_only_refuse_installs_that_do_not_fit() {
        let unpacked = 10 * TEST_MIB;
        check_unpack_limits(unpacked, unpacked, Some(unpacked)).unwrap();
        let err = check_unpack_limits(unpacked, unpacked, Some(unpacked - 1)).unwrap_err();
        assert!(
            err.to_string().contains("not enough free disk space"),
            "unexpected error: {err:#}"
        );
        assert!(check_unpack_limits(unpacked, unpacked, Some(0)).is_err());
        check_unpack_limits(unpacked, unpacked, None).unwrap();
    }

    #[test]
    fn archive_plan_counts_only_extracted_files() {
        let listing = [
            ("Mod/", true, 0),
            ("Mod/a.txt", false, 10),
            ("Mod/sub/b.txt", false, 20),
            ("__MACOSX/._a.txt", false, 5),
        ]
        .map(|(name, is_directory, size)| ArchiveListing {
            name: name.to_string(),
            is_directory,
            size,
        });
        let destination = Path::new("extract");
        let plan = plan_archive_extraction(&listing, destination).unwrap();
        assert_eq!(plan.unpacked_bytes, 30);
        assert_eq!(plan.entries.len(), 3);
        assert_eq!(
            plan.entries[2].outpath,
            destination.join("Mod").join("sub").join("b.txt")
        );
    }

    #[test]
    fn copy_archive_entry_refuses_data_past_the_listed_size() {
        let temp = tempfile::tempdir().unwrap();
        let planned = PlannedArchiveEntry {
            index: 0,
            outpath: temp.path().join("out.bin"),
            is_directory: false,
            size: 4,
        };
        let err = copy_archive_entry(&mut &b"0123456789"[..], &planned, None).unwrap_err();
        assert!(
            err.to_string().contains("larger than its listed size"),
            "unexpected error: {err:#}"
        );

        copy_archive_entry(&mut &b"0123"[..], &planned, None).unwrap();
        assert_eq!(fs::read(&planned.outpath).unwrap(), b"0123");
    }

    fn write_test_7z(path: &Path, entries: &[(&str, Option<&[u8]>)]) {
        let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
        for (name, data) in entries {
            match data {
                Some(data) => {
                    writer
                        .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*data))
                        .unwrap();
                }
                None => {
                    writer
                        .push_archive_entry(
                            sevenz_rust2::ArchiveEntry::new_directory(name),
                            None::<&[u8]>,
                        )
                        .unwrap();
                }
            }
        }
        writer.finish().unwrap();
    }

    #[test]
    fn seven_zip_extracts_nested_files_folders_and_empty_files() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("demo.7z");
        write_test_7z(
            &archive_path,
            &[
                ("Mod", None),
                ("Mod/readme.txt", Some(b"hello")),
                ("Mod/sub/data.ini", Some(b"[Constants]")),
                ("Mod/empty.txt", Some(b"")),
                ("Mod/textures", None),
            ],
        );

        let destination = temp.path().join("extract");
        extract_archive(&archive_path, &destination).unwrap();
        let root = destination.join("Mod");
        assert_eq!(fs::read(root.join("readme.txt")).unwrap(), b"hello");
        assert_eq!(
            fs::read(root.join("sub").join("data.ini")).unwrap(),
            b"[Constants]"
        );
        assert_eq!(fs::read(root.join("empty.txt")).unwrap(), b"");
        assert!(root.join("textures").is_dir());
    }

    #[test]
    fn seven_zip_extracts_every_supported_compression_method() {
        let methods = [
            sevenz_rust2::EncoderMethod::LZMA2,
            sevenz_rust2::EncoderMethod::LZMA,
            sevenz_rust2::EncoderMethod::BZIP2,
            sevenz_rust2::EncoderMethod::DEFLATE,
            sevenz_rust2::EncoderMethod::PPMD,
            sevenz_rust2::EncoderMethod::ZSTD,
            sevenz_rust2::EncoderMethod::BROTLI,
            sevenz_rust2::EncoderMethod::LZ4,
        ];
        let payload = b"[TextureOverrideBody]\nhash = 0123abcd\n".repeat(64);
        for method in methods {
            let temp = tempfile::tempdir().unwrap();
            let archive_path = temp.path().join("method.7z");
            {
                let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
                writer.set_content_methods(vec![method.into()]);
                writer
                    .push_archive_entry(
                        sevenz_rust2::ArchiveEntry::new_file("Mod/mod.ini"),
                        Some(&payload[..]),
                    )
                    .unwrap();
                writer.finish().unwrap();
            }

            let destination = temp.path().join("extract");
            extract_archive(&archive_path, &destination)
                .unwrap_or_else(|err| panic!("{} failed: {err:#}", method.name()));
            assert_eq!(
                fs::read(destination.join("Mod").join("mod.ini")).unwrap(),
                payload,
                "{} unpacked the wrong bytes",
                method.name()
            );
        }
    }

    #[test]
    fn seven_zip_extract_keeps_the_stored_modified_time() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("times.7z");
        let unix_seconds = 1_577_836_800u64;
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("Mod/readme.txt");
        entry.has_last_modified_date = true;
        entry.last_modified_date =
            sevenz_rust2::NtTime::new((unix_seconds + 11_644_473_600) * 10_000_000);
        {
            let mut writer = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
            writer.push_archive_entry(entry, Some(&b"demo"[..])).unwrap();
            writer.finish().unwrap();
        }

        let destination = temp.path().join("extract");
        extract_archive(&archive_path, &destination).unwrap();
        let modified = fs::metadata(destination.join("Mod").join("readme.txt"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            modified,
            SystemTime::UNIX_EPOCH + Duration::from_secs(unix_seconds)
        );
        assert!(nt_time_to_system_time(sevenz_rust2::NtTime::new(u64::MAX)).is_none());
    }

    #[test]
    fn seven_zip_extract_sanitizes_top_level_candidate_folder_name() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("bad-top.7z");
        write_test_7z(&archive_path, &[("Bad:Name/readme.txt", Some(b"demo"))]);

        let destination = temp.path().join("extract");
        extract_archive(&archive_path, &destination).unwrap();
        assert!(destination.join("Bad_Name").join("readme.txt").exists());
    }

    #[test]
    fn seven_zip_extract_refuses_entries_that_climb_out() {
        for name in ["../evil.txt", "Mod/../../evil.txt", "Mod\\..\\..\\evil.txt"] {
            let temp = tempfile::tempdir().unwrap();
            let archive_path = temp.path().join("climb.7z");
            write_test_7z(
                &archive_path,
                &[("Mod/readme.txt", Some(b"demo")), (name, Some(b"evil"))],
            );

            let destination = temp.path().join("extract");
            let err = extract_archive(&archive_path, &destination).unwrap_err();
            assert!(
                err.to_string().contains("invalid path"),
                "unexpected error for {name:?}: {err:#}"
            );
            assert!(!temp.path().join("evil.txt").exists());
            assert!(!destination.exists());
        }
    }

    #[test]
    fn seven_zip_extract_rejects_windows_invalid_payload_names() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("invalid-payload.7z");
        write_test_7z(
            &archive_path,
            &[("GoodMod/Bad:Name/readme.txt", Some(b"demo"))],
        );

        let destination = temp.path().join("extract");
        let err = extract_archive(&archive_path, &destination).unwrap_err();
        assert!(
            err.to_string().contains("invalid file name"),
            "unexpected error: {err:#}"
        );
        assert!(!destination.exists());
    }

    enum TestRarEntry<'a> {
        File(&'a str, &'a [u8]),
        Directory(&'a str),
        Redirect {
            name: &'a str,
            redirect_type: u64,
            target: &'a str,
            unpacked_size: u64,
        },
    }

    fn test_crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn push_rar_vint(out: &mut Vec<u8>, mut value: u64) {
        loop {
            let low_bits = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(low_bits);
                return;
            }
            out.push(low_bits | 0x80);
        }
    }

    fn rar_block(body: &[u8]) -> Vec<u8> {
        let mut checked = Vec::new();
        push_rar_vint(&mut checked, body.len() as u64);
        checked.extend_from_slice(body);
        let mut block = test_crc32(&checked).to_le_bytes().to_vec();
        block.extend(checked);
        block
    }

    fn write_test_rar(path: &Path, entries: &[TestRarEntry]) {
        let mut out = b"Rar!\x1a\x07\x01\x00".to_vec();
        out.extend(rar_block(&[1, 0, 0]));
        for entry in entries {
            let (name, data, is_directory, unpacked_size, redirect) = match *entry {
                TestRarEntry::File(name, data) => (name, data, false, data.len() as u64, None),
                TestRarEntry::Directory(name) => (name, &[][..], true, 0, None),
                TestRarEntry::Redirect {
                    name,
                    redirect_type,
                    target,
                    unpacked_size,
                } => (
                    name,
                    &[][..],
                    false,
                    unpacked_size,
                    Some((redirect_type, target)),
                ),
            };
            let mut extra = Vec::new();
            if let Some((redirect_type, target)) = redirect {
                let mut record = Vec::new();
                push_rar_vint(&mut record, 5);
                push_rar_vint(&mut record, redirect_type);
                push_rar_vint(&mut record, 0);
                push_rar_vint(&mut record, target.len() as u64);
                record.extend_from_slice(target.as_bytes());
                push_rar_vint(&mut extra, record.len() as u64);
                extra.extend(record);
            }
            let mut body = Vec::new();
            push_rar_vint(&mut body, 2);
            let mut header_flags = 0;
            if !extra.is_empty() {
                header_flags |= 1;
            }
            if !data.is_empty() {
                header_flags |= 2;
            }
            push_rar_vint(&mut body, header_flags);
            if !extra.is_empty() {
                push_rar_vint(&mut body, extra.len() as u64);
            }
            if !data.is_empty() {
                push_rar_vint(&mut body, data.len() as u64);
            }
            let has_crc = redirect.is_none() && !is_directory;
            push_rar_vint(
                &mut body,
                if is_directory {
                    1
                } else if has_crc {
                    4
                } else {
                    0
                },
            );
            push_rar_vint(&mut body, unpacked_size);
            push_rar_vint(&mut body, if is_directory { 0x10 } else { 0x20 });
            if has_crc {
                body.extend(test_crc32(data).to_le_bytes());
            }
            push_rar_vint(&mut body, 0);
            push_rar_vint(&mut body, 0);
            push_rar_vint(&mut body, name.len() as u64);
            body.extend_from_slice(name.as_bytes());
            body.extend(extra);
            out.extend(rar_block(&body));
            out.extend_from_slice(data);
        }
        out.extend(rar_block(&[5, 0, 0]));
        fs::write(path, out).unwrap();
    }

    #[test]
    fn rar_extracts_nested_files_and_folders() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("demo.rar");
        write_test_rar(
            &archive_path,
            &[
                TestRarEntry::Directory("Mod"),
                TestRarEntry::File("Mod/readme.txt", b"hello"),
                TestRarEntry::File("Mod/sub/data.ini", b"[Constants]"),
                TestRarEntry::Directory("Mod/textures"),
            ],
        );

        let destination = temp.path().join("extract");
        extract_archive(&archive_path, &destination).unwrap();
        let root = destination.join("Mod");
        assert_eq!(fs::read(root.join("readme.txt")).unwrap(), b"hello");
        assert_eq!(
            fs::read(root.join("sub").join("data.ini")).unwrap(),
            b"[Constants]"
        );
        assert!(root.join("textures").is_dir());
    }

    #[test]
    fn rar_extract_copies_file_references_and_never_creates_links() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("links.rar");
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, b"original").unwrap();
        write_test_rar(
            &archive_path,
            &[
                TestRarEntry::File("Mod/readme.txt", b"hello"),
                TestRarEntry::Redirect {
                    name: "Mod/copy.txt",
                    redirect_type: 5,
                    target: "Mod/readme.txt",
                    unpacked_size: 5,
                },
                TestRarEntry::Redirect {
                    name: "Mod/hard.txt",
                    redirect_type: 4,
                    target: "Mod/readme.txt",
                    unpacked_size: 5,
                },
                TestRarEntry::Redirect {
                    name: "Mod/unix-link",
                    redirect_type: 1,
                    target: "../../outside.txt",
                    unpacked_size: 0,
                },
                TestRarEntry::Redirect {
                    name: "Mod/windows-link",
                    redirect_type: 2,
                    target: "..\\..\\outside.txt",
                    unpacked_size: 0,
                },
                TestRarEntry::Redirect {
                    name: "Mod/junction",
                    redirect_type: 3,
                    target: "C:\\Windows",
                    unpacked_size: 0,
                },
            ],
        );

        let destination = temp.path().join("extract");
        extract_archive(&archive_path, &destination).unwrap();
        let root = destination.join("Mod");
        assert_eq!(fs::read(root.join("copy.txt")).unwrap(), b"hello");
        assert_eq!(fs::read(root.join("hard.txt")).unwrap(), b"hello");
        for link in ["unix-link", "windows-link", "junction"] {
            assert!(
                fs::symlink_metadata(root.join(link)).is_err(),
                "{link} should not exist"
            );
        }
        assert_eq!(fs::read(&outside).unwrap(), b"original");
    }

    #[test]
    fn rar_extract_refuses_a_reference_to_a_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("missing.rar");
        write_test_rar(
            &archive_path,
            &[
                TestRarEntry::File("Mod/readme.txt", b"hello"),
                TestRarEntry::Redirect {
                    name: "Mod/copy.txt",
                    redirect_type: 5,
                    target: "../../secret.txt",
                    unpacked_size: 5,
                },
            ],
        );

        let destination = temp.path().join("extract");
        assert!(extract_archive(&archive_path, &destination).is_err());
        assert!(!destination.exists());
    }

    #[test]
    fn rar_extract_sanitizes_top_level_candidate_folder_name() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("bad-top.rar");
        write_test_rar(
            &archive_path,
            &[TestRarEntry::File("Bad:Name/readme.txt", b"demo")],
        );

        let destination = temp.path().join("extract");
        extract_archive(&archive_path, &destination).unwrap();
        assert!(destination.join("Bad_Name").join("readme.txt").exists());
    }

    #[test]
    fn rar_extract_refuses_entries_that_climb_out() {
        for name in ["../evil.txt", "Mod/../../evil.txt"] {
            let temp = tempfile::tempdir().unwrap();
            let archive_path = temp.path().join("climb.rar");
            write_test_rar(
                &archive_path,
                &[
                    TestRarEntry::File("Mod/readme.txt", b"demo"),
                    TestRarEntry::File(name, b"evil"),
                ],
            );

            let destination = temp.path().join("extract");
            let err = extract_archive(&archive_path, &destination).unwrap_err();
            assert!(
                err.to_string().contains("invalid path"),
                "unexpected error for {name:?}: {err:#}"
            );
            assert!(!temp.path().join("evil.txt").exists());
            assert!(!destination.exists());
        }
    }

    #[test]
    fn rar_extract_renames_windows_invalid_payload_names() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("invalid-payload.rar");
        write_test_rar(
            &archive_path,
            &[TestRarEntry::File("GoodMod/Bad:Name/readme.txt", b"demo")],
        );

        let destination = temp.path().join("extract");
        extract_archive(&archive_path, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("GoodMod").join("Bad_Name").join("readme.txt")).unwrap(),
            b"demo"
        );
        assert_eq!(fs::read_dir(destination.join("GoodMod")).unwrap().count(), 1);
    }

    #[test]
    fn seven_zip_technical_listing_reads_names_folders_and_sizes() {
        let text = "7-Zip 25.01\r\n\r\nListing archive: demo.rar\r\n\r\n--\r\nPath = demo.rar\r\nType = Rar5\r\nPhysical Size = 900\r\n\r\n----------\r\nPath = Mod\\readme.txt\r\nFolder = -\r\nSize = 12\r\nPacked Size = 10\r\n\r\nPath = Mod\r\nFolder = +\r\nSize = 0\r\n\r\nPath = ..\\evil.txt\r\nFolder = -\r\nSize = 7\r\n";
        let listing = parse_7z_technical_listing(text);
        let summary: Vec<(&str, bool, u64)> = listing
            .iter()
            .map(|entry| (entry.name.as_str(), entry.is_directory, entry.size))
            .collect();
        assert_eq!(
            summary,
            [
                ("Mod\\readme.txt", false, 12),
                ("Mod", true, 0),
                ("..\\evil.txt", false, 7)
            ]
        );
        assert!(plan_archive_extraction(&listing, Path::new("extract")).is_err());
    }

    #[test]
    fn rar_volume_pattern_matches_question_marks_and_ignores_case() {
        assert!(matches_rar_volume_pattern("Mod.part01.rar", "Mod.part??.rar"));
        assert!(matches_rar_volume_pattern("mod.PART12.RAR", "Mod.part??.rar"));
        assert!(!matches_rar_volume_pattern("Mod.part1.rar", "Mod.part??.rar"));
        assert!(!matches_rar_volume_pattern("Other.part01.rar", "Mod.part??.rar"));
        assert!(!matches_rar_volume_pattern("Mod.part001.rar", "Mod.part??.rar"));
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

    #[test]
    fn disabled_xxmi_publish_wraps_normal_and_already_disabled_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let target_root = temp.path().join("mods");
        let normal = temp.path().join("normal");
        let already_disabled = temp.path().join("already-disabled");
        fs::create_dir_all(&normal).unwrap();
        fs::write(normal.join("mod.ini"), "normal").unwrap();
        fs::create_dir_all(already_disabled.join(DISABLED_CONTAINER)).unwrap();
        fs::write(
            already_disabled.join(DISABLED_CONTAINER).join("mod.ini"),
            "disabled",
        )
        .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        let first = install_candidate_with_state_cancelable(
            &normal,
            "Disabled Mod",
            &target_root,
            None,
            ConflictChoice::Replace,
            false,
            GameBackend::Xxmi,
            ModStatus::Disabled,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert!(!first.join("mod.ini").exists());
        assert_eq!(
            fs::read_to_string(first.join(DISABLED_CONTAINER).join("mod.ini")).unwrap(),
            "normal"
        );

        let second = install_candidate_with_state_cancelable(
            &already_disabled,
            "Already Disabled",
            &target_root,
            None,
            ConflictChoice::Replace,
            true,
            GameBackend::Xxmi,
            ModStatus::Disabled,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert!(
            !second
                .join(DISABLED_CONTAINER)
                .join(DISABLED_CONTAINER)
                .exists()
        );
        assert_eq!(
            fs::read_to_string(second.join(DISABLED_CONTAINER).join("mod.ini")).unwrap(),
            "disabled"
        );

        let merge_source = temp.path().join("merge-source");
        fs::create_dir_all(&merge_source).unwrap();
        fs::write(merge_source.join("extra.ini"), "merged").unwrap();
        let merged = install_candidate_with_state_cancelable(
            &merge_source,
            "Disabled Mod",
            &target_root,
            None,
            ConflictChoice::Merge,
            false,
            GameBackend::Xxmi,
            ModStatus::Disabled,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            fs::read_to_string(merged.join(DISABLED_CONTAINER).join("extra.ini")).unwrap(),
            "merged"
        );
        assert!(!merged.join("extra.ini").exists());
    }

    #[test]
    fn xxmi_publish_scans_nested_wrapper_for_replace_merge_and_keep_both() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("nested-wrapper");
        fs::create_dir_all(source.join("Part A")).unwrap();
        fs::create_dir_all(source.join("Part B")).unwrap();
        fs::write(source.join("Part A").join("part-a.ini"), "a").unwrap();
        fs::write(source.join("Part B").join("part-b.ini"), "b").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let marker = |root: &Path| root.join(MOD_META_DIR).join("mod-root");
        let test_game = |mods_path: &Path| crate::model::GameInstall {
            definition: crate::model::GameDefinition {
                id: "nested-wrapper-test".to_string(),
                name: "Nested Wrapper Test".to_string(),
                backend: GameBackend::Xxmi,
                xxmi_code: "nested-wrapper-test".to_string(),
            },
            mods_path_override: Some(mods_path.to_path_buf()),
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: false,
            enabled: true,
        };
        let scan = |mods_path: &Path| {
            crate::integrations::xxmi::scan_live_mods(&test_game(mods_path), false, false).unwrap()
        };
        let assert_single_wrapper = |mods_path: &Path, expected_root: &Path| {
            let scanned = scan(mods_path);
            assert_eq!(scanned.len(), 1);
            assert_eq!(scanned[0].root_path.as_path(), expected_root);
            assert_eq!(scanned[0].folder_name, "Nested Wrapper");
            assert!(
                scanned[0].root_path.join("Part A").is_dir()
                    || scanned[0]
                        .root_path
                        .join(DISABLED_CONTAINER)
                        .join("Part A")
                        .is_dir()
            );
            assert!(
                scanned[0].root_path.join("Part B").is_dir()
                    || scanned[0]
                        .root_path
                        .join(DISABLED_CONTAINER)
                        .join("Part B")
                        .is_dir()
            );
            scanned
        };

        let replace_root = temp.path().join("replace-mods");
        let replaced = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &replace_root,
            None,
            ConflictChoice::Replace,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert!(marker(&replaced).exists());
        let mut initial_scan = assert_single_wrapper(&replace_root, &replaced);
        let mut entry = initial_scan.remove(0);
        let initial_id = entry.id.clone();
        crate::integrations::xxmi::disable_mod(&mut entry).unwrap();
        let disabled_scan = assert_single_wrapper(&replace_root, &replaced);
        assert_eq!(disabled_scan[0].id, initial_id);
        assert_eq!(disabled_scan[0].status, ModStatus::Disabled);
        crate::integrations::xxmi::enable_mod(&mut entry).unwrap();
        let enabled_scan = assert_single_wrapper(&replace_root, &replaced);
        assert_eq!(enabled_scan[0].id, initial_id);
        assert_eq!(enabled_scan[0].status, ModStatus::Active);

        let merge_root = temp.path().join("merge-mods");
        let merge_existing = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &merge_root,
            None,
            ConflictChoice::Replace,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        fs::remove_file(marker(&merge_existing)).unwrap();
        let merged = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &merge_root,
            None,
            ConflictChoice::Merge,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert!(marker(&merged).exists());
        assert_single_wrapper(&merge_root, &merged);

        let keep_both_root = temp.path().join("keep-both-mods");
        let keep_both_existing = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &keep_both_root,
            None,
            ConflictChoice::Replace,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        let kept_both = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &keep_both_root,
            None,
            ConflictChoice::KeepBoth,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert!(marker(&kept_both).exists());
        let kept_both_scan = scan(&keep_both_root);
        assert_eq!(kept_both_scan.len(), 2);
        assert!(kept_both_scan.iter().all(|entry| {
            entry.root_path == keep_both_existing || entry.root_path == kept_both
        }));
        assert!(
            kept_both_scan
                .iter()
                .all(|entry| entry.root_path.join("Part A").is_dir())
        );
        assert!(
            kept_both_scan
                .iter()
                .all(|entry| entry.root_path.join("Part B").is_dir())
        );
    }

    #[test]
    fn archived_xxmi_update_replace_and_merge_preserve_identity_and_category() {
        let temp = tempfile::tempdir().unwrap();
        let archive_root = temp.path().join("Mods_Archived");
        let existing = archive_root.join("Archived Mod");
        let source = temp.path().join("update");
        fs::create_dir_all(existing.join(MOD_META_DIR)).unwrap();
        fs::write(existing.join("old.ini"), "old").unwrap();
        let old_state = crate::model::PortableModState {
            id: "archived-identity".to_string(),
            metadata: {
                let mut metadata = crate::model::ModMetadata::default();
                metadata.user.category_id = Some("category-id".to_string());
                metadata.user.category = "Category".to_string();
                metadata
            },
            source: None,
            unsafe_content: false,
            unsafe_content_auto: None,
            unsafe_content_preference: Default::default(),
            created_at: None,
            updated_at: None,
        };
        fs::write(
            existing
                .join(MOD_META_DIR)
                .join(crate::model::MOD_META_FILE),
            serde_json::to_vec(&old_state).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(existing.join(DISABLED_CONTAINER)).unwrap();
        fs::rename(
            existing.join("old.ini"),
            existing.join(DISABLED_CONTAINER).join("old.ini"),
        )
        .unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("new.ini"), "new").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        let installed = install_candidate_with_state_cancelable(
            &source,
            "Archived Mod",
            &archive_root,
            None,
            ConflictChoice::Replace,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(installed, existing);
        assert!(installed.join("new.ini").is_file());
        assert!(!installed.join(DISABLED_CONTAINER).exists());
        let replaced_state = crate::persistence::load_portable_mod_state(&installed)
            .unwrap()
            .unwrap();
        assert_eq!(replaced_state.id, "archived-identity");
        assert_eq!(
            replaced_state.metadata.user.category_id.as_deref(),
            Some("category-id")
        );

        fs::write(source.join("merged.ini"), "merged").unwrap();
        fs::create_dir_all(existing.join(DISABLED_CONTAINER)).unwrap();
        fs::rename(
            existing.join("new.ini"),
            existing.join(DISABLED_CONTAINER).join("new.ini"),
        )
        .unwrap();
        let merged = install_candidate_with_state_cancelable(
            &source,
            "Archived Mod",
            &archive_root,
            None,
            ConflictChoice::Merge,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(merged, existing);
        assert!(merged.join("new.ini").is_file());
        assert!(merged.join("merged.ini").is_file());
        assert!(!merged.join(DISABLED_CONTAINER).exists());
        let merged_state = crate::persistence::load_portable_mod_state(&merged)
            .unwrap()
            .unwrap();
        assert_eq!(merged_state.id, "archived-identity");
        assert_eq!(
            merged_state.metadata.user.category_id.as_deref(),
            Some("category-id")
        );
    }

    #[test]
    fn disabled_unreal_publish_uses_disabled_root_and_keep_both_checks_peer_root() {
        let temp = tempfile::tempdir().unwrap();
        let active_root = temp.path().join("Paks");
        let disabled_root = temp.path().join("~mods-disabledByHestia");
        let source = temp.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("mod.pak"), "pak").unwrap();
        fs::create_dir_all(source.join(MOD_META_DIR)).unwrap();
        let mut source_state = crate::model::PortableModState {
            id: "source-id".to_string(),
            metadata: Default::default(),
            source: None,
            unsafe_content: false,
            unsafe_content_auto: None,
            unsafe_content_preference: Default::default(),
            created_at: None,
            updated_at: None,
        };
        source_state.metadata.user.title = Some("Imported title".to_string());
        fs::write(
            source.join(MOD_META_DIR).join(crate::model::MOD_META_FILE),
            serde_json::to_vec(&source_state).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(active_root.join("My Mod")).unwrap();
        fs::write(active_root.join("My Mod").join("active.pak"), "active").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        let installed = install_candidate_with_state_cancelable(
            &source,
            "My Mod",
            &active_root,
            Some(&disabled_root),
            ConflictChoice::KeepBoth,
            false,
            GameBackend::UnrealEngine,
            ModStatus::Disabled,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(installed, disabled_root.join("My Mod (2)"));
        assert!(!active_root.join("My Mod (2)").exists());
        assert!(installed.join("mod.pak").exists());
        assert!(installed.join(MOD_META_DIR).join("mod-root").exists());
        let cloned_state = crate::persistence::load_portable_mod_state(&installed)
            .unwrap()
            .unwrap();
        assert_ne!(cloned_state.id, source_state.id);
        assert_eq!(
            cloned_state.metadata.user.title.as_deref(),
            Some("Imported title")
        );
    }

    #[test]
    fn unreal_publish_scans_nested_active_and_disabled_category_wrappers() {
        let temp = tempfile::tempdir().unwrap();
        let active_root = temp.path().join("Paks").join("~mods");
        let category = "Ardelia";
        let active_category = active_root.join(category);
        let source = temp.path().join("nested-wrapper");
        fs::create_dir_all(source.join("Packages").join("Main")).unwrap();
        fs::create_dir_all(source.join("Packages").join("Companion")).unwrap();
        fs::write(
            source.join("Packages").join("Main").join("main.pak"),
            "main",
        )
        .unwrap();
        fs::write(
            source
                .join("Packages")
                .join("Companion")
                .join("companion.ucas"),
            "companion",
        )
        .unwrap();
        let game = crate::model::GameInstall {
            definition: crate::model::GameDefinition {
                id: "nte".to_string(),
                name: "Neverness To Everness".to_string(),
                backend: GameBackend::UnrealEngine,
                xxmi_code: String::new(),
            },
            mods_path_override: Some(active_root.clone()),
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: false,
            enabled: true,
        };
        let disabled_root = game.disabled_mods_path(false).unwrap();
        let disabled_category = disabled_root.join(category);
        let cancel = Arc::new(AtomicBool::new(false));
        let write_state = |root: &Path, id: &str| {
            let mut state = crate::model::PortableModState {
                id: id.to_string(),
                metadata: Default::default(),
                source: None,
                unsafe_content: false,
                unsafe_content_auto: None,
                unsafe_content_preference: Default::default(),
                created_at: None,
                updated_at: None,
            };
            state.metadata.user.category_id = Some("ardelia-id".to_string());
            state.metadata.user.category = category.to_string();
            fs::create_dir_all(root.join(MOD_META_DIR)).unwrap();
            fs::write(
                root.join(MOD_META_DIR).join(crate::model::MOD_META_FILE),
                serde_json::to_vec(&state).unwrap(),
            )
            .unwrap();
        };
        let assert_entry =
            |entries: &[crate::model::ModEntry], root: &Path, id: &str, status: ModStatus| {
                let entry = entries
                    .iter()
                    .find(|entry| entry.root_path == root)
                    .expect("expected nested Unreal wrapper in scan");
                assert_eq!(entry.id, id);
                assert_eq!(entry.status, status);
                assert_eq!(
                    entry.metadata.user.category_id.as_deref(),
                    Some("ardelia-id")
                );
                assert_eq!(entry.metadata.user.category, category);
                assert!(
                    entry
                        .root_path
                        .join("Packages")
                        .join("Main")
                        .join("main.pak")
                        .is_file()
                );
                assert!(
                    entry
                        .root_path
                        .join("Packages")
                        .join("Companion")
                        .join("companion.ucas")
                        .is_file()
                );
            };

        let active_existing = active_category.join("Nested Wrapper");
        fs::create_dir_all(&active_existing).unwrap();
        write_state(&active_existing, "active-id");
        fs::write(active_existing.join("old.pak"), "old").unwrap();
        let active_installed = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &active_category,
            Some(&disabled_category),
            ConflictChoice::Replace,
            false,
            GameBackend::UnrealEngine,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(active_installed, active_existing);
        assert!(
            active_installed
                .join(MOD_META_DIR)
                .join("mod-root")
                .is_file()
        );
        let scanned = crate::integrations::unrealengine::scan_game_mods(&game, false).unwrap();
        assert_eq!(scanned.len(), 1);
        assert_entry(&scanned, &active_existing, "active-id", ModStatus::Active);

        fs::write(
            source
                .join("Packages")
                .join("Companion")
                .join("companion.utoc"),
            "companion index",
        )
        .unwrap();
        let active_merged = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &active_category,
            Some(&disabled_category),
            ConflictChoice::Merge,
            false,
            GameBackend::UnrealEngine,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(active_merged, active_existing);
        let scanned = crate::integrations::unrealengine::scan_game_mods(&game, false).unwrap();
        assert_eq!(scanned.len(), 1);
        assert_entry(&scanned, &active_existing, "active-id", ModStatus::Active);
        assert!(
            active_existing
                .join("Packages")
                .join("Companion")
                .join("companion.utoc")
                .is_file()
        );

        let disabled_existing = disabled_category.join("Nested Wrapper");
        fs::create_dir_all(&disabled_existing).unwrap();
        write_state(&disabled_existing, "disabled-id");
        fs::write(disabled_existing.join("old.pak"), "old").unwrap();
        let disabled_installed = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &active_category,
            Some(&disabled_category),
            ConflictChoice::Replace,
            false,
            GameBackend::UnrealEngine,
            ModStatus::Disabled,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(disabled_installed, disabled_existing);
        assert!(
            disabled_installed
                .join(MOD_META_DIR)
                .join("mod-root")
                .is_file()
        );
        let scanned = crate::integrations::unrealengine::scan_game_mods(&game, false).unwrap();
        assert_eq!(scanned.len(), 2);
        assert_entry(&scanned, &active_existing, "active-id", ModStatus::Active);
        assert_entry(
            &scanned,
            &disabled_existing,
            "disabled-id",
            ModStatus::Disabled,
        );

        let disabled_merged = install_candidate_with_state_cancelable(
            &source,
            "Nested Wrapper",
            &active_category,
            Some(&disabled_category),
            ConflictChoice::Merge,
            false,
            GameBackend::UnrealEngine,
            ModStatus::Disabled,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(disabled_merged, disabled_existing);
        let scanned = crate::integrations::unrealengine::scan_game_mods(&game, false).unwrap();
        assert_eq!(scanned.len(), 2);
        assert_entry(&scanned, &active_existing, "active-id", ModStatus::Active);
        assert_entry(
            &scanned,
            &disabled_existing,
            "disabled-id",
            ModStatus::Disabled,
        );
    }

    #[test]
    fn replace_preserves_xxmi_stash_and_cancellation_leaves_target_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let target_root = temp.path().join("mods");
        let existing = target_root.join("Mod");
        let source = temp.path().join("source");
        fs::create_dir_all(existing.join(MOD_META_DIR)).unwrap();
        fs::write(existing.join("old.ini"), "old").unwrap();
        fs::write(existing.join(MOD_META_DIR).join("mod.cfg"), "stash").unwrap();
        let old_state = crate::model::PortableModState {
            id: "preserved-id".to_string(),
            metadata: Default::default(),
            source: None,
            unsafe_content: false,
            unsafe_content_auto: None,
            unsafe_content_preference: Default::default(),
            created_at: None,
            updated_at: None,
        };
        fs::write(
            existing
                .join(MOD_META_DIR)
                .join(crate::model::MOD_META_FILE),
            serde_json::to_vec(&old_state).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("new.ini"), "new").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        let installed = install_candidate_with_state_cancelable(
            &source,
            "Mod",
            &target_root,
            None,
            ConflictChoice::Replace,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("new.ini")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(installed.join(MOD_META_DIR).join("mod.cfg")).unwrap(),
            "stash"
        );
        assert_eq!(
            crate::persistence::load_portable_mod_state(&installed)
                .unwrap()
                .unwrap()
                .id,
            "preserved-id"
        );
        assert!(!installed.join("old.ini").exists());

        cancel.store(true, Ordering::Relaxed);
        let err = install_candidate_with_state_cancelable(
            &source,
            "Mod",
            &target_root,
            None,
            ConflictChoice::Replace,
            false,
            GameBackend::Xxmi,
            ModStatus::Active,
            &cancel,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), CANCELLED_ERROR);
        assert_eq!(
            fs::read_to_string(installed.join("new.ini")).unwrap(),
            "new"
        );
    }

    #[test]
    fn atomic_publish_rolls_back_when_destination_is_a_blocking_file() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("active").join("Old Mod");
        let destination = temp.path().join("disabled").join("New Mod");
        fs::create_dir_all(existing.join("nested")).unwrap();
        fs::write(existing.join("nested").join("old.ini"), b"old payload").unwrap();
        fs::create_dir_all(destination.join("blocking")).unwrap();
        let blocking_file = destination.join("blocking").join("file.txt");
        fs::write(&blocking_file, b"blocking file").unwrap();

        let result;
        {
            let stage = tempfile::Builder::new()
                .prefix(".hestia-test-stage-")
                .tempdir_in(temp.path())
                .unwrap();
            let staged = stage.path().join("final");
            fs::create_dir_all(&staged).unwrap();
            fs::write(staged.join("new.ini"), b"new payload").unwrap();
            result = atomic_publish_staged_dir(&staged, &destination, Some(&existing), None, None);
            assert!(result.is_err());
            assert!(existing.join("nested").join("old.ini").is_file());
            assert_eq!(
                fs::read(existing.join("nested").join("old.ini")).unwrap(),
                b"old payload"
            );
            assert_eq!(fs::read(&blocking_file).unwrap(), b"blocking file");
        }

        assert!(existing.join("nested").join("old.ini").is_file());
        assert!(!temp.path().read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".hestia-retired-")
        }));
    }
}
