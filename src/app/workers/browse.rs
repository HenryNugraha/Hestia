fn spawn_browse_worker(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<BrowseRequest>,
    tx: WorkerTx<BrowseEvent>,
) {
    let client = runtime_services.http_client.clone();
    let json_limiter = Arc::clone(&runtime_services.json_limiter);
    let active_page_task = Arc::new(Mutex::new(None::<tokio::task::AbortHandle>));
    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            match request {
                BrowseRequest::CancelPage => {
                    if let Ok(mut active_task) = active_page_task.lock() {
                        if let Some(task) = active_task.take() {
                            task.abort();
                        }
                    }
                }
                BrowseRequest::FetchPage {
                    nonce,
                    generation,
                    game_id,
                    query,
                    character_category_id,
                    page,
                    browse_sort,
                    search_sort,
                    force_refresh,
                } => {
                    // Browse list queries are interactive and deliberately bypass the shared
                    // JSON limiter used by background update/profile work. Keep exactly one
                    // active query; a newer search, filter, or sort cancels the older request.
                    if let Ok(mut active_task) = active_page_task.lock() {
                        if let Some(task) = active_task.take() {
                            task.abort();
                        }
                    }
                    let page_tx = tx.clone();
                    let page_portable = portable.clone();
                    let page_task = tokio::spawn(async move {
                        let Some(gamebanana_id) = gamebanana::game_id_for_hestia(&game_id) else {
                            let _ = page_tx.send(BrowseEvent::PageFailed {
                                _nonce: nonce,
                                generation,
                                page,
                                error: format!("unsupported game id: {game_id}"),
                            });
                            return;
                        };
                        let cache_key = if let Some(category_id) = character_category_id {
                            gamebanana::character_browse_page_cache_key(
                                &game_id, category_id, query.as_deref(), page, browse_sort,
                            )
                        } else {
                            match query.as_deref() {
                                Some(query) if !query.trim().is_empty() => gamebanana::search_page_cache_key(
                                    &game_id, query, page, search_sort,
                                ),
                                _ => gamebanana::browse_page_cache_key(&game_id, page, browse_sort),
                            }
                        };

                        // Show known-good cached data immediately, then refresh it below.
                        if force_refresh {
                            if let Ok(Some(cached)) = persistence::cache_get(&page_portable, &cache_key) {
                                if let Ok(payload) = serde_json::from_slice(&cached) {
                                    let _ = page_tx.send(BrowseEvent::PageLoaded {
                                        _nonce: nonce,
                                        generation,
                                        game_id: game_id.clone(),
                                        query: query.clone(),
                                        character_category_id,
                                        page,
                                        payload,
                                    });
                                }
                            }
                        }

                        let started = Instant::now();
                        let result = load_browse_page_with_cache(
                            &page_portable, gamebanana_id, &game_id,
                            query.as_deref(), character_category_id, page, browse_sort,
                            search_sort, force_refresh, &cache_key,
                        ).await;
                        match result {
                            Ok((payload, refresh_error)) => {
                                if let Some(error) = refresh_error {
                                    let _ = page_tx.send(BrowseEvent::PageWarning {
                                        _nonce: nonce,
                                        generation,
                                        warning: format!(
                                            "request failed after {} ms; using cached results: {error}",
                                            started.elapsed().as_millis(),
                                        ),
                                    });
                                }
                                let _ = page_tx.send(BrowseEvent::PageLoaded {
                                    _nonce: nonce,
                                    generation,
                                    game_id,
                                    query,
                                    character_category_id,
                                    page,
                                    payload,
                                });
                            }
                            Err(err) => {
                                let _ = page_tx.send(BrowseEvent::PageFailed {
                                    _nonce: nonce,
                                    generation,
                                    page,
                                    error: format!(
                                        "request failed after {} ms (page={page}, browse_sort={browse_sort:?}, search_sort={search_sort:?}): {err:#}",
                                        started.elapsed().as_millis(),
                                    ),
                                });
                            }
                        }
                    });
                    if let Ok(mut active_task) = active_page_task.lock() {
                        *active_task = Some(page_task.abort_handle());
                    }
                }
                BrowseRequest::FetchCharacterCategories {
                    nonce,
                    game_id,
                    super_category_id,
                    force_refresh,
                } => {
                    let category_tx = tx.clone();
                    let category_portable = portable.clone();
                    tokio::spawn(async move {
                        let cache_key = gamebanana::character_categories_cache_key(
                            &game_id,
                            super_category_id,
                        );
                        match load_character_categories_with_cache(
                            &category_portable,
                            super_category_id,
                            force_refresh,
                            &cache_key,
                        )
                        .await
                        {
                            Ok((categories, used_cache_fallback)) => {
                                if used_cache_fallback {
                                    let _ = category_tx.send(BrowseEvent::CharacterCategoriesWarning {
                                        _nonce: nonce,
                                        game_id: game_id.clone(),
                                        warning: "Connection failed".to_string(),
                                    });
                                }
                                let _ = category_tx.send(BrowseEvent::CharacterCategoriesLoaded {
                                    _nonce: nonce,
                                    game_id,
                                    categories,
                                });
                            }
                            Err(err) => {
                                let _ = category_tx.send(BrowseEvent::CharacterCategoriesFailed {
                                    _nonce: nonce,
                                    game_id,
                                    error: format!("{err:#}"),
                                });
                            }
                        }
                    });
                }
                BrowseRequest::FetchDetail {
                    nonce,
                    mod_id,
                    force_refresh,
                    cached_profile_json,
                } => {
                    let detail_tx = tx.clone();
                    let detail_portable = portable.clone();
                    let detail_client = client.clone();
                    let detail_limiter = Arc::clone(&json_limiter);
                    tokio::spawn(async move {
                        let _permit = detail_limiter.acquire().await.ok();
                        let cache_key = gamebanana::profile_cache_key(mod_id);
                        match load_profile_with_cache(
                            &detail_portable,
                            &detail_client,
                            mod_id,
                            force_refresh,
                            &cache_key,
                            cached_profile_json.as_deref(),
                        )
                        .await
                        {
                            Ok((profile, used_cache_fallback)) => {
                                if used_cache_fallback {
                                    let _ = detail_tx.send(BrowseEvent::DetailWarning {
                                        _nonce: nonce,
                                        mod_id,
                                        warning: "Connection failed".to_string(),
                                    });
                                }
                                let _ = detail_tx.send(BrowseEvent::DetailLoaded {
                                    _nonce: nonce,
                                    mod_id,
                                    profile,
                                });
                            }
                            Err(err) => {
                                let _ = detail_tx.send(BrowseEvent::DetailFailed {
                                    _nonce: nonce,
                                    mod_id,
                                    error: format!("{err:#}"),
                                });
                            }
                        }
                    });
                }
                BrowseRequest::FetchUpdates { nonce, mod_id, force_refresh } => {
                    let updates_tx = tx.clone();
                    let updates_portable = portable.clone();
                    let updates_client = client.clone();
                    let updates_limiter = Arc::clone(&json_limiter);
                    tokio::spawn(async move {
                        let _permit = updates_limiter.acquire().await.ok();
                        let cache_key = gamebanana::updates_cache_key(mod_id);
                        match load_updates_with_cache(
                            &updates_portable,
                            &updates_client,
                            mod_id,
                            force_refresh,
                            &cache_key,
                        )
                        .await
                        {
                            Ok((updates, used_cache_fallback)) => {
                                if used_cache_fallback {
                                    let _ = updates_tx.send(BrowseEvent::UpdatesWarning {
                                        _nonce: nonce,
                                        mod_id,
                                        warning: "Connection failed".to_string(),
                                    });
                                }
                                let _ = updates_tx.send(BrowseEvent::UpdatesLoaded {
                                    _nonce: nonce,
                                    mod_id,
                                    updates,
                                });
                            }
                            Err(err) => {
                                let _ = updates_tx.send(BrowseEvent::UpdatesFailed {
                                    _nonce: nonce,
                                    mod_id,
                                    error: format!("{err:#}"),
                                });
                            }
                        }
                    });
                }
            }
        }
    });
}

/// Race up to three equivalent interactive JSON requests. The second request starts after
/// two seconds; the third starts two seconds later with a cache-busting query parameter.
/// The first valid response wins and all slower attempts are aborted.
async fn race_interactive_json<T, F, Fut>(fetch: F) -> Result<T>
where
    T: Send + 'static,
    F: Fn(bool) -> Fut,
    Fut: std::future::Future<Output = Result<T>> + Send + 'static,
{
    const DUPLICATE_DELAY: Duration = Duration::from_secs(2);

    let mut attempts = tokio::task::JoinSet::new();
    attempts.spawn(fetch(false));
    let mut started_attempts = 1;
    let first_duplicate = tokio::time::sleep(DUPLICATE_DELAY);
    let cache_busting_duplicate = tokio::time::sleep(DUPLICATE_DELAY * 2);
    tokio::pin!(first_duplicate);
    tokio::pin!(cache_busting_duplicate);
    let mut errors = Vec::new();

    loop {
        if started_attempts == 3 && attempts.is_empty() {
            bail!(
                "all interactive JSON attempts failed: {}",
                errors.join(" | ")
            );
        }

        tokio::select! {
            result = attempts.join_next(), if !attempts.is_empty() => match result {
                Some(Ok(Ok(value))) => {
                    attempts.abort_all();
                    return Ok(value);
                }
                Some(Ok(Err(err))) => errors.push(format!("{err:#}")),
                Some(Err(err)) => errors.push(format!("request task failed: {err}")),
                None => {}
            },
            _ = &mut first_duplicate, if started_attempts == 1 => {
                attempts.spawn(fetch(false));
                started_attempts = 2;
            }
            _ = &mut cache_busting_duplicate, if started_attempts == 2 => {
                attempts.spawn(fetch(true));
                started_attempts = 3;
            }
        }
    }
}

/// The stalled-request race must not queue behind the shared application's HTTP/1.1 pool.
/// Each attempt gets an isolated client and therefore its own connection pool/socket.
fn isolated_browse_json_client() -> Result<ClientWithMiddleware> {
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client = reqwest::Client::builder()
        .user_agent(gamebanana::USER_AGENT)
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(0)
        .build()
        .context("failed to initialize isolated Browse JSON client")?;
    Ok(MiddlewareClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build())
}

async fn load_browse_page_with_cache(
    portable: &PortablePaths,
    gamebanana_id: u64,
    _game_id: &str,
    query: Option<&str>,
    character_category_id: Option<u64>,
    page: usize,
    browse_sort: BrowseSort,
    search_sort: SearchSort,
    force_refresh: bool,
    cache_key: &str,
) -> Result<(gamebanana::ApiEnvelope<gamebanana::BrowseRecord>, Option<String>)> {
    if !force_refresh {
        if let Some(cached) = persistence::cache_get(portable, cache_key)? {
            if let Ok(payload) =
                serde_json::from_slice::<gamebanana::ApiEnvelope<gamebanana::BrowseRecord>>(&cached)
            {
                return Ok((payload, None));
            }
        }
    }

    let query = query.map(str::to_owned);
    let fetch_result = race_interactive_json(move |nocache| {
        let query = query.clone();
        async move {
            let client = isolated_browse_json_client()?;
            if let Some(category_id) = character_category_id {
                gamebanana::fetch_character_browse_page_async(
                    &client,
                    category_id,
                    query.as_deref(),
                    page,
                    browse_sort,
                    nocache,
                )
                .await
            } else {
                match query.as_deref() {
                    Some(query) if !query.trim().is_empty() => {
                        gamebanana::fetch_search_page_async(
                            &client,
                            gamebanana_id,
                            query,
                            page,
                            search_sort,
                            nocache,
                        )
                        .await
                    }
                    _ => gamebanana::fetch_browse_page_async(
                        &client,
                        gamebanana_id,
                        page,
                        browse_sort,
                        nocache,
                    )
                    .await,
                }
            }
        }
    })
    .await;

    match fetch_result {
        Ok(payload) => {
            if let Ok(bytes) = serde_json::to_vec(&payload) {
                let _ = persistence::cache_put(portable, cache_key, "browse-json", &bytes, 0);
            }
            Ok((payload, None))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = persistence::cache_get(portable, cache_key)? {
                if let Ok(payload) = serde_json::from_slice::<
                    gamebanana::ApiEnvelope<gamebanana::BrowseRecord>,
                >(&cached)
                {
                    return Ok((payload, Some(format!("{err:#}"))));
                }
            }
            Err(err)
        }
        Err(err) => Err(err),
    }
}

async fn load_character_categories_with_cache(
    portable: &PortablePaths,
    super_category_id: u64,
    force_refresh: bool,
    cache_key: &str,
) -> Result<(Vec<gamebanana::CharacterCategory>, bool)> {
    if !force_refresh {
        if let Some(cached) = persistence::cache_get(portable, cache_key)? {
            if let Ok(categories) =
                serde_json::from_slice::<Vec<gamebanana::CharacterCategory>>(&cached)
            {
                return Ok((categories, false));
            }
        }
    }

    match race_interactive_json(move |nocache| {
        async move {
            let client = isolated_browse_json_client()?;
            gamebanana::fetch_character_categories_async(&client, super_category_id, nocache).await
        }
    })
    .await
    {
        Ok(categories) => {
            if let Ok(bytes) = serde_json::to_vec(&categories) {
                let _ = persistence::cache_put(portable, cache_key, "browse-json", &bytes, 0);
            }
            Ok((categories, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = persistence::cache_get(portable, cache_key)? {
                if let Ok(categories) =
                    serde_json::from_slice::<Vec<gamebanana::CharacterCategory>>(&cached)
                {
                    return Ok((categories, true));
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
                let _ = persistence::cache_put(portable, cache_key, "browse-json", &bytes, 0);
            }
            Ok((profile, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = persistence::cache_get(portable, cache_key)? {
                if let Ok(profile) = serde_json::from_slice::<gamebanana::ProfileResponse>(&cached)
                {
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
            if let Ok(updates) =
                serde_json::from_slice::<gamebanana::ApiEnvelope<gamebanana::UpdateRecord>>(&cached)
            {
                return Ok((updates, false));
            }
        }
    }

    match gamebanana::fetch_updates_async(client, mod_id).await {
        Ok(updates) => {
            if let Ok(bytes) = serde_json::to_vec(&updates) {
                let _ = persistence::cache_put(portable, cache_key, "browse-json", &bytes, 0);
            }
            Ok((updates, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = persistence::cache_get(portable, cache_key)? {
                if let Ok(updates) = serde_json::from_slice::<
                    gamebanana::ApiEnvelope<gamebanana::UpdateRecord>,
                >(&cached)
                {
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
                    let _ =
                        persistence::cache_put(&portable, &cache_key, "browse-img", &bytes, limit);
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
                                image_thumb: load_cover_color_image_thumbnail(
                                    &bytes,
                                    thumb_profile,
                                ),
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
