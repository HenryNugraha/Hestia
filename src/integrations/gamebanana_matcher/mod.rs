//! Production-ready, pure-JSON source matcher and candidate chooser.
//!
//! The feature/model path is deliberately label-free: callers provide one raw
//! pair object containing `source`, `candidate`, `profile`, and `logs`.  The
//! model is embedded in this crate and parsed once.  Candidate selection keeps
//! deterministic release proof ahead of the learned score and abstains on
//! every ambiguity.
//!
//! When the metadata model cannot decide, a second model trained with the
//! same inputs plus GameBanana's archive path lists (`paths`) gets one try.

pub mod features;
pub mod model;
pub mod paths;

use model::NativeModel;
use serde_json::Value;
use std::sync::OnceLock;

const STAGE1_MODEL_JSON: &str = include_str!("model_stage1.json");
const STAGE2_MODEL_JSON: &str = include_str!("model_stage2.json");
const SCORE_TIE_EPSILON: f64 = 1e-12;
const VERSION_KEYS: [&str; 3] = ["_sVersion", "version", "_sFileVersion"];
const NUMERIC_KEYS: [&str; 14] = [
    "_nFilesize",
    "_nFileSize",
    "_nSize",
    "file_size",
    "size",
    "size_bytes",
    "_nDownloadSize",
    "_tsDateAdded",
    "_tsDateModified",
    "_tsDateUpdated",
    "date_added",
    "date_modified",
    "date_updated",
    "created_at",
];
const FILENAME_KEYS: [&str; 2] = ["_sFile", "name"];

static STAGE1_MODEL: OnceLock<Result<NativeModel, String>> = OnceLock::new();
static STAGE2_MODEL: OnceLock<Result<NativeModel, String>> = OnceLock::new();

fn stage1_model() -> Option<&'static NativeModel> {
    STAGE1_MODEL
        .get_or_init(|| {
            NativeModel::from_json_str(STAGE1_MODEL_JSON).map_err(|error| error.to_string())
        })
        .as_ref()
        .ok()
}

fn stage2_model() -> Option<&'static NativeModel> {
    STAGE2_MODEL
        .get_or_init(|| {
            NativeModel::from_stage2_json_str(STAGE2_MODEL_JSON).map_err(|error| error.to_string())
        })
        .as_ref()
        .ok()
}

/// The score for one source/candidate pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairScore {
    pub probability: f64,
    /// The frozen generic nonnumeric version-label veto fired for this pair.
    /// The adapter should turn this into `vetoed` candidate evidence.
    pub version_conflict: bool,
}

/// Evidence supplied by the catalog/release adapter for one candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateEvidence {
    pub file_id: u64,
    pub score: Option<f64>,
    /// The archive path model's score; `None` when the lists are unknown.
    pub path_score: Option<f64>,
    pub eligible: bool,
    pub vetoed: bool,
    pub verified_release_chain: bool,
    pub verified_typed_release: bool,
    /// Explicit catalog/version evidence required for model fallback.
    pub model_evidence_floor: bool,
}

/// Score one raw label-free JSON pair.
///
/// `None` means the checked feature extraction found an unsupported infinite
/// value, or the embedded model could not score the row. Ordinary absent
/// fields remain missing values and are median-imputed by the frozen model.
pub fn score_pair(row: &Value) -> Option<PairScore> {
    if contains_unsupported_numeric(row, None) {
        return None;
    }
    let model = stage1_model()?;
    let raw_features = features::extract_pair_checked(row).ok()?;
    let score = model.score_raw(&raw_features).ok()?;
    Some(PairScore {
        probability: score.probability,
        version_conflict: version_conflict_veto(row),
    })
}

/// Score one raw pair with the archive path model.  `path_features` comes
/// from [`paths::group_path_features`] for the pair's whole source group.
pub fn score_pair_with_paths(row: &Value, path_features: &paths::PathFeatures) -> Option<f64> {
    if contains_unsupported_numeric(row, None) {
        return None;
    }
    let model = stage2_model()?;
    let mut raw_features = features::extract_pair_checked(row).ok()?;
    raw_features.extend(
        path_features
            .iter()
            .map(|value| value.is_finite().then_some(*value)),
    );
    Some(model.score_raw(&raw_features).ok()?.probability)
}

/// Return whether the raw pair has the explicit evidence required before a
/// unique above-cutoff model result may select it.
///
/// Proof candidates do not use this floor. An active source needs either a
/// newer numeric API version or one newer explicit `v`/`ver` filename marker;
/// archived and absent sources already carry catalog evidence.
pub fn model_evidence_floor(row: &Value) -> bool {
    let Some(profile) = row.get("profile").and_then(Value::as_object) else {
        return false;
    };
    let Some(source) = row.get("source").and_then(Value::as_object) else {
        return false;
    };
    if row.get("candidate").and_then(Value::as_object).is_none() {
        return false;
    }

    let source_key = object_id(source);
    let active = catalog_contains(profile.get("_aFiles"), &source_key);
    let archived = catalog_contains(profile.get("_aArchivedFiles"), &source_key);
    if archived || (!active && !archived) {
        return true;
    }

    let candidate = row
        .get("candidate")
        .and_then(Value::as_object)
        .expect("candidate checked above");
    let source_value = Value::Object(source.clone());
    let candidate_value = Value::Object(candidate.clone());
    let source_version = version_text(&source_value);
    let candidate_version = version_text(&candidate_value);
    if features::version_progresses(&source_version, &candidate_version) {
        return true;
    }
    features::filename_version_progresses(&filename_text(source), &filename_text(candidate))
}

enum Decision<'a> {
    Chosen(u64),
    Abstain,
    /// No proof decided and the metadata model did not pick; these are the
    /// candidates the archive path model may still choose from.
    ModelUndecided(Vec<&'a CandidateEvidence>),
}

fn decide_without_paths(candidates: &[CandidateEvidence]) -> Decision<'_> {
    let allowed: Vec<&CandidateEvidence> = candidates
        .iter()
        .filter(|candidate| candidate.eligible && !candidate.vetoed)
        .collect();
    if allowed.is_empty() {
        return Decision::Abstain;
    }

    for proof in [
        unique_proof(&allowed, |candidate| candidate.verified_release_chain),
        unique_proof(&allowed, |candidate| candidate.verified_typed_release),
    ] {
        match proof {
            Some(Some(file_id)) => return Decision::Chosen(file_id),
            Some(None) => return Decision::Abstain,
            None => {}
        }
    }

    let Some(model) = stage1_model() else {
        return Decision::Abstain;
    };
    match model_choice(&allowed, |candidate| candidate.score, model.threshold()) {
        Some(file_id) => Decision::Chosen(file_id),
        None => Decision::ModelUndecided(allowed),
    }
}

/// Choose one candidate using proof first, then the frozen model cutoff,
/// then the archive path model.
///
/// Vetoed/ineligible candidates are removed before any proof is considered.
/// A unique verified release chain outranks a unique typed release proof.
/// Multiple proofs at the same priority abstain and never fall through to a
/// lower priority or the learned model. Model ties are checked before the
/// strict cutoff, including a tie that straddles the cutoff.  The path model
/// runs only when the metadata model did not pick and every remaining
/// candidate has a path score, with the same tie, cutoff and floor rules.
pub fn choose_candidate(candidates: &[CandidateEvidence]) -> Option<u64> {
    match decide_without_paths(candidates) {
        Decision::Chosen(file_id) => Some(file_id),
        Decision::Abstain => None,
        Decision::ModelUndecided(allowed) => {
            if allowed.iter().any(|candidate| candidate.path_score.is_none()) {
                return None;
            }
            let cutoff = stage2_model()?.threshold();
            model_choice(&allowed, |candidate| candidate.path_score, cutoff)
        }
    }
}

/// Whether archive path lists could still change this choice: no proof
/// decided and the metadata model did not pick.
pub fn needs_path_evidence(candidates: &[CandidateEvidence]) -> bool {
    matches!(
        decide_without_paths(candidates),
        Decision::ModelUndecided(_)
    )
}

fn model_choice(
    allowed: &[&CandidateEvidence],
    score: impl Fn(&CandidateEvidence) -> Option<f64>,
    cutoff: f64,
) -> Option<u64> {
    let mut scored: Vec<(&CandidateEvidence, f64)> = allowed
        .iter()
        .filter_map(|candidate| {
            score(candidate)
                .filter(|score| score.is_finite())
                .map(|score| (*candidate, score))
        })
        .collect();
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|left, right| right.1.total_cmp(&left.1));
    let top_score = scored[0].1;
    let ties = scored
        .iter()
        .filter(|(_, score)| (score - top_score).abs() <= SCORE_TIE_EPSILON)
        .count();
    if ties != 1 || top_score <= cutoff {
        return None;
    }
    if !scored[0].0.model_evidence_floor {
        return None;
    }
    Some(scored[0].0.file_id)
}

fn unique_proof(
    candidates: &[&CandidateEvidence],
    predicate: impl Fn(&CandidateEvidence) -> bool,
) -> Option<Option<u64>> {
    let proofs: Vec<&CandidateEvidence> = candidates
        .iter()
        .copied()
        .filter(|candidate| predicate(candidate))
        .collect();
    match proofs.as_slice() {
        [proof] => Some(Some(proof.file_id)),
        [] => None,
        _ => Some(None),
    }
}

/// Return true if a raw JSON numeric value, or a numeric size/date string,
/// cannot be represented as a finite f64. Version labels are deliberately
/// excluded: the feature extractor handles their arbitrary decimal components
/// losslessly and only rejects a resulting infinite feature.
fn contains_unsupported_numeric(value: &Value, key: Option<&str>) -> bool {
    match value {
        Value::Number(number) => number.as_f64().map_or(true, |number| !number.is_finite()),
        Value::String(text) if key.is_some_and(|key| NUMERIC_KEYS.contains(&key)) => {
            numeric_string_overflow(text)
        }
        Value::Array(values) => values
            .iter()
            .any(|value| contains_unsupported_numeric(value, None)),
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| contains_unsupported_numeric(value, Some(key))),
        _ => false,
    }
}

fn numeric_string_overflow(value: &str) -> bool {
    let text = value.trim().replace(',', "");
    if text.is_empty() {
        return false;
    }
    let has_digit = text.chars().any(|character| character.is_ascii_digit());
    let is_plain_number = text
        .chars()
        .all(|character| character.is_ascii_digit() || matches!(character, '+' | '-' | '.'));
    let is_exponent_number = text.chars().all(|character| {
        character.is_ascii_digit() || matches!(character, '+' | '-' | '.' | 'e' | 'E')
    }) && text.chars().any(|character| matches!(character, 'e' | 'E'));
    if !has_digit || (!is_plain_number && !is_exponent_number) {
        return false;
    }
    text.parse::<f64>()
        .map_or(true, |number| !number.is_finite())
}

fn version_text(value: &Value) -> String {
    let Some(object) = value.as_object() else {
        return String::new();
    };
    VERSION_KEYS
        .iter()
        .find_map(|key| object.get(*key))
        .map(|value| match value {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            Value::Bool(value) => value.to_string(),
            other => other.to_string(),
        })
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn object_id(object: &serde_json::Map<String, Value>) -> String {
    let value = object.get("_idRow").or_else(|| object.get("id"));
    match value {
        None | Some(Value::Null) | Some(Value::Bool(_)) => String::new(),
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(other) => other.to_string().trim().to_owned(),
    }
}

fn catalog_contains(value: Option<&Value>, id: &str) -> bool {
    value.and_then(Value::as_array).is_some_and(|items| {
        items.iter().any(|item| {
            item.as_object()
                .is_some_and(|object| object_id(object) == id)
        })
    })
}

fn value_is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.to_string() != "0",
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn value_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn filename_text(object: &serde_json::Map<String, Value>) -> String {
    FILENAME_KEYS
        .iter()
        .find_map(|key| {
            object
                .get(*key)
                .filter(|value| value_is_truthy(value))
                .map(value_string)
        })
        .unwrap_or_default()
}

/// Frozen V2/V3 generic nonnumeric version-label veto. It compares only the
/// source and candidate raw version labels. Numeric-looking versions are
/// accepted as version progression; equal labels after NFC + Unicode casefold
/// are accepted; differing nonnumeric labels veto the pair.
fn version_conflict_veto(row: &Value) -> bool {
    let source = row.get("source").unwrap_or(&Value::Null);
    let candidate = row.get("candidate").unwrap_or(&Value::Null);
    let source_version = version_text(source);
    let candidate_version = version_text(candidate);
    !source_version.is_empty()
        && !candidate_version.is_empty()
        && !features::has_valid_version(&source_version)
        && !features::has_valid_version(&candidate_version)
        && normalized_version(&source_version) != normalized_version(&candidate_version)
}

fn normalized_version(value: &str) -> String {
    features::generated::casefold_str(&features::generated::nfc(value.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(file_id: u64, score: Option<f64>) -> CandidateEvidence {
        CandidateEvidence {
            file_id,
            score,
            path_score: None,
            eligible: true,
            vetoed: false,
            verified_release_chain: false,
            verified_typed_release: false,
            model_evidence_floor: true,
        }
    }

    fn version_row(source_version: &str, candidate_version: &str) -> Value {
        serde_json::json!({
            "profile": {"_aFiles": []},
            "source": {"_idRow": 10, "_sFile": "old.zip", "_sVersion": source_version, "_tsDateAdded": 3, "_nFilesize": 1},
            "candidate": {"_idRow": 11, "_sFile": "new.zip", "_sVersion": candidate_version, "_tsDateAdded": 4, "_nFilesize": 2},
            "logs": null
        })
    }

    #[test]
    fn ordinary_missing_metadata_is_scored() {
        let row = serde_json::json!({"source": {}, "candidate": {}, "profile": {}, "logs": null});
        assert!(score_pair(&row).is_some());
    }

    #[test]
    fn unsupported_extreme_numeric_abstains() {
        let row = serde_json::json!({
            "source": {"_nFilesize": "1e400"},
            "candidate": {}, "profile": {}, "logs": null
        });
        assert!(score_pair(&row).is_none());
    }

    #[test]
    fn numeric_json_overflow_abstains_when_parser_preserves_number() {
        if let Ok(row) = serde_json::from_str::<Value>(
            r#"{"source":{"_nFilesize":1e400},"candidate":{},"profile":{},"logs":null}"#,
        ) {
            assert!(score_pair(&row).is_none());
        }
    }

    #[test]
    fn unicode_casefold_and_nonnumeric_labels_create_conflict() {
        let row = version_row("Ä", "Ö");
        let score = score_pair(&row).expect("ordinary metadata scores");
        assert!(score.version_conflict);
        assert!(
            !score_pair(&version_row("Complete", "complete"))
                .expect("ordinary metadata scores")
                .version_conflict
        );
        assert!(
            !score_pair(&version_row("123", "456"))
                .expect("ordinary metadata scores")
                .version_conflict
        );
    }

    #[test]
    fn version_conflict_uses_lossless_frozen_numeric_parser() {
        assert!(features::has_valid_version("١.٠"));
        assert!(features::has_valid_version("1.23abc"));
        assert!(
            !score_pair(&version_row("١.٠", "٢.٠"))
                .expect("ordinary metadata scores")
                .version_conflict
        );
        assert!(
            !score_pair(&version_row("1.23abc", "2.34abc"))
                .expect("ordinary metadata scores")
                .version_conflict
        );
    }

    #[test]
    fn huge_version_delta_is_unsupported_but_equal_huge_versions_remain_lossless() {
        let huge = format!("3{}", "0".repeat(400));
        assert!(score_pair(&version_row("1", &huge)).is_none());
        assert!(score_pair(&version_row(&huge, &huge)).is_some());
    }

    fn floor_row(
        source: Value,
        candidate: Value,
        source_active: bool,
        source_archived: bool,
    ) -> Value {
        let mut active = vec![candidate];
        let mut archived = Vec::new();
        if source_active {
            active.insert(0, source.clone());
        }
        if source_archived {
            archived.push(source.clone());
        }
        serde_json::json!({
            "profile": {"_aFiles": active, "_aArchivedFiles": archived},
            "source": source,
            "candidate": active.last().cloned().unwrap_or(Value::Null),
        })
    }

    #[test]
    fn model_evidence_floor_matches_catalog_and_numeric_progression() {
        assert!(features::version_progresses("1", "1.0"));
        assert!(!features::version_progresses("1.0", "1"));
        let source = serde_json::json!({"_idRow": 10, "_sFile": "skin.zip", "_sVersion": "1.0"});
        let candidate =
            serde_json::json!({"_idRow": 20, "_sFile": "skin-new.zip", "_sVersion": "1.1"});
        assert!(model_evidence_floor(&floor_row(
            source.clone(),
            candidate.clone(),
            false,
            false
        )));
        assert!(model_evidence_floor(&floor_row(
            source.clone(),
            candidate.clone(),
            true,
            true
        )));
        assert!(model_evidence_floor(&floor_row(
            source.clone(),
            candidate.clone(),
            true,
            false
        )));
        let same_version = serde_json::json!({
            "_idRow": 20,
            "_sFile": "skin-new.zip",
            "_sVersion": "1.0"
        });
        assert!(!model_evidence_floor(&floor_row(
            source,
            same_version,
            true,
            false
        )));
    }

    #[test]
    fn model_evidence_floor_uses_only_explicit_filename_markers() {
        assert!(features::filename_version_progresses(
            "skin_v1.zip",
            "skin_v1.0.zip"
        ));
        assert!(!features::filename_version_progresses(
            "skin_v1.0.zip",
            "skin_v1.zip"
        ));
        let source = serde_json::json!({"_idRow": 10, "_sFile": "skin_v1.0.zip"});
        let bare_number = serde_json::json!({"_idRow": 20, "_sFile": "skin_1.1.zip"});
        assert!(!model_evidence_floor(&floor_row(
            source.clone(),
            bare_number,
            true,
            false
        )));
        let marked = serde_json::json!({"_idRow": 20, "_sFile": "skin_ver1.1.zip"});
        assert!(model_evidence_floor(&floor_row(
            source.clone(),
            marked,
            true,
            false
        )));
        let multiple_markers = serde_json::json!({"_idRow": 20, "_sFile": "skin_v1.1_v2.0.zip"});
        assert!(!model_evidence_floor(&floor_row(
            source.clone(),
            multiple_markers,
            true,
            false
        )));
        let huge = format!("skin_v{}0.zip", "9".repeat(400));
        let huge_candidate = serde_json::json!({"_idRow": 20, "_sFile": huge});
        assert!(model_evidence_floor(&floor_row(
            source,
            huge_candidate,
            true,
            false
        )));
    }

    #[test]
    fn model_evidence_floor_requires_profile_and_files() {
        assert!(!model_evidence_floor(&serde_json::json!({
            "source": {},
            "candidate": {}
        })));
        assert!(!model_evidence_floor(&serde_json::json!({
            "profile": {},
            "source": {}
        })));
    }

    #[test]
    fn vetoes_are_rejected_before_proof_or_model() {
        let mut vetoed_proof = evidence(1, Some(1.0));
        vetoed_proof.vetoed = true;
        vetoed_proof.verified_release_chain = true;
        assert_eq!(choose_candidate(&[vetoed_proof]), None);
    }

    #[test]
    fn model_floor_blocks_only_the_unique_model_winner() {
        let mut high = evidence(1, Some(0.99));
        high.model_evidence_floor = false;
        let low = evidence(2, Some(0.98));
        assert_eq!(choose_candidate(&[high, low]), None);

        let mut proof = evidence(3, Some(0.99));
        proof.model_evidence_floor = false;
        proof.verified_release_chain = true;
        assert_eq!(choose_candidate(&[proof]), Some(3));
    }

    #[test]
    fn unique_chain_then_typed_proof_has_priority() {
        let mut chain = evidence(1, None);
        chain.verified_release_chain = true;
        let mut typed = evidence(2, Some(1.0));
        typed.verified_typed_release = true;
        assert_eq!(choose_candidate(&[typed, chain]), Some(1));
    }

    #[test]
    fn multiple_same_priority_proofs_abstain_without_fallthrough() {
        let mut first = evidence(1, Some(1.0));
        first.verified_release_chain = true;
        let mut second = evidence(2, Some(1.0));
        second.verified_release_chain = true;
        assert_eq!(choose_candidate(&[first, second]), None);
        let mut typed_a = evidence(3, Some(1.0));
        typed_a.verified_typed_release = true;
        let mut typed_b = evidence(4, Some(0.99));
        typed_b.verified_typed_release = true;
        let model = evidence(5, Some(1.0));
        assert_eq!(choose_candidate(&[typed_a, typed_b, model]), None);
    }

    #[test]
    #[cfg(gamebanana_matcher_fixtures)]
    fn compact_three_thousand_row_score_fixture_matches_stage1_model() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            rows: Vec<Row>,
        }
        #[derive(serde::Deserialize)]
        struct Row {
            features: Vec<Option<f64>>,
            raw_score: f64,
            probability: f64,
            accepted: bool,
        }
        let fixture: Fixture =
            serde_json::from_str(include_str!("tests/fixtures/parity-score-fixtures.json"))
                .unwrap();
        assert_eq!(fixture.rows.len(), 3016);
        let model = stage1_model().unwrap();
        for row in fixture.rows {
            let score = model.score_raw(&row.features).unwrap();
            assert!((score.raw_score - row.raw_score).abs() <= 1e-12);
            assert!((score.probability - row.probability).abs() <= 1e-12);
            assert_eq!(score.accepted, row.accepted);
        }
    }

    #[test]
    fn model_requires_strict_cutoff_and_unique_top() {
        let cutoff = stage1_model().unwrap().threshold();
        assert_eq!(choose_candidate(&[evidence(1, Some(cutoff))]), None);
        assert_eq!(
            choose_candidate(&[
                evidence(1, Some(cutoff + 0.01)),
                evidence(2, Some(cutoff + 0.009))
            ]),
            Some(1)
        );
        assert_eq!(
            choose_candidate(&[
                evidence(1, Some(cutoff + 0.01)),
                evidence(2, Some(cutoff + 0.01 - 5e-13))
            ]),
            None
        );
    }

    #[test]
    fn path_model_decides_only_after_the_metadata_model_abstains() {
        let cutoff = stage1_model().unwrap().threshold();
        let path_cutoff = stage2_model().unwrap().threshold();
        let with_path = |file_id, score, path_score| CandidateEvidence {
            path_score,
            ..evidence(file_id, score)
        };

        // Metadata model picks: path scores never override it.
        let picked = [
            with_path(1, Some(cutoff + 0.01), Some(0.0)),
            with_path(2, Some(0.1), Some(path_cutoff + 0.01)),
        ];
        assert!(!needs_path_evidence(&picked));
        assert_eq!(choose_candidate(&picked), Some(1));

        // Metadata model undecided: the path model picks.
        let undecided = [
            with_path(1, Some(0.5), Some(0.1)),
            with_path(2, Some(0.4), Some(path_cutoff + 0.01)),
        ];
        assert!(needs_path_evidence(&undecided));
        assert_eq!(choose_candidate(&undecided), Some(2));

        // One remaining candidate without a list: no path decision.
        let partial = [
            with_path(1, Some(0.5), None),
            with_path(2, Some(0.4), Some(path_cutoff + 0.01)),
        ];
        assert_eq!(choose_candidate(&partial), None);

        // The path pick still needs the evidence floor and the strict cutoff.
        let mut no_floor = with_path(2, Some(0.4), Some(path_cutoff + 0.01));
        no_floor.model_evidence_floor = false;
        assert_eq!(choose_candidate(&[no_floor]), None);
        assert_eq!(
            choose_candidate(&[with_path(2, Some(0.4), Some(path_cutoff))]),
            None
        );

        // Proofs, including an ambiguous one, never reach the path model.
        let mut proof_a = with_path(1, Some(0.1), Some(0.0));
        proof_a.verified_typed_release = true;
        let mut proof_b = with_path(2, Some(0.1), Some(path_cutoff + 0.01));
        proof_b.verified_typed_release = true;
        assert!(!needs_path_evidence(&[proof_a, proof_b]));
        assert_eq!(choose_candidate(&[proof_a, proof_b]), None);

        // Vetoed candidates neither need nor use lists.
        let mut vetoed = with_path(2, Some(0.4), Some(path_cutoff + 0.01));
        vetoed.vetoed = true;
        assert!(!needs_path_evidence(&[vetoed]));
        assert_eq!(choose_candidate(&[vetoed]), None);
    }

    #[test]
    fn path_scores_use_the_metadata_features_plus_path_columns() {
        let row = version_row("1.0", "1.1");
        let known = [0.5; paths::PATH_FEATURE_COUNT];
        let unknown = [f64::NAN; paths::PATH_FEATURE_COUNT];
        let with_lists = score_pair_with_paths(&row, &known).expect("scores");
        let without_lists = score_pair_with_paths(&row, &unknown).expect("scores");
        assert!(with_lists.is_finite() && without_lists.is_finite());
        assert_ne!(with_lists, without_lists);
    }

    #[test]
    fn model_tie_is_checked_before_cutoff_even_when_second_is_below_it() {
        let cutoff = stage1_model().unwrap().threshold();
        assert_eq!(
            choose_candidate(&[
                evidence(1, Some(cutoff + 1e-13)),
                evidence(2, Some(cutoff - 1e-13))
            ]),
            None
        );
    }
}
