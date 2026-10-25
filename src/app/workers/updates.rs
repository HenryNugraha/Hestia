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
                                let (full_items, target_positions) =
                                    merge_target_items_into_peer_group(peer_items, &target_items);
                                let group_items: Vec<_> = full_items
                                    .iter()
                                    .map(|item| (item.3, item.4.clone()))
                                    .collect();
                                let evaluations = evaluate_file_set_update_group(&group_items, &profile);
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
