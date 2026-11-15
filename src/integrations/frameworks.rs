use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use chrono::{DateTime, Utc};
use regex::Regex;

use crate::model::{DISABLED_CONTAINER, MOD_META_DIR};

pub const QUIET_FETCH_LIMIT_BYTES: u64 = 2 * 1024 * 1024;

const VERSION_LINE_SCAN_LIMIT: usize = 40;
const READABLE_TEXT_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
const README_LIMIT_BYTES: u64 = 1024 * 1024;
const MIN_NAME_MATCH_CHARS: usize = 4;
const COMMON_INI_STEMS: [&str; 10] = [
    "main", "merged", "config", "settings", "script", "d3dx", "shader", "shaders", "toggle",
    "master",
];
const REFERENCE_PREFIXES: [&str; 10] = [
    "commandlist",
    "builtincommandlist",
    "customshader",
    "builtincustomshader",
    "resource",
    "preset",
    "shaderoverride",
    "textureoverride",
    "shaderregex",
    "key",
];
const REFERENCE_STOPS: [char; 16] = [
    '=', ',', '(', ')', '!', '<', '>', '&', '|', '+', '*', '/', '%', ';', '$', '"',
];
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "tga", "bmp"];
const VERSION_SETTING_TOKENS: [&str; 6] = ["version", "ver", "api", "build", "revision", "rev"];
const OTHER_VERSION_TOKENS: [&str; 14] = [
    "required", "require", "requires", "min", "minimum", "needed", "expected", "target", "seen",
    "last", "previous", "prev", "old", "max",
];
const VERSION_LINE_STOPS: [char; 9] = [',', ';', ':', '(', ')', '|', '[', ']', '"'];
const INCLUDE_BEARING_EXTENSIONS: [&str; 4] = ["ini", "hlsl", "hlsli", "fx"];

pub fn namespace_key(raw: &str) -> Option<String> {
    let segments: Vec<String> = raw
        .split('\\')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    (!segments.is_empty()).then(|| segments.join("\\"))
}

pub fn normalized_name(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredNamespace {
    pub key: String,
    pub written: String,
    pub ini_depth: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatedVersion {
    Setting(f64),
    Line {
        words: Vec<String>,
        numbers: Vec<Vec<u64>>,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderFacts {
    pub declared: Vec<DeclaredNamespace>,
    pub called: BTreeSet<String>,
    pub has_models: bool,
    pub has_textures: bool,
    pub ini_names: BTreeSet<String>,
    pub stated_versions: BTreeMap<String, StatedVersion>,
}

impl FolderFacts {
    pub fn declared_keys(&self) -> BTreeSet<String> {
        self.declared
            .iter()
            .map(|declared| declared.key.clone())
            .collect()
    }

    pub fn declares(&self, key: &str) -> bool {
        self.declared.iter().any(|declared| declared.key == key)
    }

    pub fn declares_any(&self, keys: &BTreeSet<String>) -> bool {
        self.declared
            .iter()
            .any(|declared| keys.contains(&declared.key))
    }

    pub fn calls(&self, key: &str) -> bool {
        self.called.contains(key) && !self.declares(key)
    }

    pub fn calls_any(&self, keys: &BTreeSet<String>) -> bool {
        keys.iter().any(|key| self.calls(key))
    }

    pub fn framework_shaped(&self) -> bool {
        !self.has_models && !self.has_textures && !self.declared.is_empty()
    }

    pub fn called_but_not_declared(&self) -> BTreeSet<String> {
        self.called
            .iter()
            .filter(|key| !self.declares(key))
            .cloned()
            .collect()
    }

    pub fn shallowest_declared(&self) -> Option<&DeclaredNamespace> {
        self.declared
            .iter()
            .min_by_key(|declared| declared.ini_depth)
    }
}

fn is_model_extension(extension: &str) -> bool {
    matches!(extension, "buf" | "ib" | "vb")
        || extension
            .strip_prefix("vb")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

fn lower_extension(lower_name: &str) -> &str {
    lower_name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("")
}

fn is_readme(lower_name: &str) -> bool {
    lower_name.starts_with("readme") && matches!(lower_extension(lower_name), "txt" | "md" | "")
}

fn read_text(path: &Path, limit: u64) -> Option<String> {
    let size = fs::metadata(path).ok()?.len();
    if size > limit {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(decode_text(&bytes))
}

fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |bytes: &[u8], big_endian: bool| {
        let (pairs, _) = bytes.as_chunks::<2>();
        let units: Vec<u16> = pairs
            .iter()
            .map(|pair| {
                if big_endian {
                    u16::from_be_bytes(*pair)
                } else {
                    u16::from_le_bytes(*pair)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn relative_segments(root: &Path, path: &Path) -> Vec<String> {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|segment| *segment != DISABLED_CONTAINER)
        .map(str::to_owned)
        .collect()
}

fn content_files(root: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != OsStr::new(MOD_META_DIR))
        .flatten()
        .filter(|entry| entry.file_type().is_file())
}

pub fn scan_folder(root: &Path) -> FolderFacts {
    let mut facts = FolderFacts::default();
    let mut image_names = BTreeSet::new();
    let mut referenced_files = BTreeSet::new();
    for entry in content_files(root) {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        let lower_name = file_name.to_lowercase();
        let extension = lower_extension(&lower_name);
        if is_model_extension(extension) {
            facts.has_models = true;
        } else if extension == "dds" {
            facts.has_textures = true;
        } else if IMAGE_EXTENSIONS.contains(&extension) {
            image_names.insert(lower_name);
        } else if extension == "ini" {
            let depth = relative_segments(root, path).len().saturating_sub(1);
            if let Some(text) = read_text(path, READABLE_TEXT_LIMIT_BYTES) {
                scan_ini(&text, &lower_name, depth, &mut facts, &mut referenced_files);
            }
            facts.ini_names.insert(lower_name);
        } else if is_readme(&lower_name)
            && let Some(text) = read_text(path, README_LIMIT_BYTES)
            && let Some(stated) = text
                .lines()
                .take(VERSION_LINE_SCAN_LIMIT)
                .find_map(version_line)
        {
            facts
                .stated_versions
                .entry(format!("line:{lower_name}"))
                .or_insert(stated);
        }
    }
    if image_names
        .iter()
        .any(|name| referenced_files.contains(name))
    {
        facts.has_textures = true;
    }
    facts
}

fn scan_ini(
    text: &str,
    lower_name: &str,
    depth: usize,
    facts: &mut FolderFacts,
    referenced_files: &mut BTreeSet<String>,
) {
    let mut before_first_section = true;
    let mut ini_namespace = String::new();
    let mut settings = Vec::new();
    let line_key = format!("line:{lower_name}");
    for (index, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with(';') || line.starts_with('#') {
            if index < VERSION_LINE_SCAN_LIMIT
                && !facts.stated_versions.contains_key(&line_key)
                && let Some(stated) = version_line(line)
            {
                facts.stated_versions.insert(line_key.clone(), stated);
            }
            continue;
        }
        if line.starts_with('[') {
            before_first_section = false;
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if before_first_section && key.eq_ignore_ascii_case("namespace") {
                if let Some(namespace) = namespace_key(value) {
                    if !facts.declares(&namespace) {
                        facts.declared.push(DeclaredNamespace {
                            key: namespace.clone(),
                            written: value.trim().to_owned(),
                            ini_depth: depth,
                        });
                    }
                    ini_namespace = namespace;
                }
                continue;
            }
            if key.eq_ignore_ascii_case("filename") {
                if let Some(name) = value
                    .trim()
                    .rsplit(['\\', '/'])
                    .next()
                    .filter(|name| !name.is_empty())
                {
                    referenced_files.insert(name.to_lowercase());
                }
                continue;
            }
        }
        collect_called_namespaces(line, &mut facts.called);
        if let Some(setting) = version_setting(line) {
            settings.push(setting);
        }
    }
    for (name, value) in settings {
        facts.stated_versions.insert(
            format!("setting:{ini_namespace}:{name}"),
            StatedVersion::Setting(value),
        );
    }
}

fn reference_prefix_ends_at(lower_line: &str, backslash: usize) -> bool {
    let head = &lower_line[..backslash];
    if head.ends_with('$') {
        return true;
    }
    let word_start = head
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_ascii_alphabetic())
        .last()
        .map(|(index, _)| index);
    word_start.is_some_and(|start| REFERENCE_PREFIXES.contains(&&head[start..]))
}

fn collect_called_namespaces(line: &str, called: &mut BTreeSet<String>) {
    let lower = line.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find('\\') {
        let backslash = cursor + offset;
        cursor = backslash + 1;
        if !reference_prefix_ends_at(&lower, backslash) {
            continue;
        }
        let tail = &lower[cursor..];
        let end = tail
            .find(|character: char| {
                character.is_whitespace() || REFERENCE_STOPS.contains(&character)
            })
            .unwrap_or(tail.len());
        let segments: Vec<&str> = tail[..end].split('\\').collect();
        if segments.len() >= 2 && segments.iter().all(|segment| !segment.is_empty()) {
            called.insert(segments[..segments.len() - 1].join("\\"));
        }
        cursor += end;
    }
}

fn version_setting_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)^global\s+((?:[a-z]+\s+)*)\$([a-z0-9_]+)\s*=\s*(-?\d+(?:\.\d+)?)$")
            .expect("version setting pattern compiles")
    })
}

fn version_line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\bversion\b\s*[:=]?\s*v?(\d+(?:[.\-]\d+)*)")
            .expect("version line pattern compiles")
    })
}

fn setting_name_states_a_version(lower_name: &str) -> bool {
    let mut tokens = lower_name.split('_');
    let about_something_else = tokens
        .clone()
        .any(|token| OTHER_VERSION_TOKENS.contains(&token));
    !about_something_else
        && tokens.any(|token| VERSION_SETTING_TOKENS.contains(&token) || token.contains("version"))
}

fn version_setting(line: &str) -> Option<(String, f64)> {
    let found = version_setting_pattern().captures(line)?;
    if found
        .get(1)?
        .as_str()
        .split_whitespace()
        .any(|modifier| modifier.eq_ignore_ascii_case("persist"))
    {
        return None;
    }
    let name = found.get(2)?.as_str().to_ascii_lowercase();
    if !setting_name_states_a_version(&name) {
        return None;
    }
    let value: f64 = found.get(3)?.as_str().parse().ok()?;
    value.is_finite().then_some((name, value))
}

fn number_group(text: &str) -> Vec<u64> {
    text.split(['.', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or(u64::MAX))
        .collect()
}

fn version_line_tail(tail: &str) -> &str {
    let mut end = tail.len();
    for (index, character) in tail.char_indices() {
        let rest = &tail[index + character.len_utf8()..];
        let sentence_end =
            character == '.' && rest.chars().next().is_none_or(|next| next.is_whitespace());
        let spaced_dash = matches!(character, '-' | '\u{2013}' | '\u{2014}')
            && tail[..index].ends_with(char::is_whitespace)
            && rest.starts_with(char::is_whitespace);
        if VERSION_LINE_STOPS.contains(&character) || sentence_end || spaced_dash {
            end = index;
            break;
        }
    }
    &tail[..end]
}

fn alphabetic_words(text: &str, words: &mut Vec<String>) {
    words.extend(
        text.split(|character: char| !character.is_alphabetic())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase),
    );
}

fn version_line(line: &str) -> Option<StatedVersion> {
    let found = version_line_pattern().captures(line)?;
    let whole = found.get(0)?;
    let mut numbers = vec![number_group(found.get(1)?.as_str())];
    let mut words = Vec::new();
    alphabetic_words(&line[..whole.start()], &mut words);
    let tail = version_line_tail(&line[whole.end()..]);
    alphabetic_words(tail, &mut words);
    numbers.extend(
        tail.split(|character: char| !(character.is_ascii_digit() || character == '.'))
            .map(|part| part.trim_matches('.'))
            .filter(|part| !part.is_empty())
            .map(number_group),
    );
    Some(StatedVersion::Line { words, numbers })
}

fn compare_number_groups(mine: &[u64], arriving: &[u64]) -> Ordering {
    let length = mine.len().max(arriving.len());
    (0..length)
        .map(|index| {
            let mine = mine.get(index).copied().unwrap_or(0);
            let arriving = arriving.get(index).copied().unwrap_or(0);
            arriving.cmp(&mine)
        })
        .find(|ordering| *ordering != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn arriving_against_mine(mine: &StatedVersion, arriving: &StatedVersion) -> Option<Ordering> {
    match (mine, arriving) {
        (StatedVersion::Setting(mine), StatedVersion::Setting(arriving)) => {
            if (mine - arriving).abs() < 1e-9 {
                Some(Ordering::Equal)
            } else {
                arriving.partial_cmp(mine)
            }
        }
        (
            StatedVersion::Line {
                words: my_words,
                numbers: my_numbers,
            },
            StatedVersion::Line {
                words: arriving_words,
                numbers: arriving_numbers,
            },
        ) => {
            if my_words != arriving_words {
                return None;
            }
            let shared = my_numbers.len().min(arriving_numbers.len());
            let by_group = (0..shared)
                .map(|index| compare_number_groups(&my_numbers[index], &arriving_numbers[index]))
                .find(|ordering| *ordering != Ordering::Equal);
            Some(by_group.unwrap_or_else(|| arriving_numbers.len().cmp(&my_numbers.len())))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsideCheck {
    DifferentFramework,
    Newer,
    SameOrOlder,
    CannotTell,
}

pub fn compare_stated_versions(
    mine: &BTreeMap<String, StatedVersion>,
    arriving: &BTreeMap<String, StatedVersion>,
) -> InsideCheck {
    let mut higher = false;
    let mut lower = false;
    let mut shared = false;
    for (key, my_version) in mine {
        let Some(arriving_version) = arriving.get(key) else {
            continue;
        };
        shared = true;
        match arriving_against_mine(my_version, arriving_version) {
            Some(Ordering::Greater) => higher = true,
            Some(Ordering::Less) => lower = true,
            Some(Ordering::Equal) => {}
            None => return InsideCheck::CannotTell,
        }
    }
    match (shared, higher, lower) {
        (false, _, _) | (true, true, true) => InsideCheck::CannotTell,
        (true, true, false) => InsideCheck::Newer,
        (true, false, _) => InsideCheck::SameOrOlder,
    }
}

pub fn inside_check(mine: &FolderFacts, arriving: &FolderFacts) -> InsideCheck {
    if !arriving.declares_any(&mine.declared_keys()) {
        return InsideCheck::DifferentFramework;
    }
    compare_stated_versions(&mine.stated_versions, &arriving.stated_versions)
}

#[derive(Clone)]
struct CachedFacts {
    ini_hash: Option<String>,
    content_mtime: Option<DateTime<Utc>>,
    facts: Arc<FolderFacts>,
}

static FOLDER_FACTS: OnceLock<Mutex<HashMap<PathBuf, CachedFacts>>> = OnceLock::new();

fn folder_facts_cache() -> &'static Mutex<HashMap<PathBuf, CachedFacts>> {
    FOLDER_FACTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn known_folder_facts(
    root: &Path,
    ini_hash: Option<&str>,
    content_mtime: Option<DateTime<Utc>>,
) -> Option<Arc<FolderFacts>> {
    if ini_hash.is_none() && content_mtime.is_none() {
        return None;
    }
    let cache = folder_facts_cache().lock().ok()?;
    cache
        .get(root)
        .filter(|cached| {
            cached.ini_hash.as_deref() == ini_hash && cached.content_mtime == content_mtime
        })
        .map(|cached| Arc::clone(&cached.facts))
}

pub fn folder_facts(
    root: &Path,
    ini_hash: Option<&str>,
    content_mtime: Option<DateTime<Utc>>,
) -> Arc<FolderFacts> {
    if let Some(known) = known_folder_facts(root, ini_hash, content_mtime) {
        return known;
    }
    let facts = Arc::new(scan_folder(root));
    if (ini_hash.is_some() || content_mtime.is_some())
        && let Ok(mut cache) = folder_facts_cache().lock()
    {
        cache.insert(
            root.to_path_buf(),
            CachedFacts {
                ini_hash: ini_hash.map(str::to_owned),
                content_mtime,
                facts: Arc::clone(&facts),
            },
        );
    }
    facts
}

#[derive(Debug, Clone)]
pub struct InstalledFolder {
    pub mod_id: String,
    pub root_path: PathBuf,
    pub ini_hash: Option<String>,
    pub content_mtime: Option<DateTime<Utc>>,
    pub recorded_framework: bool,
}

impl InstalledFolder {
    pub fn facts(&self) -> Arc<FolderFacts> {
        folder_facts(
            &self.root_path,
            self.ini_hash.as_deref(),
            self.content_mtime,
        )
    }
}

pub fn loader_namespace_keys(importer_root: &Path, mods_root: &Path) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let Ok(entries) = fs::read_dir(importer_root) else {
        return keys;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == mods_root || !path.is_dir() {
            continue;
        }
        let folder = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if folder.starts_with("mods") || folder == "shadercache" {
            continue;
        }
        keys.extend(scan_folder(&path).declared_keys());
    }
    keys
}

#[derive(Debug, Clone)]
pub struct InstalledCopy {
    pub mod_id: String,
    pub inside_check: InsideCheck,
}

#[derive(Debug, Clone)]
pub struct MovablePart {
    pub relative_dir: PathBuf,
    pub facts: Arc<FolderFacts>,
}

#[derive(Debug, Clone, Default)]
pub struct CandidateScan {
    pub facts: Arc<FolderFacts>,
    pub inner_folder_name: Option<String>,
    pub callers: Vec<String>,
    pub installed_copies: Vec<InstalledCopy>,
    pub movable_parts: Vec<MovablePart>,
}

impl CandidateScan {
    pub fn framework_shaped(&self) -> bool {
        self.facts.framework_shaped()
    }
}

pub fn installed_framework_ids(
    installed: &[(InstalledFolder, Arc<FolderFacts>)],
) -> BTreeSet<String> {
    installed
        .iter()
        .filter(|(folder, facts)| {
            facts.framework_shaped()
                && (folder.recorded_framework
                    || installed.iter().any(|(other, other_facts)| {
                        other.mod_id != folder.mod_id
                            && other_facts.calls_any(&facts.declared_keys())
                    }))
        })
        .map(|(folder, _)| folder.mod_id.clone())
        .collect()
}

pub fn scan_candidate(
    candidate_root: &Path,
    names_itself: bool,
    installed: &[(InstalledFolder, Arc<FolderFacts>)],
) -> CandidateScan {
    let facts = Arc::new(scan_folder(candidate_root));
    let inner_folder_name = names_itself
        .then(|| candidate_root.file_name())
        .flatten()
        .and_then(OsStr::to_str)
        .map(str::to_owned);
    let declared = facts.declared_keys();
    let mut scan = CandidateScan {
        facts: Arc::clone(&facts),
        inner_folder_name,
        ..CandidateScan::default()
    };
    if facts.framework_shaped() {
        scan.callers = installed
            .iter()
            .filter(|(_, other)| other.calls_any(&declared))
            .map(|(folder, _)| folder.mod_id.clone())
            .collect();
        scan.installed_copies = installed
            .iter()
            .filter(|(_, other)| other.framework_shaped() && other.declares_any(&declared))
            .map(|(folder, other)| InstalledCopy {
                mod_id: folder.mod_id.clone(),
                inside_check: inside_check(other, &facts),
            })
            .collect();
    }
    scan
}

#[derive(Debug, Clone, Default)]
pub struct InstallContext {
    pub installed: Vec<InstalledFolder>,
}

impl InstallContext {
    pub fn with_facts(&self) -> Vec<(InstalledFolder, Arc<FolderFacts>)> {
        self.installed
            .iter()
            .map(|folder| (folder.clone(), folder.facts()))
            .collect()
    }
}

pub fn scan_candidates(
    candidate_roots: &[PathBuf],
    extraction_root: Option<&Path>,
    context: &InstallContext,
) -> Vec<CandidateScan> {
    let installed = context.with_facts();
    candidate_roots
        .iter()
        .map(|candidate_root| {
            let names_itself = extraction_root != Some(candidate_root.as_path());
            let mut scan = scan_candidate(candidate_root, names_itself, &installed);
            if extraction_root.is_some() && !scan.facts.framework_shaped() {
                scan.movable_parts = movable_framework_parts(candidate_root, &installed);
            }
            scan
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct BundleFolder<'a> {
    pub root: &'a Path,
    pub merges_into_the_update_target: bool,
}

pub fn frameworks_among_bundle_folders(
    folders: &[BundleFolder],
    update_target: Option<&InstalledFolder>,
    context: &InstallContext,
) -> Vec<bool> {
    let scanned: Vec<FolderFacts> = folders
        .iter()
        .map(|folder| scan_folder(folder.root))
        .collect();
    let could_be_one = |index: usize| {
        folders[index].merges_into_the_update_target && scanned[index].framework_shaped()
    };
    if !(0..folders.len()).any(could_be_one) {
        return vec![false; folders.len()];
    }
    let installed = context.with_facts();
    let update_target_facts = update_target.map(InstalledFolder::facts);
    (0..folders.len())
        .map(|index| {
            if !could_be_one(index) {
                return false;
            }
            let declared = scanned[index].declared_keys();
            let others = || (0..folders.len()).filter(move |other| *other != index);
            let update_target_uses_the_name = update_target_facts
                .as_ref()
                .is_some_and(|facts| facts.declares_any(&declared))
                || others().any(|other| {
                    folders[other].merges_into_the_update_target
                        && !scanned[other].framework_shaped()
                        && scanned[other].declares_any(&declared)
                });
            if update_target_uses_the_name {
                return false;
            }
            others().any(|other| scanned[other].calls_any(&declared))
                || installed.iter().any(|(folder, facts)| {
                    facts.calls_any(&declared)
                        || (folder.recorded_framework
                            && facts.framework_shaped()
                            && facts.declares_any(&declared))
                })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct SplitPart {
    pub new_job_id: u64,
    pub candidate_index: usize,
    pub relative_dir: Option<PathBuf>,
}

pub fn movable_framework_parts(
    candidate_root: &Path,
    installed: &[(InstalledFolder, Arc<FolderFacts>)],
) -> Vec<MovablePart> {
    let frameworks = installed_framework_ids(installed);
    let mut part_dirs: Vec<(PathBuf, String)> = Vec::new();
    for entry in content_files(candidate_root) {
        let path = entry.path();
        let is_ini = path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"));
        let Some(parent) = path.parent().filter(|parent| *parent != candidate_root) else {
            continue;
        };
        if !is_ini {
            continue;
        }
        let Some(text) = read_text(path, READABLE_TEXT_LIMIT_BYTES) else {
            continue;
        };
        let mut ini_facts = FolderFacts::default();
        scan_ini(&text, "", 0, &mut ini_facts, &mut BTreeSet::new());
        let declared = ini_facts.declared_keys();
        if declared.is_empty() {
            continue;
        }
        let Some((folder, _)) = installed.iter().find(|(folder, facts)| {
            frameworks.contains(&folder.mod_id) && facts.declares_any(&declared)
        }) else {
            continue;
        };
        part_dirs.push((parent.to_path_buf(), folder.mod_id.clone()));
    }
    part_dirs.sort();
    part_dirs.dedup();
    let topmost: Vec<(PathBuf, String)> = part_dirs
        .iter()
        .filter(|(dir, _)| {
            !part_dirs
                .iter()
                .any(|(other, _)| other != dir && dir.starts_with(other))
        })
        .cloned()
        .collect();
    topmost
        .into_iter()
        .filter_map(|(dir, installed_mod_id)| {
            let facts = scan_folder(&dir);
            let installed_keys = installed
                .iter()
                .find(|(folder, _)| folder.mod_id == installed_mod_id)
                .map(|(_, facts)| facts.declared_keys())?;
            let only_that_framework = facts.framework_shaped()
                && facts
                    .declared
                    .iter()
                    .all(|declared| installed_keys.contains(&declared.key));
            let relative_dir = dir.strip_prefix(candidate_root).ok()?.to_path_buf();
            (only_that_framework && !rest_reads_files_inside(candidate_root, &dir)).then(|| {
                MovablePart {
                    relative_dir,
                    facts: Arc::new(facts),
                }
            })
        })
        .collect()
}

fn rest_reads_files_inside(candidate_root: &Path, part_dir: &Path) -> bool {
    let Some(part_name) = part_dir
        .file_name()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase)
    else {
        return true;
    };
    let needles = [format!("{part_name}/"), format!("{part_name}\\")];
    content_files(candidate_root)
        .filter(|entry| !entry.path().starts_with(part_dir))
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|extension| {
                    INCLUDE_BEARING_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
                })
        })
        .filter_map(|entry| read_text(entry.path(), READABLE_TEXT_LIMIT_BYTES))
        .any(|text| {
            text.lines().any(|line| {
                let line = line.trim().to_lowercase();
                let reads_a_file = line.starts_with("#include")
                    || line
                        .split_once('=')
                        .is_some_and(|(key, _)| key.trim() == "filename");
                reads_a_file && needles.iter().any(|needle| line.contains(needle))
            })
        })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameworkSignature {
    pub names: Vec<String>,
    pub ini_names: BTreeSet<String>,
}

impl FrameworkSignature {
    pub fn new<'a>(
        names: impl IntoIterator<Item = &'a str>,
        ini_names: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut normalized: Vec<String> = names
            .into_iter()
            .map(normalized_name)
            .filter(|name| name.chars().count() >= MIN_NAME_MATCH_CHARS)
            .collect();
        normalized.sort();
        normalized.dedup();
        Self {
            names: normalized,
            ini_names: ini_names
                .into_iter()
                .map(str::to_lowercase)
                .filter(|name| {
                    let stem = name.trim_end_matches(".ini");
                    stem.chars().count() >= MIN_NAME_MATCH_CHARS
                        && !COMMON_INI_STEMS.contains(&stem)
                })
                .collect(),
        }
    }

    fn named_in(&self, normalized_text: &str) -> bool {
        self.names
            .iter()
            .any(|name| normalized_text.contains(name.as_str()))
    }
}

pub fn listing_holds_standalone_copy(
    paths: &[String],
    archive_file_name: &str,
    signature: &FrameworkSignature,
) -> bool {
    let mut has_ini = false;
    let mut shares_an_ini = false;
    let mut named = signature.named_in(&normalized_name(&file_name_without_site_ending(
        archive_file_name,
    )));
    for raw in paths {
        let path = raw.trim().replace('\\', "/").to_lowercase();
        let segments: Vec<&str> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        let Some((file, folders)) = segments.split_last() else {
            continue;
        };
        if segments.contains(&"__macosx") {
            continue;
        }
        let extension = lower_extension(file);
        if is_model_extension(extension) || extension == "dds" {
            return false;
        }
        if extension == "ini" {
            has_ini = true;
            let file = file.strip_prefix("disabled").unwrap_or(file);
            shares_an_ini |= signature.ini_names.contains(file);
            named |= signature.named_in(&normalized_name(file.trim_end_matches(".ini")));
        }
        named |= folders
            .iter()
            .any(|folder| signature.named_in(&normalized_name(folder)));
    }
    has_ini && (shares_an_ini || named)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListedContents {
    ModelsOrTextures,
    SettingsOnly,
    Unclear,
}

const NESTED_ARCHIVE_EXTENSIONS: [&str; 3] = ["zip", "7z", "rar"];

pub fn listed_contents(paths: &[String]) -> ListedContents {
    let mut has_ini = false;
    let mut has_nested_archive = false;
    for raw in paths {
        let path = raw.trim().replace('\\', "/").to_lowercase();
        let mut segments = path.split('/').filter(|segment| !segment.is_empty());
        if segments.clone().any(|segment| segment == "__macosx") {
            continue;
        }
        let Some(file) = segments.next_back() else {
            continue;
        };
        let extension = lower_extension(file);
        if is_model_extension(extension) || extension == "dds" {
            return ListedContents::ModelsOrTextures;
        }
        has_ini |= extension == "ini";
        has_nested_archive |= NESTED_ARCHIVE_EXTENSIONS.contains(&extension);
    }
    if has_ini && !has_nested_archive {
        ListedContents::SettingsOnly
    } else {
        ListedContents::Unclear
    }
}

fn is_site_collision_ending(ending: &str) -> bool {
    ending.len() == 5
        && ending
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        && ending.chars().any(|c| c.is_ascii_digit())
        && ending.chars().any(|c| c.is_ascii_alphabetic())
}

pub fn file_name_without_site_ending(file_name: &str) -> String {
    let stem = match file_name.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty()
                && extension.len() <= 4
                && extension.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem
        }
        _ => file_name,
    };
    match stem.rsplit_once(['_', '-']) {
        Some((name, ending)) if !name.is_empty() && is_site_collision_ending(ending) => {
            name.to_owned()
        }
        _ => stem.to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileNameVersion {
    line: String,
    groups: Vec<String>,
}

fn file_name_version_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^a-z0-9])(v\.?\s?\d+(?:\.\d+)*|\d+(?:\.\d+)+)(?:$|[^a-z0-9])")
            .expect("file name version pattern compiles")
    })
}

fn file_name_version(file_name: &str) -> Option<FileNameVersion> {
    let stem = file_name_without_site_ending(file_name).to_lowercase();
    let found = file_name_version_pattern()
        .captures_iter(&stem)
        .filter_map(|captures| captures.get(1))
        .last()?;
    let groups: Vec<String> = found
        .as_str()
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split('.')
        .map(str::to_owned)
        .collect();
    let line = normalized_name(&format!(
        "{} {}",
        &stem[..found.start()],
        &stem[found.end()..]
    ));
    Some(FileNameVersion { line, groups })
}

fn compare_digit_groups(mine: &str, arriving: &str) -> Option<Ordering> {
    let zero_led = |group: &str| group.len() > 1 && group.starts_with('0');
    if zero_led(mine) || zero_led(arriving) {
        return (mine.len() == arriving.len()).then(|| arriving.cmp(mine));
    }
    let mine: u64 = mine.parse().ok()?;
    let arriving: u64 = arriving.parse().ok()?;
    Some(arriving.cmp(&mine))
}

pub fn arriving_file_name_is_newer(my_file_name: &str, arriving_file_name: &str) -> Option<bool> {
    let mine = file_name_version(my_file_name)?;
    let arriving = file_name_version(arriving_file_name)?;
    if mine.line != arriving.line {
        return None;
    }
    let length = mine.groups.len().max(arriving.groups.len());
    for index in 0..length {
        let my_group = mine.groups.get(index).map(String::as_str).unwrap_or("0");
        let arriving_group = arriving
            .groups
            .get(index)
            .map(String::as_str)
            .unwrap_or("0");
        match compare_digit_groups(my_group, arriving_group)? {
            Ordering::Equal => {}
            Ordering::Greater => return Some(true),
            Ordering::Less => return Some(false),
        }
    }
    None
}

pub fn title_names_framework<'a>(
    page_title: &str,
    names: impl IntoIterator<Item = &'a str>,
) -> bool {
    let title = normalized_name(page_title);
    names.into_iter().any(|name| {
        let name = normalized_name(name);
        name.chars().count() >= MIN_NAME_MATCH_CHARS && title.contains(&name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn neonrookie(root: &Path, api: u32) {
        write(
            root,
            "NeonRookieFX.ini",
            &format!(
                "; NeonRookieFX - shared effect library.\nnamespace = NeonRookieFX\n[Constants]\nglobal $api_version = {api}\nglobal $cloth_api = 202\nglobal $required_efmi_version = 1.46\nglobal persist $version_seen = 3\n[CommandListDrawCloth]\nrun = CustomShaderCloth\n"
            ),
        );
        write(
            root,
            "Bindings/NRFX_ClothSlots_v1.ini",
            "namespace = NRFX_ClothSlots_v1\n[Constants]\nglobal $slot = 1\n",
        );
        write(
            root,
            "Shaders/Cloth_A_Main.hlsl",
            "float4 main() : SV_Target { return 0; }\n",
        );
        write(
            root,
            "README_EN.txt",
            "NeonRookieFX\nVersion 2026-11-03. API 100.\n",
        );
    }

    fn outfit(root: &Path) {
        write(
            root,
            "mod.ini",
            "namespace = SiInkSpirit\n[Constants]\nglobal $lib_ok = 0\n[TextureOverrideBody]\nhash = 1234abcd\nrun = CommandList\\NeonRookieFX\\DrawCloth\nResource\\NeonRookieFX\\Diffuse = ref ResourceDiffuse\nif $\\EFMITTL\\alpha>0&&$\\EFMIv1\\ready-$\\EFMIv1\\other\n  $lib_ok = $\\NeonRookieFX\\cloth_api\nendif\n[ResourceDiffuse]\nfilename = Textures\\Body\\diffuse.png\n",
        );
        write(root, "Textures/Body/diffuse.png", "png");
        write(root, "Body.buf", "buf");
    }

    #[test]
    fn a_framework_folder_has_no_models_and_states_its_names_and_versions() {
        let temp = tempfile::tempdir().unwrap();
        neonrookie(temp.path(), 100);
        let facts = scan_folder(temp.path());
        assert!(facts.framework_shaped());
        assert_eq!(
            facts.declared_keys().into_iter().collect::<Vec<_>>(),
            vec!["neonrookiefx".to_owned(), "nrfx_clothslots_v1".to_owned()]
        );
        assert_eq!(facts.shallowest_declared().unwrap().written, "NeonRookieFX");
        assert_eq!(
            facts
                .stated_versions
                .get("setting:neonrookiefx:api_version"),
            Some(&StatedVersion::Setting(100.0))
        );
        assert_eq!(
            facts.stated_versions.get("setting:neonrookiefx:cloth_api"),
            Some(&StatedVersion::Setting(202.0))
        );
        assert!(
            !facts
                .stated_versions
                .contains_key("setting:neonrookiefx:version_seen")
        );
        assert!(
            !facts
                .stated_versions
                .contains_key("setting:neonrookiefx:required_efmi_version")
        );
        assert_eq!(
            facts.stated_versions.get("line:readme_en.txt"),
            Some(&StatedVersion::Line {
                words: Vec::new(),
                numbers: vec![vec![2026, 11, 3]],
            })
        );
        assert!(facts.called.is_empty());
    }

    #[test]
    fn an_outfit_has_models_and_calls_names_it_does_not_declare() {
        let temp = tempfile::tempdir().unwrap();
        outfit(temp.path());
        let facts = scan_folder(temp.path());
        assert!(facts.has_models);
        assert!(
            facts.has_textures,
            "an image named by a filename line is a texture"
        );
        assert!(!facts.framework_shaped());
        assert!(facts.calls("neonrookiefx"));
        assert!(facts.calls("efmittl"));
        assert!(facts.calls("efmiv1"));
        assert!(!facts.called.contains("body"), "file paths are not calls");
        assert_eq!(
            facts
                .called_but_not_declared()
                .into_iter()
                .collect::<Vec<_>>(),
            vec![
                "efmittl".to_owned(),
                "efmiv1".to_owned(),
                "neonrookiefx".to_owned()
            ]
        );
    }

    #[test]
    fn a_preview_picture_is_not_a_texture_and_a_switched_off_copy_reads_the_same() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(DISABLED_CONTAINER);
        neonrookie(&root, 100);
        write(&root, "preview.png", "png");
        write(
            temp.path(),
            &format!("{MOD_META_DIR}/hestia.ini"),
            "namespace = hidden\n",
        );
        let facts = scan_folder(temp.path());
        assert!(facts.framework_shaped());
        assert!(!facts.declares("hidden"));
        assert_eq!(facts.shallowest_declared().unwrap().ini_depth, 0);
    }

    #[test]
    fn settings_files_read_the_same_in_every_text_encoding() {
        let text = "namespace = NeonRookieFX\n";
        let units = |to_bytes: fn(u16) -> [u8; 2]| -> Vec<u8> {
            text.encode_utf16().flat_map(to_bytes).collect()
        };
        let little_endian = [vec![0xFF, 0xFE], units(u16::to_le_bytes)].concat();
        let big_endian = [vec![0xFE, 0xFF], units(u16::to_be_bytes)].concat();
        let utf8_with_mark = [vec![0xEF, 0xBB, 0xBF], text.as_bytes().to_vec()].concat();
        let cut_off = [little_endian.clone(), vec![0x41]].concat();

        assert_eq!(decode_text(text.as_bytes()), text);
        assert_eq!(decode_text(&utf8_with_mark), text);
        assert_eq!(decode_text(&little_endian), text);
        assert_eq!(decode_text(&big_endian), text);
        assert_eq!(decode_text(&cut_off), text);
    }

    #[test]
    fn calls_are_found_next_to_operators_and_inside_longer_names() {
        let mut called = BTreeSet::new();
        collect_called_namespaces("x = $\\A\\one-$\\B\\two", &mut called);
        collect_called_namespaces(
            "ps-t0 = ref Resource\\shaderfixes\\help.ini\\Font",
            &mut called,
        );
        collect_called_namespaces("run = MyResource\\Nope\\thing", &mut called);
        collect_called_namespaces("run = CommandListLocal", &mut called);
        assert_eq!(
            called.into_iter().collect::<Vec<_>>(),
            vec![
                "a".to_owned(),
                "b".to_owned(),
                "shaderfixes\\help.ini".to_owned()
            ]
        );
    }

    #[test]
    fn the_inside_check_needs_one_higher_statement_and_none_lower() {
        let mine = tempfile::tempdir().unwrap();
        let arriving = tempfile::tempdir().unwrap();
        neonrookie(mine.path(), 100);
        neonrookie(arriving.path(), 101);
        let my_facts = scan_folder(mine.path());
        let arriving_facts = scan_folder(arriving.path());
        assert_eq!(inside_check(&my_facts, &arriving_facts), InsideCheck::Newer);
        assert_eq!(
            inside_check(&arriving_facts, &my_facts),
            InsideCheck::SameOrOlder
        );
        assert_eq!(inside_check(&my_facts, &my_facts), InsideCheck::SameOrOlder);

        let mut mixed = arriving_facts.clone();
        mixed.stated_versions.insert(
            "setting:neonrookiefx:cloth_api".to_owned(),
            StatedVersion::Setting(201.0),
        );
        assert_eq!(inside_check(&my_facts, &mixed), InsideCheck::CannotTell);

        let mut silent = arriving_facts.clone();
        silent.stated_versions.clear();
        assert_eq!(inside_check(&my_facts, &silent), InsideCheck::CannotTell);

        let other = tempfile::tempdir().unwrap();
        write(
            other.path(),
            "translucency.ini",
            "namespace = EFMITTL\n[Constants]\nglobal locked $version = 0.52\n",
        );
        let other_facts = scan_folder(other.path());
        assert_eq!(
            other_facts.stated_versions.get("setting:efmittl:version"),
            Some(&StatedVersion::Setting(0.52))
        );
        assert_eq!(
            inside_check(&my_facts, &other_facts),
            InsideCheck::DifferentFramework
        );
    }

    #[test]
    fn settings_compare_as_numbers_and_lines_only_with_the_same_words() {
        let setting = |value| StatedVersion::Setting(value);
        assert_eq!(
            arriving_against_mine(&setting(0.52), &setting(0.6)),
            Some(Ordering::Greater)
        );
        let main = version_line("; RabbitFX -Arknights Endfield- Version 2.8").unwrap();
        let experimental =
            version_line("; RabbitFX -Arknights Endfield- Version 4.6 Experimental 4").unwrap();
        let later =
            version_line("; RabbitFX -Arknights Endfield- Version 4.6 Experimental 5").unwrap();
        assert_eq!(arriving_against_mine(&main, &experimental), None);
        assert_eq!(
            arriving_against_mine(&experimental, &later),
            Some(Ordering::Greater)
        );
        assert_eq!(
            arriving_against_mine(&later, &experimental),
            Some(Ordering::Less)
        );
        let prose = version_line("; Version 1.2 - fixed the lace in every pass").unwrap();
        let next = version_line("; Version 1.10 - two new profiles").unwrap();
        assert_eq!(
            arriving_against_mine(&prose, &next),
            Some(Ordering::Greater)
        );
        assert!(version_line("; requires nothing").is_none());
    }

    #[test]
    fn file_names_compare_only_on_the_same_line_with_clear_numbers() {
        assert_eq!(
            file_name_without_site_ending("neonrookiefx_v2_c99f1.zip"),
            "neonrookiefx_v2"
        );
        assert_eq!(
            file_name_without_site_ending("katfxmatcap_de7ad.zip"),
            "katfxmatcap"
        );
        assert_eq!(
            file_name_without_site_ending("efmittl-052.zip"),
            "efmittl-052"
        );
        assert_eq!(
            arriving_file_name_is_newer("neonrookiefx_v2_c99f1.zip", "neonrookiefx_v3_0a1b2.zip"),
            Some(true)
        );
        assert_eq!(
            arriving_file_name_is_newer("neonrookiefx_v2.1.zip", "NeonRookieFX_v2.zip"),
            Some(false)
        );
        assert_eq!(
            arriving_file_name_is_newer("neonrookiefx_v2.zip", "neonrookiefx_v2_c99f1.zip"),
            None
        );
        assert_eq!(
            arriving_file_name_is_newer("rabbitfx_v2.8.zip", "rabbitfx_experimental_v4.6.zip"),
            None
        );
        assert_eq!(
            arriving_file_name_is_newer("efmittl-052.zip", "efmittl-06.zip"),
            None
        );
        assert_eq!(
            arriving_file_name_is_newer("lib_v0.52.zip", "lib_v0.60.zip"),
            Some(true)
        );
        assert_eq!(
            arriving_file_name_is_newer("lib_v0.052.zip", "lib_v0.6.zip"),
            None
        );
        assert_eq!(
            arriving_file_name_is_newer("28_2d258.zip", "v46e4.zip"),
            None
        );
    }

    #[test]
    fn a_listing_is_a_copy_when_it_has_no_models_and_shows_the_framework() {
        let signature =
            FrameworkSignature::new(["NeonRookieFX", "nrfx_clothslots_v1"], ["neonrookiefx.ini"]);
        let list = |lines: &[&str]| {
            lines
                .iter()
                .map(|line| (*line).to_owned())
                .collect::<Vec<_>>()
        };
        assert!(listing_holds_standalone_copy(
            &list(&[
                "NeonRookieFX/NeonRookieFX.ini",
                "NeonRookieFX/Shaders/Cloth_B.hlsl"
            ]),
            "neonrookiefx_v2_c99f1.zip",
            &signature
        ));
        assert!(!listing_holds_standalone_copy(
            &list(&["Si/NeonRookieFX/NeonRookieFX.ini", "Si/Body.buf"]),
            "si_ink_spirit.zip",
            &signature
        ));
        assert!(!listing_holds_standalone_copy(
            &list(&["Addon/Stocking_Addon.ini"]),
            "stocking_addon.zip",
            &signature
        ));
        let matcap = FrameworkSignature::new(["katfxmatcap"], []);
        assert!(listing_holds_standalone_copy(
            &list(&["Matcap.ini"]),
            "katfxmatcap_de7ad.zip",
            &matcap
        ));
        assert!(!listing_holds_standalone_copy(
            &list(&["readme.txt"]),
            "katfxmatcap_de7ad.zip",
            &matcap
        ));
    }

    #[test]
    fn a_listing_is_settings_only_when_it_shows_an_ini_and_nothing_to_draw() {
        let list = |lines: &[&str]| {
            lines
                .iter()
                .map(|line| (*line).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            listed_contents(&list(&[
                "NeonRookieFX/NeonRookieFX.ini",
                "NeonRookieFX\\Shaders\\Cloth_B.hlsl"
            ])),
            ListedContents::SettingsOnly
        );
        assert_eq!(
            listed_contents(&list(&["Si/mod.ini", "Si/Body.BUF"])),
            ListedContents::ModelsOrTextures
        );
        assert_eq!(
            listed_contents(&list(&["Si/mod.ini", "Si/Textures/diffuse.dds"])),
            ListedContents::ModelsOrTextures
        );
        assert_eq!(
            listed_contents(&list(&["Si/mod.ini", "Si/Body.vb0"])),
            ListedContents::ModelsOrTextures
        );
        assert_eq!(
            listed_contents(&list(&["notes.ini", "Si.7z"])),
            ListedContents::Unclear
        );
        assert_eq!(
            listed_contents(&list(&["readme.txt", "Shaders/"])),
            ListedContents::Unclear
        );
        assert_eq!(
            listed_contents(&list(&["__MACOSX/Si/._Body.buf", "Si/mod.ini"])),
            ListedContents::SettingsOnly
        );
        assert_eq!(listed_contents(&[]), ListedContents::Unclear);
    }

    #[test]
    fn a_page_is_the_frameworks_own_when_its_title_holds_the_name() {
        assert!(title_names_framework(
            "EFMI TTL-A General Method for Creating Translucent",
            ["EFMITTL"]
        ));
        assert!(title_names_framework(
            "EFMI RabbitFX - Glow FX + Decensor",
            ["RabbitFX"]
        ));
        assert!(!title_names_framework(
            "Si - Ink Spirit",
            ["NeonRookieFX", "nrfx_clothslots_v1"]
        ));
        assert!(!title_names_framework("Si - Ink Spirit", ["Si"]));
    }

    fn installed(mod_id: &str, root: &Path) -> (InstalledFolder, Arc<FolderFacts>) {
        (
            InstalledFolder {
                mod_id: mod_id.to_owned(),
                root_path: root.to_path_buf(),
                ini_hash: None,
                content_mtime: None,
                recorded_framework: false,
            },
            Arc::new(scan_folder(root)),
        )
    }

    #[test]
    fn a_candidate_learns_who_calls_it_and_which_copy_is_installed() {
        let library = tempfile::tempdir().unwrap();
        let outfit_root = library.path().join("Si - Ink Spirit");
        let framework_root = library.path().join("NeonRookieFX");
        outfit(&outfit_root);
        neonrookie(&framework_root, 100);
        let mods = vec![
            installed("outfit", &outfit_root),
            installed("framework", &framework_root),
        ];
        assert_eq!(
            installed_framework_ids(&mods)
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["framework".to_owned()]
        );

        let download = tempfile::tempdir().unwrap();
        let candidate = download.path().join("NeonRookieFX");
        neonrookie(&candidate, 101);
        let scan = scan_candidate(&candidate, true, &mods);
        assert_eq!(scan.inner_folder_name.as_deref(), Some("NeonRookieFX"));
        assert_eq!(scan.callers, vec!["outfit".to_owned()]);
        assert_eq!(scan.installed_copies.len(), 1);
        assert_eq!(scan.installed_copies[0].mod_id, "framework");
        assert_eq!(scan.installed_copies[0].inside_check, InsideCheck::Newer);
    }

    #[test]
    fn only_a_self_contained_copy_of_an_installed_framework_can_be_moved_out() {
        let library = tempfile::tempdir().unwrap();
        let outfit_root = library.path().join("Si - Ink Spirit");
        let framework_root = library.path().join("NeonRookieFX");
        outfit(&outfit_root);
        neonrookie(&framework_root, 100);
        let mods = vec![
            installed("outfit", &outfit_root),
            installed("framework", &framework_root),
        ];

        let download = tempfile::tempdir().unwrap();
        outfit(download.path());
        neonrookie(&download.path().join("NeonRookieFX"), 101);
        write(
            download.path(),
            "Shaders/Own.ini",
            "namespace = SiOwnShaders\n[Constants]\nglobal $x = 1\n",
        );
        let parts = movable_framework_parts(download.path(), &mods);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].relative_dir, PathBuf::from("NeonRookieFX"));

        write(
            download.path(),
            "Shaders/Own.hlsl",
            "#include \"../NeonRookieFX/Include/NRFX_v1.hlsli\"\n",
        );
        assert!(movable_framework_parts(download.path(), &mods).is_empty());
    }

    fn merged_parts<'a>(roots: &[&'a Path]) -> Vec<BundleFolder<'a>> {
        roots
            .iter()
            .copied()
            .map(|root| BundleFolder {
                root,
                merges_into_the_update_target: true,
            })
            .collect()
    }

    fn context_of(installed: &[&InstalledFolder]) -> InstallContext {
        InstallContext {
            installed: installed.iter().map(|folder| (*folder).clone()).collect(),
        }
    }

    #[test]
    fn a_merge_part_is_kept_out_when_it_is_a_framework_the_bundle_or_the_library_uses() {
        let library = tempfile::tempdir().unwrap();
        let installed_outfit_root = library.path().join("Si - Ink Spirit");
        outfit(&installed_outfit_root);
        let (installed_outfit, _) = installed("outfit", &installed_outfit_root);

        let download = tempfile::tempdir().unwrap();
        let outfit_part = download.path().join("outfit");
        let framework_part = download.path().join("framework");
        let addon_part = download.path().join("addon");
        outfit(&outfit_part);
        neonrookie(&framework_part, 101);
        write(
            &addon_part,
            "Stocking_Addon.ini",
            "namespace = SiStockingAddon\n[Constants]\nglobal $on = 1\n",
        );

        assert_eq!(
            frameworks_among_bundle_folders(
                &merged_parts(&[&outfit_part, &framework_part, &addon_part]),
                Some(&installed_outfit),
                &context_of(&[&installed_outfit]),
            ),
            vec![false, true, false],
            "the outfit part calls the framework; nothing calls the add-on"
        );

        assert_eq!(
            frameworks_among_bundle_folders(
                &merged_parts(&[&framework_part]),
                Some(&installed_outfit),
                &context_of(&[&installed_outfit]),
            ),
            vec![true],
            "an installed mod calls the framework"
        );

        assert_eq!(
            frameworks_among_bundle_folders(
                &merged_parts(&[&framework_part]),
                None,
                &InstallContext::default(),
            ),
            vec![false],
            "nothing calls it and it is not installed as a framework"
        );

        let installed_framework_root = library.path().join("NeonRookieFX");
        neonrookie(&installed_framework_root, 100);
        let (mut installed_framework, _) = installed("framework", &installed_framework_root);
        assert_eq!(
            frameworks_among_bundle_folders(
                &merged_parts(&[&framework_part]),
                None,
                &context_of(&[&installed_framework]),
            ),
            vec![false],
            "a look-alike folder nobody recorded as a framework does not count"
        );
        installed_framework.recorded_framework = true;
        assert_eq!(
            frameworks_among_bundle_folders(
                &merged_parts(&[&framework_part]),
                None,
                &context_of(&[&installed_framework]),
            ),
            vec![true],
            "it is already installed as a framework"
        );

        let separate = [
            BundleFolder {
                root: &outfit_part,
                merges_into_the_update_target: true,
            },
            BundleFolder {
                root: &framework_part,
                merges_into_the_update_target: false,
            },
        ];
        assert_eq!(
            frameworks_among_bundle_folders(
                &separate,
                Some(&installed_outfit),
                &context_of(&[&installed_outfit]),
            ),
            vec![false, false],
            "a part that already installs on its own goes through the normal review"
        );
    }

    #[test]
    fn a_merge_part_stays_merged_when_the_outfit_uses_that_name_itself() {
        let library = tempfile::tempdir().unwrap();
        let outfit_with_merged_copy = library.path().join("Si - Ink Spirit");
        outfit(&outfit_with_merged_copy);
        neonrookie(&outfit_with_merged_copy.join("NeonRookieFX"), 100);
        let (installed_outfit, _) = installed("outfit", &outfit_with_merged_copy);

        let download = tempfile::tempdir().unwrap();
        let framework_part = download.path().join("framework");
        neonrookie(&framework_part, 101);
        assert_eq!(
            frameworks_among_bundle_folders(
                &merged_parts(&[&framework_part]),
                Some(&installed_outfit),
                &context_of(&[&installed_outfit]),
            ),
            vec![false],
            "the outfit being updated carries its own merged copy"
        );

        let plain_outfit_root = library.path().join("Plain");
        outfit(&plain_outfit_root);
        let (plain_outfit, _) = installed("plain", &plain_outfit_root);
        let outfit_part_with_copy = download.path().join("outfit");
        outfit(&outfit_part_with_copy);
        neonrookie(&outfit_part_with_copy.join("NeonRookieFX"), 101);
        assert_eq!(
            frameworks_among_bundle_folders(
                &merged_parts(&[&outfit_part_with_copy, &framework_part]),
                Some(&plain_outfit),
                &context_of(&[&plain_outfit]),
            ),
            vec![false, false],
            "the outfit's new file brings its own copy under that name"
        );
    }
}
