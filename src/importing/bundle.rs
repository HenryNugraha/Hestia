//! Getting every file of one Thalia update ready before anything reaches the
//! Mods folder.
//!
//! Each archive is checked against GameBanana's checksum and unpacked first.
//! Files that go into the mod's own folder are combined into one folder here,
//! so the install that follows swaps the mod in a single step, and two merged
//! files that write the same path stop the update before anything changed.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result, bail};
use md5::{Digest, Md5};
use walkdir::WalkDir;
use xxhash_rust::xxh3::Xxh3;

use super::{
    CANCELLED_ERROR, CancelFlag, check_cancel, copy_dir_cancelable, inspect_source_cancelable,
    validate_install_folder_name,
};
use crate::model::{DISABLED_CONTAINER, ImportSource, MOD_META_DIR};
use crate::persistence;

/// Name prefix of the scratch folder one bundle is built in.
pub const BUNDLE_DIR_PREFIX: &str = "thalia-bundle-";

const READ_BUFFER_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundlePartRole {
    /// Installed as its own mod.
    Separate,
    /// The update itself. Always the first part of the mod's folder.
    Main,
    /// Added to the mod's folder.
    Merge,
}

#[derive(Debug, Clone)]
pub struct BundlePart {
    pub job_id: u64,
    pub file_name: String,
    pub archive: PathBuf,
    /// GameBanana's MD5 for the archive, when it lists one.
    pub md5: Option<String>,
    pub role: BundlePartRole,
}

#[derive(Debug, Clone)]
pub struct PreparedBundleFolder {
    pub job_id: u64,
    /// Folder holding exactly one mod folder, ready to install.
    pub source: PathBuf,
    /// What that mod folder held when it was checked.
    pub manifest: FolderManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleFailure {
    /// The archive is not the file GameBanana lists.
    Checksum { file_name: String },
    /// The archive does not hold exactly one mod folder.
    Layout { file_name: String },
    /// Two merged files write the same path with different contents.
    Clash { path: String },
    Canceled,
    Other {
        file_name: Option<String>,
        error: String,
    },
}

/// Every file in a folder with its size and a hash of its contents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderManifest(BTreeMap<String, (u64, u64)>);

/// Checks, unpacks and combines `parts` in a new scratch folder. Separate
/// parts come back first, in order, then the one combined mod folder (its
/// job is the first main or merge part). On failure nothing is left behind.
pub fn prepare_bundle(
    game_id: &str,
    parts: &[BundlePart],
    cancel: &CancelFlag,
) -> std::result::Result<(PathBuf, Vec<PreparedBundleFolder>), BundleFailure> {
    let extract_root = persistence::runtime_temp_extract_dir();
    let root = fs::create_dir_all(&extract_root)
        .and_then(|_| {
            tempfile::Builder::new()
                .prefix(BUNDLE_DIR_PREFIX)
                .tempdir_in(&extract_root)
        })
        .map_err(|err| BundleFailure::Other {
            file_name: None,
            error: format!("could not create bundle folder: {err}"),
        })?
        .keep();
    match prepare_into(&root, game_id, parts, cancel) {
        Ok(folders) => Ok((root, folders)),
        Err(failure) => {
            let _ = fs::remove_dir_all(&root);
            Err(failure)
        }
    }
}

fn prepare_into(
    root: &Path,
    game_id: &str,
    parts: &[BundlePart],
    cancel: &CancelFlag,
) -> std::result::Result<Vec<PreparedBundleFolder>, BundleFailure> {
    let failed = |part: &BundlePart| {
        let file_name = part.file_name.clone();
        move |err: anyhow::Error| {
            if err.to_string() == CANCELLED_ERROR {
                BundleFailure::Canceled
            } else {
                BundleFailure::Other {
                    file_name: Some(file_name.clone()),
                    error: format!("{err:#}"),
                }
            }
        }
    };
    let mut prepared = Vec::new();
    let mut mod_folder: Option<(u64, PathBuf)> = None;
    // Which merge part last wrote each path; `None` is the main file, which
    // merged files may overwrite.
    let mut writers: HashMap<String, Option<usize>> = HashMap::new();
    for (index, part) in parts.iter().enumerate() {
        check_cancel(cancel).map_err(failed(part))?;
        if let Some(expected) = part.md5.as_deref().filter(|md5| !md5.trim().is_empty()) {
            let actual = file_md5_hex(&part.archive, cancel).map_err(failed(part))?;
            if !actual.eq_ignore_ascii_case(expected.trim()) {
                return Err(BundleFailure::Checksum {
                    file_name: part.file_name.clone(),
                });
            }
        }
        let import = inspect_source_cancelable(
            game_id,
            ImportSource::Archive(part.archive.clone()),
            cancel,
        )
        .map_err(failed(part))?;
        let [candidate] = import.inspection.candidates.as_slice() else {
            return Err(BundleFailure::Layout {
                file_name: part.file_name.clone(),
            });
        };
        let label = folder_label(&candidate.label);
        match (part.role, &mod_folder) {
            (BundlePartRole::Separate, _) => {
                let source = root.join(part.job_id.to_string());
                let dest = source.join(&label);
                move_dir(&candidate.path, &dest, cancel).map_err(failed(part))?;
                let manifest = folder_manifest(&dest, cancel).map_err(failed(part))?;
                prepared.push(PreparedBundleFolder {
                    job_id: part.job_id,
                    source,
                    manifest,
                });
            }
            (BundlePartRole::Main | BundlePartRole::Merge, None) => {
                let dest = root.join(part.job_id.to_string()).join(&label);
                move_dir(&candidate.path, &dest, cancel).map_err(failed(part))?;
                let writer = (part.role == BundlePartRole::Merge).then_some(index);
                for key in file_keys(&dest).map_err(failed(part))? {
                    writers.insert(key, writer);
                }
                mod_folder = Some((part.job_id, dest));
            }
            (BundlePartRole::Main, Some(_)) => {
                return Err(BundleFailure::Other {
                    file_name: Some(part.file_name.clone()),
                    error: "the update file must come first".to_string(),
                });
            }
            (BundlePartRole::Merge, Some((_, dest))) => {
                merge_into(&candidate.path, dest, index, &mut writers, cancel)
                    .map_err(|err| match err {
                        MergeError::Clash(path) => BundleFailure::Clash { path },
                        MergeError::Io(err) => failed(part)(err),
                    })?;
            }
        }
    }
    if let Some((job_id, dest)) = mod_folder {
        let manifest = folder_manifest(&dest, cancel).map_err(|err| BundleFailure::Other {
            file_name: None,
            error: format!("{err:#}"),
        })?;
        prepared.push(PreparedBundleFolder {
            job_id,
            source: dest.parent().map(Path::to_path_buf).unwrap_or(dest),
            manifest,
        });
    }
    Ok(prepared)
}

enum MergeError {
    Clash(String),
    Io(anyhow::Error),
}

impl From<anyhow::Error> for MergeError {
    fn from(err: anyhow::Error) -> Self {
        Self::Io(err)
    }
}

impl From<std::io::Error> for MergeError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.into())
    }
}

/// Copies one merged file's contents into the mod folder. It may overwrite
/// the main file, which it was made for, but not another merged file: the
/// two have no order, so a different copy of the same path is a clash.
fn merge_into(
    source: &Path,
    dest: &Path,
    index: usize,
    writers: &mut HashMap<String, Option<usize>>,
    cancel: &CancelFlag,
) -> std::result::Result<(), MergeError> {
    for entry in WalkDir::new(source).min_depth(1).sort_by_file_name() {
        check_cancel(cancel)?;
        let entry = entry.map_err(anyhow::Error::from)?;
        let rel = entry
            .path()
            .strip_prefix(source)
            .map_err(anyhow::Error::from)?;
        let target = dest.join(rel);
        let key = path_key(rel);
        if entry.file_type().is_dir() {
            if target.is_file() {
                return Err(MergeError::Clash(display_path(rel)));
            }
            fs::create_dir_all(&target)?;
            continue;
        }
        if target.is_dir() {
            return Err(MergeError::Clash(display_path(rel)));
        }
        if target.is_file()
            && let Some(Some(other)) = writers.get(&key)
            && *other != index
            && !same_contents(entry.path(), &target)?
        {
            return Err(MergeError::Clash(display_path(rel)));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &target)
            .with_context(|| format!("could not copy {}", entry.path().display()))?;
        writers.insert(key, Some(index));
    }
    Ok(())
}

fn move_dir(source: &Path, dest: &Path, cancel: &CancelFlag) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(source, dest).is_ok() {
        return Ok(());
    }
    copy_dir_cancelable(source, dest, false, cancel)
}

fn folder_label(label: &str) -> String {
    let label = label.trim();
    if validate_install_folder_name(label).is_ok() {
        label.to_string()
    } else {
        "mod".to_string()
    }
}

/// Windows paths are case-insensitive, so `A.ini` and `a.ini` are one path.
fn path_key(rel: &Path) -> String {
    display_path(rel).to_lowercase()
}

fn display_path(rel: &Path) -> String {
    rel.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn file_keys(root: &Path) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    for entry in WalkDir::new(root).min_depth(1) {
        let entry = entry?;
        if entry.file_type().is_file() {
            keys.push(path_key(entry.path().strip_prefix(root)?));
        }
    }
    Ok(keys)
}

fn same_contents(a: &Path, b: &Path) -> Result<bool> {
    if fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (fs::File::open(a)?, fs::File::open(b)?);
    let mut left = vec![0_u8; READ_BUFFER_BYTES];
    let mut right = vec![0_u8; READ_BUFFER_BYTES];
    loop {
        let read = a.read(&mut left)?;
        if read == 0 {
            return Ok(true);
        }
        b.read_exact(&mut right[..read])?;
        if left[..read] != right[..read] {
            return Ok(false);
        }
    }
}

pub fn file_md5_hex(path: &Path, cancel: &CancelFlag) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut hasher = Md5::new();
    let mut buffer = vec![0_u8; READ_BUFFER_BYTES];
    loop {
        check_cancel(cancel)?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn bytes_match_md5(bytes: &[u8], expected: &str) -> bool {
    hex(&Md5::digest(bytes)).eq_ignore_ascii_case(expected.trim())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn file_xxh3(path: &Path, cancel: &CancelFlag) -> Result<u64> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Xxh3::new();
    let mut buffer = vec![0_u8; READ_BUFFER_BYTES];
    loop {
        check_cancel(cancel)?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hasher.digest());
        }
        hasher.update(&buffer[..read]);
    }
}

pub fn folder_manifest(root: &Path, cancel: &CancelFlag) -> Result<FolderManifest> {
    let mut files = BTreeMap::new();
    for entry in WalkDir::new(root).min_depth(1) {
        check_cancel(cancel)?;
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = display_path(entry.path().strip_prefix(root)?);
        let size = entry.metadata()?.len();
        files.insert(rel, (size, file_xxh3(entry.path(), cancel)?));
    }
    Ok(FolderManifest(files))
}

/// Fails unless `root` holds exactly what was checked, so files changed in
/// the scratch folder after the check never reach the Mods folder.
pub fn verify_folder_manifest(
    root: &Path,
    expected: &FolderManifest,
    cancel: &CancelFlag,
) -> Result<()> {
    if folder_manifest(root, cancel)? != *expected {
        bail!("the prepared files changed before they were installed");
    }
    Ok(())
}

/// A cheap fingerprint of a mod folder as it is on disk: every file's path,
/// size and modification time. Hestia's own metadata folder is left out, and
/// so is the disabled container, so turning the mod on or off does not count
/// as a change. `None` when the folder cannot be read.
pub fn mod_folder_state(root: &Path) -> Option<u64> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).min_depth(1) {
        let entry = entry.ok()?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(root).ok()?;
        let mut parts = rel.components().map(|part| part.as_os_str()).peekable();
        if parts.peek().is_some_and(|first| *first == MOD_META_DIR) {
            continue;
        }
        if parts.peek().is_some_and(|first| *first == DISABLED_CONTAINER) {
            parts.next();
        }
        let key = parts
            .map(|part| part.to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
            .join("/");
        let metadata = entry.metadata().ok()?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |time| time.as_nanos());
        files.push((key, metadata.len(), modified));
    }
    files.sort();
    let mut hasher = Xxh3::new();
    for (key, size, modified) in files {
        hasher.update(key.as_bytes());
        hasher.update(&[0]);
        hasher.update(&size.to_le_bytes());
        hasher.update(&modified.to_le_bytes());
    }
    Some(hasher.digest())
}

/// Scratch folders of bundles that never finished, left by a crash or a
/// closed Hestia. A bundle is never resumed, so they are only removed.
pub fn remove_stale_bundle_dirs() {
    let Ok(entries) = fs::read_dir(persistence::runtime_temp_extract_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(BUNDLE_DIR_PREFIX))
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod bundle_tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool};

    fn cancel() -> CancelFlag {
        Arc::new(AtomicBool::new(false))
    }

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn merge(
        dir: &Path,
        index: usize,
        writers: &mut HashMap<String, Option<usize>>,
        files: &[(&str, &str)],
    ) -> std::result::Result<(), String> {
        let source = dir.join(format!("part{index}"));
        for (rel, body) in files {
            write(&source, rel, body);
        }
        merge_into(&source, &dir.join("mod"), index, writers, &cancel()).map_err(|err| match err {
            MergeError::Clash(path) => path,
            MergeError::Io(err) => panic!("{err:#}"),
        })
    }

    #[test]
    fn merged_files_may_overwrite_the_main_file_but_not_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("mod");
        write(&folder, "Main.ini", "main");
        write(&folder, "tex/a.dds", "main");
        let mut writers: HashMap<String, Option<usize>> = file_keys(&folder)
            .unwrap()
            .into_iter()
            .map(|key| (key, None))
            .collect();

        // Overwriting the main file is what a merge is for.
        merge(dir.path(), 1, &mut writers, &[("main.ini", "addon")]).unwrap();
        assert_eq!(fs::read_to_string(folder.join("Main.ini")).unwrap(), "addon");
        // Another merged file with the same copy is fine.
        merge(dir.path(), 2, &mut writers, &[("MAIN.ini", "addon"), ("b.ini", "b")]).unwrap();
        // A different copy of a path another merged file wrote is not.
        assert_eq!(
            merge(dir.path(), 3, &mut writers, &[("Main.ini", "other")]),
            Err("Main.ini".to_string())
        );
        // Nor is a folder where a file is.
        assert_eq!(
            merge(dir.path(), 4, &mut writers, &[("b.ini/x.ini", "x")]),
            Err("b.ini".to_string())
        );
    }

    #[test]
    fn manifest_notices_changed_added_and_removed_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.ini", "a");
        write(dir.path(), "tex/b.dds", "b");
        let manifest = folder_manifest(dir.path(), &cancel()).unwrap();
        verify_folder_manifest(dir.path(), &manifest, &cancel()).unwrap();

        write(dir.path(), "a.ini", "A");
        assert!(verify_folder_manifest(dir.path(), &manifest, &cancel()).is_err());
        write(dir.path(), "a.ini", "a");
        write(dir.path(), "extra.ini", "x");
        assert!(verify_folder_manifest(dir.path(), &manifest, &cancel()).is_err());
        fs::remove_file(dir.path().join("extra.ini")).unwrap();
        fs::remove_file(dir.path().join("tex/b.dds")).unwrap();
        assert!(verify_folder_manifest(dir.path(), &manifest, &cancel()).is_err());
    }

    #[test]
    fn install_refuses_files_changed_after_the_check() {
        use crate::model::{ConflictChoice, GameBackend, ModStatus};
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("bundle").join("Mod");
        write(&source, "a.ini", "a");
        let manifest = folder_manifest(&source, &cancel()).unwrap();
        let mods = dir.path().join("Mods");
        write(&mods.join("Mod"), "a.ini", "old");
        let install = |manifest: &FolderManifest| {
            super::super::install_candidate_with_state_verified_cancelable(
                &source,
                "Mod",
                &mods,
                None,
                ConflictChoice::Replace,
                false,
                GameBackend::Xxmi,
                ModStatus::Active,
                Some(manifest),
                &cancel(),
            )
        };

        write(&source, "a.ini", "changed");
        let err = install(&manifest).unwrap_err();
        assert!(format!("{err:#}").contains("changed before they were installed"));
        assert_eq!(fs::read_to_string(mods.join("Mod/a.ini")).unwrap(), "old");

        write(&source, "a.ini", "a");
        install(&manifest).unwrap();
        assert_eq!(fs::read_to_string(mods.join("Mod/a.ini")).unwrap(), "a");
    }

    #[test]
    fn md5_matches_known_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        fs::write(&path, "hello").unwrap();
        assert_eq!(
            file_md5_hex(&path, &cancel()).unwrap(),
            "5d41402abc4b2a76b9719d911017c592"
        );
        assert!(bytes_match_md5(b"hello", " 5D41402ABC4B2A76B9719D911017C592 "));
        assert!(!bytes_match_md5(b"hello!", "5d41402abc4b2a76b9719d911017c592"));
    }

    #[test]
    fn mod_folder_state_ignores_metadata_and_on_off_but_not_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("mod");
        write(&root, "a.ini", "a");
        let before = mod_folder_state(&root).unwrap();
        write(&root, &format!("{MOD_META_DIR}/state.json"), "{}");
        assert_eq!(mod_folder_state(&root), Some(before));

        // Turning the mod off moves its files into the disabled container.
        let disabled = root.join(DISABLED_CONTAINER);
        fs::create_dir_all(&disabled).unwrap();
        fs::rename(root.join("a.ini"), disabled.join("a.ini")).unwrap();
        assert_eq!(mod_folder_state(&root), Some(before));

        write(&root, "b.ini", "b");
        assert_ne!(mod_folder_state(&root), Some(before));
    }

    #[test]
    fn bundle_combines_main_and_merge_and_keeps_separate_apart() {
        let dir = tempfile::tempdir().unwrap();
        let zip = |name: &str, files: &[(&str, &str)]| {
            let path = dir.path().join(name);
            let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            for (rel, body) in files {
                writer
                    .start_file(*rel, zip::write::SimpleFileOptions::default())
                    .unwrap();
                std::io::Write::write_all(&mut writer, body.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
            path
        };
        let part = |job_id, name: &str, archive: PathBuf, role| BundlePart {
            job_id,
            file_name: name.to_string(),
            md5: Some(file_md5_hex(&archive, &cancel()).unwrap()),
            archive,
            role,
        };
        let parts = vec![
            part(
                1,
                "lib.zip",
                zip("lib.zip", &[("Ink Spirit/lib.ini", "lib")]),
                BundlePartRole::Separate,
            ),
            part(
                2,
                "main.zip",
                zip("main.zip", &[("Mod/main.ini", "main")]),
                BundlePartRole::Main,
            ),
            part(
                3,
                "addon.zip",
                zip("addon.zip", &[("Addon/addon.ini", "addon")]),
                BundlePartRole::Merge,
            ),
        ];
        let (root, folders) = prepare_bundle("game", &parts, &cancel()).unwrap();
        assert_eq!(
            folders.iter().map(|folder| folder.job_id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(folders[0].source.join("Ink Spirit/lib.ini").is_file());
        let combined = folders[1].source.join("Mod");
        assert!(combined.join("main.ini").is_file());
        assert!(combined.join("addon.ini").is_file());
        verify_folder_manifest(&combined, &folders[1].manifest, &cancel()).unwrap();
        fs::remove_dir_all(root).unwrap();

        // A file that is not the one GameBanana lists stops everything.
        let mut tampered = parts.clone();
        tampered[2].md5 = Some("0".repeat(32));
        assert_eq!(
            prepare_bundle("game", &tampered, &cancel()).unwrap_err(),
            BundleFailure::Checksum {
                file_name: "addon.zip".to_string()
            }
        );
    }
}
