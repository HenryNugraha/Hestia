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
            let stream = futures_util::stream::iter(request.items.into_iter().enumerate().map(
                |(idx, (local_mod_id, _game_id, gb_id, local_sync_ts, file_set))| {
                    let client = client.clone();
                    let portable = portable.clone();
                    let json_limiter = Arc::clone(&json_limiter);
                    async move {
                        let _permit = json_limiter.acquire().await.ok();
                        let state_item = match gamebanana::fetch_profile_async(&client, gb_id).await {
                            Ok(profile) => {
                                let snapshot = profile_to_snapshot(&profile);
                                let is_unavailable = gamebanana::is_unavailable(&profile);

                                let state = if is_unavailable {
                                    ModUpdateState::MissingSource
                                } else {
                                    determine_file_set_update_state(&file_set, local_sync_ts, &profile)
                                };

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
                                (
                                    local_mod_id,
                                    state,
                                    Some(snapshot),
                                    err_msg,
                                    raw_json,
                                    Some(Box::new(profile)),
                                )
                            }
                            Err(err) => (
                                local_mod_id,
                                ModUpdateState::MissingSource,
                                None,
                                Some(format!("{err:#}")),
                                None,
                                None,
                            ),
                        };
                        (idx, state_item)
                    }
                },
            ))
            .buffer_unordered(JSON_LIMIT);

            let mut ordered: Vec<_> = stream.collect().await;
            ordered.sort_by_key(|(idx, _)| *idx);
            let states = ordered.into_iter().map(|(_, state)| state).collect();
            let _ = tx.send(UpdateCheckResult { states });
        }
    });
}

