//! Exact numeric V2 feature extraction for raw GameBanana matcher rows.
//!
//! The implementation intentionally mirrors the frozen Python extractor in
//! `hestia-sequence-matcher-20261021/trainer/matcher.py`.  `None` means the
//! Python value was NaN/missing; finite values are otherwise returned without
//! imputation or scaling.

use regex::Regex;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

#[path = "generated.rs"]
pub(crate) mod generated;

pub const NUMERIC_FEATURES: [&str; 92] = [
    "filename_char_similarity",
    "filename_token_jaccard",
    "filename_source_coverage",
    "filename_candidate_coverage",
    "description_char_similarity",
    "description_token_jaccard",
    "description_source_coverage",
    "description_candidate_coverage",
    "extension_equal",
    "source_label_present",
    "candidate_label_present",
    "label_set_equal",
    "label_jaccard",
    "log_file_size_ratio",
    "relative_file_age_days",
    "source_archived",
    "candidate_archived",
    "source_present_catalog",
    "candidate_present_catalog",
    "catalog_active_count",
    "catalog_archived_count",
    "source_newer_peer_count",
    "candidate_newer_peer_count",
    "version_source_present",
    "version_candidate_present",
    "version_source_valid",
    "version_candidate_valid",
    "version_equal",
    "version_candidate_newer",
    "version_candidate_older",
    "version_component_delta",
    "version_source_catalog_repeat_count",
    "version_candidate_catalog_repeat_count",
    "logs_known",
    "update_log_count",
    "update_attachment_total",
    "source_update_attachment_count",
    "candidate_update_attachment_count",
    "source_candidate_same_update_count",
    "source_only_update_count",
    "candidate_only_update_count",
    "source_single_file_binding_count",
    "candidate_single_file_binding_count",
    "source_multi_file_binding_count",
    "candidate_multi_file_binding_count",
    "source_linked_earliest_rank",
    "source_linked_latest_rank",
    "candidate_linked_earliest_rank",
    "candidate_linked_latest_rank",
    "linked_release_order",
    "linked_attachment_set_mean_size",
    "source_candidate_attachment_overlap",
    "active_catalog_log_coverage",
    "source_log_version_match_count",
    "candidate_log_version_match_count",
    "distinct_log_version_count",
    "candidate_date_after_source",
    "catalog_name_source_mean",
    "catalog_name_source_max",
    "catalog_name_source_median",
    "catalog_name_source_q75",
    "catalog_name_candidate_mean",
    "catalog_name_candidate_max",
    "catalog_name_candidate_median",
    "catalog_name_candidate_q75",
    "catalog_desc_source_mean",
    "catalog_desc_source_max",
    "catalog_desc_source_median",
    "catalog_desc_source_q75",
    "catalog_desc_candidate_mean",
    "catalog_desc_candidate_max",
    "catalog_desc_candidate_median",
    "catalog_desc_candidate_q75",
    "catalog_sibling_overlap_source_mean",
    "catalog_sibling_overlap_candidate_mean",
    "shared_token_idf_sum",
    "shared_token_idf_mean",
    "source_only_token_rarity",
    "candidate_only_token_rarity",
    "source_token_count",
    "candidate_token_count",
    "shared_token_count",
    "source_date_rank",
    "candidate_date_rank",
    "source_newer_catalog_peer_count",
    "candidate_newer_catalog_peer_count",
    "source_filename_number_missing",
    "candidate_filename_number_missing",
    "filename_number_shared",
    "filename_number_candidate_newer",
    "filename_number_source_count",
    "filename_number_candidate_count",
];

const ID_KEYS: [&str; 6] = ["_idRow", "id", "idRow", "file_id", "fileId", "_id"];
const NAME_KEYS: [&str; 6] = [
    "_sFile",
    "_sFileName",
    "filename",
    "file_name",
    "name",
    "_sName",
];
const DESC_KEYS: [&str; 4] = ["_sDescription", "description", "desc", "_sText"];
const VERSION_KEYS: [&str; 3] = ["_sVersion", "version", "_sFileVersion"];
const SIZE_KEYS: [&str; 7] = [
    "_nFilesize",
    "_nFileSize",
    "_nSize",
    "file_size",
    "size",
    "size_bytes",
    "_nDownloadSize",
];
const DATE_KEYS: [&str; 9] = [
    "_tsDateAdded",
    "_tsDateModified",
    "_tsDateUpdated",
    "_sDateAdded",
    "_sDateModified",
    "_sDateUpdated",
    "date_added",
    "date_modified",
    "date_updated",
];
const DATE_KEYS_EXTRA: [&str; 2] = ["created_at", "updated_at"];
const ARCHIVE_KEYS: [&str; 4] = ["_bIsArchived", "archived", "is_archived", "_bArchived"];
const TRASH_KEYS: [&str; 4] = ["_bIsTrashed", "trashed", "is_trashed", "_bTrashed"];
const ACTIVE_KEYS: [&str; 3] = ["_bIsActive", "active", "is_active"];
const LABEL_KEYS: [&str; 6] = ["_aTags", "_aLabels", "labels", "tags", "_sLabel", "label"];

fn map(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

fn first<'a>(
    value: &'a Map<String, Value>,
    keys: &[&str],
    default: Option<&'a Value>,
) -> Option<&'a Value> {
    for key in keys {
        if let Some(item) = value.get(*key) {
            if !item.is_null() {
                return Some(item);
            }
        }
    }
    default
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Object(object) => object
            .values()
            .map(value_text)
            .collect::<Vec<_>>()
            .join(" "),
        Value::Array(array) => array.iter().map(value_text).collect::<Vec<_>>().join(" "),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => number.to_string(),
    }
}

fn python_id(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(_)) => String::new(),
        Some(value) => value_text(value).trim().to_string(),
    }
}

fn item_id(item: &Value) -> String {
    map(item)
        .and_then(|object| first(object, &ID_KEYS, None))
        .map(|v| python_id(Some(v)))
        .unwrap_or_default()
}

fn text_field(item: &Value, keys: &[&str]) -> String {
    map(item)
        .and_then(|object| first(object, keys, Some(&Value::Null)))
        .map(value_text)
        .unwrap_or_default()
}

fn name(item: &Value) -> String {
    text_field(item, &NAME_KEYS)
}
fn desc(item: &Value) -> String {
    text_field(item, &DESC_KEYS)
}
fn version(item: &Value) -> String {
    text_field(item, &VERSION_KEYS).trim().to_string()
}

fn all_date_keys() -> [&'static str; 11] {
    [
        DATE_KEYS[0],
        DATE_KEYS[1],
        DATE_KEYS[2],
        DATE_KEYS[3],
        DATE_KEYS[4],
        DATE_KEYS[5],
        DATE_KEYS[6],
        DATE_KEYS[7],
        DATE_KEYS[8],
        DATE_KEYS_EXTRA[0],
        DATE_KEYS_EXTRA[1],
    ]
}

fn is_python_space(c: char) -> bool {
    generated::is_space(c as u32)
}

fn clean_text_raw(input: &str) -> String {
    let unescaped = html_unescape(input);
    let url = strip_urls(&unescaped);
    let tags = tag_regex().replace_all(&url, " ");
    let normalized = generated::nfc(tags.as_ref());
    let mut output = String::with_capacity(normalized.len());
    let mut pending_space = false;
    for c in normalized.chars() {
        if is_python_space(c) {
            pending_space = true;
            continue;
        }
        if pending_space && !output.is_empty() {
            output.push(' ');
        }
        pending_space = false;
        output.push(c);
    }
    output
}

fn clean_text(value: &Value) -> String {
    clean_text_raw(&value_text(value))
}

fn tokens(value: &Value) -> Vec<String> {
    let cleaned = clean_text(value);
    let mut result = Vec::new();
    let mut current = String::new();
    for c in cleaned.chars() {
        if generated::is_alnum(c as u32) {
            current.extend(generated::casefold(c));
        } else if !current.is_empty() {
            result.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn token_set(value: &str) -> HashSet<String> {
    tokens(&Value::String(value.to_string()))
        .into_iter()
        .collect()
}

fn python_number(value: Option<&Value>) -> f64 {
    let value = match value {
        None | Some(Value::Null) | Some(Value::Bool(_)) => return f64::NAN,
        Some(v) => v,
    };
    match value {
        Value::Number(number) => number
            .as_f64()
            .filter(|v| v.is_finite())
            .unwrap_or(f64::NAN),
        _ => number_from_text(&value_text(value)),
    }
}

fn number_from_text(text: &str) -> f64 {
    let text = text.replace(',', "");
    let chars: Vec<char> = text.chars().collect();
    let mut start = None;
    for (i, character) in chars.iter().enumerate() {
        if generated::decimal_digit(*character as u32).is_some()
            || ((*character == '+' || *character == '-')
                && chars
                    .get(i + 1)
                    .is_some_and(|next| generated::decimal_digit(*next as u32).is_some()))
        {
            start = Some(i);
            break;
        }
    }
    let start = match start {
        Some(value) => value,
        None => return f64::NAN,
    };
    let mut end = start + usize::from(chars[start] == '+' || chars[start] == '-');
    while end < chars.len() && generated::decimal_digit(chars[end] as u32).is_some() {
        end += 1;
    }
    if end < chars.len() && chars[end] == '.' {
        let decimal = end + 1;
        let mut cursor = decimal;
        while cursor < chars.len() && generated::decimal_digit(chars[cursor] as u32).is_some() {
            cursor += 1;
        }
        if cursor > decimal {
            end = cursor;
        }
    }
    let mut ascii = String::new();
    for character in &chars[start..end] {
        if let Some(digit) = generated::decimal_digit(*character as u32) {
            ascii.push(char::from(b'0' + digit));
        } else {
            ascii.push(*character);
        }
    }
    ascii.parse::<f64>().unwrap_or(f64::NAN)
}

fn size(item: &Value) -> f64 {
    map(item)
        .and_then(|object| first(object, &SIZE_KEYS, None))
        .map(|v| python_number(Some(v)))
        .unwrap_or(f64::NAN)
}

fn timestamp(item: &Value) -> f64 {
    let object = match map(item) {
        Some(value) => value,
        None => return f64::NAN,
    };
    let value = match first(object, &all_date_keys(), None) {
        Some(value) => value,
        None => return f64::NAN,
    };
    match value {
        Value::Bool(value) => f64::from(*value),
        Value::Number(number) => {
            let number = match number.as_f64() {
                Some(number) if number.is_finite() => number,
                _ => return f64::NAN,
            };
            if number > 100_000_000_000.0 {
                number / 1000.0
            } else {
                number
            }
        }
        _ => parse_iso_timestamp(&value_text(value).trim().replace('Z', "+00:00")),
    }
}

fn parse_iso_timestamp(value: &str) -> f64 {
    // GameBanana emits RFC3339 timestamps. Keep a small dependency-free parser
    // for the Python datetime.fromisoformat forms used in the fixtures.
    let (date, time) = match value.split_once('T').or_else(|| value.split_once(' ')) {
        Some(parts) => parts,
        None => (value, "00:00:00"),
    };
    let date_parts: Vec<_> = date.split('-').collect();
    if date_parts.len() != 3 {
        return f64::NAN;
    }
    let year: i32 = match date_parts[0].parse() {
        Ok(v) => v,
        Err(_) => return f64::NAN,
    };
    let month: u32 = match date_parts[1].parse() {
        Ok(v) => v,
        Err(_) => return f64::NAN,
    };
    let day: u32 = match date_parts[2].parse() {
        Ok(v) => v,
        Err(_) => return f64::NAN,
    };
    let (clock, offset) = if let Some(index) = time.find('+') {
        (&time[..index], Some(&time[index..]))
    } else if let Some(index) = time[1..].find('-') {
        let index = index + 1;
        (&time[..index], Some(&time[index..]))
    } else {
        (time, None)
    };
    let mut parts = clock.split(':');
    let hour: i64 = match parts.next().and_then(|v| v.parse().ok()) {
        Some(v) => v,
        None => return f64::NAN,
    };
    let minute: i64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let second_text = parts.next().unwrap_or("0");
    let second: f64 = match second_text.parse() {
        Ok(v) => v,
        Err(_) => return f64::NAN,
    };
    let days = days_from_civil(year, month, day);
    let mut result = days as f64 * 86_400.0 + hour as f64 * 3600.0 + minute as f64 * 60.0 + second;
    if let Some(offset) = offset {
        if offset == "Z" {
            return result;
        }
        let sign = if offset.starts_with('-') { -1.0 } else { 1.0 };
        let raw = offset.trim_start_matches(['+', '-']);
        let mut offset_parts = raw.split(':');
        let hours: f64 = match offset_parts.next().and_then(|v| v.parse().ok()) {
            Some(v) => v,
            None => return f64::NAN,
        };
        let minutes: f64 = offset_parts
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        result -= sign * (hours * 3600.0 + minutes * 60.0);
    }
    result
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = year as i64 - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn date_delta(source: &Value, candidate: &Value) -> f64 {
    let (left, right) = (timestamp(source), timestamp(candidate));
    if left.is_finite() && right.is_finite() {
        (right - left) / 86_400.0
    } else {
        f64::NAN
    }
}

fn boolean_text(value: &Value, true_word: &str) -> f64 {
    match value {
        Value::Bool(value) => f64::from(*value),
        Value::Number(number) => f64::from(number.as_f64().unwrap_or(0.0) != 0.0),
        _ => f64::from(
            matches!(
                generated::casefold_str(&value_text(value)).as_str(),
                "1" | "true" | "yes"
            ) || generated::casefold_str(&value_text(value)) == true_word,
        ),
    }
}

fn archive(item: &Value) -> f64 {
    let object = match map(item) {
        Some(value) => value,
        None => return 0.0,
    };
    if let Some(value) = first(object, &ARCHIVE_KEYS, None) {
        return boolean_text(value, "archived");
    }
    if let Some(value) = first(object, &TRASH_KEYS, None) {
        return boolean_text(value, "trashed");
    }
    if let Some(value) = first(object, &ACTIVE_KEYS, None) {
        return 1.0 - boolean_text(value, "active");
    }
    0.0
}

fn labels(item: &Value) -> Vec<String> {
    let object = match map(item) {
        Some(value) => value,
        None => return Vec::new(),
    };
    let default = Value::Array(Vec::new());
    let value = match first(object, &LABEL_KEYS, Some(&default)) {
        Some(value) => value,
        None => return Vec::new(),
    };
    let values: Vec<&Value> = match value {
        Value::Object(object) => object.values().collect(),
        Value::Array(values) => values.iter().collect(),
        _ => vec![value],
    };
    values
        .into_iter()
        .filter_map(|value| {
            let cleaned = clean_text(value);
            if cleaned.is_empty() {
                None
            } else {
                Some(generated::casefold_str(&cleaned))
            }
        })
        .collect()
}

fn sequence_ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let positions = {
        let mut map: HashMap<char, Vec<usize>> = HashMap::new();
        for (index, value) in b.iter().enumerate() {
            map.entry(*value).or_default().push(index);
        }
        map
    };
    let mut matching = 0usize;
    let mut stack = vec![(0usize, a.len(), 0usize, b.len())];
    while let Some((alo, ahi, blo, bhi)) = stack.pop() {
        if let Some((i, j, size)) = longest_match(&a, &b, &positions, alo, ahi, blo, bhi) {
            if size == 0 {
                continue;
            }
            matching += size;
            if alo < i && blo < j {
                stack.push((alo, i, blo, j));
            }
            if i + size < ahi && j + size < bhi {
                stack.push((i + size, ahi, j + size, bhi));
            }
        }
    }
    2.0 * matching as f64 / (a.len() + b.len()) as f64
}

fn longest_match(
    a: &[char],
    b: &[char],
    positions: &HashMap<char, Vec<usize>>,
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> Option<(usize, usize, usize)> {
    let mut best_i = alo;
    let mut best_j = blo;
    let mut best_size = 0usize;
    let mut previous: HashMap<usize, usize> = HashMap::new();
    for i in alo..ahi {
        let mut current = HashMap::new();
        if let Some(indices) = positions.get(&a[i]) {
            for &j in indices.iter().filter(|&&j| j >= blo && j < bhi) {
                let size = previous.get(&j.wrapping_sub(1)).copied().unwrap_or(0) + 1;
                current.insert(j, size);
                if size > best_size {
                    best_i = i + 1 - size;
                    best_j = j + 1 - size;
                    best_size = size;
                }
            }
        }
        previous = current;
    }
    let mut best_i = best_i;
    let mut best_j = best_j;
    let mut best_size = best_size;
    while best_i > alo && best_j > blo && a[best_i - 1] == b[best_j - 1] {
        best_i -= 1;
        best_j -= 1;
        best_size += 1;
    }
    while best_i + best_size < ahi
        && best_j + best_size < bhi
        && a[best_i + best_size] == b[best_j + best_size]
    {
        best_size += 1;
    }
    Some((best_i, best_j, best_size))
}

fn jaccard(left: &str, right: &str) -> f64 {
    let a = token_set(left);
    let b = token_set(right);
    if a.is_empty() && b.is_empty() {
        1.0
    } else {
        a.intersection(&b).count() as f64 / a.union(&b).count() as f64
    }
}

fn coverage(left: &str, right: &str) -> f64 {
    let a = token_set(left);
    let b = token_set(right);
    if a.is_empty() {
        0.0
    } else {
        a.intersection(&b).count() as f64 / a.len() as f64
    }
}

fn similarity(left: &str, right: &str) -> f64 {
    sequence_ratio(
        &generated::casefold_str(&clean_text_raw(left)),
        &generated::casefold_str(&clean_text_raw(right)),
    )
}

fn unwrap_profile(profile: &Value) -> Vec<Value> {
    let object = match map(profile) {
        Some(value) => value,
        None => return Vec::new(),
    };
    let mut values = Vec::new();
    for key in ["_aArchivedFiles", "archived_files", "_aArchivedFileRows"] {
        if let Some(value) = object.get(key) {
            let items: Vec<&Value> = match value {
                Value::Object(object) => object.values().collect(),
                Value::Array(values) => values.iter().collect(),
                _ => Vec::new(),
            };
            for item in items {
                let mut entry = map(item).cloned().unwrap_or_default();
                entry.insert("_bIsArchived".to_string(), Value::Bool(true));
                values.push(Value::Object(entry));
            }
        }
    }
    for key in ["_aFiles", "files", "_aFileRows", "file_rows"] {
        if let Some(value) = object.get(key) {
            match value {
                Value::Object(object) => values.extend(object.values().cloned()),
                Value::Array(array) => values.extend(array.iter().cloned()),
                _ => {}
            }
        }
    }
    if let Some(Value::Object(_)) = object.get("profile") {
        values.extend(unwrap_profile(&object["profile"]));
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        match map(&value) {
            Some(value) if !value.is_empty() => value,
            _ => continue,
        };
        let id = item_id(&value);
        if id.is_empty() && name(&value).is_empty() {
            continue;
        }
        if !id.is_empty() && !seen.insert(id) {
            continue;
        }
        result.push(value);
    }
    result
}

fn catalog_file(profile: &Value, item: &Value) -> Value {
    let id = item_id(item);
    for entry in unwrap_profile(profile) {
        if !id.is_empty() && item_id(&entry) == id {
            return entry;
        }
    }
    map(item)
        .cloned()
        .map(Value::Object)
        .unwrap_or(Value::Object(Map::new()))
}

fn extract_ids(value: &Value, result: &mut HashSet<String>) {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<&str> = ID_KEYS.to_vec();
            keys.push("_idFile");
            for key in keys {
                if let Some(value) = object.get(key) {
                    let id = python_id(Some(value));
                    if !id.is_empty() {
                        result.insert(id);
                    }
                }
            }
            for key in [
                "_aFileRowIds",
                "_aFiles",
                "files",
                "file_ids",
                "fileIds",
                "attachments",
            ] {
                if let Some(value) = object.get(key) {
                    extract_ids(value, result);
                }
            }
            for (key, nested) in object {
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| generated::decimal_digit(c as u32).is_some())
                    && matches!(nested, Value::Object(_))
                {
                    result.insert(key.clone());
                }
            }
        }
        Value::Array(array) => {
            for item in array {
                extract_ids(item, result);
            }
        }
        _ => {
            let id = python_id(Some(value));
            if !id.is_empty() {
                result.insert(id);
            }
        }
    }
}

fn attachments(log: &Value) -> HashSet<String> {
    let mut result = HashSet::new();
    if let Some(object) = map(log) {
        for key in [
            "_aFileRowIds",
            "_aFiles",
            "files",
            "file_ids",
            "fileIds",
            "attachments",
        ] {
            if let Some(value) = object.get(key) {
                extract_ids(value, &mut result);
            }
        }
    }
    result
}

fn logs(value: Option<&Value>) -> (Vec<Value>, bool) {
    let value = match value {
        None | Some(Value::Null) => return (Vec::new(), false),
        Some(value) => value,
    };
    match value {
        Value::Array(values) => (
            values
                .iter()
                .filter(|value| map(value).is_some_and(|object| !object.is_empty()))
                .cloned()
                .collect(),
            true,
        ),
        _ => (Vec::new(), false),
    }
}

/// A non-negative decimal integer with no machine-width limit.
///
/// Python's `int()` accepts arbitrary-size Unicode decimal runs.  Keeping the
/// canonical digits here preserves the tuple equality/order operations used by
/// the feature extractor without pulling a bigint dependency into the native
/// crate.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DecimalInt(String);

impl DecimalInt {
    fn from_ascii_digits(digits: String) -> Option<Self> {
        if digits.is_empty() {
            return None;
        }
        let first_nonzero = digits.as_bytes().iter().position(|digit| *digit != b'0');
        Some(Self(match first_nonzero {
            Some(index) => digits[index..].to_owned(),
            None => "0".to_owned(),
        }))
    }

    fn from_decimal_chars(chars: impl IntoIterator<Item = char>) -> Option<Self> {
        let mut digits = String::new();
        for character in chars {
            let digit = generated::decimal_digit(character as u32)?;
            digits.push(char::from(b'0' + digit));
        }
        Self::from_ascii_digits(digits)
    }

    fn is_zero(&self) -> bool {
        self.0 == "0"
    }

    fn add(&self, other: &Self) -> Self {
        let left = self.0.as_bytes();
        let right = other.0.as_bytes();
        let mut index_left = left.len();
        let mut index_right = right.len();
        let mut carry = 0u8;
        let mut digits = Vec::with_capacity(left.len().max(right.len()) + 1);
        while index_left > 0 || index_right > 0 || carry != 0 {
            let left_digit = if index_left > 0 {
                index_left -= 1;
                left[index_left] - b'0'
            } else {
                0
            };
            let right_digit = if index_right > 0 {
                index_right -= 1;
                right[index_right] - b'0'
            } else {
                0
            };
            let sum = left_digit + right_digit + carry;
            digits.push(b'0' + (sum % 10));
            carry = sum / 10;
        }
        digits.reverse();
        Self(String::from_utf8(digits).expect("decimal addition emits ASCII"))
    }

    fn sub(&self, other: &Self) -> Self {
        debug_assert!(self >= other);
        let left = self.0.as_bytes();
        let right = other.0.as_bytes();
        let mut index_left = left.len();
        let mut index_right = right.len();
        let mut borrow = 0i8;
        let mut digits = Vec::with_capacity(left.len());
        while index_left > 0 {
            index_left -= 1;
            let mut difference = left[index_left] as i8 - b'0' as i8 - borrow;
            let right_digit = if index_right > 0 {
                index_right -= 1;
                right[index_right] as i8 - b'0' as i8
            } else {
                0
            };
            difference -= right_digit;
            if difference < 0 {
                difference += 10;
                borrow = 1;
            } else {
                borrow = 0;
            }
            digits.push(b'0' + difference as u8);
        }
        while digits.len() > 1 && digits.last() == Some(&b'0') {
            digits.pop();
        }
        digits.reverse();
        Self(String::from_utf8(digits).expect("decimal subtraction emits ASCII"))
    }
}

impl Ord for DecimalInt {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}

impl PartialOrd for DecimalInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn decimal_sum(parts: &[DecimalInt]) -> DecimalInt {
    parts
        .iter()
        .fold(DecimalInt("0".to_owned()), |sum, part| sum.add(part))
}

fn decimal_difference_as_f64(minuend: &DecimalInt, subtrahend: &DecimalInt) -> f64 {
    let (negative, magnitude) = match minuend.cmp(subtrahend) {
        Ordering::Less => (true, subtrahend.sub(minuend)),
        _ => (false, minuend.sub(subtrahend)),
    };
    let text = if negative {
        format!("-{}", magnitude.0)
    } else {
        magnitude.0
    };
    match text.parse::<f64>() {
        Ok(value) => value,
        Err(_) if negative => f64::NEG_INFINITY,
        Err(_) => f64::INFINITY,
    }
}

fn parse_numbers(value: &str) -> Vec<Vec<DecimalInt>> {
    let chars: Vec<char> = clean_text_raw(value).chars().collect();
    let mut result = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if generated::decimal_digit(chars[index] as u32).is_none()
            || (index > 0 && generated::decimal_digit(chars[index - 1] as u32).is_some())
        {
            index += 1;
            continue;
        }
        let mut parts = Vec::new();
        loop {
            let start = index;
            while index < chars.len() && generated::decimal_digit(chars[index] as u32).is_some() {
                index += 1;
            }
            if let Some(value) = DecimalInt::from_decimal_chars(chars[start..index].iter().copied())
            {
                parts.push(value);
            }
            if index + 1 < chars.len()
                && matches!(chars[index], '.' | '_' | '-')
                && generated::decimal_digit(chars[index + 1] as u32).is_some()
            {
                index += 1;
                continue;
            }
            break;
        }
        if !parts.is_empty() {
            result.push(parts);
        }
    }
    result
}

fn parse_version(value: &str) -> Option<Vec<DecimalInt>> {
    parse_version_parts(value, true)
}

fn parse_version_parts(value: &str, trim_trailing_zeros: bool) -> Option<Vec<DecimalInt>> {
    let chars: Vec<char> = value.trim().chars().collect();
    for digit_start in 0..chars.len() {
        if generated::decimal_digit(chars[digit_start] as u32).is_none() {
            continue;
        }
        if digit_start > 0
            && chars[digit_start - 1].is_ascii_alphabetic()
            && !(matches!(chars[digit_start - 1], 'v' | 'V')
                && (digit_start < 2 || !chars[digit_start - 2].is_ascii_alphabetic()))
        {
            continue;
        }
        let mut cursor = digit_start;
        let mut ends = Vec::new();
        while cursor < chars.len() && generated::decimal_digit(chars[cursor] as u32).is_some() {
            cursor += 1;
            ends.push(cursor);
        }
        loop {
            if cursor + 1 < chars.len()
                && matches!(chars[cursor], '.' | '_' | '-')
                && generated::decimal_digit(chars[cursor + 1] as u32).is_some()
            {
                cursor += 1;
                while cursor < chars.len()
                    && generated::decimal_digit(chars[cursor] as u32).is_some()
                {
                    cursor += 1;
                    ends.push(cursor);
                }
            } else {
                break;
            }
        }
        for end in ends.into_iter().rev() {
            if end < chars.len() && chars[end].is_ascii_alphabetic() {
                continue;
            }
            let Some(mut parts): Option<Vec<DecimalInt>> = chars[digit_start..end]
                .iter()
                .collect::<String>()
                .split(['.', '_', '-'])
                .map(|part| DecimalInt::from_decimal_chars(part.chars()))
                .collect()
            else {
                continue;
            };
            if parts.is_empty() {
                continue;
            }
            if trim_trailing_zeros {
                while parts.len() > 1 && parts.last().is_some_and(DecimalInt::is_zero) {
                    parts.pop();
                }
            }
            return Some(parts);
        }
    }
    None
}

pub(super) fn has_valid_version(value: &str) -> bool {
    parse_version(value).is_some()
}

pub(super) fn version_progresses(source: &str, candidate: &str) -> bool {
    let Some(source_parts) = parse_version_parts(source, false) else {
        return false;
    };
    let Some(candidate_parts) = parse_version_parts(candidate, false) else {
        return false;
    };
    for (source_part, candidate_part) in source_parts.iter().zip(candidate_parts.iter()) {
        match candidate_part.cmp(source_part) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
    }
    candidate_parts.len() > source_parts.len()
}

fn ascii_case_equal(left: char, right: char) -> bool {
    left == right || left.eq_ignore_ascii_case(&right)
}

fn ascii_alphanumeric(character: char) -> bool {
    character.is_ascii_alphanumeric()
}

fn filename_marker_length(chars: &[char], start: usize) -> Option<usize> {
    if start > 0 && ascii_alphanumeric(chars[start - 1]) {
        return None;
    }
    if chars.len() >= start + 3
        && ascii_case_equal(chars[start], 'v')
        && ascii_case_equal(chars[start + 1], 'e')
        && ascii_case_equal(chars[start + 2], 'r')
    {
        Some(3)
    } else if start < chars.len() && ascii_case_equal(chars[start], 'v') {
        Some(1)
    } else {
        None
    }
}

fn parse_filename_version(value: &str) -> Option<Vec<DecimalInt>> {
    let chars: Vec<char> = value.chars().collect();
    let mut matches = Vec::new();
    for start in 0..chars.len() {
        let Some(marker_length) = filename_marker_length(&chars, start) else {
            continue;
        };
        let mut digit_start = start + marker_length;
        if digit_start < chars.len() && matches!(chars[digit_start], '.' | '_' | '-') {
            digit_start += 1;
        }
        if digit_start >= chars.len()
            || generated::decimal_digit(chars[digit_start] as u32).is_none()
        {
            continue;
        }
        let mut cursor = digit_start;
        let mut ends = Vec::new();
        while cursor < chars.len() && generated::decimal_digit(chars[cursor] as u32).is_some() {
            cursor += 1;
            ends.push(cursor);
        }
        loop {
            if cursor + 1 < chars.len()
                && matches!(chars[cursor], '.' | '_' | '-')
                && generated::decimal_digit(chars[cursor + 1] as u32).is_some()
            {
                cursor += 1;
                while cursor < chars.len()
                    && generated::decimal_digit(chars[cursor] as u32).is_some()
                {
                    cursor += 1;
                    ends.push(cursor);
                }
            } else {
                break;
            }
        }
        for end in ends.into_iter().rev() {
            if end < chars.len() && ascii_alphanumeric(chars[end]) {
                continue;
            }
            let Some(parts): Option<Vec<DecimalInt>> = chars[digit_start..end]
                .iter()
                .collect::<String>()
                .split(['.', '_', '-'])
                .map(|part| DecimalInt::from_decimal_chars(part.chars()))
                .collect()
            else {
                continue;
            };
            matches.push((start, end, parts));
            break;
        }
    }
    if matches.len() == 1 {
        Some(matches.remove(0).2)
    } else {
        None
    }
}

pub(super) fn filename_version_progresses(source: &str, candidate: &str) -> bool {
    let Some(source_parts) = parse_filename_version(source) else {
        return false;
    };
    let Some(candidate_parts) = parse_filename_version(candidate) else {
        return false;
    };
    for (source_part, candidate_part) in source_parts.iter().zip(candidate_parts.iter()) {
        match candidate_part.cmp(source_part) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
    }
    candidate_parts.len() > source_parts.len()
}

fn version_features(
    source: &Value,
    candidate: &Value,
    catalog: &[Value],
) -> HashMap<&'static str, f64> {
    let source_raw = version(source);
    let candidate_raw = version(candidate);
    let source_v = parse_version(&source_raw);
    let candidate_v = parse_version(&candidate_raw);
    let (left, right) = match (&source_v, &candidate_v) {
        (Some(a), Some(b)) => {
            let width = a.len().max(b.len());
            let mut aa = a.clone();
            let mut bb = b.clone();
            aa.resize(width, DecimalInt("0".to_owned()));
            bb.resize(width, DecimalInt("0".to_owned()));
            (Some(aa), Some(bb))
        }
        _ => (None, None),
    };
    let catalog_versions: Vec<Vec<DecimalInt>> = catalog
        .iter()
        .filter_map(|item| parse_version(&version(item)))
        .collect();
    let source_count = source_v
        .as_ref()
        .map(|v| catalog_versions.iter().filter(|item| *item == v).count())
        .unwrap_or(0);
    let candidate_count = candidate_v
        .as_ref()
        .map(|v| catalog_versions.iter().filter(|item| *item == v).count())
        .unwrap_or(0);
    let mut output = HashMap::new();
    output.insert("version_source_present", f64::from(!source_raw.is_empty()));
    output.insert(
        "version_candidate_present",
        f64::from(!candidate_raw.is_empty()),
    );
    output.insert("version_source_valid", f64::from(source_v.is_some()));
    output.insert("version_candidate_valid", f64::from(candidate_v.is_some()));
    output.insert("version_equal", f64::from(left.is_some() && left == right));
    output.insert(
        "version_candidate_newer",
        f64::from(
            left.as_ref()
                .zip(right.as_ref())
                .is_some_and(|(a, b)| b > a),
        ),
    );
    output.insert(
        "version_candidate_older",
        f64::from(
            left.as_ref()
                .zip(right.as_ref())
                .is_some_and(|(a, b)| b < a),
        ),
    );
    output.insert(
        "version_component_delta",
        left.as_ref()
            .zip(right.as_ref())
            .map(|(a, b)| decimal_difference_as_f64(&decimal_sum(b), &decimal_sum(a)))
            .unwrap_or(f64::NAN),
    );
    output.insert("version_source_catalog_repeat_count", source_count as f64);
    output.insert(
        "version_candidate_catalog_repeat_count",
        candidate_count as f64,
    );
    output
}

fn log_graph(
    logs: &[Value],
    logs_known: bool,
    source_id: &str,
    candidate_id: &str,
    catalog: &[Value],
    source: &Value,
    candidate: &Value,
) -> HashMap<&'static str, f64> {
    let mut source_logs = Vec::new();
    let mut candidate_logs = Vec::new();
    let mut both_logs = Vec::new();
    let mut source_only = 0usize;
    let mut candidate_only = 0usize;
    let mut single_source = 0usize;
    let mut single_candidate = 0usize;
    let mut multi_source = 0usize;
    let mut multi_candidate = 0usize;
    let mut attachment_total = 0usize;
    let mut attached_ids = HashSet::new();
    let active_catalog_ids: HashSet<String> = catalog
        .iter()
        .filter(|item| archive(item) == 0.0 && !item_id(item).is_empty())
        .map(|item| item_id(item))
        .collect();
    let known_times: Vec<f64> = {
        let mut values: Vec<f64> = logs
            .iter()
            .map(timestamp)
            .filter(|value| value.is_finite())
            .collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        values.dedup_by(|a, b| a == b);
        values
    };
    let rank = |value: f64| {
        known_times
            .binary_search_by(|probe| probe.partial_cmp(&value).unwrap_or(Ordering::Equal))
            .ok()
            .map(|v| v as f64)
    };
    let source_v = parse_version(&version(source));
    let candidate_v = parse_version(&version(candidate));
    let mut source_version_matches = 0usize;
    let mut candidate_version_matches = 0usize;
    let mut log_versions = Vec::new();
    for (index, log) in logs.iter().enumerate() {
        let attached = attachments(log);
        attached_ids.extend(attached.iter().cloned());
        attachment_total += attached.len();
        let has_source = attached.contains(source_id);
        let has_candidate = attached.contains(candidate_id);
        if has_source {
            source_logs.push(index);
        }
        if has_candidate {
            candidate_logs.push(index);
        }
        if has_source && has_candidate {
            both_logs.push(index);
        }
        if has_source && !has_candidate {
            source_only += 1;
        }
        if has_candidate && !has_source {
            candidate_only += 1;
        }
        if has_source && attached.len() == 1 {
            single_source += 1;
        }
        if has_candidate && attached.len() == 1 {
            single_candidate += 1;
        }
        if has_source && attached.len() > 1 {
            multi_source += 1;
        }
        if has_candidate && attached.len() > 1 {
            multi_candidate += 1;
        }
        let log_version = map(log)
            .and_then(|object| first(object, &VERSION_KEYS, None))
            .map(value_text)
            .and_then(|v| parse_version(&v));
        if let Some(parsed) = log_version {
            if source_v.as_ref() == Some(&parsed) {
                source_version_matches += 1;
            }
            if candidate_v.as_ref() == Some(&parsed) {
                candidate_version_matches += 1;
            }
            log_versions.push(parsed);
        }
    }
    let min_rank = |indices: &[usize]| -> f64 {
        indices
            .iter()
            .filter_map(|index| rank(timestamp(&logs[*index])))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
            .unwrap_or(f64::NAN)
    };
    let max_rank = |indices: &[usize]| -> f64 {
        indices
            .iter()
            .filter_map(|index| rank(timestamp(&logs[*index])))
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
            .unwrap_or(f64::NAN)
    };
    let source_times: Vec<f64> = source_logs
        .iter()
        .map(|index| timestamp(&logs[*index]))
        .filter(|value| value.is_finite())
        .collect();
    let candidate_times: Vec<f64> = candidate_logs
        .iter()
        .map(|index| timestamp(&logs[*index]))
        .filter(|value| value.is_finite())
        .collect();
    let relative_order = if !source_times.is_empty() && !candidate_times.is_empty() {
        f64::from(
            candidate_times
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min)
                > source_times.iter().copied().fold(f64::INFINITY, f64::min),
        )
    } else {
        f64::NAN
    };
    let source_ts = timestamp(source);
    let candidate_ts = timestamp(candidate);
    let candidate_date_after = if source_ts.is_finite() && candidate_ts.is_finite() {
        f64::from(candidate_ts > source_ts)
    } else {
        f64::NAN
    };
    let known = |value: f64| if logs_known { value } else { f64::NAN };
    let mut output = HashMap::new();
    output.insert("logs_known", f64::from(logs_known));
    output.insert("update_log_count", known(logs.len() as f64));
    output.insert("update_attachment_total", known(attachment_total as f64));
    output.insert(
        "source_update_attachment_count",
        known(source_logs.len() as f64),
    );
    output.insert(
        "candidate_update_attachment_count",
        known(candidate_logs.len() as f64),
    );
    output.insert(
        "source_candidate_same_update_count",
        known(both_logs.len() as f64),
    );
    output.insert("source_only_update_count", known(source_only as f64));
    output.insert("candidate_only_update_count", known(candidate_only as f64));
    output.insert(
        "source_single_file_binding_count",
        known(single_source as f64),
    );
    output.insert(
        "candidate_single_file_binding_count",
        known(single_candidate as f64),
    );
    output.insert(
        "source_multi_file_binding_count",
        known(multi_source as f64),
    );
    output.insert(
        "candidate_multi_file_binding_count",
        known(multi_candidate as f64),
    );
    output.insert("source_linked_earliest_rank", known(min_rank(&source_logs)));
    output.insert("source_linked_latest_rank", known(max_rank(&source_logs)));
    output.insert(
        "candidate_linked_earliest_rank",
        known(min_rank(&candidate_logs)),
    );
    output.insert(
        "candidate_linked_latest_rank",
        known(max_rank(&candidate_logs)),
    );
    output.insert("linked_release_order", known(relative_order));
    output.insert(
        "linked_attachment_set_mean_size",
        known(if logs.is_empty() {
            if logs_known { 0.0 } else { f64::NAN }
        } else {
            attachment_total as f64 / logs.len() as f64
        }),
    );
    output.insert(
        "source_candidate_attachment_overlap",
        known(
            both_logs.len() as f64
                / (1.max(
                    source_logs
                        .iter()
                        .chain(candidate_logs.iter())
                        .copied()
                        .collect::<HashSet<_>>()
                        .len(),
                ) as f64),
        ),
    );
    output.insert(
        "active_catalog_log_coverage",
        known(if !active_catalog_ids.is_empty() {
            attached_ids.intersection(&active_catalog_ids).count() as f64
                / active_catalog_ids.len() as f64
        } else {
            0.0
        }),
    );
    output.insert(
        "source_log_version_match_count",
        known(source_version_matches as f64),
    );
    output.insert(
        "candidate_log_version_match_count",
        known(candidate_version_matches as f64),
    );
    output.insert(
        "distinct_log_version_count",
        known(log_versions.iter().collect::<HashSet<_>>().len() as f64),
    );
    output.insert("candidate_date_after_source", candidate_date_after);
    output
}

fn quantile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut values = values.to_vec();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let position = (values.len() - 1) as f64 * fraction;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        values[lower]
    } else {
        values[lower] + (values[upper] - values[lower]) * (position - lower as f64)
    }
}

fn catalog_context(
    source: &Value,
    candidate: &Value,
    catalog: &[Value],
) -> HashMap<&'static str, f64> {
    let source_text = format!("{} {}", name(source), desc(source));
    let candidate_text = format!("{} {}", name(candidate), desc(candidate));
    let docs: Vec<HashSet<String>> = catalog
        .iter()
        .map(|item| token_set(&format!("{} {}", name(item), desc(item))))
        .collect();
    let mut df: HashMap<String, usize> = HashMap::new();
    for doc in &docs {
        for token in doc {
            *df.entry(token.clone()).or_default() += 1;
        }
    }
    let doc_count = docs.len().max(1) as f64;
    let source_tokens = token_set(&source_text);
    let candidate_tokens = token_set(&candidate_text);
    let mut shared: Vec<String> = source_tokens
        .intersection(&candidate_tokens)
        .cloned()
        .collect();
    let mut source_only: Vec<String> = source_tokens
        .difference(&candidate_tokens)
        .cloned()
        .collect();
    let mut candidate_only: Vec<String> = candidate_tokens
        .difference(&source_tokens)
        .cloned()
        .collect();
    // Python set iteration is process-seeded and therefore changes the low bits
    // of floating-point sums. The deterministic reference sorts these tokens;
    // keep the native reduction order identical for stable model inputs.
    shared.sort();
    source_only.sort();
    candidate_only.sort();
    let idf = |token: &String| {
        ((1.0 + doc_count) / (1.0 + *df.get(token).unwrap_or(&0) as f64)).ln() + 1.0
    };
    let rarity =
        |values: &[String]| values.iter().map(idf).sum::<f64>() / values.len().max(1) as f64;
    let source_id = item_id(source);
    let candidate_id = item_id(candidate);
    let siblings: Vec<&Value> = catalog
        .iter()
        .filter(|item| item_id(item) != source_id && item_id(item) != candidate_id)
        .collect();
    // Python returns mean,max,median,q75. Keep those positions explicit.
    let distribution = |field: bool, target: &Value| -> (f64, f64, f64, f64) {
        let target_value = if field { name(target) } else { desc(target) };
        let values: Vec<f64> = siblings
            .iter()
            .map(|item| {
                let item_value = if field { name(item) } else { desc(item) };
                similarity(&item_value, &target_value)
            })
            .collect();
        if values.is_empty() {
            return (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (mean, max, quantile(&values, 0.5), quantile(&values, 0.75))
    };
    let source_name_dist = distribution(true, source);
    let candidate_name_dist = distribution(true, candidate);
    let source_desc_dist = distribution(false, source);
    let candidate_desc_dist = distribution(false, candidate);
    let sibling_overlap: Vec<f64> = siblings
        .iter()
        .map(|item| jaccard(&source_text, &format!("{} {}", name(item), desc(item))))
        .collect();
    let candidate_overlap: Vec<f64> = siblings
        .iter()
        .map(|item| jaccard(&candidate_text, &format!("{} {}", name(item), desc(item))))
        .collect();
    let mut known_times: Vec<f64> = catalog
        .iter()
        .map(timestamp)
        .filter(|v| v.is_finite())
        .collect();
    known_times.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));
    known_times.dedup_by(|a, b| a == b);
    let rank = |item: &Value| {
        let ts = timestamp(item);
        if !ts.is_finite() {
            f64::NAN
        } else {
            known_times
                .iter()
                .position(|v| *v == ts)
                .map(|v| v as f64)
                .unwrap_or_else(|| known_times.iter().filter(|v| **v > ts).count() as f64)
        }
    };
    let source_ts = timestamp(source);
    let candidate_ts = timestamp(candidate);
    let newer = |item_id_value: &str, own_ts: f64| {
        catalog
            .iter()
            .filter(|item| {
                timestamp(item).is_finite()
                    && own_ts.is_finite()
                    && timestamp(item) > own_ts
                    && item_id(item) != item_id_value
            })
            .count() as f64
    };
    let source_nums = parse_numbers(&name(source));
    let candidate_nums = parse_numbers(&name(candidate));
    let same_number = f64::from(
        !source_nums.is_empty()
            && !candidate_nums.is_empty()
            && source_nums.iter().any(|v| candidate_nums.contains(v)),
    );
    let numeric_relation = if let (Some(a), Some(b)) = (source_nums.last(), candidate_nums.last()) {
        if a.len() == b.len() {
            f64::from(b > a)
        } else {
            f64::NAN
        }
    } else {
        f64::NAN
    };
    let mut output = HashMap::new();
    output.insert("catalog_name_source_mean", source_name_dist.0);
    output.insert("catalog_name_source_max", source_name_dist.1);
    output.insert("catalog_name_source_median", source_name_dist.2);
    output.insert("catalog_name_source_q75", source_name_dist.3);
    output.insert("catalog_name_candidate_mean", candidate_name_dist.0);
    output.insert("catalog_name_candidate_max", candidate_name_dist.1);
    output.insert("catalog_name_candidate_median", candidate_name_dist.2);
    output.insert("catalog_name_candidate_q75", candidate_name_dist.3);
    output.insert("catalog_desc_source_mean", source_desc_dist.0);
    output.insert("catalog_desc_source_max", source_desc_dist.1);
    output.insert("catalog_desc_source_median", source_desc_dist.2);
    output.insert("catalog_desc_source_q75", source_desc_dist.3);
    output.insert("catalog_desc_candidate_mean", candidate_desc_dist.0);
    output.insert("catalog_desc_candidate_max", candidate_desc_dist.1);
    output.insert("catalog_desc_candidate_median", candidate_desc_dist.2);
    output.insert("catalog_desc_candidate_q75", candidate_desc_dist.3);
    output.insert(
        "catalog_sibling_overlap_source_mean",
        if sibling_overlap.is_empty() {
            f64::NAN
        } else {
            sibling_overlap.iter().sum::<f64>() / sibling_overlap.len() as f64
        },
    );
    output.insert(
        "catalog_sibling_overlap_candidate_mean",
        if candidate_overlap.is_empty() {
            f64::NAN
        } else {
            candidate_overlap.iter().sum::<f64>() / candidate_overlap.len() as f64
        },
    );
    output.insert("shared_token_idf_sum", shared.iter().map(idf).sum());
    output.insert(
        "shared_token_idf_mean",
        shared.iter().map(idf).sum::<f64>() / shared.len().max(1) as f64,
    );
    output.insert("source_only_token_rarity", rarity(&source_only));
    output.insert("candidate_only_token_rarity", rarity(&candidate_only));
    output.insert("source_token_count", source_tokens.len() as f64);
    output.insert("candidate_token_count", candidate_tokens.len() as f64);
    output.insert("shared_token_count", shared.len() as f64);
    output.insert("source_date_rank", rank(source));
    output.insert("candidate_date_rank", rank(candidate));
    output.insert(
        "source_newer_catalog_peer_count",
        newer(&source_id, source_ts),
    );
    output.insert(
        "candidate_newer_catalog_peer_count",
        newer(&candidate_id, candidate_ts),
    );
    output.insert(
        "source_filename_number_missing",
        f64::from(source_nums.is_empty()),
    );
    output.insert(
        "candidate_filename_number_missing",
        f64::from(candidate_nums.is_empty()),
    );
    output.insert("filename_number_shared", same_number);
    output.insert("filename_number_candidate_newer", numeric_relation);
    output.insert("filename_number_source_count", source_nums.len() as f64);
    output.insert(
        "filename_number_candidate_count",
        candidate_nums.len() as f64,
    );
    output
}

fn has_status_key(item: &Value) -> bool {
    map(item).is_some_and(|object| {
        ARCHIVE_KEYS
            .iter()
            .chain(TRASH_KEYS.iter())
            .chain(ACTIVE_KEYS.iter())
            .any(|key| object.contains_key(*key))
    })
}

/// Checked boundary for production inference. `NaN` remains an ordinary
/// missing feature, while an infinite feature is an unsupported input and is
/// rejected before it can be silently mapped to model imputation.
pub fn extract_pair_checked(row: &Value) -> Result<Vec<Option<f64>>, ()> {
    let values = extract_pair_values(row);
    if values.iter().any(|value| value.is_infinite()) {
        return Err(());
    }
    Ok(values
        .into_iter()
        .map(|value| value.is_finite().then_some(value))
        .collect())
}

fn extract_pair_values(row: &Value) -> Vec<f64> {
    let row_object = map(row).cloned().unwrap_or_default();
    let profile = row_object
        .get("profile")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let source_value = row_object
        .get("source")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let source = if map(&source_value).is_some_and(|v| !v.is_empty()) {
        source_value
    } else if let Some(snapshot) = row_object.get("source_snapshot") {
        if map(snapshot).is_some_and(|v| !v.is_empty()) {
            map(snapshot)
                .cloned()
                .map(Value::Object)
                .unwrap_or_else(|| catalog_file(&profile, &source_value))
        } else {
            catalog_file(&profile, &source_value)
        }
    } else {
        catalog_file(&profile, &source_value)
    };
    let candidate_value = row_object
        .get("candidate")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let candidate = if map(&candidate_value).is_some_and(|v| !v.is_empty()) {
        candidate_value
    } else {
        catalog_file(&profile, &candidate_value)
    };
    let catalog = unwrap_profile(&profile);
    let (log_values, logs_known) = logs(row_object.get("logs"));
    let source_id = item_id(&source);
    let candidate_id = item_id(&candidate);
    let source_labels = labels(&source);
    let candidate_labels = labels(&candidate);
    let mut source_archive = archive(&source);
    let mut candidate_archive = archive(&candidate);
    let catalog_by_id: HashMap<String, Value> = catalog
        .iter()
        .filter_map(|item| {
            let id = item_id(item);
            if id.is_empty() {
                None
            } else {
                Some((id, item.clone()))
            }
        })
        .collect();
    if !has_status_key(&source) {
        if let Some(item) = catalog_by_id.get(&source_id) {
            source_archive = archive(item);
        }
    }
    if !has_status_key(&candidate) {
        if let Some(item) = catalog_by_id.get(&candidate_id) {
            candidate_archive = archive(item);
        }
    }
    let active = catalog.iter().filter(|item| archive(item) == 0.0).count() as f64;
    let archived = catalog.iter().filter(|item| archive(item) == 1.0).count() as f64;
    let source_ts = timestamp(&source);
    let candidate_ts = timestamp(&candidate);
    let source_newer = catalog
        .iter()
        .filter(|item| {
            timestamp(item).is_finite()
                && source_ts.is_finite()
                && timestamp(item) > source_ts
                && item_id(item) != source_id
        })
        .count() as f64;
    let candidate_newer = catalog
        .iter()
        .filter(|item| {
            timestamp(item).is_finite()
                && candidate_ts.is_finite()
                && timestamp(item) > candidate_ts
                && item_id(item) != candidate_id
        })
        .count() as f64;
    let source_size = size(&source);
    let candidate_size = size(&candidate);
    let size_ratio = if source_size.is_finite()
        && candidate_size.is_finite()
        && source_size > 0.0
        && candidate_size > 0.0
    {
        (candidate_size / source_size).ln()
    } else {
        f64::NAN
    };
    let source_name = name(&source);
    let candidate_name = name(&candidate);
    let mut numeric: HashMap<&str, f64> = HashMap::new();
    numeric.insert(
        "filename_char_similarity",
        similarity(&source_name, &candidate_name),
    );
    numeric.insert(
        "filename_token_jaccard",
        jaccard(&source_name, &candidate_name),
    );
    numeric.insert(
        "filename_source_coverage",
        coverage(&source_name, &candidate_name),
    );
    numeric.insert(
        "filename_candidate_coverage",
        coverage(&candidate_name, &source_name),
    );
    let source_desc = desc(&source);
    let candidate_desc = desc(&candidate);
    numeric.insert(
        "description_char_similarity",
        similarity(&source_desc, &candidate_desc),
    );
    numeric.insert(
        "description_token_jaccard",
        jaccard(&source_desc, &candidate_desc),
    );
    numeric.insert(
        "description_source_coverage",
        coverage(&source_desc, &candidate_desc),
    );
    numeric.insert(
        "description_candidate_coverage",
        coverage(&candidate_desc, &source_desc),
    );
    let source_ext = Path::new(&source_name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    let candidate_ext = Path::new(&candidate_name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    numeric.insert(
        "extension_equal",
        f64::from(!source_ext.is_empty() && source_ext == candidate_ext),
    );
    numeric.insert("source_label_present", f64::from(!source_labels.is_empty()));
    numeric.insert(
        "candidate_label_present",
        f64::from(!candidate_labels.is_empty()),
    );
    numeric.insert(
        "label_set_equal",
        f64::from(
            !source_labels.is_empty()
                && !candidate_labels.is_empty()
                && source_labels.iter().collect::<HashSet<_>>()
                    == candidate_labels.iter().collect::<HashSet<_>>(),
        ),
    );
    numeric.insert(
        "label_jaccard",
        jaccard(&source_labels.join(" "), &candidate_labels.join(" ")),
    );
    numeric.insert("log_file_size_ratio", size_ratio);
    numeric.insert("relative_file_age_days", date_delta(&source, &candidate));
    numeric.insert("source_archived", source_archive);
    numeric.insert("candidate_archived", candidate_archive);
    numeric.insert(
        "source_present_catalog",
        f64::from(catalog_by_id.contains_key(&source_id)),
    );
    numeric.insert(
        "candidate_present_catalog",
        f64::from(catalog_by_id.contains_key(&candidate_id)),
    );
    numeric.insert("catalog_active_count", active);
    numeric.insert("catalog_archived_count", archived);
    numeric.insert("source_newer_peer_count", source_newer);
    numeric.insert("candidate_newer_peer_count", candidate_newer);
    for (key, value) in version_features(&source, &candidate, &catalog) {
        numeric.insert(key, value);
    }
    for (key, value) in log_graph(
        &log_values,
        logs_known,
        &source_id,
        &candidate_id,
        &catalog,
        &source,
        &candidate,
    ) {
        numeric.insert(key, value);
    }
    for (key, value) in catalog_context(&source, &candidate, &catalog) {
        numeric.insert(key, value);
    }
    NUMERIC_FEATURES
        .iter()
        .map(|name| *numeric.get(name).unwrap_or(&f64::NAN))
        .collect()
}

fn tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"<[^>]*>").unwrap())
}

fn strip_urls(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < chars.len() {
        let prefix_length = if starts_ascii_casefold(&chars, index, "http://")
            || starts_ascii_casefold(&chars, index, "https://")
        {
            Some(if starts_ascii_casefold(&chars, index, "https://") {
                8
            } else {
                7
            })
        } else if starts_ascii_casefold(&chars, index, "www.") {
            Some(4)
        } else {
            None
        };
        if let Some(prefix_length) = prefix_length {
            index += prefix_length;
            while index < chars.len()
                && !is_python_space(chars[index])
                && !matches!(chars[index], '<' | '>' | '(' | ')')
            {
                index += 1;
            }
            output.push(' ');
        } else {
            output.push(chars[index]);
            index += 1;
        }
    }
    output
}

fn starts_ascii_casefold(chars: &[char], index: usize, pattern: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    index + pattern.len() <= chars.len()
        && pattern.iter().enumerate().all(|(offset, expected)| {
            chars[index + offset].to_ascii_lowercase() == expected.to_ascii_lowercase()
        })
}

fn html_unescape(value: &str) -> String {
    generated::html_unescape(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_frozen_feature_order() {
        assert_eq!(NUMERIC_FEATURES.len(), 92);
        assert_eq!(NUMERIC_FEATURES[0], "filename_char_similarity");
        assert_eq!(NUMERIC_FEATURES[91], "filename_number_candidate_count");
    }

    #[test]
    fn sequence_matcher_examples_match_difflib_ratio() {
        assert!((sequence_ratio("abc", "abc") - 1.0).abs() < 1e-12);
        assert!((sequence_ratio("ab", "ac") - 0.5).abs() < 1e-12);
    }

    #[test]
    fn generated_unicode_and_html_are_available() {
        assert_eq!(generated::casefold_str("Straße"), "strasse");
        assert_eq!(
            clean_text_raw("A&nbsp;&lt;b&gt; https://example.test/x  B"),
            "A B"
        );
    }

    #[test]
    fn decimal_components_preserve_arbitrary_unicode_digits() {
        let value = DecimalInt::from_decimal_chars("٠٠١٢٣٤٥٦٧٨٩".chars()).unwrap();
        let other =
            DecimalInt::from_decimal_chars("9999999999999999999999999999999999999999".chars())
                .unwrap();
        assert_eq!(value.0, "123456789");
        assert!(other > value);
        assert_eq!(
            parse_numbers("foo1_1234567890123456789012345678901234567890_2.zip")[0].len(),
            3
        );
    }

    #[test]
    fn decimal_version_sum_does_not_overflow_machine_integer() {
        let source = parse_version("9223372036854775800.7").unwrap();
        let candidate = parse_version("9223372036854775800.8").unwrap();
        assert_eq!(
            decimal_difference_as_f64(&decimal_sum(&candidate), &decimal_sum(&source)),
            1.0
        );
    }
}
