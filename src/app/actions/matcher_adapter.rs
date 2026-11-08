use super::*;
use crate::integrations::gamebanana;
use crate::integrations::gamebanana_matcher::paths::group_path_features;
use crate::integrations::gamebanana_matcher::{
    CandidateEvidence, choose_candidate, model_evidence_floor, needs_path_evidence, score_pair,
    score_pair_with_paths,
};
use crate::model::{FileSetRecipe, TrackedFileMeta};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(super) struct CanonicalProfile {
    pub(super) profile: gamebanana::ProfileResponse,
    pub(super) active: Vec<gamebanana::ModFile>,
    pub(super) archived: Vec<gamebanana::ModFile>,
    pub(super) all: Vec<gamebanana::ModFile>,
    pub(super) ambiguous_ids: HashSet<u64>,
}

#[derive(Debug, Clone)]
struct CanonicalBucket {
    files: Vec<gamebanana::ModFile>,
    ambiguous_ids: HashSet<u64>,
}

fn file_order_key(file: &gamebanana::ModFile) -> (u64, String, String, i64, u64, String, String) {
    (
        file.id,
        file.file_name.clone(),
        file.version.clone().unwrap_or_default(),
        file.date_added,
        file.file_size,
        file.description.clone().unwrap_or_default(),
        file.download_url.clone().unwrap_or_default(),
    )
}

fn same_file_metadata(left: &gamebanana::ModFile, right: &gamebanana::ModFile) -> bool {
    left.id == right.id
        && left.file_name == right.file_name
        && left.file_size == right.file_size
        && left.date_added == right.date_added
        && left.description == right.description
        && left.version == right.version
        && left.download_url == right.download_url
        && left.is_archived == right.is_archived
}

fn canonical_bucket(mut files: Vec<gamebanana::ModFile>, archived: bool) -> CanonicalBucket {
    if archived {
        for file in &mut files {
            file.is_archived = true;
        }
    }
    files.sort_by_key(file_order_key);

    let mut output = Vec::with_capacity(files.len());
    let mut ambiguous_ids = HashSet::new();
    let mut by_id: HashMap<u64, Vec<gamebanana::ModFile>> = HashMap::new();
    for file in files {
        if file.id == 0 {
            output.push(file);
        } else {
            by_id.entry(file.id).or_default().push(file);
        }
    }
    let mut ids: Vec<_> = by_id.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let entries = by_id
            .remove(&id)
            .expect("canonical bucket ID was collected from the map");
        if entries
            .windows(2)
            .all(|pair| same_file_metadata(&pair[0], &pair[1]))
        {
            output.push(entries[0].clone());
        } else {
            ambiguous_ids.insert(id);
        }
    }
    output.sort_by_key(file_order_key);
    CanonicalBucket {
        files: output,
        ambiguous_ids,
    }
}

pub(super) fn canonicalize_profile(profile: &gamebanana::ProfileResponse) -> CanonicalProfile {
    let active_bucket = canonical_bucket(profile.files.clone(), false);
    let archived_bucket = canonical_bucket(profile.archived_files.clone(), true);

    let archived_ids: HashSet<u64> = archived_bucket
        .files
        .iter()
        .filter(|file| file.id != 0)
        .map(|file| file.id)
        .collect();
    let mut ambiguous_ids = archived_bucket.ambiguous_ids.clone();
    ambiguous_ids.extend(
        active_bucket
            .ambiguous_ids
            .iter()
            .filter(|id| !archived_ids.contains(id))
            .copied(),
    );

    let active: Vec<_> = active_bucket
        .files
        .into_iter()
        .filter(|file| file.id == 0 || !archived_ids.contains(&file.id))
        .filter(|file| !ambiguous_ids.contains(&file.id))
        .collect();
    let archived: Vec<_> = archived_bucket
        .files
        .into_iter()
        .filter(|file| !ambiguous_ids.contains(&file.id))
        .collect();

    let mut canonical = profile.clone();
    canonical.files = active.clone();
    canonical.archived_files = archived.clone();
    let all = active
        .iter()
        .chain(archived.iter())
        .cloned()
        .collect::<Vec<_>>();
    CanonicalProfile {
        profile: canonical,
        active,
        archived,
        all,
        ambiguous_ids,
    }
}

fn matches_tracked(file: &gamebanana::ModFile, tracked: &TrackedFileMeta) -> bool {
    if tracked.file_id != 0 {
        file.id == tracked.file_id
    } else {
        file.file_name == tracked.file_name && file.date_added == tracked.date_added
    }
}

fn current_source<'a>(
    canonical: &'a CanonicalProfile,
    tracked: &TrackedFileMeta,
) -> Option<(&'a gamebanana::ModFile, bool)> {
    if let Some(file) = canonical
        .archived
        .iter()
        .find(|file| matches_tracked(file, tracked))
    {
        return Some((file, true));
    }
    canonical
        .active
        .iter()
        .find(|file| matches_tracked(file, tracked))
        .map(|file| (file, file.is_archived))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn set_text(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = nonempty(value) {
        map.insert(key.to_owned(), Value::String(value.to_owned()));
    } else {
        map.remove(key);
    }
}

fn set_i64(map: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), Value::Number(value.into()));
    } else {
        map.remove(key);
    }
}

fn set_u64(map: &mut Map<String, Value>, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), Value::Number(value.into()));
    } else {
        map.remove(key);
    }
}

fn source_value(tracked: &TrackedFileMeta, current: Option<(&gamebanana::ModFile, bool)>) -> Value {
    let (remote, is_archived) = current
        .map(|(file, archived)| (Some(file), Some(archived)))
        .unwrap_or((None, None));
    let mut object = remote
        .and_then(|file| serde_json::to_value(file).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    set_u64(
        &mut object,
        "_idRow",
        (tracked.file_id != 0)
            .then_some(tracked.file_id)
            .or_else(|| remote.map(|file| file.id)),
    );
    set_text(
        &mut object,
        "_sFile",
        nonempty(Some(tracked.file_name.as_str()))
            .or_else(|| remote.and_then(|file| nonempty(Some(file.file_name.as_str())))),
    );
    set_u64(
        &mut object,
        "_nFilesize",
        tracked
            .file_size
            .or_else(|| remote.map(|file| file.file_size).filter(|size| *size > 0)),
    );
    set_i64(
        &mut object,
        "_tsDateAdded",
        (tracked.date_added > 0)
            .then_some(tracked.date_added)
            .or_else(|| remote.map(|file| file.date_added)),
    );
    set_text(
        &mut object,
        "_sVersion",
        nonempty(tracked.version.as_deref())
            .or_else(|| remote.and_then(|file| nonempty(file.version.as_deref()))),
    );
    set_text(
        &mut object,
        "_sDescription",
        nonempty(tracked.label.as_deref())
            .or_else(|| remote.and_then(|file| nonempty(file.description.as_deref()))),
    );
    if let Some(archived) = is_archived.or_else(|| tracked.archived.then_some(true)) {
        object.insert("_bIsArchived".to_owned(), Value::Bool(archived));
    } else {
        object.remove("_bIsArchived");
    }
    Value::Object(object)
}

fn candidate_value(file: &gamebanana::ModFile, archived: bool) -> Value {
    let mut object = serde_json::to_value(file)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    object.insert("_bIsArchived".to_owned(), Value::Bool(archived));
    Value::Object(object)
}

fn public_logs(history: Option<&gamebanana::UpdateHistory>) -> Value {
    let Some(history) = history else {
        return Value::Null;
    };
    Value::Array(
        history
            .records
            .iter()
            .filter(|record| record.is_public())
            .filter_map(|record| serde_json::to_value(record).ok())
            .collect(),
    )
}

#[derive(Debug, Clone)]
pub(super) struct PairContext {
    pub(super) canonical: CanonicalProfile,
    pub(super) profile_value: Value,
    pub(super) logs_value: Value,
}

impl PairContext {
    pub(super) fn new(profile: &gamebanana::ProfileResponse) -> Option<Self> {
        let canonical = canonicalize_profile(profile);
        let profile_value = serde_json::to_value(&canonical.profile).ok()?;
        let logs_value = public_logs(canonical.profile.update_history.as_ref());
        Some(Self {
            canonical,
            profile_value,
            logs_value,
        })
    }

    fn row(&self, source: Value, candidate: Value) -> Value {
        let mut row = Map::new();
        row.insert("profile".to_owned(), self.profile_value.clone());
        row.insert("source".to_owned(), source);
        row.insert("candidate".to_owned(), candidate);
        row.insert("logs".to_owned(), self.logs_value.clone());
        Value::Object(row)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeClaim {
    pub(super) recipe_index: usize,
    pub(super) candidate_id: u64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NativeSingleEvaluation {
    pub(super) evaluation: FileSetUpdateEvaluation,
    pub(super) claim: Option<NativeClaim>,
    pub(super) is_native_single: bool,
    pub(super) abstention_state: ModUpdateState,
    pub(super) missing_listing_ids: Vec<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct NativeSingleGroup {
    pub(super) evaluations: Vec<NativeSingleEvaluation>,
}

fn known_source_date(
    tracked: &TrackedFileMeta,
    current: Option<&gamebanana::ModFile>,
) -> Option<i64> {
    (tracked.date_added > 0)
        .then_some(tracked.date_added)
        .or_else(|| current.map(|file| file.date_added).filter(|date| *date > 0))
}

fn source_matches_candidate(candidate: &gamebanana::ModFile, tracked: &TrackedFileMeta) -> bool {
    if tracked.file_id != 0 {
        candidate.id == tracked.file_id
    } else {
        candidate.file_name == tracked.file_name && candidate.date_added == tracked.date_added
    }
}

fn candidate_is_eligible(
    candidate: &gamebanana::ModFile,
    tracked: &TrackedFileMeta,
    current: Option<&gamebanana::ModFile>,
) -> bool {
    candidate
        .download_url
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty())
        && !candidate.is_archived
        && candidate.id != 0
        && !source_matches_candidate(candidate, tracked)
        && current.is_none_or(|source| candidate.id == 0 || candidate.id != source.id)
        && known_source_date(tracked, current).is_some_and(|date| candidate.date_added > date)
}

fn candidate_evidence(
    context: &PairContext,
    tracked: &TrackedFileMeta,
    current: Option<(&gamebanana::ModFile, bool)>,
    candidate: &gamebanana::ModFile,
    profile_files: &[&gamebanana::ModFile],
    active_files: &[&gamebanana::ModFile],
    shape_index: &CatalogShapeIndex,
) -> (CandidateEvidence, Option<Value>) {
    let eligible = candidate_is_eligible(candidate, tracked, current.map(|(file, _)| file));
    if !eligible {
        let evidence = CandidateEvidence {
            file_id: candidate.id,
            score: None,
            path_score: None,
            eligible: false,
            vetoed: true,
            verified_release_chain: false,
            verified_typed_release: false,
            model_evidence_floor: false,
        };
        return (evidence, None);
    }

    let source = source_value(tracked, current);
    let row = context.row(source, candidate_value(candidate, false));
    let score = score_pair(&row);
    let generic_veto = score.map(|pair| pair.version_conflict).unwrap_or(false);
    let graph_veto = super::all_best_sources_veto(
        std::slice::from_ref(tracked),
        candidate,
        &context.canonical.profile,
        profile_files,
        shape_index,
    );
    let verified_release_chain = super::release_sequence_candidate_matches(
        tracked,
        candidate,
        &context.canonical.profile,
        profile_files,
    );
    let verified_typed_release = super::typed_release_candidate_matches(
        tracked,
        candidate,
        &context.canonical.profile,
        profile_files,
        active_files,
    );
    let evidence = CandidateEvidence {
        file_id: candidate.id,
        score: score.map(|pair| pair.probability),
        path_score: None,
        eligible: true,
        vetoed: generic_veto || graph_veto,
        verified_release_chain,
        verified_typed_release,
        model_evidence_floor: model_evidence_floor(&row),
    };
    (evidence, Some(row))
}

fn attach_path_scores(
    context: &PairContext,
    source_id: Option<u64>,
    evidence: &mut [CandidateEvidence],
    rows: &[Option<Value>],
) -> Vec<u64> {
    if !needs_path_evidence(evidence) {
        return Vec::new();
    }
    let Some(source_id) = source_id else {
        return Vec::new();
    };
    let listings = &context.canonical.profile.file_listings;
    let group: Vec<usize> = (0..evidence.len())
        .filter(|index| evidence[*index].eligible)
        .collect();
    let missing: Vec<u64> = std::iter::once(source_id)
        .chain(group.iter().map(|index| evidence[*index].file_id))
        .filter(|file_id| !listings.contains_key(file_id))
        .collect();
    if !missing.is_empty() {
        return missing;
    }
    let candidate_lists: Vec<Option<&[String]>> = group
        .iter()
        .map(|index| listings.get(&evidence[*index].file_id).map(Vec::as_slice))
        .collect();
    let features = group_path_features(
        listings.get(&source_id).map(Vec::as_slice),
        &candidate_lists,
    );
    for (index, features) in group.into_iter().zip(features) {
        if let (Some(features), Some(row)) = (features, rows[index].as_ref()) {
            evidence[index].path_score = score_pair_with_paths(row, &features);
        }
    }
    Vec::new()
}

fn no_update_state(
    tracked: &TrackedFileMeta,
    current: Option<&gamebanana::ModFile>,
) -> ModUpdateState {
    if current.is_some() {
        ModUpdateState::UpToDate
    } else if tracked.file_id != 0 || !tracked.file_name.is_empty() {
        ModUpdateState::MissingSource
    } else {
        ModUpdateState::Unlinked
    }
}

fn evaluate_single(
    index: usize,
    file_set: &FileSetRecipe,
    tracked: TrackedFileMeta,
    context: &PairContext,
) -> NativeSingleEvaluation {
    let current = current_source(&context.canonical, &tracked);
    let profile_files = context.canonical.all.iter().collect::<Vec<_>>();
    let active_files = context
        .canonical
        .active
        .iter()
        .filter(|file| super::profile_file_is_active_downloadable(&context.canonical.profile, file))
        .collect::<Vec<_>>();
    let shape_index = CatalogShapeIndex::from_active_files(&active_files);
    let (mut evidence, rows): (Vec<_>, Vec<_>) = context
        .canonical
        .active
        .iter()
        .map(|candidate| {
            candidate_evidence(
                context,
                &tracked,
                current,
                candidate,
                &profile_files,
                &active_files,
                &shape_index,
            )
        })
        .unzip();
    let source_id = (tracked.file_id != 0)
        .then_some(tracked.file_id)
        .or_else(|| current.map(|(file, _)| file.id).filter(|id| *id != 0));
    let missing_listing_ids = attach_path_scores(context, source_id, &mut evidence, &rows);
    let choice = choose_candidate(&evidence);
    let Some(candidate_id) = choice else {
        return NativeSingleEvaluation {
            evaluation: FileSetUpdateEvaluation {
                state: no_update_state(&tracked, current.map(|(file, _)| file)),
                signature: None,
            },
            claim: None,
            is_native_single: true,
            abstention_state: no_update_state(&tracked, current.map(|(file, _)| file)),
            missing_listing_ids,
        };
    };
    let Some(candidate) = context
        .canonical
        .active
        .iter()
        .find(|file| file.id == candidate_id)
    else {
        return NativeSingleEvaluation {
            evaluation: FileSetUpdateEvaluation {
                state: no_update_state(&tracked, current.map(|(file, _)| file)),
                signature: None,
            },
            claim: None,
            is_native_single: true,
            abstention_state: no_update_state(&tracked, current.map(|(file, _)| file)),
            missing_listing_ids: Vec::new(),
        };
    };
    let signature =
        super::candidate_signature(vec![super::tracked_file_meta_from_mod_file(candidate)]);
    let _ = file_set;
    NativeSingleEvaluation {
        evaluation: FileSetUpdateEvaluation {
            state: ModUpdateState::UpdateAvailable,
            signature,
        },
        claim: Some(NativeClaim {
            recipe_index: index,
            candidate_id,
        }),
        is_native_single: true,
        abstention_state: no_update_state(&tracked, current.map(|(file, _)| file)),
        missing_listing_ids: Vec::new(),
    }
}

pub(super) fn evaluate_native_single_group(
    items: &[(Option<i64>, FileSetRecipe)],
    profile: &gamebanana::ProfileResponse,
) -> Option<NativeSingleGroup> {
    if gamebanana::is_unavailable(profile) {
        return None;
    }
    let context = PairContext::new(profile)?;
    let all_remote = context.canonical.all.iter().collect::<Vec<_>>();
    let mut evaluations = Vec::with_capacity(items.len());
    for (index, (_, file_set)) in items.iter().enumerate() {
        let ResolvedTrackedFiles::Tracked(mut tracked_files) =
            super::resolve_tracked_files(file_set, &all_remote)
        else {
            evaluations.push(NativeSingleEvaluation::default());
            continue;
        };
        if tracked_files.len() != 1 {
            evaluations.push(NativeSingleEvaluation::default());
            continue;
        }
        evaluations.push(evaluate_single(
            index,
            file_set,
            tracked_files.remove(0),
            &context,
        ));
    }
    Some(NativeSingleGroup {
        evaluations,
    })
}

pub(super) fn abstain_competing_native_claims(
    evaluations: &mut [NativeSingleEvaluation],
    competing_candidate_ids: &HashSet<u64>,
) {
    let mut by_candidate: HashMap<u64, Vec<usize>> = HashMap::new();
    for (index, evaluation) in evaluations.iter().enumerate() {
        if let Some(claim) = evaluation.claim {
            by_candidate
                .entry(claim.candidate_id)
                .or_default()
                .push(index);
        }
    }
    for (candidate_id, indexes) in by_candidate {
        if indexes.len() > 1 || competing_candidate_ids.contains(&candidate_id) {
            for index in indexes {
                evaluations[index].claim = None;
                evaluations[index].evaluation.signature = None;
                evaluations[index].evaluation.state = evaluations[index].abstention_state;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracked(name: &str, date_added: i64) -> TrackedFileMeta {
        TrackedFileMeta {
            file_id: 0,
            file_name: name.to_owned(),
            file_size: None,
            date_added,
            version: None,
            archived: false,
            label: None,
        }
    }

    #[test]
    fn missing_legacy_size_is_omitted_from_source_row() {
        let value = source_value(&tracked("legacy.zip", 100), None);
        let object = value.as_object().expect("source object");
        assert!(!object.contains_key("_nFilesize"));
    }

    #[test]
    fn strict_id_lookup_does_not_adopt_same_name_with_different_id() {
        let tracked = TrackedFileMeta {
            file_id: 10,
            file_name: "old.zip".to_owned(),
            date_added: 100,
            ..tracked("old.zip", 100)
        };
        let mut profile = gamebanana::ProfileResponse::default();
        profile.files.push(gamebanana::ModFile {
            id: 20,
            file_name: "old.zip".to_owned(),
            file_size: 1,
            date_added: 100,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some("https://example.com/old.zip".to_owned()),
            is_archived: false,
            md5: None,
        });
        let canonical = canonicalize_profile(&profile);
        assert!(current_source(&canonical, &tracked).is_none());
        let value = source_value(&tracked, None);
        assert_eq!(value["_idRow"], 10);
    }

    #[test]
    fn competing_claim_abstains_the_whole_recipe() {
        let candidate_id = 77;
        let mut evaluations = vec![NativeSingleEvaluation {
            evaluation: FileSetUpdateEvaluation {
                state: ModUpdateState::UpdateAvailable,
                signature: Some(IgnoredUpdateSignature {
                    files: vec![tracked("new.zip", 200)],
                    profile_update_ts: None,
                    prearmed_next_update: false,
                }),
            },
            claim: Some(NativeClaim {
                recipe_index: 0,
                candidate_id,
            }),
            is_native_single: true,
            abstention_state: ModUpdateState::UpToDate,
            missing_listing_ids: Vec::new(),
        }];

        abstain_competing_native_claims(&mut evaluations, &HashSet::from([candidate_id]));

        assert!(evaluations[0].claim.is_none());
        assert!(evaluations[0].evaluation.signature.is_none());
        assert_eq!(evaluations[0].evaluation.state, ModUpdateState::UpToDate);
    }
}
