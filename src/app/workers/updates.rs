/// More missing lists than this and the path model is skipped for the mod;
/// it needs every list of the group, so a partial fetch would be wasted.
const FILE_LISTING_FETCH_LIMIT: usize = 30;
const FILE_LISTING_FETCH_CONCURRENCY: usize = 4;
/// Time allowed for all of one mod's missing lists.  Whatever is still
/// unanswered then counts as a failure.
const FILE_LISTING_FETCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);
/// A file whose list failed is not asked for again before this.
const FILE_LISTING_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(30 * 60);

static FILE_LISTING_FAILURES: std::sync::OnceLock<
    std::sync::Mutex<HashMap<u64, std::time::Instant>>,
> = std::sync::OnceLock::new();

fn file_listing_failures() -> std::sync::MutexGuard<'static, HashMap<u64, std::time::Instant>> {
    FILE_LISTING_FAILURES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn file_listing_failed_recently(file_id: u64) -> bool {
    let mut failures = file_listing_failures();
    failures.retain(|_, at| at.elapsed() < FILE_LISTING_RETRY_AFTER);
    failures.contains_key(&file_id)
}

fn file_listing_cache_key(file_id: u64) -> String {
    format!("gamebanana-raw-file-list-v1-{file_id}")
}

fn read_cached_file_listings(
    portable: &PortablePaths,
    file_ids: &[u64],
) -> Vec<(u64, gamebanana::RawFileList)> {
    file_ids
        .iter()
        .filter_map(|id| {
            let bytes = persistence::cache_get(portable, &file_listing_cache_key(*id))
                .ok()
                .flatten()?;
            Some((*id, serde_json::from_slice(&bytes).ok()?))
        })
        .collect()
}

async fn cached_file_listings(
    portable: &PortablePaths,
    file_ids: Vec<u64>,
) -> Vec<(u64, gamebanana::RawFileList)> {
    let portable = portable.clone();
    tokio::task::spawn_blocking(move || read_cached_file_listings(&portable, &file_ids))
        .await
        .unwrap_or_default()
}

/// Attach every archive list already cached for the profile's files, so a
/// page loaded anywhere gives the matcher the same lists.
async fn attach_cached_file_listings(
    portable: &PortablePaths,
    profile: &mut gamebanana::ProfileResponse,
) {
    let ids: Vec<u64> = profile
        .files
        .iter()
        .chain(&profile.archived_files)
        .map(|file| file.id)
        .filter(|id| *id != 0 && !profile.file_listings.contains_key(id))
        .collect();
    if ids.is_empty() {
        return;
    }
    for (id, answer) in cached_file_listings(portable, ids).await {
        if let gamebanana::RawFileList::Paths(paths) = answer {
            profile.file_listings.insert(id, paths);
        }
    }
}

/// Fetch the lists the matcher asked for, a few at a time.  Every final
/// answer is cached by file id, since a file's contents never change; a
/// failure is remembered for a while so later checks do not wait on it.
/// Returns whether any list was attached.
async fn attach_wanted_file_listings(
    client: &ClientWithMiddleware,
    portable: &PortablePaths,
    profile: &mut gamebanana::ProfileResponse,
    wanted: Vec<u64>,
) -> bool {
    let wanted: Vec<u64> = wanted
        .into_iter()
        .filter(|id| !profile.file_listings.contains_key(id))
        .collect();
    if wanted.is_empty() || wanted.len() > FILE_LISTING_FETCH_LIMIT {
        return false;
    }
    let mut attached = false;
    let mut unavailable = false;
    let cached = cached_file_listings(portable, wanted.clone()).await;
    let cached_ids: HashSet<u64> = cached.iter().map(|(id, _)| *id).collect();
    for (id, answer) in cached {
        match answer {
            gamebanana::RawFileList::Paths(paths) => {
                profile.file_listings.insert(id, paths);
                attached = true;
            }
            gamebanana::RawFileList::Unavailable => unavailable = true,
        }
    }
    let to_fetch: Vec<u64> = wanted
        .into_iter()
        .filter(|id| !cached_ids.contains(id))
        .collect();
    // The path model needs every list, so one that cannot be had makes the
    // rest useless for now.
    if unavailable
        || to_fetch.is_empty()
        || to_fetch.iter().any(|id| file_listing_failed_recently(*id))
    {
        return attached;
    }

    let deadline = tokio::time::Instant::now() + FILE_LISTING_FETCH_BUDGET;
    let mut answers = futures_util::stream::iter(to_fetch.iter().copied().map(|id| async move {
        (id, gamebanana::fetch_raw_file_list_async(client, id).await)
    }))
    .buffer_unordered(FILE_LISTING_FETCH_CONCURRENCY);
    let mut answered = HashSet::new();
    let mut finished = Vec::new();
    while let Ok(Some((id, answer))) = tokio::time::timeout_at(deadline, answers.next()).await {
        answered.insert(id);
        if let Ok(answer) = answer {
            finished.push((id, answer));
        }
    }
    drop(answers);

    let now = std::time::Instant::now();
    let finished_ids: HashSet<u64> = finished.iter().map(|(id, _)| *id).collect();
    file_listing_failures().extend(
        to_fetch
            .iter()
            .filter(|id| !finished_ids.contains(id))
            .map(|id| (*id, now)),
    );
    let writes: Vec<(u64, Vec<u8>)> = finished
        .iter()
        .filter_map(|(id, answer)| Some((*id, serde_json::to_vec(answer).ok()?)))
        .collect();
    let write_portable = portable.clone();
    let _ = tokio::task::spawn_blocking(move || {
        for (id, bytes) in writes {
            let _ = persistence::cache_put(
                &write_portable,
                &file_listing_cache_key(id),
                "gamebanana-file-list",
                &bytes,
                0,
            );
        }
    })
    .await;
    for (id, answer) in finished {
        if let gamebanana::RawFileList::Paths(paths) = answer {
            profile.file_listings.insert(id, paths);
            attached = true;
        }
    }
    attached
}

fn update_check_group_key(item: &UpdateCheckItem) -> (String, u64, bool) {
    (item.1.clone(), item.2, item.5)
}

fn group_update_check_items(
    items: impl IntoIterator<Item = UpdateCheckItem>,
) -> HashMap<(String, u64, bool), Vec<UpdateCheckItem>> {
    let mut groups = HashMap::new();
    for item in items {
        groups
            .entry(update_check_group_key(&item))
            .or_insert_with(Vec::new)
            .push(item);
    }
    groups
}

fn merge_target_items_into_peer_group(
    mut peer_items: Vec<UpdateCheckItem>,
    target_items: &[UpdateCheckItem],
) -> (Vec<UpdateCheckItem>, Vec<usize>) {
    let mut target_positions = Vec::with_capacity(target_items.len());
    for item in target_items {
        let position = peer_items
            .iter()
            .position(|peer| peer.0 == item.0)
            .unwrap_or_else(|| {
                peer_items.push(item.clone());
                peer_items.len() - 1
            });
        target_positions.push(position);
    }
    (peer_items, target_positions)
}

fn update_check_error_outcomes(
    target_items: &[UpdateCheckItem],
    error: String,
    peer_fingerprint: u64,
) -> Vec<UpdateCheckOutcome> {
    target_items
        .iter()
        .map(|item| UpdateCheckOutcome {
            mod_id: item.0.clone(),
            game_id: item.1.clone(),
            gb_id: item.2,
            gb_is_tool: item.5,
            file_set: item.4.clone(),
            state: ModUpdateState::MissingSource,
            signature: None,
            snapshot: None,
            error: Some(error.clone()),
            raw_json: None,
            profile: None,
            profile_cached_at: None,
            peer_fingerprint,
        })
        .collect()
}

fn spawn_update_check_worker(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<UpdateCheckRequest>,
    tx: WorkerTx<UpdateCheckResult>,
) {
    let runtime_services = runtime_services.clone();
    let json_limiter = Arc::clone(&runtime_services.json_limiter);
    runtime_services.clone().spawn(async move {
        while let Some(request) = rx.recv().await {
            let client = runtime_services.http_client();
            let generation = request.generation;
            let peer_groups = group_update_check_items(request.peer_items);
            let target_groups = group_update_check_items(request.items);

            let stream = futures_util::stream::iter(target_groups.into_iter().map(
                |((game_id, gb_id, gb_is_tool), target_items)| {
                    let client = client.clone();
                    let portable = portable.clone();
                    let json_limiter = Arc::clone(&json_limiter);
                    let peer_items = peer_groups
                        .get(&(game_id.clone(), gb_id, gb_is_tool))
                        .cloned()
                        .unwrap_or_default();
                    let (fingerprint_items, _) = merge_target_items_into_peer_group(
                        peer_items.clone(),
                        &target_items,
                    );
                    let peer_fingerprint = update_check_group_fingerprint(&fingerprint_items);
                    async move {
                        let _permit = json_limiter.acquire().await.ok();
                        let target_outcome = |item: &UpdateCheckItem,
                                              state: ModUpdateState,
                                              signature: Option<IgnoredUpdateSignature>,
                                              snapshot: Option<GameBananaSnapshot>,
                                              error: Option<String>,
                                              raw_json: Option<String>,
                                              profile: Option<Box<gamebanana::ProfileResponse>>,
                                               profile_cached_at: Option<DateTime<Utc>>|
                         -> UpdateCheckOutcome {
                            UpdateCheckOutcome {
                                mod_id: item.0.clone(),
                                game_id: item.1.clone(),
                                gb_id: item.2,
                                gb_is_tool: item.5,
                                file_set: item.4.clone(),
                                state,
                                signature,
                                snapshot,
                                error,
                                raw_json,
                                profile,
                                profile_cached_at,
                                 peer_fingerprint,
                            }
                        };
                        match gamebanana::fetch_profile_async_typed(&client, gb_id, gb_is_tool).await
                        {
                            Ok(profile)
                                if !gamebanana::is_unavailable(&profile)
                                    && downloadable_all_files(&profile).is_empty()
                                    && peer_items
                                        .iter()
                                        .chain(target_items.iter())
                                        .any(|item| file_set_tracks_remote_files(&item.4)) =>
                            {
                                let error = format!(
                                    "GameBanana returned a profile with no downloadable files \
                                     for {gb_id}; treating it as a failed check"
                                );
                                update_check_error_outcomes(&target_items, error, peer_fingerprint)
                            }
                            Ok(mut profile) => {
                                if !gb_is_tool {
                                    gamebanana::attach_update_history_async(
                                        &client,
                                        &mut profile,
                                        false,
                                    )
                                    .await;
                                }
                                attach_cached_file_listings(&portable, &mut profile).await;
                                let (full_items, target_positions) =
                                    merge_target_items_into_peer_group(peer_items, &target_items);
                                let group_items: Vec<_> = full_items
                                    .iter()
                                    .map(|item| (item.3, item.4.clone()))
                                    .collect();
                                let mut evaluations =
                                    evaluate_file_set_update_group(&group_items, &profile);
                                let wanted = file_listing_ids_wanted(&group_items, &profile);
                                if attach_wanted_file_listings(&client, &portable, &mut profile, wanted)
                                    .await
                                {
                                    evaluations =
                                        evaluate_file_set_update_group(&group_items, &profile);
                                }
                                let snapshot = profile_to_snapshot(&profile);
                                let is_unavailable = gamebanana::is_unavailable(&profile);
                                let raw_json = serde_json::to_string(&profile).ok();
                                if let Some(raw) = raw_json.as_deref() {
                                    let _ = persistence::cache_put(
                                        &portable,
                                        &gamebanana::profile_cache_key_typed(gb_id, gb_is_tool),
                                        "browse-json",
                                        raw.as_bytes(),
                                        0,
                                    );
                                }
                                let profile_cached_at = Some(Utc::now());
                                target_items
                                    .iter()
                                    .zip(target_positions)
                                    .map(|(item, position)| {
                                        let evaluation = evaluations.get(position).cloned().unwrap_or(
                                            FileSetUpdateEvaluation {
                                                state: ModUpdateState::MissingSource,
                                                signature: None,
                                            },
                                        );
                                        let state = if is_unavailable {
                                            ModUpdateState::MissingSource
                                        } else {
                                            evaluation.state
                                        };
                                        target_outcome(
                                            item,
                                            state,
                                            evaluation.signature,
                                            Some(snapshot.clone()),
                                            gamebanana::unavailable_reason(&profile),
                                            raw_json.clone(),
                                            Some(Box::new(profile.clone())),
                                            profile_cached_at,
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }
                            Err(err) => {
                                update_check_error_outcomes(
                                    &target_items,
                                    format!("{err:#}"),
                                    peer_fingerprint,
                                )
                            }
                        }
                    }
                },
            ))
            .buffer_unordered(JSON_LIMIT);

            let mut states: Vec<_> = stream.collect::<Vec<_>>().await.into_iter().flatten().collect();
            states.sort_by(|a, b| a.mod_id.cmp(&b.mod_id));
            let _ = tx.send(UpdateCheckResult { generation, states });
        }
    });
}

#[cfg(test)]
mod update_grouping_tests {
    use super::*;

    fn item(id: &str, game_id: &str, gb_id: u64, is_tool: bool) -> UpdateCheckItem {
        (
            id.to_owned(),
            game_id.to_owned(),
            gb_id,
            None,
            FileSetRecipe::default(),
            is_tool,
        )
    }

    #[test]
    fn peer_grouping_keeps_game_and_tool_boundaries() {
        let groups = group_update_check_items([
            item("a", "game-a", 7, false),
            item("b", "game-a", 7, false),
            item("c", "game-b", 7, false),
            item("d", "game-a", 7, true),
        ]);
        assert_eq!(groups.get(&("game-a".to_owned(), 7, false)).unwrap().len(), 2);
        assert_eq!(groups.get(&("game-b".to_owned(), 7, false)).unwrap().len(), 1);
        assert_eq!(groups.get(&("game-a".to_owned(), 7, true)).unwrap().len(), 1);
    }

    #[test]
    fn single_target_is_evaluated_at_its_peer_position_but_only_target_is_emitted() {
        let peers = vec![item("peer", "game", 7, false)];
        let targets = vec![item("target", "game", 7, false)];
        let (full, positions) = merge_target_items_into_peer_group(peers, &targets);
        assert_eq!(full.iter().map(|item| item.0.as_str()).collect::<Vec<_>>(), ["peer", "target"]);
        assert_eq!(positions, vec![1]);
        let target_ids: Vec<_> = targets.iter().map(|item| item.0.as_str()).collect();
        assert_eq!(target_ids, vec!["target"]);
    }

    #[test]
    fn grouped_fetch_error_emits_only_requested_targets() {
        let peers = vec![item("peer", "game", 7, false)];
        let targets = vec![item("target", "game", 7, false)];
        let outcomes = update_check_error_outcomes(
            &targets,
            "offline".to_owned(),
            update_check_group_fingerprint(&[peers[0].clone(), targets[0].clone()]),
        );
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].mod_id, "target");
        assert_eq!(outcomes[0].state, ModUpdateState::MissingSource);
        assert_eq!(outcomes[0].error.as_deref(), Some("offline"));
        assert!(!outcomes.iter().any(|outcome| outcome.mod_id == peers[0].0));
    }

    #[test]
    fn peer_fingerprint_is_order_independent_but_recipe_sensitive() {
        let first = item("first", "game", 7, false);
        let mut second = item("second", "game", 7, false);
        let ordered = vec![first.clone(), second.clone()];
        let reversed = vec![second.clone(), first.clone()];
        assert_eq!(
            update_check_group_fingerprint(&ordered),
            update_check_group_fingerprint(&reversed)
        );
        second.4.selected_file_ids.push(42);
        assert_ne!(
            update_check_group_fingerprint(&ordered),
            update_check_group_fingerprint(&[first, second])
        );
    }
}
