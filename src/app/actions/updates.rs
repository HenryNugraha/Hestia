mod matcher_adapter;

fn tracked_file_meta_from_mod_file(file: &gamebanana::ModFile) -> TrackedFileMeta {
    TrackedFileMeta {
        file_id: file.id,
        file_name: file.file_name.clone(),
        file_size: Some(file.file_size),
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

/// Extend an installed file recipe with the files that were actually committed
/// by a Merge install.  Old metadata is intentionally retained: it can contain
/// archived/deleted files or legacy name-only identities that are no longer in
/// the current profile.  Nonzero GameBanana IDs are authoritative; legacy
/// entries without an ID fall back to their file name.
fn add_merge_tracked_file(recipe: &mut FileSetRecipe, tracked: TrackedFileMeta) {
    let duplicate = if tracked.file_id != 0 {
        recipe
            .selected_files_meta
            .iter()
            .any(|previous| previous.file_id == tracked.file_id)
    } else {
        recipe.selected_files_meta.iter().any(|previous| {
            previous.file_id == 0 && previous.file_name == tracked.file_name
        })
    };
    if !duplicate {
        recipe.selected_files_meta.push(tracked);
    }
}

fn merge_file_set_recipe(
    existing: &FileSetRecipe,
    selected_files: &[gamebanana::ModFile],
    selected_candidate_labels: &[String],
) -> FileSetRecipe {
    let mut merged = FileSetRecipe::default();

    // Keep richer metadata first, then reconstruct selectors that older state
    // persisted without a metadata record. This retains deleted/legacy files
    // without inventing a newer date or archive identity.
    for tracked in &existing.selected_files_meta {
        add_merge_tracked_file(&mut merged, tracked.clone());
    }
    let selector_count = existing
        .selected_file_ids
        .len()
        .max(existing.selected_file_names.len());
    for index in 0..selector_count {
        let file_id = existing.selected_file_ids.get(index).copied().unwrap_or(0);
        let file_name = existing
            .selected_file_names
            .get(index)
            .cloned()
            .unwrap_or_default();
        let already_tracked = if file_id != 0 {
            merged
                .selected_files_meta
                .iter()
                .any(|tracked| tracked.file_id == file_id)
        } else {
            merged
                .selected_files_meta
                .iter()
                .any(|tracked| tracked.file_name == file_name)
        };
        if !already_tracked {
            add_merge_tracked_file(&mut merged, TrackedFileMeta {
                file_id,
                file_name,
                ..TrackedFileMeta::default()
            });
        }
    }
    for label in &existing.selected_candidate_labels {
        if !label.is_empty() && !merged.selected_candidate_labels.contains(label) {
            merged.selected_candidate_labels.push(label.clone());
        }
    }

    for file in selected_files {
        add_merge_tracked_file(&mut merged, tracked_file_meta_from_mod_file(file));
    }

    // Keep selectors aligned with metadata. A legacy record cannot be paired
    // with an ID vector without inserting a fake zero ID, so names become the
    // complete selector representation whenever any ID is unknown.
    if merged
        .selected_files_meta
        .iter()
        .all(|tracked| tracked.file_id != 0)
    {
        merged.selected_file_ids = merged
            .selected_files_meta
            .iter()
            .map(|tracked| tracked.file_id)
            .collect();
        merged.selected_file_names = merged
            .selected_files_meta
            .iter()
            .map(|tracked| tracked.file_name.clone())
            .collect();
        if merged
            .selected_file_names
            .iter()
            .any(|name| name.is_empty())
        {
            merged.selected_file_names.clear();
        }
    } else {
        merged.selected_file_ids.clear();
        merged.selected_file_names = merged
            .selected_files_meta
            .iter()
            .map(|tracked| tracked.file_name.clone())
            .collect();
        if merged
            .selected_file_names
            .iter()
            .any(|name| name.is_empty())
        {
            merged.selected_file_names.clear();
        }
    }

    for label in selected_candidate_labels {
        if !label.is_empty() && !merged.selected_candidate_labels.contains(label) {
            merged.selected_candidate_labels.push(label.clone());
        }
    }
    merged
}

fn selected_file_baseline_ts(file_set: &FileSetRecipe) -> Option<i64> {
    file_set.selected_files_meta.iter().map(|file| file.date_added).max()
}

#[derive(Debug, Clone, Default)]
struct FileSetUpdateEvaluation {
    state: ModUpdateState,
    signature: Option<IgnoredUpdateSignature>,
}

fn update_task_targets_mod(task: &TaskEntry, target: &ModEntry, update_title: &str) -> bool {
    if task.game_id.as_deref() != Some(target.game_id.as_str())
        || !matches!(
            task.status,
            TaskStatus::Queued | TaskStatus::Downloading | TaskStatus::Installing | TaskStatus::Canceling
        )
    {
        return false;
    }
    match task.retry_payload.as_ref() {
        Some(TaskRetryPayload::BrowseDownload(payload)) => {
            payload.update_target_mod_id.as_deref().map_or(
                task.title == update_title,
                |id| id == target.id,
            )
        }
        None => task.title == update_title,
    }
}

/// A `current` answer with only separate extra files has no main download
/// and drops the update task, so the bundle is all that ties them to the mod.
fn thalia_bundle_updates_mod(bundle: &ThaliaInstallBundle, target: &ModEntry) -> bool {
    bundle.game_id == target.game_id
        && bundle
            .target
            .as_ref()
            .is_some_and(|(mod_id, _)| *mod_id == target.id)
}

#[derive(Debug, Clone)]
enum ResolvedTrackedFiles {
    Tracked(Vec<TrackedFileMeta>),
    MissingSource,
    Invalid,
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
    files.dedup_by(|a, b| {
        a.id == b.id || (a.file_name == b.file_name && a.date_added == b.date_added)
    });
    files
}

fn downloadable_active_files(profile: &gamebanana::ProfileResponse) -> Vec<&gamebanana::ModFile> {
    profile
        .files
        .iter()
        .filter(|file| {
            file.id != 0
                && !file.is_archived
                && file
                    .download_url
                    .as_deref()
                    .is_some_and(|url| !url.trim().is_empty())
        })
        .collect()
}

fn downloadable_all_files(profile: &gamebanana::ProfileResponse) -> Vec<&gamebanana::ModFile> {
    profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| {
            file.id != 0
                && file
                    .download_url
                    .as_deref()
                    .is_some_and(|url| !url.trim().is_empty())
        })
        .collect()
}

/// Resolves the persisted install recipe into a complete, date-bearing
/// baseline for the passive new-file signal.  A missing or ambiguous part of
/// the recipe makes the whole baseline unknown; guessing here would hide a
/// missed upload behind a local install timestamp.
fn known_installed_files_for_newer_notice(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Option<Vec<TrackedFileMeta>> {
    let canonical = matcher_adapter::canonicalize_profile(profile);
    if gamebanana::is_unavailable(&canonical.profile) || !canonical.ambiguous_ids.is_empty() {
        return None;
    }
    let all_remote_files = all_profile_files(&canonical.profile);
    if !selector_shape_is_valid(file_set) {
        return None;
    }
    let mut tracked = match resolve_tracked_files(file_set, &all_remote_files) {
        ResolvedTrackedFiles::Tracked(files) => files,
        ResolvedTrackedFiles::MissingSource
        | ResolvedTrackedFiles::Invalid
        | ResolvedTrackedFiles::Untracked => return None,
    };
    if tracked.is_empty() {
        return None;
    }
    if tracked
        .iter()
        .any(|file| file.file_id == 0 && file.file_name.trim().is_empty())
    {
        return None;
    }

    // `selected_files_meta` is authoritative when present, including for a
    // source the author later deleted.  Only a missing date may be hydrated
    // from an unambiguous current ID/name resolution.
    for installed in &mut tracked {
        if installed.date_added > 0 {
            continue;
        }
        let mut matches = all_remote_files.iter().copied().filter(|file| {
            if installed.file_id != 0 {
                file.id == installed.file_id
            } else {
                !installed.file_name.is_empty() && file.file_name == installed.file_name
            }
        });
        let Some(remote) = matches.next() else {
            return None;
        };
        if matches.next().is_some() || remote.date_added <= 0 {
            return None;
        }
        installed.date_added = remote.date_added;
        if installed.file_id == 0 {
            installed.file_id = remote.id;
        }
    }
    tracked.iter().all(|file| file.date_added > 0).then_some(tracked)
}

/// Returns active downloadable GameBanana files strictly newer than the
/// known installed archive upload baseline.  This does not infer lineage or
/// variants: it is only a conservative availability fact for the UI's
/// passive "new files" notice.
fn available_newer_files(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Vec<gamebanana::ModFile> {
    let canonical = matcher_adapter::canonicalize_profile(profile);
    if gamebanana::is_unavailable(&canonical.profile) || !canonical.ambiguous_ids.is_empty() {
        return Vec::new();
    }
    let Some(installed) = known_installed_files_for_newer_notice(file_set, &canonical.profile) else {
        return Vec::new();
    };
    let Some(baseline) = installed.iter().map(|file| file.date_added).max() else {
        return Vec::new();
    };
    let installed_ids: HashSet<u64> = installed
        .iter()
        .filter_map(|file| (file.file_id != 0).then_some(file.file_id))
        .collect();
    let mut newer: Vec<_> = canonical
        .profile
        .files
        .iter()
        .filter(|file| {
            file.id != 0
                && file.date_added > baseline
                && !installed_ids.contains(&file.id)
                && profile_file_is_active_downloadable(&canonical.profile, file)
        })
        .cloned()
        .collect();
    newer.sort_by(|a, b| {
        b.date_added
            .cmp(&a.date_added)
            .then_with(|| b.id.cmp(&a.id))
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    newer
}

/// The file ids a linked mod has installed.
fn installed_file_ids(file_set: &FileSetRecipe) -> impl Iterator<Item = u64> + '_ {
    file_set
        .selected_file_ids
        .iter()
        .copied()
        .chain(file_set.selected_files_meta.iter().map(|file| file.file_id))
        .filter(|id| *id != 0)
}

/// The files a mod has, plus those its pending update would install.
fn claimed_file_ids(
    file_set: &FileSetRecipe,
    pending_update: Option<&IgnoredUpdateSignature>,
) -> HashSet<u64> {
    installed_file_ids(file_set)
        .chain(
            pending_update
                .into_iter()
                .flat_map(|signature| signature.files.iter().map(|file| file.file_id))
                .filter(|id| *id != 0),
        )
        .collect()
}

fn all_profile_files(profile: &gamebanana::ProfileResponse) -> Vec<&gamebanana::ModFile> {
    profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .collect()
}

/// Reconstructs an old ID-only/name-only recipe only when every selected
/// identity can be recovered from one cached catalog. A partial result would
/// silently turn a multi-file install into a different recipe, so callers
/// must keep the original legacy selectors when this returns `None`.
fn reconstruct_tracked_files_from_candidates(
    file_set: &FileSetRecipe,
    candidates: &[TrackedFileMeta],
) -> Option<Vec<TrackedFileMeta>> {
    if !selector_shape_is_valid(file_set) {
        return None;
    }
    let count = file_set
        .selected_file_ids
        .len()
        .max(file_set.selected_file_names.len());
    if count == 0 {
        return None;
    }

    let mut used_ids = HashSet::new();
    let mut reconstructed = Vec::with_capacity(count);
    for index in 0..count {
        let selected_id = file_set.selected_file_ids.get(index).copied();
        let selected_name = file_set
            .selected_file_names
            .get(index)
            .map(String::as_str)
            .filter(|name| !name.is_empty());
        let mut candidate = selected_id
            .filter(|id| *id != 0)
            .and_then(|id| {
                candidates
                    .iter()
                    .find(|candidate| candidate.file_id == id && used_ids.insert(candidate.file_id))
            })
            .cloned();
        if candidate.is_none() && selected_id.map_or(true, |id| id == 0) {
            let mut name_matches = candidates.iter().filter(|candidate| {
                selected_name.is_some_and(|name| candidate.file_name == name)
                    && (candidate.file_id == 0 || !used_ids.contains(&candidate.file_id))
            });
            let first = name_matches.next();
            if name_matches.next().is_some() {
                return None;
            }
            candidate = first.cloned();
            if let Some(candidate) = candidate.as_ref() {
                if candidate.file_id != 0 && !used_ids.insert(candidate.file_id) {
                    return None;
                }
            }
        }
        reconstructed.push(candidate?);
    }
    Some(reconstructed)
}

fn selector_shape_is_valid(file_set: &FileSetRecipe) -> bool {
    let ids = &file_set.selected_file_ids;
    let names = &file_set.selected_file_names;
    let unique_ids = ids.iter().copied().collect::<HashSet<_>>().len() == ids.len();
    if ids.iter().any(|id| *id == 0)
        || !unique_ids
        || names.iter().any(|name| name.trim().is_empty())
    {
        return false;
    }
    if file_set.selected_files_meta.is_empty() {
        // Names alone must identify one file each. When stable IDs are also
        // present, names are historical hints and may legitimately repeat or
        // no longer match the current catalog.
        if ids.is_empty()
            && names
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != names.len()
        {
            return false;
        }
        if !ids.is_empty() && !names.is_empty() && ids.len() != names.len() {
            return false;
        }
        return true;
    }
    let selector_count = ids.len().max(names.len());
    if selector_count != 0
        && (ids.len() != 0 && ids.len() != file_set.selected_files_meta.len()
            || names.len() != 0 && names.len() != file_set.selected_files_meta.len())
    {
        return false;
    }
    let mut seen_ids = HashSet::new();
    let metadata_ids: HashSet<u64> = file_set
        .selected_files_meta
        .iter()
        .filter_map(|tracked| (tracked.file_id != 0).then_some(tracked.file_id))
        .collect();
    for tracked in &file_set.selected_files_meta {
        if tracked.file_id != 0 && !seen_ids.insert(tracked.file_id) {
            return false;
        }
    }
    if !ids.is_empty() && ids.iter().any(|id| !metadata_ids.contains(id)) {
        return false;
    }
    true
}

fn selectors_conflict_with_catalog(
    file_set: &FileSetRecipe,
    all_remote_files: &[&gamebanana::ModFile],
) -> bool {
    // A nonzero persisted ID is authoritative. The author may rename or
    // replace a file while reusing the old name, and the current catalog may
    // contain the same name under several distinct IDs. Only name-only
    // recipes need unique current-name resolution.
    if !file_set.selected_file_ids.is_empty() || file_set.selected_file_names.is_empty() {
        return false;
    }
    let metadata_has_complete_ids = !file_set.selected_files_meta.is_empty()
        && file_set
            .selected_files_meta
            .iter()
            .all(|tracked| tracked.file_id != 0)
        && file_set.selected_files_meta.len() == file_set.selected_file_names.len();
    if metadata_has_complete_ids {
        return false;
    }
    file_set.selected_file_names.iter().any(|name| {
        all_remote_files
            .iter()
            .filter(|file| file.file_name == *name)
            .nth(1)
            .is_some()
    })
}

/// Fills metadata introduced after older installs were written. The source
/// cache is consulted in the same order as update refreshes: raw profile,
/// snapshot, then the newly fetched profile. Missing files are left alone so
/// a deleted/legacy source remains distinguishable from an observed zero-byte
/// archive and never triggers a network or archive read.
fn hydrate_tracked_file_sizes(
    file_set: &mut FileSetRecipe,
    old_raw_profile_json: Option<&str>,
    old_snapshot: Option<&GameBananaSnapshot>,
    new_profile: Option<&gamebanana::ProfileResponse>,
) -> bool {
    if !file_set.selected_files_meta.is_empty()
        && file_set
            .selected_files_meta
            .iter()
            .all(|tracked| tracked.file_size.is_some())
    {
        return false;
    }
    let old_raw_profile = old_raw_profile_json
        .and_then(|raw| serde_json::from_str::<gamebanana::ProfileResponse>(raw).ok())
        .map(|profile| matcher_adapter::canonicalize_profile(&profile).profile);
    let old_snapshot_profile = old_snapshot
        .map(|snapshot| profile_to_response(Some(snapshot), 0))
        .map(|profile| matcher_adapter::canonicalize_profile(&profile).profile);
    let new_profile = new_profile
        .map(|profile| matcher_adapter::canonicalize_profile(profile).profile);
    let mut changed = false;

    if file_set.selected_files_meta.is_empty() {
        let old_raw_candidates = old_raw_profile.as_ref().map(|profile| {
            all_profile_files(profile)
                .into_iter()
                .map(tracked_file_meta_from_mod_file)
                .collect::<Vec<_>>()
        });
        let old_snapshot_candidates = old_snapshot_profile.as_ref().map(|profile| {
            all_profile_files(profile)
                .into_iter()
                .map(tracked_file_meta_from_mod_file)
                .collect::<Vec<_>>()
        });
        let new_profile_candidates = new_profile.as_ref().map(|profile| {
            all_profile_files(profile)
                .into_iter()
                .map(tracked_file_meta_from_mod_file)
                .collect::<Vec<_>>()
        });
        for candidates in [
            old_raw_candidates.as_deref(),
            old_snapshot_candidates.as_deref(),
            new_profile_candidates.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(reconstructed) = reconstruct_tracked_files_from_candidates(file_set, candidates) {
                file_set.selected_files_meta = reconstructed;
                changed = true;
                break;
            }
        }
    }

    if file_set.selected_files_meta.is_empty()
        || file_set
            .selected_files_meta
            .iter()
            .all(|tracked| tracked.file_size.is_some())
    {
        return changed;
    }

    let old_raw_files = old_raw_profile.as_ref().map(all_profile_files);
    let old_snapshot_files = old_snapshot_profile.as_ref().map(all_profile_files);
    let new_profile_files = new_profile.as_ref().map(all_profile_files);

    for tracked in &mut file_set.selected_files_meta {
        if tracked.file_size.is_some() {
            continue;
        }
        let size = old_raw_files
            .as_ref()
            .and_then(|files| {
                files
                    .iter()
                    .copied()
                    .find(|file| remote_file_matches_tracked(file, tracked))
                    .map(|file| file.file_size)
            })
            .or_else(|| {
                old_snapshot_files.as_ref().and_then(|files| {
                    files
                        .iter()
                        .copied()
                        .find(|file| remote_file_matches_tracked(file, tracked))
                        .map(|file| file.file_size)
                })
            })
            .or_else(|| {
                new_profile_files.as_ref().and_then(|files| {
                    files
                        .iter()
                        .copied()
                        .find(|file| remote_file_matches_tracked(file, tracked))
                        .map(|file| file.file_size)
                })
            });
        if let Some(size) = size {
            tracked.file_size = Some(size);
            changed = true;
        }
    }
    changed
}

fn remote_file_matches_tracked(file: &gamebanana::ModFile, tracked: &TrackedFileMeta) -> bool {
    if tracked.file_id != 0 {
        file.id == tracked.file_id
    } else {
        file.file_name == tracked.file_name && file.date_added == tracked.date_added
    }
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
    if !selector_shape_is_valid(file_set)
        || selectors_conflict_with_catalog(file_set, all_remote_files)
    {
        return ResolvedTrackedFiles::Invalid;
    }
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
        } else if tracked_files.is_empty() {
            ResolvedTrackedFiles::MissingSource
        } else {
            ResolvedTrackedFiles::Invalid
        };
    }

    if !file_set.selected_file_names.is_empty() {
        let mut tracked_files = Vec::with_capacity(file_set.selected_file_names.len());
        for selected_name in &file_set.selected_file_names {
            let mut matches = all_remote_files
                .iter()
                .copied()
                .filter(|file| file.file_name == *selected_name);
            let Some(file) = matches.next() else {
                continue;
            };
            if matches.next().is_some() {
                return ResolvedTrackedFiles::Invalid;
            }
            tracked_files.push(tracked_file_meta_from_mod_file(file));
        }
        return if tracked_files.len() == file_set.selected_file_names.len() {
            ResolvedTrackedFiles::Tracked(tracked_files)
        } else if tracked_files.is_empty() {
            ResolvedTrackedFiles::MissingSource
        } else {
            ResolvedTrackedFiles::Invalid
        };
    }

    ResolvedTrackedFiles::Untracked
}

fn resolve_fallback_tracked_files(
    file_set: &FileSetRecipe,
    all_profile_files: &[&gamebanana::ModFile],
) -> Option<Vec<TrackedFileMeta>> {
    if !selector_shape_is_valid(file_set)
        || selectors_conflict_with_catalog(file_set, all_profile_files)
    {
        return None;
    }
    if !file_set.selected_files_meta.is_empty() {
        return Some(file_set.selected_files_meta.clone());
    }
    if !file_set.selected_file_ids.is_empty() {
        let selected_ids: HashSet<u64> = file_set.selected_file_ids.iter().copied().collect();
        let tracked_files: Vec<_> = all_profile_files
            .iter()
            .copied()
            .filter(|file| selected_ids.contains(&file.id))
            .map(tracked_file_meta_from_mod_file)
            .collect();
        return (tracked_files.len() == selected_ids.len()).then_some(tracked_files);
    }
    if !file_set.selected_file_names.is_empty() {
        let mut tracked_files = Vec::with_capacity(file_set.selected_file_names.len());
        for selected_name in &file_set.selected_file_names {
            let mut matches = all_profile_files
                .iter()
                .copied()
                .filter(|file| file.file_name == *selected_name);
            let file = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            tracked_files.push(tracked_file_meta_from_mod_file(file));
        }
        return Some(tracked_files);
    }
    None
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

fn file_lineage_similarity(source_name: &str, candidate_name: &str, common_prefix: &str) -> f32 {
    let source_core = lineage_match_core(source_name, common_prefix);
    let candidate_core = lineage_match_core(candidate_name, common_prefix);
    let distinctive_score = lineage_core_similarity(&source_core, &candidate_core);

    let source_full = strip_release_markers(&normalized_file_stem_for_lineage(source_name));
    let candidate_full = strip_release_markers(&normalized_file_stem_for_lineage(candidate_name));
    let full_score = lineage_core_similarity(&source_full, &candidate_full);

    distinctive_score.max(full_score)
}

fn canonical_numeric_version(value: Option<&str>) -> Option<Vec<u64>> {
    let value = value?.trim();
    let value = value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .unwrap_or(value);
    if value.is_empty() {
        return None;
    }

    let mut components = Vec::new();
    for component in value.split(['.', '_', '-']) {
        if component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        components.push(component.parse::<u64>().ok()?);
    }
    while components.last() == Some(&0) {
        components.pop();
    }
    if components.is_empty() {
        components.push(0);
    }
    Some(components)
}

fn raw_source_name_coverage(source_name: &str, candidate_name: &str) -> f32 {
    let source = normalized_file_stem_for_lineage(source_name);
    let candidate = normalized_file_stem_for_lineage(candidate_name);
    if source.is_empty() {
        return 0.0;
    }
    longest_common_subsequence_len(&source, &candidate) as f32 / source.chars().count() as f32
}

fn strip_collision_suffix(file_name: &str) -> String {
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    let suffix_start = stem.len().checked_sub(5);
    let Some(suffix_start) = suffix_start else {
        return stem.to_string();
    };
    let Some(suffix) = stem.get(suffix_start..) else {
        return stem.to_string();
    };
    let Some(prefix) = stem.get(..suffix_start) else {
        return stem.to_string();
    };
    if suffix.len() != 5
        || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !suffix.bytes().any(|byte| byte.is_ascii_alphabetic())
        || !suffix.bytes().any(|byte| byte.is_ascii_digit())
        || suffix_start == 0
        || prefix
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_alphanumeric())
    {
        return stem.to_string();
    }
    prefix.to_string()
}

fn normalized_alphabetic_stem(file_name: &str) -> String {
    strip_collision_suffix(file_name)
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphabetic())
        .collect()
}

fn ascii_digit_runs_by_alphabetic_position(file_name: &str) -> HashMap<usize, Vec<String>> {
    let mut runs = HashMap::<usize, Vec<String>>::new();
    let mut alphabetic_count = 0usize;
    let mut current_run: Option<(usize, String)> = None;
    let mut finish_run = |current_run: &mut Option<(usize, String)>| {
        if let Some((position, run)) = current_run.take() {
            runs.entry(position).or_default().push(run);
        }
    };

    for character in strip_collision_suffix(file_name).chars() {
        for lowered in character.to_lowercase() {
            if lowered.is_ascii_digit() {
                match current_run.as_mut() {
                    Some((position, run)) if *position == alphabetic_count => {
                        run.push(lowered);
                    }
                    _ => {
                        finish_run(&mut current_run);
                        current_run = Some((alphabetic_count, lowered.to_string()));
                    }
                }
            } else {
                finish_run(&mut current_run);
                if lowered.is_alphabetic() {
                    alphabetic_count += 1;
                }
            }
        }
    }
    finish_run(&mut current_run);
    runs
}

fn equal_version_digit_runs_are_preserved(source_name: &str, candidate_name: &str) -> bool {
    let source_alpha = normalized_alphabetic_stem(source_name);
    let candidate_alpha = normalized_alphabetic_stem(candidate_name);
    let Some(offset_bytes) = candidate_alpha.find(&source_alpha) else {
        return false;
    };
    let offset = candidate_alpha[..offset_bytes].chars().count();
    let candidate_runs = ascii_digit_runs_by_alphabetic_position(candidate_name);
    ascii_digit_runs_by_alphabetic_position(source_name)
        .into_iter()
        .all(|(position, source_runs)| {
            candidate_runs.get(&(offset + position)) == Some(&source_runs)
        })
}

fn lowercase_alphabetic_key(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphabetic())
        .collect()
}

fn filename_identity_tokens(file_name: &str) -> Vec<(String, bool)> {
    let stem = strip_collision_suffix(file_name);
    let mut tokens = Vec::new();
    let mut run = String::new();

    let finish_run = |run: &mut String, tokens: &mut Vec<(String, bool)>| {
        if run.is_empty() {
            return;
        }
        let starts_with_ascii_digit = run
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit());
        let key = lowercase_alphabetic_key(run);
        if !key.is_empty() {
            tokens.push((key, starts_with_ascii_digit));
        }
        run.clear();
    };

    for character in stem.chars() {
        if character.is_alphanumeric() {
            run.push(character);
        } else {
            finish_run(&mut run, &mut tokens);
        }
    }
    finish_run(&mut run, &mut tokens);
    tokens
}

fn token_identity_condition(source_name: &str, candidate_name: &str) -> bool {
    let source_tokens = filename_identity_tokens(source_name);
    let candidate_tokens = filename_identity_tokens(candidate_name);
    let mut candidate_index = 0usize;

    for source_token in source_tokens {
        let mut matched = false;
        while candidate_index < candidate_tokens.len() {
            let (candidate_key, starts_with_ascii_digit) = &candidate_tokens[candidate_index];
            candidate_index += 1;
            if candidate_key.starts_with(&source_token.0) {
                matched = true;
                break;
            }
            if !*starts_with_ascii_digit {
                return false;
            }
        }
        if !matched {
            return false;
        }
    }

    candidate_tokens[candidate_index..]
        .iter()
        .all(|(_, starts_with_ascii_digit)| *starts_with_ascii_digit)
}

fn file_description_text(file: &gamebanana::ModFile) -> String {
    file.description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn tracked_or_remote_label_text(
    tracked: &TrackedFileMeta,
    remote: &gamebanana::ModFile,
) -> String {
    let remote_label = file_description_text(remote);
    if !remote_label.is_empty() {
        remote_label
    } else {
        tracked
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or_default()
            .to_string()
    }
}

fn strip_file_extension(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name)
}

fn hex_collision_suffix(stem: &str) -> Option<(&str, &str)> {
    let suffix_start = stem.len().checked_sub(5)?;
    let prefix = stem.get(..suffix_start)?;
    let suffix = stem.get(suffix_start..)?;
    if suffix.len() != 5
        || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        || prefix.is_empty()
        || prefix
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_alphanumeric())
    {
        return None;
    }
    Some((prefix, suffix))
}

fn mixed_hex_suffix(file_name: &str) -> Option<&str> {
    let stem = strip_file_extension(file_name);
    let (_, suffix) = hex_collision_suffix(stem)?;
    (suffix.bytes().any(|byte| byte.is_ascii_alphabetic())
        && suffix.bytes().any(|byte| byte.is_ascii_digit()))
    .then_some(suffix)
}

fn nonmixed_hex_suffix(file_name: &str) -> Option<&str> {
    let stem = strip_file_extension(file_name);
    let (_, suffix) = hex_collision_suffix(stem)?;
    let mixed = suffix.bytes().any(|byte| byte.is_ascii_alphabetic())
        && suffix.bytes().any(|byte| byte.is_ascii_digit());
    (!mixed).then_some(suffix)
}

fn has_gamebanana_alnum_suffix(file_name: &str) -> bool {
    let stem = strip_file_extension(file_name);
    let Some((_, suffix)) = stem.rsplit_once("_-") else {
        return false;
    };
    suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn catalog_contrast_filename_eligible(file_name: &str) -> bool {
    nonmixed_hex_suffix(file_name).is_none() && !has_gamebanana_alnum_suffix(file_name)
}

fn collision_stripped_stem(file_name: &str, allow_all_hex: bool) -> String {
    let stem = strip_file_extension(file_name);
    let Some((prefix, suffix)) = hex_collision_suffix(stem) else {
        return stem.to_string();
    };
    let mixed = suffix.bytes().any(|byte| byte.is_ascii_alphabetic())
        && suffix.bytes().any(|byte| byte.is_ascii_digit());
    if mixed || (allow_all_hex && !mixed) {
        prefix.trim_end().to_string()
    } else {
        stem.to_string()
    }
}

fn lower_alpha(value: &str) -> String {
    value.chars().flat_map(char::to_lowercase).collect()
}

fn exact_filename_key(file_name: &str, allow_all_hex: bool) -> String {
    lower_alpha(&collision_stripped_stem(file_name, allow_all_hex))
}

fn all_hex_context(
    source: &gamebanana::ModFile,
    candidate: &gamebanana::ModFile,
    all_profile_files: &[&gamebanana::ModFile],
) -> bool {
    [source.file_name.as_str(), candidate.file_name.as_str()]
        .into_iter()
        .filter(|file_name| nonmixed_hex_suffix(file_name).is_some())
        .any(|file_name| {
            let target_base = exact_filename_key(file_name, true);
            all_profile_files.iter().any(|file| {
                mixed_hex_suffix(&file.file_name).is_some()
                    && exact_filename_key(&file.file_name, false) == target_base
            })
        })
}

fn filename_atoms(file_name: &str) -> Vec<(bool, String)> {
    let stem = lower_alpha(&collision_stripped_stem(file_name, false));
    let mut atoms = Vec::new();
    let mut current = String::new();
    let mut current_is_alpha = None;

    let finish = |atoms: &mut Vec<(bool, String)>, current: &mut String, kind: &mut Option<bool>| {
        if !current.is_empty() {
            atoms.push((kind.unwrap_or(false), std::mem::take(current)));
        }
        *kind = None;
    };

    for character in stem.chars() {
        let kind = if character.is_alphabetic() {
            Some(true)
        } else if character.is_ascii_digit() {
            Some(false)
        } else {
            None
        };
        match (current_is_alpha, kind) {
            (Some(current_kind), Some(next_kind)) if current_kind == next_kind => {
                current.push(character);
            }
            (_, Some(next_kind)) => {
                finish(&mut atoms, &mut current, &mut current_is_alpha);
                current_is_alpha = Some(next_kind);
                current.push(character);
            }
            (_, None) => finish(&mut atoms, &mut current, &mut current_is_alpha),
        }
    }
    finish(&mut atoms, &mut current, &mut current_is_alpha);
    atoms
}

#[derive(Debug, Clone)]
struct CatalogShapeFile {
    file_id: u64,
    file_name: String,
    date_added: i64,
    atoms: Vec<(bool, String)>,
    description: String,
}

#[derive(Debug, Clone, Default)]
struct CatalogShapeIndex {
    files: Vec<CatalogShapeFile>,
}

impl CatalogShapeIndex {
    fn from_active_files(active_files: &[&gamebanana::ModFile]) -> Self {
        Self {
            files: active_files
                .iter()
                .filter(|file| catalog_contrast_filename_eligible(&file.file_name))
                .map(|file| CatalogShapeFile {
                    file_id: file.id,
                    file_name: file.file_name.clone(),
                    date_added: file.date_added,
                    atoms: filename_atoms(&file.file_name),
                    description: normalized_catalog_description(file),
                })
                .collect(),
        }
    }

    fn for_file(&self, file: &gamebanana::ModFile) -> Option<&CatalogShapeFile> {
        self.files.iter().find(|shape| {
            (file.id != 0 && shape.file_id == file.id)
                || (shape.file_name == file.file_name && shape.date_added == file.date_added)
        })
    }
}

fn normalized_catalog_description(file: &gamebanana::ModFile) -> String {
    file.description
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn alpha_atoms(atoms: &[(bool, String)]) -> Vec<&str> {
    atoms
        .iter()
        .filter_map(|(is_alpha, token)| is_alpha.then_some(token.as_str()))
        .collect()
}

fn direct_digit_follow<'a>(atoms: &'a [(bool, String)], alpha_index: usize) -> Option<&'a str> {
    let mut seen = 0usize;
    for (index, (is_alpha, _)) in atoms.iter().enumerate() {
        if !*is_alpha {
            continue;
        }
        if seen == alpha_index {
            return atoms
                .get(index + 1)
                .filter(|(is_alpha, _)| !*is_alpha)
                .map(|(_, token)| token.as_str());
        }
        seen += 1;
    }
    None
}

fn contrast_choice_tokens(left: &str, right: &str) -> bool {
    !left.is_empty() && !right.is_empty() && !left.starts_with(right) && !right.starts_with(left)
}

fn contrast_choice_related(choice: &str, branch: &str) -> bool {
    choice == branch || choice.starts_with(branch) || branch.starts_with(choice)
}

fn contrast_context_equal_general(
    source: &CatalogShapeFile,
    candidate: &CatalogShapeFile,
    source_alpha: &[&str],
    candidate_alpha: &[&str],
    slot: usize,
) -> bool {
    if source_alpha.len() != candidate_alpha.len() {
        return false;
    }
    if source_alpha.len() < 2 {
        return false;
    }
    source_alpha
        .iter()
        .zip(candidate_alpha)
        .enumerate()
        .all(|(index, (left, right))| {
            if index == slot || left == right {
                return true;
            }
            (left.starts_with(right) || right.starts_with(left))
                && direct_digit_follow(&source.atoms, index)
                    == direct_digit_follow(&candidate.atoms, index)
                && direct_digit_follow(&source.atoms, index).is_some()
        })
}

fn masked_catalog_frame(shape: &CatalogShapeFile, alpha_index: usize) -> Vec<(bool, String)> {
    let mut seen = 0usize;
    shape
        .atoms
        .iter()
        .map(|(is_alpha, token)| {
            if *is_alpha {
                let masked = seen == alpha_index;
                seen += 1;
                (true, masked.then(|| "*".to_owned()).unwrap_or_else(|| token.clone()))
            } else {
                (false, token.clone())
            }
        })
        .collect()
}

fn catalog_contrast_veto(
    tracked: &TrackedFileMeta,
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
    shape_index: &CatalogShapeIndex,
) -> bool {
    let Some(source_file) = fallback_source_file(tracked, all_profile_files) else {
        return false;
    };
    if !profile_file_is_active_downloadable(profile, source_file)
        || !profile_file_is_active_downloadable(profile, candidate)
    {
        return false;
    }
    let Some(source) = shape_index.for_file(source_file) else {
        return false;
    };
    let Some(candidate_shape) = shape_index.for_file(candidate) else {
        return false;
    };
    let source_alpha = alpha_atoms(&source.atoms);
    let candidate_alpha = alpha_atoms(&candidate_shape.atoms);
    if source_alpha.len() != candidate_alpha.len() {
        return false;
    }

    let mut differences = Vec::new();
    for (index, (left, right)) in source_alpha.iter().zip(&candidate_alpha).enumerate() {
        if left == right {
            continue;
        }
        let release_context = (left.starts_with(right) || right.starts_with(left))
            && direct_digit_follow(&source.atoms, index)
                == direct_digit_follow(&candidate_shape.atoms, index)
            && direct_digit_follow(&source.atoms, index).is_some();
        if !release_context {
            differences.push(index);
        }
    }
    if differences.len() != 1 {
        return false;
    }
    let slot = differences[0];
    if !source_alpha
        .iter()
        .enumerate()
        .any(|(index, _)| index != slot)
    {
        return false;
    }
    if !contrast_choice_tokens(source_alpha[slot], candidate_alpha[slot])
        || !contrast_context_equal_general(
            source,
            candidate_shape,
            &source_alpha,
            &candidate_alpha,
            slot,
        )
    {
        return false;
    }

    let source_frame = masked_catalog_frame(source, slot);
    let candidate_frame = masked_catalog_frame(candidate_shape, slot);
    let source_description = &source.description;
    let candidate_description = &candidate_shape.description;
    let mut source_frames = HashSet::from([source_frame.clone()]);
    let mut candidate_frames = HashSet::from([candidate_frame.clone()]);
    let mut source_metadata_keys = HashSet::new();
    let mut candidate_metadata_keys = HashSet::new();
    if !source_description.is_empty() {
        source_metadata_keys.insert((source_frame.clone(), source_description.clone()));
    }
    if !candidate_description.is_empty() {
        candidate_metadata_keys.insert((candidate_frame.clone(), candidate_description.clone()));
    }
    let mut metadata_peer_ids = HashSet::new();
    let mut third_choice_with_shared_label = false;

    for peer in &shape_index.files {
        if peer.file_id == source.file_id || peer.file_id == candidate_shape.file_id {
            continue;
        }
        let peer_alpha = alpha_atoms(&peer.atoms);
        if peer_alpha.len() != source_alpha.len()
            || !contrast_context_equal_general(source, peer, &source_alpha, &peer_alpha, slot)
        {
            continue;
        }
        let peer_choice = peer_alpha[slot];
        let peer_frame = masked_catalog_frame(peer, slot);
        if peer_choice == source_alpha[slot] {
            source_frames.insert(peer_frame.clone());
        }
        if peer_choice == candidate_alpha[slot] {
            candidate_frames.insert(peer_frame.clone());
        }
        if !peer.description.is_empty() {
            let peer_identity = if peer.file_id != 0 {
                (peer.file_id, String::new(), 0)
            } else {
                (0, peer.file_name.clone(), peer.date_added)
            };
            if metadata_peer_ids.insert(peer_identity) {
                let matches_source = contrast_choice_related(peer_choice, source_alpha[slot]);
                let matches_candidate = contrast_choice_related(peer_choice, candidate_alpha[slot]);
                if matches_source != matches_candidate {
                    let target = if matches_source {
                        &mut source_metadata_keys
                    } else {
                        &mut candidate_metadata_keys
                    };
                    target.insert((peer_frame.clone(), peer.description.clone()));
                }
            }
        }
        if source_frame == candidate_frame
            && !source_description.is_empty()
            && source_description == candidate_description
            && peer_frame == source_frame
            && contrast_choice_tokens(peer_choice, source_alpha[slot])
            && contrast_choice_tokens(peer_choice, candidate_alpha[slot])
            && peer.description == source_description.as_str()
        {
            third_choice_with_shared_label = true;
        }
    }

    let shared_frame_count = source_frames.intersection(&candidate_frames).count();
    let shared_metadata_key_count = source_metadata_keys
        .intersection(&candidate_metadata_keys)
        .count();
    let api_version_increased = source_version_for_refinement(tracked, source_file)
        .zip(canonical_numeric_version(candidate.version.as_deref()))
        .is_some_and(|(source_version, candidate_version)| candidate_version > source_version);
    shared_frame_count >= 2
        || shared_metadata_key_count >= 2
        || (third_choice_with_shared_label && !api_version_increased)
}

fn typed_filename_versions(file_name: &str) -> Vec<Vec<u64>> {
    let stem = collision_stripped_stem(file_name, false);
    let bytes = stem.as_bytes();
    let is_separator = |byte: u8| matches!(byte, b'.' | b'_' | b'-');
    let mut versions = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if !matches!(bytes[index], b'v' | b'V') {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        if cursor < bytes.len() && is_separator(bytes[cursor]) {
            cursor += 1;
        }
        if cursor >= bytes.len() || !bytes[cursor].is_ascii_digit() {
            index += 1;
            continue;
        }
        let mut version_text = String::new();
        loop {
            let digit_start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            version_text.push_str(&stem[digit_start..cursor]);
            if cursor + 1 < bytes.len()
                && is_separator(bytes[cursor])
                && bytes[cursor + 1].is_ascii_digit()
            {
                version_text.push(bytes[cursor] as char);
                cursor += 1;
                continue;
            }
            break;
        }
        if let Some(version) = canonical_numeric_version(Some(&version_text)) {
            versions.push(version);
        }
        index = cursor;
    }
    versions
}

static TYPED_RELEASE_MARKER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:ver|v)[._-]?[0-9]+(?:[._-][0-9]+)*")
        .expect("typed release marker regex")
});

static TYPED_RELEASE_API_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:(?:ver|v)[._-]?)?([0-9]+(?:[._-][0-9]+)*)$")
        .expect("typed release API regex")
});

#[derive(Debug, Clone)]
struct TypedReleaseMarker {
    span: (usize, usize),
    packed_components: String,
}

fn typed_release_collision_stem(file_name: &str) -> String {
    let mut stem = strip_file_extension(file_name).to_string();
    if let Some(upload_start) = stem.rfind("_-") {
        let suffix = &stem[upload_start + 2..];
        if suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            stem.truncate(upload_start);
        }
    }
    if let Some((prefix, suffix)) = hex_collision_suffix(&stem) {
        let mixed = suffix.bytes().any(|byte| byte.is_ascii_alphabetic())
            && suffix.bytes().any(|byte| byte.is_ascii_digit());
        if mixed {
            stem = prefix.trim_end_matches(['.', '_', '-', ' ']).to_string();
        }
    }
    stem
}

fn typed_release_marker(file_name: &str) -> Option<TypedReleaseMarker> {
    let stem = typed_release_collision_stem(file_name);
    let matches: Vec<_> = TYPED_RELEASE_MARKER_RE
        .find_iter(&stem)
        .filter(|matched| {
            !stem[..matched.start()]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_alphanumeric())
                && !stem[matched.end()..]
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_alphanumeric())
        })
        .collect();
    if matches.len() != 1 {
        return None;
    }
    let matched = matches[0];
    let marker = matched.as_str();
    let number_start = marker
        .char_indices()
        .find(|(_, character)| character.is_ascii_digit())
        .map(|(index, _)| index)?;
    let components: Vec<u64> = marker[number_start..]
        .split(['.', '_', '-'])
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    Some(TypedReleaseMarker {
        span: (matched.start(), matched.end()),
        packed_components: components
            .iter()
            .map(u64::to_string)
            .collect::<String>(),
    })
}

fn typed_release_api_version(value: Option<&str>) -> Option<Vec<u64>> {
    let value = value?.trim();
    let captures = TYPED_RELEASE_API_RE.captures(value)?;
    captures
        .get(1)?
        .as_str()
        .split(['.', '_', '-'])
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()
}

fn ordered_typed_release_version(version: &[u64]) -> Vec<u64> {
    let mut ordered = version.to_vec();
    while ordered.len() > 1 && ordered.last() == Some(&0) {
        ordered.pop();
    }
    ordered
}

fn typed_release_identity(
    file_name: &str,
    marker: &TypedReleaseMarker,
) -> Vec<String> {
    let stem = typed_release_collision_stem(file_name);
    let mut without_marker = String::with_capacity(stem.len().saturating_sub(
        marker.span.1.saturating_sub(marker.span.0),
    ));
    without_marker.push_str(&stem[..marker.span.0]);
    without_marker.push_str(&stem[marker.span.1..]);

    let mut runs = Vec::new();
    let mut current = String::new();
    for character in without_marker.chars() {
        if character.is_alphanumeric() {
            current.extend(character.to_lowercase());
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    if runs.is_empty()
        || !runs
            .iter()
            .flat_map(|run| run.chars())
            .any(|character| character.is_alphabetic())
    {
        Vec::new()
    } else {
        runs
    }
}

fn api_veto_guard(source_version: Option<&str>, candidate_version: Option<&str>) -> bool {
    let source_text = source_version.map(str::trim).filter(|version| !version.is_empty());
    let candidate_text = candidate_version.map(str::trim).filter(|version| !version.is_empty());
    match (source_text, candidate_text) {
        (None, None) => true,
        (Some(source), Some(candidate)) => {
            let Some(source) = canonical_numeric_version(Some(source)) else {
                return false;
            };
            let Some(candidate) = canonical_numeric_version(Some(candidate)) else {
                return false;
            };
            candidate <= source
        }
        _ => false,
    }
}

fn source_version_text_for_refinement<'a>(
    tracked: &'a TrackedFileMeta,
    source: &'a gamebanana::ModFile,
) -> Option<&'a str> {
    tracked
        .version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .or_else(|| {
            source
                .version
                .as_deref()
                .map(str::trim)
                .filter(|version| !version.is_empty())
        })
}

fn one_alpha_atom_insertion_or_deletion(
    left: &[(bool, String)],
    right: &[(bool, String)],
) -> bool {
    if right.len() != left.len() + 1 {
        return false;
    }
    (0..right.len()).any(|index| {
        right[index].0
            && right[..index]
                .iter()
                .chain(right[index + 1..].iter())
                .eq(left.iter())
    })
}

fn one_alpha_word_insertion_or_deletion(file_a: &str, file_b: &str) -> bool {
    let a = filename_atoms(file_a);
    let b = filename_atoms(file_b);
    one_alpha_atom_insertion_or_deletion(&a, &b)
        || one_alpha_atom_insertion_or_deletion(&b, &a)
}

fn profile_file_is_archived(
    profile: &gamebanana::ProfileResponse,
    file: &gamebanana::ModFile,
) -> bool {
    file.is_archived
        || profile
            .archived_files
            .iter()
            .any(|archived| remote_file_matches_tracked(archived, &tracked_file_meta_from_mod_file(file)))
}

fn profile_file_is_active_downloadable(
    profile: &gamebanana::ProfileResponse,
    file: &gamebanana::ModFile,
) -> bool {
    !profile_file_is_archived(profile, file)
        && file
            .download_url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty())
        && profile.files.iter().any(|active| {
            remote_file_matches_tracked(active, &tracked_file_meta_from_mod_file(file))
        })
}

fn source_version_for_refinement(
    tracked: &TrackedFileMeta,
    source: &gamebanana::ModFile,
) -> Option<Vec<u64>> {
    tracked
        .version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .and_then(|version| canonical_numeric_version(Some(version)))
        .or_else(|| canonical_numeric_version(source.version.as_deref()))
}

fn refinement_a_candidate_matches(
    tracked: &TrackedFileMeta,
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
    active_remote_files: &[&gamebanana::ModFile],
) -> bool {
    let Some(source) = fallback_source_file(tracked, all_profile_files) else {
        return false;
    };
    let source_date = (tracked.date_added > 0)
        .then_some(tracked.date_added)
        .unwrap_or(source.date_added);
    if candidate.date_added <= source_date
        || remote_file_matches_tracked(candidate, tracked)
        || !profile_file_is_active_downloadable(profile, candidate)
    {
        return false;
    }

    let source_version = source_version_for_refinement(tracked, source);
    let Some(candidate_version) = canonical_numeric_version(candidate.version.as_deref()) else {
        return false;
    };
    let Some(source_version) = source_version else {
        return false;
    };
    if candidate_version < source_version {
        return false;
    }

    let source_label = tracked_or_remote_label_text(tracked, source);
    let strict_api_increase = candidate_version > source_version;
    if !source_label.is_empty() && !(profile_file_is_archived(profile, source) && strict_api_increase) {
        return false;
    }

    let all_hex_peer_context = all_hex_context(source, candidate, all_profile_files);
    let source_key = exact_filename_key(&source.file_name, all_hex_peer_context);
    let candidate_key = exact_filename_key(&candidate.file_name, all_hex_peer_context);
    if !source_key.chars().any(|character| character.is_alphanumeric())
        || !candidate_key.chars().any(|character| character.is_alphanumeric())
        || source_key != candidate_key
    {
        return false;
    }

    let newer_active: Vec<_> = active_remote_files
        .iter()
        .copied()
        .filter(|file| file.date_added > source_date)
        .collect();
    newer_active.len() == 1
        && remote_file_matches_tracked(newer_active[0], &tracked_file_meta_from_mod_file(candidate))
}

fn typed_release_candidate_matches(
    tracked: &TrackedFileMeta,
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
    active_remote_files: &[&gamebanana::ModFile],
) -> bool {
    let Some(source) = fallback_source_file(tracked, all_profile_files) else {
        return false;
    };
    let source_date = (tracked.date_added > 0)
        .then_some(tracked.date_added)
        .unwrap_or(source.date_added);
    if candidate.date_added <= source_date
        || candidate.id == source.id
        || remote_file_matches_tracked(candidate, tracked)
        || !profile_file_is_active_downloadable(profile, candidate)
        || !tracked_or_remote_label_text(tracked, source).is_empty()
    {
        return false;
    }

    let Some(source_marker) = typed_release_marker(&source.file_name) else {
        return false;
    };
    let Some(candidate_marker) = typed_release_marker(&candidate.file_name) else {
        return false;
    };
    let Some(source_api) = typed_release_api_version(source_version_text_for_refinement(tracked, source)) else {
        return false;
    };
    let Some(candidate_api) = typed_release_api_version(candidate.version.as_deref()) else {
        return false;
    };
    if ordered_typed_release_version(&candidate_api) <= ordered_typed_release_version(&source_api) {
        return false;
    }
    let pack = |components: &[u64]| components.iter().map(u64::to_string).collect::<String>();
    if source_marker.packed_components != pack(&source_api)
        || candidate_marker.packed_components != pack(&candidate_api)
    {
        return false;
    }

    let source_identity = typed_release_identity(&source.file_name, &source_marker);
    let candidate_identity = typed_release_identity(&candidate.file_name, &candidate_marker);
    if source_identity.is_empty() || source_identity != candidate_identity {
        return false;
    }

    let mut newer_same_identity = Vec::new();
    for file in active_remote_files.iter().copied().filter(|file| {
        file.date_added > source_date && profile_file_is_active_downloadable(profile, file)
    }) {
        let Some(marker) = typed_release_marker(&file.file_name) else {
            continue;
        };
        if typed_release_identity(&file.file_name, &marker) != source_identity {
            continue;
        }
        if newer_same_identity.iter().any(|existing: &&gamebanana::ModFile| {
            remote_file_matches_tracked(file, &tracked_file_meta_from_mod_file(existing))
        }) {
            continue;
        }
        newer_same_identity.push(file);
    }
    newer_same_identity.len() == 1
        && remote_file_matches_tracked(
            newer_same_identity[0],
            &tracked_file_meta_from_mod_file(candidate),
        )
}

#[cfg(test)]
fn package_scope_veto(
    tracked: &TrackedFileMeta,
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
) -> bool {
    package_scope_veto_with_shapes(tracked, candidate, profile, all_profile_files, None)
}

fn package_scope_veto_with_shapes(
    tracked: &TrackedFileMeta,
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
    shape_index: Option<&CatalogShapeIndex>,
) -> bool {
    let Some(source) = fallback_source_file(tracked, all_profile_files) else {
        return false;
    };
    if !profile_file_is_active_downloadable(profile, source)
        || !profile_file_is_active_downloadable(profile, candidate)
    {
        return false;
    }

    if !api_veto_guard(
        source_version_text_for_refinement(tracked, source),
        candidate.version.as_deref(),
    ) {
        return false;
    }

    let one_word = shape_index
        .and_then(|index| {
            Some((
                index.for_file(source)?.atoms.as_slice(),
                index.for_file(candidate)?.atoms.as_slice(),
            ))
        })
        .map(|(source_atoms, candidate_atoms)| {
            one_alpha_atom_insertion_or_deletion(source_atoms, candidate_atoms)
                || one_alpha_atom_insertion_or_deletion(candidate_atoms, source_atoms)
        })
        .unwrap_or_else(|| one_alpha_word_insertion_or_deletion(&source.file_name, &candidate.file_name));
    let source_size = source.file_size;
    let candidate_size = candidate.file_size;
    let smaller = source_size.min(candidate_size);
    let larger = source_size.max(candidate_size);
    let size_extreme = source_size > 0
        && candidate_size > 0
        && smaller <= 64 * 1024
        && larger >= smaller.saturating_mul(100);
    let source_release = typed_filename_versions(&source.file_name).first().cloned();
    let candidate_release = typed_filename_versions(&candidate.file_name).first().cloned();
    let filename_version_increased = matches!(
        (source_release, candidate_release),
        (Some(source), Some(candidate)) if candidate > source
    );
    one_word && size_extreme && !filename_version_increased
}

fn all_best_sources_veto(
    tracked_files: &[TrackedFileMeta],
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
    shape_index: &CatalogShapeIndex,
) -> bool {
    !tracked_files.is_empty()
        && tracked_files.iter().all(|tracked| {
            package_scope_veto_with_shapes(
                tracked,
                candidate,
                profile,
                all_profile_files,
                Some(shape_index),
            ) || catalog_contrast_veto(
                tracked,
                candidate,
                profile,
                all_profile_files,
                shape_index,
            )
        })
}

fn fallback_source_file<'a>(
    tracked: &TrackedFileMeta,
    all_profile_files: &[&'a gamebanana::ModFile],
) -> Option<&'a gamebanana::ModFile> {
    all_profile_files
        .iter()
        .copied()
        .find(|file| {
            if tracked.file_id != 0 {
                file.id == tracked.file_id
            } else {
                file.file_name == tracked.file_name && file.date_added == tracked.date_added
            }
        })
}

fn fallback_candidate_matches(
    tracked: &TrackedFileMeta,
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
    active_remote_files: &[&gamebanana::ModFile],
) -> bool {
    if refinement_a_candidate_matches(
        tracked,
        candidate,
        profile,
        all_profile_files,
        active_remote_files,
    ) {
        return true;
    }
    if typed_release_candidate_matches(
        tracked,
        candidate,
        profile,
        all_profile_files,
        active_remote_files,
    ) {
        return true;
    }
    let Some(source_file) = fallback_source_file(tracked, all_profile_files) else {
        return false;
    };
    let source_date = (tracked.date_added > 0)
        .then_some(tracked.date_added)
        .unwrap_or(source_file.date_added);
    if candidate.date_added <= source_date || remote_file_matches_tracked(candidate, tracked) {
        return false;
    }

    let source_label_present = tracked
        .label
        .as_deref()
        .is_some_and(|label| !label.trim().is_empty())
        || source_file
            .description
            .as_deref()
            .is_some_and(|description| !description.trim().is_empty());
    if source_label_present {
        return false;
    }

    let source_version = tracked
        .version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .or_else(|| {
            source_file
                .version
                .as_deref()
                .map(str::trim)
                .filter(|version| !version.is_empty())
        });
    let Some(source_version) = canonical_numeric_version(source_version) else {
        return false;
    };
    let Some(candidate_version) = canonical_numeric_version(candidate.version.as_deref()) else {
        return false;
    };
    if candidate_version < source_version {
        return false;
    }
    if candidate_version == source_version
        && !equal_version_digit_runs_are_preserved(&tracked.file_name, &candidate.file_name)
    {
        return false;
    }

    let newer_active: Vec<_> = active_remote_files
        .iter()
        .copied()
        .filter(|file| {
            file.download_url
                .as_deref()
                .is_some_and(|url| !url.trim().is_empty())
                && file.date_added > source_date
        })
        .collect();
    if newer_active.len() != 1
        || !remote_file_matches_tracked(
            newer_active[0],
            &tracked_file_meta_from_mod_file(candidate),
        )
    {
        return false;
    }

    let source_alpha = normalized_alphabetic_stem(&tracked.file_name);
    let candidate_alpha = normalized_alphabetic_stem(&candidate.file_name);
    raw_source_name_coverage(&tracked.file_name, &candidate.file_name)
        >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD
        && !source_alpha.is_empty()
        && !candidate_alpha.is_empty()
        && candidate_alpha.contains(&source_alpha)
        && token_identity_condition(&tracked.file_name, &candidate.file_name)
}

/// Uses a complete GameBanana update history as a last-resort replacement
/// signal when the normal filename/version fallbacks cannot identify a
/// renamed file. Every catalog file must be accounted for by a single-file
/// release chain; incomplete or conflicting file associations remain neutral.
fn release_sequence_candidate_matches(
    tracked: &TrackedFileMeta,
    candidate: &gamebanana::ModFile,
    profile: &gamebanana::ProfileResponse,
    all_profile_files: &[&gamebanana::ModFile],
) -> bool {
    let Some(history) = profile.update_history.as_ref() else {
        return false;
    };
    if history.records.is_empty() {
        return false;
    }

    let Some(source) = fallback_source_file(tracked, all_profile_files) else {
        return false;
    };
    // The installed file may have been moved out of the active catalog after
    // the author published its replacement. Archived membership is authoritative
    // even when the archived payload's own flag or URL is stale/missing.
    let source_is_archived = profile_file_is_archived(profile, source);
    if !(source_is_archived || profile_file_is_active_downloadable(profile, source))
        || !profile_file_is_active_downloadable(profile, candidate)
        || source.id == candidate.id
    {
        return false;
    }

    let mut catalog = HashMap::<u64, &gamebanana::ModFile>::new();
    for file in all_profile_files {
        catalog.entry(file.id).or_insert(*file);
    }
    if !catalog.contains_key(&source.id) || !catalog.contains_key(&candidate.id) {
        return false;
    }

    // Keep the log's original spelling for conflict checks, while using the
    // existing numeric ordering helper for release progression.
    let parse_release_version = |value: Option<&str>| {
        let raw = value?.trim();
        if raw.is_empty()
            || raw.split('.').any(|part| {
                part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            return None;
        }
        let version = raw
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        Some((version, raw.to_owned()))
    };

    // Each tuple is (record date, file date, file id, numeric version,
    // original version text). Unattached prose records are intentionally
    // ignored; an attached malformed record invalidates the chain.
    let mut records = Vec::new();
    for record in &history.records {
        let mut ids = match (record.file_ids.as_ref(), record.files.as_ref()) {
            (None, None) => continue,
            (Some(file_ids), None) => file_ids.clone(),
            (None, Some(files)) => files.iter().map(|file| file.id).collect(),
            (Some(file_ids), Some(files)) => {
                let mut listed_ids = file_ids.clone();
                let mut embedded_ids: Vec<_> = files.iter().map(|file| file.id).collect();
                listed_ids.sort_unstable();
                embedded_ids.sort_unstable();
                if listed_ids != embedded_ids {
                    return false;
                }
                embedded_ids
            }
        };
        if ids.is_empty() {
            continue;
        }
        ids.sort_unstable();
        if ids.windows(2).any(|pair| pair[0] == pair[1])
            || ids.iter().any(|id| *id == 0 || !catalog.contains_key(id))
        {
            return false;
        }
        if !record.is_public() || !record.attachments_usable {
            return false;
        }
        let Some((version, version_raw)) = parse_release_version(record.version.as_deref()) else {
            return false;
        };
        if ids.len() != 1 || record.date_added <= 0 {
            return false;
        }
        let file_id = ids[0];
        let catalog_file = catalog[&file_id];

        if let Some(file_version) = catalog_file
            .version
            .as_deref()
            .map(str::trim)
            .filter(|version| !version.is_empty())
        {
            let Some((file_numeric, file_raw)) = parse_release_version(Some(file_version)) else {
                return false;
            };
            if file_numeric != version || file_raw != version_raw {
                return false;
            }
        }
        if let Some(files) = record.files.as_ref() {
            let Some(embedded) = files.iter().find(|file| file.id == file_id) else {
                return false;
            };
            if let Some(file_version) = embedded
                .version
                .as_deref()
                .map(str::trim)
                .filter(|version| !version.is_empty())
            {
                let Some((file_numeric, file_raw)) = parse_release_version(Some(file_version)) else {
                    return false;
                };
                if file_numeric != version || file_raw != version_raw {
                    return false;
                }
            }
        }
        records.push((
            record.date_added,
            catalog_file.date_added,
            file_id,
            version,
            version_raw,
        ));
    }
    if records.is_empty() {
        return false;
    }

    let mut attached = HashMap::<u64, usize>::new();
    for (_, _, file_id, _, _) in &records {
        *attached.entry(*file_id).or_default() += 1;
    }
    let all_catalog_ids: HashSet<u64> = catalog.keys().copied().collect();
    if attached.values().any(|count| *count != 1) {
        return false;
    }

    // A complete history may omit the original upload, but only one such file
    // is acceptable and it must be the oldest unlabeled catalog file. Every
    // other catalog file must be represented by exactly one update record.
    let unassociated_ids: Vec<_> = all_catalog_ids
        .iter()
        .filter(|id| !attached.contains_key(id))
        .copied()
        .collect();
    if unassociated_ids.len() > 1 {
        return false;
    }
    let unassociated_root = unassociated_ids.first().and_then(|id| catalog.get(id).copied());

    records.sort_by_key(|(record_date, _, _, _, _)| *record_date);
    if records.windows(2).any(|pair| pair[0].0 >= pair[1].0 || pair[0].1 >= pair[1].1) {
        return false;
    }
    if let Some(root) = unassociated_root {
        let first_record_file_date = records.first().map(|record| record.1).unwrap_or(0);
        let first_record_date = records.first().map(|record| record.0).unwrap_or(0);
        if root.date_added <= 0
            || first_record_file_date <= root.date_added
            || first_record_date <= root.date_added
        {
            return false;
        }
        if root
            .description
            .as_deref()
            .is_some_and(|label| !label.trim().is_empty())
            || (root.id == source.id
                && tracked
                    .label
                    .as_deref()
                    .is_some_and(|label| !label.trim().is_empty()))
        {
            return false;
        }
    }

    // Authors sometimes leave the same version on successive files. Dates and
    // one-file attachments establish the order, but a version decrease makes
    // the whole release history unusable.
    if records.windows(2).any(|pair| {
        ordered_typed_release_version(&pair[0].3)
            > ordered_typed_release_version(&pair[1].3)
    }) {
        return false;
    }

    let source_associated = attached.contains_key(&source.id);
    let source_record_index = records
        .iter()
        .position(|(_, _, file_id, _, _)| *file_id == source.id);
    let source_record_version = source_record_index.map(|index| records[index].3.clone());
    if source_associated != source_record_index.is_some() {
        return false;
    }
    if let Some(root) = unassociated_root {
        if source_associated && root.id == source.id {
            return false;
        }
        if !source_associated && root.id != source.id {
            return false;
        }
    } else if !source_associated {
        return false;
    }

    // The installed file may be the original upload or any logged release.
    // Only records after that point can be candidates; the latest such record
    // is the only file this helper is allowed to select.
    let release_records: Vec<_> = match source_record_index {
        Some(index) => records.iter().skip(index + 1).collect(),
        None => records.iter().collect(),
    };
    if release_records.is_empty() {
        return false;
    }
    let source_date = if source.date_added > 0 {
        source.date_added
    } else {
        tracked.date_added
    };
    if source_date <= 0 || release_records.iter().any(|item| item.1 <= source_date) {
        return false;
    }
    if !source_associated
        && release_records
            .iter()
            .map(|item| item.0.min(item.1))
            .min()
            .is_some_and(|earliest| source_date >= earliest)
    {
        return false;
    }

    let source_version = tracked
        .version
        .as_deref()
        .and_then(|version| parse_release_version(Some(version)).map(|value| value.0))
        .or_else(|| {
            source
                .version
                .as_deref()
                .and_then(|version| parse_release_version(Some(version)).map(|value| value.0))
        })
        .or(source_record_version);
    if let Some(source_version) = source_version {
        let first_release_version = ordered_typed_release_version(&release_records[0].3);
        let installed_version = ordered_typed_release_version(&source_version);
        if first_release_version < installed_version
            || (!source_associated
                && first_release_version == installed_version
                && release_records.len() < 2)
        {
            return false;
        }
    } else if !source_associated && release_records.len() < 2 {
        // An unassociated root with no numeric version has no evidence that a
        // single attached record is its successor. Require two later
        // one-file records so the serial chain itself supplies that proof.
        // An associated source may still advance on one record.
        return false;
    }

    let active_ids: HashSet<u64> = profile
        .files
        .iter()
        .filter(|file| profile_file_is_active_downloadable(profile, file))
        .map(|file| file.id)
        .collect();
    // With an archived source, the only active file may be the latest release.
    // An active source still requires a separate active candidate as before.
    if active_ids.is_empty()
        || (!source_is_archived
            && (active_ids.len() <= 1 || active_ids.iter().all(|id| *id == source.id)))
    {
        return false;
    }
    let latest = release_records.last().expect("checked non-empty");
    latest.2 == candidate.id && active_ids.contains(&candidate.id)
}

/// Archive path lists the matcher could still use for these recipes: the
/// installed file and every eligible candidate of each single-source recipe
/// that the metadata model left undecided.  Lists already attached to the
/// profile are left out.
fn file_listing_ids_wanted(
    items: &[(Option<i64>, FileSetRecipe)],
    profile: &gamebanana::ProfileResponse,
) -> Vec<u64> {
    if gamebanana::is_unavailable(profile)
        || !matcher_adapter::canonicalize_profile(profile)
            .ambiguous_ids
            .is_empty()
    {
        return Vec::new();
    }
    let Some(group) = matcher_adapter::evaluate_native_single_group(items, profile) else {
        return Vec::new();
    };
    let mut ids: Vec<u64> = group
        .evaluations
        .into_iter()
        .flat_map(|evaluation| evaluation.missing_listing_ids)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
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

    // Canonicalize the complete catalog before either resolver runs.  This
    // makes archived membership authoritative and prevents duplicate IDs from
    // manufacturing a release proof.  The native single-file adapter receives
    // the same canonical profile and is terminal on abstention; only recipes
    // outside its one-source scope continue through the conservative legacy
    // resolver below.
    let canonical_profile = matcher_adapter::canonicalize_profile(profile);
    if !canonical_profile.ambiguous_ids.is_empty() {
        return items
            .iter()
            .map(|_| FileSetUpdateEvaluation {
                state: ModUpdateState::CheckSkipped,
                signature: None,
            })
            .collect();
    }
    // Keep the native adapter's canonicalization anchored to the original
    // payload.  The outer canonical profile is used by the legacy resolver;
    // passing it into the adapter would discard the ambiguity provenance.
    let mut native_evaluations = matcher_adapter::evaluate_native_single_group(items, profile)
        .map(|group| group.evaluations)
        .unwrap_or_else(|| vec![matcher_adapter::NativeSingleEvaluation::default(); items.len()]);
    let profile = &canonical_profile.profile;
    let native_single_indices: HashSet<usize> = native_evaluations
        .iter()
        .enumerate()
        .filter_map(|(index, evaluation)| evaluation.is_native_single.then_some(index))
        .collect();

    let all_remote_files = all_profile_files(profile);
    let profile_files = all_profile_files(profile);
    let active_remote_files = downloadable_active_files(profile);
    let shape_files: Vec<_> = active_remote_files
        .iter()
        .copied()
        .filter(|file| profile_file_is_active_downloadable(profile, file))
        .collect();
    let shape_index = CatalogShapeIndex::from_active_files(&shape_files);
    let resolved: Vec<_> = items
        .iter()
        .map(|(_, file_set)| resolve_tracked_files(file_set, &all_remote_files))
        .collect();
    let fallback_resolved: Vec<_> = items
        .iter()
        .map(|(_, file_set)| resolve_fallback_tracked_files(file_set, &profile_files))
        .collect();
    let all_tracked_files: Vec<_> = resolved
        .iter()
        .enumerate()
        .filter(|(index, _)| !native_single_indices.contains(index))
        .filter_map(|(_, resolved)| match resolved {
            ResolvedTrackedFiles::Tracked(files) => Some(files.as_slice()),
            ResolvedTrackedFiles::MissingSource
            | ResolvedTrackedFiles::Invalid
            | ResolvedTrackedFiles::Untracked => None,
        })
        .flatten()
        .cloned()
        .collect();
    let common_prefix = lineage_common_prefix(&all_tracked_files);
    let mut assigned_candidates: Vec<Vec<TrackedFileMeta>> = vec![Vec::new(); items.len()];
    let mut release_sequence_candidates: Vec<(&gamebanana::ModFile, Vec<Vec<TrackedFileMeta>>)> =
        Vec::new();
    let mut release_priority_candidates: Vec<(&gamebanana::ModFile, Vec<Vec<TrackedFileMeta>>)> =
        Vec::new();

    for candidate in active_remote_files.iter().copied() {
        // Collect verified chain claims before the softer filename/label
        // matchers run. A single-file recipe may later promote its unique
        // claim over a filename winner; multi-file recipes retain the legacy
        // lower-priority behavior below.
        let release_matches: Vec<Vec<TrackedFileMeta>> = fallback_resolved
            .iter()
            .enumerate()
            .map(|(idx, tracked_files)| {
                if native_single_indices.contains(&idx) {
                    return Vec::new();
                }
                match tracked_files {
                    Some(tracked_files) => tracked_files
                        .iter()
                        .filter(|tracked| {
                            release_sequence_candidate_matches(
                                tracked,
                                candidate,
                                profile,
                                &profile_files,
                            )
                        })
                        .cloned()
                        .collect(),
                    None => Vec::new(),
                }
            })
            .collect();
        if release_matches.iter().any(|matches| !matches.is_empty()) {
            release_priority_candidates.push((candidate, release_matches.clone()));
        }

        let mut scores = Vec::with_capacity(resolved.len());
        for (idx, resolved_files) in resolved.iter().enumerate() {
            if native_single_indices.contains(&idx) {
                scores.push((idx, 0.0));
                continue;
            }
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
                ResolvedTrackedFiles::MissingSource
                | ResolvedTrackedFiles::Invalid
                | ResolvedTrackedFiles::Untracked => 0.0,
            };
            scores.push((idx, best_score));
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let Some((best_idx, best_score)) = scores.first().copied() else {
            continue;
        };
        let runner_up_score = scores.get(1).map(|(_, score)| *score).unwrap_or(0.0);
        if best_score >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD {
            if runner_up_score <= 0.0
                || best_score - runner_up_score >= FILE_LINEAGE_AUTO_MATCH_MARGIN
            {
                let best_sources: Vec<_> = match &resolved[best_idx] {
                    ResolvedTrackedFiles::Tracked(tracked_files) => tracked_files
                        .iter()
                        .filter(|tracked| {
                            candidate.date_added > tracked.date_added
                                && !remote_file_matches_tracked(candidate, tracked)
                                && {
                                    let score = if file_label_lineage_matches(tracked, candidate) {
                                        1.0
                                    } else {
                                        file_lineage_similarity(
                                            &tracked.file_name,
                                            &candidate.file_name,
                                            &common_prefix,
                                        )
                                    };
                                    (score - best_score).abs() <= f32::EPSILON
                                }
                        })
                        .cloned()
                        .collect(),
                    ResolvedTrackedFiles::MissingSource
                    | ResolvedTrackedFiles::Invalid
                    | ResolvedTrackedFiles::Untracked => {
                        Vec::new()
                    }
                };
                if !all_best_sources_veto(
                    &best_sources,
                    candidate,
                    profile,
                    &profile_files,
                    &shape_index,
                ) {
                    assigned_candidates[best_idx].push(tracked_file_meta_from_mod_file(candidate));
                }
            }
            continue;
        }

        let fallback_matches: Vec<Vec<TrackedFileMeta>> = fallback_resolved
            .iter()
            .enumerate()
            .map(|(idx, tracked_files)| {
                if native_single_indices.contains(&idx) {
                    return Vec::new();
                }
                match tracked_files {
                    Some(tracked_files) => tracked_files
                        .iter()
                        .filter(|tracked| {
                            fallback_candidate_matches(
                                tracked,
                                candidate,
                                profile,
                                &profile_files,
                                &active_remote_files,
                            )
                        })
                        .cloned()
                        .collect(),
                    None => Vec::new(),
                }
            })
            .collect();
        let mut fallback_scores = Vec::with_capacity(resolved.len());
        for (idx, _tracked_files) in fallback_resolved.iter().enumerate() {
            let fallback_score = (!fallback_matches[idx].is_empty()).then_some(1.0).unwrap_or(0.0);
            fallback_scores.push((idx, fallback_score));
        }
        fallback_scores
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let Some((fallback_idx, fallback_score)) = fallback_scores.first().copied() else {
            continue;
        };
        let fallback_runner_up = fallback_scores
            .get(1)
            .map(|(_, score)| *score)
            .unwrap_or(0.0);
        if fallback_score >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD
            && (fallback_runner_up <= 0.0
                || fallback_score - fallback_runner_up >= FILE_LINEAGE_AUTO_MATCH_MARGIN)
        {
            if !all_best_sources_veto(
                &fallback_matches[fallback_idx],
                candidate,
                profile,
                &profile_files,
                &shape_index,
            ) {
                assigned_candidates[fallback_idx].push(tracked_file_meta_from_mod_file(candidate));
            }
            // A current fallback match, including one rejected by a veto or
            // left tied between groups, must keep precedence over the log
            // sequence. The latter is collected only for wholly unmatched
            // candidates and evaluated in a second stage below.
            continue;
        }
        if fallback_matches.iter().any(|matches| !matches.is_empty()) {
            continue;
        }

        if release_matches.iter().any(|matches| !matches.is_empty()) {
            release_sequence_candidates.push((candidate, release_matches));
        }
    }

    // A complete one-file release chain is stronger evidence than a merely
    // similar filename. Promote it only for recipes with exactly one tracked
    // source. Multi-file recipes retain the established conservative path,
    // and conflicting release claims abstain instead of assigning one
    // download twice.
    let mut unique_claims: Vec<Vec<&gamebanana::ModFile>> = vec![Vec::new(); items.len()];
    for (candidate, release_matches) in &release_priority_candidates {
        let matching_groups: Vec<_> = release_matches
            .iter()
            .enumerate()
            .filter(|(_, matches)| !matches.is_empty())
            .map(|(idx, _)| idx)
            .collect();
        let Some(&release_idx) = matching_groups.first() else {
            continue;
        };
        if matching_groups.len() != 1
            || !matches!(
                resolved.get(release_idx),
                Some(ResolvedTrackedFiles::Tracked(tracked_files)) if tracked_files.len() == 1
            )
            || release_matches[release_idx].len() != 1
            || all_best_sources_veto(
                &release_matches[release_idx],
                candidate,
                profile,
                &profile_files,
                &shape_index,
            )
        {
            continue;
        }
        let owned_elsewhere = assigned_candidates
            .iter()
            .enumerate()
            .any(|(idx, assigned)| {
                idx != release_idx
                    && assigned.iter().any(|tracked| tracked.file_id == candidate.id)
            });
        if !owned_elsewhere {
            unique_claims[release_idx].push(*candidate);
        }
    }
    for (release_idx, claims) in unique_claims.into_iter().enumerate() {
        if claims.len() == 1 {
            assigned_candidates[release_idx] = vec![tracked_file_meta_from_mod_file(claims[0])];
        }
    }

    // The update log is deliberately lower priority than every existing
    // matcher. It may fill an otherwise unmatched group, but cannot displace
    // a current winner or resolve a current tie.
    for (candidate, release_matches) in &release_sequence_candidates {
        let mut release_scores: Vec<_> = release_matches
            .iter()
            .enumerate()
            .map(|(idx, matches)| {
                let score = (!matches.is_empty()).then_some(1.0).unwrap_or(0.0);
                (idx, score)
            })
            .collect();
        release_scores
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let Some((release_idx, release_score)) = release_scores.first().copied() else {
            continue;
        };
        let release_runner_up = release_scores
            .get(1)
            .map(|(_, score)| *score)
            .unwrap_or(0.0);
        if release_score >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD
            && (release_runner_up <= 0.0
                || release_score - release_runner_up >= FILE_LINEAGE_AUTO_MATCH_MARGIN)
            && assigned_candidates[release_idx].is_empty()
            && !all_best_sources_veto(
                &release_matches[release_idx],
                candidate,
                profile,
                &profile_files,
                &shape_index,
            )
        {
            assigned_candidates[release_idx].push(tracked_file_meta_from_mod_file(candidate));
        }
    }

    // Reserve every viable legacy graph claim before native arbitration.  A
    // multi-file recipe may already have a different soft assignment, but its
    // still-viable release claim must continue to block a native sibling from
    // taking the same candidate.  Otherwise an occupied multi-file group would
    // incorrectly suppress the evidence of a competing claim.
    let mut legacy_potential_candidate_ids = HashSet::new();
    for (candidate, release_matches) in release_priority_candidates
        .iter()
        .chain(release_sequence_candidates.iter())
    {
        let has_viable_legacy_claim = release_matches
            .iter()
            .enumerate()
            .any(|(index, matches)| {
                !native_single_indices.contains(&index)
                    && !matches.is_empty()
                    && !all_best_sources_veto(
                        matches,
                        candidate,
                        profile,
                        &profile_files,
                        &shape_index,
                    )
            });
        if has_viable_legacy_claim {
            legacy_potential_candidate_ids.insert(candidate.id);
        }
    }

    // Candidate ownership is group-wide.  A native choice that collides with
    // another native recipe or with a legacy multi-file assignment abstains;
    // the legacy recipe loses its complete assignment as well.  This keeps a
    // shared candidate from being silently installed into two recipes and
    // prevents a partial multi-file update.
    let native_candidate_ids: HashSet<u64> = native_evaluations
        .iter()
        .filter_map(|evaluation| evaluation.claim.map(|claim| claim.candidate_id))
        .collect();
    let mut legacy_owners: HashMap<u64, Vec<usize>> = HashMap::new();
    for (index, assigned) in assigned_candidates.iter().enumerate() {
        if native_single_indices.contains(&index) {
            continue;
        }
        for candidate in assigned {
            if candidate.file_id != 0 {
                legacy_owners
                    .entry(candidate.file_id)
                    .or_default()
                    .push(index);
            }
        }
    }
    let mut conflicted_legacy = HashSet::new();
    let mut legacy_competing_native = HashSet::new();
    for (candidate_id, owners) in &legacy_owners {
        let distinct_owners: HashSet<_> = owners.iter().copied().collect();
        if distinct_owners.len() > 1 {
            conflicted_legacy.extend(distinct_owners.iter().copied());
        }
        if native_candidate_ids.contains(candidate_id) {
            conflicted_legacy.extend(distinct_owners.iter().copied());
            legacy_competing_native.insert(*candidate_id);
        }
    }
    legacy_competing_native.extend(
        legacy_potential_candidate_ids
            .intersection(&native_candidate_ids)
            .copied(),
    );
    for index in &conflicted_legacy {
        assigned_candidates[*index].clear();
    }
    matcher_adapter::abstain_competing_native_claims(
        &mut native_evaluations,
        &legacy_competing_native,
    );

    items
        .iter()
        .zip(resolved.iter())
        .enumerate()
        .map(|(idx, ((local_ts, _), resolved_files))| {
            if native_single_indices.contains(&idx) {
                return native_evaluations[idx].evaluation.clone();
            }
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
                ResolvedTrackedFiles::Invalid => FileSetUpdateEvaluation {
                    state: ModUpdateState::CheckSkipped,
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

#[cfg(test)]
fn compute_update_signature(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    evaluate_file_set_update_group(
        &[(selected_file_baseline_ts(file_set), file_set.clone())],
        profile,
    )
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

fn prearm_next_update_signature(mut signature: IgnoredUpdateSignature) -> IgnoredUpdateSignature {
    signature.prearmed_next_update = true;
    signature
}

fn current_remote_signature(
    file_set: &FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> Option<IgnoredUpdateSignature> {
    let canonical_profile = matcher_adapter::canonicalize_profile(profile);
    let profile = &canonical_profile.profile;
    if !selector_shape_is_valid(file_set)
        || selectors_conflict_with_catalog(file_set, &all_profile_files(profile))
    {
        return None;
    }
    let all_remote_files: Vec<&gamebanana::ModFile> = profile
        .files
        .iter()
        .chain(profile.archived_files.iter())
        .filter(|file| {
            file.id != 0
                && file
                    .download_url
                    .as_deref()
                    .is_some_and(|url| !url.trim().is_empty())
        })
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
                        if tracked.file_id != 0 {
                            file.id == tracked.file_id
                        } else {
                            file.file_name == tracked.file_name
                                && file.date_added == tracked.date_added
                        }
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
        // IDs are authoritative. Never fall through to a same-name file with
        // a different ID after a source was deleted and reuploaded.
        return profile_update_signature(profile).map(prearm_next_update_signature);
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

#[cfg(test)]
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
        .map(|profile| matcher_adapter::canonicalize_profile(&profile).profile)
        .or_else(|| {
            source
                .snapshot
                .as_ref()
                .map(|snapshot| {
                    profile_to_response(
                        Some(snapshot),
                        source.gamebanana.as_ref().map(|link| link.mod_id).unwrap_or(0),
                    )
                })
                .map(|profile| matcher_adapter::canonicalize_profile(&profile).profile)
        })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CachedProfileFreshness {
    /// A recorded cache write is stronger than any legacy remote date. This
    /// prevents an old profile with a malformed future file date from
    /// displacing a newer observed cache.
    observed_at: Option<DateTime<Utc>>,
    legacy_remote_timestamp: i64,
}

/// Returns the newest remote timestamp available in a cached profile. Legacy
/// source records have no cache-write timestamp, so their profile and file
/// dates are the only freshness evidence available.
fn cached_profile_freshness(
    source: &ModSourceData,
    profile: &gamebanana::ProfileResponse,
) -> CachedProfileFreshness {
    CachedProfileFreshness {
        observed_at: source.profile_cached_at,
        legacy_remote_timestamp: profile
            .date_updated
            .or(Some(profile.date_modified))
            .into_iter()
            .chain(std::iter::once(profile.date_added))
            .chain(
                profile
                    .files
                    .iter()
                    .chain(profile.archived_files.iter())
                    .map(|file| file.date_added)
                    .max(),
            )
            .max()
            .unwrap_or(0),
    }
}

fn cached_profile_catalog_fingerprint(profile: &gamebanana::ProfileResponse) -> String {
    profile_catalog_fingerprint(profile, true)
}

/// Without history the page reads as if GameBanana's update history had not
/// been loaded, so the same page fingerprints the same either way.
fn profile_catalog_fingerprint(
    profile: &gamebanana::ProfileResponse,
    with_history: bool,
) -> String {
    let update_history = profile.update_history.as_ref().filter(|_| with_history);
    let mut files: Vec<_> = profile
        .files
        .iter()
        .map(|file| matching_catalog_file_fingerprint(file, false))
        .chain(
            profile
                .archived_files
                .iter()
                .map(|file| matching_catalog_file_fingerprint(file, true)),
        )
        .collect();
    files.sort_by_key(|file| serde_json::to_string(file).unwrap_or_default());

    let history = update_history
        .map(|history| {
            let mut records: Vec<_> = history
                .records
                .iter()
                .map(matching_release_record_fingerprint)
                .collect();
            records.sort_by_key(|record| serde_json::to_string(record).unwrap_or_default());
            records
        })
        .unwrap_or_default();
    serde_json::to_string(&serde_json::json!({
        "id": profile.id,
        "date_added": profile.date_added,
        "date_modified": profile.date_modified,
        "date_updated": profile.date_updated,
        "private": profile.is_private,
        "deleted": profile.is_deleted,
        "trashed": profile.is_trashed,
        "withheld": profile.is_withheld,
        "files": files,
        "history_known": update_history.is_some(),
        "history": history,
    }))
    .unwrap_or_default()
}

fn matching_catalog_file_fingerprint(
    file: &gamebanana::ModFile,
    archived_bucket: bool,
) -> serde_json::Value {
    serde_json::json!({
        "id": file.id,
        "name": file.file_name,
        "date": file.date_added,
        "size": file.file_size,
        "version": file.version,
        "description": file.description,
        "archived": archived_bucket || file.is_archived,
        "download_url_present": file.download_url.is_some(),
        "downloadable": file.download_url.as_deref().is_some_and(|url| !url.trim().is_empty()),
    })
}

fn matching_release_record_fingerprint(
    record: &gamebanana::UpdateRecord,
) -> serde_json::Value {
    let mut file_ids = record.file_ids.clone().unwrap_or_default();
    file_ids.sort_unstable();
    let mut files: Vec<_> = record
        .files
        .as_ref()
        .into_iter()
        .flatten()
        .map(|file| matching_catalog_file_fingerprint(file, file.is_archived))
        .collect();
    files.sort_by_key(|file| serde_json::to_string(file).unwrap_or_default());
    serde_json::json!({
        "id": record.id,
        "name": record.name,
        "date_modified": record.date_modified,
        "date_added": record.date_added,
        "version": record.version,
        "status": record.status,
        "file_ids": file_ids,
        "files": files,
        "attachments_usable": record.attachments_usable,
        "private": record.is_private,
        "trashed": record.is_trashed,
        "withheld": record.is_withheld,
    })
}

/// Builds one cached catalog per GameBanana group and evaluates every local
/// recipe against that catalog. This is deliberately immutable: callers can
/// capture the evidence before applying status, ignore, or local-change
/// overlays and before taking mutable borrows of individual mods.
fn build_cached_update_evidence(
    mods: &[ModEntry],
    target_game_id: Option<&str>,
) -> HashMap<String, CachedUpdateEvidence> {
    let mut groups: HashMap<(String, u64, bool), Vec<&ModEntry>> = HashMap::new();
    for mod_entry in mods {
        if target_game_id.is_some_and(|game_id| mod_entry.game_id != game_id) {
            continue;
        }
        let Some(source) = mod_entry.source.as_ref() else {
            continue;
        };
        let Some(link) = source.gamebanana.as_ref() else {
            continue;
        };
        groups
            .entry((
                mod_entry.game_id.clone(),
                link.mod_id,
                gamebanana::is_tool_url(&link.url),
            ))
            .or_default()
            .push(mod_entry);
    }

    let mut evidence = HashMap::new();
    for (_group_key, group) in groups {
        let mut chosen: Option<(
            CachedProfileFreshness,
            String,
            Arc<gamebanana::ProfileResponse>,
            Option<DateTime<Utc>>,
        )> = None;
        let mut conflicting_at_freshness = false;
        for mod_entry in &group {
            let Some(source) = mod_entry.source.as_ref() else {
                continue;
            };
            let Some(profile) = source_profile_for_compare(source) else {
                continue;
            };
            let freshness = cached_profile_freshness(source, &profile);
            let fingerprint = cached_profile_catalog_fingerprint(&profile);
            match chosen.as_ref() {
                Some((chosen_freshness, _, _, _)) if freshness < *chosen_freshness => {}
                Some((chosen_freshness, chosen_fingerprint, _, _))
                    if freshness == *chosen_freshness && fingerprint != *chosen_fingerprint =>
                {
                    conflicting_at_freshness = true;
                }
                Some((chosen_freshness, _, chosen_profile, _))
                    if freshness == *chosen_freshness =>
                {
                    // Same catalog: keep the copy with more archive lists.
                    let chosen_listings = chosen_profile.file_listings.len();
                    if profile.file_listings.len() > chosen_listings {
                        chosen = Some((
                            freshness,
                            fingerprint,
                            Arc::new(profile),
                            source.profile_cached_at,
                        ));
                    }
                }
                _ => {
                    let profile_cached_at = source.profile_cached_at;
                    chosen = Some((
                        freshness,
                        fingerprint,
                        Arc::new(profile),
                        profile_cached_at,
                    ));
                    conflicting_at_freshness = false;
                }
            }
        }
        if conflicting_at_freshness {
            for mod_entry in group {
                evidence.insert(
                    mod_entry.id.clone(),
                    CachedUpdateEvidence {
                        state: ModUpdateState::CheckSkipped,
                        signature: None,
                        prearmed_signature: None,
                        newer_files: Vec::new(),
                        installed_files: Vec::new(),
                        other_mods_files: Vec::new(),
                        profile: None,
                        profile_cached_at: None,
                        thalia_pending: false,
                    },
                );
            }
            continue;
        }
        let Some((_, _, profile, profile_cached_at)) = chosen else {
            continue;
        };
        let group_items: Vec<_> = group
            .iter()
            .map(|mod_entry| {
                let source = mod_entry.source.as_ref().expect("group source");
                (
                    selected_file_baseline_ts(&source.file_set)
                        .or_else(|| source.snapshot.as_ref().and_then(|snapshot| snapshot.update_ts))
                        .or_else(|| mod_entry.content_mtime.map(|mtime| mtime.timestamp())),
                    source.file_set.clone(),
                )
            })
            .collect();
        let evaluations = evaluate_file_set_update_group(&group_items, &profile);
        let claims: Vec<HashSet<u64>> = group
            .iter()
            .zip(&evaluations)
            .map(|(mod_entry, evaluation)| {
                let source = mod_entry.source.as_ref().expect("group source");
                let pending = (evaluation.state == ModUpdateState::UpdateAvailable)
                    .then_some(evaluation.signature.as_ref())
                    .flatten();
                claimed_file_ids(&source.file_set, pending)
            })
            .collect();
        for (index, (mod_entry, evaluation)) in group.iter().zip(evaluations).enumerate() {
            // A file another mod of this page has, or is about to get, is not
            // news for this one.
            let mut other_mods_files: Vec<u64> = claims
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .flat_map(|(_, ids)| ids.iter().copied())
                .collect();
            other_mods_files.sort_unstable();
            other_mods_files.dedup();
            evidence.insert(
                mod_entry.id.clone(),
                CachedUpdateEvidence {
                    state: evaluation.state,
                    signature: evaluation.signature,
                    prearmed_signature: mod_entry
                        .source
                        .as_ref()
                        .and_then(|source| current_remote_signature(&source.file_set, &profile)),
                    newer_files: mod_entry
                        .source
                        .as_ref()
                        .map(|source| {
                            let dismissed: HashSet<u64> = source
                                .dismissed_newer_file_ids
                                .iter()
                                .copied()
                                .collect();
                            available_newer_files(&source.file_set, &profile)
                                .into_iter()
                                .filter(|file| {
                                    !dismissed.contains(&file.id)
                                        && other_mods_files.binary_search(&file.id).is_err()
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    installed_files: mod_entry
                        .source
                        .as_ref()
                        .and_then(|source| {
                            known_installed_files_for_newer_notice(&source.file_set, &profile)
                        })
                        .unwrap_or_default(),
                    other_mods_files,
                    profile: Some(Arc::clone(&profile)),
                    profile_cached_at,
                    thalia_pending: false,
                },
            );
        }
    }
    evidence
}

/// Whether the mod's on-disk fingerprint diverges from the fingerprint the
/// user last accepted via "Ignore local changes". Returns true when nothing
/// was accepted, so callers can `&&` it with the baseline comparison.
fn local_changes_diverge_from_accepted(source: &ModSourceData, mod_entry: &ModEntry) -> bool {
    let Some(accepted) = source.accepted_local_changes.as_ref() else {
        return true;
    };
    accepted.content_mtime.map(|t| t.timestamp()) != mod_entry.content_mtime.map(|t| t.timestamp())
        || accepted.ini_hash != mod_entry.ini_hash
}

/// True when the mod's files differ from the install baseline and the user has
/// not accepted the current files via "Ignore local changes". The baseline is
/// never rewritten by that acceptance, so clearing `accepted_local_changes`
/// restores the original verdict.
fn mod_has_local_changes_for_update_check(mod_entry: &ModEntry) -> bool {
    let Some(source) = mod_entry.source.as_ref() else {
        return false;
    };
    let diverges_from_baseline = source.baseline_content_mtime.map(|t| t.timestamp())
        != mod_entry.content_mtime.map(|t| t.timestamp())
        || source.baseline_ini_hash != mod_entry.ini_hash;
    diverges_from_baseline && local_changes_diverge_from_accepted(source, mod_entry)
}

/// Whether the mod carries an "Ignore local changes" acceptance that still
/// matches its files. Once the files drift past the accepted fingerprint the
/// acceptance is spent: the mod reads Modified again and the checkbox shows
/// unchecked, so re-checking it accepts the new state.
fn mod_ignores_local_changes(mod_entry: &ModEntry) -> bool {
    mod_entry.source.as_ref().is_some_and(|source| {
        source.accepted_local_changes.is_some()
            && !local_changes_diverge_from_accepted(source, mod_entry)
    })
}

/// Drops a spent acceptance (files drifted past the accepted fingerprint).
/// Returns true when something was cleared so the caller can persist.
fn clear_spent_local_changes_acceptance(mod_entry: &mut ModEntry) -> bool {
    let spent = mod_entry.source.as_ref().is_some_and(|source| {
        source.accepted_local_changes.is_some()
            && local_changes_diverge_from_accepted(source, mod_entry)
    });
    if !spent {
        return false;
    }
    if let Some(source) = mod_entry.source.as_mut() {
        source.accepted_local_changes = None;
    }
    true
}

/// Toggles "Ignore local changes" for a linked mod. Enabling snapshots the
/// current on-disk fingerprint as accepted (the install baseline is kept, so
/// this is reversible); disabling drops the acceptance so the baseline
/// comparison takes over again. Grouped callers reconcile the resulting state
/// against the complete GameBanana peer group after this mutation. Returns
/// true when anything changed; the caller persists the mod and app state.
fn set_mod_ignore_local_changes(mod_entry: &mut ModEntry, ignore: bool) -> bool {
    let content_mtime = mod_entry.content_mtime;
    let ini_hash = mod_entry.ini_hash.clone();
    let Some(source) = mod_entry.source.as_mut() else {
        return false;
    };
    let next = ignore.then(|| AcceptedLocalChanges {
        content_mtime,
        ini_hash,
        accepted_at: Some(chrono::Utc::now()),
    });
    let record_changed = match (source.accepted_local_changes.as_ref(), next.as_ref()) {
        (None, None) => false,
        (Some(current), Some(next)) => {
            current.content_mtime.map(|t| t.timestamp()) != next.content_mtime.map(|t| t.timestamp())
                || current.ini_hash != next.ini_hash
        }
        _ => true,
    };
    if !record_changed {
        return false;
    }
    source.accepted_local_changes = next;
    // Do not derive a one-item remote verdict here. The production callers
    // immediately reconcile the full peer group; this fallback only keeps the
    // local label coherent for legacy/unit callers that have no group context.
    if mod_has_local_changes_for_update_check(mod_entry) {
        mod_entry.update_state = ModUpdateState::ModifiedLocally;
    } else if mod_entry.update_state == ModUpdateState::ModifiedLocally {
        mod_entry.update_state = ModUpdateState::Unlinked;
    }
    true
}

#[cfg(test)]
fn apply_ignored_update_override(
    source: &mut ModSourceData,
    raw_state: ModUpdateState,
    profile: Option<&gamebanana::ProfileResponse>,
) -> ModUpdateState {
    let current_signature = profile.and_then(|profile| {
        current_update_signature_for_state(&source.file_set, profile, raw_state)
    });
    apply_ignored_update_override_with_signature(source, raw_state, current_signature)
}

/// Keep an exact Ignore-once acknowledgement when the conservative matcher
/// cannot classify the update, but the cached catalog still contains the same
/// active downloadable newer file(s). This covers an AI-only match without
/// allowing an old acknowledgement to hide a different local update.
fn ignored_signature_still_matches_newer_active_file(
    source: &ModSourceData,
    ignored: &IgnoredUpdateSignature,
) -> bool {
    if ignored.prearmed_next_update || ignored.files.is_empty() {
        return false;
    }
    let Some(raw_profile) = source.raw_profile_json.as_deref() else {
        return false;
    };
    let Ok(profile) = serde_json::from_str::<gamebanana::ProfileResponse>(raw_profile) else {
        return false;
    };
    let newer_files = available_newer_files(&source.file_set, &profile);
    !newer_files.is_empty()
        && ignored.files.iter().all(|ignored_file| {
            newer_files.iter().any(|file| {
                (ignored_file.file_id == 0 || ignored_file.file_id == file.id)
                    && ignored_file.file_name == file.file_name
                    && ignored_file.date_added == file.date_added
                    && ignored_file.version == file.version
            })
        })
}

/// Applies the ignore overlays using a signature produced by the grouped
/// worker evaluation.  Update-check results must use this rather than
/// rebuilding a signature from one source's profile: the worker's signature
/// represents the complete peer group and its collision decision.
fn apply_ignored_update_override_with_signature(
    source: &mut ModSourceData,
    raw_state: ModUpdateState,
    current_signature: Option<IgnoredUpdateSignature>,
) -> ModUpdateState {
    if source.ignore_update_always {
        source.ignored_update_signature = None;
        return ModUpdateState::IgnoringUpdateAlways;
    }
    let preserve_unresolved_ignore = matches!(
        raw_state,
        ModUpdateState::UpToDate | ModUpdateState::MissingSource
    ) && source
        .ignored_update_signature
        .as_ref()
        .is_some_and(|ignored| ignored_signature_still_matches_newer_active_file(source, ignored));
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
                .is_some_and(|ignored| {
                    ignored.prearmed_next_update || preserve_unresolved_ignore
                })
            {
                source.ignored_update_signature = None;
            }
        }
        ModUpdateState::Unlinked
        | ModUpdateState::IgnoringUpdateOnce
        | ModUpdateState::IgnoringUpdateAlways => {
            source.ignored_update_signature = None;
        }
        ModUpdateState::MissingSource => {
            if !preserve_unresolved_ignore {
                source.ignored_update_signature = None;
            }
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
            !ignored.prearmed_next_update
                && !preserve_unresolved_ignore
                && current_signature.as_ref() != Some(ignored)
        })
    {
        source.ignored_update_signature = None;
    }

    raw_state
}

#[cfg(test)]
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

fn backfill_selected_files_meta(
    file_set: &mut FileSetRecipe,
    profile: &gamebanana::ProfileResponse,
) -> bool {
    if !file_set.selected_files_meta.is_empty() || file_set.selected_file_ids.is_empty() {
        return false;
    }
    let canonical = matcher_adapter::canonicalize_profile(profile);
    let candidates: Vec<_> = all_profile_files(&canonical.profile)
        .into_iter()
        .map(tracked_file_meta_from_mod_file)
        .collect();
    let Some(matched) = reconstruct_tracked_files_from_candidates(file_set, &candidates) else {
        return false;
    };
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
    let canonical = matcher_adapter::canonicalize_profile(profile);
    let profile = &canonical.profile;
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

fn determine_update_state(
    local_ts: Option<i64>,
    profile: &gamebanana::ProfileResponse,
) -> ModUpdateState {
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

fn profile_to_response(
    snapshot: Option<&GameBananaSnapshot>,
    mod_id: u64,
) -> gamebanana::ProfileResponse {
    let Some(snapshot) = snapshot else {
        return gamebanana::ProfileResponse::default();
    };

    let mut profile = gamebanana::ProfileResponse {
        id: mod_id,
        name: snapshot.title.clone(),
        update_history: snapshot.update_history.clone(),
        short_description: snapshot.description.clone(),
        date_added: snapshot.publish_ts.unwrap_or_default(),
        date_modified: snapshot.update_ts.unwrap_or_default(),
        date_updated: snapshot.update_ts,
        is_private: snapshot.is_private,
        is_deleted: snapshot.is_deleted,
        is_trashed: snapshot.is_trashed,
        is_withheld: snapshot.is_withheld,
        category: snapshot
            .category
            .as_ref()
            .map(|category| gamebanana::SubmissionCategory {
                id: category.id,
                name: category.name.clone(),
            }),
        super_category: snapshot
            .super_category_id
            .map(|id| gamebanana::SubmissionCategory {
                id,
                name: String::new(),
            }),
        content_ratings: if snapshot.unsafe_content {
            HashMap::from([(String::from("snapshot"), String::from("unsafe"))])
        } else {
            HashMap::new()
        },
        ..Default::default()
    };
    for file in &snapshot.files {
        let remote_file = gamebanana::ModFile {
            id: file.file_id,
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            date_added: file.date_added,
            download_count: file.download_count,
            description: file.description.clone(),
            version: file.version.clone(),
            download_url: file.download_url.clone(),
            is_archived: file.archived,
            md5: file.md5.clone(),
        };
        if file.archived {
            profile.archived_files.push(remote_file);
        } else {
            profile.files.push(remote_file);
        }
    }
    profile
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
            version: file.version.clone(),
            download_url: file.download_url.clone(),
            archived: file.is_archived,
            md5: file.md5.clone(),
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
            version: file.version.clone(),
            download_url: file.download_url.clone(),
            archived: true,
            md5: file.md5.clone(),
        });
    }
    GameBananaSnapshot {
        title: profile.name.clone(),
        authors: gamebanana::all_authors(profile),
        update_history: profile.update_history.clone(),
        version: None,
        publish_ts: Some(profile.date_added),
        update_ts: profile.date_updated.or(Some(profile.date_modified)),
        description: profile.short_description.clone(),
        preview_urls: profile
            .preview_media
            .as_ref()
            .map(|preview| {
                preview
                    .images
                    .iter()
                    .map(gamebanana::full_image_url)
                    .collect()
            })
            .unwrap_or_default(),
        files,
        category: profile.category.as_ref().filter(|category| category.id != 0).map(|category| crate::model::GameBananaCategoryLink {
            id: category.id,
            name: category.name.clone(),
        }),
        super_category_id: profile.super_category.as_ref().map(|category| category.id).filter(|id| *id != 0),
        is_private: profile.is_private,
        is_deleted: profile.is_deleted,
        is_trashed: profile.is_trashed,
        is_withheld: profile.is_withheld,
        unsafe_content: !profile.content_ratings.is_empty(),
    }
}

impl HestiaApp {
    /// Reuses the transient grouped matching map for all library consumers
    /// sharing the current card cache inputs. Folder selection calls this
    /// before filtering, so a render does not rebuild every GameBanana peer
    /// group on each frame.
    fn ensure_cached_grouped_update_evidence(&mut self) {
        let cache_key = self.grouped_update_evidence_cache_key();
        if self.library_card_cache.evidence_key == Some(cache_key) {
            return;
        }
        let mut evidence = build_cached_update_evidence(&self.state.mods, None);
        self.overlay_thalia_update_evidence(&mut evidence);
        self.library_card_cache.evidence = evidence;
        self.library_card_cache.evidence_key = Some(cache_key);
    }

    /// Matching evidence is independent of search, sorting, grouping, and
    /// other card presentation choices. Keep those inputs out of this key so
    /// typing a search query does not rescore every peer group. The transient
    /// Thalia result fingerprint is included so a completed recommendation
    /// invalidates the cached card evidence without touching persisted state.
    fn grouped_update_evidence_cache_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        Self::grouped_update_evidence_cache_key_for_mods(&self.state.mods).hash(&mut hasher);
        self.thalia_evidence_fingerprint().hash(&mut hasher);
        hasher.finish()
    }

    fn grouped_update_evidence_cache_key_for_mods(mods: &[ModEntry]) -> u64 {
        let mut hasher = DefaultHasher::new();
        mods.len().hash(&mut hasher);
        for mod_entry in mods {
            Self::hash_mod_for_library_cache(&mut hasher, mod_entry);
        }
        hasher.finish()
    }

    /// Returns grouped cached evidence for UI actions. The library-card cache
    /// normally already contains this map; the fallback builds the same full
    /// peer group when an action is opened before cards have rendered.
    fn cached_grouped_evidence_for_mod(
        &self,
        mod_entry: &ModEntry,
    ) -> Option<CachedUpdateEvidence> {
        let cache_key = self.grouped_update_evidence_cache_key();
        if self.library_card_cache.evidence_key == Some(cache_key) {
            if let Some(evidence) = self.library_card_cache.evidence.get(&mod_entry.id) {
                return Some(evidence.clone());
            }
        }
        let mut evidence = build_cached_update_evidence(&self.state.mods, Some(&mod_entry.game_id));
        self.overlay_thalia_update_evidence(&mut evidence);
        evidence.remove(&mod_entry.id)
    }

    fn cached_grouped_signature_for_mod(
        &self,
        mod_entry: &ModEntry,
    ) -> Option<IgnoredUpdateSignature> {
        self.cached_grouped_evidence_for_mod(mod_entry).and_then(|evidence| {
            if evidence.state == ModUpdateState::UpdateAvailable {
                evidence.signature.or(evidence.prearmed_signature)
            } else {
                evidence.prearmed_signature.or(evidence.signature)
            }
        })
    }

    /// Applies the same status, Ignore, and local-change overlays used by the
    /// grouped matcher without mutating source metadata. Library filters and
    /// rows must classify the effective grouped state rather than persisted
    /// state, which can lag until the next refresh.
    fn effective_grouped_update_state(
        mod_entry: &ModEntry,
        evidence: Option<&CachedUpdateEvidence>,
        status_targets: ModStatusTargets,
    ) -> ModUpdateState {
        let Some(evidence) = evidence else {
            return mod_entry.update_state;
        };
        if !Self::status_target_enabled(&mod_entry.status, status_targets) {
            return ModUpdateState::CheckSkipped;
        }
        let Some(source) = mod_entry.source.as_ref() else {
            return evidence.state;
        };
        if source.ignore_update_always {
            return ModUpdateState::IgnoringUpdateAlways;
        }
        if mod_has_local_changes_for_update_check(mod_entry) {
            return ModUpdateState::ModifiedLocally;
        }
        if evidence.state == ModUpdateState::UpdateAvailable
            && source
                .ignored_update_signature
                .as_ref()
                .is_some_and(|ignored| {
                    ignored.prearmed_next_update
                        || evidence.signature.as_ref() == Some(ignored)
                })
        {
            return ModUpdateState::IgnoringUpdateOnce;
        }
        evidence.state
    }

    /// Returns whether the passive file-availability notice should be shown
    /// for a mod.  Smart matching remains authoritative: a recognized smart
    /// update owns the card even if the broader availability scan also sees
    /// other files.  This function only reads cached evidence and preferences.
    fn has_newer_files_notice_from_evidence(
        mod_entry: &ModEntry,
        evidence: &CachedUpdateEvidence,
        status_targets: ModStatusTargets,
    ) -> bool {
        if evidence.state == ModUpdateState::UpdateAvailable
            || !Self::status_target_enabled(&mod_entry.status, status_targets)
        {
            return false;
        }
        let Some(source) = mod_entry.source.as_ref() else {
            return false;
        };
        if source.ignore_update_always {
            return false;
        }
        let dismissed: HashSet<u64> = source
            .dismissed_newer_file_ids
            .iter()
            .copied()
            .filter(|id| *id != 0)
            .collect();
        // An Ignore Once acknowledgement applies to the exact file IDs that
        // the smart matcher presented.  Keep that preference from being
        // reintroduced through the broader passive list.
        let ignored_once: HashSet<u64> = source
            .ignored_update_signature
            .as_ref()
            .map(|signature| {
                signature
                    .files
                    .iter()
                    .filter_map(|file| (file.file_id != 0).then_some(file.file_id))
                    .collect()
            })
            .unwrap_or_default();
        evidence.newer_files.iter().any(|file| {
            file.id != 0 && !dismissed.contains(&file.id) && !ignored_once.contains(&file.id)
        })
    }

    fn has_newer_files_notice(&self, mod_entry: &ModEntry) -> bool {
        let Some(evidence) = self.cached_grouped_evidence_for_mod(mod_entry) else {
            return false;
        };
        Self::has_newer_files_notice_from_evidence(
            mod_entry,
            &evidence,
            self.state.static_prefs.update_check_statuses,
        )
    }

    /// Dismisses exactly the IDs that the file review UI showed.  This is a
    /// per-upload acknowledgement, not a state or install mutation: later
    /// GameBanana IDs will naturally surface again.
    fn dismiss_newer_files_notice(&mut self, mod_id: &str, file_ids: &[u64]) -> bool {
        let mut ids: Vec<u64> = file_ids.iter().copied().filter(|id| *id != 0).collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return false;
        }
        let Some(mod_index) = self
            .state
            .mods
            .iter()
            .position(|mod_entry| mod_entry.id == mod_id)
        else {
            return false;
        };
        let backend = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == self.state.mods[mod_index].game_id)
            .map(|game| game.definition.backend)
            .unwrap_or_default();
        let previous_dismissals = {
            let Some(source) = self.state.mods[mod_index].source.as_mut() else {
                return false;
            };
            let previous = source.dismissed_newer_file_ids.clone();
            source.dismissed_newer_file_ids.extend(ids);
            source.dismissed_newer_file_ids.sort_unstable();
            source.dismissed_newer_file_ids.dedup();
            if source.dismissed_newer_file_ids == previous {
                return false;
            }
            previous
        };
        let save_result = match backend {
            GameBackend::Xxmi => xxmi::save_mod_metadata(&mut self.state.mods[mod_index]),
            GameBackend::UnrealEngine => {
                unrealengine::write_portable_metadata(&self.state.mods[mod_index])
            }
        };
        if let Err(err) = save_result {
            // Do not leave a dismissal that was never durably persisted: the
            // next refresh should keep showing the notice.
            if let Some(source) = self.state.mods[mod_index].source.as_mut() {
                source.dismissed_newer_file_ids = previous_dismissals;
            }
            self.report_error_message(
                format!("could not save new-file dismissal: {err:#}"),
                None,
            );
            return false;
        }
        self.library_card_cache.key = None;
        self.library_card_cache.rows = Arc::new(Vec::new());
        self.library_card_cache.evidence.clear();
        self.library_card_cache.evidence_key = None;
        true
    }

    /// Reapplies cached grouped evidence after an install has written a new
    /// recipe. This runs after the metadata borrow is released, so every peer
    /// in the GameBanana group is evaluated against the same catalog.
    fn reconcile_cached_grouped_update_states(&mut self, target_game_id: Option<&str>) {
        let evidence = build_cached_update_evidence(&self.state.mods, target_game_id);
        for mod_entry in &mut self.state.mods {
            let Some(grouped) = evidence.get(&mod_entry.id) else {
                continue;
            };
            let raw_state = if mod_has_local_changes_for_update_check(mod_entry) {
                ModUpdateState::ModifiedLocally
            } else {
                grouped.state
            };
            let Some(source) = mod_entry.source.as_mut() else {
                continue;
            };
            let next_state = if !Self::status_target_enabled(
                &mod_entry.status,
                self.state.static_prefs.update_check_statuses,
            ) {
                ModUpdateState::CheckSkipped
            } else {
                apply_ignored_update_override_with_signature(
                    source,
                    raw_state,
                    grouped.signature.clone(),
                )
            };
            if mod_entry.update_state != next_state {
                mod_entry.update_state = next_state;
                let _ = xxmi::save_mod_metadata(mod_entry);
            }
        }
    }

    fn set_mod_ignore_local_changes_grouped(
        &mut self,
        mod_entry_id: &str,
        ignore: bool,
    ) -> bool {
        let Some(index) = self
            .state
            .mods
            .iter()
            .position(|mod_entry| mod_entry.id == mod_entry_id)
        else {
            return false;
        };
        let changed = set_mod_ignore_local_changes(&mut self.state.mods[index], ignore);
        if changed {
            let game_id = self.state.mods[index].game_id.clone();
            self.reconcile_cached_grouped_update_states(Some(&game_id));
        }
        changed
    }

    fn update_check_item_for_mod(
        &mut self,
        mod_entry_id: &str,
    ) -> Option<UpdateCheckItem> {
        let mod_entry = self.state.mods.iter_mut().find(|m| m.id == mod_entry_id)?;
        let source = mod_entry.source.as_mut()?;
        let (linked_mod_id, is_tool) = source
            .gamebanana
            .as_ref()
            .map(|link| (link.mod_id, gamebanana::is_tool_url(&link.url)))?;
        let metadata_changed = hydrate_tracked_file_sizes(
            &mut source.file_set,
            source.raw_profile_json.as_deref(),
            source.snapshot.as_ref(),
            None,
        );
        let local_sync_ts = selected_file_baseline_ts(&source.file_set)
            .or_else(|| source.snapshot.as_ref().and_then(|s| s.update_ts))
            .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));
        let file_set = source.file_set.clone();
        let mod_id = mod_entry.id.clone();
        let game_id = mod_entry.game_id.clone();
        let _ = source;
        if metadata_changed {
            let _ = xxmi::save_mod_metadata(mod_entry);
        }
        Some((
            mod_id,
            game_id,
            linked_mod_id,
            local_sync_ts,
            file_set,
            is_tool,
        ))
    }

    fn dispatch_update_check_items(
        &mut self,
        items: Vec<UpdateCheckItem>,
    ) {
        if items.is_empty() {
            return;
        }
        let peer_items = self.build_update_check_peer_context();
        self.update_check_active_items = items.clone();
        if self.update_check_tx.send(UpdateCheckRequest {
            generation: self.update_check_generation,
            items,
            peer_items,
        }).is_ok() {
            self.update_check_inflight = true;
        }
    }

    /// Builds matching context from every linked sibling, while targets remain
    /// the subset selected by status/local-change policy. This runs at dispatch
    /// time so manual checks and proxy restarts use the same current context.
    fn build_update_check_peer_context(&mut self) -> Vec<UpdateCheckItem> {
        let mut items = Vec::new();
        let mut metadata_changed = false;
        for mod_entry in &mut self.state.mods {
            let Some((item, changed)) = (|| {
                let source = mod_entry.source.as_mut()?;
                let link = source.gamebanana.as_ref()?;
                let changed = hydrate_tracked_file_sizes(
                    &mut source.file_set,
                    source.raw_profile_json.as_deref(),
                    source.snapshot.as_ref(),
                    None,
                );
                let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                    .or_else(|| source.snapshot.as_ref().and_then(|snapshot| snapshot.update_ts))
                    .or_else(|| mod_entry.content_mtime.map(|mtime| mtime.timestamp()));
                Some((
                    (
                        mod_entry.id.clone(),
                        mod_entry.game_id.clone(),
                        link.mod_id,
                        local_sync_ts,
                        source.file_set.clone(),
                        gamebanana::is_tool_url(&link.url),
                    ),
                    changed,
                ))
            })() else {
                continue;
            };
            if changed {
                let _ = xxmi::save_mod_metadata(mod_entry);
                metadata_changed = true;
            }
            items.push(item);
        }
        if metadata_changed {
            self.save_state();
        }
        items
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
        if self
            .state
            .mods
            .iter()
            .find(|mod_entry| mod_entry.id == mod_entry_id)
            .is_some_and(|mod_entry| self.folder_batch_game_busy(&mod_entry.game_id))
        {
            self.pending_update_check_mods.insert(mod_entry_id.to_string());
            return;
        }
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

    fn queue_update_check_for_linked_mods_internal(
        &mut self,
        target_game_id: Option<&str>,
        force: bool,
    ) {
        let round_games: Vec<String> = target_game_id
            .map(|game_id| vec![game_id.to_string()])
            .unwrap_or_else(|| {
                self.state
                    .mods
                    .iter()
                    .map(|mod_entry| mod_entry.game_id.clone())
                    .collect()
            });
        // Hold blue notices from now until this check's Thalia round, even
        // when the check itself has to wait for a busy batch or another check.
        self.thalia_matching.begin_round(round_games.iter().cloned());
        if force {
            self.thalia_matching
                .mark_force_refresh_for_games(round_games);
        }
        if self.folder_batch_job.is_some()
            || target_game_id.is_some_and(|game_id| self.folder_batch_game_busy(game_id))
        {
            self.pending_update_check_game = target_game_id.map(str::to_string);
            return;
        }
        if self.update_check_inflight {
            self.pending_update_check_game = target_game_id.map(|id| id.to_string());
            return;
        }
        self.pending_update_check_game = None;

        const UPDATE_CHECK_COOLDOWN_SECS: i64 = 1800;
        let now = chrono::Utc::now();

        // The *network* refetch is throttled per game (the schedule is persisted
        // so app restarts can't bypass it and hammer GameBanana). The reconcile
        // pass in the loop below is free -- it reads the cached snapshot already
        // on disk -- so it runs on every call regardless of the cooldown. Without
        // that, a restart inside the cooldown window leaves every linked mod stuck
        // at the load-time `Unlinked` default (blank card, "unlinked" badge) even
        // though the source is right there. Only the fetch itself waits.
        let allow_network = force || {
            let schedule_key = target_game_id.unwrap_or_default();
            !self
                .state
                .last_update_check_time_by_game
                .get(schedule_key)
                .is_some_and(|last_check| {
                    now.signed_duration_since(*last_check).num_seconds()
                        < UPDATE_CHECK_COOLDOWN_SECS
                })
        };
        // Hydrate legacy recipes and capture one immutable catalog per linked
        // group before taking the per-mod mutable borrows below. The evidence
        // includes ignored/skipped/local-change peers; those overlays are
        // applied after the grouped match.
        self.build_update_check_peer_context();
        let cached_evidence = build_cached_update_evidence(&self.state.mods, target_game_id);
        let mut items = Vec::with_capacity(self.state.mods.len());
        let update_check_statuses = self.state.static_prefs.update_check_statuses;
        let modified_update_behavior = self.state.static_prefs.modified_update_behavior;
        let mut state_changed_without_fetch = false;
        for mod_entry in &mut self.state.mods {
            if let Some(id) = target_game_id {
                if mod_entry.game_id != id { continue; }
            }
            // Only mods with a GameBanana link participate.
            let Some((linked_mod_id, is_tool)) = mod_entry
                .source
                .as_ref()
                .and_then(|source| source.gamebanana.as_ref())
                .map(|link| (link.mod_id, gamebanana::is_tool_url(&link.url)))
            else {
                continue;
            };
            let metadata_backfilled = mod_entry
                .source
                .as_mut()
                .is_some_and(|source| {
                    hydrate_tracked_file_sizes(
                        &mut source.file_set,
                        source.raw_profile_json.as_deref(),
                        source.snapshot.as_ref(),
                        None,
                    )
                });
            // Terminal, non-network states take precedence and short-circuit.
            if !Self::status_target_enabled(&mod_entry.status, update_check_statuses) {
                if mod_entry.update_state != ModUpdateState::CheckSkipped || metadata_backfilled {
                    mod_entry.update_state = ModUpdateState::CheckSkipped;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            if mod_entry
                .source
                .as_ref()
                .is_some_and(|source| source.ignore_update_always)
            {
                if mod_entry.update_state != ModUpdateState::IgnoringUpdateAlways || metadata_backfilled {
                    mod_entry.update_state = ModUpdateState::IgnoringUpdateAlways;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }
            if modified_update_behavior == ModifiedUpdateBehavior::HideButton
                && mod_has_local_changes_for_update_check(mod_entry)
            {
                if mod_entry.update_state != ModUpdateState::ModifiedLocally || metadata_backfilled {
                    mod_entry.update_state = ModUpdateState::ModifiedLocally;
                    let _ = xxmi::save_mod_metadata(mod_entry);
                    state_changed_without_fetch = true;
                }
                continue;
            }

            // Reconcile the live verdict from the cached snapshot so a linked mod
            // shows its real last-known state immediately, independent of the
            // network throttle below. The ignore overlay mirrors the
            // network-result path (consume_update_check_results) so an
            // ignored-once update keeps its state across restarts. A mod that has
            // never been fetched has no snapshot to compare against, so it stays
            // at whatever it loaded as (see the badge's linked-but-unchecked case).
            // A spent "Ignore local changes" acceptance (files drifted past the
            // accepted fingerprint) is dropped here so the checkbox and the
            // Modified label agree.
            let acceptance_cleared = clear_spent_local_changes_acceptance(mod_entry);
            let mut reconciled_changed = false;
            if let Some(evidence) = cached_evidence.get(&mod_entry.id) {
                let raw_state = if mod_has_local_changes_for_update_check(mod_entry) {
                    ModUpdateState::ModifiedLocally
                } else {
                    evidence.state
                };
                let reconciled = mod_entry.source.as_mut().map(|source| {
                    apply_ignored_update_override_with_signature(
                        source,
                        raw_state,
                        evidence.signature.clone(),
                    )
                });
                if let Some(reconciled) = reconciled {
                    if mod_entry.update_state != reconciled {
                        mod_entry.update_state = reconciled;
                        reconciled_changed = true;
                    }
                }
            }
            if metadata_backfilled || acceptance_cleared || reconciled_changed {
                let _ = xxmi::save_mod_metadata(mod_entry);
                state_changed_without_fetch = true;
            }

            // Everything above is free (local + cached). The network refetch is
            // the throttled part, so it is the only thing gated by the cooldown.
            if !allow_network {
                continue;
            }
            // Automatic checks leave MissingSource mods alone (a genuinely deleted
            // mod would fail the same way forever), but an explicit Reload must be
            // able to recover mods that were marked Missing by a bad API response.
            if !force && !should_check_update_state(mod_entry.update_state) {
                continue;
            }
            if !force
                && mod_entry
                    .source
                    .as_ref()
                    .and_then(|source| source.update_check_retry_after)
                    .is_some_and(|retry_after| retry_after > now)
            {
                continue;
            }
            let Some(source) = mod_entry.source.as_ref() else {
                continue;
            };
            // Prefer the exact GameBanana file(s) this mod was installed from.
            // Fall back to the profile snapshot timestamp for older metadata.
            let local_sync_ts = selected_file_baseline_ts(&source.file_set)
                .or_else(|| source.snapshot.as_ref().and_then(|s| s.update_ts))
                .or_else(|| mod_entry.content_mtime.map(|t| t.timestamp()));

            items.push((
                mod_entry.id.clone(),
                mod_entry.game_id.clone(),
                linked_mod_id,
                local_sync_ts,
                source.file_set.clone(),
                is_tool,
            ));
        }
        if state_changed_without_fetch {
            self.save_state();
        }
        // With nothing to fetch, no check result will arrive to start the
        // Thalia round, so ask now from the cached catalog.
        let nothing_to_fetch = items.is_empty();
        self.dispatch_update_check_items(items);
        if !allow_network || nothing_to_fetch {
            let game_ids: HashSet<String> = target_game_id
                .map(|game_id| [game_id.to_string()].into_iter().collect())
                .unwrap_or_else(|| self.state.mods.iter().map(|mod_entry| mod_entry.game_id.clone()).collect());
            self.schedule_thalia_matching_for_games(&game_ids);
        }
    }

    fn consume_update_check_results(&mut self) {
        if self.folder_batch_job.is_some() {
            return;
        }
        while let Ok(result) = self.update_check_rx.try_recv() {
            if result.generation != self.update_check_generation {
                continue;
            }
            self.update_check_inflight = false;
            let current_peer_items = self.build_update_check_peer_context();
            let checked_game_ids: HashSet<String> = self.update_check_active_items
                .iter()
                .map(|(_, game_id, _, _, _, _)| game_id.clone())
                .collect();
            let thalia_game_ids = checked_game_ids.clone();
            self.update_check_active_items.clear();
            let mut warn_lines: Vec<String> = Vec::new();
            let mut auto_update_ids: Vec<String> = Vec::new();
            let mut stale_group_targets: HashMap<(String, u64, bool), String> = HashMap::new();
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
            for outcome in result.states {
                let UpdateCheckOutcome {
                    mod_id,
                    game_id,
                    gb_id,
                    gb_is_tool,
                    file_set,
                    state,
                    signature,
                    snapshot,
                    error: err,
                    raw_json,
                    profile,
                    profile_cached_at,
                    peer_fingerprint,
                } = outcome;
                let group_key = (game_id.clone(), gb_id, gb_is_tool);
                if update_check_group_fingerprint_for_group(
                    &current_peer_items,
                    &game_id,
                    gb_id,
                    gb_is_tool,
                ) != peer_fingerprint
                {
                    stale_group_targets
                        .entry(group_key)
                        .or_insert_with(|| mod_id.clone());
                    continue;
                }
                let grouped_signature = signature.clone();
                let mut mod_updated = false;
                let mut retry_schedule_changed = false;
                let mut should_sync_images = false;
                let mut sync_profile:
                    Option<(Box<gamebanana::ProfileResponse>, Option<DateTime<Utc>>)> = None;
                let has_pending_update_finalization =
                    self.pending_update_finalization_for_mod(&mod_id);
                let fetch_failed = err.is_some()
                    && snapshot.is_none()
                    && raw_json.is_none()
                    && profile.is_none();
                let Some(mod_index) = self.state.mods.iter().position(|m| {
                    m.id == mod_id
                        && m.game_id == game_id
                        && m.source.as_ref().is_some_and(|source| {
                            source.file_set == file_set
                                && source.gamebanana.as_ref().is_some_and(|link| {
                                    link.mod_id == gb_id
                                        && gamebanana::is_tool_url(&link.url) == gb_is_tool
                                })
                        })
                }) else {
                    // The user may have relinked or changed the recipe while
                    // this request was in flight. Never apply a stale grouped
                    // result to the new source.
                    continue;
                };
                if let Some(mod_entry) = self.state.mods.get_mut(mod_index) {
                    let old_preview_urls = mod_entry
                        .source
                        .as_ref()
                        .and_then(|s| s.snapshot.as_ref())
                        .map(|s| s.preview_urls.clone())
                        .unwrap_or_default();
                    let has_local_changes = mod_has_local_changes_for_update_check(mod_entry);
                    if fetch_failed && !has_local_changes {
                        warn_lines.push(format!(
                            "{} (update check failed; keeping previous state: {})",
                            mod_entry.folder_name,
                            err.as_deref().unwrap_or("unknown error"),
                        ));
                    }
                    if let Some(snapshot) = snapshot.as_ref() {
                        mod_entry.unsafe_content_auto = snapshot.unsafe_content;
                        mod_entry.unsafe_content = mod_entry
                            .unsafe_content_preference
                            .resolve(mod_entry.unsafe_content_auto);
                    }
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
                            let _ = hydrate_tracked_file_sizes(
                                &mut source.file_set,
                                source.raw_profile_json.as_deref(),
                                source.snapshot.as_ref(),
                                Some(profile),
                            );
                            let _ = backfill_selected_files_meta(&mut source.file_set, profile);
                            let _ = backfill_tracked_file_labels(&mut source.file_set, profile);
                        }
                        if let Some(s) = snapshot {
                            source.snapshot = Some(s);
                        }
                        if let Some(raw) = raw_json {
                            source.raw_profile_json = Some(raw);
                        }
                        if profile_cached_at.is_some() {
                            source.profile_cached_at = profile_cached_at;
                        }
                        let raw_state = if has_local_changes {
                            ModUpdateState::ModifiedLocally
                        } else {
                            state
                        };
                        if !fetch_failed {
                            mod_entry.update_state = apply_ignored_update_override_with_signature(
                                source,
                                raw_state,
                                signature,
                            );
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
                        sync_profile = profile.map(|profile| (profile, profile_cached_at));
                    }
                    let modified_update_available =
                        Self::has_modified_update_available_from_grouped(
                            mod_entry,
                            state,
                            grouped_signature.as_ref(),
                        );
                    let auto_update_allowed = mod_entry.update_state == ModUpdateState::UpdateAvailable
                        || (self.state.static_prefs.modified_update_behavior == ModifiedUpdateBehavior::Yes
                            && modified_update_available);
                    let should_auto_apply = !fetch_failed
                        && auto_update_allowed
                        && !has_pending_update_finalization
                        && Self::status_target_enabled(
                            &mod_entry.status,
                            self.state.static_prefs.auto_update_statuses,
                        )
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
                    if let Some((p, profile_cached_at)) = sync_profile {
                        if let Some((mod_root_path, game_id)) = self
                            .state
                            .mods
                            .iter()
                            .find(|m| m.id == mod_id)
                            .map(|m| (m.root_path.clone(), m.game_id.clone()))
                            .filter(|(_, game_id)| !self.folder_batch_game_busy(game_id))
                        {
                            let job_id = self.next_background_job_id();
                            if self.install_request_tx.send(InstallRequest::SyncImages {
                                job_id,
                                mod_entry_id: mod_id.clone(),
                                mod_root_path,
                                profile: p,
                                profile_cached_at,
                            }).is_ok() {
                                self.mod_image_sync_inflight.insert(job_id, game_id);
                            }
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
            for mod_id in stale_group_targets.values() {
                self.pending_update_check_mods.insert(mod_id.clone());
            }
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
            let force_games: HashSet<String> = thalia_game_ids
                .iter()
                .filter(|game_id| self.thalia_matching.take_force_refresh_for_game(game_id))
                .cloned()
                .collect();
            self.schedule_thalia_matching_for_games_with_force(&thalia_game_ids, &force_games);
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

    fn has_active_update_for_mod(&self, mod_entry: &ModEntry) -> bool {
        let update_title = self.text().updating_task(
            mod_entry.metadata.user.title.as_ref().unwrap_or(&mod_entry.folder_name),
        );
        self.pending_update_finalization_for_mod(&mod_entry.id)
            || self.browse_state.pending_installs.iter().any(|pending| {
                pending.update_target_id.as_deref() == Some(mod_entry.id.as_str())
                    && pending.game_id == mod_entry.game_id
            })
            || self
                .thalia_install_bundles
                .iter()
                .any(|bundle| thalia_bundle_updates_mod(bundle, mod_entry))
            || self.pending_browse_install_meta.values().any(|meta| {
                meta.update_target_mod_id.as_deref() == Some(mod_entry.id.as_str())
                    && meta.game_id == mod_entry.game_id
            })
            || self.state.tasks.iter().any(|task| {
                update_task_targets_mod(task, mod_entry, &update_title)
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
        if let Some(choice) = self
            .pending_browse_install_meta
            .get(&job_id)
            .and_then(|meta| meta.explicit_resolution)
            .and_then(|resolution| match resolution {
                ImportResolution::Replace => Some(ConflictChoice::Replace),
                ImportResolution::Merge => Some(ConflictChoice::Merge),
                ImportResolution::KeepBoth => Some(ConflictChoice::KeepBoth),
                ImportResolution::Ask => None,
            })
        {
            return Some(choice);
        }
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
            if !self
                .startup_path_scan
                .as_ref()
                .is_some_and(StartupPathScanState::accepts_worker_event)
            {
                continue;
            }
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
                let mut enabled_game_id = None;
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
                            game.mods_path_override =
                                game.vanilla_exe_path_override.as_ref().and_then(|path| {
                                    default_unreal_pak_mods_path_from_exe(&game.definition.id, path)
                                });
                        }
                        if !game.enabled {
                            enabled_game_id = Some(game.definition.id.clone());
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
                // Startup recovery skips disabled games, so a game enabled by the path
                // scan still owes one recovery pass.
                if let Some(game_id) = enabled_game_id {
                    self.queue_profile_recovery_for_game(&game_id);
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
                let path = status.selected_candidate.clone().or_else(|| {
                    allow_fallback
                        .then(|| status.candidates.first().cloned())
                        .flatten()
                })?;
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
                    let mut enabled_game_id = None;
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
                            game.mods_path_override =
                                game.vanilla_exe_path_override.as_ref().and_then(|path| {
                                    default_unreal_pak_mods_path_from_exe(&game.definition.id, path)
                                });
                        }
                        if !game.enabled {
                            enabled_game_id = Some(game.definition.id.clone());
                        }
                        game.enabled = true;
                        ctx.data_mut(|data| {
                            data.remove::<String>(egui::Id::new((
                                "settings_vanilla_path",
                                game.definition.id.as_str(),
                            )));
                        });
                    }
                    // Startup recovery skips disabled games, so a game enabled by the path
                    // scan still owes one recovery pass.
                    if let Some(game_id) = enabled_game_id {
                        self.queue_profile_recovery_for_game(&game_id);
                    }
                }
            }
        }
    }

    fn queue_game_refresh(&mut self, game_id: String) {
        if self.folder_batch_game_busy(&game_id) {
            self.refresh_pending_selected_game = Some(game_id);
            return;
        }
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
                    if self.folder_batch_game_busy(&game_id) {
                        self.refresh_pending_selected_game = Some(game_id);
                        continue;
                    }
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
                    let mut new_tool_ids = Vec::new();
                    if is_current {
                        self.invalidate_stale_mod_textures(&old_ts);
                        self.sync_selection_after_refresh();
                        self.backfill_missing_mod_images(Some(&game_id));
                        new_tool_ids = self.sync_tools_for_selected_game();
                    }
                    let installed_mod_ids =
                        self.resolve_pending_install_finalization_for_game(&game_id);
                    let finalized_install = !installed_mod_ids.is_empty();
                    self.announce_installed_mod_tools(&new_tool_ids, &installed_mod_ids);
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
                self.queue_game_refresh(next_game_id);
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

    /// Finalizes every pending install that landed in `game_id` and returns the ids of the mods
    /// those installs produced (empty when nothing was pending).
    fn resolve_pending_install_finalization_for_game(&mut self, game_id: &str) -> Vec<String> {
        let mut installed_mod_ids = Vec::new();
        let job_ids: Vec<u64> = self.pending_install_finalize.keys().copied().collect();
        let mut job_ids = job_ids;
        job_ids.sort_unstable();
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
            installed_mod_ids.extend(self.finalize_install_after_refresh(job_id, payload));
        }
        installed_mod_ids
    }

    /// Returns the ids of the mods this install produced.
    fn finalize_install_after_refresh(
        &mut self,
        job_id: u64,
        payload: PendingInstallFinalize,
    ) -> Vec<String> {
        let PendingInstallFinalize {
            installed_paths,
            installed_candidate_labels,
            gb_profile,
            rel_paths,
            pending_meta,
            pending_unsafe,
            install_state: local_install_state,
            preserve_existing_state,
            preserved_states,
            target_category_id,
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
                    mod_entry.unsafe_content_auto = true;
                    mod_entry.unsafe_content = mod_entry
                        .unsafe_content_preference
                        .resolve(mod_entry.unsafe_content_auto);
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

        // The publish worker stages the requested state atomically. Final state is resolved after
        // category assignment below; no second disable pass can transiently mutate a replacement.
        // An external install (no Browse metadata, no GameBanana profile) may
        // ship its own preview image; adopt it as the initial mod image the
        // same way a manually supplied one lands. The scan sees the files in
        // their final place, including DISABLED_BY_HESTIA when applicable.
        if pending_meta.is_none() && gb_profile.is_none() {
            for id in &newly_installed_ids {
                self.enqueue_adopt_bundled_preview_images(id);
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
        // The in-game overlay's installs go where it showed them, or into
        // their character's category.
        let overlay_category = self.game_overlay_install_category(job_id);
        for id in &newly_installed_ids {
            match &overlay_category {
                Some(Some(category_id)) => self.assign_mod_category(id, Some(category_id.clone())),
                _ => self.apply_browse_download_category(
                    id,
                    pending_meta.as_ref(),
                    gb_profile.as_deref(),
                    overlay_category.is_some(),
                ),
            }
        }
        // External installs (no GameBanana listing) inherit the category folder
        // the user was drilled into when they queued the install. GameBanana
        // downloads/updates carry pending_meta and derive their own category
        // above, so this fallback stays out of their way. Skip if the folder was
        // deleted between queueing and finalize.
        if pending_meta.is_none() {
            if let Some(category_id) = target_category_id.filter(|id| {
                self.state.categories.iter().any(|category| category.id == *id)
            }) {
                for id in &newly_installed_ids {
                    self.assign_mod_category(id, Some(category_id.clone()));
                }
            }
        }

        // The worker publishes Auto candidates disabled.  Resolve their final
        // state only after the refresh has assigned categories. Auto candidates
        // are still disabled here, so only pre-existing or successfully enabled
        // peers count and one candidate can claim an otherwise empty category.
        let preserved_status_for = |mod_entry: &ModEntry| {
            preserved_states.iter().find_map(|(path, status)| {
                Self::install_path_matches_mod_root(path, &mod_entry.root_path)
                    .then_some(status.clone())
            })
        };
        let preserve_state = preserve_existing_state
            || pending_meta
                .as_ref()
                .is_some_and(|meta| meta.preserve_existing_state);
        let mut auto_category_claims = HashSet::new();
        for mod_id in &newly_installed_ids {
            let Some(mod_entry) = self.state.mods.iter().find(|m| m.id == *mod_id).cloned() else {
                continue;
            };
            let desired = if preserve_state {
                preserved_status_for(&mod_entry)
            } else {
                None
            }
            .or_else(|| match local_install_state {
                ModInstallState::Enabled => Some(ModStatus::Active),
                ModInstallState::Disabled => Some(ModStatus::Disabled),
                ModInstallState::Auto => {
                    if pending_meta
                        .as_ref()
                        .is_some_and(|meta| meta.update_target_mod_id.as_deref() == Some(mod_id))
                    {
                        return Some(
                            if pending_meta
                                .as_ref()
                                .is_some_and(|meta| meta.update_target_was_disabled)
                            {
                                ModStatus::Disabled
                            } else {
                                ModStatus::Active
                            },
                        );
                    }
                    let desired = auto_install_status(
                        &self.state.categories,
                        &mod_entry,
                        &self.state.mods,
                        &auto_category_claims,
                    );
                    Some(desired)
                }
            });
            if desired != Some(ModStatus::Active) {
                continue;
            }
            if mod_entry.status == ModStatus::Disabled {
                self.enable_or_restore_mod_by_id(&mod_entry.id);
            }
            if local_install_state == ModInstallState::Auto {
                if let Some(category_id) = effective_category_id(&self.state.categories, &mod_entry)
                    && self
                        .state
                        .mods
                        .iter()
                        .find(|entry| entry.id == mod_entry.id)
                        .is_some_and(|entry| entry.status == ModStatus::Active)
                {
                    auto_category_claims.insert(category_id.to_string());
                }
            }
        }
        // Auto status resolution can enable a staged candidate. Reconcile only
        // after that final status assignment so grouped matching never leaves
        // an enabled candidate stuck at the worker's temporary CheckSkipped.
        let affected_game_ids: HashSet<String> = newly_installed_ids
            .iter()
            .filter_map(|id| {
                self.state
                    .mods
                    .iter()
                    .find(|mod_entry| mod_entry.id == *id)
                    .map(|mod_entry| mod_entry.game_id.clone())
            })
            .collect();
        for game_id in affected_game_ids {
            self.reconcile_cached_grouped_update_states(Some(&game_id));
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
                let mod_root_and_game = self
                    .state
                    .mods
                    .iter()
                    .find(|m| m.id == mod_entry_id)
                    .map(|m| (m.root_path.clone(), m.game_id.clone()))
                    .or_else(|| {
                        self.install_inflight
                            .get(&job_id)
                            .map(|job| (first_path.clone(), job.game_id.clone()))
                    });
                if let Some((mod_root_path, game_id)) = mod_root_and_game
                    .filter(|(_, game_id)| !self.folder_batch_game_busy(game_id))
                {
                    if self.install_request_tx.send(InstallRequest::SyncImages {
                        job_id: image_job_id,
                        mod_entry_id,
                        mod_root_path,
                        profile,
                        profile_cached_at: pending_meta
                            .as_ref()
                            .and_then(|meta| meta.profile_cached_at),
                    }).is_ok() {
                        self.mod_image_sync_inflight.insert(image_job_id, game_id);
                    }
                }
            }
        }

        if let Some(id) = primary_id {
            match self.state.static_prefs.after_install_behavior {
                AfterInstallBehavior::DoNothing => {}
                AfterInstallBehavior::AddToSelection => {
                    self.begin_library_mod_selection();
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
        self.finish_game_overlay_install(job_id, &newly_installed_ids);
        newly_installed_ids
    }

    /// `always` gives the mod a category even when the setting for Browse
    /// downloads is off.
    fn apply_browse_download_category(
        &mut self,
        mod_entry_id: &str,
        pending_meta: Option<&PendingBrowseInstallMeta>,
        gb_profile: Option<&gamebanana::ProfileResponse>,
        always: bool,
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
        if !enabled && !always {
            return;
        }

        let Some(mod_index) = self.state.mods.iter().position(|mod_entry| {
            mod_entry.id == mod_entry_id && mod_entry.game_id == meta.game_id
        }) else {
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

        let remote_category_id = gb_profile
            .and_then(|profile| profile.category.as_ref())
            .map(|category| category.id)
            .filter(|id| *id != 0);
        // Set only for a character's mods.
        let character_super_category_id =
            gamebanana::character_super_category_id_for_hestia(&meta.game_id).filter(|id| {
                gb_profile
                    .and_then(|profile| profile.super_category.as_ref())
                    .is_some_and(|category| category.id == *id)
            });
        let (category_id, category_name) = if let Some(existing) = download_category(
            &self.state.categories,
            &self.state.mods,
            &meta.game_id,
            &category_name,
            remote_category_id,
            character_super_category_id,
        ) {
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
                gamebanana_character: None,
            });
            let text = self.text();
            self.log_action(
                text.category_action(),
                &text.category_created(&category_name),
            );
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
        let Some(meta) = pending_meta else {
            return;
        };
        let Some(target_mod_id) = meta.update_target_mod_id.as_deref() else {
            return;
        };
        let Some(mod_entry) =
            self.state.mods.iter_mut().find(|mod_entry| {
                mod_entry.id == target_mod_id && mod_entry.game_id == meta.game_id
            })
        else {
            return;
        };

        let now = Utc::now();
        let source = mod_entry.source.get_or_insert_with(ModSourceData::default);
        source.gamebanana = Some(GameBananaLink {
            mod_id: meta.mod_id,
            url: gamebanana::browser_url_typed(meta.mod_id, meta.is_tool),
        });
        let selected_candidate_labels = Vec::new();
        source.file_set = if meta.explicit_resolution == Some(ImportResolution::Merge) {
            merge_file_set_recipe(
                &source.file_set,
                &meta.selected_files,
                &selected_candidate_labels,
            )
        } else {
            FileSetRecipe {
                selected_file_ids: meta
                    .selected_files
                    .iter()
                    .map(|file| file.id)
                    .collect(),
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
                selected_candidate_labels,
            }
        };
        source.history.downloaded_at = Some(now);
        source.history.updated_at = Some(now);
        source.ignored_update_signature = None;
        if let Some(profile) = gb_profile {
            source.snapshot = Some(profile_to_snapshot(profile));
            source.raw_profile_json = serde_json::to_string(profile).ok();
            if let Some(profile_cached_at) = meta.profile_cached_at {
                source.profile_cached_at = Some(profile_cached_at);
            }
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
        let profile_cached_at = pending_meta
            .as_ref()
            .and_then(|meta| meta.profile_cached_at);
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
                url: gamebanana::browser_url_typed(meta.mod_id, meta.is_tool),
            });
            source.file_set = if meta.explicit_resolution == Some(ImportResolution::Merge) {
                merge_file_set_recipe(
                    &source.file_set,
                    &meta.selected_files,
                    &selected_candidate_labels,
                )
            } else {
                FileSetRecipe {
                    selected_file_ids: meta
                        .selected_files
                        .iter()
                        .map(|f| f.id)
                        .collect(),
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
                }
            };
            source.history.downloaded_at = Some(now);
            source.history.installed_at = Some(now);
            source.history.updated_at = Some(now);
            source.ignored_update_signature = None;
            source.baseline_content_mtime = mod_entry.content_mtime;
            source.baseline_ini_hash = mod_entry.ini_hash.clone();
            // A fresh baseline supersedes any "Ignore local changes" acceptance.
            source.accepted_local_changes = None;

            mod_entry.unsafe_content_auto = gb_profile
                .as_ref()
                .map(|p| !p.content_ratings.is_empty())
                .or_else(|| {
                    source
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.unsafe_content)
                })
                .unwrap_or(mod_entry.unsafe_content_auto);
            mod_entry.unsafe_content = mod_entry
                .unsafe_content_preference
                .resolve(mod_entry.unsafe_content_auto);

            if let Some(profile) = gb_profile.as_ref() {
                source.snapshot = Some(profile_to_snapshot(profile));
                source.raw_profile_json = serde_json::to_string(profile).ok();
                if let Some(profile_cached_at) = profile_cached_at {
                    source.profile_cached_at = Some(profile_cached_at);
                }
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
            let needs_sync = self
                .state
                .mods
                .iter()
                .find(|m| m.id == id)
                .map_or(false, |m| {
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
        profile_cached_at: Option<DateTime<Utc>>,
    ) {
        let target_indices: Vec<usize> = self.state.mods.iter().enumerate()
            .filter(|(_, m)| m.id == mod_entry_id)
            .map(|(i, _)| i)
            .collect();

        if target_indices.is_empty() { return; }

        let mut first_folder_name = String::new();
        let target_game_ids: HashSet<String> = target_indices
            .iter()
            .filter_map(|idx| self.state.mods.get(*idx).map(|mod_entry| mod_entry.game_id.clone()))
            .collect();
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
            Self::apply_sync_profile_metadata(mod_entry, &profile, profile_cached_at);
            let _ = xxmi::save_mod_metadata(mod_entry);
        }

        for game_id in target_game_ids {
            self.reconcile_cached_grouped_update_states(Some(&game_id));
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

    /// Image-only backfill may receive a profile reconstructed from the
    /// persisted snapshot. Only a profile accompanied by a real fetch time is
    /// allowed to replace the matching catalog and freshness timestamp.
    fn apply_sync_profile_metadata(
        mod_entry: &mut ModEntry,
        profile: &gamebanana::ProfileResponse,
        profile_cached_at: Option<DateTime<Utc>>,
    ) {
        let Some(cached_at) = profile_cached_at else {
            return;
        };
        let Some(source) = mod_entry.source.as_mut() else {
            return;
        };
        let _ = hydrate_tracked_file_sizes(
            &mut source.file_set,
            source.raw_profile_json.as_deref(),
            source.snapshot.as_ref(),
            Some(profile),
        );
        source.snapshot = Some(profile_to_snapshot(profile));
        source.raw_profile_json = serde_json::to_string(profile).ok();
        source.profile_cached_at = Some(cached_at);
        mod_entry.unsafe_content_auto = !profile.content_ratings.is_empty();
        mod_entry.unsafe_content = mod_entry
            .unsafe_content_preference
            .resolve(mod_entry.unsafe_content_auto);
        source.baseline_content_mtime = mod_entry.content_mtime;
        source.baseline_ini_hash = mod_entry.ini_hash.clone();
        // A fresh baseline supersedes any "Ignore local changes" acceptance.
        source.accepted_local_changes = None;
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
        if self.has_active_update_for_mod(&mod_entry) {
            return false;
        }
        let thalia_plan = self.thalia_update_install_plan(mod_entry_id);
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

        self.browse_state.pending_installs.push(PendingBrowseInstall {
            task_id,
            mod_id,
            game_id,
            update_target_id: Some(mod_entry_id.to_string()),
            thalia_plan,
            // Updates preserve the target state during finalization; staging
            // is still disabled so a failed refresh can never expose a new
            // active tree prematurely.
            install_state: ModInstallState::Auto,
            preserve_existing_state: true,
        });
        self.request_browse_detail_refresh(mod_id);
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

/// Resolve the Auto policy for one candidate after refresh has hydrated its
/// category. `mods` includes candidates from this and other ready jobs; the
/// caller's `claims` set records a successful enable whose status may not yet
/// be visible to a subsequent decision in the same frame.
fn auto_install_status(
    categories: &[ModCategory],
    candidate: &ModEntry,
    mods: &[ModEntry],
    claims: &HashSet<String>,
) -> ModStatus {
    let category_id = effective_category_id(categories, candidate);
    let category_has_active = category_id.is_some_and(|category_id| {
        mods.iter().any(|other| {
            other.id != candidate.id
                && other.game_id == candidate.game_id
                && effective_category_id(categories, other) == Some(category_id)
                && other.status == ModStatus::Active
        })
    });
    let first_candidate = category_id.is_some_and(|category_id| !claims.contains(category_id));
    ModInstallState::resolve_auto(category_id, category_has_active, first_candidate)
}

/// The category a mod from GameBanana category `name` (id `remote_id`) goes
/// into, if there is one: the only one linked to it by hand, then one with
/// its name. The link button is gone, but the links people made before stay.
/// A character's mods, whose `character_super_id` is the game's character
/// section, also go into one with its short name ("Tangtang" for "Operators:
/// Tangtang"), then the only one whose GameBanana mods are all its own, like
/// the overlay matches them. Several categories can share a character, so
/// none is picked over the others.
fn download_category<'a>(
    categories: &'a [ModCategory],
    mods: &[ModEntry],
    game_id: &str,
    name: &str,
    remote_id: Option<u64>,
    character_super_id: Option<u64>,
) -> Option<&'a ModCategory> {
    let game_categories = || {
        categories
            .iter()
            .filter(move |category| category.game_id == game_id)
    };
    let linked = only(game_categories().filter(|category| {
        category
            .gamebanana_character
            .as_ref()
            .is_some_and(|link| Some(link.id) == remote_id)
    }));
    if linked.is_some() {
        return linked;
    }
    if let Some(named) = game_categories().find(|category| category.name.eq_ignore_ascii_case(name)) {
        return Some(named);
    }
    let super_id = character_super_id?;
    if let Some(named) = game_categories()
        .find(|category| overlay_protocol::names_character(&category.name, name))
    {
        return Some(named);
    }
    let remote_id = remote_id?;
    let mut members: HashMap<&str, Vec<&GameBananaSnapshot>> = HashMap::new();
    for entry in mods.iter().filter(|entry| entry.game_id == game_id) {
        if let Some(category_id) = effective_category_id(categories, entry)
            && let Some(snapshot) = entry.source.as_ref().and_then(|source| source.snapshot.as_ref())
        {
            members.entry(category_id).or_default().push(snapshot);
        }
    }
    only(game_categories().filter(|category| {
        category
            .resolved_gamebanana_character(
                members.get(category.id.as_str()).into_iter().flatten().copied(),
                Some(super_id),
            )
            .is_some_and(|link| link.id == remote_id)
    }))
}

/// The item, when there is exactly one.
fn only<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

#[cfg(test)]
mod download_category_tests {
    use super::*;
    use crate::model::GameBananaCategoryLink;

    const OPERATORS: u64 = 42770;
    const TANGTANG: u64 = 7;
    const ARCLIGHT: u64 = 8;

    fn category(id: &str, name: &str) -> ModCategory {
        ModCategory {
            id: id.to_owned(),
            game_id: "endfield".to_owned(),
            name: name.to_owned(),
            order: 0,
            gamebanana_character: None,
        }
    }

    /// A mod in `category_id`, downloaded from the character `character`.
    fn mod_from(id: &str, category_id: &str, character: Option<u64>) -> ModEntry {
        let mut metadata = crate::model::ModMetadata::default();
        metadata.user.category_id = Some(category_id.to_owned());
        ModEntry {
            id: id.to_owned(),
            game_id: "endfield".to_owned(),
            folder_name: id.to_owned(),
            root_path: PathBuf::from(format!(r"D:\Mods\{id}")),
            status: ModStatus::Disabled,
            metadata,
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
            source: character.map(|character| ModSourceData {
                snapshot: Some(GameBananaSnapshot {
                    category: Some(GameBananaCategoryLink {
                        id: character,
                        name: String::new(),
                    }),
                    super_category_id: Some(OPERATORS),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            update_state: ModUpdateState::Unlinked,
        }
    }

    #[test]
    fn auto_install_uncategorized_candidate_stays_disabled() {
        let mut candidate = mod_from("new", "cat", None);
        candidate.metadata.user.category_id = None;
        assert_eq!(
            auto_install_status(&[category("cat", "Outfits")], &candidate, &[candidate.clone()], &HashSet::new()),
            ModStatus::Disabled
        );
    }

    #[test]
    fn auto_install_sees_active_peer_in_the_same_category() {
        let candidate = mod_from("new", "cat", None);
        let mut peer = mod_from("peer", "cat", None);
        peer.status = ModStatus::Active;
        assert_eq!(
            auto_install_status(
                &[category("cat", "Outfits")],
                &candidate,
                &[candidate.clone(), peer],
                &HashSet::new(),
            ),
            ModStatus::Disabled
        );
    }

    #[test]
    fn auto_install_resolves_legacy_category_names() {
        let mut candidate = mod_from("new", "cat", None);
        candidate.metadata.user.category_id = None;
        candidate.metadata.user.category = "Outfits".to_owned();
        assert_eq!(
            auto_install_status(
                &[category("cat", "Outfits")],
                &candidate,
                &[candidate.clone()],
                &HashSet::new(),
            ),
            ModStatus::Active
        );
    }

    #[test]
    fn auto_install_active_replacement_blocks_new_sibling() {
        let mut replacement = mod_from("replacement", "cat", None);
        replacement.status = ModStatus::Active;
        let candidate = mod_from("new", "cat", None);
        assert_eq!(
            auto_install_status(
                &[category("cat", "Outfits")],
                &candidate,
                &[replacement, candidate.clone()],
                &HashSet::new(),
            ),
            ModStatus::Disabled
        );
    }

    #[test]
    fn auto_install_enables_first_successful_candidate_only() {
        let first = mod_from("first", "cat", None);
        let mut second = mod_from("second", "cat", None);
        let categories = [category("cat", "Outfits")];
        let mut mods = vec![first.clone(), second.clone()];
        let mut claims = HashSet::new();
        assert_eq!(
            auto_install_status(&categories, &mods[0], &mods, &claims),
            ModStatus::Active
        );
        // Mimic the backend's successful enable and the finalizer's claim.
        mods[0].status = ModStatus::Active;
        claims.insert("cat".to_owned());
        second.status = ModStatus::Disabled;
        assert_eq!(
            auto_install_status(&categories, &mods[1], &mods, &claims),
            ModStatus::Disabled
        );
    }

    /// Where a Tangtang mod goes, by category id.
    fn tangtang(categories: &[ModCategory], mods: &[ModEntry]) -> Option<String> {
        download_category(
            categories,
            mods,
            "endfield",
            "Operators: Tangtang",
            Some(TANGTANG),
            Some(OPERATORS),
        )
        .map(|category| category.id.clone())
    }

    #[test]
    fn a_character_mod_finds_its_category_by_name() {
        let both = [
            category("short", "tangtang"),
            category("full", "Operators: Tangtang"),
        ];
        assert_eq!(
            tangtang(&both, &[]).as_deref(),
            Some("full"),
            "GameBanana's name first"
        );
        assert_eq!(
            tangtang(&both[..1], &[]).as_deref(),
            Some("short"),
            "then the short name"
        );

        let mut other_game = category("short", "Tangtang");
        other_game.game_id = "zzz".to_owned();
        assert_eq!(tangtang(&[other_game], &[]), None, "another game's");

        let swords = [category("swords", "Swords")];
        assert_eq!(
            download_category(&swords, &[], "endfield", "Weapons: Swords", Some(3), None)
                .map(|category| category.id.as_str()),
            None,
            "only a character's mods go by the short name"
        );
        assert_eq!(
            download_category(&swords, &[], "endfield", "swords", Some(3), None)
                .map(|category| category.id.as_str()),
            Some("swords"),
            "any mod goes by the whole name"
        );
    }

    #[test]
    fn a_character_mod_finds_its_category_by_the_mods_inside() {
        let categories = [
            category("tang", "Tang"),
            category("mixed", "Mixed"),
            category("arc", "Operators: Arclight"),
        ];
        let mut mods = vec![
            mod_from("t1", "tang", Some(TANGTANG)),
            mod_from("t2", "tang", None),
            mod_from("m1", "mixed", Some(TANGTANG)),
            mod_from("m2", "mixed", Some(ARCLIGHT)),
        ];
        assert_eq!(tangtang(&categories, &mods).as_deref(), Some("tang"));

        mods.push(mod_from("a1", "arc", Some(TANGTANG)));
        assert_eq!(tangtang(&categories, &mods), None, "two categories hold only Tangtang");

        let mut linked = categories.clone();
        linked[1].gamebanana_character = Some(GameBananaCategoryLink {
            id: TANGTANG,
            name: "Tangtang".to_owned(),
        });
        assert_eq!(tangtang(&linked, &mods).as_deref(), Some("mixed"), "a link made by hand");
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
    use serde::Deserialize;

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
            md5: None,
        }
    }

    #[test]
    fn merge_recipe_preserves_legacy_archived_files_and_deduplicates_new_ids() {
        let mut archived = mod_file(11, "old-archived.zip", 100);
        archived.is_archived = true;
        let mut existing = FileSetRecipe {
            selected_file_ids: vec![11, 11],
            selected_file_names: vec!["old-archived.zip".to_string()],
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&archived)],
            selected_candidate_labels: vec!["Legacy".to_string()],
        };
        let mut legacy = mod_file(0, "legacy-name-only.zip", 90);
        legacy.description = Some("Legacy candidate".to_string());
        existing.selected_files_meta.push(tracked_file_meta_from_mod_file(&legacy));

        let new_file = mod_file(22, "new.zip", 200);
        let merged = merge_file_set_recipe(
            &existing,
            &[new_file.clone(), new_file],
            &["Legacy".to_string(), "Current".to_string()],
        );

        assert!(merged.selected_file_ids.is_empty());
        assert_eq!(
            merged.selected_file_names,
            vec!["old-archived.zip", "legacy-name-only.zip", "new.zip"]
        );
        assert!(merged
            .selected_files_meta
            .iter()
            .any(|file| file.file_id == 11 && file.archived));
        assert!(merged
            .selected_files_meta
            .iter()
            .any(|file| file.file_id == 0 && file.file_name == "legacy-name-only.zip"));
        assert_eq!(merged.selected_files_meta.iter().filter(|file| file.file_id == 22).count(), 1);
        assert_eq!(merged.selected_candidate_labels, vec!["Legacy", "Current"]);
        assert!(selector_shape_is_valid(&merged));
    }

    #[test]
    fn merge_recipe_keeps_id_only_legacy_selector_without_empty_names() {
        let existing = FileSetRecipe {
            selected_file_ids: vec![11],
            selected_file_names: Vec::new(),
            selected_files_meta: Vec::new(),
            selected_candidate_labels: Vec::new(),
        };
        let merged = merge_file_set_recipe(&existing, &[mod_file(22, "new.zip", 200)], &[]);

        assert_eq!(merged.selected_file_ids, vec![11, 22]);
        assert!(merged.selected_file_names.is_empty());
        assert!(selector_shape_is_valid(&merged));
        assert!(merged
            .selected_files_meta
            .iter()
            .any(|file| file.file_id == 11 && file.file_name.is_empty()));
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
    fn undecided_recipe_asks_for_every_archive_list_of_its_group() {
        let source = mod_file(10, "skin.zip", 100);
        let first = mod_file(20, "skin_a.zip", 200);
        let second = mod_file(21, "skin_b.zip", 200);
        let mut remote = profile(vec![source.clone(), first, second], 300);
        let items = [(Some(source.date_added), fallback_file_set(&source))];
        let list = |name: &str| vec![format!("Skin/{name}.ini"), "Skin/body.dds".to_owned()];

        assert_eq!(file_listing_ids_wanted(&items, &remote), vec![10, 20, 21]);

        remote.file_listings.insert(10, list("skin"));
        remote.file_listings.insert(20, list("skin"));
        assert_eq!(file_listing_ids_wanted(&items, &remote), vec![21]);

        remote.file_listings.insert(21, list("other"));
        assert!(file_listing_ids_wanted(&items, &remote).is_empty());
        // Thin metadata: the path model may not pick either, but the lists
        // never turn the recipe into a skipped check.
        let evaluations = evaluate_file_set_update_group(&items, &remote);
        assert_ne!(evaluations[0].state, ModUpdateState::CheckSkipped);
    }

    #[test]
    fn native_group_skips_every_recipe_for_conflicting_catalog_id() {
        let source = mod_file(10, "old.zip", 100);
        let mut conflicting = source.clone();
        conflicting.file_name = "different.zip".to_owned();
        conflicting.date_added = 101;
        let remote = profile(vec![source.clone(), conflicting], 200);
        let item = (
            Some(source.date_added),
            fallback_file_set(&source),
        );

        let evaluations = evaluate_file_set_update_group(&[item], &remote);

        assert_eq!(evaluations[0].state, ModUpdateState::CheckSkipped);
        assert!(evaluations[0].signature.is_none());
    }

    #[test]
    fn identical_catalog_duplicates_deduplicate_and_archived_wins() {
        let source = mod_file(10, "old.zip", 100);
        let mut archived = source.clone();
        archived.is_archived = true;
        let remote = gamebanana::ProfileResponse {
            files: vec![source.clone(), source.clone()],
            archived_files: vec![archived.clone()],
            ..profile(Vec::new(), 200)
        };
        let canonical = matcher_adapter::canonicalize_profile(&remote);

        assert!(canonical.active.is_empty());
        assert_eq!(canonical.archived.len(), 1);
        assert_eq!(canonical.archived[0].id, archived.id);
        assert!(canonical.ambiguous_ids.is_empty());
    }

    #[test]
    fn active_flag_only_archived_row_stays_archived_and_ineligible() {
        let mut source = mod_file(10, "old.zip", 100);
        source.is_archived = true;
        let remote = profile(vec![source], 200);
        let canonical = matcher_adapter::canonicalize_profile(&remote);

        assert_eq!(canonical.active.len(), 1);
        assert!(canonical.active[0].is_archived);
        assert!(downloadable_active_files(&canonical.profile).is_empty());
    }

    #[test]
    fn newer_file_signal_uses_known_archive_date_and_active_downloads_only() {
        let source = mod_file(10, "old.zip", 100);
        let valid = mod_file(20, "new.zip", 200);
        let mut blank_url = mod_file(21, "blank.zip", 300);
        blank_url.download_url = Some("  ".to_owned());
        let mut zero_id = mod_file(0, "zero.zip", 400);
        zero_id.download_url = Some("https://example.com/zero.zip".to_owned());
        let mut archived = mod_file(22, "archived.zip", 500);
        archived.is_archived = true;
        let remote = gamebanana::ProfileResponse {
            files: vec![source.clone(), valid, blank_url, zero_id],
            archived_files: vec![archived],
            ..profile(Vec::new(), 500)
        };

        let newer = available_newer_files(&fallback_file_set(&source), &remote);

        assert_eq!(newer.iter().map(|file| file.id).collect::<Vec<_>>(), vec![20]);
    }

    #[test]
    fn newer_file_signal_keeps_deleted_source_baseline_and_resurfaces_new_ids() {
        let mut deleted_source = mod_file(10, "old.zip", 100);
        deleted_source.download_url = None;
        deleted_source.is_archived = true;
        let candidate = mod_file(20, "new.zip", 200);
        let remote = profile(vec![candidate.clone()], 200);
        let file_set = fallback_file_set(&deleted_source);

        assert_eq!(available_newer_files(&file_set, &remote)[0].id, 20);
        let mut dismissed = ModSourceData::default();
        let legacy = serde_json::to_value(&dismissed).unwrap();
        dismissed.dismissed_newer_file_ids = vec![20];
        let roundtrip: ModSourceData = serde_json::from_value(serde_json::to_value(&dismissed).unwrap()).unwrap();
        assert_eq!(roundtrip.dismissed_newer_file_ids, vec![20]);
        let legacy_roundtrip: ModSourceData = serde_json::from_value(legacy).unwrap();
        assert!(legacy_roundtrip.dismissed_newer_file_ids.is_empty());

        let candidate_2 = mod_file(21, "newer.zip", 300);
        let remote_2 = profile(vec![candidate.clone(), candidate_2], 300);
        let newer = available_newer_files(&file_set, &remote_2);
        assert_eq!(newer.iter().map(|file| file.id).collect::<Vec<_>>(), vec![21, 20]);
    }

    #[test]
    fn newer_file_signal_suppresses_unknown_or_ambiguous_baseline() {
        let source = mod_file(10, "old.zip", 100);
        let candidate = mod_file(20, "new.zip", 200);
        let remote = profile(vec![source.clone(), candidate.clone()], 200);
        let partial = FileSetRecipe {
            selected_files_meta: vec![TrackedFileMeta {
                file_id: 99,
                file_name: "missing.zip".to_owned(),
                date_added: 0,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(available_newer_files(&partial, &remote).is_empty());
        let malformed_known_date = FileSetRecipe {
            selected_files_meta: vec![TrackedFileMeta {
                file_id: 0,
                file_name: "  ".to_owned(),
                date_added: 100,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(available_newer_files(&malformed_known_date, &remote).is_empty());

        let mut duplicate = source.clone();
        duplicate.file_name = "different.zip".to_owned();
        let ambiguous = profile(vec![source, duplicate, candidate], 200);
        assert!(available_newer_files(&fallback_file_set(&ambiguous.files[0]), &ambiguous).is_empty());
    }

    /// Mod 728709: the main file was installed as one mod and a small extra
    /// as another.  The main file is newer than the extra, but it is not news
    /// for the extra's mod, while a truly new file still is.
    #[test]
    fn files_other_mods_of_the_page_have_are_not_newer_files() {
        let extra = mod_file(10, "neonrookiefx.zip", 100);
        let between = mod_file(30, "hair_addon.zip", 150);
        let main = mod_file(20, "inkspirit.zip", 200);
        let page = profile(vec![main.clone(), extra.clone()], 200);
        let mut extra_mod = cached_mod_entry("extra", &page, Utc::now());
        extra_mod.source.as_mut().expect("source").file_set = fallback_file_set(&extra);
        let mut main_mod = cached_mod_entry("main", &page, Utc::now());
        main_mod.source.as_mut().expect("source").file_set = fallback_file_set(&main);

        let evidence = build_cached_update_evidence(&[extra_mod.clone(), main_mod.clone()], None);
        let for_extra = &evidence["extra"];
        assert!(for_extra.newer_files.is_empty());
        assert_eq!(for_extra.other_mods_files, vec![20]);
        assert_eq!(evidence["main"].other_mods_files, vec![10]);

        let page = profile(vec![main.clone(), between.clone(), extra.clone()], 200);
        for mod_entry in [&mut extra_mod, &mut main_mod] {
            mod_entry.source.as_mut().expect("source").raw_profile_json =
                serde_json::to_string(&page).ok();
        }
        let evidence = build_cached_update_evidence(&[extra_mod, main_mod.clone()], None);
        let newer: Vec<u64> = evidence["extra"].newer_files.iter().map(|file| file.id).collect();
        assert_eq!(newer, vec![30]);

        // Alone on the page, the main file is news again.
        let evidence = build_cached_update_evidence(
            &[cached_mod_entry_with_file("extra", &page, &extra)],
            None,
        );
        let newer: Vec<u64> = evidence["extra"].newer_files.iter().map(|file| file.id).collect();
        assert_eq!(newer, vec![20, 30]);
    }

    #[test]
    fn claimed_files_include_a_pending_update() {
        let installed = fallback_file_set(&mod_file(10, "a.zip", 100));
        let pending = IgnoredUpdateSignature {
            files: vec![tracked_file_meta_from_mod_file(&mod_file(20, "b.zip", 200))],
            ..Default::default()
        };
        let mut claimed: Vec<u64> = claimed_file_ids(&installed, Some(&pending)).into_iter().collect();
        claimed.sort_unstable();
        assert_eq!(claimed, vec![10, 20]);
        assert_eq!(claimed_file_ids(&installed, None), HashSet::from([10]));
    }

    fn cached_mod_entry_with_file(
        id: &str,
        page: &gamebanana::ProfileResponse,
        file: &gamebanana::ModFile,
    ) -> ModEntry {
        let mut mod_entry = cached_mod_entry(id, page, Utc::now());
        mod_entry.source.as_mut().expect("source").file_set = fallback_file_set(file);
        mod_entry
    }

    #[test]
    fn newer_file_notice_respects_smart_precedence_and_ignore_always() {
        let source = mod_file(10, "old.zip", 100);
        let candidate = mod_file(20, "new.zip", 200);
        let profile = profile(vec![source.clone()], 100);
        let mut mod_entry = cached_mod_entry("notice", &profile, Utc::now());
        let evidence = CachedUpdateEvidence {
            state: ModUpdateState::UpToDate,
            signature: None,
            prearmed_signature: None,
            newer_files: vec![candidate.clone()],
            installed_files: vec![tracked_file_meta_from_mod_file(&source)],
            other_mods_files: Vec::new(),
            profile: Some(Arc::new(profile.clone())),
            profile_cached_at: None,
            thalia_pending: false,
        };
        assert!(HestiaApp::has_newer_files_notice_from_evidence(
            &mod_entry,
            &evidence,
            ModStatusTargets::default(),
        ));
        let mut smart = evidence.clone();
        smart.state = ModUpdateState::UpdateAvailable;
        assert!(!HestiaApp::has_newer_files_notice_from_evidence(
            &mod_entry,
            &smart,
            ModStatusTargets::default(),
        ));
        mod_entry
            .source
            .as_mut()
            .expect("source")
            .ignore_update_always = true;
        assert!(!HestiaApp::has_newer_files_notice_from_evidence(
            &mod_entry,
            &evidence,
            ModStatusTargets::default(),
        ));
    }

    #[test]
    fn native_group_requires_a_known_positive_source_date() {
        let source = mod_file(10, "old.zip", 0);
        let candidate = mod_file(20, "new.zip", 200);
        let remote = profile(vec![source.clone(), candidate], 200);
        let item = (Some(0), fallback_file_set(&source));

        let evaluations = evaluate_file_set_update_group(&[item], &remote);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert!(evaluations[0].signature.is_none());
    }

    #[test]
    fn native_native_candidate_collision_abstains_both_recipes() {
        let candidate_id = 77;
        let mut evaluations = vec![
            native_test_evaluation(0, candidate_id),
            native_test_evaluation(1, candidate_id),
        ];

        matcher_adapter::abstain_competing_native_claims(&mut evaluations, &HashSet::new());

        assert!(evaluations.iter().all(|evaluation| {
            evaluation.claim.is_none()
                && evaluation.evaluation.signature.is_none()
                && evaluation.evaluation.state == ModUpdateState::UpToDate
        }));
    }

    #[test]
    fn native_multi_candidate_collision_abstains_native_recipe_whole() {
        let candidate_id = 77;
        let mut evaluations = vec![native_test_evaluation(0, candidate_id)];

        // This ID represents a complete legacy multi-file assignment.  The
        // native recipe must clear its entire result when the group owner is
        // already claimed elsewhere; callers clear the legacy signature too.
        matcher_adapter::abstain_competing_native_claims(
            &mut evaluations,
            &HashSet::from([candidate_id]),
        );

        assert!(evaluations[0].claim.is_none());
        assert!(evaluations[0].evaluation.signature.is_none());
        assert_eq!(evaluations[0].evaluation.state, ModUpdateState::UpToDate);
    }

    fn native_test_evaluation(
        recipe_index: usize,
        candidate_id: u64,
    ) -> matcher_adapter::NativeSingleEvaluation {
        matcher_adapter::NativeSingleEvaluation {
            evaluation: FileSetUpdateEvaluation {
                state: ModUpdateState::UpdateAvailable,
                signature: Some(IgnoredUpdateSignature {
                    files: vec![TrackedFileMeta {
                        file_id: candidate_id,
                        file_name: "new.zip".to_owned(),
                        file_size: Some(1),
                        date_added: 200,
                        version: None,
                        archived: false,
                        label: None,
                    }],
                    profile_update_ts: None,
                    prearmed_next_update: false,
                }),
            },
            claim: Some(matcher_adapter::NativeClaim {
                recipe_index,
                candidate_id,
            }),
            is_native_single: true,
            abstention_state: ModUpdateState::UpToDate,
            missing_listing_ids: Vec::new(),
        }
    }

    #[test]
    fn snapshot_only_profile_reconstructs_files_for_update_checks() {
        let source = versioned_mod_file(10, "legacy.zip", 100, "1.0");
        let mut candidate = versioned_mod_file(20, "legacyplus.zip", 200, "2.0");
        candidate.md5 = Some("5d41402abc4b2a76b9719d911017c592".to_string());
        let mut archived = mod_file(30, "legacy_old.zip", 50);
        archived.is_archived = true;
        archived.version = Some("0.9".to_string());
        let mut remote = profile(vec![source.clone(), candidate], 200);
        remote.archived_files = vec![archived];

        let snapshot = profile_to_snapshot(&remote);
        let reconstructed = profile_to_response(Some(&snapshot), 42);

        assert!(!gamebanana::is_unavailable(&reconstructed));
        assert_eq!(reconstructed.id, 42);
        assert_eq!(reconstructed.files.len(), 2);
        assert_eq!(reconstructed.archived_files.len(), 1);
        assert_eq!(reconstructed.files[0].id, source.id);
        assert_eq!(reconstructed.archived_files[0].id, 30);
        assert_eq!(reconstructed.files[0].version.as_deref(), Some("1.0"));
        assert_eq!(reconstructed.archived_files[0].version.as_deref(), Some("0.9"));
        assert_eq!(
            reconstructed.files[1].md5.as_deref(),
            Some("5d41402abc4b2a76b9719d911017c592")
        );
        assert_eq!(reconstructed.files[0].md5, None);
        assert_ne!(
            determine_file_set_update_state(
                &fallback_file_set(&source),
                Some(source.date_added),
                &reconstructed,
            ),
            ModUpdateState::MissingSource
        );
        assert_eq!(
            determine_update_state(Some(source.date_added), &reconstructed),
            ModUpdateState::UpdateAvailable
        );
    }

    #[test]
    fn tracked_file_meta_size_is_optional_for_legacy_metadata() {
        let legacy = serde_json::json!({
            "file_id": 10,
            "file_name": "legacy.zip",
            "date_added": 100,
            "version": null,
            "archived": false
        });
        let parsed: TrackedFileMeta = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.file_size, None);

        let source = mod_file(10, "legacy.zip", 100);
        let tracked = tracked_file_meta_from_mod_file(&source);
        assert_eq!(tracked.file_size, Some(source.file_size));
        let roundtrip: TrackedFileMeta =
            serde_json::from_value(serde_json::to_value(&tracked).unwrap()).unwrap();
        assert_eq!(roundtrip.file_size, Some(source.file_size));
    }

    #[test]
    fn tracked_file_meta_signature_equality_ignores_label_and_size() {
        let source = mod_file(10, "legacy.zip", 100);
        let mut left = tracked_file_meta_from_mod_file(&source);
        let mut right = left.clone();
        left.file_size = None;
        left.label = None;
        right.file_size = Some(999);
        right.label = Some("Main".to_owned());
        assert_eq!(left, right);
    }

    #[test]
    fn profile_cache_timestamp_is_optional_for_legacy_source_metadata() {
        let source = ModSourceData::default();
        let mut legacy = serde_json::to_value(&source).unwrap();
        legacy
            .as_object_mut()
            .expect("source metadata object")
            .remove("profile_cached_at");
        let parsed: ModSourceData = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.profile_cached_at, None);

        let mut current = ModSourceData::default();
        current.profile_cached_at = Some(Utc::now());
        let roundtrip: ModSourceData =
            serde_json::from_value(serde_json::to_value(&current).unwrap()).unwrap();
        assert_eq!(roundtrip.profile_cached_at, current.profile_cached_at);
    }

    fn cached_mod_entry(
        id: &str,
        profile: &gamebanana::ProfileResponse,
        cached_at: DateTime<Utc>,
    ) -> ModEntry {
        let source_file = profile.files.first().expect("cached source file");
        ModEntry {
            id: id.to_owned(),
            game_id: "game".to_owned(),
            folder_name: id.to_owned(),
            root_path: PathBuf::from(id),
            status: ModStatus::Active,
            metadata: crate::model::ModMetadata::default(),
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
            source: Some(ModSourceData {
                gamebanana: Some(GameBananaLink {
                    mod_id: profile.id,
                    url: gamebanana::browser_url(profile.id),
                }),
                raw_profile_json: serde_json::to_string(profile).ok(),
                profile_cached_at: Some(cached_at),
                file_set: fallback_file_set(source_file),
                ..Default::default()
            }),
            update_state: ModUpdateState::UpToDate,
        }
    }

    #[test]
    fn synthetic_image_sync_preserves_profile_cache_until_real_fetch() {
        let source = mod_file(10, "source.zip", 100);
        let old_profile = profile(vec![source.clone()], 100);
        let old_cached_at = Utc::now();
        let mut mod_entry = cached_mod_entry("source", &old_profile, old_cached_at);
        let old_raw = mod_entry
            .source
            .as_ref()
            .and_then(|source| source.raw_profile_json.clone())
            .expect("old cached profile");

        let candidate = mod_file(20, "source-v2.zip", 200);
        let fresh_profile = profile(vec![source, candidate], 200);
        HestiaApp::apply_sync_profile_metadata(&mut mod_entry, &fresh_profile, None);
        let source_metadata = mod_entry.source.as_ref().expect("source metadata");
        assert_eq!(source_metadata.raw_profile_json.as_deref(), Some(old_raw.as_str()));
        assert_eq!(source_metadata.profile_cached_at, Some(old_cached_at));

        let fresh_cached_at = old_cached_at + chrono::Duration::nanoseconds(1);
        HestiaApp::apply_sync_profile_metadata(
            &mut mod_entry,
            &fresh_profile,
            Some(fresh_cached_at),
        );
        let source_metadata = mod_entry.source.as_ref().expect("source metadata");
        assert_eq!(source_metadata.profile_cached_at, Some(fresh_cached_at));
        assert_eq!(
            source_metadata.raw_profile_json.as_deref(),
            serde_json::to_string(&fresh_profile).ok().as_deref()
        );
    }

    #[test]
    fn equal_freshness_conflicting_cached_catalogs_provide_no_grouped_evidence() {
        let source = mod_file(10, "source.zip", 100);
        let mut first = profile(vec![source.clone()], 100);
        first.id = 7;
        let mut second_source = source.clone();
        second_source.file_name = "renamed-source.zip".to_owned();
        let mut second = profile(vec![second_source], 100);
        second.id = 7;
        let cached_at = Utc::now();
        let mut first_mod = cached_mod_entry("first", &first, cached_at);
        first_mod.update_state = ModUpdateState::UpdateAvailable;
        let mods = vec![
            first_mod,
            cached_mod_entry("second", &second, cached_at),
        ];

        let evidence = build_cached_update_evidence(&mods, Some("game"));
        assert_eq!(evidence.len(), 2);
        assert!(evidence.values().all(|value| {
            value.state == ModUpdateState::CheckSkipped
                && value.signature.is_none()
                && value.prearmed_signature.is_none()
        }));
    }

    #[test]
    fn cached_freshness_preserves_nanoseconds_and_observed_cache_precedence() {
        let profile = profile(vec![mod_file(10, "source.zip", 100)], 100);
        let observed_at = Utc::now();
        let later_observed_at = observed_at + chrono::Duration::nanoseconds(1);
        let mut observed_source = ModSourceData::default();
        observed_source.profile_cached_at = Some(observed_at);
        let mut later_source = ModSourceData::default();
        later_source.profile_cached_at = Some(later_observed_at);
        assert!(cached_profile_freshness(&later_source, &profile)
            > cached_profile_freshness(&observed_source, &profile));

        let mut legacy_profile = profile.clone();
        legacy_profile.files[0].date_added = i64::MAX;
        let legacy_source = ModSourceData::default();
        assert!(cached_profile_freshness(&observed_source, &profile)
            > cached_profile_freshness(&legacy_source, &legacy_profile));
    }

    #[test]
    fn cached_catalog_fingerprint_ignores_counters_and_file_order() {
        let source = mod_file(10, "source.zip", 100);
        let second = mod_file(20, "new.zip", 200);
        let mut first = profile(vec![source.clone(), second.clone()], 200);
        let mut changed_counters = first.clone();
        changed_counters.files.reverse();
        changed_counters.files[0].download_count = 999;
        assert_eq!(
            cached_profile_catalog_fingerprint(&first),
            cached_profile_catalog_fingerprint(&changed_counters)
        );

        first.files[0].file_size += 1;
        assert_ne!(
            cached_profile_catalog_fingerprint(&first),
            cached_profile_catalog_fingerprint(&changed_counters)
        );
    }

    #[test]
    fn cached_catalog_fingerprint_distinguishes_unknown_and_known_empty_history() {
        let profile_without_history = profile(vec![mod_file(10, "source.zip", 100)], 100);
        let mut profile_with_known_empty_history = profile_without_history.clone();
        profile_with_known_empty_history.update_history = Some(gamebanana::UpdateHistory {
            records: Vec::new(),
        });
        assert_ne!(
            cached_profile_catalog_fingerprint(&profile_without_history),
            cached_profile_catalog_fingerprint(&profile_with_known_empty_history)
        );
        let mut changed_profile_date = profile_without_history.clone();
        changed_profile_date.date_modified += 1;
        assert_ne!(
            cached_profile_catalog_fingerprint(&profile_without_history),
            cached_profile_catalog_fingerprint(&changed_profile_date)
        );

        // A Thalia plan made from a page saved by Browse (no history) must
        // still match the same page loaded with history for the update.
        assert_eq!(
            thalia_profile_fingerprint(&profile_without_history),
            thalia_profile_fingerprint(&profile_with_known_empty_history)
        );
        assert_ne!(
            thalia_profile_fingerprint(&profile_without_history),
            thalia_profile_fingerprint(&changed_profile_date)
        );
    }

    #[test]
    fn library_cache_hash_distinguishes_snapshot_history_presence() {
        fn source_hash(source: &ModSourceData) -> u64 {
            let mut hasher = DefaultHasher::new();
            HestiaApp::hash_mod_source_for_library_cache(&mut hasher, Some(source));
            hasher.finish()
        }

        let mut without_history = ModSourceData::default();
        without_history.snapshot = Some(GameBananaSnapshot::default());
        let mut with_known_empty_history = without_history.clone();
        with_known_empty_history
            .snapshot
            .as_mut()
            .expect("snapshot")
            .update_history = Some(gamebanana::UpdateHistory { records: Vec::new() });

        assert_ne!(
            source_hash(&without_history),
            source_hash(&with_known_empty_history)
        );
    }

    #[test]
    fn grouped_evidence_key_uses_matching_inputs_only() {
        let base = cached_mod_entry(
            "source",
            &profile(vec![mod_file(10, "source.zip", 100)], 100),
            Utc::now(),
        );
        let presentation_variant = base.clone();
        assert_eq!(
            HestiaApp::grouped_update_evidence_cache_key_for_mods(&[base.clone()]),
            HestiaApp::grouped_update_evidence_cache_key_for_mods(&[presentation_variant])
        );

        let mut recipe_variant = base;
        recipe_variant
            .source
            .as_mut()
            .expect("cached source")
            .file_set
            .selected_file_names
            .push("renamed.zip".to_owned());
        assert_ne!(
            HestiaApp::grouped_update_evidence_cache_key_for_mods(&[recipe_variant]),
            HestiaApp::grouped_update_evidence_cache_key_for_mods(&[cached_mod_entry(
                "source",
                &profile(vec![mod_file(10, "source.zip", 100)], 100),
                Utc::now(),
            )])
        );
    }

    #[test]
    fn cached_group_evidence_keeps_ignored_peers_in_collision_matching() {
        let source = mod_file(10, "source.zip", 100);
        let candidate = mod_file(20, "source-v2.zip", 200);
        let remote = profile(vec![source.clone(), candidate], 200);
        let first = cached_mod_entry("first", &remote, Utc::now());
        let mut ignored = cached_mod_entry("ignored", &remote, Utc::now());
        ignored
            .source
            .as_mut()
            .expect("cached source")
            .ignore_update_always = true;

        let mods = vec![first, ignored.clone()];
        let evidence = build_cached_update_evidence(&mods, Some("game"));
        assert_eq!(evidence.len(), 2, "ignored peers remain collision evidence");
        assert!(
            evidence
                .values()
                .filter(|evidence| evidence.state == ModUpdateState::UpdateAvailable)
                .count()
                <= 1,
            "a target cannot be assigned to both installed peers"
        );

        let grouped = evidence.get("ignored").expect("ignored peer evidence");
        let source = ignored
            .source
            .as_mut()
            .expect("ignored peer source");
        assert_eq!(
            apply_ignored_update_override_with_signature(
                source,
                grouped.state,
                grouped.signature.clone().or(grouped.prearmed_signature.clone()),
            ),
            ModUpdateState::IgnoringUpdateAlways
        );
    }

    #[test]
    fn cached_up_to_date_group_preserves_prearmed_signature_for_next_release() {
        let source = versioned_mod_file(10, "current_v1.zip", 100, "1");
        let old_profile = profile(vec![source.clone()], 100);
        let old_mod = cached_mod_entry("source", &old_profile, Utc::now());
        let old_evidence = build_cached_update_evidence(std::slice::from_ref(&old_mod), Some("game"));
        let prearmed = old_evidence
            .get("source")
            .and_then(|evidence| evidence.prearmed_signature.clone())
            .expect("up-to-date source has a prearmed signature");
        assert!(prearmed.prearmed_next_update);

        let candidate = versioned_mod_file(20, "current_v2.zip", 200, "2");
        let new_profile = profile(vec![source.clone(), candidate], 200);
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &new_profile,
        )
        .into_iter()
        .next()
        .expect("single grouped evaluation");
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);

        let mut source_metadata = old_mod
            .source
            .expect("source metadata")
            .clone();
        source_metadata.ignored_update_signature = Some(prearmed);
        assert_eq!(
            apply_ignored_update_override_with_signature(
                &mut source_metadata,
                evaluation.state,
                evaluation.signature,
            ),
            ModUpdateState::IgnoringUpdateOnce
        );
        assert!(!source_metadata
            .ignored_update_signature
            .as_ref()
            .expect("consumed ignore-once signature")
            .prearmed_next_update);
    }

    #[test]
    fn tracked_file_meta_size_hydrates_from_legacy_cache_then_snapshot_then_new_profile() {
        let mut source = mod_file(10, "legacy.zip", 100);
        source.file_size = 321;
        let mut file_set = FileSetRecipe {
            selected_files_meta: vec![TrackedFileMeta {
                file_id: source.id,
                file_name: source.file_name.clone(),
                date_added: source.date_added,
                ..Default::default()
            }],
            ..Default::default()
        };
        let old_profile = profile(vec![source.clone()], 100);
        let old_raw = serde_json::to_string(&old_profile).unwrap();
        assert!(hydrate_tracked_file_sizes(
            &mut file_set,
            Some(&old_raw),
            None,
            None,
        ));
        assert_eq!(file_set.selected_files_meta[0].file_size, Some(321));

        file_set.selected_files_meta[0].file_size = None;
        let old_snapshot = profile_to_snapshot(&old_profile);
        assert!(hydrate_tracked_file_sizes(
            &mut file_set,
            None,
            Some(&old_snapshot),
            None,
        ));
        assert_eq!(file_set.selected_files_meta[0].file_size, Some(321));

        file_set.selected_files_meta[0].file_size = None;
        assert!(hydrate_tracked_file_sizes(
            &mut file_set,
            None,
            None,
            Some(&old_profile),
        ));
        assert_eq!(file_set.selected_files_meta[0].file_size, Some(321));
    }

    #[test]
    fn deleted_source_refresh_keeps_cached_archive_size_before_evaluation() {
        let mut source = mod_file(10, "legacy.zip", 100);
        source.file_size = 654;
        let old_profile = profile(vec![source.clone()], 100);
        let old_raw = serde_json::to_string(&old_profile).unwrap();
        let candidate = mod_file(20, "replacement.zip", 200);
        let new_profile = profile(vec![candidate], 200);
        let mut file_set = FileSetRecipe {
            selected_files_meta: vec![TrackedFileMeta {
                file_id: source.id,
                file_name: source.file_name.clone(),
                date_added: source.date_added,
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(hydrate_tracked_file_sizes(
            &mut file_set,
            Some(&old_raw),
            None,
            Some(&new_profile),
        ));
        assert_eq!(file_set.selected_files_meta[0].file_size, Some(654));
        assert_eq!(
            determine_file_set_update_state(&file_set, Some(source.date_added), &new_profile),
            ModUpdateState::MissingSource
        );
    }

    #[test]
    fn legacy_id_only_recipe_reconstructs_deleted_source_from_old_raw_cache() {
        let mut source = mod_file(10, "legacy.zip", 100);
        source.file_size = 654;
        let old_raw = serde_json::to_string(&profile(vec![source.clone()], 100)).unwrap();
        let new_profile = profile(vec![mod_file(20, "replacement.zip", 200)], 200);
        let mut file_set = FileSetRecipe {
            selected_file_ids: vec![source.id],
            ..Default::default()
        };

        assert!(hydrate_tracked_file_sizes(
            &mut file_set,
            Some(&old_raw),
            None,
            Some(&new_profile),
        ));
        assert_eq!(file_set.selected_files_meta.len(), 1);
        assert_eq!(file_set.selected_files_meta[0].file_id, source.id);
        assert_eq!(file_set.selected_files_meta[0].file_size, Some(654));
    }

    #[test]
    fn legacy_name_only_recipe_reconstructs_deleted_source_from_old_snapshot() {
        let mut source = mod_file(10, "legacy.zip", 100);
        source.file_size = 987;
        let old_profile = profile(vec![source.clone()], 100);
        let old_snapshot = profile_to_snapshot(&old_profile);
        let new_profile = profile(vec![mod_file(20, "replacement.zip", 200)], 200);
        let mut file_set = FileSetRecipe {
            selected_file_names: vec![source.file_name.clone()],
            ..Default::default()
        };

        assert!(hydrate_tracked_file_sizes(
            &mut file_set,
            None,
            Some(&old_snapshot),
            Some(&new_profile),
        ));
        assert_eq!(file_set.selected_files_meta.len(), 1);
        assert_eq!(file_set.selected_files_meta[0].file_name, source.file_name);
        assert_eq!(file_set.selected_files_meta[0].file_size, Some(987));
    }

    #[test]
    fn partial_legacy_selection_is_kept_without_truncating_to_recovered_files() {
        let source = mod_file(10, "legacy.zip", 100);
        let old_raw = serde_json::to_string(&profile(vec![source.clone()], 100)).unwrap();
        let mut file_set = FileSetRecipe {
            selected_file_ids: vec![source.id, 99],
            ..Default::default()
        };

        assert!(!hydrate_tracked_file_sizes(
            &mut file_set,
            Some(&old_raw),
            None,
            None,
        ));
        assert!(file_set.selected_files_meta.is_empty());
        assert_eq!(file_set.selected_file_ids, vec![source.id, 99]);
    }

    #[test]
    fn legacy_id_does_not_fallback_to_a_reuploaded_same_name() {
        let mut replacement = mod_file(20, "legacy.zip", 200);
        replacement.file_size = 222;
        let fresh_profile = profile(vec![replacement], 200);
        let mut file_set = FileSetRecipe {
            selected_file_ids: vec![10],
            selected_file_names: vec!["legacy.zip".to_owned()],
            ..Default::default()
        };

        assert!(!hydrate_tracked_file_sizes(
            &mut file_set,
            None,
            None,
            Some(&fresh_profile),
        ));
        assert!(file_set.selected_files_meta.is_empty());
        assert_eq!(file_set.selected_file_ids, vec![10]);
        assert_eq!(file_set.selected_file_names, vec!["legacy.zip"]);
    }

    #[test]
    fn partial_new_profile_backfill_does_not_truncate_legacy_selection() {
        let source = mod_file(10, "legacy.zip", 100);
        let second = mod_file(11, "second.zip", 110);
        let old_raw = serde_json::to_string(&profile(vec![source.clone()], 100)).unwrap();
        let mut file_set = FileSetRecipe {
            selected_file_ids: vec![source.id, second.id],
            ..Default::default()
        };
        let partial_new_profile = profile(vec![source], 100);

        assert!(!hydrate_tracked_file_sizes(
            &mut file_set,
            Some(&old_raw),
            None,
            Some(&partial_new_profile),
        ));
        assert!(!backfill_selected_files_meta(
            &mut file_set,
            &partial_new_profile,
        ));
        assert!(file_set.selected_files_meta.is_empty());
        assert_eq!(file_set.selected_file_ids, vec![10, 11]);
    }

    #[test]
    fn partial_or_ambiguous_selectors_are_neutral() {
        let source = mod_file(10, "legacy.zip", 100);
        let candidate = mod_file(20, "replacement.zip", 200);
        let remote = profile(vec![source.clone(), candidate], 200);

        let partial = FileSetRecipe {
            selected_file_ids: vec![source.id, 99],
            ..Default::default()
        };
        let partial_evaluation = determine_file_set_update_state(&partial, Some(100), &remote);
        assert_eq!(partial_evaluation, ModUpdateState::CheckSkipped);

        let duplicate_name_profile = profile(vec![
            source.clone(),
            mod_file(11, "duplicate.zip", 110),
            mod_file(12, "duplicate.zip", 120),
        ], 120);
        let duplicate_name = FileSetRecipe {
            selected_file_names: vec!["duplicate.zip".to_owned()],
            ..Default::default()
        };
        assert_eq!(
            determine_file_set_update_state(&duplicate_name, Some(110), &duplicate_name_profile),
            ModUpdateState::CheckSkipped
        );

        let renamed_id = FileSetRecipe {
            selected_file_ids: vec![source.id],
            selected_file_names: vec!["replacement.zip".to_owned()],
            ..Default::default()
        };
        assert_eq!(
            determine_file_set_update_state(&renamed_id, Some(100), &remote),
            ModUpdateState::UpToDate
        );
    }

    #[test]
    fn installed_ids_are_authoritative_over_reused_or_reordered_names() {
        let deleted = mod_file(10, "legacy.zip", 100);
        let reused = mod_file(20, "legacy.zip", 200);
        let duplicate_name = mod_file(30, "legacy.zip", 210);
        let remote = profile(vec![reused, duplicate_name.clone()], 210);

        let deleted_source = FileSetRecipe {
            selected_file_ids: vec![deleted.id],
            selected_file_names: vec![deleted.file_name.clone()],
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&deleted)],
            ..Default::default()
        };
        match resolve_tracked_files(&deleted_source, &all_profile_files(&remote)) {
            ResolvedTrackedFiles::Tracked(files) => assert_eq!(files[0].file_id, deleted.id),
            other => panic!("deleted persisted source was not retained: {other:?}"),
        }
        let signature = current_remote_signature(&deleted_source, &remote)
            .expect("current profile signature remains available");
        assert!(signature.files.is_empty());

        let hydrated_name_only = FileSetRecipe {
            selected_file_names: vec!["legacy.zip".to_owned()],
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&deleted)],
            ..Default::default()
        };
        match resolve_tracked_files(&hydrated_name_only, &all_profile_files(&remote)) {
            ResolvedTrackedFiles::Tracked(files) => assert_eq!(files[0].file_id, deleted.id),
            other => panic!("hydrated stable ID was treated as ambiguous name: {other:?}"),
        }

        let reordered = FileSetRecipe {
            selected_file_ids: vec![30, 10],
            selected_file_names: vec!["legacy.zip".to_owned(), "legacy.zip".to_owned()],
            selected_files_meta: vec![
                tracked_file_meta_from_mod_file(&duplicate_name),
                tracked_file_meta_from_mod_file(&deleted),
            ],
            ..Default::default()
        };
        match resolve_tracked_files(&reordered, &all_profile_files(&remote)) {
            ResolvedTrackedFiles::Tracked(files) => {
                assert_eq!(files.iter().map(|file| file.file_id).collect::<Vec<_>>(), vec![30, 10]);
            }
            other => panic!("reordered installed metadata was rejected: {other:?}"),
        }
    }

    #[test]
    fn canonical_archive_wins_size_and_label_backfill() {
        let mut active = mod_file(10, "legacy.zip", 111);
        active.file_size = 111;
        active.description = Some("stale active".to_owned());
        let mut archived = mod_file(10, "legacy.zip", 222);
        archived.file_size = 222;
        archived.is_archived = true;
        archived.description = Some("archived label".to_owned());
        let mut remote = profile(vec![active], 200);
        remote.archived_files = vec![archived];
        let mut file_set = FileSetRecipe {
            selected_files_meta: vec![TrackedFileMeta {
                file_id: 10,
                file_name: "legacy.zip".to_owned(),
                file_size: None,
                date_added: 100,
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(hydrate_tracked_file_sizes(
            &mut file_set,
            None,
            None,
            Some(&remote),
        ));
        assert_eq!(file_set.selected_files_meta[0].file_size, Some(222));
        assert!(backfill_tracked_file_labels(&mut file_set, &remote));
        assert_eq!(
            file_set.selected_files_meta[0].label.as_deref(),
            Some("archived label")
        );
    }

    #[test]
    fn snapshot_roundtrip_preserves_versions_for_archived_a_recovery() {
        let mut source = versioned_mod_file(10, "ink_spirit_ebbcf.zip", 100, "1.0");
        source.download_url = None;
        source.is_archived = false;
        let candidate = versioned_mod_file(20, "ink_spirit_a1b2c.zip", 200, "2.0");
        let mut live = profile(vec![candidate.clone()], 200);
        live.archived_files = vec![source.clone()];

        let live_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        assert_eq!(live_evaluation.state, ModUpdateState::UpdateAvailable);

        let snapshot = profile_to_snapshot(&live);
        let restored = profile_to_response(Some(&snapshot), live.id);
        assert_eq!(
            restored.archived_files[0].version.as_deref(),
            Some("1.0")
        );
        assert_eq!(restored.files[0].version.as_deref(), Some("2.0"));

        let restored_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &restored,
        )[0]
            .clone();
        assert_eq!(restored_evaluation.state, live_evaluation.state);
        assert_eq!(
            restored_evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            live_evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id)
        );
    }

    #[test]
    fn snapshot_roundtrip_preserves_equal_versions_for_package_veto() {
        let mut source = mod_file(
            10,
            "star_rail_update_mods_for_characterbody_21.zip",
            100,
        );
        source.version = Some("1.0".to_string());
        source.file_size = 7_059_626;
        let mut candidate = mod_file(
            20,
            "source_star_rail_update_mods_for_characterbody_21.zip",
            200,
        );
        candidate.version = Some("1.0".to_string());
        candidate.file_size = 3_785;
        let live = profile(vec![source.clone(), candidate], 200);
        let live_files = all_profile_files(&live);
        assert!(package_scope_veto(
            &tracked_file_meta_from_mod_file(&source),
            &live.files[1],
            &live,
            &live_files,
        ));

        let snapshot = profile_to_snapshot(&live);
        let restored = profile_to_response(Some(&snapshot), live.id);
        assert_eq!(restored.files[0].version.as_deref(), Some("1.0"));
        assert_eq!(restored.files[1].version.as_deref(), Some("1.0"));
        let restored_files = all_profile_files(&restored);
        assert!(package_scope_veto(
            &tracked_file_meta_from_mod_file(&restored.files[0]),
            &restored.files[1],
            &restored,
            &restored_files,
        ));

        let live_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        let restored_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &restored,
        )[0]
            .clone();
        assert_eq!(live_evaluation.state, ModUpdateState::UpToDate);
        assert_eq!(restored_evaluation.state, live_evaluation.state);
        assert!(live_evaluation.signature.is_none());
        assert!(restored_evaluation.signature.is_none());
    }

    #[test]
    fn old_snapshot_json_without_file_versions_does_not_invent_versions() {
        let source = versioned_mod_file(10, "legacy.zip", 100, "1.0");
        let candidate = versioned_mod_file(20, "legacyplus.zip", 200, "2.0");
        let remote = profile(vec![source.clone(), candidate], 200);
        let snapshot = profile_to_snapshot(&remote);
        let mut old_json = serde_json::to_value(&snapshot).unwrap();
        for file in old_json["files"].as_array_mut().unwrap() {
            file.as_object_mut().unwrap().remove("version");
        }

        let old_snapshot: GameBananaSnapshot = serde_json::from_value(old_json).unwrap();
        let restored = profile_to_response(Some(&old_snapshot), remote.id);
        assert!(restored.files.iter().all(|file| file.version.is_none()));
        assert_ne!(
            determine_file_set_update_state(
                &fallback_file_set(&source),
                Some(source.date_added),
                &restored,
            ),
            ModUpdateState::MissingSource
        );
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
    fn profile_without_download_urls_keeps_known_source_up_to_date() {
        // A known installed source remains present even when the catalog has no
        // usable download candidates. The update check must not call that
        // clerical metadata loss MissingSource.
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
            ModUpdateState::UpToDate
        );
    }

    #[test]
    fn blank_urls_and_zero_id_files_are_not_downloadable_candidates() {
        let mut blank_url = mod_file(10, "blank.zip", 100);
        blank_url.download_url = Some("  \t".to_owned());
        let valid_zero_id = mod_file(0, "zero-id.zip", 110);
        let profile = profile(vec![blank_url, valid_zero_id], 110);

        assert!(downloadable_active_files(&profile).is_empty());
        assert!(downloadable_all_files(&profile).is_empty());
    }

    #[test]
    fn legacy_selected_file_ids_resolve_but_native_matching_abstains() {
        let profile = profile(
            vec![
                mod_file(10, "old.zip", 100),
                mod_file(20, "old v2.zip", 200),
            ],
            200,
        );
        let file_set = FileSetRecipe {
            selected_file_ids: vec![10],
            ..Default::default()
        };

        match resolve_tracked_files(&file_set, &all_profile_files(&profile)) {
            ResolvedTrackedFiles::Tracked(files) => {
                assert_eq!(files.len(), 1);
                assert_eq!(files[0].file_id, 10);
            }
            other => panic!("legacy ID selector did not resolve: {other:?}"),
        }
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), file_set)],
            &profile,
        )
        .into_iter()
        .next()
        .expect("single grouped evaluation");
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
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

    fn versioned_mod_file(
        id: u64,
        file_name: &str,
        date_added: i64,
        version: &str,
    ) -> gamebanana::ModFile {
        gamebanana::ModFile {
            version: Some(version.to_string()),
            ..mod_file(id, file_name, date_added)
        }
    }

    fn fallback_file_set(source: &gamebanana::ModFile) -> FileSetRecipe {
        FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(source)],
            ..Default::default()
        }
    }

    fn release_record(
        id: u64,
        date_added: i64,
        version: &str,
        file_ids: Option<Vec<u64>>,
    ) -> gamebanana::UpdateRecord {
        gamebanana::UpdateRecord {
            id,
            name: String::new(),
            date_modified: date_added,
            date_added,
            profile_url: String::new(),
            html_text: None,
            version: Some(version.to_string()),
            status: Some(0),
            file_ids,
            files: None,
            attachments_usable: true,
            is_private: false,
            is_trashed: false,
            is_withheld: false,
        }
    }

    fn attach_release_history(
        profile: &mut gamebanana::ProfileResponse,
        records: Vec<gamebanana::UpdateRecord>,
    ) {
        profile.update_history = Some(gamebanana::UpdateHistory { records });
    }

    fn release_sequence_matches_for_test(
        source: &gamebanana::ModFile,
        candidate: &gamebanana::ModFile,
        remote: &gamebanana::ProfileResponse,
    ) -> bool {
        let all_files = all_profile_files(remote);
        release_sequence_candidate_matches(
            &tracked_file_meta_from_mod_file(source),
            candidate,
            remote,
            &all_files,
        )
    }

    fn renamed_release_fixture() -> (
        gamebanana::ModFile,
        gamebanana::ModFile,
        gamebanana::ProfileResponse,
    ) {
        // This fixture uses an explicit source version so its single-record
        // cases cover the known-numeric path. The real 728107 source has no
        // per-file API version and is covered by the two-record regression
        // fixture instead.
        let source = versioned_mod_file(
            1849618,
            "845ae2cf2f3406ac2dd3371ea60aff22.rar",
            100,
            "1.0",
        );
        let candidate = mod_file(1853590, "si101.rar", 200);
        let mut remote = profile(vec![source.clone(), candidate.clone()], 200);
        attach_release_history(
            &mut remote,
            vec![release_record(465719, 150, "1.01", Some(vec![candidate.id]))],
        );
        (source, candidate, remote)
    }

    #[test]
    fn release_sequence_recovers_hash_to_renamed_file_and_roundtrips_history() {
        let (source, candidate, remote) = renamed_release_fixture();
        let file_set = fallback_file_set(&source);
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), file_set.clone())],
            &remote,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
        let selected = update_candidate_files_from_signature(
            evaluation.signature.as_ref(),
            &remote.files,
        );
        assert_eq!(selected.iter().map(|file| file.id).collect::<Vec<_>>(), vec![candidate.id]);

        let snapshot = profile_to_snapshot(&remote);
        let restored = profile_to_response(Some(&snapshot), remote.id);
        assert_eq!(restored.update_history, remote.update_history);
        let restored_evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), file_set)],
            &restored,
        )[0]
            .clone();
        assert_eq!(restored_evaluation.state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            restored_evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(candidate.id)
        );

        let mut old_cache = snapshot;
        old_cache.update_history = None;
        let old_cached_profile = profile_to_response(Some(&old_cache), remote.id);
        assert_eq!(
            evaluate_file_set_update_group(
                &[(Some(source.date_added), fallback_file_set(&source))],
                &old_cached_profile,
            )[0]
            .state,
            ModUpdateState::UpToDate
        );
    }

    #[test]
    fn release_sequence_recovers_the_next_file_in_a_complete_chain() {
        let source = mod_file(1849618, "845ae2cf2f3406ac2dd3371ea60aff22.rar", 100);
        let first = versioned_mod_file(1853590, "si101.rar", 200, "1.01");
        let latest = versioned_mod_file(1855843, "si102.rar", 300, "1.02");
        let mut remote = profile(vec![source.clone(), first, latest.clone()], 300);
        attach_release_history(
            &mut remote,
            vec![
                release_record(465719, 150, "1.01", Some(vec![1853590])),
                release_record(466339, 250, "1.02", Some(vec![latest.id])),
            ],
        );
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &remote,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(latest.id)
        );
    }

    #[test]
    fn release_sequence_accepts_an_archived_middle_release_and_fourth_equal_version() {
        let root = versioned_mod_file(1, "initial-random.zip", 100, "2.0");
        let first = versioned_mod_file(2, "first-random.zip", 200, "2.0");
        let mut source = versioned_mod_file(3, "middle-random.zip", 300, "2.0");
        source.is_archived = true;
        source.description = Some("Installed middle release".to_owned());
        let candidate = versioned_mod_file(4, "fourth-random.zip", 400, "2.0");
        let mut remote = profile(vec![candidate.clone()], candidate.date_added);
        remote.archived_files = vec![root, first.clone(), source.clone()];
        attach_release_history(
            &mut remote,
            vec![
                release_record(10, 150, "2.0", Some(vec![first.id])),
                release_record(11, 250, "2.0", Some(vec![source.id])),
                release_record(12, 350, "2.0", Some(vec![candidate.id])),
            ],
        );

        assert!(release_sequence_matches_for_test(&source, &candidate, &remote));
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &remote,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(candidate.id)
        );
    }

    #[test]
    fn release_sequence_rejects_unattached_root_after_first_log_record() {
        let root = mod_file(1, "initial-random.zip", 100);
        let candidate = versioned_mod_file(2, "latest-random.zip", 200, "2.0");
        let mut remote = profile(vec![root.clone(), candidate.clone()], candidate.date_added);
        attach_release_history(
            &mut remote,
            vec![release_record(10, 90, "2.0", Some(vec![candidate.id]))],
        );

        assert!(!release_sequence_matches_for_test(&root, &candidate, &remote));
    }

    #[test]
    fn release_sequence_requires_two_records_only_for_versionless_unattached_root() {
        let root = mod_file(10, "initial-random.zip", 100);
        let first = mod_file(20, "first-random.zip", 200);
        let latest = mod_file(30, "latest-random.zip", 300);
        let mut remote = profile(vec![root.clone(), first, latest.clone()], latest.date_added);
        attach_release_history(
            &mut remote,
            vec![
                release_record(100, 150, "1.0", Some(vec![20])),
                release_record(101, 250, "2.0", Some(vec![latest.id])),
            ],
        );

        assert!(release_sequence_matches_for_test(&root, &latest, &remote));

        let source = mod_file(40, "associated-random.zip", 100);
        let candidate = mod_file(50, "associated-latest-random.zip", 200);
        let mut associated = profile(vec![source.clone(), candidate.clone()], candidate.date_added);
        attach_release_history(
            &mut associated,
            vec![
                release_record(102, 150, "1.0", Some(vec![source.id])),
                release_record(103, 250, "2.0", Some(vec![candidate.id])),
            ],
        );

        assert!(release_sequence_matches_for_test(&source, &candidate, &associated));
    }

    #[test]
    fn release_sequence_does_not_fabricate_graph_source_when_catalog_omits_it() {
        let (source, candidate, mut remote) = renamed_release_fixture();
        remote.files.retain(|file| file.id != source.id);
        let all_files = all_profile_files(&remote);
        assert!(!release_sequence_candidate_matches(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &remote,
            &all_files,
        ));
    }

    #[test]
    fn release_sequence_rejects_an_unattached_catalog_file_after_the_chain() {
        let extra = versioned_mod_file(1, "extra-old-catalog-file.zip", 350, "2.0");
        let mut source = versioned_mod_file(2, "middle-random.zip", 200, "2.0");
        source.is_archived = true;
        let candidate = versioned_mod_file(3, "latest-random.zip", 300, "2.0");
        let mut remote = profile(vec![candidate.clone(), extra], candidate.date_added);
        remote.archived_files = vec![source.clone()];
        attach_release_history(
            &mut remote,
            vec![
                release_record(10, 150, "2.0", Some(vec![source.id])),
                release_record(11, 250, "2.0", Some(vec![candidate.id])),
            ],
        );

        assert!(!release_sequence_matches_for_test(&source, &candidate, &remote));
    }

    #[test]
    fn verified_release_chain_outranks_a_softer_filename_match() {
        let source = mod_file(1, "legacy_random_hash.rar", 100);
        let filename_match = mod_file(2, "legacy_random_hash_plus.rar", 200);
        let verified_latest = mod_file(3, "renamed_release_hash.zip", 300);
        let mut remote = profile(
            vec![source.clone(), filename_match, verified_latest.clone()],
            verified_latest.date_added,
        );
        attach_release_history(
            &mut remote,
            vec![
                release_record(10, 150, "1.0", Some(vec![2])),
                release_record(11, 250, "2.0", Some(vec![verified_latest.id])),
            ],
        );

        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &remote,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(verified_latest.id)
        );
    }

    #[test]
    fn release_sequence_accepts_archived_original_and_selects_latest_728709_release() {
        let mut source = versioned_mod_file(
            1852143,
            "neonrookie_-_ink_spirit_free.zip",
            100,
            "1.6",
        );
        source.download_url = None;
        // Some API payloads leave the per-file flag unset; placement in the
        // archived-files array is the authoritative archived signal.
        source.is_archived = false;
        let mut first = versioned_mod_file(
            1853077,
            "neonrookie_ink_spirit_12_freeplus_300likes.zip",
            200,
            "1.6",
        );
        first.download_url = None;
        first.is_archived = true;
        let latest = versioned_mod_file(
            1856058,
            "neonrookie_ink_spirit_14_freeplusplus_600likes_update.zip",
            300,
            "1.6",
        );
        let mut remote = profile(vec![latest.clone()], 300);
        remote.id = 728709;
        remote.archived_files = vec![source.clone(), first];
        attach_release_history(
            &mut remote,
            vec![
                release_record(465610, 150, "1.6", Some(vec![1853077])),
                release_record(466397, 250, "1.6", Some(vec![latest.id])),
            ],
        );

        let raw_profile: gamebanana::ProfileResponse =
            serde_json::from_slice(&serde_json::to_vec(&remote).unwrap()).unwrap();
        let snapshot = profile_to_snapshot(&raw_profile);
        let restored = profile_to_response(Some(&snapshot), remote.id);
        for checked in [&remote, &raw_profile, &restored] {
            let evaluation = evaluate_file_set_update_group(
                &[(Some(source.date_added), fallback_file_set(&source))],
                checked,
            )[0]
                .clone();
            assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
            let selected = update_candidate_files_from_signature(
                evaluation.signature.as_ref(),
                &checked.files,
            );
            assert_eq!(
                selected.iter().map(|file| file.id).collect::<Vec<_>>(),
                vec![latest.id]
            );
        }
    }

    #[test]
    fn release_sequence_accepts_archived_source_with_only_one_active_renamed_candidate() {
        let (mut source, candidate, mut remote) = renamed_release_fixture();
        source.download_url = None;
        source.is_archived = false;
        remote.files = vec![candidate.clone()];
        remote.archived_files = vec![source.clone()];

        assert!(release_sequence_matches_for_test(&source, &candidate, &remote));
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &remote,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(candidate.id)
        );
    }

    #[test]
    fn release_sequence_requires_archived_source_and_active_downloadable_candidate() {
        let (source, candidate, mut remote) = renamed_release_fixture();

        let mut archived_candidate = candidate.clone();
        archived_candidate.is_archived = true;
        remote.files.clear();
        remote.archived_files = vec![source.clone(), archived_candidate];
        assert!(!release_sequence_matches_for_test(&source, &candidate, &remote));

        let mut remote = renamed_release_fixture().2;
        let mut url_less_candidate = candidate.clone();
        url_less_candidate.download_url = None;
        remote.files[1] = url_less_candidate.clone();
        assert!(!release_sequence_matches_for_test(&source, &url_less_candidate, &remote));

        let mut url_less_source = source.clone();
        url_less_source.download_url = None;
        let mut remote = profile(vec![url_less_source.clone(), candidate.clone()], 200);
        attach_release_history(
            &mut remote,
            vec![release_record(465719, 150, "1.01", Some(vec![candidate.id]))],
        );
        assert!(!release_sequence_matches_for_test(
            &url_less_source,
            &candidate,
            &remote,
        ));

        let mut remote = renamed_release_fixture().2;
        remote.files.retain(|file| file.id != source.id);
        assert!(!release_sequence_matches_for_test(&source, &candidate, &remote));
    }

    #[test]
    fn release_sequence_keeps_archived_source_ties_ambiguous() {
        let (mut source, candidate, mut remote) = renamed_release_fixture();
        source.is_archived = true;
        remote.files = vec![candidate];
        remote.archived_files = vec![source.clone()];
        let recipe = fallback_file_set(&source);
        let evaluations = evaluate_file_set_update_group(
            &[(Some(source.date_added), recipe.clone()), (Some(source.date_added), recipe)],
            &remote,
        );
        assert!(evaluations.iter().all(|evaluation| {
            evaluation.signature.is_none()
                && matches!(evaluation.state, ModUpdateState::UpToDate)
        }));
    }

    #[test]
    fn release_sequence_abstains_when_history_evidence_is_not_serial() {
        let cases = [
            "missing_history",
            "failed_attachment",
            "multi_file_release",
            "mismatched_embedded_file",
            "conflicting_versions",
            "same_version_api_conflict",
            "decreasing_version",
            "reversed_dates",
            "sideload_candidate",
            "unexplained_catalog_peer",
        ];
        for case in cases {
            let source = mod_file(1, "old-hash.rar", 100);
            let first = versioned_mod_file(2, "si101.rar", 200, "1.01");
            let latest = versioned_mod_file(3, "si102.rar", 300, "1.02");
            let mut remote = profile(vec![source.clone(), first.clone(), latest.clone()], 300);
            match case {
                "missing_history" => {}
                "failed_attachment" => {
                    let mut first_record = release_record(10, 150, "1.01", Some(vec![2]));
                    first_record.attachments_usable = false;
                    attach_release_history(&mut remote, vec![
                        first_record,
                        release_record(11, 250, "1.02", Some(vec![3])),
                    ]);
                }
                "multi_file_release" => attach_release_history(
                    &mut remote,
                    vec![release_record(10, 150, "1.01", Some(vec![2, 3]))],
                ),
                "mismatched_embedded_file" => {
                    let mut record = release_record(10, 150, "1.01", Some(vec![2]));
                    record.files = Some(vec![latest.clone()]);
                    attach_release_history(&mut remote, vec![record]);
                }
                "conflicting_versions" => attach_release_history(
                    &mut remote,
                    vec![
                        release_record(10, 150, "1.01", Some(vec![2])),
                        release_record(11, 250, "1.02", Some(vec![2])),
                    ],
                ),
                "same_version_api_conflict" => attach_release_history(
                    &mut remote,
                    vec![
                        release_record(10, 150, "1.01", Some(vec![2])),
                        release_record(11, 250, "1.01", Some(vec![3])),
                    ],
                ),
                "decreasing_version" => attach_release_history(
                    &mut remote,
                    vec![
                        release_record(10, 150, "1.02", Some(vec![2])),
                        release_record(11, 250, "1.01", Some(vec![3])),
                    ],
                ),
                "reversed_dates" => attach_release_history(
                    &mut remote,
                    vec![
                        release_record(10, 250, "1.01", Some(vec![2])),
                        release_record(11, 150, "1.02", Some(vec![3])),
                    ],
                ),
                "sideload_candidate" => {
                    remote.files[2].download_url = None;
                    attach_release_history(
                        &mut remote,
                        vec![
                            release_record(10, 150, "1.01", Some(vec![2])),
                            release_record(11, 250, "1.02", Some(vec![3])),
                        ],
                    );
                }
                "unexplained_catalog_peer" => {
                    remote.files.push(mod_file(4, "unlisted.zip", 400));
                    attach_release_history(
                        &mut remote,
                        vec![
                            release_record(10, 150, "1.01", Some(vec![2])),
                            release_record(11, 250, "1.02", Some(vec![3])),
                        ],
                    );
                }
                _ => unreachable!(),
            }
            assert_eq!(
                evaluate_file_set_update_group(
                    &[(Some(source.date_added), fallback_file_set(&source))],
                    &remote,
                )[0]
                .state,
                ModUpdateState::UpToDate,
                "case {case}"
            );
        }
    }

    #[test]
    fn release_sequence_keeps_duplicate_installed_groups_ambiguous() {
        let (source, _, remote) = renamed_release_fixture();
        let recipe = fallback_file_set(&source);
        let evaluations = evaluate_file_set_update_group(
            &[(Some(100), recipe.clone()), (Some(100), recipe)],
            &remote,
        );
        assert!(evaluations.iter().all(|evaluation|
            evaluation.state == ModUpdateState::UpToDate && evaluation.signature.is_none()));
    }

    #[test]
    fn release_sequence_does_not_assign_one_target_to_two_installed_sources() {
        let source = mod_file(1, "source-one-random.zip", 100);
        let middle = mod_file(2, "source-two-random.zip", 200);
        let latest = mod_file(3, "latest-random.zip", 300);
        let mut remote = profile(vec![source.clone(), middle.clone(), latest], 300);
        attach_release_history(
            &mut remote,
            vec![
                release_record(10, 150, "1.0", Some(vec![middle.id])),
                release_record(11, 250, "2.0", Some(vec![3])),
            ],
        );
        let evaluations = evaluate_file_set_update_group(
            &[
                (Some(source.date_added), fallback_file_set(&source)),
                (Some(middle.date_added), fallback_file_set(&middle)),
            ],
            &remote,
        );
        assert!(evaluations.iter().all(|evaluation| {
            evaluation.state == ModUpdateState::UpToDate && evaluation.signature.is_none()
        }));
    }

    #[test]
    fn release_sequence_does_not_resolve_a_tie_by_excluding_an_occupied_group() {
        let source = mod_file(1, "old-hash.rar", 100);
        let mut first = mod_file(2, "ordinary.zip", 200);
        first.description = Some("Main component".to_owned());
        let latest = mod_file(3, "unrelated-name.rar", 300);
        let mut remote = profile(vec![source.clone(), first, latest], 300);
        attach_release_history(&mut remote, vec![
            release_record(10, 150, "1.01", Some(vec![2])),
            release_record(11, 250, "1.02", Some(vec![3])),
        ]);
        let mut group_with_current_match = fallback_file_set(&source);
        group_with_current_match.selected_files_meta.push(TrackedFileMeta {
            file_id: 99,
            file_name: "original-component.zip".to_owned(),
            date_added: 100,
            label: Some("Main component".to_owned()),
            ..Default::default()
        });
        let evaluations = evaluate_file_set_update_group(
            &[(Some(100), group_with_current_match), (Some(100), fallback_file_set(&source))],
            &remote,
        );
        assert_eq!(evaluations[0].state, ModUpdateState::UpdateAvailable);
        assert_eq!(evaluations[0].signature.as_ref().unwrap().files.iter()
            .map(|file| file.file_id).collect::<Vec<_>>(), vec![2]);
        assert!(evaluations[1].signature.is_none());
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
    }

    #[test]
    fn release_sequence_preserves_existing_package_veto() {
        let mut source = mod_file(1, "base.zip", 100);
        source.file_size = 1;
        let mut candidate = mod_file(2, "base_extra.zip", 200);
        candidate.file_size = 100;
        let mut remote = profile(vec![source.clone(), candidate.clone()], 200);
        attach_release_history(
            &mut remote,
            vec![release_record(10, 150, "1.0", Some(vec![candidate.id]))],
        );
        assert!(package_scope_veto(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &remote,
            &all_profile_files(&remote),
        ));
        assert_eq!(
            evaluate_file_set_update_group(
                &[(Some(source.date_added), fallback_file_set(&source))],
                &remote,
            )[0]
            .state,
            ModUpdateState::UpToDate
        );
    }

    #[test]
    #[ignore = "requires HESTIA_UPDATE_MATCH_REPLAY to point at an offline fixture"]
    fn replay_update_match_fixture_when_requested() {
        #[derive(Debug, Deserialize)]
        struct ReplaySourceSnapshot {
            #[serde(rename = "_idRow")]
            file_id: u64,
            #[serde(rename = "_sFile")]
            file_name: String,
            #[serde(default, rename = "_nFilesize")]
            file_size: Option<u64>,
            #[serde(rename = "_tsDateAdded")]
            date_added: i64,
            #[serde(default, rename = "_sVersion")]
            version: Option<String>,
            #[serde(default, rename = "_bIsArchived")]
            archived: bool,
            #[serde(default, rename = "_sDescription")]
            description: Option<String>,
        }

        impl ReplaySourceSnapshot {
            fn tracked(self) -> TrackedFileMeta {
                TrackedFileMeta {
                    file_id: self.file_id,
                    file_name: self.file_name,
                    file_size: self.file_size,
                    date_added: self.date_added,
                    version: self
                        .version
                        .as_deref()
                        .map(str::trim)
                        .filter(|version| !version.is_empty())
                        .map(str::to_owned),
                    archived: self.archived,
                    label: self
                        .description
                        .as_deref()
                        .map(str::trim)
                        .filter(|description| !description.is_empty())
                        .map(str::to_owned),
                }
            }
        }

        #[derive(Debug, Deserialize)]
        struct ReplayRow {
            source_id: u64,
            candidate_id: u64,
            expected_selected: bool,
            #[serde(default)]
            source_snapshot: Option<ReplaySourceSnapshot>,
        }
        #[derive(Debug, Deserialize)]
        struct ReplayProfile {
            profile: gamebanana::ProfileResponse,
            rows: Vec<ReplayRow>,
        }
        #[derive(Debug, Deserialize)]
        struct ReplayFixture {
            profiles: Vec<ReplayProfile>,
        }

        let path = std::env::var_os("HESTIA_UPDATE_MATCH_REPLAY")
            .expect("HESTIA_UPDATE_MATCH_REPLAY must point at the offline replay fixture");
        let path = std::path::PathBuf::from(path);
        let fixture: ReplayFixture = serde_json::from_slice(
            &std::fs::read(&path).unwrap_or_else(|error| {
                panic!("failed to read HESTIA_UPDATE_MATCH_REPLAY {}: {error}", path.display())
            }),
        )
        .unwrap_or_else(|error| {
            panic!(
                "failed to parse HESTIA_UPDATE_MATCH_REPLAY {}: {error}",
                path.display()
            )
        });

        let mut checked = 0usize;
        let mut mismatches = Vec::new();
        let mut results = Vec::new();
        for replay in fixture.profiles {
            let all_files = all_profile_files(&replay.profile);
            for row in replay.rows {
                let tracked = row
                    .source_snapshot
                    .map(ReplaySourceSnapshot::tracked)
                    .or_else(|| {
                        all_files
                            .iter()
                            .copied()
                            .find(|file| file.id == row.source_id)
                            .map(tracked_file_meta_from_mod_file)
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "replay source {} is absent from profile {} and has no source_snapshot",
                            row.source_id, replay.profile.id
                        )
                    });
                let replay_candidate = all_files
                    .iter()
                    .copied()
                    .find(|file| file.id == row.candidate_id);
                let verified_release_chain = replay_candidate.is_some_and(|candidate| {
                        release_sequence_candidate_matches(
                            &tracked,
                            candidate,
                            &replay.profile,
                            &all_files,
                        )
                    });
                let verified_typed_release = replay_candidate.is_some_and(|candidate| {
                    typed_release_candidate_matches(
                        &tracked,
                        candidate,
                        &replay.profile,
                        &all_files,
                        &downloadable_active_files(&replay.profile),
                    )
                });
                let shape_files = downloadable_active_files(&replay.profile);
                let shape_index = CatalogShapeIndex::from_active_files(&shape_files);
                let package_scope_veto = replay_candidate.is_some_and(|candidate| {
                    package_scope_veto_with_shapes(
                        &tracked,
                        candidate,
                        &replay.profile,
                        &all_files,
                        Some(&shape_index),
                    )
                });
                let catalog_contrast_veto = replay_candidate.is_some_and(|candidate| {
                    catalog_contrast_veto(
                        &tracked,
                        candidate,
                        &replay.profile,
                        &all_files,
                        &shape_index,
                    )
                });
                let evaluation = evaluate_file_set_update_group(
                    &[(
                        Some(tracked.date_added),
                        FileSetRecipe {
                            selected_files_meta: vec![tracked.clone()],
                            ..Default::default()
                        },
                    )],
                    &replay.profile,
                )[0]
                    .clone();
                let selected = evaluation.signature.is_some_and(|signature| {
                    signature
                        .files
                        .iter()
                        .any(|file| file.file_id == row.candidate_id)
                });
                let state = format!("{:?}", evaluation.state);
                results.push(serde_json::json!({
                    "mod_id": replay.profile.id,
                    "source_id": row.source_id,
                    "candidate_id": row.candidate_id,
                    "state": state,
                    "selected": selected,
                    "verified_release_chain": verified_release_chain,
                    "verified_typed_release": verified_typed_release,
                    "package_scope_veto": package_scope_veto,
                    "catalog_contrast_veto": catalog_contrast_veto,
                    "existing_pair_veto": package_scope_veto || catalog_contrast_veto,
                    "expected_selected": row.expected_selected,
                }));
                if selected != row.expected_selected {
                    mismatches.push(format!(
                        "profile {} source {} candidate {}: expected {}, actual {}",
                        replay.profile.id, row.source_id, row.candidate_id,
                        row.expected_selected, selected,
                    ));
                }
                checked += 1;
            }
        }
        assert!(checked > 0, "offline replay fixture contained no rows");
        if let Some(output) = std::env::var_os("HESTIA_UPDATE_MATCH_OUTPUT") {
            std::fs::write(output, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
        }
        assert!(mismatches.is_empty(), "{} of {checked} replay mismatches:\n{}",
            mismatches.len(), mismatches.iter().take(30).cloned().collect::<Vec<_>>().join("\n"));
        eprintln!("{checked} replay comparisons passed");
    }

    #[test]
    fn weak_neon_rookie_coverage_is_observed_but_native_matching_abstains() {
        let source = versioned_mod_file(
            7287091,
            "neonrookie_-_ink_spirit_free.zip",
            100,
            "1.6",
        );
        let candidate = versioned_mod_file(
            7287092,
            "neonrookie_ink_spirit_12_freeplus_300likes.zip",
            200,
            "1.6",
        );
        let live = profile(vec![source.clone(), candidate.clone()], 200);
        assert!(raw_source_name_coverage(&source.file_name, &candidate.file_name)
            >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD);
        let evaluations = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        );

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert!(evaluations[0].signature.is_none());
    }

    #[test]
    fn release_sequence_selects_latest_equal_version_neon_rookie_release() {
        let source = versioned_mod_file(
            1852143,
            "neonrookie_-_ink_spirit_free.zip",
            1_792_233_412,
            "1.6",
        );
        let mut first = versioned_mod_file(
            1853077,
            "neonrookie_ink_spirit_12_freeplus_300likes.zip",
            1_792_271_518,
            "1.6",
        );
        first.description = Some("300 likes".to_owned());
        let mut latest = versioned_mod_file(
            1856058,
            "neonrookie_ink_spirit_14_freeplusplus_600likes_update.zip",
            1_792_449_429,
            "1.6",
        );
        latest.description = Some("600 likes".to_owned());
        let mut remote = profile(vec![source.clone(), first, latest.clone()], latest.date_added);
        remote.id = 728709;
        attach_release_history(
            &mut remote,
            vec![
                release_record(465610, 1_792_278_443, "1.6", Some(vec![1853077])),
                release_record(466397, 1_792_449_912, "1.6", Some(vec![latest.id])),
            ],
        );

        let raw_profile: gamebanana::ProfileResponse =
            serde_json::from_slice(&serde_json::to_vec(&remote).unwrap()).unwrap();
        let snapshot = profile_to_snapshot(&raw_profile);
        let restored = profile_to_response(Some(&snapshot), remote.id);
        for checked in [&remote, &raw_profile, &restored] {
            let evaluation = evaluate_file_set_update_group(
                &[(Some(source.date_added), fallback_file_set(&source))],
                checked,
            )[0]
            .clone();
            assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
            let selected = update_candidate_files_from_signature(
                evaluation.signature.as_ref(),
                &checked.files,
            );
            assert_eq!(
                selected.iter().map(|file| file.id).collect::<Vec<_>>(),
                vec![latest.id]
            );
        }
    }

    #[test]
    fn release_sequence_boundaries_hold_for_flat_equal_version_chains() {
        let source = versioned_mod_file(10, "source-random-a.zip", 100, "2.0");
        let first = versioned_mod_file(20, "release-random-b.zip", 200, "2.0");
        let latest = versioned_mod_file(30, "release-random-c.zip", 300, "2.0");
        let mut remote = profile(vec![source.clone(), first, latest.clone()], 300);
        attach_release_history(
            &mut remote,
            vec![
                release_record(100, 150, "2.0", Some(vec![20])),
                release_record(101, 250, "2.0", Some(vec![latest.id])),
            ],
        );
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &remote,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluation
                .signature
                .as_ref()
                .map(|signature| signature.files.iter().map(|file| file.file_id).collect::<Vec<_>>()),
            Some(vec![latest.id])
        );

        let source = versioned_mod_file(40, "single-source-random.zip", 100, "2.0");
        let candidate = versioned_mod_file(41, "single-release-random.zip", 200, "2.0");
        let mut single_release = profile(vec![source.clone(), candidate.clone()], 200);
        attach_release_history(
            &mut single_release,
            vec![release_record(102, 150, "2.0", Some(vec![candidate.id]))],
        );
        assert!(!release_sequence_matches_for_test(
            &source,
            &candidate,
            &single_release,
        ));

        let source = versioned_mod_file(50, "decrease-source-random.zip", 100, "1.0");
        let first = versioned_mod_file(51, "decrease-first-random.zip", 200, "1.2");
        let latest = versioned_mod_file(52, "decrease-latest-random.zip", 300, "1.1");
        let mut decreasing = profile(vec![source.clone(), first, latest.clone()], 300);
        attach_release_history(
            &mut decreasing,
            vec![
                release_record(103, 150, "1.2", Some(vec![51])),
                release_record(104, 250, "1.1", Some(vec![latest.id])),
            ],
        );
        assert!(!release_sequence_matches_for_test(
            &source,
            &latest,
            &decreasing,
        ));

        let source = versioned_mod_file(60, "equal-file-source-random.zip", 100, "2.0");
        let first = versioned_mod_file(61, "equal-file-first-random.zip", 200, "2.0");
        let latest = versioned_mod_file(62, "equal-file-latest-random.zip", 200, "2.0");
        let mut equal_file_date = profile(vec![source.clone(), first, latest.clone()], 200);
        attach_release_history(
            &mut equal_file_date,
            vec![
                release_record(105, 150, "2.0", Some(vec![61])),
                release_record(106, 250, "2.0", Some(vec![latest.id])),
            ],
        );
        assert!(!release_sequence_matches_for_test(
            &source,
            &latest,
            &equal_file_date,
        ));

        let source = versioned_mod_file(70, "equal-log-source-random.zip", 100, "2.0");
        let first = versioned_mod_file(71, "equal-log-first-random.zip", 200, "2.0");
        let latest = versioned_mod_file(72, "equal-log-latest-random.zip", 300, "2.0");
        let mut equal_log_date = profile(vec![source.clone(), first, latest.clone()], 300);
        attach_release_history(
            &mut equal_log_date,
            vec![
                release_record(107, 150, "2.0", Some(vec![71])),
                release_record(108, 150, "2.0", Some(vec![latest.id])),
            ],
        );
        assert!(!release_sequence_matches_for_test(
            &source,
            &latest,
            &equal_log_date,
        ));

        let mut labeled_root = mod_file(80, "labeled-root-random.zip", 100);
        labeled_root.description = Some("Main File".to_owned());
        let labeled_candidate = mod_file(81, "labeled-release-random.zip", 200);
        let mut labeled_root_profile = profile(
            vec![labeled_root.clone(), labeled_candidate.clone()],
            labeled_candidate.date_added,
        );
        attach_release_history(
            &mut labeled_root_profile,
            vec![release_record(109, 150, "1.0", Some(vec![labeled_candidate.id]))],
        );
        assert!(!release_sequence_matches_for_test(
            &labeled_root,
            &labeled_candidate,
            &labeled_root_profile,
        ));
    }

    #[test]
    fn refinement_a_recovers_archived_all_letter_collision_suffix_with_peer_context() {
        let mut source = versioned_mod_file(10, "hideuid_ebbcf.zip", 100, "1.0");
        source.download_url = None;
        // Some API responses omit `_bIsArchived` on the archived-file array;
        // membership in `archived_files` is the authoritative signal here.
        source.is_archived = false;
        let candidate = versioned_mod_file(20, "hideuid_a1b2c.zip", 200, "1.1");
        let profile = gamebanana::ProfileResponse {
            id: 1,
            files: vec![candidate.clone()],
            archived_files: vec![source.clone()],
            date_modified: 200,
            date_updated: Some(200),
            ..Default::default()
        };
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &profile,
        )[0]
            .clone();

        let files = all_profile_files(&profile);
        let active = downloadable_active_files(&profile);
        assert!(all_hex_context(&source, &candidate, &files));
        assert!(refinement_a_candidate_matches(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &profile,
            &files,
            &active,
        ));
        assert!(fallback_candidate_matches(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &profile,
            &files,
            &active,
        ));
        assert_eq!(evaluation.state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluation
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(candidate.id)
        );

        let numeric_source = mod_file(30, "blue-purple_cyrene_88434.rar", 100);
        let numeric_candidate = mod_file(31, "blue-purple_cyrene_c51df.zip", 200);
        assert!(all_hex_context(
            &numeric_source,
            &numeric_candidate,
            &[&numeric_source, &numeric_candidate],
        ));
        let unrelated = mod_file(32, "other_12345.zip", 150);
        assert!(!all_hex_context(
            &numeric_source,
            &mod_file(33, "blue-purple_cyrene_67890.zip", 200),
            &[&numeric_source, &unrelated],
        ));
    }

    #[test]
    fn wuwa_simple_lumi_numeric_lineage_is_observed_but_native_abstains() {
        let mut source = mod_file(1_340_688, "simple_lumi_sliders12.7z", 1_734_014_918);
        source.file_size = 23_828_781;
        source.description = Some("latex with old texture files".to_string());
        let mut candidate = mod_file(1_433_795, "simple_lumi_sliders152.zip", 1_746_439_465);
        candidate.file_size = 42_613_472;
        candidate.description = Some("Texture fix".to_string());
        let live = profile(vec![source.clone(), candidate.clone()], candidate.date_added);
        assert!(raw_source_name_coverage(&source.file_name, &candidate.file_name)
            >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD);
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();

        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn wuwa_phoebe_numeric_lineage_is_observed_but_native_abstains() {
        let mut source = mod_file(1_403_685, "feebeepanties.zip", 1_742_421_721);
        source.file_size = 77_022_290;
        source.description = Some(
            "Fixed and merged, use left and right arrow keys to toggle between versions"
                .to_string(),
        );
        let mut candidate = mod_file(1_420_478, "feebeepanties_22.zip", 1_744_573_838);
        candidate.file_size = 68_894_655;
        candidate.description = Some("Fixed version for update 2.2".to_string());
        let live = profile(vec![source.clone(), candidate.clone()], candidate.date_added);
        assert!(raw_source_name_coverage(&source.file_name, &candidate.file_name)
            >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD);
        let evaluation = evaluate_file_set_update_group(
            &[(Some(source.date_added), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();

        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn package_scope_veto_rejects_tiny_to_full_package_with_one_inserted_word() {
        let mut source = mod_file(
            10,
            "star_rail_update_mods_for_characterbody_21.zip",
            100,
        );
        source.file_size = 7_059_626;
        let mut candidate = mod_file(
            20,
            "source_star_rail_update_mods_for_characterbody_21.zip",
            200,
        );
        candidate.file_size = 3_785;
        let profile = profile(vec![source.clone(), candidate.clone()], 200);
        let tracked = tracked_file_meta_from_mod_file(&source);
        let files = all_profile_files(&profile);

        assert!(package_scope_veto(&tracked, &candidate, &profile, &files));
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &profile,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn package_scope_veto_does_not_reassign_to_runner_up_group() {
        let mut source = mod_file(
            10,
            "star_rail_update_mods_for_characterbody_21.zip",
            100,
        );
        source.file_size = 7_059_626;
        let mut runner_up_source = mod_file(
            11,
            "star_rail_update_mods_for_weapon_21.zip",
            100,
        );
        runner_up_source.file_size = 7_059_626;
        let mut candidate = mod_file(
            20,
            "source_star_rail_update_mods_for_characterbody_21.zip",
            200,
        );
        candidate.file_size = 3_785;
        let profile = profile(
            vec![source.clone(), runner_up_source.clone(), candidate.clone()],
            200,
        );
        let files = all_profile_files(&profile);

        assert!(package_scope_veto(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &profile,
            &files,
        ));
        assert!(!package_scope_veto(
            &tracked_file_meta_from_mod_file(&runner_up_source),
            &candidate,
            &profile,
            &files,
        ));

        let evaluations = evaluate_file_set_update_group(
            &[
                (Some(100), fallback_file_set(&source)),
                (Some(100), fallback_file_set(&runner_up_source)),
            ],
            &profile,
        );
        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert!(evaluations.iter().all(|evaluation| evaluation.signature.is_none()));
    }

    #[test]
    fn package_scope_veto_allows_declared_filename_version_advances() {
        let mut api_source = mod_file(10, "mod.zip", 100);
        api_source.version = Some("1.0".to_string());
        api_source.file_size = 4_000;
        let mut api_candidate = mod_file(20, "mod_full.zip", 200);
        api_candidate.version = Some("2.0".to_string());
        api_candidate.file_size = 500_000;
        let api_profile = profile(vec![api_source.clone(), api_candidate.clone()], 200);
        let api_files = all_profile_files(&api_profile);
        assert!(!package_scope_veto(
            &tracked_file_meta_from_mod_file(&api_source),
            &api_candidate,
            &api_profile,
            &api_files,
        ));

        let mut blank_source = api_source.clone();
        blank_source.version = None;
        let mut blank_candidate = api_candidate.clone();
        blank_candidate.version = None;
        let blank_profile = profile(vec![blank_source.clone(), blank_candidate.clone()], 200);
        let blank_files = all_profile_files(&blank_profile);
        assert!(package_scope_veto(
            &tracked_file_meta_from_mod_file(&blank_source),
            &blank_candidate,
            &blank_profile,
            &blank_files,
        ));

        let mut invalid_source = blank_source.clone();
        invalid_source.version = Some("beta".to_string());
        let mut invalid_candidate = blank_candidate.clone();
        invalid_candidate.version = Some("beta".to_string());
        let invalid_profile = profile(vec![invalid_source.clone(), invalid_candidate.clone()], 200);
        let invalid_files = all_profile_files(&invalid_profile);
        assert!(!package_scope_veto(
            &tracked_file_meta_from_mod_file(&invalid_source),
            &invalid_candidate,
            &invalid_profile,
            &invalid_files,
        ));

        let mut asymmetric_candidate = blank_candidate.clone();
        asymmetric_candidate.version = Some("1.0".to_string());
        let asymmetric_profile = profile(vec![blank_source.clone(), asymmetric_candidate.clone()], 200);
        let asymmetric_files = all_profile_files(&asymmetric_profile);
        assert!(!package_scope_veto(
            &tracked_file_meta_from_mod_file(&blank_source),
            &asymmetric_candidate,
            &asymmetric_profile,
            &asymmetric_files,
        ));

        let filename_source = mod_file(30, "mod_v1.zip", 100);
        let mut filename_candidate = mod_file(40, "mod_full_v2.zip", 200);
        filename_candidate.file_size = 500_000;
        let filename_profile = profile(
            vec![filename_source.clone(), filename_candidate.clone()],
            200,
        );
        let filename_files = all_profile_files(&filename_profile);
        assert!(!package_scope_veto(
            &tracked_file_meta_from_mod_file(&filename_source),
            &filename_candidate,
            &filename_profile,
            &filename_files,
        ));
    }

    #[test]
    fn refinement_typed_filename_versions_are_embedded_for_package_scope() {
        assert_eq!(typed_filename_versions("stellev2openshirt.zip"), vec![vec![2]]);
        assert_eq!(typed_filename_versions("foo_v1_part2.zip"), vec![vec![1]]);
    }

    #[test]
    fn catalog_contrast_blocks_cross_language_releases_but_native_abstains() {
        let source = labeled_mod_file(10, "plugin_v235_en.zip", 100, "Language");
        let zh235 = labeled_mod_file(11, "plugin_v235_zh.zip", 110, "Language");
        let en300 = labeled_mod_file(12, "plugin_v300_en.zip", 200, "Language");
        let zh300 = labeled_mod_file(13, "plugin_v300_zh.zip", 210, "Language");
        let live = profile(vec![source.clone(), zh235.clone(), en300.clone(), zh300], 210);
        let files = all_profile_files(&live);
        let shapes = CatalogShapeIndex::from_active_files(&downloadable_active_files(&live));

        assert!(catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &zh235,
            &live,
            &files,
            &shapes,
        ));
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &en300,
            &live,
            &files,
            &shapes,
        ));

        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn catalog_contrast_blocks_min_max_across_release_prefixes_then_native_abstains() {
        let source = labeled_mod_file(
            20,
            "nom_liino_secret_servicevf141_min_.rar",
            100,
            "Merge script",
        );
        let old_max = labeled_mod_file(
            21,
            "nom_liino_secret_servicevf141_max_.rar",
            110,
            "Merge script",
        );
        let new_min = labeled_mod_file(
            22,
            "nom_liino_secret_servicev141_min_.rar",
            200,
            "Merge script",
        );
        let new_max = labeled_mod_file(
            23,
            "nom_liino_secret_servicev141_max_.rar",
            210,
            "Merge script",
        );
        let live = profile(
            vec![source.clone(), old_max.clone(), new_min.clone(), new_max.clone()],
            210,
        );
        let files = all_profile_files(&live);
        let shapes = CatalogShapeIndex::from_active_files(&downloadable_active_files(&live));

        assert!(catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &new_max,
            &live,
            &files,
            &shapes,
        ));
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &new_min,
            &live,
            &files,
            &shapes,
        ));

        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn catalog_contrast_blocks_generic_label_weapon_choices() {
        let source = labeled_mod_file(30, "desert_wedge.7z", 100, "Weapon Skin");
        let opus = labeled_mod_file(31, "desert_opus.7z", 110, "Weapon Skin");
        let navigator = labeled_mod_file(32, "desert_navigator.7z", 120, "Weapon Skin");
        let object = labeled_mod_file(33, "desert_obj.7z", 130, "Weapon Skin");
        let live = profile(vec![source.clone(), opus, navigator, object], 130);
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn catalog_contrast_nonveto_does_not_override_native_abstention() {
        let source = labeled_mod_file(40, "desert_wedge.7z", 100, "Weapon Skin");
        let rename = labeled_mod_file(41, "desert_opus.7z", 110, "Weapon Skin");
        let rename_live = profile(vec![source.clone(), rename], 110);
        let rename_files = all_profile_files(&rename_live);
        let rename_shapes = CatalogShapeIndex::from_active_files(&downloadable_active_files(&rename_live));
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &rename_live.files[1],
            &rename_live,
            &rename_files,
            &rename_shapes,
        ));
        let rename_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &rename_live,
        )[0]
            .clone();
        assert_eq!(rename_evaluation.state, ModUpdateState::UpToDate);
        assert!(rename_evaluation.signature.is_none());

        let prefix_source = labeled_mod_file(42, "widget_core.zip", 100, "Main");
        let prefix_candidate = labeled_mod_file(43, "widget_coreplus.zip", 110, "Main");
        let prefix_live = profile(vec![prefix_source.clone(), prefix_candidate], 110);
        let prefix_files = all_profile_files(&prefix_live);
        let prefix_shapes = CatalogShapeIndex::from_active_files(&downloadable_active_files(&prefix_live));
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&prefix_source),
            &prefix_live.files[1],
            &prefix_live,
            &prefix_files,
            &prefix_shapes,
        ));
        let prefix_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&prefix_source))],
            &prefix_live,
        )[0]
            .clone();
        assert_eq!(prefix_evaluation.state, ModUpdateState::UpToDate);
        assert!(prefix_evaluation.signature.is_none());
    }

    #[test]
    fn numeric_release_lineage_is_nonveto_but_native_abstains() {
        let source = mod_file(50, "widget_v1.zip", 100);
        let next = mod_file(51, "widget_v2.zip", 110);
        let newest = mod_file(52, "widget_v3.zip", 120);
        let live = profile(vec![source.clone(), next.clone(), newest], 120);
        let files = all_profile_files(&live);
        let shapes = CatalogShapeIndex::from_active_files(&downloadable_active_files(&live));
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &next,
            &live,
            &files,
            &shapes,
        ));
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn catalog_contrast_ignores_archived_and_url_less_evidence() {
        let source = labeled_mod_file(60, "desert_wedge.7z", 100, "Weapon Skin");
        let candidate = labeled_mod_file(61, "desert_opus.7z", 110, "Weapon Skin");
        let mut archived_peer = labeled_mod_file(62, "desert_navigator.7z", 120, "Weapon Skin");
        archived_peer.is_archived = true;
        let mut url_less_peer = labeled_mod_file(63, "desert_obj.7z", 130, "Weapon Skin");
        url_less_peer.download_url = Some("   ".to_string());
        let mut archived_shadow = archived_peer.clone();
        archived_shadow.is_archived = false;
        let mut live = profile(
            vec![source.clone(), candidate.clone(), url_less_peer, archived_shadow],
            130,
        );
        live.archived_files = vec![archived_peer];
        let files = all_profile_files(&live);
        let shape_files: Vec<_> = downloadable_active_files(&live)
            .into_iter()
            .filter(|file| profile_file_is_active_downloadable(&live, file))
            .collect();
        let shapes = CatalogShapeIndex::from_active_files(&shape_files);
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &live,
            &files,
            &shapes,
        ));
    }

    #[test]
    fn catalog_contrast_keeps_equally_best_same_choice_source() {
        let weapon = labeled_mod_file(70, "desert_wedge.7z", 100, "Weapon Skin");
        let old_navigator = labeled_mod_file(71, "desert_navigator.7z", 101, "Weapon Skin");
        let candidate = labeled_mod_file(72, "desert_navigator.7z", 110, "Weapon Skin");
        let third_weapon = labeled_mod_file(73, "desert_opus.7z", 120, "Weapon Skin");
        let live = profile(
            vec![weapon.clone(), old_navigator.clone(), candidate.clone(), third_weapon],
            120,
        );
        let source_set = FileSetRecipe {
            selected_files_meta: vec![
                tracked_file_meta_from_mod_file(&weapon),
                tracked_file_meta_from_mod_file(&old_navigator),
            ],
            ..Default::default()
        };
        let evaluations = evaluate_file_set_update_group(&[(Some(100), source_set)], &live);
        assert_eq!(evaluations[0].state, ModUpdateState::UpdateAvailable);
        assert_eq!(
            evaluations[0]
                .signature
                .as_ref()
                .and_then(|signature| signature.files.first())
                .map(|file| file.file_id),
            Some(candidate.id)
        );
    }

    #[test]
    fn catalog_contrast_does_not_reassign_a_vetoed_winner_to_a_runner_up_group() {
        let winner_source = labeled_mod_file(74, "desert_wedge.7z", 100, "Weapon Skin");
        let third_weapon = labeled_mod_file(75, "desert_navigator.7z", 150, "Weapon Skin");
        let runner_up_source = labeled_mod_file(76, "desert_opus_12.7z", 100, "Other");
        let candidate = labeled_mod_file(77, "desert_opus.7z", 200, "Weapon Skin");
        let live = profile(
            vec![
                winner_source.clone(),
                third_weapon,
                runner_up_source.clone(),
                candidate,
            ],
            200,
        );
        let evaluations = evaluate_file_set_update_group(
            &[
                (Some(100), fallback_file_set(&winner_source)),
                (Some(100), fallback_file_set(&runner_up_source)),
            ],
            &live,
        );
        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert!(evaluations.iter().all(|evaluation| evaluation.signature.is_none()));
    }

    #[test]
    fn catalog_contrast_nonveto_single_alpha_context_still_abstains_natively() {
        let source = labeled_mod_file(78, "wedge.zip", 100, "Weapon Skin");
        let candidate = labeled_mod_file(79, "opus.zip", 110, "Weapon Skin");
        let third = labeled_mod_file(80, "navigator.zip", 120, "Weapon Skin");
        let live = profile(vec![source.clone(), candidate, third], 120);
        let files = all_profile_files(&live);
        let shapes = CatalogShapeIndex::from_active_files(&downloadable_active_files(&live));
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &live.files[1],
            &live,
            &files,
            &shapes,
        ));
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn opaque_upload_suffixes_have_no_catalog_veto_but_native_abstains() {
        for names in [
            [
                "widget_abcde.zip",
                "widget_faded.zip",
                "widget_beade.zip",
            ],
            [
                "widget_-ChRnAM.zip",
                "widget_-QaZsWx.zip",
                "widget_-YuIoPl.zip",
            ],
        ] {
            let files: Vec<_> = names
                .into_iter()
                .enumerate()
                .map(|(index, file_name)| {
                    let mut file = versioned_mod_file(
                        81 + index as u64,
                        file_name,
                        100 + index as i64 * 100,
                        "1.0",
                    );
                    file.description = Some("Main".to_string());
                    file
                })
                .collect();
            let source = files[0].clone();
            let live = profile(files, 300);
            let active = downloadable_active_files(&live);
            let shapes = CatalogShapeIndex::from_active_files(&active);
            assert!(shapes.files.is_empty());
            let source_files = all_profile_files(&live);
            assert!(!catalog_contrast_veto(
                &tracked_file_meta_from_mod_file(&source),
                &live.files[1],
                &live,
                &source_files,
                &shapes,
            ));
            let evaluation = evaluate_file_set_update_group(
                &[(Some(100), fallback_file_set(&source))],
                &live,
            )[0]
                .clone();
            assert_eq!(evaluation.state, ModUpdateState::UpToDate);
            assert!(evaluation.signature.is_none());
        }
    }

    #[test]
    fn named_api_version_increase_is_nonveto_but_native_abstains() {
        let mut source = versioned_mod_file(84, "widget_alpha.zip", 100, "1.0");
        let mut candidate = versioned_mod_file(85, "widget_beta.zip", 200, "2.0");
        let mut third = versioned_mod_file(86, "widget_stable.zip", 300, "3.0");
        source.description = Some("Main".to_string());
        candidate.description = Some("Main".to_string());
        third.description = Some("Main".to_string());
        let live = profile(vec![source.clone(), candidate.clone(), third], 300);
        let files = all_profile_files(&live);
        let active = downloadable_active_files(&live);
        let shapes = CatalogShapeIndex::from_active_files(&active);
        assert!(!catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &live,
            &files,
            &shapes,
        ));
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();
        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    fn catalog_veto_for_test(
        source: gamebanana::ModFile,
        candidate: gamebanana::ModFile,
        peers: Vec<gamebanana::ModFile>,
    ) -> bool {
        let mut files = vec![source.clone(), candidate.clone()];
        files.extend(peers);
        let live = profile(files, candidate.date_added);
        let all_files = all_profile_files(&live);
        let shapes = CatalogShapeIndex::from_active_files(&downloadable_active_files(&live));
        catalog_contrast_veto(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &live,
            &all_files,
            &shapes,
        )
    }

    #[test]
    fn catalog_metadata_witness_blocks_repeated_labels_across_prefix_parallel_releases() {
        let source = labeled_mod_file(90, "exce_ellen_joe_regular.zip", 100, "Main");
        let candidate = labeled_mod_file(91, "exce_ellen_joe_original.zip", 110, "Main");
        let source_parallel = labeled_mod_file(
            92,
            "exce_ellen_joe_regularplus_1.zip",
            120,
            "Main",
        );
        let candidate_parallel = labeled_mod_file(
            93,
            "exce_ellen_joe_originalplus_1.zip",
            130,
            "Main",
        );

        assert!(catalog_veto_for_test(
            source,
            candidate,
            vec![source_parallel, candidate_parallel],
        ));
    }

    #[test]
    fn catalog_metadata_witness_needs_two_shared_keys_and_nonempty_labels() {
        let source = labeled_mod_file(94, "exce_ellen_joe_regular.zip", 100, "Main");
        let candidate = labeled_mod_file(95, "exce_ellen_joe_original.zip", 110, "Main");
        assert!(!catalog_veto_for_test(source, candidate, Vec::new()));

        let source = mod_file(96, "exce_ellen_joe_regular.zip", 100);
        let candidate = mod_file(97, "exce_ellen_joe_original.zip", 110);
        let source_parallel = mod_file(98, "exce_ellen_joe_regularplus_1.zip", 120);
        let candidate_parallel = mod_file(99, "exce_ellen_joe_originalplus_1.zip", 130);
        assert!(!catalog_veto_for_test(
            source,
            candidate,
            vec![source_parallel, candidate_parallel],
        ));
    }

    #[test]
    fn catalog_metadata_witness_rejects_ambiguous_prefix_peers() {
        let source = labeled_mod_file(100, "exce_ellen_joe_regular.zip", 100, "Main");
        let candidate = labeled_mod_file(101, "exce_ellen_joe_rework.zip", 110, "Main");
        let ambiguous = labeled_mod_file(102, "exce_ellen_joe_re_1.zip", 120, "Main");

        assert!(!catalog_veto_for_test(source, candidate, vec![ambiguous]));
    }

    #[test]
    fn catalog_metadata_witness_ignores_archived_prefix_evidence() {
        let source = labeled_mod_file(103, "exce_ellen_joe_regular.zip", 100, "Main");
        let candidate = labeled_mod_file(104, "exce_ellen_joe_original.zip", 110, "Main");
        let mut source_parallel = labeled_mod_file(
            105,
            "exce_ellen_joe_regularplus_1.zip",
            120,
            "Main",
        );
        source_parallel.is_archived = true;
        let mut candidate_parallel = labeled_mod_file(
            106,
            "exce_ellen_joe_originalplus_1.zip",
            130,
            "Main",
        );
        candidate_parallel.is_archived = true;

        assert!(!catalog_veto_for_test(
            source,
            candidate,
            vec![source_parallel, candidate_parallel],
        ));
    }

    #[test]
    fn catalog_metadata_witness_keeps_direct_prefix_upgrades_allowed() {
        let source = labeled_mod_file(107, "exce_ellen_joe_regular.zip", 100, "Main");
        let candidate = labeled_mod_file(108, "exce_ellen_joe_regularplus.zip", 110, "Main");
        let source_parallel = labeled_mod_file(
            109,
            "exce_ellen_joe_regularplusplus_1.zip",
            120,
            "Main",
        );
        let candidate_parallel = labeled_mod_file(
            110,
            "exce_ellen_joe_regularplusplus_1.zip",
            130,
            "Main",
        );

        assert!(!catalog_veto_for_test(
            source,
            candidate,
            vec![source_parallel, candidate_parallel],
        ));
    }

    #[test]
    fn catalog_metadata_witness_normalizes_two_exact_labeled_frames() {
        let source = labeled_mod_file(111, "exce_ellen_joe_regular.zip", 100, "Old");
        let candidate = labeled_mod_file(112, "exce_ellen_joe_original.zip", 110, " old ");
        let source_current = labeled_mod_file(113, "exce_ellen_joe_regular.zip", 120, "CURRENT");
        let candidate_current = labeled_mod_file(
            114,
            "exce_ellen_joe_original.zip",
            130,
            " current ",
        );

        assert!(catalog_veto_for_test(
            source,
            candidate,
            vec![source_current, candidate_current],
        ));
    }

    #[test]
    fn catalog_metadata_witness_deduplicates_repeated_peer_ids() {
        let source = labeled_mod_file(115, "exce_ellen_joe_regular.zip", 100, "SourceOnly");
        let candidate = labeled_mod_file(116, "exce_ellen_joe_original.zip", 110, "CandidateOnly");
        let source_peer_x = labeled_mod_file(
            117,
            "exce_ellen_joe_regularplus_1.zip",
            120,
            "LabelX",
        );
        let source_peer_y = labeled_mod_file(
            117,
            "exce_ellen_joe_regularplus_1.zip",
            120,
            "LabelY",
        );
        let candidate_peer_y = labeled_mod_file(
            118,
            "exce_ellen_joe_originalplus_1.zip",
            130,
            "LabelY",
        );
        let candidate_peer_x = labeled_mod_file(
            118,
            "exce_ellen_joe_originalplus_1.zip",
            130,
            "LabelX",
        );

        assert!(!catalog_veto_for_test(
            source,
            candidate,
            vec![source_peer_x, source_peer_y, candidate_peer_y, candidate_peer_x],
        ));
    }

    #[test]
    fn catalog_metadata_witness_ignores_url_less_peer_labels() {
        let source = labeled_mod_file(119, "exce_ellen_joe_regular.zip", 100, "Old");
        let candidate = labeled_mod_file(120, "exce_ellen_joe_original.zip", 110, "Old");
        let mut source_peer = labeled_mod_file(
            121,
            "exce_ellen_joe_regularplus_1.zip",
            120,
            "Current",
        );
        source_peer.download_url = Some("   ".to_string());
        let mut candidate_peer = labeled_mod_file(
            122,
            "exce_ellen_joe_originalplus_1.zip",
            130,
            "Current",
        );
        candidate_peer.download_url = Some("   ".to_string());

        assert!(!catalog_veto_for_test(
            source,
            candidate,
            vec![source_peer, candidate_peer],
        ));
    }

    #[test]
    fn weak_coverage_hydrates_metadata_but_native_matching_abstains() {
        let source = versioned_mod_file(10, "legacy_base.zip", 100, "v1.6");
        let candidate = versioned_mod_file(20, "legacy_baseplus.zip", 200, "1.6");
        let mut tracked = tracked_file_meta_from_mod_file(&source);
        tracked.version = None;
        tracked.label = None;
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked.clone()],
            ..Default::default()
        };
        let evaluations = evaluate_file_set_update_group(
            &[(Some(100), file_set.clone())],
            &profile(vec![source.clone(), candidate.clone()], 200),
        );
        let live = profile(vec![source.clone(), candidate.clone()], 200);
        let all_files = all_profile_files(&live);
        let active_files = downloadable_active_files(&live);
        assert!(fallback_candidate_matches(
            &tracked,
            &candidate,
            &live,
            &all_files,
            &active_files,
        ));
        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert!(evaluations[0].signature.is_none());

        let mut whitespace_version = tracked.clone();
        whitespace_version.version = Some("   ".to_string());
        let whitespace_evaluation = evaluate_file_set_update_group(
            &[(
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![whitespace_version],
                    ..Default::default()
                },
            )],
            &profile(vec![source.clone(), candidate.clone()], 200),
        )[0]
            .clone();
        assert_eq!(whitespace_evaluation.state, ModUpdateState::UpToDate);
        assert!(whitespace_evaluation.signature.is_none());

        let mut invalid_version = tracked.clone();
        invalid_version.version = Some("unknown".to_string());
        let invalid_evaluation = evaluate_file_set_update_group(
            &[(
                Some(100),
                FileSetRecipe {
                    selected_files_meta: vec![invalid_version],
                    ..Default::default()
                },
            )],
            &profile(vec![source.clone(), candidate.clone()], 200),
        )[0]
            .clone();
        assert_eq!(invalid_evaluation.state, ModUpdateState::UpToDate);

        let mut labeled_source = source;
        labeled_source.description = Some("Main File".to_string());
        let labeled_evaluations = evaluate_file_set_update_group(
            &[(Some(100), file_set)],
            &profile(vec![labeled_source, candidate], 200),
        );
        assert_eq!(labeled_evaluations[0].state, ModUpdateState::UpToDate);
        assert!(labeled_evaluations[0].signature.is_none());
    }

    #[test]
    fn current_assignment_wins_before_fallback_but_native_abstains() {
        let fallback_source = versioned_mod_file(
            10,
            "neonrookie_-_ink_spirit_free.zip",
            100,
            "1.6",
        );
        let mut candidate = versioned_mod_file(
            20,
            "neonrookie_ink_spirit_12_freeplus_300likes.zip",
            200,
            "1.6",
        );
        candidate.description = Some("300 likes".to_string());
        let current_source = labeled_mod_file(11, "unrelated_current.zip", 100, "300 likes");
        let profile = profile(
            vec![fallback_source.clone(), current_source.clone(), candidate.clone()],
            200,
        );
        assert!(fallback_candidate_matches(
            &tracked_file_meta_from_mod_file(&fallback_source),
            &candidate,
            &profile,
            &all_profile_files(&profile),
            &downloadable_active_files(&profile),
        ));
        let evaluations = evaluate_file_set_update_group(
            &[
                (Some(100), fallback_file_set(&fallback_source)),
                (Some(100), fallback_file_set(&current_source)),
            ],
            &profile,
        );

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert!(evaluations[1].signature.is_none());
    }

    #[test]
    fn current_assignment_ambiguity_is_not_rescued_by_fallback() {
        let fallback_source = versioned_mod_file(
            10,
            "neonrookie_-_ink_spirit_free.zip",
            100,
            "1.6",
        );
        let mut candidate = versioned_mod_file(
            20,
            "neonrookie_ink_spirit_12_freeplus_300likes.zip",
            200,
            "1.6",
        );
        candidate.description = Some("300 likes".to_string());
        let current_a = labeled_mod_file(11, "unrelated_current_a.zip", 100, "300 likes");
        let current_b = labeled_mod_file(12, "unrelated_current_b.zip", 100, "300 likes");
        let profile = profile(
            vec![
                fallback_source.clone(),
                current_a.clone(),
                current_b.clone(),
                candidate,
            ],
            200,
        );
        let evaluations = evaluate_file_set_update_group(
            &[
                (Some(100), fallback_file_set(&fallback_source)),
                (Some(100), fallback_file_set(&current_a)),
                (Some(100), fallback_file_set(&current_b)),
            ],
            &profile,
        );

        assert!(evaluations
            .iter()
            .all(|evaluation| evaluation.state == ModUpdateState::UpToDate));
        assert!(evaluations
            .iter()
            .all(|evaluation| evaluation.signature.is_none()));
    }

    #[test]
    fn equal_fallback_matches_are_ambiguous_and_rejected() {
        let source_a = versioned_mod_file(10, "neonrookie_free.zip", 100, "1.0");
        let source_b = versioned_mod_file(11, "neonrookie_free.zip", 100, "1.0");
        let candidate = versioned_mod_file(20, "neonrookie_freeplus.zip", 200, "1.0");
        let evaluations = evaluate_file_set_update_group(
            &[
                (Some(100), fallback_file_set(&source_a)),
                (Some(100), fallback_file_set(&source_b)),
            ],
            &profile(vec![source_a, source_b, candidate], 200),
        );

        assert!(evaluations
            .iter()
            .all(|evaluation| evaluation.state == ModUpdateState::UpToDate));
        assert!(evaluations
            .iter()
            .all(|evaluation| evaluation.signature.is_none()));
    }

    #[test]
    fn fallback_rejects_multiple_newer_files_and_non_active_candidates() {
        let source = versioned_mod_file(10, "variant_base.zip", 100, "1.0");
        let candidate = versioned_mod_file(20, "variant_base_plus.zip", 200, "1.0");
        let unrelated = versioned_mod_file(21, "other_file.zip", 201, "1.0");
        let mut archived_candidate = candidate.clone();
        archived_candidate.is_archived = true;
        let mut no_url_candidate = candidate.clone();
        no_url_candidate.download_url = Some("   ".to_string());

        for files in [
            vec![source.clone(), candidate.clone(), unrelated],
            vec![source.clone(), archived_candidate],
            vec![source, no_url_candidate],
        ] {
            let evaluation = evaluate_file_set_update_group(
                &[(Some(100), fallback_file_set(&files[0]))],
                &profile(files, 200),
            )[0]
                .clone();
            assert_eq!(evaluation.state, ModUpdateState::UpToDate);
            assert!(evaluation.signature.is_none());
        }
    }

    #[test]
    fn fallback_rejects_equal_date_and_invalid_or_decreasing_versions() {
        let equal_date_source = versioned_mod_file(1, "variant_base.zip", 100, "1.0");
        let equal_date_candidate = versioned_mod_file(2, "variant_base_new.zip", 100, "1.0");
        let equal_date_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&equal_date_source))],
            &profile(vec![equal_date_source, equal_date_candidate], 100),
        )[0]
            .clone();
        assert_eq!(equal_date_evaluation.state, ModUpdateState::UpToDate);

        let cases = [
            (None, Some("1.0")),
            (Some("not-a-version"), Some("1.0")),
            (Some("2.0"), Some("1.0")),
            (Some("1.0"), None),
            (Some("1"), Some("1.0")),
        ];
        for (source_version, candidate_version) in cases {
            let mut source = mod_file(10, "foo_part1.zip", 100);
            source.version = source_version.map(str::to_string);
            let mut candidate = mod_file(20, "foo_part2.zip", 200);
            candidate.version = candidate_version.map(str::to_string);
            let evaluation = evaluate_file_set_update_group(
                &[(Some(100), fallback_file_set(&source))],
                &profile(vec![source, candidate], 200),
            )[0]
                .clone();
            assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        }
    }

    #[test]
    fn fallback_does_not_accept_internal_variant_insertions_or_numeric_names() {
        let source = versioned_mod_file(10, "character_sfw_v1.zip", 100, "1.0");
        let variant = versioned_mod_file(
            20,
            "character_special_edition_nsfw_v2.zip",
            200,
            "2.0",
        );
        let variant_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &profile(vec![source.clone(), variant], 200),
        )[0]
            .clone();
        assert_eq!(variant_evaluation.state, ModUpdateState::UpToDate);

        let numeric_source = versioned_mod_file(30, "123.zip", 100, "1.0");
        let numeric_candidate = versioned_mod_file(31, "123_plus.zip", 200, "2.0");
        let numeric_evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&numeric_source))],
            &profile(vec![numeric_source, numeric_candidate], 200),
        )[0]
            .clone();
        assert_eq!(numeric_evaluation.state, ModUpdateState::UpToDate);
    }

    #[test]
    fn fallback_filename_normalization_handles_unicode_and_mixed_hex_suffixes() {
        assert!(canonical_numeric_version(Some("V1_2-0")) == Some(vec![1, 2]));
        assert!(canonical_numeric_version(Some("1.2.18446744073709551616")).is_none());
        assert_eq!(normalized_alphabetic_stem("Straße_v1_9f8e7.zip"), "straßev");
        assert_eq!(normalized_alphabetic_stem("猫_v2_ab12c.zip"), "猫v");
        assert_eq!(normalized_alphabetic_stem("猫_v2_12345.zip"), "猫v");
        assert_eq!(normalized_alphabetic_stem("猫猫.zip"), "猫猫");
        assert_eq!(normalized_alphabetic_stem("İstanbul_2.zip"), "istanbul");
        assert!(raw_source_name_coverage("猫_v1.zip", "猫_v1_plus.zip") >= 0.72);
        assert!(normalized_alphabetic_stem("Straße_v2_ab12c.zip")
            .contains(&normalized_alphabetic_stem("Straße_v1_9f8e7.zip")));
    }

    #[test]
    fn equal_version_numeric_component_changes_are_not_fallback_matches() {
        let source = versioned_mod_file(10, "foo_part1.zip", 100, "1.0");
        let candidate = versioned_mod_file(20, "foo_part2.zip", 200, "1.0");
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &profile(vec![source, candidate], 200),
        )[0]
            .clone();

        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    #[test]
    fn equal_version_digit_guard_uses_alphabetic_anchor_and_allows_extra_runs() {
        assert!(!equal_version_digit_runs_are_preserved("foo_part1.zip", "foo_part2.zip"));
        assert!(!equal_version_digit_runs_are_preserved(
            "foo_part1.zip",
            "foo_part2_v1.zip"
        ));
        assert!(equal_version_digit_runs_are_preserved(
            "foo_part1.zip",
            "foo_part1_expanded12.zip"
        ));
        assert!(equal_version_digit_runs_are_preserved(
            "widget_v1_0.zip",
            "widget_v1_0_plus.zip"
        ));
        assert!(!equal_version_digit_runs_are_preserved(
            "foo_part1.zip",
            "foo_part_v1.zip"
        ));
        assert!(equal_version_digit_runs_are_preserved(
            "foo_part1_ab12c.zip",
            "foo_part1_plus.zip"
        ));
    }

    #[test]
    fn token_identity_allows_release_suffixes_but_rejects_extra_words() {
        assert!(token_identity_condition(
            "neonrookie_-_ink_spirit_free.zip",
            "neonrookie_ink_spirit_12_freeplus_300likes.zip"
        ));
        assert!(token_identity_condition("base.zip", "base_2.zip"));
        assert!(!token_identity_condition(
            "eous_drip.zip",
            "eous_drip_supremever.zip"
        ));
        assert!(!token_identity_condition("outfit.zip", "outfit_scarf.zip"));
        assert!(!token_identity_condition("猫猫.zip", "猫猫_plus.zip"));
    }

    #[test]
    fn increased_api_version_matches_typed_helper_but_native_abstains() {
        let source = versioned_mod_file(10, "foo_part1.zip", 100, "1.0");
        let candidate = versioned_mod_file(20, "foo_part2.zip", 200, "2.0");
        let live = profile(vec![source.clone(), candidate.clone()], 200);
        assert!(canonical_numeric_version(source.version.as_deref())
            < canonical_numeric_version(candidate.version.as_deref()));
        let evaluation = evaluate_file_set_update_group(
            &[(Some(100), fallback_file_set(&source))],
            &live,
        )[0]
            .clone();

        assert_eq!(evaluation.state, ModUpdateState::UpToDate);
        assert!(evaluation.signature.is_none());
    }

    fn typed_candidate_matches_for_test(
        source: gamebanana::ModFile,
        candidate: gamebanana::ModFile,
        peers: Vec<gamebanana::ModFile>,
    ) -> bool {
        let mut files = vec![source.clone(), candidate.clone()];
        files.extend(peers);
        let remote = profile(files, candidate.date_added);
        let all_files = all_profile_files(&remote);
        let active_files = downloadable_active_files(&remote);
        typed_release_candidate_matches(
            &tracked_file_meta_from_mod_file(&source),
            &candidate,
            &remote,
            &all_files,
            &active_files,
        )
    }

    #[test]
    fn typed_release_recovers_packed_api_filename_versions() {
        let source = versioned_mod_file(
            10,
            "zhuangfangyi_radialgui_og_evilfall_anim_free_v112_c4b51.zip",
            100,
            "Ver1.1.2",
        );
        let candidate = versioned_mod_file(
            20,
            "zhuangfangyi_radialgui_og_evilfall_anim_free_v14.zip",
            200,
            "Ver1.4",
        );

        assert!(typed_candidate_matches_for_test(source, candidate, Vec::new()));
    }

    #[test]
    fn typed_release_strips_stacked_upload_suffixes_but_keeps_full_edition_separate() {
        let source = versioned_mod_file(
            10,
            "liino_moonlit_violet_base_ver_30_-Ff2mQd.zip",
            100,
            "3.0",
        );
        let candidate = versioned_mod_file(
            20,
            "liino_moonlit_violet_base_ver_31_64f72_-jIwNqi.zip",
            200,
            "3.1",
        );
        let full = versioned_mod_file(
            30,
            "liino_moonlit_violet_full_ver_46_-Ab12Cd.zip",
            201,
            "4.6",
        );

        assert!(typed_candidate_matches_for_test(source, candidate, vec![full]));
    }

    #[test]
    fn typed_release_rejects_api_filename_mismatches_and_part_number_changes() {
        let source = versioned_mod_file(10, "widget_v1.zip", 100, "1");
        let mismatched = versioned_mod_file(20, "widget_v2.zip", 200, "1.1");
        assert!(!typed_candidate_matches_for_test(
            source.clone(),
            mismatched,
            Vec::new(),
        ));

        let source = versioned_mod_file(11, "widget_part1_v1.zip", 100, "1");
        let changed_part = versioned_mod_file(21, "widget_part2_v2.zip", 200, "2");
        assert!(!typed_candidate_matches_for_test(
            source,
            changed_part,
            Vec::new(),
        ));
    }

    #[test]
    fn typed_release_keeps_language_and_min_max_choices_separate() {
        let source = versioned_mod_file(10, "widget_english_v1.zip", 100, "1");
        let translated = versioned_mod_file(20, "widget_中文_v2.zip", 200, "2");
        assert!(!typed_candidate_matches_for_test(
            source,
            translated,
            Vec::new(),
        ));

        let source = versioned_mod_file(11, "widget_min_v1.zip", 100, "1");
        let maximum = versioned_mod_file(21, "widget_max_v2.zip", 200, "2");
        assert!(!typed_candidate_matches_for_test(
            source,
            maximum,
            Vec::new(),
        ));
    }

    #[test]
    fn typed_release_counts_newer_same_identity_peers_even_with_invalid_api() {
        let source = versioned_mod_file(10, "widget_v1.zip", 100, "1");
        let candidate = versioned_mod_file(20, "widget_v2.zip", 200, "2");
        let invalid_peer = versioned_mod_file(30, "widget_v3.zip", 201, "not-a-version");

        assert!(!typed_candidate_matches_for_test(
            source,
            candidate,
            vec![invalid_peer],
        ));
    }

    #[test]
    fn typed_release_requires_a_blank_source_label_and_one_marker() {
        let source = labeled_mod_file(10, "widget_v1.zip", 100, "Main");
        let candidate = versioned_mod_file(20, "widget_v2.zip", 200, "2");
        assert!(!typed_candidate_matches_for_test(
            source,
            candidate,
            Vec::new(),
        ));

        let source = versioned_mod_file(11, "widget.zip", 100, "1");
        let candidate = versioned_mod_file(21, "widget_v2.zip", 200, "2");
        assert!(!typed_candidate_matches_for_test(
            source,
            candidate,
            Vec::new(),
        ));

        let source = versioned_mod_file(12, "widget_v1_ver2.zip", 100, "1");
        let candidate = versioned_mod_file(22, "widget_v3.zip", 200, "3");
        assert!(!typed_candidate_matches_for_test(
            source,
            candidate,
            Vec::new(),
        ));
    }

    #[test]
    fn typed_release_identity_preserves_unicode_runs_and_numeric_atoms() {
        let source = versioned_mod_file(10, "角色版_v1.zip", 100, "1");
        let edition = versioned_mod_file(20, "角色标准_v2.zip", 200, "2");
        assert!(!typed_candidate_matches_for_test(
            source,
            edition,
            Vec::new(),
        ));

        let source = versioned_mod_file(11, "widget_１_v1.zip", 100, "1");
        let numeric = versioned_mod_file(21, "widget_２_v2.zip", 200, "2");
        assert!(!typed_candidate_matches_for_test(
            source,
            numeric,
            Vec::new(),
        ));
    }

    #[test]
    fn typed_release_accepts_archived_url_less_source_metadata() {
        let mut source = versioned_mod_file(10, "widget_v1.zip", 100, "1");
        source.download_url = None;
        source.is_archived = false;
        let candidate = versioned_mod_file(20, "widget_v2.zip", 200, "2");
        let remote = gamebanana::ProfileResponse {
            files: vec![candidate.clone()],
            archived_files: vec![source.clone()],
            date_modified: 200,
            date_updated: Some(200),
            ..Default::default()
        };
        let all_files = all_profile_files(&remote);
        let active_files = downloadable_active_files(&remote);
        let tracked = tracked_file_meta_from_mod_file(&source);

        assert!(typed_release_candidate_matches(
            &tracked,
            &candidate,
            &remote,
            &all_files,
            &active_files,
        ));
    }

    #[test]
    fn typed_release_marker_boundaries_are_unicode_alphanumeric_aware() {
        assert!(typed_release_marker("中文v1.zip").is_none());
        assert!(typed_release_marker("中文_v1.zip").is_some());
        assert!(typed_release_marker("modéver1.zip").is_none());
        assert!(typed_release_api_version(Some("-1")).is_none());
        assert!(typed_release_api_version(Some("_1")).is_none());
        assert!(typed_release_api_version(Some(".1")).is_none());
        assert_eq!(typed_release_api_version(Some("v.1")), Some(vec![1]));
    }

    #[test]
    fn typed_release_fallback_keeps_tied_groups_ambiguous() {
        let source_a = versioned_mod_file(10, "widget_v1.zip", 100, "1");
        let source_b = versioned_mod_file(11, "widget_v1.zip", 100, "1");
        let candidate = versioned_mod_file(20, "widget_v2.zip", 200, "2");
        let remote = profile(
            vec![source_a.clone(), source_b.clone(), candidate],
            200,
        );
        let evaluations = evaluate_file_set_update_group(
            &[
                (Some(100), fallback_file_set(&source_a)),
                (Some(100), fallback_file_set(&source_b)),
            ],
            &remote,
        );

        assert!(evaluations
            .iter()
            .all(|evaluation| evaluation.state == ModUpdateState::UpToDate));
        assert!(evaluations
            .iter()
            .all(|evaluation| evaluation.signature.is_none()));
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
    fn label_lineage_maps_hash_reupload_but_native_abstains_without_source() {
        let old_main = labeled_mod_file(10, "v23_9f8e7d.zip", 100, "Main File");
        let new_main = labeled_mod_file(30, "v24_a1b2c3.zip", 300, "main file");
        let new_experimental = labeled_mod_file(40, "v42_d4e5f6.zip", 300, "Experimental");
        let profile = profile(vec![new_main, new_experimental], 300);
        let file_set = FileSetRecipe {
            selected_files_meta: vec![tracked_file_meta_from_mod_file(&old_main)],
            ..Default::default()
        };

        assert!(file_label_lineage_matches(
            &tracked_file_meta_from_mod_file(&old_main),
            &profile.files[0],
        ));
        assert_eq!(
            determine_file_set_update_state(&file_set, Some(100), &profile),
            ModUpdateState::MissingSource
        );
        assert!(compute_update_signature(&file_set, &profile).is_none());
    }

    #[test]
    fn label_lineage_distinguishes_variants_but_native_abstains_without_sources() {
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

        assert!(file_label_lineage_matches(
            &tracked_file_meta_from_mod_file(&old_main),
            &profile.files[0],
        ));
        assert!(file_label_lineage_matches(
            &tracked_file_meta_from_mod_file(&old_experimental),
            &profile.files[1],
        ));
        assert_eq!(evaluations[0].state, ModUpdateState::MissingSource);
        assert_eq!(evaluations[1].state, ModUpdateState::MissingSource);
        assert!(evaluations.iter().all(|evaluation| evaluation.signature.is_none()));
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
    fn sibling_file_lineage_is_observed_but_native_abstains_for_hair() {
        let body = mod_file(1357843, "covencarlottabody.zip", 100);
        let hair = mod_file(1357842, "covencarlottahair.zip", 100);
        let crystal = mod_file(1357844, "covencarlottacrystalhair.zip", 100);
        let hair_update = mod_file(2000001, "covencarlottahair v2.zip", 200);
        let profile = profile(
            vec![
                body.clone(),
                hair.clone(),
                crystal.clone(),
                hair_update.clone(),
            ],
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

        let common_prefix = lineage_common_prefix(&[
            tracked_file_meta_from_mod_file(&hair),
            tracked_file_meta_from_mod_file(&body),
            tracked_file_meta_from_mod_file(&crystal),
        ]);
        assert!(file_lineage_similarity(
            &hair.file_name,
            &hair_update.file_name,
            &common_prefix,
        ) >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD);
        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert!(evaluations[1].signature.is_none());
        assert_eq!(evaluations[2].state, ModUpdateState::UpToDate);
    }

    #[test]
    fn sibling_file_lineage_new_prefix_is_observed_but_native_abstains() {
        let body = mod_file(1357843, "covencarlottabody.zip", 100);
        let hair = mod_file(1357842, "covencarlottahair.zip", 100);
        let crystal = mod_file(1357844, "covencarlottacrystalhair.zip", 100);
        let hair_update = mod_file(2000001, "covencarlottanewhair.zip", 200);
        let profile = profile(
            vec![
                body.clone(),
                hair.clone(),
                crystal.clone(),
                hair_update.clone(),
            ],
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

        let common_prefix = lineage_common_prefix(&[
            tracked_file_meta_from_mod_file(&hair),
            tracked_file_meta_from_mod_file(&body),
            tracked_file_meta_from_mod_file(&crystal),
        ]);
        assert!(file_lineage_similarity(
            &hair.file_name,
            &hair_update.file_name,
            &common_prefix,
        ) >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD);
        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert!(evaluations[1].signature.is_none());
        assert_eq!(evaluations[2].state, ModUpdateState::UpToDate);
    }

    #[test]
    fn sibling_file_lineage_is_observed_but_native_abstains_for_crystal() {
        let body = mod_file(1357843, "covencarlottabody.zip", 100);
        let hair = mod_file(1357842, "covencarlottahair.zip", 100);
        let crystal = mod_file(1357844, "covencarlottacrystalhair.zip", 100);
        let crystal_update = mod_file(2000001, "covencarlottacrystalhair FIX.zip", 200);
        let profile = profile(
            vec![
                body.clone(),
                hair.clone(),
                crystal.clone(),
                crystal_update.clone(),
            ],
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

        let common_prefix = lineage_common_prefix(&[
            tracked_file_meta_from_mod_file(&hair),
            tracked_file_meta_from_mod_file(&body),
            tracked_file_meta_from_mod_file(&crystal),
        ]);
        assert!(file_lineage_similarity(
            &crystal.file_name,
            &crystal_update.file_name,
            &common_prefix,
        ) >= FILE_LINEAGE_AUTO_MATCH_THRESHOLD);
        let evaluations = evaluate_file_set_update_group(&items, &profile);

        assert_eq!(evaluations[0].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[1].state, ModUpdateState::UpToDate);
        assert_eq!(evaluations[2].state, ModUpdateState::UpToDate);
        assert!(evaluations[2].signature.is_none());
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
        let signature = current_update_signature_for_state(
            &FileSetRecipe::default(),
            &profile,
            ModUpdateState::UpdateAvailable,
        )
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
        let current = versioned_mod_file(10, "current_v1.zip", 100, "1");
        let update = versioned_mod_file(20, "current_v2.zip", 200, "2");
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

        let state = apply_ignored_update_override(
            &mut source,
            ModUpdateState::UpdateAvailable,
            Some(&update_profile),
        );

        let signature = source.ignored_update_signature.as_ref().unwrap();
        assert_eq!(state, ModUpdateState::IgnoringUpdateOnce);
        assert!(!signature.prearmed_next_update);
        assert_eq!(signature.files.len(), 1);
        assert_eq!(signature.files[0].file_id, 20);
    }

    #[test]
    fn prearmed_ignore_once_converts_for_modified_local_update() {
        let current = versioned_mod_file(10, "current_v1.zip", 100, "1");
        let update = versioned_mod_file(20, "current_v2.zip", 200, "2");
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

        let state = apply_ignored_update_override(
            &mut source,
            ModUpdateState::ModifiedLocally,
            Some(&update_profile),
        );

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

        let state = apply_ignored_update_override(
            &mut source,
            ModUpdateState::UpdateAvailable,
            Some(&update_profile),
        );

        assert_eq!(state, ModUpdateState::UpdateAvailable);
        assert!(source.ignored_update_signature.is_none());
    }

    #[test]
    fn unresolved_local_state_preserves_an_ignored_newer_file_from_catalog() {
        let installed = mod_file(10, "installed.zip", 100);
        let ignored = versioned_mod_file(20, "installed v2.zip", 200, "2");
        let profile = profile(vec![installed.clone(), ignored.clone()], 200);
        let file_set = fallback_file_set(&installed);
        let ignored_signature = candidate_signature(vec![tracked_file_meta_from_mod_file(&ignored)]);
        let mut source = ModSourceData {
            file_set,
            raw_profile_json: serde_json::to_string(&profile).ok(),
            ignored_update_signature: ignored_signature,
            ..Default::default()
        };

        let state = apply_ignored_update_override_with_signature(
            &mut source,
            ModUpdateState::UpToDate,
            None,
        );
        assert_eq!(state, ModUpdateState::UpToDate);
        assert!(source.ignored_update_signature.is_some());

        let state = apply_ignored_update_override_with_signature(
            &mut source,
            ModUpdateState::MissingSource,
            None,
        );
        assert_eq!(state, ModUpdateState::MissingSource);
        assert!(source.ignored_update_signature.is_some());
    }

    #[test]
    fn unresolved_local_state_clears_when_ignored_file_is_removed_or_changed() {
        let installed = mod_file(10, "installed.zip", 100);
        let ignored = versioned_mod_file(20, "installed v2.zip", 200, "2");
        let file_set = fallback_file_set(&installed);
        let ignored_signature = candidate_signature(vec![tracked_file_meta_from_mod_file(&ignored)]);

        let removed_profile = profile(vec![installed.clone()], 200);
        let mut removed_source = ModSourceData {
            file_set: file_set.clone(),
            raw_profile_json: serde_json::to_string(&removed_profile).ok(),
            ignored_update_signature: ignored_signature.clone(),
            ..Default::default()
        };
        apply_ignored_update_override_with_signature(
            &mut removed_source,
            ModUpdateState::MissingSource,
            None,
        );
        assert!(removed_source.ignored_update_signature.is_none());

        let changed = versioned_mod_file(20, "renamed.zip", 201, "3");
        let changed_profile = profile(vec![installed, changed], 201);
        let mut changed_source = ModSourceData {
            file_set,
            raw_profile_json: serde_json::to_string(&changed_profile).ok(),
            ignored_update_signature: ignored_signature,
            ..Default::default()
        };
        apply_ignored_update_override_with_signature(
            &mut changed_source,
            ModUpdateState::UpToDate,
            None,
        );
        assert!(changed_source.ignored_update_signature.is_none());
    }

    #[test]
    fn real_newer_local_signature_clears_an_ignored_ai_candidate() {
        let installed = mod_file(10, "installed.zip", 100);
        let ignored = versioned_mod_file(20, "installed v2.zip", 200, "2");
        let newer = versioned_mod_file(30, "installed v3.zip", 300, "3");
        let file_set = fallback_file_set(&installed);
        let mut source = ModSourceData {
            file_set,
            ignored_update_signature: candidate_signature(vec![
                tracked_file_meta_from_mod_file(&ignored),
            ]),
            ..Default::default()
        };
        let current = candidate_signature(vec![tracked_file_meta_from_mod_file(&newer)]);

        let state = apply_ignored_update_override_with_signature(
            &mut source,
            ModUpdateState::UpdateAvailable,
            current,
        );
        assert_eq!(state, ModUpdateState::UpdateAvailable);
        assert!(source.ignored_update_signature.is_none());
    }
}

#[cfg(test)]
mod ignore_local_changes_tests {
    use super::*;

    fn mod_entry_with_baseline() -> ModEntry {
        let baseline_mtime = chrono::Utc::now() - chrono::Duration::hours(1);
        let current_mtime = chrono::Utc::now();
        let profile = gamebanana::ProfileResponse {
            id: 1,
            date_modified: 100,
            date_updated: Some(100),
            ..Default::default()
        };
        let source = ModSourceData {
            gamebanana: Some(GameBananaLink {
                mod_id: 1,
                url: "https://gamebanana.com/mods/1".to_string(),
            }),
            raw_profile_json: serde_json::to_string(&profile).ok(),
            baseline_content_mtime: Some(baseline_mtime),
            baseline_ini_hash: Some("baseline".to_string()),
            ..Default::default()
        };
        ModEntry {
            id: "mod".to_string(),
            game_id: "test".to_string(),
            folder_name: "Arcane".to_string(),
            root_path: std::path::PathBuf::from("Arcane"),
            status: ModStatus::Active,
            metadata: crate::model::ModMetadata::default(),
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            content_mtime: Some(current_mtime),
            ini_hash: Some("edited".to_string()),
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
            source: Some(source),
            update_state: ModUpdateState::ModifiedLocally,
        }
    }

    #[test]
    fn accepting_hides_modified_without_touching_the_baseline_and_is_reversible() {
        let mut mod_entry = mod_entry_with_baseline();
        let baseline_mtime = mod_entry.source.as_ref().unwrap().baseline_content_mtime;
        assert!(mod_has_local_changes_for_update_check(&mod_entry));
        assert!(!mod_ignores_local_changes(&mod_entry));

        assert!(set_mod_ignore_local_changes(&mut mod_entry, true));
        assert!(mod_ignores_local_changes(&mod_entry));
        assert!(!mod_has_local_changes_for_update_check(&mod_entry));
        assert_ne!(mod_entry.update_state, ModUpdateState::ModifiedLocally);
        let source = mod_entry.source.as_ref().unwrap();
        assert_eq!(source.baseline_content_mtime, baseline_mtime);
        assert_eq!(source.baseline_ini_hash.as_deref(), Some("baseline"));

        // Re-applying the same acceptance is a no-op.
        assert!(!set_mod_ignore_local_changes(&mut mod_entry, true));

        assert!(set_mod_ignore_local_changes(&mut mod_entry, false));
        assert!(!mod_ignores_local_changes(&mod_entry));
        assert!(mod_has_local_changes_for_update_check(&mod_entry));
        assert_eq!(mod_entry.update_state, ModUpdateState::ModifiedLocally);
    }

    #[test]
    fn later_edits_surface_as_modified_again_while_accepted() {
        let mut mod_entry = mod_entry_with_baseline();
        assert!(set_mod_ignore_local_changes(&mut mod_entry, true));
        assert!(!mod_has_local_changes_for_update_check(&mod_entry));

        mod_entry.ini_hash = Some("edited again".to_string());
        assert!(mod_has_local_changes_for_update_check(&mod_entry));
        assert!(!mod_ignores_local_changes(&mod_entry));
        // The spent acceptance is dropped by the next check pass.
        assert!(clear_spent_local_changes_acceptance(&mut mod_entry));
        assert!(mod_entry.source.as_ref().unwrap().accepted_local_changes.is_none());
        assert!(!clear_spent_local_changes_acceptance(&mut mod_entry));

        // Accepting again snapshots the newer fingerprint.
        assert!(set_mod_ignore_local_changes(&mut mod_entry, true));
        assert!(!mod_has_local_changes_for_update_check(&mod_entry));
    }

    #[test]
    fn files_matching_the_baseline_are_never_modified_even_when_accepted() {
        let mut mod_entry = mod_entry_with_baseline();
        assert!(set_mod_ignore_local_changes(&mut mod_entry, true));
        let source = mod_entry.source.as_ref().unwrap();
        mod_entry.content_mtime = source.baseline_content_mtime;
        mod_entry.ini_hash = source.baseline_ini_hash.clone();
        assert!(!mod_has_local_changes_for_update_check(&mod_entry));
    }

}
