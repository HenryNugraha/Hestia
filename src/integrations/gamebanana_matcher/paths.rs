//! Archive path features from GameBanana's RawFileList answers.
//!
//! A faithful port of the study's `paths.py`.  Each candidate is compared
//! with the installed source; the group's other candidates give the contrast
//! ("is this candidate the best name match among the files on offer?").  An
//! unknown list gives NaN for every feature, like the trainer.

use std::collections::BTreeSet;

pub const PATH_FEATURE_COUNT: usize = 13;
pub const PATH_FEATURE_NAMES: [&str; PATH_FEATURE_COUNT] = [
    "paths_known",
    "path_jaccard",
    "path_source_kept",
    "path_candidate_from_source",
    "name_jaccard",
    "name_source_kept",
    "ini_jaccard",
    "folder_token_jaccard",
    "log_path_count_ratio",
    "candidate_nested_archive",
    "name_jaccard_rank",
    "name_jaccard_margin",
    "name_kept_margin",
];

const SKIP_NAMES: [&str; 3] = ["desktop.ini", "thumbs.db", ".ds_store"];
const NESTED: [&str; 3] = [".zip", ".rar", ".7z"];

pub type PathFeatures = [f64; PATH_FEATURE_COUNT];

/// Python's `str.isspace`, which also counts the four ASCII separators
/// that Rust's `char::is_whitespace` leaves out.
fn python_space(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

/// One path per line, split like Python's `str.splitlines`, trimmed, and
/// without blank lines.
pub fn split_raw_file_list(body: &str) -> Vec<String> {
    let line_break = |character: char| {
        matches!(
            character,
            '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
    };
    body.split(line_break)
        .map(|line| line.trim_matches(python_space))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn strip_disabled(name: &str) -> &str {
    name.strip_prefix("disabled").unwrap_or(name)
}

fn ascii_tokens(folder: &str, tokens: &mut BTreeSet<String>) {
    tokens.extend(
        folder
            .split(|character: char| !(character.is_ascii_lowercase() || character.is_ascii_digit()))
            .filter(|token| !token.is_empty())
            .map(str::to_owned),
    );
}

/// One archive's normalized contents.
#[derive(Debug, Clone, PartialEq)]
struct Listing {
    nested: bool,
    paths: BTreeSet<String>,
    names: BTreeSet<String>,
    inis: BTreeSet<String>,
    folder_tokens: BTreeSet<String>,
}

impl Listing {
    fn new(raw: &[String]) -> Option<Self> {
        let mut parts: Vec<Vec<String>> = Vec::new();
        for line in raw {
            let path = line
                .trim_matches(python_space)
                .replace('\\', "/")
                .to_lowercase();
            let segments: Vec<String> = path
                .trim_matches('/')
                .split('/')
                .filter(|segment| !segment.is_empty())
                .map(str::to_owned)
                .collect();
            let Some(last) = segments.last() else {
                continue;
            };
            if segments.iter().any(|segment| segment == "__macosx")
                || SKIP_NAMES.contains(&last.as_str())
            {
                continue;
            }
            parts.push(segments);
        }
        let nested = parts.iter().any(|segments| {
            let last = segments.last().expect("segments are never empty");
            NESTED.iter().any(|extension| last.ends_with(extension))
        });
        // Drop shared wrapper folders, the way an install unwraps them.
        let mut wrapper = Vec::new();
        while !parts.is_empty()
            && parts.iter().all(|segments| segments.len() > 1)
            && parts.iter().all(|segments| segments[0] == parts[0][0])
        {
            wrapper.push(parts[0][0].clone());
            for segments in &mut parts {
                segments.remove(0);
            }
        }
        let mut paths = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut folder_tokens = BTreeSet::new();
        for segments in &parts {
            let (last, folders) = segments.split_last().expect("segments are never empty");
            let name = strip_disabled(last);
            let mut path = folders.join("/");
            if !folders.is_empty() {
                path.push('/');
            }
            path.push_str(name);
            paths.insert(path);
            names.insert(name.to_owned());
            for folder in folders {
                ascii_tokens(folder, &mut folder_tokens);
            }
        }
        for folder in &wrapper {
            ascii_tokens(folder, &mut folder_tokens);
        }
        if paths.is_empty() {
            return None;
        }
        let inis = names.iter().filter(|name| name.ends_with(".ini")).cloned().collect();
        Some(Self {
            nested,
            paths,
            names,
            inis,
            folder_tokens,
        })
    }
}

fn jaccard(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    let shared = left.intersection(right).count();
    let union = left.len() + right.len() - shared;
    if union == 0 {
        f64::NAN
    } else {
        shared as f64 / union as f64
    }
}

fn kept(from: &BTreeSet<String>, into: &BTreeSet<String>) -> f64 {
    if from.is_empty() {
        f64::NAN
    } else {
        from.intersection(into).count() as f64 / from.len() as f64
    }
}

#[derive(Debug, Clone, Copy)]
struct PairValues {
    path_jaccard: f64,
    path_source_kept: f64,
    path_candidate_from_source: f64,
    name_jaccard: f64,
    name_source_kept: f64,
    ini_jaccard: f64,
    folder_token_jaccard: f64,
    log_path_count_ratio: f64,
    candidate_nested_archive: f64,
}

fn pair_values(source: &Listing, candidate: &Listing) -> PairValues {
    PairValues {
        path_jaccard: jaccard(&source.paths, &candidate.paths),
        path_source_kept: kept(&source.paths, &candidate.paths),
        path_candidate_from_source: kept(&candidate.paths, &source.paths),
        name_jaccard: jaccard(&source.names, &candidate.names),
        name_source_kept: kept(&source.names, &candidate.names),
        ini_jaccard: if source.inis.is_empty() && candidate.inis.is_empty() {
            f64::NAN
        } else {
            jaccard(&source.inis, &candidate.inis)
        },
        folder_token_jaccard: if source.folder_tokens.is_empty()
            && candidate.folder_tokens.is_empty()
        {
            f64::NAN
        } else {
            jaccard(&source.folder_tokens, &candidate.folder_tokens)
        },
        log_path_count_ratio: (candidate.paths.len() as f64 / source.paths.len() as f64).ln(),
        candidate_nested_archive: if candidate.nested { 1.0 } else { 0.0 },
    }
}

/// Feature rows for one source group, in candidate order.  `None` means the
/// source or that candidate has no usable list.
pub fn group_path_features(
    source: Option<&[String]>,
    candidates: &[Option<&[String]>],
) -> Vec<Option<PathFeatures>> {
    let source = source.and_then(Listing::new);
    let values: Vec<Option<PairValues>> = candidates
        .iter()
        .map(|candidate| {
            let source = source.as_ref()?;
            let candidate = Listing::new((*candidate)?)?;
            Some(pair_values(source, &candidate))
        })
        .collect();
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = (*value)?;
            let others: Vec<PairValues> = values
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .filter_map(|(_, other)| *other)
                .collect();
            let best_name = others
                .iter()
                .map(|other| other.name_jaccard)
                .fold(f64::NAN, f64::max);
            let best_kept = others
                .iter()
                .map(|other| other.name_source_kept)
                .filter(|kept| !kept.is_nan())
                .fold(f64::NAN, f64::max);
            let rank = 1.0
                + others
                    .iter()
                    .filter(|other| other.name_jaccard > value.name_jaccard)
                    .count() as f64;
            let (name_margin, kept_margin) = if others.is_empty() {
                (f64::NAN, f64::NAN)
            } else {
                (
                    value.name_jaccard - best_name,
                    value.name_source_kept - best_kept,
                )
            };
            Some([
                1.0,
                value.path_jaccard,
                value.path_source_kept,
                value.path_candidate_from_source,
                value.name_jaccard,
                value.name_source_kept,
                value.ini_jaccard,
                value.folder_token_jaccard,
                value.log_path_count_ratio,
                value.candidate_nested_archive,
                rank,
                name_margin,
                kept_margin,
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn splits_lines_like_python_and_drops_blanks() {
        assert_eq!(
            split_raw_file_list("a/b.ini\r\n  c.dds \n\n\u{2028}d.buf\u{1c}"),
            list(&["a/b.ini", "c.dds", "d.buf"])
        );
    }

    #[test]
    fn wrappers_junk_and_disabled_prefix_are_normalized() {
        let source = list(&["Wrap/Mod/a.ini", "Wrap/Mod/Tex/b.dds", "__MACOSX/Wrap/._a.ini", "Wrap/Thumbs.db"]);
        let candidate = list(&["Other/Mod/DISABLEDa.ini", "Other/Mod/Tex/b.dds"]);
        let features = group_path_features(Some(&source), &[Some(&candidate)]);
        let row = features[0].expect("both lists known");
        assert_eq!(row[0], 1.0);
        assert_eq!(row[1], 1.0, "same paths once wrappers are dropped");
        assert_eq!(row[4], 1.0);
        assert!(row[11].is_nan(), "a lone candidate has no contrast");
        assert_eq!(row[10], 1.0);
    }

    #[test]
    fn single_file_lists_keep_their_folder() {
        let source = list(&["Folder/a.ini"]);
        let candidate = list(&["a.ini"]);
        let row = group_path_features(Some(&source), &[Some(&candidate)])[0].unwrap();
        // One path with a folder is a wrapper; both become "a.ini".
        assert_eq!(row[1], 1.0);
    }

    #[test]
    fn unknown_lists_give_no_features_and_no_contrast() {
        let source = list(&["m/a.ini", "m/b.ini"]);
        let known = list(&["m/a.ini"]);
        let features = group_path_features(Some(&source), &[Some(&known), None]);
        assert!(features[1].is_none());
        assert!(features[0].unwrap()[11].is_nan());
        assert!(group_path_features(None, &[Some(&known)])[0].is_none());
        assert!(group_path_features(Some(&list(&["Desktop.ini"])), &[Some(&known)])[0].is_none());
    }

    #[test]
    fn contrast_ranks_the_closest_name_match_first() {
        let source = list(&["m/a.ini", "m/b.dds", "m/c.buf"]);
        let close = list(&["m/a.ini", "m/b.dds"]);
        let far = list(&["m/x.ini"]);
        let features = group_path_features(Some(&source), &[Some(&close), Some(&far)]);
        let close = features[0].unwrap();
        let far = features[1].unwrap();
        assert_eq!(close[10], 1.0);
        assert_eq!(far[10], 2.0);
        assert!(close[11] > 0.0 && far[11] < 0.0);
    }

    #[test]
    #[cfg(gamebanana_matcher_path_fixtures)]
    fn real_and_edge_groups_match_the_python_features() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            feature_names: Vec<String>,
            groups: Vec<Group>,
        }
        #[derive(serde::Deserialize)]
        struct Group {
            label: String,
            source: Option<Vec<String>>,
            candidates: Vec<Option<Vec<String>>>,
            expected: Vec<Vec<Option<f64>>>,
        }
        let fixture: Fixture =
            serde_json::from_str(include_str!("tests/fixtures/path-feature-fixtures.json")).unwrap();
        assert_eq!(fixture.feature_names, PATH_FEATURE_NAMES);
        assert!(fixture.groups.len() > 100);
        for group in fixture.groups {
            let candidates: Vec<Option<&[String]>> =
                group.candidates.iter().map(|list| list.as_deref()).collect();
            let actual = group_path_features(group.source.as_deref(), &candidates);
            for (row, expected) in actual.iter().zip(&group.expected) {
                let row = row.unwrap_or([f64::NAN; PATH_FEATURE_COUNT]);
                for (index, (value, expected)) in row.iter().zip(expected).enumerate() {
                    match expected {
                        None => assert!(value.is_nan(), "{} feature {index}", group.label),
                        Some(expected) => assert!(
                            (value - expected).abs() <= 1e-12,
                            "{} feature {index}: {value} != {expected}",
                            group.label
                        ),
                    }
                }
            }
        }
    }
}
