//! Per-game profile storage and `.tzst` archive primitives.
//!
//! The active profile is represented by the live game directories.  Inactive profiles are
//! tar+zstd archives below a backend-specific `Mods_Profiles` directory.  This module deliberately
//! contains no UI or switching policy; callers can compose the primitives into a transactional
//! switch operation.

use std::{
    cell::Cell,
    collections::HashSet,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    rc::Rc,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::{Archive, Builder, EntryType, Header};
use uuid::Uuid;
use zstd::stream::{read::Decoder, write::Encoder};

use crate::model::{GameBackend, GameInstall, MODS_PROFILES_DIR, ModCategory};

pub const PROFILE_ARCHIVE_EXTENSION: &str = "tzst";
pub const PROFILE_ARCHIVE_FORMAT_VERSION: u32 = 1;
pub const PROFILE_METADATA_FILE: &str = "profile.json";
pub const PROFILE_README_FILE: &str = "readme.txt";
pub const PROFILE_STAGING_DIR: &str = ".staging";
pub const PROFILE_ARCHIVE_RESERVED_ARCHIVED_ROOT: &str = "Archived";
pub const PROFILE_README_CONTENT: &str = "\
Hestia profile archives

The .tzst files in this folder are Hestia profile archives. Each file contains a tar archive compressed with Zstandard.

You can open them with:
- Windows 11 File Explorer (with current system updates)
- 7-Zip 24.05 or newer
- WinRAR 6.21 or newer

Hestia manages these archives automatically. Do not rename, modify, move, or delete them while Hestia is running.
";

/// The live roots captured by a profile archive.  XXMI uses `archived`; Unreal uses `disabled`
/// and reserves `Archived` in the archive layout for future support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRoots {
    pub profiles_dir: PathBuf,
    pub mods: PathBuf,
    pub archived: Option<PathBuf>,
    pub disabled: Option<PathBuf>,
}

impl ProfileRoots {
    pub fn staging_dir(&self) -> PathBuf {
        self.profiles_dir.join(PROFILE_STAGING_DIR)
    }

    pub fn archive_path(&self, profile_id: Uuid) -> PathBuf {
        self.profiles_dir
            .join(format!("{profile_id}.{PROFILE_ARCHIVE_EXTENSION}"))
    }
}

/// Ensure the persistent profile storage directory and its user-facing archive note exist.
pub fn ensure_profile_storage_layout(roots: &ProfileRoots) -> Result<()> {
    fs::create_dir_all(&roots.profiles_dir).with_context(|| {
        format!(
            "failed to create profile storage directory {}",
            roots.profiles_dir.display()
        )
    })?;
    let readme = roots.profiles_dir.join(PROFILE_README_FILE);
    if !fs::read(&readme)
        .is_ok_and(|contents| contents.as_slice() == PROFILE_README_CONTENT.as_bytes())
    {
        fs::write(&readme, PROFILE_README_CONTENT)
            .with_context(|| format!("failed to write {}", readme.display()))?;
    }
    Ok(())
}

/// Derive backend-safe profile directories from the game's current mod roots.
pub fn profile_roots(game: &GameInstall, use_default_mods_path: bool) -> Result<ProfileRoots> {
    let mods = game
        .mods_path(use_default_mods_path)
        .context("game does not have a configured mods path")?;
    let parent = mods
        .parent()
        .context("mods path has no parent directory")?
        .to_path_buf();

    match game.definition.backend {
        GameBackend::Xxmi => Ok(ProfileRoots {
            profiles_dir: parent.join(MODS_PROFILES_DIR),
            mods,
            archived: Some(parent.join("Mods_Archived")),
            disabled: None,
        }),
        GameBackend::UnrealEngine => {
            // NTE's live path is HT/Content/Paks/~mods.  Keep profile archives outside Paks so
            // Unreal never attempts to load them.  Custom non-Paks paths use their parent as the
            // content-like root, preserving the same safety property.
            let content_root = if parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Paks"))
            {
                parent
                    .parent()
                    .context("Unreal Paks path has no Content parent")?
                    .to_path_buf()
            } else {
                parent.clone()
            };
            Ok(ProfileRoots {
                profiles_dir: content_root.join(MODS_PROFILES_DIR),
                mods,
                archived: None,
                disabled: game.disabled_mods_path(use_default_mods_path),
            })
        }
    }
}

pub fn profile_archive_path(
    game: &GameInstall,
    use_default_mods_path: bool,
    profile_id: Uuid,
) -> Result<PathBuf> {
    Ok(profile_roots(game, use_default_mods_path)?.archive_path(profile_id))
}

/// Return free bytes on the volume hosting profile storage.  Callers should invoke this before
/// staging a switch, reserving space for both the extracted target and the outgoing archive.
pub fn available_profile_space(path: &Path) -> Result<u64> {
    let probe = if path.exists() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    fs2::available_space(probe)
        .with_context(|| format!("failed to query free space for {}", probe.display()))
}

pub fn ensure_profile_space(path: &Path, required_bytes: u64) -> Result<()> {
    let available = available_profile_space(path)?;
    if available < required_bytes {
        bail!(
            "insufficient profile storage: {} bytes available, {} required",
            available,
            required_bytes
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileArchiveMetadata {
    pub format_version: u32,
    pub profile_id: Uuid,
    pub game_id: String,
    pub display_name: String,
    #[serde(default)]
    pub backend: GameBackend,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub uncompressed_size: u64,
    #[serde(default)]
    pub file_count: u64,
    /// Metadata that is portable across machines and survives profile renames.
    #[serde(default)]
    pub portable_metadata: std::collections::HashMap<String, serde_json::Value>,
    /// Profile-owned category definitions. `None` preserves compatibility with archives
    /// written before categories became profile-scoped; `Some(vec![])` is intentional empty.
    #[serde(default)]
    pub categories: Option<Vec<ModCategory>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveResult {
    pub archive_path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub uncompressed_size: u64,
    pub file_count: u64,
}

/// Archive or extraction progress measured against the uncompressed profile payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArchiveProgress {
    pub files_processed: u64,
    pub bytes_processed: u64,
    pub total_files: u64,
    pub total_bytes: u64,
}

pub type ProgressCallback<'a> = &'a mut dyn FnMut(ArchiveProgress) -> Result<()>;

/// Progress while reading a compressed profile archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArchiveReadProgress {
    pub bytes_read: u64,
    pub total_bytes: u64,
}

pub type ReadProgressCallback<'a> = &'a mut dyn FnMut(ArchiveReadProgress) -> Result<()>;

struct ProfileArchiveHashReader {
    file: File,
    hasher: Sha256,
}

impl ProfileArchiveHashReader {
    fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            file: File::open(path)?,
            hasher: Sha256::new(),
        })
    }

    fn finish_sha256(self) -> String {
        format_sha256(self.hasher)
    }
}

impl Read for ProfileArchiveHashReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.file.read(buffer)?;
        self.hasher.update(&buffer[..read]);
        Ok(read)
    }
}

#[derive(Default)]
struct ProfileArchiveProgressState {
    raw_bytes_read: Cell<u64>,
    payload_start: Cell<u64>,
    payload_total: Cell<u64>,
}

struct ProfileArchiveProgressReader<'a, R> {
    inner: R,
    state: Rc<ProfileArchiveProgressState>,
    callback: Option<ReadProgressCallback<'a>>,
}

impl<'a, R> ProfileArchiveProgressReader<'a, R> {
    fn new(
        inner: R,
        state: Rc<ProfileArchiveProgressState>,
        callback: Option<ReadProgressCallback<'a>>,
    ) -> Self {
        Self {
            inner,
            state,
            callback,
        }
    }

    fn finish(self) -> R {
        self.inner
    }

    fn report_progress(&mut self) -> io::Result<()> {
        if let Some(callback) = self.callback.as_deref_mut() {
            let total_bytes = self.state.payload_total.get();
            let bytes_read = self
                .state
                .raw_bytes_read
                .get()
                .saturating_sub(self.state.payload_start.get())
                .min(total_bytes);
            callback(ArchiveReadProgress {
                bytes_read,
                total_bytes,
            })
            .map_err(|error| io::Error::other(error.to_string()))?;
        }
        Ok(())
    }
}

impl<R: Read> Read for ProfileArchiveProgressReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.report_progress()?;
        let read = self.inner.read(buffer)?;
        self.state
            .raw_bytes_read
            .set(self.state.raw_bytes_read.get().saturating_add(read as u64));
        self.report_progress()?;
        Ok(read)
    }
}

/// Create a profile archive atomically through a sibling `.tzst.part` file.
pub fn create_profile_archive_with_progress(
    roots: &ProfileRoots,
    metadata: &ProfileArchiveMetadata,
    destination: &Path,
    mut progress: Option<ProgressCallback<'_>>,
) -> Result<ArchiveResult> {
    ensure_tzst_path(destination)?;
    if metadata.format_version == 0 {
        bail!("profile archive format version must be non-zero");
    }
    validate_source_tree(&roots.mods)?;
    if let Some(path) = &roots.archived {
        validate_source_tree(path)?;
    }
    if let Some(path) = &roots.disabled {
        validate_source_tree(path)?;
    }
    let mut stats = SourceStats::default();
    collect_source_stats(&roots.mods, &mut stats)?;
    if let Some(path) = &roots.archived {
        collect_source_stats(path, &mut stats)?;
    }
    if let Some(path) = &roots.disabled {
        collect_source_stats(path, &mut stats)?;
    }
    let mut archive_metadata = metadata.clone();
    archive_metadata.uncompressed_size = stats.bytes;
    archive_metadata.file_count = stats.files;
    if let Some(callback) = progress.as_deref_mut() {
        callback(ArchiveProgress {
            total_files: stats.files,
            total_bytes: stats.bytes,
            ..ArchiveProgress::default()
        })?;
    }

    let parent = destination
        .parent()
        .context("profile archive destination has no parent")?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(
        "{}.part",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("profile.tzst")
    ));
    if staging.exists() {
        fs::remove_file(&staging)?;
    }

    let file = File::create(&staging)
        .with_context(|| format!("failed to create {}", staging.display()))?;
    let mut encoder = Encoder::new(file, -1).context("failed to initialize zstd encoder")?;
    let threads = num_cpus::get().max(1) as u32;
    // Match the zstd CLI's `-T0` behavior by using all available logical processors.
    encoder
        .multithread(threads)
        .context("failed to configure zstd worker threads")?;
    encoder
        .include_checksum(true)
        .context("failed to enable zstd checksum")?;
    let mut builder = Builder::new(encoder);
    builder.follow_symlinks(false);

    append_json(&mut builder, PROFILE_METADATA_FILE, &archive_metadata)?;
    let mut processed = ArchiveProgress {
        total_files: stats.files,
        total_bytes: stats.bytes,
        ..ArchiveProgress::default()
    };
    append_tree(
        &mut builder,
        &roots.mods,
        "Mods",
        &mut processed,
        &mut progress,
    )?;
    if let Some(path) = &roots.archived {
        append_tree(
            &mut builder,
            path,
            "Mods_Archived",
            &mut processed,
            &mut progress,
        )?;
    }
    if let Some(path) = &roots.disabled {
        append_tree(
            &mut builder,
            path,
            "Disabled",
            &mut processed,
            &mut progress,
        )?;
        append_empty_dir(&mut builder, PROFILE_ARCHIVE_RESERVED_ARCHIVED_ROOT)?;
    }
    let encoder = builder
        .into_inner()
        .context("failed to finalize tar stream")?;
    let mut file = encoder.finish().context("failed to finalize zstd stream")?;
    file.flush()?;
    drop(file);

    let bytes = fs::metadata(&staging)?.len();
    let sha256 = sha256_file(&staging)?;
    commit_archive_part(&staging, destination)?;
    Ok(ArchiveResult {
        archive_path: destination.to_path_buf(),
        sha256,
        bytes,
        uncompressed_size: stats.bytes,
        file_count: stats.files,
    })
}

/// Extract a `.tzst` archive into a new empty staging directory, validating every path before
/// writing.  Absolute paths, traversal, special files, and links escaping the destination are
/// rejected.
pub fn extract_profile_archive(
    archive_path: &Path,
    destination: &Path,
) -> Result<ProfileArchiveMetadata> {
    extract_profile_archive_with_progress(archive_path, destination, None)
}

/// Progress/cancellation-aware extraction variant. Validation and extraction happen in one
/// decompression pass. Callers should use a disposable staging directory because an error can
/// leave already-validated entries in the destination.
pub fn extract_profile_archive_with_progress(
    archive_path: &Path,
    destination: &Path,
    progress: Option<ProgressCallback<'_>>,
) -> Result<ProfileArchiveMetadata> {
    extract_profile_archive_impl(archive_path, destination, None, progress, None)
}

/// Extract and verify an archive in one compressed-input pass.
///
/// The checksum is compared only after extraction has completed in the disposable staging
/// directory, so callers can validate the selected profile before touching the active roots.
pub fn extract_profile_archive_verified_with_progress(
    archive_path: &Path,
    destination: &Path,
    expected_sha256: Option<&str>,
    read_progress: Option<ReadProgressCallback<'_>>,
) -> Result<ProfileArchiveMetadata> {
    extract_profile_archive_impl(
        archive_path,
        destination,
        expected_sha256,
        None,
        read_progress,
    )
}

fn extract_profile_archive_impl(
    archive_path: &Path,
    destination: &Path,
    expected_sha256: Option<&str>,
    mut progress: Option<ProgressCallback<'_>>,
    read_progress: Option<ReadProgressCallback<'_>>,
) -> Result<ProfileArchiveMetadata> {
    ensure_tzst_path(archive_path)?;
    if destination.exists() {
        if !destination.is_dir() || fs::read_dir(destination)?.next().is_some() {
            bail!("profile extraction destination must be a new empty directory");
        }
    } else {
        fs::create_dir_all(destination)?;
    }

    let hash_reader = ProfileArchiveHashReader::new(archive_path)?;
    let decoder = Decoder::new(hash_reader)?;
    let progress_state = Rc::new(ProfileArchiveProgressState::default());
    let mut progress_reader =
        ProfileArchiveProgressReader::new(decoder, Rc::clone(&progress_state), read_progress);
    let mut archive = Archive::new(&mut progress_reader);
    let mut seen = HashSet::new();
    let mut metadata: Option<ProfileArchiveMetadata> = None;
    let mut processed = ArchiveProgress::default();
    for item in archive.entries()? {
        let mut entry = item?;
        let path = entry.path()?.into_owned();
        validate_relative_path(&path)?;
        if !seen.insert(path.clone()) {
            bail!("profile archive contains duplicate path {}", path.display());
        }
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            let target = entry
                .link_name()?
                .context("profile archive link has no target")?;
            validate_link_target(&path, &target)?;
        } else if !(entry_type.is_file() || entry_type.is_dir()) {
            bail!(
                "profile archive contains unsupported entry type at {}",
                path.display()
            );
        }
        let size = entry.size();
        if path == Path::new(PROFILE_METADATA_FILE) {
            if !entry_type.is_file() {
                bail!("profile.json must be a regular file");
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            let parsed: ProfileArchiveMetadata =
                serde_json::from_slice(&bytes).context("failed to parse embedded profile.json")?;
            if parsed.format_version == 0 {
                bail!("profile archive format version is invalid");
            }
            progress_state
                .payload_start
                .set(progress_state.raw_bytes_read.get());
            progress_state.payload_total.set(parsed.uncompressed_size);
            processed.total_files = parsed.file_count;
            processed.total_bytes = parsed.uncompressed_size;
            metadata = Some(parsed);
            if let Some(callback) = progress.as_deref_mut() {
                callback(processed)?;
            }
            continue;
        }
        entry
            .unpack_in(destination)
            .with_context(|| format!("failed to extract profile entry {}", path.display()))?;
        if entry_type.is_file() {
            processed.files_processed += 1;
            processed.bytes_processed += size;
        }
        if let Some(callback) = progress.as_deref_mut() {
            callback(processed)?;
        }
    }
    let metadata = metadata.context("profile archive is missing profile.json")?;
    if processed.files_processed != metadata.file_count
        || processed.bytes_processed != metadata.uncompressed_size
    {
        bail!(
            "profile archive payload does not match profile.json (expected {} files / {} bytes, got {} files / {} bytes)",
            metadata.file_count,
            metadata.uncompressed_size,
            processed.files_processed,
            processed.bytes_processed
        );
    }
    drop(archive);
    io::copy(&mut progress_reader, &mut io::sink())?;
    let decoder = progress_reader.finish();
    let reader = decoder.finish().into_inner();
    let actual_sha256 = reader.finish_sha256();
    if let Some(expected_sha256) = expected_sha256
        && !actual_sha256.eq_ignore_ascii_case(expected_sha256.trim())
    {
        bail!(
            "profile archive checksum mismatch: expected {}, got {}",
            expected_sha256,
            actual_sha256
        );
    }
    Ok(metadata)
}

/// Read the embedded profile manifest without materializing payload files. Full entry validation
/// is performed by extraction.
pub fn read_profile_archive_metadata(archive_path: &Path) -> Result<ProfileArchiveMetadata> {
    ensure_tzst_path(archive_path)?;
    let mut decoder = Decoder::new(File::open(archive_path)?)?;
    let mut archive = Archive::new(&mut decoder);
    for item in archive.entries()? {
        let mut entry = item?;
        let path = entry.path()?.into_owned();
        validate_relative_path(&path)?;
        if path != Path::new(PROFILE_METADATA_FILE) {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            bail!("profile.json must be a regular file");
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let metadata: ProfileArchiveMetadata =
            serde_json::from_slice(&bytes).context("failed to parse embedded profile.json")?;
        if metadata.format_version == 0 {
            bail!("profile archive format version is invalid");
        }
        return Ok(metadata);
    }
    bail!("profile archive is missing profile.json")
}

/// Compute the SHA-256 digest of a completed `.tzst` archive.
#[cfg(test)]
pub fn profile_archive_sha256(archive_path: &Path) -> Result<String> {
    ensure_tzst_path(archive_path)?;
    if !archive_path.is_file() {
        bail!("profile archive does not exist: {}", archive_path.display());
    }
    sha256_file(archive_path)
}

#[cfg(test)]
pub fn verify_profile_archive_sha256(archive_path: &Path, expected: &str) -> Result<()> {
    let actual = profile_archive_sha256(archive_path)?;
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        bail!(
            "profile archive checksum mismatch: expected {}, got {}",
            expected,
            actual
        );
    }
    Ok(())
}

fn append_json<W: Write>(
    builder: &mut Builder<W>,
    path: &str,
    value: &impl Serialize,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o644);
    header.set_size(bytes.len() as u64);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes.as_slice())?;
    Ok(())
}

fn append_tree<W: Write>(
    builder: &mut Builder<W>,
    source: &Path,
    archive_name: &str,
    progress: &mut ArchiveProgress,
    callback: &mut Option<ProgressCallback<'_>>,
) -> Result<()> {
    if !source.exists() {
        append_empty_dir(builder, archive_name)?;
        return Ok(());
    }
    for item in walkdir::WalkDir::new(source).follow_links(false) {
        let item = item?;
        let relative = item.path().strip_prefix(source)?;
        let target = if relative.as_os_str().is_empty() {
            PathBuf::from(archive_name)
        } else {
            PathBuf::from(archive_name).join(relative)
        };
        if item.file_type().is_dir() {
            builder.append_dir(&target, item.path())?;
        } else if item.file_type().is_file() {
            builder.append_path_with_name(item.path(), &target)?;
            progress.files_processed += 1;
            progress.bytes_processed += item.metadata()?.len();
            if let Some(callback) = callback.as_deref_mut() {
                callback(*progress)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct SourceStats {
    files: u64,
    bytes: u64,
}

fn collect_source_stats(path: &Path, stats: &mut SourceStats) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    for item in walkdir::WalkDir::new(path).follow_links(false) {
        let item = item?;
        if item.file_type().is_file() {
            stats.files += 1;
            stats.bytes += item.metadata()?.len();
        }
    }
    Ok(())
}

fn commit_archive_part(staging: &Path, destination: &Path) -> Result<()> {
    let backup = PathBuf::from(format!("{}.bak", destination.display()));
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    let had_existing = destination.exists();
    if had_existing {
        fs::rename(destination, &backup).with_context(|| {
            format!(
                "failed to stage existing profile archive {}",
                destination.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(staging, destination) {
        if had_existing {
            let _ = fs::rename(&backup, destination);
        }
        return Err(error).with_context(|| {
            format!("failed to commit profile archive {}", destination.display())
        });
    }
    if had_existing {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn append_empty_dir<W: Write>(builder: &mut Builder<W>, path: &str) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_mode(0o755);
    header.set_size(0);
    header.set_cksum();
    builder.append_data(&mut header, format!("{path}/"), io::empty())?;
    Ok(())
}

fn validate_source_tree(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        bail!("profile root is not a directory: {}", path.display());
    }
    for item in walkdir::WalkDir::new(path).follow_links(false) {
        let item = item?;
        if item.file_type().is_symlink() {
            bail!(
                "profile roots cannot contain symlinks: {}",
                item.path().display()
            );
        }
        if !item.file_type().is_dir() && !item.file_type().is_file() {
            bail!(
                "profile roots contain unsupported file: {}",
                item.path().display()
            );
        }
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("profile archive contains an empty path");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "profile archive path escapes destination: {}",
                    path.display()
                )
            }
        }
    }
    Ok(())
}

fn validate_link_target(path: &Path, target: &Path) -> Result<()> {
    if target.is_absolute() {
        bail!(
            "profile archive link target is absolute: {}",
            target.display()
        );
    }
    let mut components = path
        .parent()
        .map(Path::components)
        .into_iter()
        .flatten()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(PathBuf::from(value)),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>();
    for component in target.components() {
        match component {
            Component::Normal(value) => components.push(PathBuf::from(value)),
            Component::CurDir => {}
            Component::ParentDir => {
                if components.pop().is_none() {
                    bail!(
                        "profile archive link target escapes destination: {} -> {}",
                        path.display(),
                        target.display()
                    );
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!(
                    "profile archive link target is absolute: {}",
                    target.display()
                )
            }
        }
    }
    Ok(())
}

fn ensure_tzst_path(path: &Path) -> Result<()> {
    let extension = path.extension().and_then(|value| value.to_str());
    if extension != Some(PROFILE_ARCHIVE_EXTENSION) {
        bail!("profile archive must use .{PROFILE_ARCHIVE_EXTENSION} extension");
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format_sha256(hasher))
}

fn format_sha256(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn xxmi_game(mods: &Path) -> GameInstall {
        let mut game = crate::model::seeded_games()
            .into_iter()
            .find(|game| game.is_xxmi())
            .unwrap();
        game.mods_path_override = Some(mods.to_path_buf());
        game
    }

    #[test]
    fn xxmi_profile_paths_are_siblings_of_live_roots() {
        let root = PathBuf::from(r"C:\Games\Mods");
        let paths = profile_roots(&xxmi_game(&root), false).unwrap();
        assert_eq!(paths.profiles_dir, PathBuf::from(r"C:\Games\Mods_Profiles"));
        assert_eq!(
            paths.archived,
            Some(PathBuf::from(r"C:\Games\Mods_Archived"))
        );
        assert!(paths.disabled.is_none());
    }

    #[test]
    fn profile_storage_layout_creates_and_restores_readme() {
        let temp = tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let roots = ProfileRoots {
            profiles_dir: profiles_dir.clone(),
            mods: temp.path().join("Mods"),
            archived: Some(temp.path().join("Mods_Archived")),
            disabled: None,
        };

        ensure_profile_storage_layout(&roots).unwrap();
        let readme = profiles_dir.join(PROFILE_README_FILE);
        assert_eq!(fs::read_to_string(&readme).unwrap(), PROFILE_README_CONTENT);

        fs::write(&readme, "outdated").unwrap();
        ensure_profile_storage_layout(&roots).unwrap();
        assert_eq!(fs::read_to_string(readme).unwrap(), PROFILE_README_CONTENT);
    }

    #[test]
    fn archive_extract_roundtrip_preserves_metadata_and_files() {
        let temp = tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let archived = temp.path().join("Mods_Archived");
        fs::create_dir_all(mods.join("nested")).unwrap();
        fs::create_dir_all(&archived).unwrap();
        fs::write(mods.join("nested").join("one.bin"), b"one").unwrap();
        fs::write(archived.join("old.txt"), b"old").unwrap();
        let roots = ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods,
            archived: Some(archived),
            disabled: None,
        };
        let metadata = ProfileArchiveMetadata {
            format_version: PROFILE_ARCHIVE_FORMAT_VERSION,
            profile_id: Uuid::new_v4(),
            game_id: "test".to_string(),
            display_name: r#"<>:"/\|?* CON. "#.to_string(),
            backend: GameBackend::Xxmi,
            created_at: Utc::now(),
            uncompressed_size: 0,
            file_count: 0,
            portable_metadata: HashMap::from([(String::from("origin"), serde_json::json!("test"))]),
            categories: Some(vec![crate::model::ModCategory {
                id: "cat-1".to_string(),
                game_id: "test".to_string(),
                name: "Gameplay/Illegal?.txt".to_string(),
                order: 3,
            }]),
        };
        let destination = roots.profiles_dir.join("default.tzst");
        let result =
            create_profile_archive_with_progress(&roots, &metadata, &destination, None).unwrap();
        assert_eq!(result.archive_path, destination);
        assert_eq!(result.sha256.len(), 64);
        let manifest = read_profile_archive_metadata(&destination).unwrap();
        assert_eq!(manifest.profile_id, metadata.profile_id);
        assert_eq!(manifest.display_name, metadata.display_name);
        assert_eq!(manifest.categories, metadata.categories);
        assert_eq!(manifest.file_count, 2);
        assert_eq!(manifest.uncompressed_size, 6);
        assert_eq!(profile_archive_sha256(&destination).unwrap(), result.sha256);
        verify_profile_archive_sha256(&destination, &result.sha256).unwrap();
        let extracted = temp.path().join("extract");
        let mut read_updates = Vec::new();
        let mut read_progress = |update: ArchiveReadProgress| -> Result<()> {
            read_updates.push(update);
            Ok(())
        };
        let extracted_metadata = extract_profile_archive_verified_with_progress(
            &destination,
            &extracted,
            Some(&result.sha256),
            Some(&mut read_progress),
        )
        .unwrap();
        assert_eq!(extracted_metadata.profile_id, metadata.profile_id);
        assert_eq!(extracted_metadata.categories, metadata.categories);
        assert!(read_updates.len() >= 2);
        assert_eq!(read_updates.first().unwrap().bytes_read, 0);
        assert_eq!(
            read_updates.last().unwrap().bytes_read,
            extracted_metadata.uncompressed_size
        );
        assert_eq!(
            read_updates.last().unwrap().total_bytes,
            extracted_metadata.uncompressed_size
        );
        assert!(
            read_updates
                .windows(2)
                .all(|updates| updates[0].bytes_read <= updates[1].bytes_read)
        );
        assert_eq!(
            fs::read(extracted.join("Mods/nested/one.bin")).unwrap(),
            b"one"
        );
        assert_eq!(
            fs::read(extracted.join("Mods_Archived/old.txt")).unwrap(),
            b"old"
        );

        let checksum_error = extract_profile_archive_verified_with_progress(
            &destination,
            &temp.path().join("checksum-error"),
            Some(&"0".repeat(64)),
            None,
        )
        .unwrap_err();
        assert!(
            checksum_error
                .to_string()
                .contains("profile archive checksum mismatch")
        );

        let mut cancellation_observed = false;
        let mut cancel = |update: ArchiveReadProgress| -> Result<()> {
            if update.bytes_read != 0 {
                cancellation_observed = true;
                bail!("test extraction canceled");
            }
            Ok(())
        };
        assert!(
            extract_profile_archive_verified_with_progress(
                &destination,
                &temp.path().join("canceled-extract"),
                Some(&result.sha256),
                Some(&mut cancel),
            )
            .is_err()
        );
        assert!(cancellation_observed);
    }

    #[test]
    fn verified_extraction_reports_and_cancels_inside_a_large_file() {
        let temp = tempdir().unwrap();
        let mods = temp.path().join("Mods");
        fs::create_dir_all(&mods).unwrap();
        let payload = vec![0x5a; 2 * 1024 * 1024];
        fs::write(mods.join("large.bin"), &payload).unwrap();
        let roots = ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods,
            archived: None,
            disabled: None,
        };
        let metadata = ProfileArchiveMetadata {
            format_version: PROFILE_ARCHIVE_FORMAT_VERSION,
            profile_id: Uuid::new_v4(),
            game_id: "test".to_string(),
            display_name: "Large".to_string(),
            backend: GameBackend::Xxmi,
            created_at: Utc::now(),
            uncompressed_size: 0,
            file_count: 0,
            portable_metadata: HashMap::new(),
            categories: Some(Vec::new()),
        };
        let destination = roots.profiles_dir.join("large.tzst");
        let result =
            create_profile_archive_with_progress(&roots, &metadata, &destination, None).unwrap();

        let mut updates = Vec::new();
        let mut progress = |update: ArchiveReadProgress| -> Result<()> {
            updates.push(update);
            Ok(())
        };
        extract_profile_archive_verified_with_progress(
            &destination,
            &temp.path().join("large-extract"),
            Some(&result.sha256),
            Some(&mut progress),
        )
        .unwrap();
        assert!(updates.iter().any(|update| {
            update.total_bytes == payload.len() as u64
                && update.bytes_read > 0
                && update.bytes_read < update.total_bytes
        }));

        let mut canceled_inside_payload = false;
        let mut cancel = |update: ArchiveReadProgress| -> Result<()> {
            if update.total_bytes != 0 && update.bytes_read >= update.total_bytes / 4 {
                canceled_inside_payload = true;
                bail!("test extraction canceled inside payload");
            }
            Ok(())
        };
        assert!(
            extract_profile_archive_verified_with_progress(
                &destination,
                &temp.path().join("large-canceled"),
                Some(&result.sha256),
                Some(&mut cancel),
            )
            .is_err()
        );
        assert!(canceled_inside_payload);
    }

    #[test]
    fn extraction_rejects_traversal_and_unsafe_links() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("bad.tzst");
        let file = File::create(&archive_path).unwrap();
        let mut encoder = Encoder::new(file, 1).unwrap();
        let mut builder = Builder::new(&mut encoder);
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Regular);
        header.set_mode(0o644);
        header.set_size(1);
        header.as_mut_bytes()[..9].copy_from_slice(b"../escape");
        header.set_cksum();
        builder.append(&header, &b"x"[..]).unwrap();
        drop(builder);
        encoder.finish().unwrap();
        let error = extract_profile_archive(&archive_path, &temp.path().join("extract"));
        assert!(error.is_err());

        let link_archive_path = temp.path().join("bad-link.tzst");
        let file = File::create(&link_archive_path).unwrap();
        let mut encoder = Encoder::new(file, 1).unwrap();
        let mut builder = Builder::new(&mut encoder);
        let mut link = Header::new_gnu();
        link.set_entry_type(EntryType::Symlink);
        link.set_mode(0o777);
        link.as_mut_bytes()[..9].copy_from_slice(b"Mods/link");
        link.set_link_name("../escape").unwrap();
        link.set_cksum();
        builder.append(&link, &b""[..]).unwrap();
        drop(builder);
        encoder.finish().unwrap();
        let error = extract_profile_archive(&link_archive_path, &temp.path().join("link-extract"));
        assert!(error.is_err());
    }
}
