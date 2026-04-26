fn spawn_browse_worker(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<BrowseRequest>,
    tx: WorkerTx<BrowseEvent>,
) {
    let client = runtime_services.http_client.clone();
    let json_limiter = Arc::clone(&runtime_services.json_limiter);
    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            match request {
                BrowseRequest::FetchPage {
                    nonce,
                    generation,
                    game_id,
                    query,
                    page,
                    browse_sort,
                    search_sort,
                    force_refresh,
                } => {
                    let Some(gamebanana_id) = gamebanana::game_id_for_hestia(&game_id) else {
                        let _ = tx.send(BrowseEvent::PageFailed {
                            _nonce: nonce,
                            generation,
                            error: format!("unsupported game id: {game_id}"),
                        });
                        continue;
                    };
                    let _permit = json_limiter.acquire().await.ok();
                    let cache_key = match query.as_deref() {
                        Some(query) if !query.trim().is_empty() => {
                            gamebanana::search_page_cache_key(&game_id, query, page, search_sort)
                        }
                        _ => gamebanana::browse_page_cache_key(&game_id, page, browse_sort),
                    };
                    let result = load_browse_page_with_cache(
                        &portable,
                        &client,
                        gamebanana_id,
                        &game_id,
                        query.as_deref(),
                        page,
                        browse_sort,
                        search_sort,
                        force_refresh,
                        &cache_key,
                    )
                    .await;
                    match result {
                        Ok((payload, used_cache_fallback)) => {
                            if used_cache_fallback {
                                let _ = tx.send(BrowseEvent::PageWarning {
                                    _nonce: nonce,
                                    generation,
                                    warning: "Connection failed".to_string(),
                                });
                            }
                            let _ = tx.send(BrowseEvent::PageLoaded {
                                _nonce: nonce,
                                generation,
                                game_id,
                                query,
                                page,
                                payload,
                            });
                        }
                        Err(err) => {
                            let _ = tx.send(BrowseEvent::PageFailed {
                                _nonce: nonce,
                                generation,
                                error: format!("{err:#}"),
                            });
                        }
                    }
                }
                BrowseRequest::FetchDetail {
                    nonce,
                    mod_id,
                    force_refresh,
                    cached_profile_json,
                } => {
                    let _permit = json_limiter.acquire().await.ok();
                    let cache_key = gamebanana::profile_cache_key(mod_id);
                    match load_profile_with_cache(
                        &portable,
                        &client,
                        mod_id,
                        force_refresh,
                        &cache_key,
                        cached_profile_json.as_deref(),
                    )
                    .await
                    {
                        Ok((profile, used_cache_fallback)) => {
                            if used_cache_fallback {
                                let _ = tx.send(BrowseEvent::DetailWarning {
                                    _nonce: nonce,
                                    mod_id,
                                    warning: "Connection failed".to_string(),
                                });
                            }
                            let _ = tx.send(BrowseEvent::DetailLoaded {
                                _nonce: nonce,
                                mod_id,
                                profile,
                            });
                        }
                        Err(err) => {
                            let _ = tx.send(BrowseEvent::DetailFailed {
                                _nonce: nonce,
                                mod_id,
                                error: format!("{err:#}"),
                            });
                        }
                    }
                }
                BrowseRequest::FetchUpdates { nonce, mod_id, force_refresh } => {
                    let _permit = json_limiter.acquire().await.ok();
                    let cache_key = gamebanana::updates_cache_key(mod_id);
                    match load_updates_with_cache(&portable, &client, mod_id, force_refresh, &cache_key).await {
                        Ok((updates, used_cache_fallback)) => {
                            if used_cache_fallback {
                                let _ = tx.send(BrowseEvent::UpdatesWarning {
                                    _nonce: nonce,
                                    mod_id,
                                    warning: "Connection failed".to_string(),
                                });
                            }
                            let _ = tx.send(BrowseEvent::UpdatesLoaded {
                                _nonce: nonce,
                                mod_id,
                                updates,
                            });
                        }
                        Err(err) => {
                            let _ = tx.send(BrowseEvent::UpdatesFailed {
                                _nonce: nonce,
                                mod_id,
                                error: format!("{err:#}"),
                            });
                        }
                    }
                }
            }
        }
    });
}

async fn load_browse_page_with_cache(
    portable: &PortablePaths,
    client: &ClientWithMiddleware,
    gamebanana_id: u64,
    _game_id: &str,
    query: Option<&str>,
    page: usize,
    browse_sort: BrowseSort,
    search_sort: SearchSort,
    force_refresh: bool,
    cache_key: &str,
) -> Result<(gamebanana::ApiEnvelope<gamebanana::BrowseRecord>, bool)> {
    if !force_refresh {
        if let Some(cached) = persistence::cache_get(portable, cache_key)? {
            if let Ok(payload) = serde_json::from_slice::<gamebanana::ApiEnvelope<gamebanana::BrowseRecord>>(&cached) {
                return Ok((payload, false));
            }
        }
    }

    let fetch_result = match query {
        Some(query) if !query.trim().is_empty() => {
            gamebanana::fetch_search_page_async(client, gamebanana_id, query, page, search_sort).await
        }
        _ => gamebanana::fetch_browse_page_async(client, gamebanana_id, page, browse_sort).await,
    };

    match fetch_result {
        Ok(payload) => {
            if let Ok(bytes) = serde_json::to_vec(&payload) {
                let _ = persistence::cache_put(
                    portable,
                    cache_key,
                    "browse-json",
                    &bytes,
                    0,
                );
            }
            Ok((payload, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = persistence::cache_get(portable, cache_key)? {
                if let Ok(payload) = serde_json::from_slice::<gamebanana::ApiEnvelope<gamebanana::BrowseRecord>>(&cached) {
                    return Ok((payload, true));
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

async fn load_profile_with_cache(
    portable: &PortablePaths,
    client: &ClientWithMiddleware,
    mod_id: u64,
    force_refresh: bool,
    cache_key: &str,
    cached_profile_json: Option<&str>,
) -> Result<(gamebanana::ProfileResponse, bool)> {
    if !force_refresh {
        if let Some(raw) = cached_profile_json {
            if let Ok(profile) = serde_json::from_str::<gamebanana::ProfileResponse>(raw) {
                return Ok((profile, false));
            }
        }
        if let Some(cached) = persistence::cache_get(portable, cache_key)? {
            if let Ok(profile) = serde_json::from_slice::<gamebanana::ProfileResponse>(&cached) {
                return Ok((profile, false));
            }
        }
    }

    match gamebanana::fetch_profile_async(client, mod_id).await {
        Ok(profile) => {
            if let Ok(bytes) = serde_json::to_vec(&profile) {
                let _ = persistence::cache_put(
                    portable,
                    cache_key,
                    "browse-json",
                    &bytes,
                    0,
                );
            }
            Ok((profile, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = persistence::cache_get(portable, cache_key)? {
                if let Ok(profile) = serde_json::from_slice::<gamebanana::ProfileResponse>(&cached) {
                    return Ok((profile, true));
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

async fn load_updates_with_cache(
    portable: &PortablePaths,
    client: &ClientWithMiddleware,
    mod_id: u64,
    force_refresh: bool,
    cache_key: &str,
) -> Result<(gamebanana::ApiEnvelope<gamebanana::UpdateRecord>, bool)> {
    if !force_refresh {
        if let Some(cached) = persistence::cache_get(portable, cache_key)? {
            if let Ok(updates) = serde_json::from_slice::<gamebanana::ApiEnvelope<gamebanana::UpdateRecord>>(&cached) {
                return Ok((updates, false));
            }
        }
    }

    match gamebanana::fetch_updates_async(client, mod_id).await {
        Ok(updates) => {
            if let Ok(bytes) = serde_json::to_vec(&updates) {
                let _ = persistence::cache_put(
                    portable,
                    cache_key,
                    "browse-json",
                    &bytes,
                    0,
                );
            }
            Ok((updates, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = persistence::cache_get(portable, cache_key)? {
                if let Ok(updates) = serde_json::from_slice::<gamebanana::ApiEnvelope<gamebanana::UpdateRecord>>(&cached) {
                    return Ok((updates, true));
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

fn spawn_browse_image_workers(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    cache_limit_bytes: Arc<std::sync::atomic::AtomicU64>,
    mut rx: WorkerRx<BrowseImageRequest>,
    tx: WorkerTx<BrowseImageResult>,
) {
    let client = runtime_services.http_client.clone();
    let handle = runtime_services.handle();
    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            let client = client.clone();
            let portable = portable.clone();
            let tx = tx.clone();
            let handle = handle.clone();
            let cache_limit_bytes = Arc::clone(&cache_limit_bytes);
            tokio::spawn(async move {
                if request.cancel.load(Ordering::Relaxed) {
                    return;
                }

                let url = request.url.clone();
                let cache_key = request.cache_key.clone();
                let limit = cache_limit_bytes.load(Ordering::Relaxed);
                let bytes_result = async {
                    if let Some(cached) = persistence::cache_get(&portable, &cache_key)? {
                        return Ok::<Vec<u8>, anyhow::Error>(cached);
                    }
                    let bytes = client
                        .get(&url)
                        .send()
                        .await?
                        .error_for_status()?
                        .bytes()
                        .await?
                        .to_vec();
                    let _ = persistence::cache_put(&portable, &cache_key, "browse-img", &bytes, limit);
                    Ok(bytes)
                }
                .await;

                match bytes_result {
                    Ok(bytes) => {
                        if request.cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let thumb_profile = request.thumb_profile;
                        let load_full = request.load_full;
                        let result = handle
                            .spawn_blocking(move || BrowseImageResult {
                                texture_key: request.texture_key,
                                thumb_texture_key: request.thumb_texture_key,
                                image_full: if load_full {
                                    load_cover_color_image(&bytes)
                                } else {
                                    None
                                },
                                image_thumb: load_cover_color_image_thumbnail(&bytes, thumb_profile),
                                cancel_key: request.cancel_key,
                                failure: None,
                            })
                            .await;

                        if let Ok(result) = result {
                            let _ = tx.send(result);
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(BrowseImageResult {
                            texture_key: request.texture_key,
                            thumb_texture_key: request.thumb_texture_key,
                            image_full: None,
                            image_thumb: None,
                            cancel_key: request.cancel_key,
                            failure: Some(BrowseImageFailure {
                                url,
                                timed_out: is_timeout_error(&err),
                            }),
                        });
                    }
                }
            });
        }
    });
}

