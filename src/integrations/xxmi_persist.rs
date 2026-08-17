//! Preservation of 3DMigoto persistent mod settings (`d3dx_user.ini`) across Hestia
//! operations that move, rename, hide, replace, or repackage XXMI mod folders.
//!
//! 3DMigoto namespaces persisted variables by the declaring `.ini`'s path relative to the
//! directory containing `d3dx.ini`, so any folder move orphans a mod's saved customization.
//! This module implements the capture / restore / reroute mechanisms from
//! `PLAN - PREVENTING MOD SETTING LOST.md`: settings are extracted into a per-mod stash at
//! `<mod root>\⬢HESTIA\mod.cfg` while the mod is hidden, and merged back under the mod's
//! current namespace prefix when it becomes visible again.
//!
//! The stash file must never use an `.ini` extension: `include_recursive = Mods` makes
//! 3DMigoto parse every `.ini` under `Mods`, including inside the metadata folder.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sysinfo::UpdateKind;

use crate::{
    model::{GameBackend, GameInstall, MOD_META_DIR, ModEntry, ModStatus},
    persistence,
};

pub const USER_INI_FILE: &str = "d3dx_user.ini";
const USER_INI_TMP_FILE: &str = "d3dx_user.ini.tmp";
const D3DX_INI_FILE: &str = "d3dx.ini";
pub const MOD_STASH_FILE: &str = "mod.cfg";
const LEGACY_HELPER_INI_FILE: &str = "hestia.ini";
/// Key inside `ProfileArchiveMetadata.portable_metadata` holding the whole-file
/// `d3dx_user.ini` snapshot for a profile.
pub const USER_INI_SNAPSHOT_METADATA_KEY: &str = "xxmi.d3dx_user_ini";
const USER_INI_HEADER: &str = "; AUTOMATICALLY GENERATED FILE - DO NOT EDIT";
const CONSTANTS_SECTION: &str = "[Constants]";
const IMPORTER_ROOT_WALK_UP_LEVELS: usize = 4;
const STASH_FORMAT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Case-insensitive key matching
//
// 3DMigoto lowercases ini keys at parse time on both sides of a lookup, so matching is
// case-insensitive while writing stays verbatim. Unicode lowercasing is used so non-ASCII
// folder names compare the same way regardless of who produced the key text.
// ---------------------------------------------------------------------------

fn chars_ci_equal(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Byte length of the leading portion of `key` that case-insensitively matches `prefix`,
/// or `None` when `key` does not start with `prefix`.
fn ci_prefix_len(key: &str, prefix: &str) -> Option<usize> {
    let mut key_iter = key.char_indices();
    let mut consumed = 0usize;
    for expected in prefix.chars() {
        let (index, actual) = key_iter.next()?;
        if !chars_ci_equal(actual, expected) {
            return None;
        }
        consumed = index + actual.len_utf8();
    }
    Some(consumed)
}

fn keys_ci_equal(a: &str, b: &str) -> bool {
    let mut a_chars = a.chars();
    let mut b_chars = b.chars();
    loop {
        match (a_chars.next(), b_chars.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if chars_ci_equal(x, y) => {}
            _ => return false,
        }
    }
}

fn prefixes_overlap_ci(a: &str, b: &str) -> bool {
    ci_prefix_len(a, b).is_some() || ci_prefix_len(b, a).is_some()
}

pub fn namespace_prefixes_overlap(a: &str, b: &str) -> bool {
    prefixes_overlap_ci(a, b)
}

pub fn namespace_prefixes_equal(a: &str, b: &str) -> bool {
    keys_ci_equal(a, b)
}

fn ascii_lowercase(text: &str) -> String {
    text.chars().map(|c| c.to_ascii_lowercase()).collect()
}

// ---------------------------------------------------------------------------
// UserIniDoc
// ---------------------------------------------------------------------------

/// In-memory `d3dx_user.ini`, preserving every line that is not explicitly modified.
#[derive(Debug)]
pub struct UserIniDoc {
    lines: Vec<String>,
    line_ending: &'static str,
    dirty: bool,
}

fn parse_entry(line: &str) -> Option<(&str, &str)> {
    // trim_start only: the value's trailing bytes are preserved verbatim (the plan's
    // byte-for-byte rule); line-level `\r` was already stripped during parsing.
    let trimmed = line.trim_start();
    if !trimmed.starts_with('$') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    Some((key.trim_end(), value.trim_start()))
}

impl UserIniDoc {
    fn from_text(text: &str) -> Self {
        let line_ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let mut lines: Vec<String> = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
        // A trailing newline produces one empty tail element; drop it so save can append the
        // final newline uniformly.
        if lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        Self {
            lines,
            line_ending,
            dirty: false,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            lines: vec![USER_INI_HEADER.to_string(), CONSTANTS_SECTION.to_string()],
            line_ending: "\r\n",
            dirty: false,
        }
    }

    /// Opens `<importer root>\d3dx_user.ini`. `Ok(None)` when the file does not exist;
    /// `Err` when it exists but cannot be read as UTF-8 (callers must treat that as a
    /// no-op for the whole feature rather than risk mangling the file).
    pub fn open(importer_root: &Path) -> Result<Option<Self>> {
        // A stale tmp from a crashed write is dead weight; remove it opportunistically.
        let _ = fs::remove_file(importer_root.join(USER_INI_TMP_FILE));
        let path = importer_root.join(USER_INI_FILE);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err).context(format!("reading {}", path.display())),
        };
        let text = String::from_utf8(bytes)
            .map_err(|_| anyhow!("{} is not valid UTF-8", path.display()))?;
        Ok(Some(Self::from_text(&text)))
    }

    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for line in &self.lines {
            text.push_str(line);
            text.push_str(self.line_ending);
        }
        text
    }

    /// Atomic write: serialize to `d3dx_user.ini.tmp` in the same directory, then rename
    /// over the target so a crash can never leave a truncated `d3dx_user.ini`.
    pub fn save_atomic(&self, importer_root: &Path) -> Result<()> {
        let tmp = importer_root.join(USER_INI_TMP_FILE);
        let target = importer_root.join(USER_INI_FILE);
        fs::write(&tmp, self.to_text().as_bytes()).context(format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &target).context(format!(
            "renaming {} over {}",
            tmp.display(),
            target.display()
        ))?;
        Ok(())
    }

    /// Entries under `prefix` as `(key relative to prefix, value)`, leaving the document
    /// untouched.
    fn read_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        for line in &self.lines {
            if let Some((key, value)) = parse_entry(line)
                && let Some(len) = ci_prefix_len(key, prefix)
            {
                entries.push((key[len..].to_string(), value.to_string()));
            }
        }
        entries
    }

    fn read_prefix_full(&self, prefix: &str) -> Vec<(String, String)> {
        self.read_prefix(prefix)
            .into_iter()
            .map(|(key, value)| (format!("{prefix}{key}"), value))
            .collect()
    }

    /// Extract-and-remove: returns entries under `prefix` (keys relative to it) and deletes
    /// their lines. Never leaves duplicates behind.
    fn take_prefix(&mut self, prefix: &str) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        self.lines.retain(|line| {
            if let Some((key, value)) = parse_entry(line)
                && let Some(len) = ci_prefix_len(key, prefix)
            {
                entries.push((key[len..].to_string(), value.to_string()));
                return false;
            }
            true
        });
        if !entries.is_empty() {
            self.dirty = true;
        }
        entries
    }

    fn take_prefix_full(&mut self, prefix: &str) -> Vec<(String, String)> {
        self.take_prefix(prefix)
            .into_iter()
            .map(|(key, value)| (format!("{prefix}{key}"), value))
            .collect()
    }

    /// Merge entries under `prefix`: an existing key is replaced in place (keeping the
    /// file's own key text); new keys are appended to `[Constants]`, created if absent.
    #[cfg(test)]
    fn merge_prefix(&mut self, prefix: &str, entries: &[(String, String)]) {
        for (relative_key, value) in entries {
            let full_key = format!("{prefix}{relative_key}");
            let mut replaced = false;
            for line in &mut self.lines {
                if let Some((key, existing_value)) = parse_entry(line)
                    && keys_ci_equal(key, &full_key)
                {
                    if existing_value != value {
                        *line = format!("{key} = {value}");
                        self.dirty = true;
                    }
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                let at = self.constants_insert_index();
                self.lines.insert(at, format!("{full_key} = {value}"));
                self.dirty = true;
            }
        }
    }

    fn merge_full_entries(&mut self, entries: &[(String, String)]) {
        for (full_key, value) in entries {
            let mut replaced = false;
            for line in &mut self.lines {
                if let Some((key, existing_value)) = parse_entry(line)
                    && keys_ci_equal(key, full_key)
                {
                    if existing_value != value {
                        *line = format!("{key} = {value}");
                        self.dirty = true;
                    }
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                let at = self.constants_insert_index();
                self.lines.insert(at, format!("{full_key} = {value}"));
                self.dirty = true;
            }
        }
    }

    /// Rewrite every key under `from` to sit under `to`, values untouched. Returns the
    /// number of rewritten lines.
    fn move_prefix(&mut self, from: &str, to: &str) -> usize {
        let mut moved = 0usize;
        for line in &mut self.lines {
            if let Some((key, value)) = parse_entry(line)
                && let Some(len) = ci_prefix_len(key, from)
            {
                let new_key = format!("{to}{}", &key[len..]);
                if new_key != key {
                    *line = format!("{new_key} = {value}");
                    self.dirty = true;
                }
                moved += 1;
            }
        }
        moved
    }

    /// Index right after the last line of the `[Constants]` section, creating the section
    /// (and the generated-file header on a blank document) when missing.
    fn constants_insert_index(&mut self) -> usize {
        let section_index = self
            .lines
            .iter()
            .position(|line| line.trim().eq_ignore_ascii_case(CONSTANTS_SECTION));
        match section_index {
            Some(index) => {
                let mut end = self.lines.len();
                for (offset, line) in self.lines[index + 1..].iter().enumerate() {
                    if line.trim_start().starts_with('[') {
                        end = index + 1 + offset;
                        break;
                    }
                }
                end
            }
            None => {
                if self.lines.is_empty() {
                    self.lines.push(USER_INI_HEADER.to_string());
                }
                self.lines.push(CONSTANTS_SECTION.to_string());
                self.dirty = true;
                self.lines.len()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mod stash — ⬢HESTIA\mod.cfg
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StashEntry {
    pub key: String,
    pub value: String,
}

fn stash_version_default() -> u32 {
    STASH_FORMAT_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModStash {
    #[serde(default = "stash_version_default")]
    pub version: u32,
    /// Full namespace prefix in effect when the stash was last written; the only input
    /// `reroute` needs to repair external path drift.
    #[serde(default)]
    pub source_prefix: String,
    #[serde(default = "Utc::now")]
    pub captured_at: DateTime<Utc>,
    /// Keys are relative to the mod root, never full namespaces, so a stash re-anchors on
    /// whatever path the mod currently has.
    #[serde(default)]
    pub entries: Vec<StashEntry>,
    /// Full persisted keys for all owned prefixes. This is the current representation and
    /// supports explicit mod namespaces such as `namespace = liino`, whose persisted keys
    /// are not under the folder-derived `$\mods\...` prefix. Older stashes populate only
    /// `entries`; those are upgraded through `source_prefix` on read.
    #[serde(default)]
    pub full_entries: Vec<StashEntry>,
}

pub fn stash_path(mod_root: &Path) -> PathBuf {
    mod_root.join(MOD_META_DIR).join(MOD_STASH_FILE)
}

pub fn read_stash(mod_root: &Path) -> Option<ModStash> {
    let bytes = fs::read(stash_path(mod_root)).ok()?;
    let mut stash: ModStash = serde_json::from_slice(&bytes).ok()?;
    upgrade_stash_full_entries(&mut stash);
    Some(stash)
}

fn write_stash(mod_root: &Path, stash: &ModStash) -> Result<()> {
    let path = stash_path(mod_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context(format!("creating {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(stash)?;
    fs::write(&path, bytes).context(format!("writing {}", path.display()))?;
    Ok(())
}

fn upgrade_stash_full_entries(stash: &mut ModStash) {
    if !stash.full_entries.is_empty() || stash.source_prefix.is_empty() {
        return;
    }
    stash.full_entries = stash
        .entries
        .iter()
        .map(|entry| StashEntry {
            key: format!("{}{}", stash.source_prefix, entry.key),
            value: entry.value.clone(),
        })
        .collect();
}

fn full_entries_to_legacy_entries(full_entries: &[StashEntry], prefix: &str) -> Vec<StashEntry> {
    full_entries
        .iter()
        .filter_map(|entry| {
            ci_prefix_len(&entry.key, prefix).map(|len| StashEntry {
                key: entry.key[len..].to_string(),
                value: entry.value.clone(),
            })
        })
        .collect()
}

/// Raw stash bytes, for carrying `mod.cfg` across an import/Replace that destroys the
/// mod folder. Returns `None` when the mod has no stash.
pub fn read_stash_bytes(mod_root: &Path) -> Option<Vec<u8>> {
    fs::read(stash_path(mod_root)).ok()
}

/// Re-create a previously read `mod.cfg` inside the replacement folder. The replacement
/// wins if it somehow carries its own stash already.
pub fn restore_stash_bytes(mod_root: &Path, bytes: &Option<Vec<u8>>) {
    let Some(bytes) = bytes else {
        return;
    };
    let path = stash_path(mod_root);
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_ok()
    {
        let _ = fs::write(path, bytes);
    }
}

// ---------------------------------------------------------------------------
// Importer root + namespace prefix
// ---------------------------------------------------------------------------

/// Walk up from the mods path looking for the directory `d3dx.ini` lives in. Signal
/// priority beats proximity: `d3dx.ini` is authoritative (it defines the include root the
/// namespaces are relative to), `d3d11.dll` is a fallback, and `d3dx_user.ini` is checked
/// last because it legitimately does not exist until something has been persisted.
pub fn importer_root_from_mods_path(mods_path: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![mods_path.to_path_buf()];
    let mut current = mods_path;
    for _ in 0..IMPORTER_ROOT_WALK_UP_LEVELS {
        match current.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                candidates.push(parent.to_path_buf());
                current = parent;
            }
            _ => break,
        }
    }
    for marker in ["d3dx.ini", "d3d11.dll", USER_INI_FILE] {
        for candidate in &candidates {
            if candidate.join(marker).is_file() {
                return Some(candidate.clone());
            }
        }
    }
    None
}

pub fn importer_root_for(game: &GameInstall, use_default: bool) -> Option<PathBuf> {
    if game.definition.backend != GameBackend::Xxmi {
        return None;
    }
    importer_root_from_mods_path(&game.mods_path(use_default)?)
}

/// A mod is live when 3DMigoto can currently see it. Disabled mods keep their folder in
/// place (contents move into `DISABLED_BY_HESTIA\`, which the recursive include still
/// walks, but their declaring inis are hidden), archived mods sit outside `Mods` entirely;
/// both are hidden for persistence purposes.
pub fn is_live(status: &ModStatus) -> bool {
    matches!(status, ModStatus::Active)
}

/// Minimal view of a mod for persistence decisions, so tests do not have to construct a
/// full `ModEntry`.
pub struct ModPersistView<'a> {
    pub root_path: &'a Path,
    pub folder_name: &'a str,
    pub status: ModStatus,
    pub archive_original_path: Option<&'a Path>,
}

impl<'a> From<&'a ModEntry> for ModPersistView<'a> {
    fn from(entry: &'a ModEntry) -> Self {
        Self {
            root_path: &entry.root_path,
            folder_name: &entry.folder_name,
            status: entry.status.clone(),
            archive_original_path: entry.archive_original_path.as_deref(),
        }
    }
}

/// The mod root 3DMigoto sees (or would see) the mod at while live. Archived mods derive
/// from their live location: `archive_original_path` when known, otherwise the game's
/// mods root joined with the folder name — the normal path after a Hestia restart, since
/// `archive_original_path` is never persisted.
fn live_mod_root(view: &ModPersistView<'_>, mods_root: Option<&Path>) -> Option<PathBuf> {
    match view.status {
        ModStatus::Active | ModStatus::Disabled => Some(view.root_path.to_path_buf()),
        ModStatus::Archived => view
            .archive_original_path
            .map(Path::to_path_buf)
            .or_else(|| mods_root.map(|root| root.join(view.folder_name))),
    }
}

/// `"$\" + relative path + "\"`, ASCII-lowercased, `\`-separated. Uses the logical paths
/// Hestia already holds; never canonicalizes, so a symlinked `Mods` keeps the relative
/// form 3DMigoto walked from `d3dx.ini`.
fn namespace_prefix_for_root(mod_root: &Path, importer_root: &Path) -> Option<String> {
    let relative = mod_root.strip_prefix(importer_root).ok()?;
    let mut prefix = String::from("$");
    let mut components = 0usize;
    for component in relative.components() {
        let text = component.as_os_str().to_str()?;
        prefix.push('\\');
        prefix.push_str(&ascii_lowercase(text));
        components += 1;
    }
    if components == 0 {
        return None;
    }
    prefix.push('\\');
    Some(prefix)
}

fn explicit_namespace_prefix(namespace: &str) -> Option<String> {
    let namespace = namespace.trim();
    if namespace.is_empty() {
        return None;
    }
    let mut prefix = String::from("$");
    for segment in namespace
        .split('\\')
        .filter(|segment| !segment.trim().is_empty())
    {
        prefix.push('\\');
        prefix.push_str(&ascii_lowercase(segment.trim()));
    }
    if prefix == "$" {
        return None;
    }
    prefix.push('\\');
    Some(prefix)
}

fn explicit_namespace_prefixes_for_root(mod_root: &Path) -> Vec<String> {
    let mut prefixes = Vec::new();
    collect_explicit_namespace_prefixes(mod_root, &mut prefixes);
    prefixes
}

pub fn explicit_namespace_prefixes_for_mod_root(mod_root: &Path) -> Vec<String> {
    explicit_namespace_prefixes_for_root(mod_root)
}

fn live_mod_prefixes(
    importer_root: &Path,
    mods_root: Option<&Path>,
    view: &ModPersistView<'_>,
) -> Vec<String> {
    if !is_live(&view.status) {
        return Vec::new();
    }
    let mut prefixes = Vec::new();
    if let Some(root) = live_mod_root(view, mods_root)
        && let Some(prefix) = namespace_prefix_for_root(&root, importer_root)
    {
        prefixes.push(prefix);
    }
    for prefix in explicit_namespace_prefixes_for_root(view.root_path) {
        if !prefixes
            .iter()
            .any(|existing| keys_ci_equal(existing, &prefix))
        {
            prefixes.push(prefix);
        }
    }
    prefixes
}

fn mod_prefixes_for_view(
    importer_root: &Path,
    mods_root: Option<&Path>,
    view: &ModPersistView<'_>,
) -> Vec<String> {
    let mut prefixes = Vec::new();
    if let Some(root) = live_mod_root(view, mods_root)
        && let Some(prefix) = namespace_prefix_for_root(&root, importer_root)
    {
        prefixes.push(prefix);
    }
    for prefix in explicit_namespace_prefixes_for_root(view.root_path) {
        if !prefixes
            .iter()
            .any(|existing| keys_ci_equal(existing, &prefix))
        {
            prefixes.push(prefix);
        }
    }
    prefixes
}

fn normalized_var_name(var_name: &str) -> Option<String> {
    let var_name = var_name.trim().trim_start_matches('$').trim();
    (!var_name.is_empty()).then(|| var_name.to_string())
}

fn normalized_ini_relative_key(rel_path: &str, var_name: &str) -> Option<String> {
    let rel_path = rel_path.trim().trim_matches(['/', '\\']).replace('/', "\\");
    if rel_path.is_empty() {
        return None;
    }
    Some(format!("{}\\{var_name}", ascii_lowercase(&rel_path)))
}

fn read_stash_relative_values(stash: &ModStash, prefixes: &[String]) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for prefix in prefixes {
        for entry in &stash.full_entries {
            if let Some(len) = ci_prefix_len(&entry.key, prefix) {
                values.insert(entry.key[len..].to_string(), entry.value.clone());
            }
        }
    }
    if values.is_empty() {
        for entry in &stash.entries {
            values.insert(entry.key.clone(), entry.value.clone());
        }
    }
    values
}

fn set_stash_full_entry(
    stash: &mut ModStash,
    full_key: String,
    value: String,
    legacy_prefix: &str,
) -> bool {
    for entry in &mut stash.full_entries {
        if keys_ci_equal(&entry.key, &full_key) {
            if entry.value == value {
                return false;
            }
            entry.value = value;
            stash.entries = full_entries_to_legacy_entries(&stash.full_entries, legacy_prefix);
            stash.captured_at = Utc::now();
            return true;
        }
    }
    stash.full_entries.push(StashEntry {
        key: full_key,
        value,
    });
    stash.entries = full_entries_to_legacy_entries(&stash.full_entries, legacy_prefix);
    stash.captured_at = Utc::now();
    true
}

fn choose_mod_variable_full_key(
    existing_keys: &[String],
    prefixes: &[String],
    folder_prefix: Option<&String>,
    ini_rel_path: &str,
    var_name: &str,
) -> Option<String> {
    let fallback_prefix = prefixes.first()?;
    let rel_var_name = normalized_ini_relative_key(ini_rel_path, var_name);
    for prefix in prefixes {
        let expected_direct = format!("{prefix}{var_name}");
        let expected_relative = rel_var_name
            .as_ref()
            .map(|rel| format!("{prefix}{rel}"));
        for key in existing_keys {
            if keys_ci_equal(&key, &expected_direct)
                || expected_relative
                    .as_ref()
                    .is_some_and(|expected| keys_ci_equal(&key, expected))
            {
                return Some(key.clone());
            }
        }
    }
    Some(if folder_prefix.is_some_and(|prefix| keys_ci_equal(prefix, fallback_prefix)) {
        rel_var_name
            .as_ref()
            .map(|rel| format!("{fallback_prefix}{rel}"))
            .unwrap_or_else(|| format!("{fallback_prefix}{var_name}"))
    } else {
        format!("{fallback_prefix}{var_name}")
    })
}

fn collect_explicit_namespace_prefixes(path: &Path, prefixes: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_explicit_namespace_prefixes(&path, prefixes);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        {
            collect_explicit_namespace_prefixes_from_ini(&path, prefixes);
        }
    }
}

fn collect_explicit_namespace_prefixes_from_ini(path: &Path, prefixes: &mut Vec<String>) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let mut before_first_section = true;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            before_first_section = false;
            continue;
        }
        if !before_first_section {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("namespace")
            && let Some(prefix) = explicit_namespace_prefix(value)
            && !prefixes
                .iter()
                .any(|existing| keys_ci_equal(existing, &prefix))
        {
            prefixes.push(prefix);
        }
    }
}

// ---------------------------------------------------------------------------
// PersistTx — one transaction per game/importer
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    /// Extract entries and write them to the mod's stash.
    Stash,
    /// Extract entries without writing a stash — the folder is about to be destroyed.
    Purge,
}

/// In-memory rollback point. `d3dx_user.ini` is small, so a full clone of the parsed
/// document is the smallest robust implementation.
pub struct PersistCheckpoint {
    lines: Vec<String>,
    dirty: bool,
}

/// Per-game transaction: one `UserIniDoc` load, N in-memory mutations, one atomic write.
/// Document mutations are provisional until the paired filesystem operation succeeds —
/// callers checkpoint before a capture and roll back when the filesystem step fails, so a
/// wrongly committed removal can never reach disk.
pub struct PersistTx {
    importer_root: PathBuf,
    mods_root: Option<PathBuf>,
    doc: UserIniDoc,
    doc_unreadable: bool,
    warnings: Vec<String>,
    shared_explicit_prefixes: Vec<String>,
    explicit_prefixes_by_root: HashMap<PathBuf, Vec<String>>,
}

pub struct CommitOutcome {
    pub wrote: bool,
    pub warnings: Vec<String>,
}

/// `None` when the feature does not apply to this game: non-XXMI backend, no mods path,
/// or no importer root discoverable near it.
pub fn begin(game: &GameInstall, use_default: bool) -> Option<PersistTx> {
    let importer_root = importer_root_for(game, use_default)?;
    let mods_root = game.mods_path(use_default);
    let (doc, doc_unreadable, warnings) = match UserIniDoc::open(&importer_root) {
        Ok(Some(doc)) => (doc, false, Vec::new()),
        // Missing file: captures find nothing, a restore creates it on write.
        Ok(None) => (UserIniDoc::new_empty(), false, Vec::new()),
        Err(err) => (
            UserIniDoc::new_empty(),
            true,
            vec![format!("mod settings preservation skipped: {err:#}")],
        ),
    };
    Some(PersistTx {
        importer_root,
        mods_root,
        doc,
        doc_unreadable,
        warnings,
        shared_explicit_prefixes: Vec::new(),
        explicit_prefixes_by_root: HashMap::new(),
    })
}

impl PersistTx {
    pub fn importer_root(&self) -> &Path {
        &self.importer_root
    }

    pub fn checkpoint(&self) -> PersistCheckpoint {
        PersistCheckpoint {
            lines: self.doc.lines.clone(),
            dirty: self.doc.dirty,
        }
    }

    pub fn rollback(&mut self, checkpoint: PersistCheckpoint) {
        self.doc.lines = checkpoint.lines;
        self.doc.dirty = checkpoint.dirty;
    }

    /// Ordering marker for the action layer; the mutation is already in the document.
    pub fn keep(&mut self, _checkpoint: PersistCheckpoint) {}

    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    pub fn set_shared_explicit_prefixes(&mut self, prefixes: Vec<String>) {
        self.shared_explicit_prefixes = prefixes;
    }

    pub fn set_explicit_namespace_prefixes_for_root(
        &mut self,
        root_path: PathBuf,
        prefixes: Vec<String>,
    ) {
        self.explicit_prefixes_by_root.insert(root_path, prefixes);
    }

    fn prefix_for(&self, view: &ModPersistView<'_>) -> Option<String> {
        let root = live_mod_root(view, self.mods_root.as_deref())?;
        namespace_prefix_for_root(&root, &self.importer_root)
    }

    fn explicit_prefixes_for(&self, view: &ModPersistView<'_>) -> Vec<String> {
        self.explicit_prefixes_by_root
            .get(view.root_path)
            .cloned()
            .unwrap_or_else(|| explicit_namespace_prefixes_for_root(view.root_path))
    }

    fn prefixes_for(&self, view: &ModPersistView<'_>) -> Vec<String> {
        let mut prefixes = Vec::new();
        if let Some(prefix) = self.prefix_for(view) {
            prefixes.push(prefix);
        }
        for prefix in self.explicit_prefixes_for(view) {
            if self.is_shared_explicit_prefix(&prefix) {
                continue;
            }
            if !prefixes
                .iter()
                .any(|existing| keys_ci_equal(existing, &prefix))
            {
                prefixes.push(prefix);
            }
        }
        prefixes
    }

    fn shared_explicit_prefixes_for(&self, view: &ModPersistView<'_>) -> Vec<String> {
        self.explicit_prefixes_for(view)
            .into_iter()
            .filter(|prefix| self.is_shared_explicit_prefix(prefix))
            .collect()
    }

    fn is_shared_explicit_prefix(&self, prefix: &str) -> bool {
        self.shared_explicit_prefixes
            .iter()
            .any(|shared| prefixes_overlap_ci(shared, prefix))
    }

    fn is_shared_explicit_key(&self, key: &str) -> bool {
        self.shared_explicit_prefixes
            .iter()
            .any(|prefix| ci_prefix_len(key, prefix).is_some())
    }

    pub fn capture(&mut self, entry: &ModEntry, mode: CaptureMode) -> Result<bool> {
        self.capture_view(&entry.into(), mode)
    }

    pub fn capture_view(&mut self, view: &ModPersistView<'_>, mode: CaptureMode) -> Result<bool> {
        if self.doc_unreadable {
            return Ok(false);
        }
        // An archived mod's entries left the file when it was hidden; anything now sitting
        // under its prospective prefix belongs to whichever live mod occupies that folder
        // name. Taking it would steal a same-named live mod's settings, so archived
        // captures never touch the document (its own values are already in the stash).
        if matches!(view.status, ModStatus::Archived) {
            return Ok(false);
        }
        let prefixes = self.prefixes_for(view);
        if prefixes.is_empty() {
            return Ok(false);
        }
        let mut removed = Vec::new();
        for prefix in &prefixes {
            removed.extend(self.doc.take_prefix_full(prefix));
        }
        let mut copied_shared = Vec::new();
        if mode == CaptureMode::Stash {
            for prefix in self.shared_explicit_prefixes_for(view) {
                for (key, value) in self.doc.read_prefix_full(&prefix) {
                    if removed
                        .iter()
                        .chain(copied_shared.iter())
                        .any(|(existing, _)| keys_ci_equal(existing, &key))
                    {
                        continue;
                    }
                    copied_shared.push((key, value));
                }
            }
        }
        if removed.is_empty() && copied_shared.is_empty() {
            // Empty reads are not a reliable "user wiped every setting" signal. During a
            // mod switch, the running importer may not have saved the just-restored live
            // values yet, so overwriting an existing stash here can destroy the only good
            // copy. Keep the last stash unless we actually captured entries.
            return Ok(false);
        }
        if mode == CaptureMode::Stash {
            let source_prefix = prefixes[0].clone();
            let full_entries: Vec<StashEntry> = removed
                .iter()
                .chain(copied_shared.iter())
                .map(|(key, value)| StashEntry {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect();
            let entries = full_entries_to_legacy_entries(&full_entries, &source_prefix);
            write_stash(
                view.root_path,
                &ModStash {
                    version: STASH_FORMAT_VERSION,
                    source_prefix,
                    captured_at: Utc::now(),
                    entries,
                    full_entries,
                },
            )?;
        }
        Ok(true)
    }

    pub fn restore(&mut self, entry: &ModEntry) -> Result<bool> {
        self.restore_view(&entry.into())
    }

    /// Merge the stash back under the mod's current prefix. Only legal for live mods:
    /// writing live entries for a hidden mod puts keys in the file that nothing declares,
    /// and the next 3DMigoto save drops them.
    pub fn restore_view(&mut self, view: &ModPersistView<'_>) -> Result<bool> {
        if self.doc_unreadable || !is_live(&view.status) {
            return Ok(false);
        }
        let Some(mut stash) = read_stash(view.root_path) else {
            return Ok(false);
        };
        if stash.full_entries.is_empty() {
            return Ok(false);
        }
        let Some(prefix) = self.prefix_for(view) else {
            return Ok(false);
        };
        // Reroute first: absorb any stale-prefix entries left by a path change Hestia did
        // not perform, then re-anchor the stash onto the current prefix.
        if !stash.source_prefix.is_empty() && !keys_ci_equal(&stash.source_prefix, &prefix) {
            let old_prefix = stash.source_prefix.clone();
            self.doc.move_prefix(&old_prefix, &prefix);
            stash.source_prefix = prefix.clone();
            for entry in &mut stash.full_entries {
                if let Some(len) = ci_prefix_len(&entry.key, &old_prefix) {
                    entry.key = format!("{prefix}{}", &entry.key[len..]);
                }
            }
            stash.entries = full_entries_to_legacy_entries(&stash.full_entries, &prefix);
            write_stash(view.root_path, &stash)?;
        }
        let entries: Vec<(String, String)> = stash
            .full_entries
            .iter()
            .filter(|entry| !self.is_shared_explicit_key(&entry.key))
            .map(|entry| (entry.key.clone(), entry.value.clone()))
            .collect();
        if entries.is_empty() {
            return Ok(false);
        }
        self.doc.merge_full_entries(&entries);
        Ok(true)
    }

    pub fn reroute(&mut self, entry: &ModEntry) -> Result<bool> {
        self.reroute_view(&entry.into())
    }

    /// Repair path drift: rewrite live entries from the stash's recorded prefix to the
    /// mod's current prefix, or — for a hidden mod — absorb them into the stash instead
    /// of writing anything live.
    pub fn reroute_view(&mut self, view: &ModPersistView<'_>) -> Result<bool> {
        if self.doc_unreadable {
            return Ok(false);
        }
        let Some(mut stash) = read_stash(view.root_path) else {
            return Ok(false);
        };
        let Some(prefix) = self.prefix_for(view) else {
            return Ok(false);
        };
        if stash.source_prefix.is_empty() || keys_ci_equal(&stash.source_prefix, &prefix) {
            return Ok(false);
        }
        let old_prefix = stash.source_prefix.clone();
        if is_live(&view.status) {
            self.doc.move_prefix(&old_prefix, &prefix);
        } else if matches!(view.status, ModStatus::Disabled) {
            // A disabled mod still owns its folder, so stale entries under its old prefix
            // are unambiguously its own — absorb them into the stash. An archived mod's
            // old prefix may since have been claimed by a live mod of the same name, so
            // its reroute is bookkeeping only; orphaned entries are dropped by 3DMigoto's
            // next save anyway.
            let absorbed = self.doc.take_prefix(&old_prefix);
            if !absorbed.is_empty() {
                let absorbed_full: Vec<StashEntry> = absorbed
                    .iter()
                    .map(|(key, value)| StashEntry {
                        key: format!("{old_prefix}{key}"),
                        value: value.clone(),
                    })
                    .collect();
                stash.full_entries = absorbed_full;
                stash.captured_at = Utc::now();
            }
        }
        for entry in &mut stash.full_entries {
            if let Some(len) = ci_prefix_len(&entry.key, &old_prefix) {
                entry.key = format!("{prefix}{}", &entry.key[len..]);
            }
        }
        stash.source_prefix = prefix;
        stash.entries = full_entries_to_legacy_entries(&stash.full_entries, &stash.source_prefix);
        write_stash(view.root_path, &stash)?;
        Ok(true)
    }

    pub fn rebase(&mut self, entry: &ModEntry) -> Result<bool> {
        self.rebase_view(&entry.into())
    }

    /// Stash-only bookkeeping for a hidden mod whose folder moved: point `source_prefix`
    /// at the new prospective live prefix without touching `d3dx_user.ini`.
    pub fn rebase_view(&mut self, view: &ModPersistView<'_>) -> Result<bool> {
        let Some(mut stash) = read_stash(view.root_path) else {
            return Ok(false);
        };
        let Some(prefix) = self.prefix_for(view) else {
            return Ok(false);
        };
        if keys_ci_equal(&stash.source_prefix, &prefix) {
            return Ok(false);
        }
        let old_prefix = stash.source_prefix.clone();
        stash.source_prefix = prefix;
        for entry in &mut stash.full_entries {
            if let Some(len) = ci_prefix_len(&entry.key, &old_prefix) {
                entry.key = format!("{}{}", stash.source_prefix, &entry.key[len..]);
            }
        }
        stash.entries = full_entries_to_legacy_entries(&stash.full_entries, &stash.source_prefix);
        write_stash(view.root_path, &stash)?;
        Ok(true)
    }

    pub fn baseline(&mut self, entry: &ModEntry) -> Result<bool> {
        self.baseline_view(&entry.into())
    }

    /// Record a repair anchor for a live mod that has entries but no stash yet, without
    /// removing anything from `d3dx_user.ini`. Mods with no entries get no file: they
    /// have nothing to lose.
    pub fn baseline_view(&mut self, view: &ModPersistView<'_>) -> Result<bool> {
        if self.doc_unreadable || !is_live(&view.status) {
            return Ok(false);
        }
        if stash_path(view.root_path).exists() {
            return Ok(false);
        }
        let prefixes = self.prefixes_for(view);
        let Some(prefix) = prefixes.first().cloned() else {
            return Ok(false);
        };
        let mut full_entries = Vec::new();
        for prefix in &prefixes {
            full_entries.extend(
                self.doc
                    .read_prefix_full(prefix)
                    .into_iter()
                    .map(|(key, value)| StashEntry { key, value }),
            );
        }
        if full_entries.is_empty() {
            return Ok(false);
        }
        let entries = full_entries_to_legacy_entries(&full_entries, &prefix);
        write_stash(
            view.root_path,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: prefix,
                captured_at: Utc::now(),
                entries,
                full_entries,
            },
        )?;
        Ok(true)
    }

    pub fn restore_imported(&mut self, entry: &ModEntry) -> Result<bool> {
        self.restore_imported_view(&entry.into())
    }

    /// Restore for a freshly imported folder that arrived carrying a stash, but only when
    /// no live entries already exist under the mod's current prefix — live entries are
    /// newer than anything an imported stash can hold.
    pub fn restore_imported_view(&mut self, view: &ModPersistView<'_>) -> Result<bool> {
        if self.doc_unreadable || !is_live(&view.status) {
            return Ok(false);
        }
        let prefixes = self.prefixes_for(view);
        if prefixes.is_empty() {
            return Ok(false);
        }
        if prefixes
            .iter()
            .any(|prefix| !self.doc.read_prefix(prefix).is_empty())
        {
            return Ok(false);
        }
        self.restore_view(view)
    }

    /// Writes the document if any kept mutation dirtied it. Never writes a document that
    /// failed to read.
    pub fn commit(self) -> Result<CommitOutcome> {
        if self.doc_unreadable || !self.doc.dirty {
            return Ok(CommitOutcome {
                wrote: false,
                warnings: self.warnings,
            });
        }
        self.doc.save_atomic(&self.importer_root)?;
        Ok(CommitOutcome {
            wrote: true,
            warnings: self.warnings,
        })
    }
}

// ---------------------------------------------------------------------------
// Profile snapshot helpers
// ---------------------------------------------------------------------------

/// Whole-file text of `d3dx_user.ini`, or `None` when missing or not UTF-8.
pub fn snapshot_user_ini(importer_root: &Path) -> Option<String> {
    let bytes = fs::read(importer_root.join(USER_INI_FILE)).ok()?;
    String::from_utf8(bytes).ok()
}

/// The content a brand-new empty profile starts with — same header and line-ending style
/// as a restore-created file, so the two paths produce identical documents.
pub fn clean_user_ini_snapshot() -> String {
    UserIniDoc::new_empty().to_text()
}

pub fn apply_user_ini_snapshot(importer_root: &Path, text: &str) -> Result<()> {
    let tmp = importer_root.join(USER_INI_TMP_FILE);
    let target = importer_root.join(USER_INI_FILE);
    fs::write(&tmp, text.as_bytes()).context(format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &target).context(format!(
        "renaming {} over {}",
        tmp.display(),
        target.display()
    ))?;
    Ok(())
}

pub fn read_mod_variables(
    game: &GameInstall,
    use_default: bool,
    entry: &ModEntry,
) -> Result<HashMap<String, String>> {
    let Some(importer_root) = importer_root_for(game, use_default) else {
        return Ok(HashMap::new());
    };
    let view = ModPersistView::from(entry);
    let prefixes = match view.status {
        ModStatus::Active => {
            live_mod_prefixes(&importer_root, game.mods_path(use_default).as_deref(), &view)
        }
        ModStatus::Disabled | ModStatus::Archived => {
            mod_prefixes_for_view(&importer_root, game.mods_path(use_default).as_deref(), &view)
        }
    };
    if prefixes.is_empty() {
        return Ok(HashMap::new());
    }
    if matches!(view.status, ModStatus::Disabled | ModStatus::Archived) {
        return Ok(read_stash(&entry.root_path)
            .as_ref()
            .map(|stash| read_stash_relative_values(stash, &prefixes))
            .unwrap_or_default());
    }
    let Some(doc) = UserIniDoc::open(&importer_root)? else {
        return Ok(HashMap::new());
    };
    let mut values = HashMap::new();
    for prefix in prefixes {
        for (key, value) in doc.read_prefix(&prefix) {
            values.insert(key, value);
        }
    }
    Ok(values)
}

pub fn set_mod_variable(
    game: &GameInstall,
    use_default: bool,
    entry: &ModEntry,
    ini_rel_path: &str,
    var_name: &str,
    value: &str,
) -> Result<bool> {
    let Some(var_name) = normalized_var_name(var_name) else {
        return Ok(false);
    };
    let Some(importer_root) = importer_root_for(game, use_default) else {
        return Ok(false);
    };
    let view = ModPersistView::from(entry);
    let folder_prefix = live_mod_root(&view, game.mods_path(use_default).as_deref())
        .and_then(|root| namespace_prefix_for_root(&root, &importer_root));
    let prefixes = match view.status {
        ModStatus::Active => {
            live_mod_prefixes(&importer_root, game.mods_path(use_default).as_deref(), &view)
        }
        ModStatus::Disabled | ModStatus::Archived => {
            mod_prefixes_for_view(&importer_root, game.mods_path(use_default).as_deref(), &view)
        }
    };
    let Some(fallback_prefix) = prefixes.first() else {
        return Ok(false);
    };
    if matches!(view.status, ModStatus::Disabled | ModStatus::Archived) {
        let mut stash = read_stash(&entry.root_path).unwrap_or_else(|| ModStash {
            version: STASH_FORMAT_VERSION,
            source_prefix: fallback_prefix.clone(),
            captured_at: Utc::now(),
            entries: Vec::new(),
            full_entries: Vec::new(),
        });
        if stash.source_prefix.is_empty() {
            stash.source_prefix = fallback_prefix.clone();
        }
        let existing_keys: Vec<String> = stash
            .full_entries
            .iter()
            .map(|entry| entry.key.clone())
            .collect();
        let Some(full_key) = choose_mod_variable_full_key(
            &existing_keys,
            &prefixes,
            folder_prefix.as_ref(),
            ini_rel_path,
            &var_name,
        ) else {
            return Ok(false);
        };
        let legacy_prefix = stash.source_prefix.clone();
        let changed = set_stash_full_entry(
            &mut stash,
            full_key,
            value.trim().to_string(),
            &legacy_prefix,
        );
        if changed {
            write_stash(&entry.root_path, &stash)?;
        }
        return Ok(changed);
    }
    let mut doc = match UserIniDoc::open(&importer_root)? {
        Some(doc) => doc,
        None => UserIniDoc::new_empty(),
    };
    let existing_keys: Vec<String> = prefixes
        .iter()
        .flat_map(|prefix| doc.read_prefix_full(prefix).into_iter().map(|(key, _)| key))
        .collect();
    let Some(full_key) = choose_mod_variable_full_key(
        &existing_keys,
        &prefixes,
        folder_prefix.as_ref(),
        ini_rel_path,
        &var_name,
    ) else {
        return Ok(false);
    };
    doc.merge_full_entries(&[(full_key, value.trim().to_string())]);
    if doc.dirty {
        doc.save_atomic(&importer_root)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn user_ini_snapshot_from_metadata(
    portable_metadata: &HashMap<String, serde_json::Value>,
) -> Option<String> {
    portable_metadata
        .get(USER_INI_SNAPSHOT_METADATA_KEY)?
        .as_str()
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// d3dx.ini reload config + reload hotkey
// ---------------------------------------------------------------------------

const LEGACY_HELPER_INI_SUPPORTED: &str = "; Hestia XXMI helper\r\n\
; Generated by Hestia. Safe to delete; Hestia may recreate it.\r\n\
\r\n\
namespace = hestia\\bridge\r\n\
\r\n\
[System]\r\n\
additional_foreground_window = Hestia\r\n";

const LEGACY_HELPER_INI_NEUTRAL: &str = "; Hestia XXMI helper\r\n\
; Generated by Hestia. Safe to delete; Hestia may recreate it.\r\n\
\r\n\
namespace = hestia\\bridge\r\n\
\r\n\
[System]\r\n\
; additional_foreground_window support was not detected for this importer DLL.\r\n";

const HESTIA_D3DX_BEGIN: &str = "; --- Hestia begin ---";
const HESTIA_D3DX_END: &str = "; --- Hestia end ---";
const LEGACY_HESTIA_D3DX_COMMENT_1: &str = "; Hestia uses this experimental block to let XXMI receive reload hotkeys while Hestia is focused.";
const LEGACY_HESTIA_D3DX_COMMENT_2: &str =
    "; Safe to delete this whole block if you remove Hestia.";
const FOREGROUND_WINDOW_KEY: &str = "additional_foreground_window";
const HESTIA_WINDOW_TITLE: &str = "Hestia";
const FOREGROUND_RELOAD_ATTEMPTS: u32 = 5;
const FOREGROUND_RELOAD_RETRY_DELAY: Duration = Duration::from_millis(100);
const FOREGROUND_SETTLE_TIMEOUT: Duration = Duration::from_millis(900);
const FOREGROUND_SETTLE_POLL: Duration = Duration::from_millis(25);
const FOCUS_ROUTE_RETRY_TIMEOUT: Duration = Duration::from_millis(10_000);
const FOCUS_ROUTE_RETRY_DELAY: Duration = Duration::from_millis(250);
const KEYBOARD_IDLE_REQUIRED: Duration = Duration::from_millis(350);
const KEYBOARD_IDLE_POLL: Duration = Duration::from_millis(25);
const RELOAD_KEY_SETTLE_UP_MS: u64 = 180;
const RELOAD_KEY_HOLD_MS: u64 = 350;
const RELOAD_KEY_PULSE_COUNT: u32 = 2;
const RELOAD_KEY_PULSE_GAP_MS: u64 = 220;

#[derive(Clone, Debug)]
pub struct ReloadHotkeyReport {
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct D3dxForegroundConflict {
    pub path: PathBuf,
    pub current_value: String,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct FocusRouteOutcome {
    sent: bool,
    restored_focus: bool,
    used_unlock: bool,
    attempts: u32,
    waited_for_keyboard_idle: bool,
}

enum D3dxPatch {
    Unchanged,
    Updated(String),
    // Known limitation: XXMI reads `additional_foreground_window` as one value, not a
    // multi-value list. If another tool already owns it, Hestia skips the patch and reload send.
    BlockedByExistingBinding,
}

/// Whether a synthetic reload hotkey can actually reach this importer while Hestia is the
/// foreground window. Without `additional_foreground_window` support in the installed DLL
/// (`check_foreground_window = 1` ships enabled), a send from Hestia lands nowhere useful
/// and the caller should skip it — the next launch reads the file anyway.
pub fn reload_hotkey_supported(importer_root: &Path) -> bool {
    importer_dll_supports_foreground_window(importer_root)
        && d3dx_ini_hestia_foreground_binding_active(importer_root)
}

/// Read-only capability probe: the importer DLL supports `additional_foreground_window`
/// when its bytes contain the UTF-16LE spelling of the option name. No code is executed.
fn importer_dll_supports_foreground_window(importer_root: &Path) -> bool {
    let Ok(bytes) = fs::read(importer_root.join("d3d11.dll")) else {
        return false;
    };
    let needle: Vec<u8> = "additional_foreground_window"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    bytes.windows(needle.len()).any(|window| window == needle)
}

/// Install or refresh Hestia's reload helper binding in root `d3dx.ini`.
///
/// `additional_foreground_window` is a root `d3dx.ini` option, not a mod ini option. Older
/// builds wrote `Mods\hestia.ini`; this function removes that legacy file only when its bytes
/// exactly match Hestia's generated content.
pub fn ensure_reload_config(
    game: &GameInstall,
    use_default: bool,
    enable_reload: bool,
) -> Result<()> {
    let Some(importer_root) = importer_root_for(game, use_default) else {
        return Ok(());
    };
    if let Some(mods_path) = game.mods_path(use_default)
        && mods_path.is_dir()
    {
        cleanup_legacy_helper_ini(&mods_path)?;
    }

    let d3dx_ini = importer_root.join(D3DX_INI_FILE);
    let bytes = fs::read(&d3dx_ini).context(format!("reading {}", d3dx_ini.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| anyhow!("{} is not valid UTF-8", d3dx_ini.display()))?;
    let enable = enable_reload && importer_dll_supports_foreground_window(&importer_root);

    match patch_d3dx_ini_text(&text, enable) {
        D3dxPatch::Unchanged | D3dxPatch::BlockedByExistingBinding => Ok(()),
        D3dxPatch::Updated(updated) => {
            rotate_d3dx_backup(&d3dx_ini)?;
            persistence::write_atomic_text(&d3dx_ini, &updated)
                .context(format!("writing {}", d3dx_ini.display()))
        }
    }
}

pub fn reload_config_conflict(
    game: &GameInstall,
    use_default: bool,
) -> Result<Option<D3dxForegroundConflict>> {
    let Some(importer_root) = importer_root_for(game, use_default) else {
        return Ok(None);
    };
    if !importer_dll_supports_foreground_window(&importer_root) {
        return Ok(None);
    }
    let path = importer_root.join(D3DX_INI_FILE);
    let text = fs::read_to_string(&path).context(format!("reading {}", path.display()))?;
    let body = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines = split_ini_lines(body);
    remove_exact_hestia_d3dx_block(&mut lines);
    let Some((_, value)) = first_system_foreground_binding_entry(&lines) else {
        return Ok(None);
    };
    let current_value = value.trim();
    if current_value.eq_ignore_ascii_case(HESTIA_WINDOW_TITLE) {
        Ok(None)
    } else {
        Ok(Some(D3dxForegroundConflict {
            path,
            current_value: current_value.to_string(),
        }))
    }
}

pub fn replace_reload_config_conflict(game: &GameInstall, use_default: bool) -> Result<()> {
    let Some(importer_root) = importer_root_for(game, use_default) else {
        return Ok(());
    };
    let d3dx_ini = importer_root.join(D3DX_INI_FILE);
    let bytes = fs::read(&d3dx_ini).context(format!("reading {}", d3dx_ini.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| anyhow!("{} is not valid UTF-8", d3dx_ini.display()))?;
    match patch_d3dx_ini_text_replace_conflict(&text) {
        D3dxPatch::Unchanged | D3dxPatch::BlockedByExistingBinding => Ok(()),
        D3dxPatch::Updated(updated) => {
            rotate_d3dx_backup(&d3dx_ini)?;
            persistence::write_atomic_text(&d3dx_ini, &updated)
                .context(format!("writing {}", d3dx_ini.display()))
        }
    }
}

fn cleanup_legacy_helper_ini(mods_path: &Path) -> Result<()> {
    let path = mods_path.join(LEGACY_HELPER_INI_FILE);
    let Ok(existing) = fs::read(&path) else {
        return Ok(());
    };
    if existing == LEGACY_HELPER_INI_SUPPORTED.as_bytes()
        || existing == LEGACY_HELPER_INI_NEUTRAL.as_bytes()
    {
        fs::remove_file(&path).context(format!("removing {}", path.display()))?;
    }
    Ok(())
}

fn patch_d3dx_ini_text(text: &str, enable: bool) -> D3dxPatch {
    let has_bom = text.starts_with('\u{feff}');
    let body = if has_bom {
        text.strip_prefix('\u{feff}').unwrap_or(text)
    } else {
        text
    };
    let line_ending = detect_line_ending(body);
    let trailing_newline = body.ends_with('\n');
    let mut lines = split_ini_lines(body);
    let removed_hestia_block = remove_exact_hestia_d3dx_block(&mut lines);

    if enable {
        match first_system_foreground_binding(&lines) {
            Some(value) if value.trim().eq_ignore_ascii_case(HESTIA_WINDOW_TITLE) => {
                if removed_hestia_block {
                    return finish_d3dx_patch(
                        text,
                        join_ini_lines(&lines, line_ending, trailing_newline, has_bom),
                    );
                }
                return D3dxPatch::Unchanged;
            }
            Some(_) => {
                // XXMI reads this through single-value GetIniString(), not its multi-key helper.
                // Existing non-Hestia values are deliberately left alone until Hestia has a
                // verified conflict-resolution UI.
                if removed_hestia_block {
                    return finish_d3dx_patch(
                        text,
                        join_ini_lines(&lines, line_ending, trailing_newline, has_bom),
                    );
                }
                return D3dxPatch::BlockedByExistingBinding;
            }
            None => {
                insert_hestia_foreground_binding(&mut lines);
                return finish_d3dx_patch(text, join_ini_lines(&lines, line_ending, true, has_bom));
            }
        }
    }

    if removed_hestia_block {
        finish_d3dx_patch(
            text,
            join_ini_lines(&lines, line_ending, trailing_newline, has_bom),
        )
    } else if comment_hestia_foreground_binding_in_marked_block(&mut lines) {
        finish_d3dx_patch(
            text,
            join_ini_lines(&lines, line_ending, trailing_newline, has_bom),
        )
    } else if let Some((idx, value)) = first_system_foreground_binding_entry(&lines)
        && value.trim().eq_ignore_ascii_case(HESTIA_WINDOW_TITLE)
    {
        lines[idx] = format!("; {}", lines[idx]);
        finish_d3dx_patch(
            text,
            join_ini_lines(&lines, line_ending, trailing_newline, has_bom),
        )
    } else {
        D3dxPatch::Unchanged
    }
}

fn patch_d3dx_ini_text_replace_conflict(text: &str) -> D3dxPatch {
    let has_bom = text.starts_with('\u{feff}');
    let body = if has_bom {
        text.strip_prefix('\u{feff}').unwrap_or(text)
    } else {
        text
    };
    let line_ending = detect_line_ending(body);
    let trailing_newline = body.ends_with('\n');
    let mut lines = split_ini_lines(body);
    remove_exact_hestia_d3dx_block(&mut lines);

    if let Some((idx, value)) = first_system_foreground_binding_entry(&lines) {
        if value.trim().eq_ignore_ascii_case(HESTIA_WINDOW_TITLE) {
            return finish_d3dx_patch(
                text,
                join_ini_lines(&lines, line_ending, trailing_newline, has_bom),
            );
        }
        lines[idx] = format!("; {}", lines[idx]);
    }
    insert_hestia_foreground_binding(&mut lines);
    finish_d3dx_patch(text, join_ini_lines(&lines, line_ending, true, has_bom))
}

fn finish_d3dx_patch(original: &str, updated: String) -> D3dxPatch {
    if updated == original {
        D3dxPatch::Unchanged
    } else {
        D3dxPatch::Updated(updated)
    }
}

fn d3dx_ini_hestia_foreground_binding_active(importer_root: &Path) -> bool {
    let Ok(text) = fs::read_to_string(importer_root.join(D3DX_INI_FILE)) else {
        return false;
    };
    let body = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let lines = split_ini_lines(body);
    first_system_foreground_binding(&lines)
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(HESTIA_WINDOW_TITLE))
}

fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn split_ini_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect();
    if text.ends_with('\n') && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines
}

fn join_ini_lines(
    lines: &[String],
    line_ending: &str,
    trailing_newline: bool,
    include_bom: bool,
) -> String {
    let mut text = String::new();
    if include_bom {
        text.push('\u{feff}');
    }
    for (idx, line) in lines.iter().enumerate() {
        if idx > 0 {
            text.push_str(line_ending);
        }
        text.push_str(line);
    }
    if trailing_newline {
        text.push_str(line_ending);
    }
    text
}

fn hestia_foreground_binding_lines() -> Vec<String> {
    vec![
        HESTIA_D3DX_BEGIN.to_string(),
        "; Enables Hestia to trigger XXMI's in-game reload while Hestia is focused.".to_string(),
        "; Safe to delete this marked block if you stop using Hestia.".to_string(),
        format!("{FOREGROUND_WINDOW_KEY} = {HESTIA_WINDOW_TITLE}"),
        HESTIA_D3DX_END.to_string(),
        String::new(),
    ]
}

fn insert_hestia_foreground_binding(lines: &mut Vec<String>) {
    if let Some(system_idx) = lines.iter().position(|line| {
        section_name(line.trim()).is_some_and(|section| section.eq_ignore_ascii_case("System"))
    }) {
        let mut binding = hestia_foreground_binding_lines();
        lines.splice(system_idx + 1..system_idx + 1, binding.drain(..));
        return;
    }

    if !lines.is_empty() && lines.last().is_some_and(|line| !line.trim().is_empty()) {
        lines.push(String::new());
    }
    lines.push("[System]".to_string());
    lines.extend(hestia_foreground_binding_lines());
}

fn hestia_d3dx_block_range(lines: &[String]) -> Option<(usize, usize, usize)> {
    let Some(begin) = lines
        .iter()
        .position(|line| line.trim() == HESTIA_D3DX_BEGIN)
    else {
        return None;
    };
    let Some(end) = lines[begin + 1..]
        .iter()
        .position(|line| line.trim() == HESTIA_D3DX_END)
        .map(|idx| begin + 1 + idx)
    else {
        return None;
    };
    let mut drain_end = end + 1;
    if lines
        .get(drain_end)
        .is_some_and(|line| line.trim().is_empty())
    {
        drain_end += 1;
    }
    Some((begin, end, drain_end))
}

fn hestia_d3dx_exact_block_lines() -> Vec<String> {
    let mut lines = hestia_foreground_binding_lines();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines
}

fn legacy_hestia_d3dx_exact_block_lines() -> Vec<String> {
    vec![
        HESTIA_D3DX_BEGIN.to_string(),
        LEGACY_HESTIA_D3DX_COMMENT_1.to_string(),
        LEGACY_HESTIA_D3DX_COMMENT_2.to_string(),
        "[System]".to_string(),
        format!("{FOREGROUND_WINDOW_KEY} = {HESTIA_WINDOW_TITLE}"),
        HESTIA_D3DX_END.to_string(),
    ]
}

fn remove_exact_hestia_d3dx_block(lines: &mut Vec<String>) -> bool {
    let Some((begin, end, drain_end)) = hestia_d3dx_block_range(lines) else {
        return false;
    };
    let block = &lines[begin..=end];
    if block != hestia_d3dx_exact_block_lines().as_slice()
        && block != legacy_hestia_d3dx_exact_block_lines().as_slice()
    {
        return false;
    }
    lines.drain(begin..drain_end);
    true
}

fn comment_hestia_foreground_binding_in_marked_block(lines: &mut [String]) -> bool {
    let Some((begin, end, _)) = hestia_d3dx_block_range(lines) else {
        return false;
    };
    for line in &mut lines[begin + 1..end] {
        let trimmed = line.trim();
        if trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(FOREGROUND_WINDOW_KEY)
            && value.trim().eq_ignore_ascii_case(HESTIA_WINDOW_TITLE)
        {
            *line = format!("; {}", line);
            return true;
        }
    }
    false
}

fn first_system_foreground_binding(lines: &[String]) -> Option<&str> {
    first_system_foreground_binding_entry(lines).map(|(_, value)| value)
}

fn first_system_foreground_binding_entry(lines: &[String]) -> Option<(usize, &str)> {
    let mut in_system = false;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if let Some(section) = section_name(trimmed) {
            in_system = section.eq_ignore_ascii_case("System");
            continue;
        }
        if !in_system {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(FOREGROUND_WINDOW_KEY) {
            return Some((idx, value.trim()));
        }
    }
    None
}

fn section_name(line: &str) -> Option<&str> {
    let inner = line.strip_prefix('[')?;
    let (section, _) = inner.split_once(']')?;
    Some(section.trim())
}

fn rotate_d3dx_backup(path: &Path) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("{} has no valid file name", path.display()))?;
    let backup = path.with_file_name(format!("{file_name}.bak"));
    if backup.exists() {
        let archive_dir = path.with_file_name(format!("{file_name}.bak - Hestia Backup"));
        fs::create_dir_all(&archive_dir).context(format!("creating {}", archive_dir.display()))?;
        let stamp = Utc::now().format("%Y%m%d-%H%M%S%.3f");
        let mut archived = archive_dir.join(format!("{file_name}.bak.{stamp}"));
        let mut suffix = 1usize;
        while archived.exists() {
            archived = archive_dir.join(format!("{file_name}.bak.{stamp}.{suffix}"));
            suffix += 1;
        }
        fs::rename(&backup, &archived).context(format!(
            "moving {} to {}",
            backup.display(),
            archived.display()
        ))?;
    }
    fs::copy(path, &backup).context(format!(
        "copying {} to {}",
        path.display(),
        backup.display()
    ))?;
    Ok(())
}

/// The virtual-key code of the importer's `reload_config` binding, parsed from
/// `d3dx.ini`. Defaults to F10, the binding every supported importer ships with.
#[cfg(windows)]
fn reload_hotkey_vk(importer_root: &Path) -> u16 {
    const VK_F1: u16 = 0x70;
    const DEFAULT: u16 = 0x79; // VK_F10
    let Ok(text) = fs::read_to_string(importer_root.join("d3dx.ini")) else {
        return DEFAULT;
    };
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("reload_config") else {
            continue;
        };
        let Some((_, binding)) = rest.split_once('=') else {
            continue;
        };
        for token in binding.split_whitespace() {
            let name = token.strip_prefix("VK_").unwrap_or(token);
            if let Some(number) = name.strip_prefix(['F', 'f'])
                && let Ok(index) = number.parse::<u16>()
                && (1..=24).contains(&index)
            {
                return VK_F1 + index - 1;
            }
        }
    }
    DEFAULT
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default)]
struct KeySpec {
    ctrl: bool,
    alt: bool,
    shift: bool,
    key: u16,
}

#[cfg(windows)]
fn vk_for_named_token(token: &str) -> Option<u16> {
    let token = token.to_ascii_lowercase();
    let name = token.strip_prefix("vk_").unwrap_or(&token);
    if let Some(number) = name.strip_prefix('f')
        && let Ok(index) = number.parse::<u16>()
        && (1..=24).contains(&index)
    {
        return Some(0x70 + index - 1);
    }
    match name {
        "back" => Some(0x08),
        "tab" => Some(0x09),
        "return" | "enter" => Some(0x0D),
        "shift" => Some(0x10),
        "control" | "ctrl" => Some(0x11),
        "menu" | "alt" => Some(0x12),
        "escape" | "esc" => Some(0x1B),
        "space" => Some(0x20),
        "prior" | "pageup" | "pgup" => Some(0x21),
        "next" | "pagedown" | "pgdn" => Some(0x22),
        "end" => Some(0x23),
        "home" => Some(0x24),
        "left" => Some(0x25),
        "up" => Some(0x26),
        "right" => Some(0x27),
        "down" => Some(0x28),
        "insert" | "ins" => Some(0x2D),
        "delete" | "del" => Some(0x2E),
        "oem_1" => Some(0xBA),
        "oem_plus" => Some(0xBB),
        "oem_comma" => Some(0xBC),
        "oem_minus" => Some(0xBD),
        "oem_period" => Some(0xBE),
        "oem_2" => Some(0xBF),
        "oem_3" => Some(0xC0),
        "oem_4" => Some(0xDB),
        "oem_5" => Some(0xDC),
        "oem_6" => Some(0xDD),
        "oem_7" => Some(0xDE),
        _ => None,
    }
}

#[cfg(windows)]
fn vk_for_char_token(token: &str) -> Option<(u16, bool)> {
    let mut chars = token.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let shift = c.is_ascii_uppercase();
    match c.to_ascii_uppercase() {
        'A'..='Z' => Some((c.to_ascii_uppercase() as u16, shift)),
        '0'..='9' => Some((c as u16, false)),
        ' ' => Some((0x20, false)),
        ';' => Some((0xBA, false)),
        '=' => Some((0xBB, false)),
        ',' => Some((0xBC, false)),
        '-' => Some((0xBD, false)),
        '.' => Some((0xBE, false)),
        '/' => Some((0xBF, false)),
        '`' => Some((0xC0, false)),
        '[' => Some((0xDB, false)),
        '\\' => Some((0xDC, false)),
        ']' => Some((0xDD, false)),
        '\'' => Some((0xDE, false)),
        _ => None,
    }
}

#[cfg(windows)]
fn parse_key_spec(raw: &str) -> Option<KeySpec> {
    let mut spec = KeySpec::default();
    for token in raw.split_whitespace() {
        let token_lc = token.to_ascii_lowercase();
        if token_lc.starts_with("no_") {
            continue;
        }
        match token_lc.as_str() {
            "ctrl" | "control" => {
                spec.ctrl = true;
                continue;
            }
            "alt" => {
                spec.alt = true;
                continue;
            }
            "shift" => {
                spec.shift = true;
                continue;
            }
            _ => {}
        }
        if spec.key != 0 {
            return None;
        }
        if let Some(vk) = vk_for_named_token(&token_lc) {
            spec.key = vk;
        } else if let Some((vk, needs_shift)) = vk_for_char_token(token) {
            spec.key = vk;
            spec.shift |= needs_shift;
        } else {
            return None;
        }
    }
    (spec.key != 0).then_some(spec)
}

#[cfg(windows)]
#[derive(Clone, Debug)]
enum ReloadForeground {
    Hestia {
        title: String,
    },
    Game {
        title: String,
        hwnd: windows::Win32::Foundation::HWND,
    },
    Other {
        title: String,
    },
    None,
}

#[cfg(windows)]
impl ReloadForeground {
    fn label(&self) -> String {
        match self {
            Self::Hestia { title } => format!("Hestia foreground ({title:?})"),
            Self::Game { title, .. } => format!("game foreground ({title:?})"),
            Self::Other { title } => format!("other foreground ({title:?})"),
            Self::None => "no foreground window".to_string(),
        }
    }
}

#[cfg(windows)]
fn foreground_window_title_and_pid() -> Option<(windows::Win32::Foundation::HWND, String, u32)> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let length = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buffer = vec![0u16; length.saturating_add(1)];
    let read = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    let title = String::from_utf16_lossy(&buffer[..read.max(0) as usize]);
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    Some((hwnd, title, pid))
}

fn game_exe_candidates(game: &GameInstall) -> Vec<String> {
    [game.vanilla_exe_path(), game.modded_exe_path()]
        .into_iter()
        .flatten()
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .flat_map(|name| {
            let lower = name.to_ascii_lowercase();
            let stem = Path::new(&name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_ascii_lowercase);
            stem.into_iter().chain(std::iter::once(lower))
        })
        .collect()
}

fn process_matches_game_candidates(
    process_name: &std::ffi::OsStr,
    command_line: &[std::ffi::OsString],
    candidates: &[String],
) -> bool {
    let process_name = process_name.to_string_lossy().to_ascii_lowercase();
    if candidates
        .iter()
        .any(|candidate| candidate == &process_name)
    {
        return true;
    }
    command_line.iter().any(|arg| {
        let arg = arg.to_string_lossy().to_ascii_lowercase();
        arg.split(['/', '\\'])
            .filter_map(|part| {
                let trimmed = part.trim_matches(['"', '\'']);
                (!trimmed.is_empty()).then_some(trimmed)
            })
            .any(|part| {
                candidates.iter().any(|candidate| {
                    part == candidate
                        || Path::new(part)
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .is_some_and(|stem| stem.eq_ignore_ascii_case(candidate))
                })
            })
    })
}

pub fn game_process_running_for_reload(game: &GameInstall) -> bool {
    let candidates = game_exe_candidates(game);
    if candidates.is_empty() {
        return false;
    }
    let system = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(
            sysinfo::ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet),
        ),
    );
    system.processes().values().any(|process| {
        process_matches_game_candidates(process.name(), process.cmd(), &candidates)
            || process
                .exe()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let lower = name.to_ascii_lowercase();
                    let stem = Path::new(name)
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(str::to_ascii_lowercase);
                    candidates
                        .iter()
                        .any(|candidate| candidate == &lower || stem.as_ref() == Some(candidate))
                })
    })
}

#[cfg(windows)]
fn foreground_for_reload(game: &GameInstall) -> ReloadForeground {
    let Some((hwnd, title, pid)) = foreground_window_title_and_pid() else {
        return ReloadForeground::None;
    };
    if title == HESTIA_WINDOW_TITLE {
        return ReloadForeground::Hestia { title };
    }
    let candidates = game_exe_candidates(game);
    if candidates.is_empty() {
        return ReloadForeground::Other { title };
    }
    let system = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(
            sysinfo::ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet),
        ),
    );
    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid))
        && (process_matches_game_candidates(process.name(), process.cmd(), &candidates)
            || process
                .exe()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let lower = name.to_ascii_lowercase();
                    let stem = Path::new(name)
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(str::to_ascii_lowercase);
                    candidates
                        .iter()
                        .any(|candidate| candidate == &lower || stem.as_ref() == Some(candidate))
                }))
    {
        return ReloadForeground::Game { title, hwnd };
    }
    ReloadForeground::Other { title }
}

#[cfg(windows)]
fn hestia_window() -> Option<windows::Win32::Foundation::HWND> {
    use windows::{Win32::UI::WindowsAndMessaging::FindWindowW, core::w};

    let hwnd = unsafe { FindWindowW(None, w!("Hestia")).ok()? };
    (!hwnd.0.is_null()).then_some(hwnd)
}

#[cfg(windows)]
fn set_foreground_window(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::{
        Foundation::HWND,
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
            SW_SHOWNORMAL, SetForegroundWindow, ShowWindow,
        },
    };

    if hwnd.0.is_null() || unsafe { !IsWindow(Some(hwnd)).as_bool() } {
        return false;
    }

    let current_foreground = unsafe { GetForegroundWindow() };
    let current_thread = unsafe { GetCurrentThreadId() };
    let target_thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
    let foreground_thread = if current_foreground.0.is_null() {
        0
    } else {
        unsafe { GetWindowThreadProcessId(current_foreground, None) }
    };
    let attached_foreground = foreground_thread != 0
        && foreground_thread != current_thread
        && unsafe { AttachThreadInput(current_thread, foreground_thread, true).as_bool() };
    let attached_target = target_thread != 0
        && target_thread != current_thread
        && target_thread != foreground_thread
        && unsafe { AttachThreadInput(current_thread, target_thread, true).as_bool() };

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = BringWindowToTop(hwnd);
    }
    let focused = unsafe { SetForegroundWindow(hwnd).as_bool() };

    if attached_target {
        unsafe {
            let _ = AttachThreadInput(current_thread, target_thread, false);
        }
    }
    if attached_foreground {
        unsafe {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
    }

    focused
        || foreground_window_title_and_pid()
            .is_some_and(|(foreground, _, _)| foreground == hwnd || foreground == HWND(hwnd.0))
}

#[cfg(windows)]
fn send_alt_foreground_unlock_tap() -> Result<bool> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
        KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_MENU,
    };

    let ctrl_down = unsafe { GetAsyncKeyState(i32::from(VK_CONTROL.0)) } < 0;
    let alt_down = unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0;
    if ctrl_down || alt_down {
        return Ok(false);
    }

    let key_input = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(VK_MENU.0),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key_input(KEYBD_EVENT_FLAGS(0)), key_input(KEYEVENTF_KEYUP)];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        return Err(anyhow!(
            "foreground unlock Alt tap delivered {sent} of {} inputs",
            inputs.len()
        ));
    }
    std::thread::sleep(Duration::from_millis(80));
    Ok(true)
}

#[cfg(windows)]
fn set_foreground_window_with_unlock(
    hwnd: windows::Win32::Foundation::HWND,
) -> Result<(bool, bool)> {
    if set_foreground_window(hwnd) {
        return Ok((true, false));
    }
    let unlocked = send_alt_foreground_unlock_tap()?;
    if !unlocked {
        return Ok((false, false));
    }
    Ok((set_foreground_window(hwnd), true))
}

#[cfg(windows)]
fn wait_for_foreground_window(
    hwnd: windows::Win32::Foundation::HWND,
    title: Option<&str>,
    timeout: Duration,
) -> bool {
    let started = std::time::Instant::now();
    loop {
        if foreground_window_title_and_pid().is_some_and(|(foreground, foreground_title, _)| {
            foreground == hwnd && title.is_none_or(|expected| foreground_title == expected)
        }) {
            return true;
        }
        if started.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(FOREGROUND_SETTLE_POLL);
    }
}

#[cfg(windows)]
fn foreground_is_game_or_hestia(game_hwnd: windows::Win32::Foundation::HWND) -> bool {
    foreground_window_title_and_pid().is_some_and(|(foreground, title, _)| {
        foreground == game_hwnd || title == HESTIA_WINDOW_TITLE
    })
}

#[cfg(windows)]
fn keyboard_key_down_for_reload() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    // Start at VK_BACK (0x08), intentionally skipping mouse buttons. This is a
    // physical-key idle gate for the foreground hand-off, not a general input lock.
    (0x08..=0xfe).any(|vk| unsafe { GetAsyncKeyState(vk) } < 0)
}

#[cfg(windows)]
fn wait_for_keyboard_idle(timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    let mut idle_since = None;
    loop {
        if keyboard_key_down_for_reload() {
            idle_since = None;
        } else {
            let now = std::time::Instant::now();
            let since = idle_since.get_or_insert(now);
            if now.duration_since(*since) >= KEYBOARD_IDLE_REQUIRED {
                return true;
            }
        }
        if started.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(KEYBOARD_IDLE_POLL);
    }
}

#[cfg(windows)]
fn send_reload_hotkey_burst(importer_root: &Path, pulses: u32) -> Result<bool> {
    for pulse in 0..pulses.max(1) {
        match send_reload_hotkey(importer_root)? {
            true => {}
            false => return Ok(false),
        }
        if pulse + 1 < pulses {
            std::thread::sleep(Duration::from_millis(RELOAD_KEY_PULSE_GAP_MS));
        }
    }
    Ok(true)
}

#[cfg(windows)]
fn send_reload_hotkey_via_hestia_focus_with_retry(
    importer_root: &Path,
    restore_hwnd: windows::Win32::Foundation::HWND,
) -> Result<FocusRouteOutcome> {
    let started = std::time::Instant::now();
    let mut attempts = 0;
    let mut used_unlock = false;
    let mut last_error;
    let mut waited_for_keyboard_idle = false;
    loop {
        attempts += 1;
        if !foreground_is_game_or_hestia(restore_hwnd) {
            return Err(anyhow!(
                "foreground changed away from the game/Hestia while waiting to route reload"
            ));
        }
        let elapsed = started.elapsed();
        if elapsed >= FOCUS_ROUTE_RETRY_TIMEOUT {
            return Err(anyhow!(
                "keyboard was not idle for {}ms before reload focus route timed out",
                KEYBOARD_IDLE_REQUIRED.as_millis()
            ));
        }
        if keyboard_key_down_for_reload() {
            waited_for_keyboard_idle = true;
            let remaining = FOCUS_ROUTE_RETRY_TIMEOUT.saturating_sub(elapsed);
            if !wait_for_keyboard_idle(remaining.min(Duration::from_millis(1_500))) {
                if started.elapsed() >= FOCUS_ROUTE_RETRY_TIMEOUT {
                    return Err(anyhow!(
                        "keyboard was not idle for {}ms before reload focus route timed out",
                        KEYBOARD_IDLE_REQUIRED.as_millis()
                    ));
                }
                continue;
            }
        }
        match send_reload_hotkey_via_hestia_focus(importer_root, restore_hwnd) {
            Ok(outcome) => {
                let attempt_used_unlock = outcome.used_unlock;
                used_unlock |= attempt_used_unlock;
                return Ok(FocusRouteOutcome {
                    sent: outcome.sent,
                    restored_focus: outcome.restored_focus,
                    used_unlock,
                    attempts,
                    waited_for_keyboard_idle,
                });
            }
            Err(error) => {
                last_error = Some(error);
                if started.elapsed() >= FOCUS_ROUTE_RETRY_TIMEOUT {
                    break;
                }
                std::thread::sleep(FOCUS_ROUTE_RETRY_DELAY);
            }
        }
    }

    Err(anyhow!(
        "Hestia window could not be foregrounded for reload after {attempts} attempt(s) over {}ms: {}",
        FOCUS_ROUTE_RETRY_TIMEOUT.as_millis(),
        last_error
            .map(|error| format!("{error:#}"))
            .unwrap_or_else(|| "unknown error".to_string())
    ))
}

#[cfg(windows)]
fn send_reload_hotkey_via_hestia_focus(
    importer_root: &Path,
    restore_hwnd: windows::Win32::Foundation::HWND,
) -> Result<FocusRouteOutcome> {
    let Some(hestia_hwnd) = hestia_window() else {
        return Err(anyhow!("Hestia window was not found"));
    };
    let (hestia_foregrounded, used_unlock) = set_foreground_window_with_unlock(hestia_hwnd)?;
    if !hestia_foregrounded {
        return Err(anyhow!(
            "Hestia window could not be foregrounded{}",
            if used_unlock {
                " even after Alt foreground unlock"
            } else {
                ""
            }
        ));
    }
    if !wait_for_foreground_window(
        hestia_hwnd,
        Some(HESTIA_WINDOW_TITLE),
        FOREGROUND_SETTLE_TIMEOUT,
    ) {
        return Err(anyhow!(
            "Hestia window did not become foreground before reload send"
        ));
    }
    suppress_next_synthetic_reload_key();
    let sent = send_reload_hotkey_burst(importer_root, RELOAD_KEY_PULSE_COUNT)?;
    std::thread::sleep(Duration::from_millis(80));
    let restored = set_foreground_window(restore_hwnd)
        && wait_for_foreground_window(restore_hwnd, None, FOREGROUND_SETTLE_TIMEOUT);
    Ok(FocusRouteOutcome {
        sent,
        restored_focus: restored,
        used_unlock,
        attempts: 1,
        waited_for_keyboard_idle: false,
    })
}

#[cfg(windows)]
fn focus_route_message(outcome: FocusRouteOutcome) -> String {
    let mut parts = vec!["sent reload hotkey via Hestia focus".to_string()];
    if outcome.waited_for_keyboard_idle {
        parts.push("waited for keyboard idle".to_string());
    }
    if outcome.attempts > 1 {
        parts.push(format!("focus attempts: {}", outcome.attempts));
    }
    if outcome.used_unlock {
        parts.push("used foreground unlock".to_string());
    }
    parts.push(format!("restored game focus: {}", outcome.restored_focus));
    parts.join("; ")
}

/// Foreground-aware profile reload sender. If the game is foreground, XXMI accepts normal
/// global input through its own `pid == GetCurrentProcessId()` foreground check. If Hestia is
/// foreground, this relies on the `additional_foreground_window = Hestia` d3dx.ini grant.
/// Other foreground windows get a short retry window to cover focus transitions.
#[cfg(windows)]
pub fn send_reload_hotkey_foreground_aware(
    game: &GameInstall,
    use_default: bool,
) -> Result<ReloadHotkeyReport> {
    let Some(importer_root) = importer_root_for(game, use_default) else {
        return Ok(ReloadHotkeyReport {
            message: "skipped: importer root was not found".to_string(),
        });
    };

    let mut last_foreground = ReloadForeground::None;
    for attempt in 1..=FOREGROUND_RELOAD_ATTEMPTS {
        let foreground = foreground_for_reload(game);
        let label = foreground.label();
        match foreground {
            ReloadForeground::Game { hwnd, .. } => {
                if !reload_hotkey_supported(&importer_root) {
                    let sent = send_reload_hotkey_burst(&importer_root, RELOAD_KEY_PULSE_COUNT)?;
                    return Ok(ReloadHotkeyReport {
                        message: if sent {
                            format!("sent reload hotkey; {label}; direct game foreground path")
                        } else {
                            format!("skipped on attempt {attempt}; modifier guard active; {label}")
                        },
                    });
                }
                let outcome = send_reload_hotkey_via_hestia_focus_with_retry(&importer_root, hwnd)?;
                return Ok(ReloadHotkeyReport {
                    message: if outcome.sent {
                        focus_route_message(outcome)
                    } else {
                        format!("skipped on attempt {attempt}; modifier guard active; {label}")
                    },
                });
            }
            ReloadForeground::Hestia { .. } => {
                if !reload_hotkey_supported(&importer_root) {
                    return Ok(ReloadHotkeyReport {
                        message: format!(
                            "skipped: Hestia is foreground but additional_foreground_window is unavailable; {label}"
                        ),
                    });
                }
                suppress_next_synthetic_reload_key();
                let sent = send_reload_hotkey_burst(&importer_root, RELOAD_KEY_PULSE_COUNT)?;
                return Ok(ReloadHotkeyReport {
                    message: if sent {
                        format!("sent reload hotkey; {label}")
                    } else {
                        format!("skipped on attempt {attempt}; modifier guard active; {label}")
                    },
                });
            }
            other => {
                last_foreground = other;
                if attempt < FOREGROUND_RELOAD_ATTEMPTS {
                    std::thread::sleep(FOREGROUND_RELOAD_RETRY_DELAY);
                }
            }
        }
    }

    Ok(ReloadHotkeyReport {
        message: format!(
            "skipped: foreground was not Hestia or the game after {FOREGROUND_RELOAD_ATTEMPTS} attempts; last {}",
            last_foreground.label()
        ),
    })
}

#[cfg(not(windows))]
pub fn send_reload_hotkey_foreground_aware(
    _game: &GameInstall,
    _use_default: bool,
) -> Result<ReloadHotkeyReport> {
    Ok(ReloadHotkeyReport {
        message: "skipped: reload hotkey sending is only supported on Windows".to_string(),
    })
}

#[cfg(windows)]
fn send_key_spec(spec: KeySpec) -> Result<bool> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
        KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT,
    };

    let ctrl_down = unsafe { GetAsyncKeyState(i32::from(VK_CONTROL.0)) } < 0;
    let alt_down = unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0;
    let shift_down = unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) } < 0;
    if ctrl_down || alt_down || shift_down {
        return Ok(false);
    }

    let key_input = |vk: u16, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let send_one = |input: INPUT, label: &str| -> Result<()> {
        let inputs = [input];
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(anyhow!("SendInput delivered {sent} of 1 {label} input"));
        }
        Ok(())
    };

    send_one(key_input(spec.key, KEYEVENTF_KEYUP), "hotkey settle-up")?;
    std::thread::sleep(std::time::Duration::from_millis(RELOAD_KEY_SETTLE_UP_MS));

    let mut modifiers = Vec::new();
    if spec.ctrl {
        modifiers.push(VK_CONTROL.0);
    }
    if spec.alt {
        modifiers.push(VK_MENU.0);
    }
    if spec.shift {
        modifiers.push(VK_SHIFT.0);
    }
    for modifier in &modifiers {
        send_one(
            key_input(*modifier, KEYBD_EVENT_FLAGS(0)),
            "hotkey modifier down",
        )?;
    }
    send_one(key_input(spec.key, KEYBD_EVENT_FLAGS(0)), "hotkey key down")?;
    std::thread::sleep(std::time::Duration::from_millis(RELOAD_KEY_HOLD_MS));
    let key_up_result = send_one(key_input(spec.key, KEYEVENTF_KEYUP), "hotkey key up");
    for modifier in modifiers.iter().rev() {
        let _ = send_one(key_input(*modifier, KEYEVENTF_KEYUP), "hotkey modifier up");
    }
    key_up_result?;
    Ok(true)
}

#[cfg(windows)]
pub fn send_mod_hotkey_foreground_aware(
    game: &GameInstall,
    use_default: bool,
    key_spec: &str,
) -> Result<ReloadHotkeyReport> {
    let Some(importer_root) = importer_root_for(game, use_default) else {
        return Ok(ReloadHotkeyReport {
            message: "skipped: importer root was not found".to_string(),
        });
    };
    let Some(spec) = parse_key_spec(key_spec) else {
        return Ok(ReloadHotkeyReport {
            message: format!("skipped: unsupported hotkey binding {key_spec:?}"),
        });
    };

    let mut last_foreground = ReloadForeground::None;
    for attempt in 1..=FOREGROUND_RELOAD_ATTEMPTS {
        let foreground = foreground_for_reload(game);
        let label = foreground.label();
        match foreground {
            ReloadForeground::Game { .. } => {
                let sent = send_key_spec(spec)?;
                return Ok(ReloadHotkeyReport {
                    message: if sent {
                        format!("sent mod hotkey; {label}; direct game foreground path")
                    } else {
                        format!("skipped on attempt {attempt}; modifier guard active; {label}")
                    },
                });
            }
            ReloadForeground::Hestia { .. } => {
                if !reload_hotkey_supported(&importer_root) {
                    return Ok(ReloadHotkeyReport {
                        message: format!(
                            "skipped: Hestia is foreground but additional_foreground_window is unavailable; {label}"
                        ),
                    });
                }
                let sent = send_key_spec(spec)?;
                return Ok(ReloadHotkeyReport {
                    message: if sent {
                        format!("sent mod hotkey; {label}")
                    } else {
                        format!("skipped on attempt {attempt}; modifier guard active; {label}")
                    },
                });
            }
            other => {
                last_foreground = other;
                if attempt < FOREGROUND_RELOAD_ATTEMPTS {
                    std::thread::sleep(FOREGROUND_RELOAD_RETRY_DELAY);
                }
            }
        }
    }

    Ok(ReloadHotkeyReport {
        message: format!(
            "skipped: foreground was not Hestia or the game after {FOREGROUND_RELOAD_ATTEMPTS} attempts; last {}",
            last_foreground.label()
        ),
    })
}

#[cfg(not(windows))]
pub fn send_mod_hotkey_foreground_aware(
    _game: &GameInstall,
    _use_default: bool,
    _key_spec: &str,
) -> Result<ReloadHotkeyReport> {
    Ok(ReloadHotkeyReport {
        message: "skipped: mod hotkey sending is only supported on Windows".to_string(),
    })
}

static SYNTHETIC_RELOAD_KEY_SUPPRESS_UNTIL: std::sync::OnceLock<
    std::sync::Mutex<Option<std::time::Instant>>,
> = std::sync::OnceLock::new();

pub fn suppress_next_synthetic_reload_key() {
    let lock = SYNTHETIC_RELOAD_KEY_SUPPRESS_UNTIL.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(mut until) = lock.lock() {
        *until = Some(std::time::Instant::now() + Duration::from_millis(2500));
    }
}

pub fn take_synthetic_reload_key_suppression() -> bool {
    let Some(lock) = SYNTHETIC_RELOAD_KEY_SUPPRESS_UNTIL.get() else {
        return false;
    };
    let Ok(mut until) = lock.lock() else {
        return false;
    };
    match *until {
        Some(expires_at) if std::time::Instant::now() < expires_at => true,
        Some(_) => {
            *until = None;
            false
        }
        None => false,
    }
}

/// Send the importer's reload hotkey so a running game picks up changed live mods or a
/// just-written `d3dx_user.ini`. Returns `Ok(false)` when the send was skipped.
///
/// Hard guard: `wipe_user_config = ctrl alt no_shift VK_F10` means an F10 delivered while
/// Ctrl and Alt happen to be held erases every persisted setting for the importer, so the
/// send is skipped outright whenever either modifier is physically down. A skipped reload
/// is benign — the next launch reads the file anyway.
#[cfg(windows)]
pub fn send_reload_hotkey(importer_root: &Path) -> Result<bool> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
        KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_MENU,
    };

    let ctrl_down = unsafe { GetAsyncKeyState(i32::from(VK_CONTROL.0)) } < 0;
    let alt_down = unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0;
    if ctrl_down || alt_down {
        return Ok(false);
    }
    let vk = VIRTUAL_KEY(reload_hotkey_vk(importer_root));
    let key_input = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let send_one = |input: INPUT, label: &str| -> Result<()> {
        let inputs = [input];
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(anyhow!("SendInput delivered {sent} of 1 {label} input"));
        }
        Ok(())
    };

    send_one(key_input(KEYEVENTF_KEYUP), "reload key settle-up")?;
    std::thread::sleep(std::time::Duration::from_millis(RELOAD_KEY_SETTLE_UP_MS));

    let ctrl_down = unsafe { GetAsyncKeyState(i32::from(VK_CONTROL.0)) } < 0;
    let alt_down = unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0;
    if ctrl_down || alt_down {
        return Ok(false);
    }

    send_one(key_input(KEYBD_EVENT_FLAGS(0)), "reload key down")?;
    std::thread::sleep(std::time::Duration::from_millis(RELOAD_KEY_HOLD_MS));
    if let Err(err) = send_one(key_input(KEYEVENTF_KEYUP), "reload key up") {
        return Err(anyhow!(
            "reload key was pressed but release failed: {err:#}"
        ));
    }
    Ok(true)
}

#[cfg(not(windows))]
pub fn send_reload_hotkey(_importer_root: &Path) -> Result<bool> {
    Ok(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "; AUTOMATICALLY GENERATED FILE - DO NOT EDIT\r\n\
;\r\n\
[Constants]\r\n\
$\\efmiv1\\first_run = 0\r\n\
$\\mods\\arcane：libertas & gui_animation\\arcane.ini\\swapkey0 = 0\r\n\
$\\mods\\tangtang seaside breeze 1.2\\inner\\tangtang.ini\\tail = 1\r\n\
$\\mods\\tangtang seaside breeze 1.2\\inner\\tangtang.ini\\final_x_off = 0.720000029\r\n\
$\\mods\\other mod\\other.ini\\value = 3\r\n";

    fn doc() -> UserIniDoc {
        UserIniDoc::from_text(SAMPLE)
    }

    fn view<'a>(
        root: &'a Path,
        folder: &'a str,
        status: ModStatus,
        archive_original: Option<&'a Path>,
    ) -> ModPersistView<'a> {
        ModPersistView {
            root_path: root,
            folder_name: folder,
            status,
            archive_original_path: archive_original,
        }
    }

    fn game(mods_path: PathBuf) -> GameInstall {
        GameInstall {
            definition: crate::model::GameDefinition {
                id: "test".to_string(),
                name: "Test".to_string(),
                backend: GameBackend::Xxmi,
                xxmi_code: "TEST".to_string(),
            },
            mods_path_override: Some(mods_path),
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: true,
            enabled: true,
        }
    }

    fn mod_entry(root_path: PathBuf, status: ModStatus) -> ModEntry {
        ModEntry {
            id: "mod".to_string(),
            game_id: "test".to_string(),
            folder_name: "Arcane".to_string(),
            root_path,
            status,
            metadata: crate::model::ModMetadata::default(),
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            source: None,
            update_state: crate::model::ModUpdateState::Unlinked,
        }
    }

    fn test_roots() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let importer_root = temp.path().to_path_buf();
        let mods_root = importer_root.join("Mods");
        fs::create_dir_all(&mods_root).unwrap();
        fs::write(importer_root.join("d3dx.ini"), "[Include]\n").unwrap();
        (temp, importer_root, mods_root)
    }

    #[test]
    fn roundtrip_preserves_untouched_text() {
        let doc = doc();
        assert_eq!(doc.to_text(), SAMPLE);
        assert!(!doc.dirty);
    }

    #[test]
    fn take_prefix_removes_only_matching_lines() {
        let mut doc = doc();
        let taken = doc.take_prefix("$\\mods\\Tangtang Seaside Breeze 1.2\\");
        assert_eq!(
            taken,
            vec![
                ("inner\\tangtang.ini\\tail".to_string(), "1".to_string()),
                (
                    "inner\\tangtang.ini\\final_x_off".to_string(),
                    "0.720000029".to_string()
                ),
            ]
        );
        assert!(doc.dirty);
        let text = doc.to_text();
        assert!(!text.contains("tangtang"));
        assert!(text.contains("$\\efmiv1\\first_run = 0"));
        assert!(text.contains("$\\mods\\other mod\\other.ini\\value = 3"));
        assert!(text.contains("arcane"));
    }

    #[test]
    fn merge_replaces_in_place_and_appends_new_keys() {
        let mut doc = doc();
        doc.merge_prefix(
            "$\\mods\\other mod\\",
            &[
                ("other.ini\\value".to_string(), "9".to_string()),
                ("other.ini\\fresh".to_string(), "7".to_string()),
            ],
        );
        let text = doc.to_text();
        assert!(text.contains("$\\mods\\other mod\\other.ini\\value = 9"));
        assert!(text.contains("$\\mods\\other mod\\other.ini\\fresh = 7"));
        assert_eq!(text.matches("other.ini\\value").count(), 1);
    }

    #[test]
    fn set_mod_variable_updates_existing_ini_relative_key() {
        let (_temp, importer_root, mods_root) = test_roots();
        let mod_root = mods_root.join("Arcane");
        fs::create_dir_all(&mod_root).unwrap();
        fs::write(
            importer_root.join(USER_INI_FILE),
            "[Constants]\r\n$\\mods\\arcane\\mod.ini\\swap = 0\r\n",
        )
        .unwrap();
        let game = game(mods_root);
        let entry = mod_entry(mod_root, ModStatus::Active);

        let wrote = set_mod_variable(&game, false, &entry, "mod.ini", "swap", "2").unwrap();

        assert!(wrote);
        let text = fs::read_to_string(importer_root.join(USER_INI_FILE)).unwrap();
        assert!(text.contains("$\\mods\\arcane\\mod.ini\\swap = 2"));
        assert!(!text.contains("$\\mods\\arcane\\swap = 2"));
    }

    #[test]
    fn set_mod_variable_appends_ini_relative_key_when_missing() {
        let (_temp, importer_root, mods_root) = test_roots();
        let mod_root = mods_root.join("Arcane");
        fs::create_dir_all(&mod_root).unwrap();
        fs::write(importer_root.join(USER_INI_FILE), "[Constants]\r\n").unwrap();
        let game = game(mods_root);
        let entry = mod_entry(mod_root, ModStatus::Active);

        let wrote = set_mod_variable(&game, false, &entry, "mod.ini", "swap", "1").unwrap();

        assert!(wrote);
        let text = fs::read_to_string(importer_root.join(USER_INI_FILE)).unwrap();
        assert!(text.contains("$\\mods\\arcane\\mod.ini\\swap = 1"));
    }

    #[test]
    fn set_mod_variable_writes_disabled_mod_stash() {
        let (_temp, importer_root, mods_root) = test_roots();
        let mod_root = mods_root.join("Arcane");
        fs::create_dir_all(&mod_root).unwrap();
        fs::write(importer_root.join(USER_INI_FILE), "[Constants]\r\n").unwrap();
        let game = game(mods_root);
        let entry = mod_entry(mod_root, ModStatus::Disabled);

        let wrote = set_mod_variable(&game, false, &entry, "mod.ini", "swap", "1").unwrap();

        assert!(wrote);
        let text = fs::read_to_string(importer_root.join(USER_INI_FILE)).unwrap();
        assert!(!text.contains("swap = 1"));
        let stash = read_stash(&entry.root_path).unwrap();
        assert_eq!(stash.source_prefix, "$\\mods\\arcane\\");
        assert_eq!(
            stash
                .full_entries
                .iter()
                .find(|entry| entry.key == "$\\mods\\arcane\\mod.ini\\swap")
                .map(|entry| entry.value.as_str()),
            Some("1")
        );
    }

    #[test]
    fn set_mod_variable_writes_archived_mod_stash() {
        let (_temp, importer_root, mods_root) = test_roots();
        let archived_root = importer_root.join("Mods_Archived").join("Arcane");
        fs::create_dir_all(&archived_root).unwrap();
        fs::write(importer_root.join(USER_INI_FILE), "[Constants]\r\n").unwrap();
        let game = game(mods_root);
        let entry = mod_entry(archived_root, ModStatus::Archived);

        let wrote = set_mod_variable(&game, false, &entry, "mod.ini", "swap", "1").unwrap();

        assert!(wrote);
        let text = fs::read_to_string(importer_root.join(USER_INI_FILE)).unwrap();
        assert!(!text.contains("swap = 1"));
        let stash = read_stash(&entry.root_path).unwrap();
        assert_eq!(stash.source_prefix, "$\\mods\\arcane\\");
        assert_eq!(
            stash
                .full_entries
                .iter()
                .find(|entry| entry.key == "$\\mods\\arcane\\mod.ini\\swap")
                .map(|entry| entry.value.as_str()),
            Some("1")
        );
    }

    #[test]
    fn read_mod_variables_returns_relative_keys_for_active_mod() {
        let (_temp, importer_root, mods_root) = test_roots();
        let mod_root = mods_root.join("Arcane");
        fs::create_dir_all(&mod_root).unwrap();
        fs::write(
            importer_root.join(USER_INI_FILE),
            "[Constants]\r\n$\\mods\\arcane\\mod.ini\\swap = 2\r\n",
        )
        .unwrap();
        let game = game(mods_root);
        let entry = mod_entry(mod_root, ModStatus::Active);

        let values = read_mod_variables(&game, false, &entry).unwrap();

        assert_eq!(values.get("mod.ini\\swap").map(String::as_str), Some("2"));
    }

    #[test]
    fn read_mod_variables_returns_disabled_stash_values() {
        let (_temp, _importer_root, mods_root) = test_roots();
        let mod_root = mods_root.join("Arcane");
        fs::create_dir_all(&mod_root).unwrap();
        let game = game(mods_root);
        let entry = mod_entry(mod_root, ModStatus::Disabled);
        set_mod_variable(&game, false, &entry, "mod.ini", "swap", "1").unwrap();

        let values = read_mod_variables(&game, false, &entry).unwrap();

        assert_eq!(values.get("mod.ini\\swap").map(String::as_str), Some("1"));
    }

    #[test]
    fn read_mod_variables_returns_archived_stash_values() {
        let (_temp, importer_root, mods_root) = test_roots();
        let archived_root = importer_root.join("Mods_Archived").join("Arcane");
        fs::create_dir_all(&archived_root).unwrap();
        let game = game(mods_root);
        let entry = mod_entry(archived_root, ModStatus::Archived);
        set_mod_variable(&game, false, &entry, "mod.ini", "swap", "1").unwrap();

        let values = read_mod_variables(&game, false, &entry).unwrap();

        assert_eq!(values.get("mod.ini\\swap").map(String::as_str), Some("1"));
    }

    #[cfg(windows)]
    #[test]
    fn parses_simple_mod_hotkey_specs() {
        let shift_slash = parse_key_spec("shift /").unwrap();
        assert!(shift_slash.shift);
        assert_eq!(shift_slash.key, 0xBF);
        let no_mod_period = parse_key_spec("no_modifiers .").unwrap();
        assert!(!no_mod_period.ctrl);
        assert!(!no_mod_period.alt);
        assert!(!no_mod_period.shift);
        assert_eq!(no_mod_period.key, 0xBE);
        let ctrl_four = parse_key_spec("ctrl 4").unwrap();
        assert!(ctrl_four.ctrl);
        assert_eq!(ctrl_four.key, 0x34);
    }

    #[test]
    fn merge_identical_value_stays_clean() {
        let mut doc = doc();
        doc.merge_prefix(
            "$\\mods\\other mod\\",
            &[("other.ini\\value".to_string(), "3".to_string())],
        );
        assert!(!doc.dirty);
        assert_eq!(doc.to_text(), SAMPLE);
    }

    #[test]
    fn merge_creates_constants_section_when_missing() {
        let mut doc = UserIniDoc::from_text("");
        doc.merge_prefix("$\\mods\\m\\", &[("a.ini\\k".to_string(), "1".to_string())]);
        let text = doc.to_text();
        assert!(text.contains(USER_INI_HEADER));
        assert!(text.contains(CONSTANTS_SECTION));
        assert!(text.contains("$\\mods\\m\\a.ini\\k = 1"));
    }

    #[test]
    fn move_prefix_rewrites_keys_verbatim_values() {
        let mut doc = doc();
        let moved = doc.move_prefix(
            "$\\mods\\tangtang seaside breeze 1.2\\",
            "$\\mods\\renamed breeze\\",
        );
        assert_eq!(moved, 2);
        let text = doc.to_text();
        assert!(
            text.contains(
                "$\\mods\\renamed breeze\\inner\\tangtang.ini\\final_x_off = 0.720000029"
            )
        );
        assert!(!text.contains("tangtang seaside breeze 1.2"));
    }

    #[test]
    fn ci_prefix_len_handles_unicode() {
        let key = "$\\mods\\汤汤 QINGBO\\mod.ini\\tail";
        let len = ci_prefix_len(key, "$\\mods\\汤汤 qingbo\\").unwrap();
        assert_eq!(&key[len..], "mod.ini\\tail");
        assert!(ci_prefix_len(key, "$\\mods\\other\\").is_none());
    }

    #[test]
    fn namespace_prefix_derivation() {
        let importer = Path::new("C:\\XXMI\\EFMI");
        let live = Path::new("C:\\XXMI\\EFMI\\Mods\\My Mod NAME");
        assert_eq!(
            namespace_prefix_for_root(live, importer).unwrap(),
            "$\\mods\\my mod name\\"
        );
        let outside = Path::new("D:\\elsewhere\\My Mod");
        assert!(namespace_prefix_for_root(outside, importer).is_none());
        assert!(namespace_prefix_for_root(importer, importer).is_none());
    }

    #[test]
    fn importer_root_resolution_prefers_d3dx_ini() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("EFMI");
        let mods = root.join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        assert!(importer_root_from_mods_path(&mods).is_none());
        std::fs::write(root.join("d3d11.dll"), b"dll").unwrap();
        assert_eq!(importer_root_from_mods_path(&mods).unwrap(), root);
        // A d3dx.ini further up still wins over a nearer d3d11.dll.
        std::fs::write(temp.path().join("d3dx.ini"), b"ini").unwrap();
        assert_eq!(importer_root_from_mods_path(&mods).unwrap(), temp.path());
    }

    fn tx_for(temp: &tempfile::TempDir) -> (PathBuf, PathBuf, PersistTx) {
        let root = temp.path().join("EFMI");
        let mods = root.join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(root.join("d3dx.ini"), b"ini").unwrap();
        std::fs::write(root.join(USER_INI_FILE), SAMPLE.as_bytes()).unwrap();
        let doc = UserIniDoc::open(&root).unwrap().unwrap();
        let tx = PersistTx {
            importer_root: root.clone(),
            mods_root: Some(mods.clone()),
            doc,
            doc_unreadable: false,
            warnings: Vec::new(),
            shared_explicit_prefixes: Vec::new(),
            explicit_prefixes_by_root: HashMap::new(),
        };
        (root, mods, tx)
    }

    #[test]
    fn capture_live_writes_stash_and_removes_entries() {
        let temp = tempfile::tempdir().unwrap();
        let (root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Tangtang Seaside Breeze 1.2");
        std::fs::create_dir_all(&mod_root).unwrap();
        let view = view(
            &mod_root,
            "Tangtang Seaside Breeze 1.2",
            ModStatus::Active,
            None,
        );
        assert!(tx.capture_view(&view, CaptureMode::Stash).unwrap());
        let stash = read_stash(&mod_root).unwrap();
        assert_eq!(stash.entries.len(), 2);
        assert_eq!(stash.entries[0].key, "inner\\tangtang.ini\\tail");
        assert_eq!(
            stash.source_prefix,
            "$\\mods\\tangtang seaside breeze 1.2\\"
        );
        let outcome = tx.commit().unwrap();
        assert!(outcome.wrote);
        let text = std::fs::read_to_string(root.join(USER_INI_FILE)).unwrap();
        assert!(!text.contains("tangtang"));
        assert!(text.contains("$\\efmiv1\\first_run = 0"));
    }

    #[test]
    fn capture_live_includes_explicit_namespace_entries() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Liino Personal Knit");
        std::fs::create_dir_all(mod_root.join("K")).unwrap();
        std::fs::write(
            mod_root.join("mod.ini"),
            "namespace = liino\r\n[Constants]\r\n",
        )
        .unwrap();
        std::fs::write(mod_root.join("K").join("1.ini"), "[Constants]\r\n").unwrap();
        tx.doc.merge_full_entries(&[
            ("$\\liino\\swapkey0".to_string(), "1".to_string()),
            (
                "$\\mods\\liino personal knit\\k\\1.ini\\z".to_string(),
                "2".to_string(),
            ),
        ]);
        tx.doc.dirty = false;

        let view = view(&mod_root, "Liino Personal Knit", ModStatus::Active, None);
        assert!(tx.capture_view(&view, CaptureMode::Stash).unwrap());
        let text = tx.doc.to_text();
        assert!(!text.contains("$\\liino\\swapkey0"));
        assert!(!text.contains("liino personal knit\\k\\1.ini\\z"));
        let stash = read_stash(&mod_root).unwrap();
        assert!(
            stash
                .full_entries
                .iter()
                .any(|entry| entry.key == "$\\liino\\swapkey0")
        );
        assert!(
            stash
                .full_entries
                .iter()
                .any(|entry| entry.key == "$\\mods\\liino personal knit\\k\\1.ini\\z")
        );
    }

    #[test]
    fn capture_live_copies_shared_explicit_namespace_entries_without_taking_them() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        tx.set_shared_explicit_prefixes(vec!["$\\rabbitfx\\".to_string()]);
        let mod_root = mods.join("Rabbit Addon");
        std::fs::create_dir_all(&mod_root).unwrap();
        std::fs::write(
            mod_root.join("mod.ini"),
            "namespace = rabbitfx\r\n[Constants]\r\n",
        )
        .unwrap();
        tx.doc.merge_full_entries(&[
            ("$\\rabbitfx\\quality".to_string(), "3".to_string()),
            (
                "$\\mods\\rabbit addon\\mod.ini\\enabled".to_string(),
                "1".to_string(),
            ),
        ]);
        tx.doc.dirty = false;

        let view = view(&mod_root, "Rabbit Addon", ModStatus::Active, None);
        assert!(tx.capture_view(&view, CaptureMode::Stash).unwrap());
        let text = tx.doc.to_text();
        assert!(text.contains("$\\rabbitfx\\quality = 3"));
        assert!(!text.contains("$\\mods\\rabbit addon\\mod.ini\\enabled"));
        let stash = read_stash(&mod_root).unwrap();
        assert!(
            stash
                .full_entries
                .iter()
                .any(|entry| entry.key == "$\\rabbitfx\\quality")
        );
        assert!(
            stash
                .full_entries
                .iter()
                .any(|entry| entry.key == "$\\mods\\rabbit addon\\mod.ini\\enabled")
        );
    }

    #[test]
    fn capture_purge_removes_without_stash() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Other Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        let view = view(&mod_root, "Other Mod", ModStatus::Active, None);
        assert!(tx.capture_view(&view, CaptureMode::Purge).unwrap());
        assert!(read_stash(&mod_root).is_none());
        assert!(tx.doc.dirty);
    }

    #[test]
    fn capture_live_empty_without_stash_is_noop() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("No Settings Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        let view = view(&mod_root, "No Settings Mod", ModStatus::Active, None);
        assert!(!tx.capture_view(&view, CaptureMode::Stash).unwrap());
        assert!(read_stash(&mod_root).is_none());
        assert!(!tx.doc.dirty);
    }

    #[test]
    fn capture_live_empty_with_stash_keeps_last_known_values() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Wiped Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\wiped mod\\".to_string(),
                captured_at: Utc::now(),
                entries: vec![StashEntry {
                    key: "a.ini\\old".to_string(),
                    value: "5".to_string(),
                }],
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let view = view(&mod_root, "Wiped Mod", ModStatus::Active, None);
        assert!(!tx.capture_view(&view, CaptureMode::Stash).unwrap());
        let stash = read_stash(&mod_root).unwrap();
        assert_eq!(stash.entries.len(), 1);
        assert_eq!(stash.entries[0].key, "a.ini\\old");
        assert_eq!(stash.entries[0].value, "5");
    }

    #[test]
    fn capture_hidden_empty_keeps_stash() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Hidden Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        let original = ModStash {
            version: STASH_FORMAT_VERSION,
            source_prefix: "$\\mods\\hidden mod\\".to_string(),
            captured_at: Utc::now(),
            entries: vec![StashEntry {
                key: "a.ini\\kept".to_string(),
                value: "2".to_string(),
            }],
            full_entries: Vec::new(),
        };
        write_stash(&mod_root, &original).unwrap();
        let view = view(&mod_root, "Hidden Mod", ModStatus::Disabled, None);
        assert!(!tx.capture_view(&view, CaptureMode::Stash).unwrap());
        let stash = read_stash(&mod_root).unwrap();
        assert_eq!(stash.entries, original.entries);
    }

    #[test]
    fn restore_hidden_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Hidden Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\hidden mod\\".to_string(),
                captured_at: Utc::now(),
                entries: vec![StashEntry {
                    key: "a.ini\\k".to_string(),
                    value: "1".to_string(),
                }],
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let view = view(&mod_root, "Hidden Mod", ModStatus::Archived, None);
        assert!(!tx.restore_view(&view).unwrap());
        assert!(!tx.doc.dirty);
    }

    #[test]
    fn restore_reroutes_stale_prefix_first() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        // The file still holds entries under the OLD name; the stash was captured there too.
        let mod_root = mods.join("Renamed Breeze");
        std::fs::create_dir_all(&mod_root).unwrap();
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\tangtang seaside breeze 1.2\\".to_string(),
                captured_at: Utc::now(),
                entries: vec![StashEntry {
                    key: "inner\\tangtang.ini\\tail".to_string(),
                    value: "4".to_string(),
                }],
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let view = view(&mod_root, "Renamed Breeze", ModStatus::Active, None);
        assert!(tx.restore_view(&view).unwrap());
        let text = tx.doc.to_text();
        assert!(text.contains("$\\mods\\renamed breeze\\inner\\tangtang.ini\\tail = 4"));
        assert!(
            text.contains(
                "$\\mods\\renamed breeze\\inner\\tangtang.ini\\final_x_off = 0.720000029"
            )
        );
        assert!(!text.contains("tangtang seaside breeze 1.2\\inner"));
        let stash = read_stash(&mod_root).unwrap();
        assert_eq!(stash.source_prefix, "$\\mods\\renamed breeze\\");
    }

    #[test]
    fn restore_preserves_explicit_namespace_entries() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Liino Personal Knit");
        std::fs::create_dir_all(&mod_root).unwrap();
        std::fs::write(
            mod_root.join("mod.ini"),
            "namespace = liino\r\n[Constants]\r\n",
        )
        .unwrap();
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\liino personal knit\\".to_string(),
                captured_at: Utc::now(),
                entries: Vec::new(),
                full_entries: vec![
                    StashEntry {
                        key: "$\\liino\\swapkey7".to_string(),
                        value: "1".to_string(),
                    },
                    StashEntry {
                        key: "$\\mods\\liino personal knit\\k\\1.ini\\z".to_string(),
                        value: "2".to_string(),
                    },
                ],
            },
        )
        .unwrap();

        let view = view(&mod_root, "Liino Personal Knit", ModStatus::Active, None);
        assert!(tx.restore_view(&view).unwrap());
        let text = tx.doc.to_text();
        assert!(text.contains("$\\liino\\swapkey7 = 1"));
        assert!(text.contains("$\\mods\\liino personal knit\\k\\1.ini\\z = 2"));
    }

    #[test]
    fn restore_skips_shared_explicit_namespace_entries_from_old_stash() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        tx.set_shared_explicit_prefixes(vec!["$\\rabbitfx\\".to_string()]);
        let mod_root = mods.join("Rabbit Addon");
        std::fs::create_dir_all(&mod_root).unwrap();
        std::fs::write(
            mod_root.join("mod.ini"),
            "namespace = rabbitfx\r\n[Constants]\r\n",
        )
        .unwrap();
        tx.doc
            .merge_full_entries(&[("$\\rabbitfx\\quality".to_string(), "9".to_string())]);
        tx.doc.dirty = false;
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\rabbit addon\\".to_string(),
                captured_at: Utc::now(),
                entries: Vec::new(),
                full_entries: vec![
                    StashEntry {
                        key: "$\\rabbitfx\\quality".to_string(),
                        value: "3".to_string(),
                    },
                    StashEntry {
                        key: "$\\mods\\rabbit addon\\mod.ini\\enabled".to_string(),
                        value: "1".to_string(),
                    },
                ],
            },
        )
        .unwrap();

        let view = view(&mod_root, "Rabbit Addon", ModStatus::Active, None);
        assert!(tx.restore_view(&view).unwrap());
        let text = tx.doc.to_text();
        assert!(text.contains("$\\rabbitfx\\quality = 9"));
        assert!(text.contains("$\\mods\\rabbit addon\\mod.ini\\enabled = 1"));
    }

    #[test]
    fn reroute_hidden_absorbs_entries_into_stash() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Renamed Breeze");
        std::fs::create_dir_all(&mod_root).unwrap();
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\tangtang seaside breeze 1.2\\".to_string(),
                captured_at: Utc::now(),
                entries: Vec::new(),
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let view = view(&mod_root, "Renamed Breeze", ModStatus::Disabled, None);
        assert!(tx.reroute_view(&view).unwrap());
        let stash = read_stash(&mod_root).unwrap();
        assert_eq!(stash.source_prefix, "$\\mods\\renamed breeze\\");
        assert_eq!(
            stash.entries.len(),
            2,
            "live leftovers absorbed into the stash"
        );
        assert!(!tx.doc.to_text().contains("tangtang"));
    }

    #[test]
    fn archived_capture_never_takes_from_the_document() {
        // A live mod named "Other Mod" owns the prefix; an archived mod with the same
        // folder name must not steal its entries on capture.
        let temp = tempfile::tempdir().unwrap();
        let (_root, _mods, mut tx) = tx_for(&temp);
        let archived_root = temp
            .path()
            .join("EFMI")
            .join("Mods_Archived")
            .join("Other Mod");
        std::fs::create_dir_all(&archived_root).unwrap();
        let archived = view(&archived_root, "Other Mod", ModStatus::Archived, None);
        assert!(!tx.capture_view(&archived, CaptureMode::Stash).unwrap());
        assert!(!tx.doc.dirty);
        assert!(
            tx.doc
                .to_text()
                .contains("$\\mods\\other mod\\other.ini\\value = 3")
        );
        assert!(read_stash(&archived_root).is_none());
    }

    #[test]
    fn archived_reroute_is_bookkeeping_only() {
        // The archived mod was renamed while hidden; its old prefix is now owned by a live
        // mod. Reroute must update the stash anchor without absorbing the live entries.
        let temp = tempfile::tempdir().unwrap();
        let (_root, _mods, mut tx) = tx_for(&temp);
        let archived_root = temp
            .path()
            .join("EFMI")
            .join("Mods_Archived")
            .join("Renamed Mod");
        std::fs::create_dir_all(&archived_root).unwrap();
        let original_entries = vec![StashEntry {
            key: "a.ini\\kept".to_string(),
            value: "7".to_string(),
        }];
        write_stash(
            &archived_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\other mod\\".to_string(),
                captured_at: Utc::now(),
                entries: original_entries.clone(),
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let archived = view(&archived_root, "Renamed Mod", ModStatus::Archived, None);
        assert!(tx.reroute_view(&archived).unwrap());
        assert!(!tx.doc.dirty);
        assert!(
            tx.doc
                .to_text()
                .contains("$\\mods\\other mod\\other.ini\\value = 3")
        );
        let stash = read_stash(&archived_root).unwrap();
        assert_eq!(stash.source_prefix, "$\\mods\\renamed mod\\");
        assert_eq!(stash.entries, original_entries);
    }

    #[test]
    fn rebase_updates_stash_only() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Renamed While Hidden");
        std::fs::create_dir_all(&mod_root).unwrap();
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\old name\\".to_string(),
                captured_at: Utc::now(),
                entries: vec![StashEntry {
                    key: "a.ini\\k".to_string(),
                    value: "1".to_string(),
                }],
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let view = view(&mod_root, "Renamed While Hidden", ModStatus::Archived, None);
        assert!(tx.rebase_view(&view).unwrap());
        assert!(!tx.doc.dirty);
        let stash = read_stash(&mod_root).unwrap();
        assert_eq!(stash.source_prefix, "$\\mods\\renamed while hidden\\");
        assert_eq!(stash.entries.len(), 1);
    }

    #[test]
    fn archived_prefix_falls_back_to_mods_root() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, tx) = tx_for(&temp);
        let archived_root = temp
            .path()
            .join("EFMI")
            .join("Mods_Archived")
            .join("Other Mod");
        let fallback_view = view(&archived_root, "Other Mod", ModStatus::Archived, None);
        assert_eq!(
            tx.prefix_for(&fallback_view).unwrap(),
            "$\\mods\\other mod\\"
        );
        let explicit = mods.join("Explicit Path Mod");
        let explicit_view = view(
            &archived_root,
            "ignored",
            ModStatus::Archived,
            Some(&explicit),
        );
        assert_eq!(
            tx.prefix_for(&explicit_view).unwrap(),
            "$\\mods\\explicit path mod\\"
        );
    }

    #[test]
    fn checkpoint_rollback_discards_capture() {
        let temp = tempfile::tempdir().unwrap();
        let (root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Other Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        let cp = tx.checkpoint();
        let view = view(&mod_root, "Other Mod", ModStatus::Active, None);
        assert!(tx.capture_view(&view, CaptureMode::Stash).unwrap());
        assert!(tx.doc.dirty);
        tx.rollback(cp);
        assert!(!tx.doc.dirty);
        assert!(
            tx.doc
                .to_text()
                .contains("$\\mods\\other mod\\other.ini\\value = 3")
        );
        let outcome = tx.commit().unwrap();
        assert!(!outcome.wrote);
        let text = std::fs::read_to_string(root.join(USER_INI_FILE)).unwrap();
        assert_eq!(text, SAMPLE);
    }

    #[test]
    fn baseline_records_anchor_without_doc_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Other Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        let view = view(&mod_root, "Other Mod", ModStatus::Active, None);
        assert!(tx.baseline_view(&view).unwrap());
        assert!(!tx.doc.dirty);
        let stash = read_stash(&mod_root).unwrap();
        assert_eq!(stash.entries.len(), 1);
        // Existing stash — even an empty one — blocks a re-baseline.
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: stash.source_prefix.clone(),
                captured_at: Utc::now(),
                entries: Vec::new(),
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        assert!(!tx.baseline_view(&view).unwrap());
        assert!(read_stash(&mod_root).unwrap().entries.is_empty());
    }

    #[test]
    fn restore_imported_skips_when_live_entries_exist() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, mods, mut tx) = tx_for(&temp);
        let mod_root = mods.join("Other Mod");
        std::fs::create_dir_all(&mod_root).unwrap();
        write_stash(
            &mod_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\other mod\\".to_string(),
                captured_at: Utc::now(),
                entries: vec![StashEntry {
                    key: "other.ini\\value".to_string(),
                    value: "99".to_string(),
                }],
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let view = view(&mod_root, "Other Mod", ModStatus::Active, None);
        assert!(!tx.restore_imported_view(&view).unwrap());
        assert!(tx.doc.to_text().contains("other.ini\\value = 3"));
        // Once the live entries are gone, the stash applies.
        tx.doc.take_prefix("$\\mods\\other mod\\");
        assert!(tx.restore_imported_view(&view).unwrap());
        assert!(
            tx.doc
                .to_text()
                .contains("$\\mods\\other mod\\other.ini\\value = 99")
        );
    }

    #[test]
    fn unreadable_doc_never_writes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("EFMI");
        let mods = root.join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(root.join("d3dx.ini"), b"ini").unwrap();
        std::fs::write(root.join(USER_INI_FILE), [0xFF, 0xFE, 0x00]).unwrap();
        let error = UserIniDoc::open(&root).unwrap_err();
        assert!(error.to_string().contains("UTF-8"));
        let mut tx = PersistTx {
            importer_root: root.clone(),
            mods_root: Some(mods.clone()),
            doc: UserIniDoc::new_empty(),
            doc_unreadable: true,
            warnings: vec!["skipped".to_string()],
            shared_explicit_prefixes: Vec::new(),
            explicit_prefixes_by_root: HashMap::new(),
        };
        let mod_root = mods.join("Any");
        std::fs::create_dir_all(&mod_root).unwrap();
        let view = view(&mod_root, "Any", ModStatus::Active, None);
        assert!(!tx.capture_view(&view, CaptureMode::Stash).unwrap());
        let outcome = tx.commit().unwrap();
        assert!(!outcome.wrote);
        assert_eq!(
            std::fs::read(root.join(USER_INI_FILE)).unwrap(),
            vec![0xFF, 0xFE, 0x00],
            "an unreadable d3dx_user.ini must never be overwritten"
        );
    }

    #[test]
    fn stale_tmp_is_cleaned_on_open() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join(USER_INI_TMP_FILE), b"stale").unwrap();
        std::fs::write(root.join(USER_INI_FILE), SAMPLE.as_bytes()).unwrap();
        let _ = UserIniDoc::open(root).unwrap();
        assert!(!root.join(USER_INI_TMP_FILE).exists());
    }

    #[test]
    fn d3dx_patch_merges_marked_binding_into_system_section() {
        let original =
            "[Loader]\r\nTarget = Game.exe\r\n\r\n[System]\r\ncheck_foreground_window = 1\r\n";
        let D3dxPatch::Updated(updated) = patch_d3dx_ini_text(original, true) else {
            panic!("expected d3dx.ini to be patched");
        };
        assert!(updated.starts_with("[Loader]\r\nTarget = Game.exe"));
        assert!(updated.contains("[System]\r\n; --- Hestia begin ---\r\n"));
        assert!(updated.contains("additional_foreground_window = Hestia\r\n; --- Hestia end ---"));
        assert!(updated.contains("[Loader]\r\nTarget = Game.exe"));
        assert_eq!(updated.matches("[System]").count(), 1);
        assert!(matches!(
            patch_d3dx_ini_text(&updated, true),
            D3dxPatch::Unchanged
        ));
    }

    #[test]
    fn d3dx_patch_leaves_existing_non_hestia_binding_alone() {
        let original = "[System]\r\nadditional_foreground_window = Other Tool\r\n";
        assert!(matches!(
            patch_d3dx_ini_text(original, true),
            D3dxPatch::BlockedByExistingBinding
        ));
    }

    #[test]
    fn d3dx_patch_removes_hestia_block_when_support_is_disabled() {
        let D3dxPatch::Updated(updated) = patch_d3dx_ini_text("[System]\r\nx = 1\r\n", true) else {
            panic!("expected initial insert");
        };
        let D3dxPatch::Updated(cleaned) = patch_d3dx_ini_text(&updated, false) else {
            panic!("expected generated block removal");
        };
        assert!(!cleaned.contains(HESTIA_D3DX_BEGIN));
        assert!(cleaned.contains("[System]\r\nx = 1\r\n"));
    }

    #[test]
    fn d3dx_patch_disables_loose_hestia_binding_when_support_is_disabled() {
        let original = "[System]\r\nadditional_foreground_window = hestia\r\nx = 1\r\n";
        let D3dxPatch::Updated(updated) = patch_d3dx_ini_text(original, false) else {
            panic!("expected loose Hestia binding to be commented out");
        };
        assert!(updated.contains("; additional_foreground_window = hestia\r\n"));
        assert!(updated.contains("x = 1\r\n"));
    }

    #[test]
    fn d3dx_patch_preserves_modified_hestia_block_when_support_is_disabled() {
        let original = "[System]\r\n; --- Hestia begin ---\r\n\
; user edited this block\r\n\
additional_foreground_window = Hestia\r\n\
custom_user_field = keep me\r\n\
; --- Hestia end ---\r\n\
x = 1\r\n";
        let D3dxPatch::Updated(updated) = patch_d3dx_ini_text(original, false) else {
            panic!("expected Hestia binding inside modified block to be commented out");
        };
        assert!(updated.contains("; --- Hestia begin ---\r\n"));
        assert!(updated.contains("; additional_foreground_window = Hestia\r\n"));
        assert!(updated.contains("custom_user_field = keep me\r\n"));
        assert!(updated.contains("; --- Hestia end ---\r\n"));
        assert!(updated.contains("x = 1\r\n"));
    }

    #[test]
    fn d3dx_patch_replace_conflict_comments_existing_binding() {
        let original =
            "[System]\r\n; user comment\r\nadditional_foreground_window = Other Tool\r\nx = 1\r\n";
        let D3dxPatch::Updated(updated) = patch_d3dx_ini_text_replace_conflict(original) else {
            panic!("expected conflicting binding to be replaced");
        };
        assert!(updated.starts_with("[System]\r\n; --- Hestia begin ---\r\n"));
        assert!(updated.contains("; additional_foreground_window = Other Tool\r\n"));
        assert!(updated.contains("; user comment\r\n"));
        assert!(updated.contains("x = 1\r\n"));
        assert_eq!(updated.matches("additional_foreground_window").count(), 2);
    }

    #[test]
    fn d3dx_patch_removes_legacy_duplicate_system_block() {
        let original = "; --- Hestia begin ---\r\n\
; Hestia uses this experimental block to let XXMI receive reload hotkeys while Hestia is focused.\r\n\
; Safe to delete this whole block if you remove Hestia.\r\n\
[System]\r\n\
additional_foreground_window = Hestia\r\n\
; --- Hestia end ---\r\n\
\r\n\
[Loader]\r\nTarget = Game.exe\r\n\r\n[System]\r\ncheck_foreground_window = 1\r\n";
        let D3dxPatch::Updated(updated) = patch_d3dx_ini_text(original, true) else {
            panic!("expected legacy block to be replaced");
        };
        assert!(updated.starts_with("[Loader]\r\nTarget = Game.exe"));
        assert_eq!(updated.matches("[System]").count(), 1);
        assert!(updated.contains("[System]\r\n; --- Hestia begin ---\r\n"));
        assert!(updated.contains("check_foreground_window = 1\r\n"));
    }

    #[test]
    fn legacy_helper_ini_cleanup_removes_only_hestia_generated_content() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        let helper = mods.join(LEGACY_HELPER_INI_FILE);
        std::fs::write(&helper, LEGACY_HELPER_INI_SUPPORTED.as_bytes()).unwrap();
        cleanup_legacy_helper_ini(&mods).unwrap();
        assert!(!helper.exists());

        std::fs::write(
            &helper,
            b"[System]\r\nadditional_foreground_window = User\r\n",
        )
        .unwrap();
        cleanup_legacy_helper_ini(&mods).unwrap();
        assert_eq!(
            std::fs::read_to_string(&helper).unwrap(),
            "[System]\r\nadditional_foreground_window = User\r\n"
        );
    }

    #[test]
    fn d3dx_backup_rotation_keeps_latest_bak_and_archives_previous() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(D3DX_INI_FILE);
        let backup = temp.path().join(format!("{D3DX_INI_FILE}.bak"));
        let archive_dir = temp
            .path()
            .join(format!("{D3DX_INI_FILE}.bak - Hestia Backup"));
        std::fs::write(&path, b"current").unwrap();
        std::fs::write(&backup, b"previous").unwrap();

        rotate_d3dx_backup(&path).unwrap();

        assert_eq!(std::fs::read(&backup).unwrap(), b"current");
        let archived: Vec<_> = std::fs::read_dir(&archive_dir).unwrap().collect();
        assert_eq!(archived.len(), 1);
        let archived_path = archived[0].as_ref().unwrap().path();
        assert_eq!(std::fs::read(archived_path).unwrap(), b"previous");
    }

    #[test]
    fn clean_snapshot_matches_new_empty_doc() {
        let clean = clean_user_ini_snapshot();
        assert_eq!(
            clean,
            format!("{USER_INI_HEADER}\r\n{CONSTANTS_SECTION}\r\n")
        );
        let temp = tempfile::tempdir().unwrap();
        apply_user_ini_snapshot(temp.path(), &clean).unwrap();
        assert_eq!(snapshot_user_ini(temp.path()).unwrap(), clean);
    }

    #[test]
    fn stash_bytes_carry_across_replace() {
        let temp = tempfile::tempdir().unwrap();
        let old_root = temp.path().join("Mod");
        std::fs::create_dir_all(&old_root).unwrap();
        write_stash(
            &old_root,
            &ModStash {
                version: STASH_FORMAT_VERSION,
                source_prefix: "$\\mods\\mod\\".to_string(),
                captured_at: Utc::now(),
                entries: Vec::new(),
                full_entries: Vec::new(),
            },
        )
        .unwrap();
        let bytes = read_stash_bytes(&old_root);
        assert!(bytes.is_some());
        std::fs::remove_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&old_root).unwrap();
        restore_stash_bytes(&old_root, &bytes);
        assert!(read_stash(&old_root).is_some());
        restore_stash_bytes(&old_root, &None);
        assert!(read_stash(&old_root).is_some());
    }
}
