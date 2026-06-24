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
            label: nested
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("mod")
                .to_string(),
            path: nested.clone(),
        });
        notice = Some("Nested top-level folder detected. Hestia will import the inner folder as the mod root.".to_string());
    } else if top_level_dirs.len() > 1 && top_level_files.is_empty() {
        for dir in top_level_dirs {
            candidates.push(ImportCandidate {
                label: dir
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or("mod")
                    .to_string(),
                path: dir,
            });
        }
        notice = Some("Multiple top-level folders detected. Choose which folder should be treated as the mod root.".to_string());
    } else if top_level_dirs.is_empty() && top_level_files.is_empty() {
        bail!("import source is empty");
    } else {
        let label = match source {
            ImportSource::Folder(path) | ImportSource::Archive(path) => path
                .file_stem()
                .or_else(|| path.file_name())
                .and_then(OsStr::to_str)
                .unwrap_or("mod")
                .to_string(),
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
            label: nested
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("mod")
                .to_string(),
            path: nested.clone(),
        });
        notice = Some("Nested top-level folder detected. Hestia will import the inner folder as the mod root.".to_string());
    } else if top_level_dirs.len() > 1 && top_level_files.is_empty() {
        for dir in top_level_dirs {
            candidates.push(ImportCandidate {
                label: dir
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or("mod")
                    .to_string(),
                path: dir,
            });
        }
        notice = Some("Multiple top-level folders detected. Choose which folder should be treated as the mod root.".to_string());
    } else if top_level_dirs.is_empty() && top_level_files.is_empty() {
        bail!("import source is empty");
    } else {
        let label = match source {
            ImportSource::Folder(path) | ImportSource::Archive(path) => path
                .file_stem()
                .or_else(|| path.file_name())
                .and_then(OsStr::to_str)
                .unwrap_or("mod")
                .to_string(),
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
    let extension = archive
        .extension()
        .and_then(OsStr::to_str)
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("unsupported archive with no extension"))?;

    match extension.as_str() {
        "zip" => extract_zip(archive, destination),
        "7z" => extract_7z(archive, destination),
        "rar" => extract_rar(archive, destination),
        _ => bail!("unsupported archive format: {}", extension),
    }
}

fn extract_archive_cancelable(
    archive: &Path,
    destination: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
    check_cancel(cancel)?;
    let extension = archive
        .extension()
        .and_then(OsStr::to_str)
        .map(|s| s.to_ascii_lowercase())
        .ok_or_else(|| anyhow!("unsupported archive with no extension"))?;

    match extension.as_str() {
        "zip" => extract_zip_cancelable(archive, destination, cancel),
        "7z" => extract_7z_cancelable(archive, destination, cancel),
        "rar" => extract_rar_cancelable(archive, destination, cancel),
        _ => bail!("unsupported archive format: {}", extension),
    }
}

#[allow(dead_code)]
fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    extract_zip_with_cancel(archive, destination, None)
}

fn extract_zip_cancelable(archive: &Path, destination: &Path, cancel: &CancelFlag) -> Result<()> {
    extract_zip_with_cancel(archive, destination, Some(cancel))
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

#[allow(dead_code)]
fn extract_7z(archive: &Path, destination: &Path) -> Result<()> {
    sevenz_rust::decompress_file(archive, destination).context("failed to extract .7z archive")
}

fn extract_7z_cancelable(archive: &Path, destination: &Path, cancel: &CancelFlag) -> Result<()> {
    check_cancel(cancel)?;
    sevenz_rust::decompress_file(archive, destination).context("failed to extract .7z archive")?;
    check_cancel(cancel)?;
    Ok(())
}

#[allow(dead_code)]
fn extract_rar(archive: &Path, destination: &Path) -> Result<()> {
    match extract_rar_with_unrar(archive, destination, None) {
        Ok(()) => Ok(()),
        Err(unrar_err) => extract_rar_with_7z_fallback(archive, destination, None, unrar_err),
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

fn extract_rar_cancelable(archive: &Path, destination: &Path, cancel: &CancelFlag) -> Result<()> {
    check_cancel(cancel)?;
    match extract_rar_with_unrar(archive, destination, Some(cancel)) {
        Ok(()) => Ok(()),
        Err(unrar_err) => {
            extract_rar_with_7z_fallback(archive, destination, Some(cancel), unrar_err)
        }
    }
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
    check_cancel(cancel)?;
    if destination.exists() && replace_existing {
        fs::remove_dir_all(destination)?;
    }
    let mut created_dirs = HashSet::new();
    ensure_directory(destination, &mut created_dirs)?;
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
            copy_file_with_parent_recovery(entry.path(), &target)?;
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
                copy_dir_cancelable(candidate_path, &initial_target, true, cancel)?;
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
mod tests {
    use super::*;
    use std::io::Write;

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
