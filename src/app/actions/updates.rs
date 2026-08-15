fn tracked_file_meta_from_mod_file(file: &gamebanana::ModFile) -> TrackedFileMeta {
    TrackedFileMeta {
        file_id: file.id,
        file_name: file.file_name.clone(),
        date_added: file.date_added,
        version: file.version.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| v.to_string()),
        archived: file.is_archived,
        label: file
            .description
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string),
    }
}

fn selected_file_baseline_ts(file_set: &FileSetRecipe) -> Option<i64> {
    file_set.selected_files_meta.iter().map(|file| file.date_added).max()
}

#[derive(Debug, Clone, Default)]
struct FileSetUpdateEvaluation {
    state: ModUpdateState,
    signature: Option<IgnoredUpdateSignature>,
}

#[derive(Debug, Clone)]
enum ResolvedTrackedFiles {
    Tracked(Vec<TrackedFileMeta>),
    MissingSource,
    Untracked,
}

const FILE_LINEAGE_AUTO_MATCH_THRESHOLD: f32 = 0.72;
const FILE_LINEAGE_AUTO_MATCH_MARGIN: f32 = 0.18;
const FILE_LINEAGE_COMMON_PREFIX_MIN_CHARS: usize = 6;
const FILE_LINEAGE_MIN_DISTINCTIVE_CHARS: usize = 3;

fn candidate_signature(mut candidates: Vec<TrackedFileMeta>) -> Option<IgnoredUpdateSignature> {
    if candidates.is_empty() {
        None
    } else {
        candidates.sort_by(|a, b| {
            b.date_added
                .cmp(&a.date_added)
                .then_with(|| a.file_name.cmp(&b.file_name))
                .then_with(|| a.file_id.cmp(&b.file_id))
        });
        candidates.dedup_by(|a, b| {
            a.file_id == b.file_id
                || (a.file_name == b.file_name && a.date_added == b.date_added)
        });
        Some(IgnoredUpdateSignature {
            files: candidates,
            profile_update_ts: None,
            prearmed_next_update: false,
        })
    }
}

fn update_candidate_files_from_signature(
    signature: Option<&IgnoredUpdateSignature>,
    selectable: &[gamebanana::ModFile],
) -> Vec<gamebanana::ModFile> {
    let Some(signature) = signature else {
        return Vec::new();
    };
    let mut files = Vec::with_capacity(signature.files.len());
    for tracked in &signature.files {
        if let Some(file) = selectable
            .iter()
            .find(|file| {
                (tracked.file_id != 0 && file.id == tracked.file_id)
                    || (file.file_name == tracked.file_name
                        && file.date_added == tracked.date_added)
            })
            .cloned()
        {
            files.push(file);
        }
    }
    files.sort_by(|a, b| {
        b.date_added
            .cmp(&a.date_added)
            .then_with(|| a.file_name.cmp(&b.file_name))
            .then_with(|| a.id.cmp(&b.id))
    });
    files.dedup_by(|a, b| a.id == b.id || (a.file_name == b.file_name && a.date_added == b.date_added));
    files
}

fn downloadable_active_files(profile: &gamebanana::ProfileResponse) -> Vec<&gamebanana::ModFile> {
    profile
        .files
        .iter()
        .filter(|file| file.download_url.is_some() && !file.is_archived)
        .collect()
}

fn downloadable_all_files(profile: &gamebanana::ProfileResponse) -> Vec<&gamebanana::ModFile> {
    profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| file.download_url.is_some())
        .collect()
}

fn remote_file_matches_tracked(file: &gamebanana::ModFile, tracked: &TrackedFileMeta) -> bool {
    (tracked.file_id != 0 && file.id == tracked.file_id)
        || (file.file_name == tracked.file_name && file.date_added == tracked.date_added)
}

fn tracked_files_still_exist(
    tracked_files: &[TrackedFileMeta],
    all_remote_files: &[&gamebanana::ModFile],
) -> bool {
    tracked_files.iter().all(|tracked| {
        all_remote_files
            .iter()
            .any(|file| remote_file_matches_tracked(file, tracked))
    })
}

fn resolve_tracked_files(
    file_set: &FileSetRecipe,
    all_remote_files: &[&gamebanana::ModFile],
) -> ResolvedTrackedFiles {
    if !file_set.selected_files_meta.is_empty() {
        return ResolvedTrackedFiles::Tracked(file_set.selected_files_meta.clone());
    }

    if !file_set.selected_file_ids.is_empty() {
        let selected_ids: HashSet<u64> = file_set.selected_file_ids.iter().copied().collect();
        let tracked_files: Vec<_> = all_remote_files
            .iter()
            .copied()
            .filter(|file| selected_ids.contains(&file.id))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        return if tracked_files.len() == selected_ids.len() {
            ResolvedTrackedFiles::Tracked(tracked_files)
        } else {
            ResolvedTrackedFiles::MissingSource
        };
    }

    if !file_set.selected_file_names.is_empty() {
        let mut tracked_files = Vec::with_capacity(file_set.selected_file_names.len());
        for selected_name in &file_set.selected_file_names {
            if let Some(file) = all_remote_files
                .iter()
                .copied()
                .find(|file| file.file_name == *selected_name)
            {
                tracked_files.push(tracked_file_meta_from_mod_file(file));
            } else {
                tracked_files.push(TrackedFileMeta {
                    file_id: 0,
                    file_name: selected_name.clone(),
                    date_added: 0,
                    version: None,
                    archived: false,
                    label: None,
                });
            }
        }
        return ResolvedTrackedFiles::Tracked(tracked_files);
    }

    ResolvedTrackedFiles::Untracked
}

/// True when the file set names specific remote files, i.e. the update check
/// would compare them against the profile's downloadable files and could flip
/// the mod to `MissingSource` if none of them are found.
fn file_set_tracks_remote_files(file_set: &FileSetRecipe) -> bool {
    !file_set.selected_files_meta.is_empty()
        || !file_set.selected_file_ids.is_empty()
        || !file_set.selected_file_names.is_empty()
}

fn normalized_file_label(label: &str) -> String {
    label
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// Pairs a tracked file with a candidate through the author-assigned file label
/// ("Main File" vs "Experimental"). This is the only usable lineage signal when
/// file names are random hashes and every re-upload replaces the old file.
fn file_label_lineage_matches(tracked: &TrackedFileMeta, candidate: &gamebanana::ModFile) -> bool {
    let tracked_label = tracked
        .label
        .as_deref()
        .map(normalized_file_label)
        .filter(|label| !label.is_empty());
    let candidate_label = candidate
        .description
        .as_deref()
        .map(normalized_file_label)
        .filter(|label| !label.is_empty());
    matches!((tracked_label, candidate_label), (Some(a), Some(b)) if a == b)
}

fn normalized_file_stem_for_lineage(file_name: &str) -> String {
    let without_extension = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    without_extension
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn longest_common_prefix_len(values: &[String]) -> usize {
    let Some(first) = values.first() else {
        return 0;
    };
    let mut prefix_len = first.chars().count();
    for value in values.iter().skip(1) {
        let common = first
            .chars()
            .zip(value.chars())
            .take_while(|(a, b)| a == b)
            .count();
        prefix_len = prefix_len.min(common);
        if prefix_len == 0 {
            break;
        }
    }
    prefix_len
}

fn lineage_common_prefix(tracked_files: &[TrackedFileMeta]) -> String {
    let mut stems: Vec<String> = tracked_files
        .iter()
        .map(|file| normalized_file_stem_for_lineage(&file.file_name))
        .filter(|stem| stem.chars().count() >= FILE_LINEAGE_MIN_DISTINCTIVE_CHARS)
        .collect();
    stems.sort();
    stems.dedup();

    if stems.len() < 2 {
        return String::new();
    }

    let prefix_len = longest_common_prefix_len(&stems);
    let shortest_len = stems.iter().map(|stem| stem.chars().count()).min().unwrap_or(0);
    if prefix_len < FILE_LINEAGE_COMMON_PREFIX_MIN_CHARS
        || shortest_len.saturating_sub(prefix_len) < FILE_LINEAGE_MIN_DISTINCTIVE_CHARS
    {
        return String::new();
    }

    stems[0].chars().take(prefix_len).collect()
}

fn strip_lineage_common_prefix(value: &str, common_prefix: &str) -> String {
    if common_prefix.is_empty() || !value.starts_with(common_prefix) {
        return value.to_string();
    }
    let stripped: String = value.chars().skip(common_prefix.chars().count()).collect();
    if stripped.chars().count() >= FILE_LINEAGE_MIN_DISTINCTIVE_CHARS {
        stripped
    } else {
        value.to_string()
    }
}

fn split_version_marker_prefix(value: &str) -> Option<&str> {
    let rest = value.strip_prefix('v')?;
    if rest.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        let remaining = rest.trim_start_matches(|ch: char| ch.is_ascii_digit());
        if remaining.chars().count() >= FILE_LINEAGE_MIN_DISTINCTIVE_CHARS {
            return Some(remaining);
        }
    }
    None
}

fn split_version_marker_suffix(value: &str) -> Option<&str> {
    let digit_start = value
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_ascii_digit())
        .map(|(idx, ch)| idx + ch.len_utf8())?;
    if digit_start >= value.len() {
        return None;
    }
    let before_digits = &value[..digit_start];
    let marker_start = before_digits.strip_suffix('v')?;
    if marker_start.chars().count() >= FILE_LINEAGE_MIN_DISTINCTIVE_CHARS {
        Some(marker_start)
    } else {
        None
    }
}

fn strip_release_marker_once(value: &str) -> Option<String> {
    if let Some(rest) = split_version_marker_prefix(value) {
        return Some(rest.to_string());
    }
    if let Some(rest) = split_version_marker_suffix(value) {
        return Some(rest.to_string());
    }

    const WORD_MARKERS: &[&str] = &[
        "hotfix", "updated", "update", "fixed", "fix", "new", "final",
    ];
    for marker in WORD_MARKERS {
        if let Some(rest) = value.strip_prefix(marker) {
            if rest.chars().count() >= FILE_LINEAGE_MIN_DISTINCTIVE_CHARS {
                return Some(rest.to_string());
            }
        }
        if let Some(rest) = value.strip_suffix(marker) {
            if rest.chars().count() >= FILE_LINEAGE_MIN_DISTINCTIVE_CHARS {
                return Some(rest.to_string());
            }
        }
    }

    None
}

fn strip_release_markers(value: &str) -> String {
    let mut current = value.to_string();
    while let Some(next) = strip_release_marker_once(&current) {
        if next == current {
            break;
        }
        current = next;
    }
    current
}

fn lineage_match_core(file_name: &str, common_prefix: &str) -> String {
    let normalized = normalized_file_stem_for_lineage(file_name);
    let distinctive = strip_lineage_common_prefix(&normalized, common_prefix);
    strip_release_markers(&distinctive)
}

fn longest_common_subsequence_len(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() || b.is_empty() {
        return 0;
    }

    let mut previous = vec![0usize; b.len() + 1];
    let mut current = vec![0usize; b.len() + 1];
    for a_ch in &a {
        for (b_idx, b_ch) in b.iter().enumerate() {
            current[b_idx + 1] = if a_ch == b_ch {
                previous[b_idx] + 1
            } else {
                current[b_idx].max(previous[b_idx + 1])
            };
        }
        std::mem::swap(&mut previous, &mut current);
        current.fill(0);
    }
    previous[b.len()]
}

fn lineage_core_similarity(source_core: &str, candidate_core: &str) -> f32 {
    let source_len = source_core.chars().count();
    let candidate_len = candidate_core.chars().count();
    if source_len == 0 || candidate_len == 0 {
        return 0.0;
    }
    if source_core == candidate_core {
        return 1.0;
    }

    let matched = longest_common_subsequence_len(source_core, candidate_core);
    if matched < FILE_LINEAGE_MIN_DISTINCTIVE_CHARS {
        return 0.0;
    }
    let unmatched_source = source_len.saturating_sub(matched);
    let unmatched_candidate = candidate_len.saturating_sub(matched);
    let denominator = source_len.max(candidate_len) as f32;
    ((matched as f32
        - 0.65 * unmatched_source as f32
        - 0.65 * unmatched_candidate as f32)
        / denominator)
        .clamp(0.0, 1.0)
}

fn file_lineage_similarity(
    source_name: &str,
    candidate_name: &str,
    common_prefix: &str,
) -> f32 {
    let source_core = lineage_match_core(source_name, common_prefix);
    let candidate_core = lineage_match_core(candidate_name, common_prefix);
    let distinctive_score = lineage_core_similarity(&source_core, &candidate_core);

    let source_full = strip_release_markers(&normalized_file_stem_for_lineage(source_name));
    let candidate_full = strip_release_markers(&normalized_file_stem_for_lineage(candidate_name));
    let full_score = lineage_core_similarity(&source_full, &candidate_full);

    distinctive_score.max(full_score)
}

fn evaluate_file_set_update_group(
    items: &[(Option<i64>, FileSetRecipe)],
    profile: &gamebanana::ProfileResponse,
) -> Vec<FileSetUpdateEvaluation> {
    if gamebanana::is_unavailable(profile) {
        return items
            .iter()
            .map(|_| FileSetUpdateEvaluation {
                state: ModUpdateState::MissingSource,
                signature: None,
            })
            .collect();
    }

    let all_remote_files = downloadable_all_files(profile);
    let active_remote_files = downloadable_active_files(profile);
    let resolved: Vec<_> = items
        .iter()
        .map(|(_, file_set)| resolve_tracked_files(file_set, &all_remote_files))
        .collect();
    let all_tracked_files: Vec<_> = resolved
        .iter()
        .filter_map(|resolved| match resolved {
            ResolvedTrackedFiles::Tracked(files) => Some(files.as_slice()),
            ResolvedTrackedFiles::MissingSource | ResolvedTrackedFiles::Untracked => None,
        })
        .flatten()
        .cloned()
        .collect();
    let common_prefix = lineage_common_prefix(&all_tracked_files);
    let mut assigned_candidates: Vec<Vec<TrackedFileMeta>> = vec![Vec::new(); items.len()];

    for candidate in active_remote_files {
        let mut scores = Vec::with_capacity(resolved.len());
        for (idx, resolved_files) in resolved.iter().enumerate() {
            let best_score = match resolved_files {
                ResolvedTrackedFiles::Tracked(tracked_files) => tracked_files
                    .iter()
                    .filter(|tracked| {
                        candidate.date_added > tracked.date_added
                            && !remote_file_matches_tracked(candidate, tracked)
                    })
                    .map(|tracked| {
                        if file_label_lineage_matches(tracked, candidate) {
                            1.0
                        } else {
                            file_lineage_similarity(
                                &tracked.file_name,
                                &candidate.file_name,
                                &common_prefix,
                            )
                        }
                    })
                    .fold(0.0_f32, f32::max),
                ResolvedTrackedFiles::MissingSource | ResolvedTrackedFiles::Untracked => 0.0,
            };
            scores.push((idx, best_score));
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let Some((best_idx, best_score)) = scores.first().copied() else {
            continue;
        };
        if best_score < FILE_LINEAGE_AUTO_MATCH_THRESHOLD {
            continue;
        }
        let runner_up_score = scores.get(1).map(|(_, score)| *score).unwrap_or(0.0);
        if runner_up_score > 0.0 && best_score - runner_up_score < FILE_LINEAGE_AUTO_MATCH_MARGIN {
            continue;
        }

        assigned_candidates[best_idx].push(tracked_file_meta_from_mod_file(candidate));
    }

    items
        .iter()
        .zip(resolved.iter())
        .enumerate()
        .map(|(idx, ((local_ts, _), resolved_files))| {
            if !assigned_candidates[idx].is_empty() {
                return FileSetUpdateEvaluation {
                    state: ModUpdateState::UpdateAvailable,
                    signature: candidate_signature(assigned_candidates[idx].clone()),
                };
            }

            match resolved_files {
                ResolvedTrackedFiles::Tracked(tracked_files) => {
                    if tracked_files_still_exist(tracked_files, &all_remote_files) {
                        FileSetUpdateEvaluation {
                            state: ModUpdateState::UpToDate,
                            signature: None,
                        }
                    } else {
                        FileSetUpdateEvaluation {
                            state: ModUpdateState::MissingSource,
                            signature: None,
                        }
                    }
                }
                ResolvedTrackedFiles::MissingSource => FileSetUpdateEvaluation {
                    state: ModUpdateState::MissingSource,
                    signature: None,
                },
                ResolvedTrackedFiles::Untracked => FileSetUpdateEvaluation {
                    state: determine_update_state(*local_ts, profile),
                    signature: None,
                },
            }
        })
        .collect()
}

fn compute_update_signature(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    evaluate_file_set_update_group(&[(selected_file_baseline_ts(file_set), file_set.clone())], profile)
        .into_iter()
        .next()
        .and_then(|evaluation| evaluation.signature)
}

fn profile_update_signature(
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    profile
        .date_updated
        .or(Some(profile.date_modified))
        .filter(|update_ts| *update_ts > 0)
        .map(|update_ts| IgnoredUpdateSignature {
            files: Vec::new(),
            profile_update_ts: Some(update_ts),
            prearmed_next_update: false,
        })
}

fn prearm_next_update_signature(
    mut signature: IgnoredUpdateSignature,
) -> IgnoredUpdateSignature {
    signature.prearmed_next_update = true;
    signature
}

fn current_remote_signature(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    let all_remote_files: Vec<&gamebanana::ModFile> = profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| file.download_url.is_some())
        .collect();
    if !file_set.selected_files_meta.is_empty() {
        let tracked_files: Vec<_> = file_set
            .selected_files_meta
            .iter()
            .filter_map(|tracked| {
                all_remote_files
                    .iter()
                    .copied()
                    .find(|file| {
                        file.id == tracked.file_id
                            || (file.file_name == tracked.file_name
                                && file.date_added == tracked.date_added)
                    })
                    .map(tracked_file_meta_from_mod_file)
            })
            .collect();
        if tracked_files.len() == file_set.selected_files_meta.len() {
            return candidate_signature(tracked_files).map(prearm_next_update_signature);
        }
    }
    if !file_set.selected_file_ids.is_empty() {
        let selected_ids: HashSet<u64> = file_set.selected_file_ids.iter().copied().collect();
        let tracked_files: Vec<_> = all_remote_files
            .iter()
            .copied()
            .filter(|file| selected_ids.contains(&file.id))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        if tracked_files.len() == selected_ids.len() {
            return candidate_signature(tracked_files).map(prearm_next_update_signature);
        }
    }
    if !file_set.selected_file_names.is_empty() {
        let selected_names: HashSet<&str> = file_set
            .selected_file_names
            .iter()
            .map(String::as_str)
            .collect();
        let tracked_files: Vec<_> = all_remote_files
            .iter()
            .copied()
            .filter(|file| selected_names.contains(file.file_name.as_str()))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        if tracked_files.len() == selected_names.len() {
            return candidate_signature(tracked_files).map(prearm_next_update_signature);
        }
    }
    profile_update_signature(profile).map(prearm_next_update_signature)
}

fn current_update_signature_for_state(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
    raw_state: ModUpdateState,
) -> Option<IgnoredUpdateSignature> {
    compute_update_signature(file_set, profile).or_else(|| {
        matches!(raw_state, ModUpdateState::UpdateAvailable)
            .then(|| profile_update_signature(profile))
            .flatten()
    })
}

fn source_profile_for_compare(source: &ModSourceData) -> Option<gamebanana::ProfileResponse> {
    source
        .raw_profile_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<gamebanana::ProfileResponse>(raw).ok())
        .or_else(|| source.snapshot.as_ref().map(|snapshot| profile_to_response(Some(snapshot))))
}

fn compute_raw_update_state(mod_entry: &ModEntry) -> Option<ModUpdateState> {
    let source = mod_entry.source.as_ref()?;
    let profile = source_profile_for_compare(source)?;
    let has_local_changes = source.baseline_content_mtime.map(|t| t.timestamp())
        != mod_entry.content_mtime.map(|t| t.timestamp())
        || source.baseline_ini_hash != mod_entry.ini_hash;
    if has_local_changes {
        Some(ModUpdateState::ModifiedLocally)
    } else {
        let local_sync_ts = selected_file_baseline_ts(&source.file_set)
            .or(profile.date_updated.or(Some(profile.date_modified)));
        Some(determine_file_set_update_state(&source.file_set, local_sync_ts, &profile))
    }
}

fn mod_has_local_changes_for_update_check(mod_entry: &ModEntry) -> bool {
    let Some(source) = mod_entry.source.as_ref() else {
        return false;
    };
    source.baseline_content_mtime.map(|t| t.timestamp())
        != mod_entry.content_mtime.map(|t| t.timestamp())
        || source.baseline_ini_hash != mod_entry.ini_hash
}

fn apply_ignored_update_override(
    source: &mut ModSourceData,
    raw_state: ModUpdateState,
    profile: Option<&gamebanana::ProfileResponse>,
) -> ModUpdateState {
    if source.ignore_update_always {
        source.ignored_update_signature = None;
        return ModUpdateState::IgnoringUpdateAlways;
    }
    let current_signature =
        profile.and_then(|profile| current_update_signature_for_state(&source.file_set, profile, raw_state));
    match raw_state {
        ModUpdateState::UpdateAvailable => {
            if let Some(current) = current_signature.as_ref() {
                if source
                    .ignored_update_signature
                    .as_ref()
                    .is_some_and(|ignored| ignored.prearmed_next_update)
                {
                    let mut ignored = current.clone();
                    ignored.prearmed_next_update = false;
                    source.ignored_update_signature = Some(ignored);
                    return ModUpdateState::IgnoringUpdateOnce;
                }
                if source
                    .ignored_update_signature
                    .as_ref()
                    .is_some_and(|ignored| ignored == current)
                {
                    return ModUpdateState::IgnoringUpdateOnce;
                }
            }
        }
        ModUpdateState::UpToDate
        | ModUpdateState::CheckSkipped
        => {
            if !source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|ignored| ignored.prearmed_next_update)
            {
                source.ignored_update_signature = None;
            }
        }
        ModUpdateState::Unlinked
        | ModUpdateState::MissingSource
        | ModUpdateState::IgnoringUpdateOnce
        | ModUpdateState::IgnoringUpdateAlways => {
            source.ignored_update_signature = None;
        }
        ModUpdateState::ModifiedLocally => {}
    }

    if matches!(raw_state, ModUpdateState::ModifiedLocally) {
        if let Some(current) = current_signature.as_ref() {
            if source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|ignored| ignored.prearmed_next_update)
            {
                let mut ignored = current.clone();
                ignored.prearmed_next_update = false;
                source.ignored_update_signature = Some(ignored);
            }
        }
    }

    if source
        .ignored_update_signature
        .as_ref()
        .is_some_and(|ignored| {
            !ignored.prearmed_next_update && current_signature.as_ref() != Some(ignored)
        })
    {
        source.ignored_update_signature = None;
    }

    raw_state
}

fn ignore_once_signature_for_mod(mod_entry: &ModEntry) -> Option<IgnoredUpdateSignature> {
    let source = mod_entry.source.as_ref()?;
    let profile = source_profile_for_compare(source)?;
    let raw_state = if matches!(mod_entry.update_state, ModUpdateState::ModifiedLocally) {
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or_else(|| source.snapshot.as_ref().and_then(|snapshot| snapshot.update_ts))
                .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));
        determine_file_set_update_state(&source.file_set, local_sync_ts, &profile)
    } else {
        mod_entry.update_state
    };
    current_update_signature_for_state(&source.file_set, &profile, raw_state)
        .or_else(|| current_remote_signature(&source.file_set, &profile))
}

fn determine_file_set_update_state(
    file_set: &FileSetRecipe,
    local_ts: Option<i64>,
    profile: &gamebanana::ProfileResponse,
) -> ModUpdateState {
    evaluate_file_set_update_group(&[(local_ts, file_set.clone())], profile)
        .into_iter()
        .next()
        .map(|evaluation| evaluation.state)
        .unwrap_or(ModUpdateState::MissingSource)
}

fn backfill_selected_files_meta(file_set: &mut FileSetRecipe, profile: &gamebanana::ProfileResponse) -> bool {
    if !file_set.selected_files_meta.is_empty() || file_set.selected_file_ids.is_empty() {
        return false;
    }
    let matched: Vec<_> = profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| file_set.selected_file_ids.contains(&file.id))
        .map(tracked_file_meta_from_mod_file)
        .collect();
    if matched.is_empty() {
        return false;
    }
    file_set.selected_files_meta = matched;
    true
}

/// Adopts labels onto tracked files persisted before labels existed, while the
/// original file can still be found remotely. Once the author re-uploads (old
/// file gone), the label is the only way to identify the successor.
fn backfill_tracked_file_labels(
    file_set: &mut FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> bool {
    let mut changed = false;
    for tracked in &mut file_set.selected_files_meta {
        if tracked.label.is_some() {
            continue;
        }
        let label = profile
            .files
            .iter()
            .chain(profile.archived_files.iter())
            .find(|file| remote_file_matches_tracked(file, tracked))
            .and_then(|file| file.description.as_deref())
            .map(str::trim)
            .filter(|label| !label.is_empty());
        if let Some(label) = label {
            tracked.label = Some(label.to_string());
            changed = true;
        }
    }
    changed
}

fn determine_update_state(local_ts: Option<i64>, profile: &gamebanana::ProfileResponse) -> ModUpdateState {
    if gamebanana::is_unavailable(profile) {
        return ModUpdateState::MissingSource;
    }
    let remote_ts = profile.date_updated.or(Some(profile.date_modified));
    if let (Some(local), Some(remote)) = (local_ts, remote_ts) {
        if remote > local {
            return ModUpdateState::UpdateAvailable;
        }
    }
    ModUpdateState::UpToDate
}

fn should_check_update_state(state: ModUpdateState) -> bool {
    state != ModUpdateState::MissingSource
}

fn profile_to_response(snapshot: Option<&GameBananaSnapshot>) -> gamebanana::ProfileResponse {
    snapshot
        .map(|s| gamebanana::ProfileResponse {
            is_private: s.is_private,
            is_deleted: s.is_deleted,
            is_trashed: s.is_trashed,
            is_withheld: s.is_withheld,
            date_updated: s.update_ts,
            ..Default::default()
        })
        .unwrap_or_default()
}

fn profile_to_snapshot(profile: &gamebanana::ProfileResponse) -> GameBananaSnapshot {
    let mut files = Vec::with_capacity(profile.files.len() + profile.archived_files.len());
    for file in &profile.files {
        files.push(GameBananaFileMeta {
            file_id: file.id,
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            date_added: file.date_added,
            download_count: file.download_count,
            description: file.description.clone(),
            download_url: file.download_url.clone(),
            archived: false,
        });
    }
    for file in &profile.archived_files {
        files.push(GameBananaFileMeta {
            file_id: file.id,
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            date_added: file.date_added,
            download_count: file.download_count,
            description: file.description.clone(),
            download_url: file.download_url.clone(),
            archived: true,
        });
    }
    GameBananaSnapshot {
        title: profile.name.clone(),
        authors: gamebanana::all_authors(profile),
        version: None,
        publish_ts: Some(profile.date_added),
        update_ts: profile.date_updated.or(Some(profile.date_modified)),
        description: profile.short_description.clone(),
        preview_urls: profile
            .preview_media
            .as_ref()
            .map(|preview| preview.images.iter().map(gamebanana::full_image_url).collect())
            .unwrap_or_default(),
        files,
        is_private: profile.is_private,
        is_deleted: profile.is_deleted,
        is_trashed: profile.is_trashed,
        is_withheld: profile.is_withheld,
        unsafe_content: !profile.content_ratings.is_empty(),
    }
}

impl HestiaApp {
    fn update_check_item_for_mod(
        &self,
        mod_entry_id: &str,
    ) -> Option<(String, String, u64, Option<i64>, FileSetRecipe, bool)> {
        let mod_entry = self.state.mods.iter().find(|m| m.id == mod_entry_id)?;
        let source = mod_entry.source.as_ref()?;
        let link = source.gamebanana.as_ref()?;
        let local_sync_ts = selected_file_baseline_ts(&source.file_set)
            .or_else(|| source.snapshot.as_ref().and_then(|s| s.update_ts))
            .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));
        Some((
            mod_entry.id.clone(),
            mod_entry.game_id.clone(),
            link.mod_id,
            local_sync_ts,
            source.file_set.clone(),
            gamebanana::is_tool_url(&link.url),
        ))
    }

    fn dispatch_update_check_items(
        &mut self,
        items: Vec<(String, String, u64, Option<i64>, FileSetRecipe, bool)>,
    ) {
        if items.is_empty() {
            return;
        }
        self.update_check_active_items = items.clone();
        if self.update_check_tx.send(UpdateCheckRequest {
            generation: self.update_check_generation,
            items,
        }).is_ok() {
            self.update_check_inflight = true;
        }
    }

    fn restart_active_update_check_for_proxy_change(&mut self) {
        if !self.update_check_inflight || self.update_check_active_items.is_empty() {
            return;
        }
        self.update_check_generation = self.update_check_generation.wrapping_add(1);
        let items = self.update_check_active_items.clone();
        self.update_check_inflight = false;
        self.dispatch_update_check_items(items);
    }

    fn queue_update_check_for_mod(&mut self, mod_entry_id: &str) {
        if self.update_check_inflight {
            self.pending_update_check_mods
                .insert(mod_entry_id.to_string());
            return;
        }
        self.pending_update_check_mods.remove(mod_entry_id);
        let Some(item) = self.update_check_item_for_mod(mod_entry_id) else {
            return;
        };
        self.pending_update_check_game = None;
        self.dispatch_update_check_items(vec![item]);
    }

    fn queue_update_check_for_linked_mods(&mut self, target_game_id: Option<&str>) {
        self.queue_update_check_for_linked_mods_internal(target_game_id, false);
    }

    fn queue_update_check_for_linked_mods_force(&mut self, target_game_id: Option<&str>) {
        self.queue_update_check_for_linked_mods_internal(target_game_id, true);
    }

    fn queue_update_check_for_linked_mods_internal(&mut self, target_game_id: Option<&str>, force: bool) {
        if self.update_check_inflight {
            self.pending_update_check_game = target_game_id.map(|id| id.to_string());
            return;
        }
        self.pending_update_check_game = None;
        
        const UPDATE_CHECK_COOLDOWN_SECS: i64 = 1800;
        let now = chrono::Utc::now();

        // Automatic checks are throttled per game. The schedule is persisted so
        // app startup cannot bypass the cooldown.
        if !force {
            let schedule_key = target_game_id.unwrap_or_default();
            if self.state.last_update_check_time_by_game.get(schedule_key).is_some_and(|last_check| {
                now.signed_duration_since(*last_check).num_seconds() < UPDATE_CHECK_COOLDOWN_SECS
            }) {
                return;
            }
        }
        let mut items = Vec::with_capacity(self.state.mods.len());
        let update_check_statuses = self.state.static_prefs.update_check_statuses;
        let modified_update_behavior = self.state.static_prefs.modified_update_behavior;
        let mut state_changed_without_fetch = false;
        for mod_entry in &mut self.state.mods {
            if let Some(id) = target_game_id {
                if mod_entry.game_id != id { continue; }
            }
            if mod_entry
                .source
                .as_ref()
                .and_then(|source| source.gamebanana.as_ref())
                .is_none()
            {
                continue;
            }
            if !Self::status_target_enabled(&mod_entry.status, update_check_statuses) {
                if mod_entry.update_state != ModUpdateState::CheckSkipped {
                    mod_entry.update_state = ModUpdateState::CheckSkipped;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            let Some(source) = &mod_entry.source else {
                continue;
            };
            let Some(link) = &source.gamebanana else {
                continue;
            };
            // Automatic checks leave MissingSource mods alone (a genuinely deleted
            // mod would fail the same way forever), but an explicit Reload must be
            // able to recover mods that were marked Missing by a bad API response.
            if !force && !should_check_update_state(mod_entry.update_state) {
                continue;
            }
            if !force && source.update_check_retry_after.is_some_and(|retry_after| retry_after > now) {
                continue;
            }
            if source.ignore_update_always {
                if mod_entry.update_state != ModUpdateState::IgnoringUpdateAlways {
                    mod_entry.update_state = ModUpdateState::IgnoringUpdateAlways;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            if modified_update_behavior == ModifiedUpdateBehavior::HideButton
                && mod_has_local_changes_for_update_check(mod_entry)
            {
                if mod_entry.update_state != ModUpdateState::ModifiedLocally {
                    mod_entry.update_state = ModUpdateState::ModifiedLocally;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            // Prefer the exact GameBanana file(s) this mod was installed from.
            // Fall back to the profile snapshot timestamp for older metadata.
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or_else(|| source.snapshot.as_ref().and_then(|s| s.update_ts))
                .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));

            items.push((
                mod_entry.id.clone(),
                mod_entry.game_id.clone(),
                link.mod_id,
                local_sync_ts,
                source.file_set.clone(),
                gamebanana::is_tool_url(&link.url),
            ));
        }
        if state_changed_without_fetch {
            self.save_state();
        }
        self.dispatch_update_check_items(items);
    }

    fn consume_update_check_results(&mut self) {
        while let Ok(result) = self.update_check_rx.try_recv() {
            if result.generation != self.update_check_generation {
                continue;
            }
            self.update_check_inflight = false;
            let checked_game_ids: HashSet<String> = self.update_check_active_items
                .iter()
                .map(|(_, game_id, _, _, _, _)| game_id.clone())
                .collect();
            self.update_check_active_items.clear();
            let mut warn_lines: Vec<String> = Vec::new();
            let mut auto_update_ids: Vec<String> = Vec::new();
            let active_update_tasks: HashSet<(String, String)> = self
                .state
                .tasks
                .iter()
                .filter(|task| {
                    matches!(
                        task.status,
                        TaskStatus::Queued
                            | TaskStatus::Downloading
                            | TaskStatus::Installing
                            | TaskStatus::Canceling
                    )
                })
                .filter_map(|task| {
                    task.game_id
                        .as_ref()
                        .map(|game_id| (task.title.clone(), game_id.clone()))
                })
                .collect();
            let text = self.text();
            for (mod_id, state, snapshot, err, raw_json, profile) in result.states {
                let mut mod_updated = false;
                let mut retry_schedule_changed = false;
                let mut should_sync_images = false;
                let mut sync_profile: Option<Box<gamebanana::ProfileResponse>> = None;
                let has_pending_update_finalization =
                    self.pending_update_finalization_for_mod(&mod_id);
                let fetch_failed = err.is_some()
                    && snapshot.is_none()
                    && raw_json.is_none()
                    && profile.is_none();
                if let Some(mod_entry) = self.state.mods.iter_mut().find(|m| m.id == mod_id) {
                    let old_preview_urls = mod_entry
                        .source
                        .as_ref()
                        .and_then(|s| s.snapshot.as_ref())
                        .map(|s| s.preview_urls.clone())
                        .unwrap_or_default();
                    let has_local_changes = mod_entry
                        .source
                        .as_ref()
                        .is_some_and(|source| {
                            source.baseline_content_mtime.map(|t| t.timestamp()) != mod_entry.content_mtime.map(|t| t.timestamp())
                                || source.baseline_ini_hash != mod_entry.ini_hash
                        });
                    if fetch_failed && !has_local_changes {
                        warn_lines.push(format!(
                            "{} (update check failed; keeping previous state: {})",
                            mod_entry.folder_name,
                            err.as_deref().unwrap_or("unknown error"),
                        ));
                    }
                    mod_entry.unsafe_content = snapshot
                        .as_ref()
                        .map_or(mod_entry.unsafe_content, |s| s.unsafe_content);
                    if let Some(snap) = snapshot.as_ref() {
                        should_sync_images = old_preview_urls != snap.preview_urls
                            || Self::is_missing_expected_source_images(mod_entry, snap);
                    }
                    if let Some(source) = mod_entry.source.as_mut() {
                        let retry_after = fetch_failed
                            .then(|| chrono::Utc::now() + chrono::Duration::minutes(30));
                        retry_schedule_changed = source.update_check_retry_after != retry_after;
                        source.update_check_retry_after = retry_after;
                        if let Some(profile) = profile.as_deref() {
                            let _ = backfill_selected_files_meta(&mut source.file_set, profile);
                            let _ = backfill_tracked_file_labels(&mut source.file_set, profile);
                        }
                        if let Some(s) = snapshot {
                            source.snapshot = Some(s);
                        }
                        if let Some(raw) = raw_json {
                            source.raw_profile_json = Some(raw);
                        }
                        let raw_state = if has_local_changes {
                            ModUpdateState::ModifiedLocally
                        } else {
                            state
                        };
                        if !fetch_failed {
                            mod_entry.update_state =
                                apply_ignored_update_override(source, raw_state, profile.as_deref());
                        } else if has_local_changes {
                            mod_entry.update_state = ModUpdateState::ModifiedLocally;
                        }
                        if let Some(message) = err {
                            if !fetch_failed {
                                warn_lines.push(format!("{} ({message})", mod_entry.folder_name));
                            }
                        }
                    }
                    if !fetch_failed || has_local_changes || retry_schedule_changed {
                        let _ = xxmi::save_mod_metadata(mod_entry);
                        mod_updated = true;
                    }
                    if should_sync_images {
                        sync_profile = profile;
                    }
                    let modified_update_available = Self::has_modified_update_available(mod_entry);
                    let auto_update_allowed = mod_entry.update_state == ModUpdateState::UpdateAvailable
                        || (self.state.static_prefs.modified_update_behavior == ModifiedUpdateBehavior::Yes
                            && modified_update_available);
                    let should_auto_apply = !fetch_failed
                        && auto_update_allowed
                        && !has_pending_update_finalization
                        && Self::status_target_enabled(&mod_entry.status, self.state.static_prefs.auto_update_statuses)
                        && !active_update_tasks.contains(&(
                            text.updating_task(
                                mod_entry
                                    .metadata
                                    .user
                                    .title
                                    .as_ref()
                                    .unwrap_or(&mod_entry.folder_name),
                            ),
                            mod_entry.game_id.clone(),
                        ));
                    if should_auto_apply {
                        auto_update_ids.push(mod_entry.id.clone());
                    }
                }
                if mod_updated && should_sync_images {
                    if let Some(p) = sync_profile {
                        if let Some(mod_root_path) = self
                            .state
                            .mods
                            .iter()
                            .find(|m| m.id == mod_id)
                            .map(|m| m.root_path.clone())
                        {
                            let job_id = self.next_background_job_id();
                            let _ = self.install_request_tx.send(InstallRequest::SyncImages {
                                job_id,
                                mod_entry_id: mod_id.clone(),
                                mod_root_path,
                                profile: p,
                            });
                        }
                    } else {
                        self.enqueue_mod_image_sync(&mod_id);
                    }
                }
            }
            // A completed manual or automatic request resets the automatic
            // cooldown. This is deliberately recorded only after the worker
            // returns, never when a request is merely queued.
            let completed_at = chrono::Utc::now();
            for game_id in checked_game_ids {
                self.state.last_update_check_time_by_game.insert(game_id, completed_at);
            }
            for line in warn_lines {
                self.log_warn(format!("update check: {line}"));
            }
            self.save_state();
            for mod_id in auto_update_ids {
                self.queue_update_apply(&mod_id);
            }
            if let Some(game_id) = self.pending_update_check_game.take() {
                self.queue_update_check_for_linked_mods(Some(&game_id));
            } else if !self.pending_update_check_mods.is_empty() {
                let pending_ids: Vec<_> = self.pending_update_check_mods.drain().collect();
                let items: Vec<_> = pending_ids
                    .into_iter()
                    .filter_map(|mod_id| self.update_check_item_for_mod(&mod_id))
                    .collect();
                self.dispatch_update_check_items(items);
            }
        }
    }

    fn status_target_enabled(status: &ModStatus, targets: ModStatusTargets) -> bool {
        match status {
            ModStatus::Active => targets.active,
            ModStatus::Disabled => targets.disabled,
            ModStatus::Archived => targets.archived,
        }
    }

    fn should_show_local_change_update_prefs(mod_entry: &ModEntry) -> bool {
        matches!(mod_entry.update_state, ModUpdateState::ModifiedLocally)
    }

    fn pending_update_finalization_for_mod(&self, mod_entry_id: &str) -> bool {
        self.pending_install_finalize.values().any(|pending| {
            pending
                .pending_meta
                .as_ref()
                .and_then(|meta| meta.update_target_mod_id.as_deref())
                == Some(mod_entry_id)
        })
    }

    fn should_auto_replace_update(&self, job_id: u64) -> bool {
        if self.state.static_prefs.always_replace_on_update {
            return true;
        }
        let _ = job_id;
        false
    }

    fn configured_existing_target_choice(&self) -> Option<ConflictChoice> {
        match self.state.static_prefs.import_resolution {
            ImportResolution::Ask => None,
            ImportResolution::Replace => Some(ConflictChoice::Replace),
            ImportResolution::Merge => Some(ConflictChoice::Merge),
            ImportResolution::KeepBoth => Some(ConflictChoice::KeepBoth),
        }
    }

    fn resolve_update_existing_target_choice(&self, job_id: u64) -> Option<ConflictChoice> {
        if self.should_auto_replace_update(job_id) {
            Some(ConflictChoice::Replace)
        } else {
            self.configured_existing_target_choice()
        }
    }

    fn consume_startup_scan_events(&mut self) {
        while let Ok(event) = self.startup_scan_rx.try_recv() {
            match event {
                StartupScanEvent::Ready(mods) => {
                    self.state.mods = mods;
                    self.restore_imported_mod_categories(None);
                    // Startup uses its own scan worker instead of the normal refresh paths,
                    // so run the XXMI persistence pass here too. This installs or cleans the
                    // d3dx.ini reload helper before the user launches the selected game.
                    let scan_game_ids: Vec<String> = match self.selected_game() {
                        Some(game) => vec![game.definition.id.clone()],
                        None => self
                            .state
                            .games
                            .iter()
                            .filter(|game| game.is_xxmi())
                            .map(|game| game.definition.id.clone())
                            .collect(),
                    };
                    for game_id in scan_game_ids {
                        self.run_xxmi_persist_scan_pass(&game_id);
                    }
                    self.sync_selection_after_refresh();
                    self.backfill_missing_mod_images(None);
                    self.sync_tools_for_selected_game();
                    self.save_state();
                    self.startup_scan_loading = false;
                    let launch_game_id = self.selected_game().map(|g| g.definition.id.clone());
                    self.queue_update_check_for_linked_mods(launch_game_id.as_deref());
                    self.request_automatic_app_update_check(0.0);
                }
                StartupScanEvent::Failed(error) => {
                    self.startup_scan_loading = false;
                    self.report_error_message(error, None);
                }
            }
        }
    }

    fn consume_startup_path_scan_events(&mut self, ctx: &egui::Context) {
        let mut saw_event = false;
        while let Ok(event) = self.startup_path_scan_rx.try_recv() {
            saw_event = true;
            match event {
                StartupPathScanEvent::Found { kind, path } => {
                    let mut should_save_found_path = false;
                    let game_for_kind = match &kind {
                        StartupPathTargetKind::Game(game_id) => self
                            .state
                            .games
                            .iter()
                            .find(|game| game.definition.id == *game_id)
                            .cloned(),
                        StartupPathTargetKind::Xxmi => None,
                    };
                    if let Some(scan) = self.startup_path_scan.as_mut() {
                        if let Some(status) =
                            scan.statuses.iter_mut().find(|status| status.kind == kind)
                        {
                            if Self::merge_startup_path_candidate(
                                status,
                                game_for_kind.as_ref(),
                                path,
                            ) {
                                should_save_found_path = true;
                            }
                        }
                    }
                    if should_save_found_path && self.apply_startup_found_path_if_missing(ctx, &kind) {
                        self.save_state();
                    }
                }
                StartupPathScanEvent::Finished { stopped } => {
                    if let Some(scan) = self.startup_path_scan.as_mut() {
                        scan.stopped = stopped;
                        scan.finished = true;
                    }
                }
            }
        }

        if saw_event || self.startup_path_scan.is_some() {
            ctx.request_repaint();
        }
    }

    fn merge_startup_path_candidate(
        status: &mut StartupPathScanStatus,
        game: Option<&GameInstall>,
        path: PathBuf,
    ) -> bool {
        if status.candidates.iter().any(|candidate| candidate == &path) {
            return false;
        }

        let previous_selected = status.selected_candidate.clone();
        if let Some(game) = game.filter(|game| game.is_unreal_engine()) {
            if let Some(key) = Self::startup_unreal_candidate_key(game, &path) {
                if let Some(existing_index) = status.candidates.iter().position(|candidate| {
                    Self::startup_unreal_candidate_key(game, candidate).as_ref() == Some(&key)
                }) {
                    let existing = status.candidates[existing_index].clone();
                    if Self::startup_unreal_candidate_is_better(game, &path, &existing) {
                        status.candidates[existing_index] = path.clone();
                        if status
                            .selected_candidate
                            .as_ref()
                            .is_none_or(|selected| selected == &existing)
                        {
                            status.selected_candidate = Some(path);
                        }
                    }
                    return status.selected_candidate != previous_selected;
                }
            }
        }

        status.candidates.push(path);
        if status.selected_candidate.is_none() {
            status.selected_candidate = status.candidates.first().cloned();
        }
        status.selected_candidate != previous_selected
    }

    fn startup_unreal_candidate_key(game: &GameInstall, path: &Path) -> Option<String> {
        default_unreal_pak_mods_path_from_exe(&game.definition.id, path)
            .map(|path| Self::normalized_startup_path_key(&path))
    }

    fn normalized_startup_path_key(path: &Path) -> String {
        path.display()
            .to_string()
            .replace('/', "\\")
            .to_ascii_lowercase()
    }

    fn startup_unreal_candidate_is_better(
        game: &GameInstall,
        candidate: &Path,
        current: &Path,
    ) -> bool {
        Self::startup_unreal_candidate_score(game, candidate)
            < Self::startup_unreal_candidate_score(game, current)
    }

    fn startup_unreal_candidate_score(game: &GameInstall, path: &Path) -> u8 {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match game.definition.id.as_str() {
            "nte" => match file_name.as_str() {
                "htgame.exe" => 0,
                "ht-win64-shipping.exe" => 1,
                "nteglobalgame.exe" | "ntegame.exe" => 2,
                "nevernesstoeverness.exe" | "neverness to everness.exe" | "nte.exe"
                | "ht.exe" => 3,
                name if name.contains("launcher") => 4,
                _ => 5,
            },
            _ => {
                if file_name.contains("shipping") {
                    0
                } else if file_name.contains("launcher") {
                    3
                } else {
                    1
                }
            }
        }
    }

    fn apply_startup_found_path_if_missing(
        &mut self,
        ctx: &egui::Context,
        kind: &StartupPathTargetKind,
    ) -> bool {
        let Some(path) = self
            .startup_path_scan
            .as_ref()
            .and_then(|scan| {
                scan.statuses
                    .iter()
                    .find(|status| &status.kind == kind)
                    .and_then(|status| status.selected_candidate.clone())
            })
        else {
            return false;
        };

        let mut changed = false;
        match kind {
            StartupPathTargetKind::Xxmi => {
                if self
                    .state
                    .static_prefs.modded_launcher_path_override
                    .as_ref()
                    .is_none_or(|path| !path.is_file())
                {
                    self.state.static_prefs.modded_launcher_path_override = Some(path.clone());
                    changed = true;
                }
                for game in &mut self.state.games {
                    if !game.is_xxmi() {
                        continue;
                    }
                    if game
                        .modded_exe_path_override
                        .as_ref()
                        .is_none_or(|path| !path.is_file())
                    {
                        game.modded_exe_path_override = Some(path.clone());
                        changed = true;
                    }
                    if game
                        .mods_path_override
                        .as_ref()
                        .is_none_or(|path| !path.is_dir())
                    {
                        if let Some(mods_path) =
                            default_mods_path_from_launcher(&path, &game.definition.xxmi_code)
                        {
                            game.mods_path_override = Some(mods_path);
                            changed = true;
                        }
                    }
                }
                if changed {
                    ctx.data_mut(|data| {
                        data.remove::<String>(egui::Id::new("launcher_path_input"));
                        for game in &self.state.games {
                            data.remove::<String>(egui::Id::new((
                                "settings_mods_path",
                                game.definition.id.as_str(),
                            )));
                        }
                    });
                }
            }
            StartupPathTargetKind::Game(game_id) => {
                if let Some(game) = self
                    .state
                    .games
                    .iter_mut()
                    .find(|game| game.definition.id == *game_id)
                {
                    if game
                        .vanilla_exe_path_override
                        .as_ref()
                        .is_none_or(|path| !path.is_file())
                    {
                        game.vanilla_exe_path_override = Some(path);
                        if game.is_unreal_engine()
                            && game.mods_path_override.as_ref().is_none_or(|path| !path.is_dir())
                        {
                            game.mods_path_override = game
                                .vanilla_exe_path_override
                                .as_ref()
                                .and_then(|path| default_unreal_pak_mods_path_from_exe(&game.definition.id, path));
                        }
                        game.enabled = true;
                        changed = true;
                        ctx.data_mut(|data| {
                            data.remove::<String>(egui::Id::new((
                                "settings_vanilla_path",
                                game.definition.id.as_str(),
                            )));
                        });
                    }
                }
            }
        }
        changed
    }

    fn finish_startup_path_scan(&mut self, ctx: &egui::Context) {
        let stopped = self
            .startup_path_scan
            .as_ref()
            .is_some_and(|scan| scan.stopped);
        self.apply_startup_path_scan_results(ctx, !stopped);
        let run_initial_mod_scan_after = self
            .startup_path_scan
            .as_ref()
            .is_some_and(|scan| scan.run_initial_mod_scan_after);
        self.state.startup_path_scan_completed = true;
        self.save_state();
        self.ensure_selected_game_enabled(ctx);
        self.startup_path_scan = None;
        if run_initial_mod_scan_after {
            self.dispatch_startup_mod_scan();
        }
    }

    fn apply_startup_path_scan_results(&mut self, ctx: &egui::Context, allow_fallback: bool) {
        let Some(scan) = self.startup_path_scan.as_ref() else {
            return;
        };
        let paths_to_apply = scan
            .statuses
            .iter()
            .filter_map(|status| {
                let path = status
                    .selected_candidate
                    .clone()
                    .or_else(|| allow_fallback.then(|| status.candidates.first().cloned()).flatten())?;
                Some((status.kind.clone(), path))
            })
            .collect::<Vec<_>>();
        for (kind, path) in paths_to_apply {
            match kind {
                StartupPathTargetKind::Xxmi => {
                    self.state.static_prefs.modded_launcher_path_override = Some(path.clone());
                    for game in &mut self.state.games {
                        if !game.is_xxmi() {
                            continue;
                        }
                        game.modded_exe_path_override = Some(path.clone());
                        game.mods_path_override =
                            default_mods_path_from_launcher(&path, &game.definition.xxmi_code);
                    }
                    let mods_dir_error = self
                        .state
                        .games
                        .iter()
                        .filter(|game| {
                            game.enabled
                                && game.is_xxmi()
                                && game
                                    .vanilla_exe_path_override
                                    .as_ref()
                                    .is_some_and(|path| path.is_file())
                        })
                        .filter_map(|game| {
                            let mods_path = game.mods_path(self.state.static_prefs.use_default_mods_path)?;
                            fs::create_dir_all(&mods_path)
                                .err()
                                .map(|err| (mods_path, err))
                        })
                        .next();
                    ctx.data_mut(|data| {
                        data.remove::<String>(egui::Id::new("launcher_path_input"));
                        for game in &self.state.games {
                            data.remove::<String>(egui::Id::new((
                                "settings_mods_path",
                                game.definition.id.as_str(),
                            )));
                        }
                    });
                    if let Some((mods_path, err)) = mods_dir_error {
                        self.report_error_message(
                            format!("failed to create mod directory: {}: {err}", mods_path.display()),
                            Some(self.text().could_not_create_mods_folder()),
                        );
                    }
                }
                StartupPathTargetKind::Game(game_id) => {
                    if let Some(game) = self
                        .state
                        .games
                        .iter_mut()
                        .find(|game| game.definition.id == game_id)
                    {
                        game.vanilla_exe_path_override = Some(path);
                        if game.is_unreal_engine()
                            && game.mods_path_override.as_ref().is_none_or(|path| !path.is_dir())
                        {
                            game.mods_path_override = game
                                .vanilla_exe_path_override
                                .as_ref()
                                .and_then(|path| default_unreal_pak_mods_path_from_exe(&game.definition.id, path));
                        }
                        game.enabled = true;
                        ctx.data_mut(|data| {
                            data.remove::<String>(egui::Id::new((
                                "settings_vanilla_path",
                                game.definition.id.as_str(),
                            )));
                        });
                    }
                }
            }
        }
    }

    fn queue_game_refresh(&mut self, game_id: String) {
        if self.refresh_inflight {
            self.refresh_pending_selected_game = Some(game_id);
            return;
        }
        self.dispatch_selected_game_refresh(game_id);
    }

    fn dispatch_selected_game_refresh(&mut self, game_id: String) {
        let existing_mods = self
            .state
            .mods
            .iter()
            .filter(|mod_entry| mod_entry.game_id == game_id)
            .cloned()
            .collect();
        let request = RefreshRequest {
            game_id: game_id.clone(),
            games: self.state.games.clone(),
            use_default_mods_path: self.state.static_prefs.use_default_mods_path,
            existing_mods,
        };
        if self.refresh_request_tx.send(request).is_ok() {
            self.refresh_inflight = true;
        } else {
            self.refresh_inflight = false;
            self.report_error_message(
                format!("failed to queue selected-game refresh for {game_id}"),
                None,
            );
        }
    }

    fn consume_refresh_events(&mut self) {
        while let Ok(event) = self.refresh_result_rx.try_recv() {
            self.refresh_inflight = false;
            match event {
                RefreshEvent::Ready { game_id, mods } => {
                    let reload_before = self
                        .pending_reload_summary
                        .as_ref()
                        .is_some_and(|(reload_game_id, _)| reload_game_id == &game_id)
                        .then(|| {
                            self.pending_reload_summary
                                .take()
                                .map(|(_, snapshots)| snapshots)
                        })
                        .flatten();
                    let is_current = self
                        .selected_game()
                        .is_some_and(|g| g.definition.id == game_id);
                    let old_ts: HashMap<String, DateTime<Utc>> = self.state.mods.iter()
                        .map(|m| (m.id.clone(), m.updated_at))
                        .collect();
                    self.state.mods.retain(|m| m.game_id != game_id);
                    self.state.mods.extend(mods);
                    self.state.mods.sort_by(|a, b| {
                        a.game_id.cmp(&b.game_id).then_with(|| {
                            a.folder_name
                                .to_lowercase()
                                .cmp(&b.folder_name.to_lowercase())
                        })
                    });
                    self.restore_imported_mod_categories(Some(&game_id));
                    self.run_xxmi_persist_scan_pass(&game_id);
                    if is_current {
                        self.invalidate_stale_mod_textures(&old_ts);
                        self.sync_selection_after_refresh();
                        self.backfill_missing_mod_images(Some(&game_id));
                        self.sync_tools_for_selected_game();
                    }
                    let finalized_install =
                        self.resolve_pending_install_finalization_for_game(&game_id);
                    self.save_state();
                    if reload_before.is_some() {
                        self.queue_update_check_for_linked_mods_force(Some(&game_id));
                    } else if !finalized_install {
                        self.queue_update_check_for_linked_mods(Some(&game_id));
                    }
                    if let Some(before) = reload_before {
                        let after = self.capture_reload_snapshots(Some(&game_id));
                        let summary = self.build_reload_summary(&before, &after);
                        self.push_log(
                            self.text()
                                .reload_action(&self.reload_summary_log_text(&summary)),
                        );
                        for line in &summary.detail_lines {
                            self.push_log(self.text().reload_action(&line));
                        }
                        self.set_message_ok(self.reload_summary_toast_text(&summary));
                        self.request_automatic_app_update_check(0.0);
                    }
                }
                RefreshEvent::Failed { game_id, error } => {
                    if self
                        .pending_reload_summary
                        .as_ref()
                        .is_some_and(|(reload_game_id, _)| reload_game_id == &game_id)
                    {
                        self.pending_reload_summary = None;
                    }
                    let is_current = self
                        .selected_game()
                        .is_some_and(|g| g.definition.id == game_id);
                    if is_current {
                    self.report_error_message(
                        format!("selected-game refresh failed for {game_id}: {error}"),
                        Some(self.text().could_not_refresh_mods()),
                    );
                }
                }
            }
            if let Some(next_game_id) = self.refresh_pending_selected_game.take() {
                self.dispatch_selected_game_refresh(next_game_id);
            }
        }
    }

    /// The install worker and the disk scanner build these paths independently,
    /// so separator style, verbatim prefixes, and casing can differ for the same
    /// location on Windows. Exact `PathBuf` equality would then leave a freshly
    /// installed mod unmatched here — and therefore permanently Unlinked, since
    /// this match is what carries the GameBanana metadata onto the new entry.
    fn install_path_matches_mod_root(installed: &Path, mod_root: &Path) -> bool {
        if installed == mod_root {
            return true;
        }
        let normalized_components = |path: &Path| -> Vec<String> {
            path.components()
                .map(|component| {
                    let text = component
                        .as_os_str()
                        .to_string_lossy()
                        .to_ascii_lowercase();
                    text.strip_prefix(r"\\?\")
                        .map(str::to_string)
                        .unwrap_or(text)
                })
                .collect()
        };
        normalized_components(installed) == normalized_components(mod_root)
    }

    fn resolve_pending_install_finalization_for_game(&mut self, game_id: &str) -> bool {
        let mut finalized_any = false;
        let job_ids: Vec<u64> = self.pending_install_finalize.keys().copied().collect();
        for job_id in job_ids {
            let Some(payload) = self.pending_install_finalize.get(&job_id).cloned() else {
                continue;
            };
            let belongs_to_game = payload.installed_paths.iter().any(|path| {
                self.state
                    .mods
                    .iter()
                    .find(|m| Self::install_path_matches_mod_root(path, &m.root_path))
                    .is_some_and(|m| m.game_id == game_id)
            });
            if !belongs_to_game {
                continue;
            }
            let _ = self.pending_install_finalize.remove(&job_id);
            self.finalize_install_after_refresh(job_id, payload);
            finalized_any = true;
        }
        finalized_any
    }

    fn finalize_install_after_refresh(&mut self, _job_id: u64, payload: PendingInstallFinalize) {
        let PendingInstallFinalize {
            installed_paths,
            installed_candidate_labels,
            gb_profile,
            rel_paths,
            pending_meta,
            pending_unsafe,
            install_disabled: local_install_disabled,
        } = payload;
        let post_install_rename = pending_meta
            .as_ref()
            .and_then(|meta| {
                meta.update_target_mod_id
                    .as_ref()
                    .zip(meta.post_install_rename_to.as_ref())
                    .map(|(mod_id, name)| (mod_id.clone(), name.clone()))
            });
        for path in &installed_paths {
            self.pending_known_installed_paths.remove(path);
        }
        let mut first_mod_name = String::new();
        let mut primary_id = None;
        let mut newly_installed_ids = Vec::with_capacity(installed_paths.len());

        for (i, path) in installed_paths.iter().enumerate() {
            if let Some(mod_entry) = self
                .state
                .mods
                .iter_mut()
                .find(|m| Self::install_path_matches_mod_root(path, &m.root_path))
            {
                if i == 0 {
                    first_mod_name = mod_entry.folder_name.clone();
                    primary_id = Some(mod_entry.id.clone());
                }
                if pending_unsafe {
                    mod_entry.unsafe_content = true;
                }
                if pending_meta.is_none() {
                    let backend = self
                        .state
                        .games
                        .iter()
                        .find(|game| game.definition.id == mod_entry.game_id)
                        .map(|game| game.definition.backend)
                        .unwrap_or_default();
                    match backend {
                        GameBackend::Xxmi => {
                            let _ = xxmi::save_mod_metadata(mod_entry);
                        }
                        GameBackend::UnrealEngine => {
                            let _ = unrealengine::write_portable_metadata(mod_entry);
                        }
                    }
                }
                newly_installed_ids.push(mod_entry.id.clone());
            }
        }

        let install_disabled = pending_meta
            .as_ref()
            .is_some_and(|meta| meta.install_disabled)
            || local_install_disabled;
        if install_disabled {
            for mod_id in &newly_installed_ids {
                let game = self
                    .state
                    .mods
                    .iter()
                    .find(|m| m.id == *mod_id)
                    .and_then(|m| {
                        self.state
                            .games
                            .iter()
                            .find(|game| game.definition.id == m.game_id)
                            .cloned()
                    });
                let use_default = self.state.static_prefs.use_default_mods_path;
                let (result, name) = if let Some(mod_entry) = self
                    .state
                    .mods
                    .iter()
                    .find(|m| m.id == *mod_id && m.status == ModStatus::Active)
                {
                    let name = mod_entry.folder_name.clone();
                    if self
                        .mod_action_lock_reason(mod_entry, ModMutationKind::DisableActive)
                        .is_some()
                    {
                        (Some(Err(anyhow!(self.text().mods_locked_probably_by_game()))), Some(name))
                    } else {
                        let result = match game.as_ref().map(|game| game.definition.backend) {
                            Some(GameBackend::Xxmi) => {
                                let mut ptx = game
                                    .as_ref()
                                    .and_then(|game| self.begin_xxmi_persist_tx(game));
                                let result = self
                                    .state
                                    .mods
                                    .iter_mut()
                                    .find(|m| m.id == *mod_id && m.status == ModStatus::Active)
                                    .map(|mod_entry| {
                                        Self::persisted_xxmi_disable(&mut ptx, mod_entry)
                                    })
                                    .unwrap_or_else(|| Err(anyhow!("mod not found")));
                                if let Some(game) = game.clone() {
                                    let request_reload =
                                        result.is_ok().then_some(ReloadHotkeyTrigger::UpdatingMods);
                                    self.finish_xxmi_persist_tx(&game, ptx, request_reload);
                                }
                                result
                            }
                            Some(GameBackend::UnrealEngine) => self
                                .state
                                .mods
                                .iter_mut()
                                .find(|m| m.id == *mod_id && m.status == ModStatus::Active)
                                .map(|mod_entry| {
                                    unrealengine::disable_mod(
                                        mod_entry,
                                        game.as_ref().expect("game checked"),
                                        use_default,
                                    )
                                })
                                .unwrap_or_else(|| Err(anyhow!("mod not found"))),
                            None => Err(anyhow!("game not found")),
                        };
                        (Some(result), Some(name))
                    }
                } else {
                    (None, None)
                };
                if let (Some(Err(err)), Some(name)) = (result, name) {
                    let toast = self.mod_action_error_toast(
                        &err,
                        self.text().could_not_disable_installed_mod(),
                    );
                    self.report_error_message(
                        format!("installed mod could not be disabled for {name}: {err:#}"),
                        Some(toast),
                    );
                }
            }
        } else if pending_meta
            .as_ref()
            .is_some_and(|meta| meta.update_target_was_disabled)
        {
            if let Some(target_mod_id) = pending_meta
                .as_ref()
                .and_then(|meta| meta.update_target_mod_id.as_deref())
            {
                let game = self
                    .state
                    .mods
                    .iter()
                    .find(|m| m.id == target_mod_id)
                    .and_then(|m| {
                        self.state
                            .games
                            .iter()
                            .find(|game| game.definition.id == m.game_id)
                            .cloned()
                    });
                let use_default = self.state.static_prefs.use_default_mods_path;
                let (result, name) = if newly_installed_ids.iter().any(|id| id == target_mod_id) {
                    if let Some(mod_entry) = self
                        .state
                        .mods
                        .iter()
                        .find(|m| m.id == target_mod_id && m.status == ModStatus::Active)
                    {
                        let name = mod_entry.folder_name.clone();
                        if self
                            .mod_action_lock_reason(mod_entry, ModMutationKind::DisableActive)
                            .is_some()
                        {
                            (Some(Err(anyhow!(self.text().mods_locked_probably_by_game()))), Some(name))
                        } else {
                            let result = match game.as_ref().map(|game| game.definition.backend) {
                                Some(GameBackend::Xxmi) => {
                                    let mut ptx = game
                                        .as_ref()
                                        .and_then(|game| self.begin_xxmi_persist_tx(game));
                                    let result = self
                                        .state
                                        .mods
                                        .iter_mut()
                                        .find(|m| {
                                            m.id == target_mod_id && m.status == ModStatus::Active
                                        })
                                        .map(|mod_entry| {
                                            Self::persisted_xxmi_disable(&mut ptx, mod_entry)
                                        })
                                        .unwrap_or_else(|| Err(anyhow!("mod not found")));
                                    if let Some(game) = game.clone() {
                                        let request_reload = result
                                            .is_ok()
                                            .then_some(ReloadHotkeyTrigger::UpdatingMods);
                                        self.finish_xxmi_persist_tx(&game, ptx, request_reload);
                                    }
                                    result
                                }
                                Some(GameBackend::UnrealEngine) => self
                                    .state
                                    .mods
                                    .iter_mut()
                                    .find(|m| m.id == target_mod_id && m.status == ModStatus::Active)
                                    .map(|mod_entry| {
                                        unrealengine::disable_mod(
                                            mod_entry,
                                            game.as_ref().expect("game checked"),
                                            use_default,
                                        )
                                    })
                                    .unwrap_or_else(|| Err(anyhow!("mod not found"))),
                                None => Err(anyhow!("game not found")),
                            };
                            (Some(result), Some(name))
                        }
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
                if let (Some(Err(err)), Some(name)) = (result, name) {
                    let toast = self.mod_action_error_toast(
                        &err,
                        self.text().could_not_keep_mod_disabled(),
                    );
                    self.report_error_message(
                        format!("updated mod could not be kept disabled for {name}: {err:#}"),
                        Some(toast),
                    );
                }
            }
        }

        for id in &newly_installed_ids {
            let candidate_labels = self
                .state
                .mods
                .iter()
                .find(|m| m.id == *id)
                .map(|m| {
                    installed_candidate_labels
                        .iter()
                        .filter(|(path, _)| Self::install_path_matches_mod_root(path, &m.root_path))
                        .map(|(_, label)| label.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            self.apply_sync_metadata(
                id,
                pending_meta.clone(),
                gb_profile.clone(),
                rel_paths.clone(),
                candidate_labels,
            );
        }
        for id in &newly_installed_ids {
            self.apply_browse_download_category(
                id,
                pending_meta.as_ref(),
                gb_profile.as_deref(),
            );
        }
        if let Some((target_mod_id, rename_to)) = post_install_rename {
            match self.rename_mod_folder(&target_mod_id, &rename_to) {
                Ok(()) => {
                    self.log_action(self.text().action_renamed(), &rename_to);
                    self.save_state();
                    if let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == target_mod_id) {
                        first_mod_name = mod_entry.folder_name.clone();
                    }
                }
                Err(err) => {
                    self.report_warn(
                        format!("post-install rename failed: {err:#}"),
                        Some(self.text().rename_failed()),
                    );
                }
            }
        }
        self.run_xxmi_persist_import_restore(&newly_installed_ids);
        let install_reload_trigger = if pending_meta
            .as_ref()
            .and_then(|meta| meta.update_target_mod_id.as_ref())
            .is_some()
        {
            ReloadHotkeyTrigger::UpdatingMods
        } else {
            ReloadHotkeyTrigger::InstallingMods
        };
        self.request_xxmi_reload_for_live_mod_ids(&newly_installed_ids, install_reload_trigger);

        if let (Some(profile), Some(first_path)) = (gb_profile.clone(), installed_paths.first()) {
            let mod_id = pending_meta
                .as_ref()
                .and_then(|meta| meta.update_target_mod_id.clone())
                .or_else(|| {
                    self.state
                        .mods
                        .iter()
                        .find(|m| m.root_path == *first_path)
                        .map(|m| m.id.clone())
                });
            if let Some(mod_entry_id) = mod_id {
                let image_job_id = self.next_background_job_id();
                let mod_root_path = self
                    .state
                    .mods
                    .iter()
                    .find(|m| m.id == mod_entry_id)
                    .map(|m| m.root_path.clone())
                    .unwrap_or_else(|| first_path.clone());
                let _ = self.install_request_tx.send(InstallRequest::SyncImages {
                    job_id: image_job_id,
                    mod_entry_id,
                    mod_root_path,
                    profile,
                });
            }
        }

        if let Some(id) = primary_id {
            match self.state.static_prefs.after_install_behavior {
                AfterInstallBehavior::DoNothing => {}
                AfterInstallBehavior::AddToSelection => {
                    self.selected_mods.insert(id.clone());
                }
                AfterInstallBehavior::OpenModDetail => {
                    self.set_selected_mod_id(Some(id.clone()));
                }
            }
            let count = installed_paths.len();
            if count > 1 {
                let text = self.text();
                self.log_action(text.installed_action(), &text.library_mods_count(count));
                self.set_message_ok(text.installed_count(count));
            } else if !first_mod_name.is_empty() {
                let text = self.text();
                self.log_action(text.installed_action(), &first_mod_name);
                self.set_message_ok(text.installed_name(&first_mod_name));
            }
        } else if let Some(first_path) = installed_paths.first() {
            let fallback_name = first_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("mod");
            let text = self.text();
            self.log_action(text.installed_action(), fallback_name);
            self.set_message_ok(text.installed_name(fallback_name));
        }
    }

    fn apply_browse_download_category(
        &mut self,
        mod_entry_id: &str,
        pending_meta: Option<&PendingBrowseInstallMeta>,
        gb_profile: Option<&gamebanana::ProfileResponse>,
    ) {
        let Some(meta) = pending_meta else { return; };
        if meta.update_target_mod_id.is_some() {
            return;
        }

        let enabled = if let Some(enabled) = self
            .state
            .create_downloaded_mod_category_by_game
            .get(&meta.game_id)
            .copied()
        {
            enabled
        } else {
            let game_has_categories = self
                .state
                .categories
                .iter()
                .any(|category| category.game_id == meta.game_id);
            let enabled = !game_has_categories;
            self.state
                .create_downloaded_mod_category_by_game
                .insert(meta.game_id.clone(), enabled);
            self.save_state();
            enabled
        };
        if !enabled {
            return;
        }

        let Some(mod_index) = self
            .state
            .mods
            .iter()
            .position(|mod_entry| mod_entry.id == mod_entry_id && mod_entry.game_id == meta.game_id)
        else {
            return;
        };
        let mod_name = self.state.mods[mod_index]
            .metadata
            .user
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&self.state.mods[mod_index].folder_name)
            .to_string();

        let Some(category_name) = gb_profile.and_then(gamebanana::profile_category_name) else {
            let text = self.text();
            self.log_action(
                text.category_action(),
                &text.category_skipped_no_valid_gamebanana_category(&mod_name),
            );
            return;
        };

        let (category_id, category_name) = if let Some(existing) = self
            .state
            .categories
            .iter()
            .find(|category| {
                category.game_id == meta.game_id
                    && category.name.eq_ignore_ascii_case(category_name.as_str())
            })
        {
            (existing.id.clone(), existing.name.clone())
        } else {
            let category_id = Uuid::new_v4().to_string();
            let order = self
                .state
                .categories
                .iter()
                .filter(|category| category.game_id == meta.game_id)
                .map(|category| category.order)
                .max()
                .unwrap_or(-1)
                + 1;
            self.state.categories.push(ModCategory {
                id: category_id.clone(),
                game_id: meta.game_id.clone(),
                name: category_name.clone(),
                order,
            });
            let text = self.text();
            self.log_action(text.category_action(), &text.category_created(&category_name));
            (category_id, category_name)
        };

        let old_category = self.state.mods[mod_index].metadata.user.category.clone();
        let changed = self.state.mods[mod_index].metadata.user.category_id.as_deref()
            != Some(category_id.as_str())
            || old_category != category_name;
        if changed {
            let mod_entry = &mut self.state.mods[mod_index];
            mod_entry.metadata.user.category_id = Some(category_id);
            mod_entry.metadata.user.category = category_name.clone();
            let _ = xxmi::save_mod_metadata(mod_entry);
            self.log_category_change(&mod_name, &old_category, &category_name);
        }
        self.save_state();
    }

    fn apply_pending_update_source_metadata_before_refresh(
        &mut self,
        pending_meta: Option<&PendingBrowseInstallMeta>,
        gb_profile: Option<&gamebanana::ProfileResponse>,
    ) {
        let Some(meta) = pending_meta else { return; };
        let Some(target_mod_id) = meta.update_target_mod_id.as_deref() else { return; };
        let Some(mod_entry) = self
            .state
            .mods
            .iter_mut()
            .find(|mod_entry| mod_entry.id == target_mod_id && mod_entry.game_id == meta.game_id)
        else {
            return;
        };

        let now = Utc::now();
        let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
        source.gamebanana = Some(GameBananaLink {
            mod_id: meta.mod_id,
            url: gamebanana::browser_url(meta.mod_id),
        });
        source.file_set = FileSetRecipe {
            selected_file_ids: meta.selected_files.iter().map(|file| file.id).collect(),
            selected_file_names: meta
                .selected_files
                .iter()
                .map(|file| file.file_name.clone())
                .collect(),
            selected_files_meta: meta
                .selected_files
                .iter()
                .map(tracked_file_meta_from_mod_file)
                .collect(),
            selected_candidate_labels: Vec::new(),
        };
        source.history.downloaded_at = Some(now);
        source.history.updated_at = Some(now);
        source.ignored_update_signature = None;
        if let Some(profile) = gb_profile {
            source.snapshot = Some(profile_to_snapshot(profile));
            source.raw_profile_json = serde_json::to_string(profile).ok();
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or(profile.date_updated.or(Some(profile.date_modified)));
            let raw_state = determine_file_set_update_state(&source.file_set, local_sync_ts, profile);
            mod_entry.update_state = apply_ignored_update_override(source, raw_state, Some(profile));
        } else {
            mod_entry.update_state = ModUpdateState::UpToDate;
        }
        let _ = xxmi::save_mod_metadata(mod_entry);
        self.save_state();
    }

    fn apply_sync_metadata(
        &mut self,
        mod_entry_id: &str,
        pending_meta: Option<PendingBrowseInstallMeta>,
        gb_profile: Option<Box<gamebanana::ProfileResponse>>,
        rel_paths: Vec<String>,
        selected_candidate_labels: Vec<String>,
    ) {
        let Some(meta) = pending_meta else { return; };

        // Identify all mods sharing this ID to keep them in sync
        let target_indices: Vec<usize> = self.state.mods.iter().enumerate()
            .filter(|(_, m)| m.id == mod_entry_id && m.game_id == meta.game_id)
            .map(|(i, _)| i)
            .collect();

        for idx in target_indices {
            let mod_entry = &mut self.state.mods[idx];
            let now = Utc::now();
            let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
            source.gamebanana = Some(GameBananaLink {
                mod_id: meta.mod_id,
                url: gamebanana::browser_url(meta.mod_id),
            });
            source.file_set = FileSetRecipe {
                selected_file_ids: meta.selected_files.iter().map(|f| f.id).collect(),
                selected_file_names: meta
                    .selected_files
                    .iter()
                    .map(|f| f.file_name.clone())
                    .collect(),
                selected_files_meta: meta
                    .selected_files
                    .iter()
                    .map(tracked_file_meta_from_mod_file)
                    .collect(),
                selected_candidate_labels: selected_candidate_labels.clone(),
            };
            source.history.downloaded_at = Some(now);
            source.history.installed_at = Some(now);
            source.history.updated_at = Some(now);
            source.ignored_update_signature = None;
            source.baseline_content_mtime = mod_entry.content_mtime;
            source.baseline_ini_hash = mod_entry.ini_hash.clone();
            
            let profile_compare = if let Some(p) = gb_profile.as_ref() {
                (**p).clone()
            } else {
                profile_to_response(source.snapshot.as_ref())
            };
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or(profile_compare.date_updated.or(Some(profile_compare.date_modified)));
            let raw_state = determine_file_set_update_state(&source.file_set, local_sync_ts, &profile_compare);
            mod_entry.update_state =
                apply_ignored_update_override(source, raw_state, gb_profile.as_deref().or(Some(&profile_compare)));
            mod_entry.unsafe_content = gb_profile.as_ref().is_some_and(|p| !p.content_ratings.is_empty());

            if let Some(profile) = gb_profile.as_ref() {
                source.snapshot = Some(profile_to_snapshot(profile));
                source.raw_profile_json = serde_json::to_string(profile).ok();
                if !rel_paths.is_empty() {
                    mod_entry.metadata.user.screenshots = rel_paths.clone();
                    if mod_entry.metadata.user.cover_image.as_ref().map_or(true, |s| s.trim().is_empty()) {
                        mod_entry.metadata.user.cover_image = rel_paths.first().cloned();
                    }
                }
            }
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
    }

    fn backfill_missing_mod_images(&mut self, target_game_id: Option<&str>) {
        if let Some(id) = self.selected_mod_id.clone() {
            let needs_sync = self.state.mods.iter().find(|m| m.id == id).map_or(false, |m| {
                if let Some(game_id) = target_game_id {
                    if m.game_id != game_id {
                        return false;
                    }
                }
                m.source.as_ref().is_some_and(|s| s.gamebanana.is_some())
                    && m.metadata.user.screenshots.is_empty()
            });

            if needs_sync {
                self.enqueue_mod_image_sync(&id);
            }
        }
    }

    fn apply_mod_sync_result(
        &mut self,
        mod_entry_id: &str,
        profile: gamebanana::ProfileResponse,
        rel_paths: Vec<String>,
    ) {
        let target_indices: Vec<usize> = self.state.mods.iter().enumerate()
            .filter(|(_, m)| m.id == mod_entry_id)
            .map(|(i, _)| i)
            .collect();

        if target_indices.is_empty() { return; }

        let mut first_folder_name = String::new();
        for (i, idx) in target_indices.into_iter().enumerate() {
            let mod_entry = &mut self.state.mods[idx];
            if i == 0 { first_folder_name = mod_entry.folder_name.clone(); }

            if !rel_paths.is_empty() {
                mod_entry.metadata.user.screenshots = rel_paths.clone();
                if mod_entry
                    .metadata
                    .user
                    .cover_image
                    .as_deref()
                    .map(|s| s.trim().is_empty() || s.contains("gb_"))
                    .unwrap_or(true)
                {
                    mod_entry.metadata.user.cover_image = rel_paths.first().cloned();
                }
            }
            let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
            source.snapshot = Some(profile_to_snapshot(&profile));
            source.raw_profile_json = serde_json::to_string(&profile).ok();
            mod_entry.unsafe_content = !profile.content_ratings.is_empty();
            source.baseline_content_mtime = mod_entry.content_mtime;
            source.baseline_ini_hash = mod_entry.ini_hash.clone();
            let local_sync_ts = profile.date_updated.or(Some(profile.date_modified));
            mod_entry.update_state = determine_update_state(local_sync_ts, &profile);
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
        
        let folder_name = first_folder_name;

        self.save_state();

        let prefix = format!("my-mod-shot-{mod_entry_id}-");
        self.mod_cover_textures.retain(|key, _| key != mod_entry_id && !key.starts_with(&prefix));
        self.mod_full_textures.retain(|key, _| key != mod_entry_id && !key.starts_with(&prefix));
        self.pending_mod_image_requests
            .retain(|key| key != mod_entry_id && !key.starts_with(&prefix));
        self.pending_image_loads
            .retain(|key| key != mod_entry_id && !key.starts_with(&prefix));
        self.inflight_full_image_requests
            .retain(|key| key != mod_entry_id && !key.starts_with(&prefix));
        // A pre-sync failure must not delay the freshly synced cover; the next
        // request should go straight to the worker.
        self.mod_thumb_unavailable.remove(mod_entry_id);
        self.rebuild_texture_tracking();
        self.log_action(self.text().synced_action(), &folder_name);
    }

    /// Early cover publish from an image-sync job: the first preview is on disk
    /// while the rest of the gallery is still downloading. Pointing cover_image
    /// at it now lets the mod card and its category folder tile stop waiting on
    /// the full sync. Screenshots stay untouched so an interrupted sync is still
    /// caught by `backfill_missing_mod_images` (gated on empty screenshots); the
    /// final SyncImagesDone applies the complete result as before.
    fn apply_mod_sync_cover(&mut self, mod_entry_id: &str, cover_rel_path: &str) {
        let mut changed = false;
        for mod_entry in self
            .state
            .mods
            .iter_mut()
            .filter(|m| m.id == mod_entry_id)
        {
            let replaceable = mod_entry
                .metadata
                .user
                .cover_image
                .as_deref()
                .map(|s| s.trim().is_empty() || s.contains("gb_"))
                .unwrap_or(true);
            if !replaceable {
                continue;
            }
            mod_entry.metadata.user.cover_image = Some(cover_rel_path.to_string());
            let _ = xxmi::save_mod_metadata(mod_entry);
            changed = true;
        }
        if !changed {
            return;
        }
        self.save_state();
        // Invalidate unconditionally: on a mod update the rel path is unchanged
        // while the bytes are new, so comparing paths would pin a stale texture.
        self.clear_mod_card_texture(mod_entry_id);
        self.mod_thumb_unavailable.remove(mod_entry_id);
    }

    fn queue_update_apply(&mut self, mod_entry_id: &str) -> bool {
        let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == mod_entry_id).cloned() else { return false; };
        if self
            .mod_action_lock_reason(&mod_entry, ModMutationKind::UpdateExisting)
            .is_some()
        {
            self.report_locked_mods(Some(self.text().update_unavailable()));
            return false;
        }
        let Some(source) = &mod_entry.source else { return false; };
        let Some(link) = &source.gamebanana else { return false; };

        let mod_id = link.mod_id;
        let game_id = mod_entry.game_id.clone();
        if !self.game_can_download_mods(&game_id) {
            self.report_warn(
                self.game_mod_setup_message(&game_id),
                Some(self.text().update_unavailable()),
            );
            return false;
        }
        let title = mod_entry.metadata.user.title.as_ref().unwrap_or(&mod_entry.folder_name).clone();

        let task_id = self.next_background_job_id();
        self.add_task(
            task_id,
            TaskKind::Download,
            TaskStatus::Queued,
            self.text().updating_task(&title),
            Some(game_id.clone()),
            None,
            mod_entry.unsafe_content,
        );

        self.request_browse_detail(mod_id);
        self.resolve_browse_install_after_detail(PendingBrowseInstall {
            task_id,
            mod_id,
            game_id,
            update_target_id: Some(mod_entry_id.to_string()),
            install_disabled: false,
        });
        true
    }

    fn cancel_update_process_for_mod(&mut self, mod_entry: &ModEntry) {
        let title = mod_entry
            .metadata
            .user
            .title
            .as_ref()
            .unwrap_or(&mod_entry.folder_name)
            .clone();
        let task_title = self.text().updating_task(&title);
        let task_ids: Vec<u64> = self
            .state
            .tasks
            .iter()
            .filter(|task| {
                task.title == task_title
                    && task.game_id.as_deref() == Some(mod_entry.game_id.as_str())
                    && matches!(
                        task.status,
                        TaskStatus::Queued
                            | TaskStatus::Downloading
                            | TaskStatus::Installing
                            | TaskStatus::Canceling
                    )
            })
            .map(|task| task.id)
            .collect();
        for task_id in task_ids {
            self.cancel_task(task_id);
        }
    }

}

#[cfg(test)]
mod startup_path_candidate_tests {
    use super::*;

    fn nte_game() -> GameInstall {
        GameInstall {
            definition: crate::model::GameDefinition {
                id: "nte".to_string(),
                name: "Neverness To Everness".to_string(),
                backend: GameBackend::UnrealEngine,
                xxmi_code: String::new(),
            },
            mods_path_override: None,
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: true,
            enabled: true,
        }
    }

    #[test]
    fn nte_scan_collapses_executables_from_same_unreal_install() {
        let game = nte_game();
        let root = PathBuf::from(r"C:\Games\Neverness To Everness");
        let launcher = root.join("NTEGlobal").join("NTEGlobalLauncher.exe");
        let wrapper = root.join("NTEGlobal").join("NTEGlobalGame.exe");
        let game_exe = root
            .join("Client")
            .join("WindowsNoEditor")
            .join("HT")
            .join("Binaries")
            .join("Win64")
            .join("HTGame.exe");
        let mut status = StartupPathScanStatus {
            kind: StartupPathTargetKind::Game("nte".to_string()),
            label: "Neverness To Everness".to_string(),
            candidates: Vec::new(),
            selected_candidate: None,
            choosing: false,
        };

        assert!(HestiaApp::merge_startup_path_candidate(
            &mut status,
            Some(&game),
            launcher.clone(),
        ));
        assert_eq!(status.candidates, vec![launcher.clone()]);
        assert_eq!(status.selected_candidate.as_ref(), Some(&launcher));

        assert!(HestiaApp::merge_startup_path_candidate(
            &mut status,
            Some(&game),
            game_exe.clone(),
        ));
        assert_eq!(status.candidates, vec![game_exe.clone()]);
        assert_eq!(status.selected_candidate.as_ref(), Some(&game_exe));

        assert!(!HestiaApp::merge_startup_path_candidate(
            &mut status,
            Some(&game),
            wrapper,
        ));
        assert_eq!(status.candidates, vec![game_exe.clone()]);
        assert_eq!(status.selected_candidate.as_ref(), Some(&game_exe));
    }
}

#[cfg(test)]
mod update_signature_tests {
    use super::*;

    fn mod_file(id: u64, file_name: &str, date_added: i64) -> gamebanana::ModFile {
        gamebanana::ModFile {
            id,
            file_name: file_name.to_string(),
            file_size: 1,
            date_added,
            download_count: 0,
            description: None,
            version: None,
            download_url: Some(format!("https://example.com/{file_name}")),
            is_archived: false,
        }
    }

    fn profile(files: Vec<gamebanana::ModFile>, update_ts: i64) -> gamebanana::ProfileResponse {
        gamebanana::ProfileResponse {
            id: 1,
            date_modified: update_ts,
            date_updated: Some(update_ts),
            files,
            ..Default::default()
        }
    }

    #[test]
    fn file_set_tracks_remote_files_detects_every_tracking_shape() {
        assert!(!file_set_tracks_remote_files(&FileSetRecipe::default()));
        assert!(file_set_tracks_remote_files(&FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&mod_file(
                10, "a.zip", 100
            ))],
            ..Default::default()
        }));
        assert!(file_set_tracks_remote_files(&FileSetRecipe {
            selected_file_ids: vec![10],
            ..Default::default()
        }));
        assert!(file_set_tracks_remote_files(&FileSetRecipe {
            selected_file_names: vec!["a.zip".to_string()],
            ..Default::default()
        }));
    }

    #[test]
    fn profile_without_download_urls_would_otherwise_flip_tracked_mods_to_missing() {
        // The scenario the worker guard protects against: the profile parses and
        // looks available, but every file lost its download URL, which silently
        // evaluated to MissingSource before the guard existed.
        let mut degraded = mod_file(10, "current.zip", 100);
        degraded.download_url = None;
        let profile = profile(vec![degraded.clone()], 100);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&degraded)],
            ..Default::default()
        };

        assert!(!gamebanana::is_unavailable(&profile));
        assert!(downloadable_all_files(&profile).is_empty());
        assert!(file_set_tracks_remote_files(&file_set));
        assert_eq!(
            determine_file_set_update_state(&file_set, Some(100), &profile),
            ModUpdateState::MissingSource
        );
    }

    #[test]
    fn update_signature_uses_legacy_selected_file_ids() {
        let profile = profile(vec![mod_file(10, "old.zip", 100), mod_file(20, "old v2.zip", 200)], 200);
        let file_set = FileSetRecipe {
            selected_file_ids: vec![10],
            ..Default::default()
        };

        let signature = compute_update_signature(&file_set, &profile).unwrap();

        assert!(!signature.prearmed_next_update);
        assert_eq!(signature.profile_update_ts, None);
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 20);
    }

    #[test]
    fn unrelated_new_file_does_not_update_tracked_file() {
        let current = mod_file(10, "old.zip", 100);
        let unrelated = mod_file(20, "new.zip", 200);
        let profile = profile(vec![current.clone(), unrelated], 200);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&current)],
            ..Default::default()
        };

        assert!(compute_update_signature(&file_set, &profile).is_none());
        assert_eq!(
            determine_file_set_update_state(&file_set, Some(100), &profile),
            ModUpdateState::UpToDate
        );
    }

    fn labeled_mod_file(
        id: u64,
        file_name: &str,
        date_added: i64,
        label: &str,
    ) -> gamebanana::ModFile {
        gamebanana::ModFile {
            description: Some(label.to_string()),
            ..mod_file(id, file_name, date_added)
        }
    }

    // RabbitFX ships files named `v<version>_<hash>.zip`; every release replaces
    // both variants, so the file label is the only way to tell which new file
    // succeeds the one that was installed.
    #[test]
    fn label_match_maps_hash_named_reupload_to_tracked_variant() {
        let old_main = labeled_mod_file(10, "v23_9f8e7d.zip", 100, "Main File");
        let new_main = labeled_mod_file(30, "v24_a1b2c3.zip", 300, "main file");
        let new_experimental = labeled_mod_file(40, "v42_d4e5f6.zip", 300, "Experimental");
        let profile = profile(vec![new_main, new_experimental], 300);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&old_main)],
            ..Default::default()
        };

        assert_eq!(
            determine_file_set_update_state(&file_set, Some(100), &profile),
            ModUpdateState::UpdateAvailable
        );
        let signature = compute_update_signature(&file_set, &profile).unwrap();
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 30);
    }

    #[test]
    fn label_match_assigns_each_variant_to_its_own_mod() {
        let old_main = labeled_mod_file(10, "v23_9f8e7d.zip", 100, "Main File");
        let old_experimental = labeled_mod_file(20, "v41_112233.zip", 100, "Experimental");
        let new_main = labeled_mod_file(30, "v24_a1b2c3.zip", 300, "Main File");
        let new_experimental = labeled_mod_file(40, "v42_d4e5f6.zip", 300, "Experimental");
        let profile = profile(vec![new_main, new_experimental], 300);
        let items = vec![
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&old_main)],
                    ..Default::default()
                },
            ),
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&old_experimental)],
                    ..Default::default()
                },
            ),
        ];

        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpdateAvailable);
        let main_signature = evaluations[0].signature.as_ref().unwrap();
        assert_eq!(main_signature.files.len(), 1);
        assert_eq!(main_signature.files[0].file_id, 30);

        assert_eq!(evaluations[1].state, ModUpdateState::UpdateAvailable);
        let experimental_signature = evaluations[1].signature.as_ref().unwrap();
        assert_eq!(experimental_signature.files.len(), 1);
        assert_eq!(experimental_signature.files[0].file_id, 40);
    }

    #[test]
    fn backfill_adopts_labels_from_still_existing_remote_files() {
        let unlabeled = mod_file(10, "v23_9f8e7d.zip", 100);
        let mut file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&unlabeled)],
            ..Default::default()
        };
        assert!(file_set.selected_files_meta[0].label.is_none());

        let remote = labeled_mod_file(10, "v23_9f8e7d.zip", 100, "Main File");
        let profile = profile(vec![remote], 100);

        assert!(backfill_tracked_file_labels(&mut file_set, &profile));
        assert_eq!(
            file_set.selected_files_meta[0].label.as_deref(),
            Some("Main File")
        );
        // Second pass is a no-op.
        assert!(!backfill_tracked_file_labels(&mut file_set, &profile));
    }

    #[test]
    fn sibling_file_lineage_maps_hair_update_to_hair_entry_only() {
        let body = mod_file(1357843, "covencarlottabody.zip", 100);
        let hair = mod_file(1357842, "covencarlottahair.zip", 100);
        let crystal = mod_file(1357844, "covencarlottacrystalhair.zip", 100);
        let hair_update = mod_file(2000001, "covencarlottahair v2.zip", 200);
        let profile = profile(
            vec![body.clone(), hair.clone(), crystal.clone(), hair_update.clone()],
            200,
        );
        let items = vec![
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&body)],
                    ..Default::default()
                },
            ),
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&hair)],
                    ..Default::default()
                },
            ),
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&crystal)],
                    ..Default::default()
                },
            ),
        ];

        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpdateAvailable);
        assert_eq!(evaluations[2].state, ModUpdateState::UpToDate);
        assert_eq!(
            evaluations[1]
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(hair_update.id)
        );
    }

    #[test]
    fn sibling_file_lineage_handles_new_prefix_after_shared_name() {
        let body = mod_file(1357843, "covencarlottabody.zip", 100);
        let hair = mod_file(1357842, "covencarlottahair.zip", 100);
        let crystal = mod_file(1357844, "covencarlottacrystalhair.zip", 100);
        let hair_update = mod_file(2000001, "covencarlottanewhair.zip", 200);
        let profile = profile(
            vec![body.clone(), hair.clone(), crystal.clone(), hair_update.clone()],
            200,
        );
        let items = vec![
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&body)],
                    ..Default::default()
                },
            ),
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&hair)],
                    ..Default::default()
                },
            ),
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&crystal)],
                    ..Default::default()
                },
            ),
        ];

        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpdateAvailable);
        assert_eq!(evaluations[2].state, ModUpdateState::UpToDate);
        assert_eq!(
            evaluations[1]
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(hair_update.id)
        );
    }

    #[test]
    fn sibling_file_lineage_maps_crystal_update_to_crystal_entry_only() {
        let body = mod_file(1357843, "covencarlottabody.zip", 100);
        let hair = mod_file(1357842, "covencarlottahair.zip", 100);
        let crystal = mod_file(1357844, "covencarlottacrystalhair.zip", 100);
        let crystal_update = mod_file(2000001, "covencarlottacrystalhair FIX.zip", 200);
        let profile = profile(
            vec![body.clone(), hair.clone(), crystal.clone(), crystal_update.clone()],
            200,
        );
        let items = vec![
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&body)],
                    ..Default::default()
                },
            ),
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&hair)],
                    ..Default::default()
                },
            ),
            (
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![tracked_file_meta_from_mod_file(&crystal)],
                    ..Default::default()
                },
            ),
        ];

        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[2].state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluations[2]
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(crystal_update.id)
        );
    }

    #[test]
    fn duplicated_full_file_sets_are_ambiguous_and_not_auto_assigned() {
        let body = mod_file(1357843, "covencarlottabody.zip", 100);
        let hair = mod_file(1357842, "covencarlottahair.zip", 100);
        let crystal = mod_file(1357844, "covencarlottacrystalhair.zip", 100);
        let hair_update = mod_file(2000001, "covencarlottahair v2.zip", 200);
        let profile = profile(
            vec![body.clone(), hair.clone(), crystal.clone(), hair_update],
            200,
        );
        let duplicated_file_set = FileSetRecipe {
            selected_files_meta: vec![
                tracked_file_meta_from_mod_file(&body),
                tracked_file_meta_from_mod_file(&hair),
                tracked_file_meta_from_mod_file(&crystal),
            ],
            ..Default::default()
        };
        let items = vec![
            (Some(100), duplicated_file_set.clone()),
            (Some(100), duplicated_file_set.clone()),
            (Some(100), duplicated_file_set),
        ];

        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[2].state, ModUpdateState::UpToDate);
        assert!(evaluations.iter().all(|evaluation| evaluation.signature.is_none()));
    }

    #[test]
    fn update_signature_falls_back_to_profile_timestamp_for_update_available() {
        let profile = profile(Vec::new(), 200);
        let signature =
            current_update_signature_for_state(&FileSetRecipe::default(), &profile, ModUpdateState::UpdateAvailable)
                .unwrap();

        assert!(signature.files.is_empty());
        assert_eq!(signature.profile_update_ts, Some(200));
        assert!(!signature.prearmed_next_update);
    }

    #[test]
    fn update_signature_does_not_use_profile_timestamp_without_current_update() {
        let profile = profile(Vec::new(), 200);

        assert!(
            current_update_signature_for_state(&FileSetRecipe::default(), &profile, ModUpdateState::UpToDate)
                .is_none()
        );
    }

    #[test]
    fn missing_source_is_not_retried() {
        assert!(!should_check_update_state(ModUpdateState::MissingSource));
        assert!(should_check_update_state(ModUpdateState::UpToDate));
        assert!(should_check_update_state(ModUpdateState::UpdateAvailable));
    }

    #[test]
    fn prearmed_ignore_once_persists_while_up_to_date() {
        let current = mod_file(10, "current.zip", 100);
        let profile = profile(vec![current.clone()], 100);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&current)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set: file_set.clone(),
            ignored_update_signature: current_remote_signature(&file_set, &profile),
            ..Default::default()
        };

        let state = apply_ignored_update_override(&mut source, ModUpdateState::UpToDate, Some(&profile));

        assert_eq!(state, ModUpdateState::UpToDate);
        assert!(
            source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|signature| signature.prearmed_next_update)
        );
    }

    #[test]
    fn prearmed_ignore_once_converts_to_next_update_signature() {
        let current = mod_file(10, "current.zip", 100);
        let update = mod_file(20, "current v2.zip", 200);
        let current_profile = profile(vec![current.clone()], 100);
        let update_profile = profile(vec![current.clone(), update], 200);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&current)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set: file_set.clone(),
            ignored_update_signature: current_remote_signature(&file_set, &current_profile),
            ..Default::default()
        };

        let state =
            apply_ignored_update_override(&mut source, ModUpdateState::UpdateAvailable, Some(&update_profile));

        let signature = source.ignored_update_signature.as_ref().unwrap();
        assert_eq!(state, ModUpdateState::IgnoringUpdateOnce);
        assert!(!signature.prearmed_next_update);
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 20);
    }

    #[test]
    fn prearmed_ignore_once_converts_for_modified_local_update() {
        let current = mod_file(10, "current.zip", 100);
        let update = mod_file(20, "current v2.zip", 200);
        let current_profile = profile(vec![current.clone()], 100);
        let update_profile = profile(vec![current.clone(), update], 200);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&current)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set: file_set.clone(),
            ignored_update_signature: current_remote_signature(&file_set, &current_profile),
            ..Default::default()
        };

        let state =
            apply_ignored_update_override(&mut source, ModUpdateState::ModifiedLocally, Some(&update_profile));

        let signature = source.ignored_update_signature.as_ref().unwrap();
        assert_eq!(state, ModUpdateState::ModifiedLocally);
        assert!(!signature.prearmed_next_update);
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 20);
    }

    #[test]
    fn normal_ignore_once_clears_on_subsequent_update() {
        let installed = mod_file(10, "installed.zip", 100);
        let ignored = mod_file(20, "installed v2.zip", 200);
        let newer = mod_file(30, "installed v3.zip", 300);
        let update_profile = profile(vec![installed.clone(), ignored.clone(), newer], 300);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&installed)],
            ..Default::default()
        };
        let mut source = ModSourceData {
            file_set,
            ignored_update_signature: Some(IgnoredUpdateSignature {
                files: vec![tracked_file_meta_from_mod_file(&ignored)],
                profile_update_ts: None,
                prearmed_next_update: false,
            }),
            ..Default::default()
        };

        let state =
            apply_ignored_update_override(&mut source, ModUpdateState::UpdateAvailable, Some(&update_profile));

        assert_eq!(state, ModUpdateState::UpdateAvailable);
        assert!(source.ignored_update_signature.is_none());
    }
}
