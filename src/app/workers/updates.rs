fn spawn_update_check_worker(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<UpdateCheckRequest>,
    tx: WorkerTx<UpdateCheckResult>,
) {
    let client = runtime_services.http_client.clone();
    let json_limiter = Arc::clone(&runtime_services.json_limiter);
    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            let mut grouped: HashMap<u64, Vec<(usize, String, Option<i64>, FileSetRecipe)>> =
                HashMap::new();
            for (idx, (local_mod_id, _game_id, gb_id, local_sync_ts, file_set)) in
                request.items.into_iter().enumerate()
            {
                grouped
                    .entry(gb_id)
                    .or_default()
                    .push((idx, local_mod_id, local_sync_ts, file_set));
            }

            let stream = futures_util::stream::iter(grouped.into_iter().map(
                |(gb_id, local_items)| {
                    let client = client.clone();
                    let portable = portable.clone();
                    let json_limiter = Arc::clone(&json_limiter);
                    async move {
                        let _permit = json_limiter.acquire().await.ok();
                        match gamebanana::fetch_profile_async(&client, gb_id).await {
                            Ok(profile) => {
                                let snapshot = profile_to_snapshot(&profile);
                                let is_unavailable = gamebanana::is_unavailable(&profile);

                                let raw_json = serde_json::to_string(&profile).ok();
                                if let Some(raw) = raw_json.as_deref() {
                                    let _ = persistence::cache_put(
                                        &portable,
                                        &gamebanana::profile_cache_key(gb_id),
                                        "browse-json",
                                        raw.as_bytes(),
                                        0,
                                    );
                                }
                                let err_msg = gamebanana::unavailable_reason(&profile);
                                local_items
                                    .into_iter()
                                    .map(|(idx, local_mod_id, local_sync_ts, file_set)| {
                                        let state = if is_unavailable {
                                            ModUpdateState::MissingSource
                                        } else {
                                            determine_file_set_update_state(
                                                &file_set,
                                                local_sync_ts,
                                                &profile,
                                            )
                                        };
                                        (
                                            idx,
                                            (
                                                local_mod_id,
                                                state,
                                                Some(snapshot.clone()),
                                                err_msg.clone(),
                                                raw_json.clone(),
                                                Some(Box::new(profile.clone())),
                                            ),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }
                            Err(err) => {
                                let error = Some(format!("{err:#}"));
                                local_items
                                    .into_iter()
                                    .map(|(idx, local_mod_id, _, _)| {
                                        (
                                            idx,
                                            (
                                                local_mod_id,
                                                ModUpdateState::MissingSource,
                                                None,
                                                error.clone(),
                                                None,
                                                None,
                                            ),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }
                        }
                    }
                },
            ))
            .buffer_unordered(JSON_LIMIT);

            let mut ordered: Vec<_> = stream.collect::<Vec<_>>().await.into_iter().flatten().collect();
            ordered.sort_by_key(|(idx, _)| *idx);
            let states = ordered.into_iter().map(|(_, state)| state).collect();
            let _ = tx.send(UpdateCheckResult { states });
        }
    });
}

