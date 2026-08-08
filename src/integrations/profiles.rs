//! Per-game profile storage and `.tzst` archive primitives.
//!
//! The active profile is represented by the live game directories. Inactive profiles normally
//! live in tar+zstd archives below a backend-specific `Mods_Profiles` directory; a
//! `<uuid>.profile` container is the crash-safe authoritative copy while background compression
//! is pending. This module deliberately contains no UI or switching policy; callers can compose
//! the primitives into a transactional switch operation.

use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tar::{Archive, Builder, EntryType, Header};
use xxhash_rust::xxh3::xxh3_64;
use zstd::stream::{read::Decoder, write::Encoder};

use crate::model::{
    GameBackend, GameInstall, MODS_PROFILES_DIR, ModCategory, ProfileId, ToolEntry,
};

pub const PROFILE_ARCHIVE_EXTENSION: &str = "tzst";
pub const PROFILE_ARCHIVE_FORMAT_VERSION: u32 = 1;
pub const PROFILE_METADATA_FILE: &str = "profile.json";
pub const PROFILE_README_FILE: &str = "readme.txt";
pub const PROFILE_STAGING_DIR: &str = ".staging";
pub const PROFILE_ARCHIVE_RESERVED_ARCHIVED_ROOT: &str = "Archived";
// Written verbatim into the profile storage folder, so it is the only explanation a user gets
// while browsing there. Keep it in step with the actual layout: the naming lines change when
// storage names gain readable labels.
pub const PROFILE_README_CONTENT: &str = "\
Hestia Profile Storage

This folder stores inactive Hestia profiles. The active profile is not kept here - it stays in the game's normal Mods folder.

WHAT YOU MAY SEE

<name> [<id>].tzst
A compressed inactive profile. It is a TAR archive compressed with Zstandard. The name is a label for you; the short id in brackets is what Hestia matches on.

<name> [<id>]
An uncompressed profile. Hestia keeps this folder while switching profiles, and until a pending compression finishes. Seeing one next to the matching .tzst is normal, and Hestia removes it once the archive is complete.

readme.txt
This file.

.extracting, .journal, .part, .bak, .conflict-*, and .deleting entries
Files and folders left behind by an interrupted operation. Hestia finishes or rolls back extracting folders, journals, sidecars, and deleting folders on the next launch. Redundant conflict archives are removed when Hestia can prove they match the live archive; other conflict archives are preserved for manual review.

OPENING AN ARCHIVE

You can inspect .tzst archives with:
- Windows 11 File Explorer with current system updates
- 7-Zip 24.05 or newer
- WinRAR 6.21 or newer

IMPORTANT

Renaming an inactive profile in Hestia renames its files here too. Renaming an inactive profile yourself changes the display name Hestia restores on the next launch. Hestia identifies it by the id in brackets, so do not change the bracketed id.

Dropping a .tzst profile from another install into this folder is enough for Hestia to pick it up. Its mods and categories are restored; its tools are not, and are detected fresh from its own mods folder instead.

Do not edit, move, or delete anything in this folder while Hestia is running. Manage profiles through Hestia whenever possible.

If Hestia or Windows closes during profile switching or compression, reopen Hestia and allow it to recover the profile data. Do not remove the temporary entries yourself first - Hestia needs them to finish or roll back what was interrupted.
";

/// Length of the short profile id embedded in storage names.
pub const PROFILE_SHORT_ID_LEN: usize = 8;
/// Longest label allowed in a storage name, ellipsis included.
pub const PROFILE_LABEL_MAX_LEN: usize = 24;
/// Used when a profile name sanitizes away to nothing, so a storage name never starts with the
/// marker's leading space.
const PROFILE_LABEL_FALLBACK: &str = "Profile";
/// Single character, deliberately not `...`: Windows silently strips trailing dots, so an ASCII
/// ellipsis would be eaten by the filesystem and the label would look arbitrarily cut instead of
/// visibly shortened.
const PROFILE_LABEL_ELLIPSIS: char = '…';

/// Invisible or direction-controlling characters. They are legal in filenames but let a stored
/// name render as something other than what it is, so they are dropped rather than substituted —
/// substituting would turn an invisible character into visible noise.
fn is_invisible_or_bidi(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'                  // soft hyphen
            | '\u{200B}'..='\u{200F}' // zero-width spaces and joiners, LRM/RLM
            | '\u{202A}'..='\u{202E}' // bidi embedding and override
            | '\u{2066}'..='\u{2069}' // bidi isolates
            | '\u{FEFF}' // zero-width no-break space
    )
}

/// The id as it appears in storage names. The id *is* the eight hex digits, so this is only a
/// rendering, not a truncation.
pub fn profile_short_id(profile_id: ProfileId) -> String {
    profile_id.to_string()
}

/// Reduce a profile name to a filesystem-safe label.
///
/// This is the user-visible storage label for an inactive profile. It can become the recovered
/// display name when app state is rebuilt, so lossy substitution is kept to the minimum needed for a
/// stable filename. Names are user-supplied and localized, so only genuinely unsafe characters are
/// touched: the set Windows rejects (a superset of what Linux and macOS reject), control
/// characters, and invisible or bidi-control characters that would let a stored name render as
/// something it is not.
///
/// Reserved device names (`CON`, `NUL`, `COM1`…) need no special handling because the ` [id]`
/// marker is always appended, so the stem is never exactly a reserved name.
pub fn sanitize_profile_label(name: &str) -> String {
    let mut label = String::with_capacity(name.len());
    for character in name.chars() {
        match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => label.push('_'),
            character if character.is_control() => label.push('_'),
            character if is_invisible_or_bidi(character) => {}
            character => label.push(character),
        }
    }
    // Windows silently drops trailing dots and spaces, which would make the stored label disagree
    // with what was written.
    let trim = |value: &str| value.trim_start().trim_end_matches(['.', ' ']).to_string();
    let label = trim(&label);
    if label.is_empty() {
        return PROFILE_LABEL_FALLBACK.to_string();
    }
    if label.chars().count() <= PROFILE_LABEL_MAX_LEN {
        return label;
    }
    // Count the ellipsis against the budget so the label never exceeds the cap, and take whole
    // characters so a multi-byte name is never split mid-character.
    let mut clamped: String = label
        .chars()
        .take(PROFILE_LABEL_MAX_LEN.saturating_sub(1))
        .collect();
    clamped = trim(&clamped);
    if clamped.is_empty() {
        return PROFILE_LABEL_FALLBACK.to_string();
    }
    clamped.push(PROFILE_LABEL_ELLIPSIS);
    clamped
}

/// Storage stem for a profile: `<label> [<short id>]`.
pub fn profile_storage_stem(display_name: &str, profile_id: ProfileId) -> String {
    format!(
        "{} [{}]",
        sanitize_profile_label(display_name),
        profile_short_id(profile_id)
    )
}

/// Split a storage stem into its label and short id.
///
/// Anchored at the right across a fixed-width marker, so a label containing brackets or spaces
/// round-trips: `Weird [name] [1fe9ec7a]` yields `("Weird [name]", "1fe9ec7a")`. The label is
/// returned for display and staleness checks only — it is never an identity. Short ids compare
/// case-insensitively, since a hand-renamed file is still the same profile.
pub fn parse_profile_storage_stem(stem: &str) -> Option<(&str, &str)> {
    let marker_len = PROFILE_SHORT_ID_LEN + 3; // " [" + id + "]"
    let boundary = stem.len().checked_sub(marker_len)?;
    if !stem.is_char_boundary(boundary) {
        return None;
    }
    let (label, marker) = stem.split_at(boundary);
    let short_id = marker
        .strip_prefix(" [")
        .and_then(|marker| marker.strip_suffix(']'))?;
    if short_id.len() != PROFILE_SHORT_ID_LEN
        || !short_id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return None;
    }
    (!label.is_empty()).then_some((label, short_id))
}

/// Whether a stem's short id refers to this profile. Case-insensitive so a file renamed in
/// Explorer still resolves.
pub fn profile_storage_stem_matches(stem: &str, profile_id: ProfileId) -> bool {
    parse_profile_storage_stem(stem)
        .is_some_and(|(_, short_id)| short_id.eq_ignore_ascii_case(&profile_short_id(profile_id)))
}

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

    /// Loose profile container path (`<label> [<short id>]`).
    ///
    /// Resolves to whatever is already on disk for this profile, whatever its label, so a folder
    /// renamed by hand still round-trips. Falls back to the canonical name when nothing exists yet,
    /// which is what a caller creating a container wants.
    pub fn profile_path(&self, profile_id: ProfileId) -> PathBuf {
        self.find_profile_storage(profile_id, false)
            .unwrap_or_else(|| self.profile_path_for(profile_id, PROFILE_LABEL_FALLBACK))
    }

    /// Compressed profile archive path (`<label> [<short id>].tzst`), resolved like
    /// [`Self::profile_path`].
    pub fn archive_path(&self, profile_id: ProfileId) -> PathBuf {
        self.find_profile_storage(profile_id, true)
            .unwrap_or_else(|| self.archive_path_for(profile_id, PROFILE_LABEL_FALLBACK))
    }

    /// Canonical container path for a profile carrying this display name.
    pub fn profile_path_for(&self, profile_id: ProfileId, display_name: &str) -> PathBuf {
        self.profiles_dir
            .join(profile_storage_stem(display_name, profile_id))
    }

    /// Canonical archive path for a profile carrying this display name.
    pub fn archive_path_for(&self, profile_id: ProfileId, display_name: &str) -> PathBuf {
        self.profiles_dir.join(format!(
            "{}.{PROFILE_ARCHIVE_EXTENSION}",
            profile_storage_stem(display_name, profile_id)
        ))
    }

    /// Existing storage for a profile, matched on the short id rather than the label.
    ///
    /// The label is only ever decoration, so it is never part of the match — the authoritative
    /// identity lives in the entry's own `profile.json`.
    pub fn find_profile_storage(&self, profile_id: ProfileId, archive: bool) -> Option<PathBuf> {
        let entries = fs::read_dir(&self.profiles_dir).ok()?;
        let mut best: Option<PathBuf> = None;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some(stem) = (if archive {
                name.strip_suffix(&format!(".{PROFILE_ARCHIVE_EXTENSION}"))
            } else {
                Some(name.as_ref())
            }) else {
                continue;
            };
            if !profile_storage_stem_matches(stem, profile_id) {
                continue;
            }
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) == archive {
                continue;
            }
            // Deterministic when a hand-made copy leaves two candidates, so repeated runs agree.
            if best.as_ref().is_none_or(|current| entry.path() < *current) {
                best = Some(entry.path());
            }
        }
        best
    }

    /// Sibling path used while atomically writing a profile archive.
    pub fn archive_part_path(&self, profile_id: ProfileId) -> PathBuf {
        archive_sidecar_path(&self.archive_path(profile_id), "part")
    }

    /// Sibling backup path used while atomically replacing a profile archive.
    pub fn archive_backup_path(&self, profile_id: ProfileId) -> PathBuf {
        archive_sidecar_path(&self.archive_path(profile_id), "bak")
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
    profile_id: ProfileId,
) -> Result<PathBuf> {
    Ok(profile_roots(game, use_default_mods_path)?.archive_path(profile_id))
}

pub fn profile_path(
    game: &GameInstall,
    use_default_mods_path: bool,
    profile_id: ProfileId,
) -> Result<PathBuf> {
    Ok(profile_roots(game, use_default_mods_path)?.profile_path(profile_id))
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
    pub profile_id: ProfileId,
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
    /// Profile-owned tools with their launch options and pins. `None` preserves compatibility
    /// with archives written before tools became profile-scoped; `Some(vec![])` is intentionally
    /// empty.
    #[serde(default)]
    pub tools: Option<Vec<ToolEntry>>,
    /// Profile-owned blacklist of auto-detected tool paths the user removed.
    #[serde(default)]
    pub tool_blacklist: Option<Vec<String>>,
    /// Fingerprint of the source roots used to create the archive. Older archives omit it.
    #[serde(default)]
    pub source_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveResult {
    pub archive_path: PathBuf,
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

/// Create a profile archive atomically through a sibling `.profile.tzst.part` file.
#[cfg(test)]
pub fn create_profile_archive_with_progress(
    roots: &ProfileRoots,
    metadata: &ProfileArchiveMetadata,
    destination: &Path,
    progress: Option<ProgressCallback<'_>>,
) -> Result<ArchiveResult> {
    ensure_tzst_path(destination)?;
    if metadata.format_version == 0 {
        bail!("profile archive format version must be non-zero");
    }
    let inventory = inspect_profile_source(roots)?;
    create_profile_archive_from_inventory_with_progress(&inventory, metadata, destination, progress)
}

/// Create an archive from a previously inspected source inventory without walking the source
/// roots again. The inventory can be reused for fingerprint/reuse decisions before writing.
pub fn create_profile_archive_from_inventory_with_progress(
    inventory: &ProfileSourceInventory,
    metadata: &ProfileArchiveMetadata,
    destination: &Path,
    mut progress: Option<ProgressCallback<'_>>,
) -> Result<ArchiveResult> {
    ensure_tzst_path(destination)?;
    if metadata.format_version == 0 {
        bail!("profile archive format version must be non-zero");
    }
    let stats = &inventory.stats;
    let mut archive_metadata = metadata.clone();
    archive_metadata.uncompressed_size = stats.bytes;
    archive_metadata.file_count = stats.files;
    archive_metadata.source_fingerprint = Some(inventory.fingerprint.clone());
    if let Some(callback) = progress.as_deref_mut() {
        callback(ArchiveProgress {
            total_files: stats.files,
            total_bytes: stats.bytes,
            ..ArchiveProgress::default()
        })?;
    }
    let shared_progress = progress
        .take()
        .map(|callback| Rc::new(RefCell::new(callback)));

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
    // Keep compression single-core for background profile operations. Level -1 is zstd's
    // fast mode 1, preserving the previous throughput-oriented compression setting.
    encoder
        .multithread(1)
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
    append_inventory(
        &mut builder,
        &inventory.mods,
        &mut processed,
        shared_progress.as_ref(),
    )?;
    if let Some(inventory) = &inventory.archived {
        append_inventory(
            &mut builder,
            inventory,
            &mut processed,
            shared_progress.as_ref(),
        )?;
    }
    if let Some(inventory) = &inventory.disabled {
        append_inventory(
            &mut builder,
            inventory,
            &mut processed,
            shared_progress.as_ref(),
        )?;
        append_empty_dir(&mut builder, PROFILE_ARCHIVE_RESERVED_ARCHIVED_ROOT)?;
    }
    let encoder = builder
        .into_inner()
        .context("failed to finalize tar stream")?;
    let mut file = encoder.finish().context("failed to finalize zstd stream")?;
    file.flush()?;
    file.sync_all()
        .context("failed to sync completed profile archive")?;
    drop(file);

    let bytes = fs::metadata(&staging)?.len();
    commit_archive_part(&staging, destination)?;
    Ok(ArchiveResult {
        archive_path: destination.to_path_buf(),
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
    read_progress: Option<ReadProgressCallback<'_>>,
) -> Result<ProfileArchiveMetadata> {
    extract_profile_archive_impl(archive_path, destination, read_progress)
}

fn extract_profile_archive_impl(
    archive_path: &Path,
    destination: &Path,
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

    let decoder = Decoder::new(File::open(archive_path)?)?;
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
            metadata = Some(parsed);
            // Keep profile.json in the extracted loose container. It is part of the container's
            // self-contained metadata even though it is excluded from payload counts.
            fs::write(destination.join(PROFILE_METADATA_FILE), &bytes)
                .with_context(|| "failed to materialize extracted profile.json")?;
            continue;
        }
        entry
            .unpack_in(destination)
            .with_context(|| format!("failed to extract profile entry {}", path.display()))?;
        if entry_type.is_file() {
            processed.files_processed += 1;
            processed.bytes_processed += size;
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
    // Finishing the decoder forces zstd to verify its frame checksum and trailing input.
    let _ = decoder.finish();
    Ok(metadata)
}

/// Read the embedded profile manifest without materializing payload files. Full entry validation
/// is performed by extraction.
/// Rewrite an archive under a new profile identity, streaming entry by entry.
///
/// Used to split a duplicated profile into a genuinely separate one. The identity lives in
/// `profile.json` at the head of the tar, and the whole thing is zstd-compressed, so it cannot be
/// patched in place — but streaming avoids ever materializing the uncompressed tree on disk, which
/// for a large profile is the difference between needing a few megabytes and needing gigabytes.
///
/// `progress` receives bytes copied so far against the source's recorded uncompressed size;
/// returning an error from it aborts and leaves the destination untouched.
pub fn reidentify_profile_archive(
    source: &Path,
    destination: &Path,
    new_profile_id: ProfileId,
    new_display_name: &str,
    mut progress: Option<&mut dyn FnMut(u64, u64) -> Result<()>>,
) -> Result<ProfileArchiveMetadata> {
    ensure_tzst_path(source)?;
    ensure_tzst_path(destination)?;
    let mut metadata = read_profile_archive_metadata(source)?;
    let total = metadata.uncompressed_size;
    metadata.profile_id = new_profile_id;
    metadata.display_name = new_display_name.to_string();
    // The copy is a distinct profile from this point, so it cannot claim the source's identity or
    // pretend its bytes were produced from the live roots.
    metadata.created_at = Utc::now();
    metadata.source_fingerprint = None;

    let parent = destination
        .parent()
        .context("profile archive destination has no parent")?;
    fs::create_dir_all(parent)?;
    let staging = archive_sidecar_path(destination, "part");
    if staging.exists() {
        fs::remove_file(&staging)?;
    }

    let result = (|| -> Result<()> {
        let file = File::create(&staging)
            .with_context(|| format!("failed to create {}", staging.display()))?;
        let mut encoder = Encoder::new(file, -1).context("failed to initialize zstd encoder")?;
        encoder.multithread(1)?;
        encoder.include_checksum(true)?;
        let mut builder = Builder::new(encoder);
        builder.follow_symlinks(false);
        append_json(&mut builder, PROFILE_METADATA_FILE, &metadata)?;

        let mut decoder = Decoder::new(File::open(source)?)?;
        let mut archive = Archive::new(&mut decoder);
        let mut copied = 0u64;
        for item in archive.entries()? {
            let mut entry = item?;
            let path = entry.path()?.into_owned();
            validate_relative_path(&path)?;
            // The replacement metadata is already written; skip the original.
            if path == Path::new(PROFILE_METADATA_FILE) {
                continue;
            }
            let mut header = entry.header().clone();
            header.set_cksum();
            let size = header.size().unwrap_or(0);
            let mut bytes = Vec::with_capacity(size.min(8 * 1024 * 1024) as usize);
            entry.read_to_end(&mut bytes)?;
            builder.append_data(&mut header, &path, bytes.as_slice())?;
            copied = copied.saturating_add(bytes.len() as u64);
            if let Some(progress) = progress.as_deref_mut() {
                progress(copied, total)?;
            }
        }
        builder.into_inner()?.finish()?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    fs::rename(&staging, destination)
        .with_context(|| format!("failed to finalize {}", destination.display()))?;
    Ok(metadata)
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceEntryKind {
    Directory,
    File,
}

#[derive(Debug)]
struct SourceEntry {
    source_path: PathBuf,
    archive_path: PathBuf,
    kind: SourceEntryKind,
    size: u64,
    modified: Option<(u64, u32)>,
}

#[derive(Debug, Default)]
struct SourceInventory {
    present: bool,
    archive_name: String,
    entries: Vec<SourceEntry>,
    files: u64,
    bytes: u64,
}

/// Opaque, validated inventory of all source roots that will be written to a profile archive.
/// The inventory owns no source files; it retains paths and metadata for reuse by callers.
#[derive(Debug)]
pub struct ProfileSourceInventory {
    mods: SourceInventory,
    archived: Option<SourceInventory>,
    disabled: Option<SourceInventory>,
    stats: SourceStats,
    fingerprint: String,
}

impl ProfileSourceInventory {
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

#[derive(Debug, Default)]
struct SourceStats {
    files: u64,
    bytes: u64,
}

const ARCHIVE_PROGRESS_CHUNK_SIZE: usize = 64 * 1024;

struct ArchiveSourceReader<'progress, 'callback> {
    file: File,
    progress: &'progress mut ArchiveProgress,
    callback: Option<Rc<RefCell<ProgressCallback<'callback>>>>,
}

impl Read for ArchiveSourceReader<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let limit = buffer.len().min(ARCHIVE_PROGRESS_CHUNK_SIZE);
        let read = self.file.read(&mut buffer[..limit])?;
        if read != 0 {
            self.progress.bytes_processed =
                self.progress.bytes_processed.saturating_add(read as u64);
            if let Some(callback) = &self.callback {
                callback.borrow_mut()(*self.progress)
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
        }
        Ok(read)
    }
}

impl SourceStats {
    fn add(&mut self, inventory: &SourceInventory) {
        self.files = self.files.saturating_add(inventory.files);
        self.bytes = self.bytes.saturating_add(inventory.bytes);
    }
}

/// Validate and inventory one live root. The resulting entries are reused for archive writing,
/// so source paths/types are not walked a second time.
fn inventory_source_tree(path: &Path, archive_name: &str) -> Result<SourceInventory> {
    let root_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SourceInventory {
                archive_name: archive_name.to_string(),
                ..SourceInventory::default()
            });
        }
        Err(error) => return Err(error.into()),
    };
    if root_metadata.file_type().is_symlink() {
        bail!("profile roots cannot contain symlinks: {}", path.display());
    }
    if !root_metadata.is_dir() {
        bail!("profile root is not a directory: {}", path.display());
    }

    let mut inventory = SourceInventory {
        present: true,
        archive_name: archive_name.to_string(),
        ..SourceInventory::default()
    };
    for item in walkdir::WalkDir::new(path).follow_links(false) {
        let item = item?;
        let file_type = item.file_type();
        if file_type.is_symlink() {
            bail!(
                "profile roots cannot contain symlinks: {}",
                item.path().display()
            );
        }
        let relative = item.path().strip_prefix(path)?;
        let archive_path = if relative.as_os_str().is_empty() {
            PathBuf::from(archive_name)
        } else {
            PathBuf::from(archive_name).join(relative)
        };
        let source_metadata = item.metadata()?;
        let (kind, size) = if file_type.is_dir() {
            (SourceEntryKind::Directory, 0)
        } else if file_type.is_file() {
            let size = source_metadata.len();
            inventory.files = inventory.files.saturating_add(1);
            inventory.bytes = inventory.bytes.saturating_add(size);
            (SourceEntryKind::File, size)
        } else {
            bail!(
                "profile roots contain unsupported file: {}",
                item.path().display()
            );
        };
        // Windows can update directory mtimes lazily after child creation. Directory mtimes are
        // not payload content, so only regular-file mtimes participate in the stable fingerprint.
        let modified = (kind == SourceEntryKind::File)
            .then(|| source_metadata.modified().ok())
            .flatten()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| (duration.as_secs(), duration.subsec_nanos()));
        inventory.entries.push(SourceEntry {
            source_path: item.path().to_path_buf(),
            archive_path,
            kind,
            size,
            modified,
        });
    }
    Ok(inventory)
}

fn source_fingerprint_for_inventories(
    mods: &SourceInventory,
    archived: Option<&SourceInventory>,
    disabled: Option<&SourceInventory>,
) -> String {
    let mut records = Vec::new();
    append_fingerprint_records(&mut records, mods);
    if let Some(inventory) = archived {
        append_fingerprint_records(&mut records, inventory);
    }
    if let Some(inventory) = disabled {
        append_fingerprint_records(&mut records, inventory);
    }
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend_from_slice(record.as_bytes());
        bytes.push(b'\n');
    }
    format!("{:016x}", xxh3_64(&bytes))
}

fn append_fingerprint_records(records: &mut Vec<String>, inventory: &SourceInventory) {
    records.push(format!(
        "root\0{}\0{}",
        inventory.archive_name, inventory.present
    ));
    let mut entries = inventory
        .entries
        .iter()
        .map(|entry| {
            let kind = match entry.kind {
                SourceEntryKind::Directory => 'd',
                SourceEntryKind::File => 'f',
            };
            let modified = entry.modified.map_or_else(
                || "-".to_string(),
                |(seconds, nanos)| format!("{seconds}:{nanos}"),
            );
            let path = entry.archive_path.to_string_lossy().replace('\\', "/");
            format!("entry\0{kind}\0{path}\0{}\0{modified}", entry.size)
        })
        .collect::<Vec<_>>();
    entries.sort_unstable();
    records.extend(entries);
}

/// Compute the stable filesystem fingerprint used in profile archive metadata.
///
/// A `None` fingerprint in an older archive means callers cannot safely skip recompression.
pub fn inspect_profile_source(roots: &ProfileRoots) -> Result<ProfileSourceInventory> {
    let mods = inventory_source_tree(&roots.mods, "Mods")?;
    let archived = roots
        .archived
        .as_deref()
        .map(|path| inventory_source_tree(path, "Mods_Archived"))
        .transpose()?;
    let disabled = roots
        .disabled
        .as_deref()
        .map(|path| inventory_source_tree(path, "Disabled"))
        .transpose()?;
    let mut stats = SourceStats::default();
    stats.add(&mods);
    if let Some(inventory) = &archived {
        stats.add(inventory);
    }
    if let Some(inventory) = &disabled {
        stats.add(inventory);
    }
    let fingerprint =
        source_fingerprint_for_inventories(&mods, archived.as_ref(), disabled.as_ref());
    Ok(ProfileSourceInventory {
        mods,
        archived,
        disabled,
        stats,
        fingerprint,
    })
}

#[cfg(test)]
pub fn profile_source_fingerprint(roots: &ProfileRoots) -> Result<String> {
    Ok(inspect_profile_source(roots)?.fingerprint().to_owned())
}

fn append_inventory<'a, W: Write>(
    builder: &mut Builder<W>,
    inventory: &SourceInventory,
    progress: &mut ArchiveProgress,
    callback: Option<&Rc<RefCell<ProgressCallback<'a>>>>,
) -> Result<()> {
    if !inventory.present {
        // Missing optional roots are represented in the archive as empty directories for
        // backward-compatible extraction layout.
        append_empty_dir(builder, &inventory.archive_name)?;
        return Ok(());
    }
    for entry in &inventory.entries {
        match entry.kind {
            SourceEntryKind::Directory => {
                builder.append_dir(&entry.archive_path, &entry.source_path)?
            }
            SourceEntryKind::File => {
                let metadata = fs::metadata(&entry.source_path)?;
                if !metadata.is_file() {
                    bail!(
                        "profile source changed from a file: {}",
                        entry.source_path.display()
                    );
                }
                let mut header = Header::new_gnu();
                header.set_metadata(&metadata);
                header.set_entry_type(EntryType::Regular);
                header.set_size(entry.size);
                let mut reader = ArchiveSourceReader {
                    file: File::open(&entry.source_path)?,
                    progress,
                    callback: callback.cloned(),
                };
                builder.append_data(&mut header, &entry.archive_path, &mut reader)?;
                progress.files_processed += 1;
                if let Some(callback) = callback {
                    callback.borrow_mut()(*progress)?;
                }
            }
        }
    }
    Ok(())
}

fn commit_archive_part(staging: &Path, destination: &Path) -> Result<()> {
    let backup = archive_sidecar_path(destination, "bak");
    if backup.exists() {
        fs::rename(&backup, next_archive_conflict_path(destination)).with_context(|| {
            format!(
                "failed to preserve existing profile archive backup {}",
                backup.display()
            )
        })?;
    }
    let mut had_existing = destination.is_file();
    if destination.exists() && !had_existing {
        fs::rename(destination, next_archive_conflict_path(destination)).with_context(|| {
            format!(
                "failed to preserve profile archive collision {}",
                destination.display()
            )
        })?;
        had_existing = false;
    }
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

fn next_archive_conflict_path(archive: &Path) -> PathBuf {
    let parent = archive.parent().unwrap_or_else(|| Path::new("."));
    let file_name = archive
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("profile.tzst");
    let stem = file_name.strip_suffix(".tzst").unwrap_or(file_name);
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    for suffix in 0u32.. {
        let candidate = if suffix == 0 {
            parent.join(format!("{stem}.conflict-{nonce}.tzst"))
        } else {
            parent.join(format!("{stem}.conflict-{nonce}-{suffix}.tzst"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn archive_sidecar_path(archive: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = archive.to_path_buf();
    let extension = archive
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or(PROFILE_ARCHIVE_EXTENSION);
    sidecar.set_extension(format!("{extension}.{suffix}"));
    sidecar
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

#[cfg(test)]
mod storage_name_tests {
    use super::*;

    fn id(hex: &str) -> ProfileId {
        hex.parse().unwrap()
    }

    #[test]
    fn stem_round_trips_names_containing_brackets_and_spaces() {
        let profile_id = id("1fe9ec7a");
        for name in [
            "Patch 1.4",
            "Weird [name]",
            "Patch 1.4 - final",
            "配置文件",
            "Профиль",
        ] {
            let stem = profile_storage_stem(name, profile_id);
            assert_eq!(
                parse_profile_storage_stem(&stem),
                Some((name, "1fe9ec7a")),
                "{stem} must parse back to its name"
            );
        }
    }

    #[test]
    fn a_name_that_looks_like_a_marker_does_not_confuse_the_parser() {
        // The right anchor must win, so the trailing marker is the one that counts.
        let stem = profile_storage_stem("Backup [deadbeef]", id("1fe9ec7a"));
        assert_eq!(stem, "Backup [deadbeef] [1fe9ec7a]");
        assert_eq!(
            parse_profile_storage_stem(&stem),
            Some(("Backup [deadbeef]", "1fe9ec7a"))
        );
    }

    #[test]
    fn short_ids_compare_case_insensitively() {
        let profile_id: ProfileId = "1fe9ec7a".parse().unwrap();

        assert_eq!(
            parse_profile_storage_stem("Patch 1.4 [1FE9EC7A]"),
            Some(("Patch 1.4", "1FE9EC7A")),
            "a file renamed in Explorer is still the same profile"
        );
        assert!(profile_storage_stem_matches(
            "Patch 1.4 [1FE9EC7A]",
            profile_id
        ));
        assert!(profile_storage_stem_matches(
            "Renamed [1fe9ec7a]",
            profile_id
        ));
        assert!(!profile_storage_stem_matches(
            "Patch 1.4 [7a15244d]",
            profile_id
        ));
    }

    #[test]
    fn a_long_label_is_clamped_with_an_ellipsis_within_the_cap() {
        let label = sanitize_profile_label("Endfield Patch 1.4 Operators Everything");

        assert_eq!(label.chars().count(), PROFILE_LABEL_MAX_LEN);
        assert!(label.ends_with('…'), "truncation must be visible: {label}");
        assert!(
            !label.contains("..."),
            "an ASCII ellipsis would be stripped by Windows as trailing dots"
        );
        let stem = profile_storage_stem("Endfield Patch 1.4 Operators Everything", id("1fe9ec7a"));
        assert_eq!(
            parse_profile_storage_stem(&stem).map(|(_, short)| short),
            Some("1fe9ec7a"),
            "a clamped label must still round-trip its id"
        );
    }

    #[test]
    fn invisible_and_bidi_characters_are_dropped_not_substituted() {
        assert_eq!(sanitize_profile_label("Pa\u{200B}tch"), "Patch");
        assert_eq!(sanitize_profile_label("\u{202E}gnp.exe"), "gnp.exe");
        assert_eq!(sanitize_profile_label("soft\u{00AD}hyphen"), "softhyphen");
    }

    #[test]
    fn quarantine_and_sidecar_stems_are_rejected() {
        for stem in [
            "Patch 1.4 [1fe9ec7a].conflict-000000000000007b",
            "Patch 1.4 [1fe9ec7a].deleting",
            "Patch 1.4 [1fe9ec7a] ",
            "Patch 1.4 [1fe9ec7]",
            "Patch 1.4 [1fe9ec7ag]",
            "Patch 1.4[1fe9ec7a]",
            "[1fe9ec7a]",
            " [1fe9ec7a]",
            ".staging",
            "readme.txt",
            "1fe9ec7a-6ad1-46b6-9558-e8483dc62bcf.profile",
        ] {
            assert_eq!(
                parse_profile_storage_stem(stem),
                None,
                "{stem} must not be mistaken for live profile data"
            );
        }
    }

    #[test]
    fn sanitizing_replaces_only_what_the_filesystem_rejects() {
        assert_eq!(sanitize_profile_label("a/b\\c:d*e?f"), "a_b_c_d_e_f");
        assert_eq!(sanitize_profile_label("tab\there"), "tab_here");
        assert_eq!(
            sanitize_profile_label("配置文件 v2"),
            "配置文件 v2",
            "non-ASCII names are valid on NTFS and must survive untouched"
        );
        assert_eq!(
            sanitize_profile_label("trailing... "),
            "trailing",
            "Windows silently drops trailing dots and spaces"
        );
        assert_eq!(
            sanitize_profile_label("///"),
            "___",
            "an all-illegal name still yields a usable, distinguishable stem"
        );
        // The fallback is only for names that reduce to nothing at all.
        assert_eq!(sanitize_profile_label("  "), "Profile");
        assert_eq!(sanitize_profile_label(""), "Profile");
        assert_eq!(sanitize_profile_label("..."), "Profile");
    }

    #[test]
    fn long_and_multibyte_names_truncate_on_character_boundaries() {
        let long = "配".repeat(200);
        let label = sanitize_profile_label(&long);

        assert_eq!(label.chars().count(), PROFILE_LABEL_MAX_LEN);
        let mut characters: Vec<char> = label.chars().collect();
        assert_eq!(characters.pop(), Some(PROFILE_LABEL_ELLIPSIS));
        assert!(
            characters.iter().all(|character| *character == '配'),
            "clamping must land on character boundaries, never mid-character: {label}"
        );
        assert_eq!(
            label.len(),
            characters.len() * '配'.len_utf8() + PROFILE_LABEL_ELLIPSIS.len_utf8(),
            "the byte length must account for whole characters only"
        );
        let stem = profile_storage_stem(&long, id("1fe9ec7a"));
        assert_eq!(
            parse_profile_storage_stem(&stem).map(|(_, short)| short),
            Some("1fe9ec7a")
        );
    }

    #[test]
    fn a_reserved_device_name_is_neutralized_by_the_marker() {
        // "CON" alone is unusable on Windows; "CON [1fe9ec7a]" is not.
        let stem = profile_storage_stem("CON", id("1fe9ec7a"));
        assert_eq!(stem, "CON [1fe9ec7a]");
        assert_eq!(parse_profile_storage_stem(&stem), Some(("CON", "1fe9ec7a")));
    }

    #[test]
    fn short_id_is_the_lowercase_hex_prefix_of_the_uuid() {
        let profile_id: ProfileId = "1fe9ec7a".parse().unwrap();

        assert_eq!(profile_short_id(profile_id), "1fe9ec7a");
        assert_eq!(
            profile_storage_stem("Patch 1.4", profile_id),
            "Patch 1.4 [1fe9ec7a]"
        );
    }
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
    fn profile_paths_use_canonical_container_and_archive_names() {
        let temp = tempdir().unwrap();
        let roots = ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        let profile_id: ProfileId = "00000000".parse().unwrap();
        // With nothing on disk, the resolver falls back to the canonical shape.
        assert_eq!(
            roots.profile_path(profile_id),
            roots.profiles_dir.join("Profile [00000000]")
        );
        let archive = roots.archive_path(profile_id);
        assert_eq!(archive, roots.profiles_dir.join("Profile [00000000].tzst"));
        // A caller that knows the profile's name gets it in the storage name.
        assert_eq!(
            roots.profile_path_for(profile_id, "Patch 1.4"),
            roots.profiles_dir.join("Patch 1.4 [00000000]")
        );
        assert_eq!(
            roots.archive_path_for(profile_id, "Patch 1.4"),
            roots.profiles_dir.join("Patch 1.4 [00000000].tzst")
        );
        // Existing data wins over the fallback, whatever it is labelled.
        fs::create_dir_all(&roots.profiles_dir).unwrap();
        let existing = roots.profiles_dir.join("Renamed By Hand [00000000].tzst");
        fs::write(&existing, b"archive").unwrap();
        assert_eq!(
            roots.archive_path(profile_id),
            existing,
            "resolution matches on the short id, never the label"
        );
        fs::remove_file(&existing).unwrap();
        assert_eq!(
            roots.archive_part_path(profile_id),
            PathBuf::from(format!("{}.part", archive.display()))
        );
        assert_eq!(
            roots.archive_backup_path(profile_id),
            PathBuf::from(format!("{}.bak", archive.display()))
        );
    }

    #[test]
    fn reidentifying_an_archive_gives_it_a_new_identity_and_keeps_every_file() {
        let temp = tempdir().unwrap();
        let mods = temp.path().join("Mods");
        fs::create_dir_all(mods.join("SomeMod")).unwrap();
        fs::write(mods.join("SomeMod").join("a.bin"), vec![b'a'; 5000]).unwrap();
        fs::write(mods.join("b.txt"), b"second file").unwrap();
        let roots = ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods,
            archived: None,
            disabled: None,
        };
        let original_id = ProfileId::random();
        let source = temp.path().join("source.tzst");
        let mut original = ProfileArchiveMetadata {
            format_version: PROFILE_ARCHIVE_FORMAT_VERSION,
            profile_id: original_id,
            game_id: "test".to_string(),
            display_name: "Patch 1.4".to_string(),
            backend: GameBackend::Xxmi,
            created_at: Utc::now(),
            uncompressed_size: 0,
            file_count: 0,
            portable_metadata: HashMap::new(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
            source_fingerprint: None,
        };
        original.format_version = PROFILE_ARCHIVE_FORMAT_VERSION;
        create_profile_archive_with_progress(&roots, &original, &source, None).unwrap();

        let new_id = ProfileId::random();
        let destination = temp.path().join("copy.tzst");
        let mut seen_progress = Vec::new();
        let mut report = |copied: u64, total: u64| {
            seen_progress.push((copied, total));
            Ok(())
        };
        let rewritten = reidentify_profile_archive(
            &source,
            &destination,
            new_id,
            "Patch 1.4 (copy)",
            Some(&mut report),
        )
        .unwrap();

        assert_eq!(rewritten.profile_id, new_id);
        assert_eq!(rewritten.display_name, "Patch 1.4 (copy)");
        assert!(
            rewritten.source_fingerprint.is_none(),
            "a rewritten copy no longer matches any live roots"
        );
        assert!(!seen_progress.is_empty(), "progress must be reported");

        // The source is untouched and the copy carries the same payload.
        assert_eq!(
            read_profile_archive_metadata(&source).unwrap().profile_id,
            original_id
        );
        let extracted = temp.path().join("extracted");
        extract_profile_archive(&destination, &extracted).unwrap();
        assert_eq!(
            fs::read(extracted.join("Mods").join("SomeMod").join("a.bin"))
                .unwrap()
                .len(),
            5000
        );
        assert_eq!(
            fs::read(extracted.join("Mods").join("b.txt")).unwrap(),
            b"second file"
        );
        assert_eq!(
            read_profile_archive_metadata(&destination)
                .unwrap()
                .profile_id,
            new_id,
            "only one profile.json survives, the rewritten one"
        );
    }

    #[test]
    fn source_fingerprint_tracks_payload_changes() {
        let temp = tempdir().unwrap();
        let mods = temp.path().join("Mods");
        fs::create_dir_all(&mods).unwrap();
        let roots = ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: mods.clone(),
            archived: None,
            disabled: None,
        };
        fs::write(mods.join("payload.bin"), b"one").unwrap();
        let first = profile_source_fingerprint(&roots).unwrap();
        fs::write(mods.join("payload.bin"), b"two-and-more").unwrap();
        let second = profile_source_fingerprint(&roots).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn failed_archive_commit_restores_existing_final() {
        let temp = tempdir().unwrap();
        let destination = temp.path().join("profile.profile.tzst");
        let staging = temp.path().join("profile.profile.tzst.part");
        fs::write(&destination, b"old archive").unwrap();
        let error = commit_archive_part(&staging, &destination).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed to commit profile archive")
        );
        assert_eq!(fs::read(&destination).unwrap(), b"old archive");
        assert!(!destination.with_extension("tzst.bak").exists());
    }

    #[test]
    fn archive_commit_preserves_preexisting_backup_collision() -> Result<()> {
        let temp = tempdir().unwrap();
        let destination = temp.path().join("profile.profile.tzst");
        let staging = temp.path().join("profile.profile.tzst.part");
        let backup = archive_sidecar_path(&destination, "bak");
        fs::write(&destination, b"old archive").unwrap();
        fs::write(&backup, b"stale backup").unwrap();
        fs::write(&staging, b"new archive").unwrap();

        commit_archive_part(&staging, &destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"new archive");
        assert!(!backup.exists());
        let conflicts = fs::read_dir(temp.path())?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        let conflict = conflicts
            .into_iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("profile.profile.conflict-") && name.ends_with(".tzst")
                    })
            })
            .context("preexisting backup collision was not preserved")?;
        assert_eq!(fs::read(conflict).unwrap(), b"stale backup");
        Ok::<(), anyhow::Error>(())
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
            profile_id: ProfileId::random(),
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
            tools: None,
            tool_blacklist: None,
            source_fingerprint: None,
        };
        let destination = roots.archive_path(metadata.profile_id);
        let inventory = inspect_profile_source(&roots).unwrap();
        assert_eq!(
            inventory.fingerprint(),
            profile_source_fingerprint(&roots).unwrap()
        );
        let result = create_profile_archive_from_inventory_with_progress(
            &inventory,
            &metadata,
            &destination,
            None,
        )
        .unwrap();
        assert_eq!(result.archive_path, destination);
        let manifest = read_profile_archive_metadata(&destination).unwrap();
        assert_eq!(manifest.profile_id, metadata.profile_id);
        assert_eq!(manifest.display_name, metadata.display_name);
        assert_eq!(manifest.categories, metadata.categories);
        assert_eq!(manifest.file_count, 2);
        assert_eq!(manifest.uncompressed_size, 6);
        assert_eq!(
            manifest.source_fingerprint,
            Some(profile_source_fingerprint(&roots).unwrap())
        );
        let extracted = temp.path().join("extract");
        let mut read_updates = Vec::new();
        let mut read_progress = |update: ArchiveReadProgress| -> Result<()> {
            read_updates.push(update);
            Ok(())
        };
        let extracted_metadata = extract_profile_archive_with_progress(
            &destination,
            &extracted,
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
        assert_eq!(
            serde_json::from_slice::<ProfileArchiveMetadata>(
                &fs::read(extracted.join(PROFILE_METADATA_FILE)).unwrap()
            )
            .unwrap()
            .profile_id,
            metadata.profile_id
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
            extract_profile_archive_with_progress(
                &destination,
                &temp.path().join("canceled-extract"),
                Some(&mut cancel),
            )
            .is_err()
        );
        assert!(cancellation_observed);
    }

    #[test]
    fn archive_progress_cancels_inside_large_file() {
        let temp = tempdir().unwrap();
        let mods = temp.path().join("Mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("large.bin"), vec![0x5a; 4 * 1024 * 1024]).unwrap();
        let roots = ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: mods.clone(),
            archived: None,
            disabled: None,
        };
        let metadata = ProfileArchiveMetadata {
            format_version: PROFILE_ARCHIVE_FORMAT_VERSION,
            profile_id: ProfileId::random(),
            game_id: "test".to_string(),
            display_name: "Canceled".to_string(),
            backend: GameBackend::Xxmi,
            created_at: Utc::now(),
            uncompressed_size: 0,
            file_count: 0,
            portable_metadata: HashMap::new(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
            source_fingerprint: None,
        };
        let destination = roots.archive_path(metadata.profile_id);
        let mut canceled = false;
        let mut callback = |update: ArchiveProgress| -> Result<()> {
            if update.bytes_processed > 0 {
                canceled = true;
                bail!("archive canceled inside payload");
            }
            Ok(())
        };
        let error = create_profile_archive_with_progress(
            &roots,
            &metadata,
            &destination,
            Some(&mut callback),
        )
        .unwrap_err();
        assert!(canceled);
        assert!(
            error
                .to_string()
                .contains("archive canceled inside payload")
        );
        assert!(mods.join("large.bin").is_file());
        assert!(!destination.exists());
    }

    #[test]
    fn extraction_reports_and_cancels_inside_a_large_file() {
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
            profile_id: ProfileId::random(),
            game_id: "test".to_string(),
            display_name: "Large".to_string(),
            backend: GameBackend::Xxmi,
            created_at: Utc::now(),
            uncompressed_size: 0,
            file_count: 0,
            portable_metadata: HashMap::new(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
            source_fingerprint: None,
        };
        let destination = roots.archive_path(metadata.profile_id);
        let _result =
            create_profile_archive_with_progress(&roots, &metadata, &destination, None).unwrap();

        let mut updates = Vec::new();
        let mut progress = |update: ArchiveReadProgress| -> Result<()> {
            updates.push(update);
            Ok(())
        };
        extract_profile_archive_with_progress(
            &destination,
            &temp.path().join("large-extract"),
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
            extract_profile_archive_with_progress(
                &destination,
                &temp.path().join("large-canceled"),
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
