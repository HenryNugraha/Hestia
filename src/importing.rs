use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};
use tempfile::TempDir;
use walkdir::WalkDir;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use crate::model::{ConflictChoice, ImportCandidate, ImportInspection, ImportSource};
use crate::persistence;

pub const CANCELLED_ERROR: &str = "install canceled";
pub type CancelFlag = Arc<AtomicBool>;

pub struct PreparedImport {
    pub _temp_dir: Option<TempDir>,
    pub inspection: ImportInspection,
}

fn check_cancel(flag: &CancelFlag) -> Result<()> {
    if flag.load(Ordering::Relaxed) {
        bail!(CANCELLED_ERROR);
    }
    Ok(())
}

#[allow(dead_code)]
pub fn inspect_source(game_id: &str, source: ImportSource) -> Result<PreparedImport> {
    match source.clone() {
        ImportSource::Folder(path) => {
            let inspection = inspect_directory(game_id, &source, &path)?;
            Ok(PreparedImport {
                _temp_dir: None,
                inspection,
            })
        }
        ImportSource::Archive(path) => {
            let extract_root = persistence::runtime_temp_extract_dir();
            fs::create_dir_all(&extract_root)?;
            let temp_dir = tempfile::Builder::new()
                .prefix("inspect-")
                .tempdir_in(&extract_root)
                .context("failed to create temp dir for archive inspection")?;
            extract_archive(&path, temp_dir.path())?;
            let inspection = inspect_directory(game_id, &source, temp_dir.path())?;
            Ok(PreparedImport {
                _temp_dir: Some(temp_dir),
                inspection,
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
    match source.clone() {
        ImportSource::Folder(path) => {
            let inspection = inspect_directory_cancelable(game_id, &source, &path, cancel)?;
            Ok(PreparedImport {
                _temp_dir: None,
                inspection,
            })
        }
        ImportSource::Archive(path) => {
            let extract_root = persistence::runtime_temp_extract_dir();
            fs::create_dir_all(&extract_root)?;
            let temp_dir = tempfile::Builder::new()
                .prefix("inspect-")
                .tempdir_in(&extract_root)
                .context("failed to create temp dir for archive inspection")?;
            extract_archive_cancelable(&path, temp_dir.path(), cancel)?;
            check_cancel(cancel)?;
            let inspection = inspect_directory_cancelable(game_id, &source, temp_dir.path(), cancel)?;
            Ok(PreparedImport {
                _temp_dir: Some(temp_dir),
                inspection,
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

    let mut candidates = Vec::new();
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

    let mut candidates = Vec::new();
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
    let file = fs::File::open(archive)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("archive contains invalid path"))?;
        let outpath = destination.join(enclosed);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }
    }
    Ok(())
}

fn extract_zip_cancelable(
    archive: &Path,
    destination: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        check_cancel(cancel)?;
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("archive contains invalid path"))?;
        let outpath = destination.join(enclosed);
        if entry.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn extract_7z(archive: &Path, destination: &Path) -> Result<()> {
    sevenz_rust::decompress_file(archive, destination).context("failed to extract .7z archive")
}

fn extract_7z_cancelable(
    archive: &Path,
    destination: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
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

fn extract_rar_cancelable(
    archive: &Path,
    destination: &Path,
    cancel: &CancelFlag,
) -> Result<()> {
    check_cancel(cancel)?;
    match extract_rar_with_unrar(archive, destination, Some(cancel)) {
        Ok(()) => Ok(()),
        Err(unrar_err) => extract_rar_with_7z_fallback(archive, destination, Some(cancel), unrar_err),
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
    bail!(
        "{:#}\n7-Zip fallback failed: {}",
        unrar_error,
        stderr
    );
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
    fs::create_dir_all(destination)?;
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), target)?;
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
    fs::create_dir_all(destination)?;
    for entry in WalkDir::new(source) {
        check_cancel(cancel)?;
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn install_candidate_cancelable(
    candidate_path: &Path,
    preferred_name: &str,
    target_root: &Path,
    choice: ConflictChoice,
    cancel: &CancelFlag,
) -> Result<Option<PathBuf>> {
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
                copy_dir_cancelable(candidate_path, &target, false, cancel)?;
                Ok(Some(target))
            }
        };
    }

    copy_dir_cancelable(candidate_path, &initial_target, false, cancel)?;
    Ok(Some(initial_target))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
