fn spawn_browse_worker(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<BrowseRequest>,
    tx: WorkerTx<BrowseEvent>,
) {
    let runtime_services = runtime_services.clone();
    let json_limiter = Arc::clone(&runtime_services.json_limiter);
    let active_page_task = Arc::new(Mutex::new(None::<tokio::task::AbortHandle>));
    runtime_services.clone().spawn(async move {
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
                    let page_proxy = runtime_services.custom_proxy();
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
                            if let Ok(Some(cached)) =
                                cache_get_blocking(page_portable.clone(), cache_key.clone()).await
                            {
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
                            &page_portable, page_proxy, gamebanana_id, &game_id,
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
                    let category_proxy = runtime_services.custom_proxy();
                    tokio::spawn(async move {
                        let cache_key = gamebanana::character_categories_cache_key(
                            &game_id,
                            super_category_id,
                        );
                        match load_character_categories_with_cache(
                            &category_portable,
                            category_proxy,
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
                    let detail_client = runtime_services.http_client();
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
                    let updates_client = runtime_services.http_client();
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
                BrowseRequest::FetchMyModUpdates { nonce, mod_id } => {
                    let updates_tx = tx.clone();
                    let updates_portable = portable.clone();
                    let updates_client = runtime_services.http_client();
                    let updates_limiter = Arc::clone(&json_limiter);
                    tokio::spawn(async move {
                        let _permit = updates_limiter.acquire().await.ok();
                        match load_my_mod_updates(&updates_portable, &updates_client, mod_id).await {
                            Ok(updates) => {
                                let _ = updates_tx.send(BrowseEvent::MyModUpdatesLoaded {
                                    _nonce: nonce,
                                    mod_id,
                                    updates,
                                });
                            }
                            Err(err) => {
                                let _ = updates_tx.send(BrowseEvent::MyModUpdatesFailed {
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
async fn race_interactive_json<T, F, Fut>(cache_bust_first: bool, fetch: F) -> Result<T>
where
    T: Send + 'static,
    F: Fn(bool) -> Fut,
    Fut: std::future::Future<Output = Result<T>> + Send + 'static,
{
    const DUPLICATE_DELAY: Duration = Duration::from_secs(2);

    let mut attempts = tokio::task::JoinSet::new();
    attempts.spawn(fetch(cache_bust_first));
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
                attempts.spawn(fetch(cache_bust_first));
                started_attempts = 2;
            }
            _ = &mut cache_busting_duplicate, if started_attempts == 2 => {
                attempts.spawn(fetch(true));
                started_attempts = 3;
            }
        }
    }
}

#[cfg(test)]
mod browse_worker_tests {
    use super::*;

    #[test]
    fn my_mod_updates_freshness_window_bounds() {
        // Fresh: from the moment it was fetched up to (but not including) the TTL.
        assert!(my_mod_updates_cache_fresh(0));
        assert!(my_mod_updates_cache_fresh(MY_MOD_UPDATES_TTL_SECS - 1));
        // Stale: at and beyond the 30-minute window -> triggers a refresh.
        assert!(!my_mod_updates_cache_fresh(MY_MOD_UPDATES_TTL_SECS));
        assert!(!my_mod_updates_cache_fresh(MY_MOD_UPDATES_TTL_SECS + 1));
        // Clock skew (future timestamp) -> treated as stale, never pinned open.
        assert!(!my_mod_updates_cache_fresh(-1));
    }

    #[tokio::test]
    async fn forced_interactive_json_race_starts_with_cache_busting_request() {
        let used_nocache =
            race_interactive_json(
                true,
                |nocache| async move { Ok::<bool, anyhow::Error>(nocache) },
            )
            .await
            .unwrap();

        assert!(used_nocache);
    }

    #[tokio::test]
    async fn normal_interactive_json_race_starts_without_cache_busting_request() {
        let used_nocache = race_interactive_json(false, |nocache| async move {
            Ok::<bool, anyhow::Error>(nocache)
        })
        .await
        .unwrap();

        assert!(!used_nocache);
    }
}

/// The stalled-request race must not queue behind the shared application's HTTP/1.1 pool.
/// Each attempt gets an isolated client and therefore its own connection pool/socket.
fn isolated_browse_json_client(
    custom_proxy: &Option<CustomProxyConfig>,
) -> Result<ClientWithMiddleware> {
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client = RuntimeServices::async_client_builder_for(custom_proxy)
        .user_agent(gamebanana::USER_AGENT)
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(0)
        .build()
        .context("failed to initialize isolated Browse JSON client")?;
    Ok(MiddlewareClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build())
}

async fn cache_get_blocking(
    portable: PortablePaths,
    cache_key: String,
) -> Result<Option<Vec<u8>>> {
    tokio::task::spawn_blocking(move || persistence::cache_get(&portable, &cache_key))
        .await
        .map_err(|err| anyhow!("cache read worker failed: {err}"))?
}

fn cache_put_blocking_detached(
    portable: &PortablePaths,
    cache_key: &str,
    cache_type: &'static str,
    bytes: Vec<u8>,
    max_bytes: u64,
) {
    let portable = portable.clone();
    let cache_key = cache_key.to_string();
    std::mem::drop(tokio::task::spawn_blocking(move || {
        let _ = persistence::cache_put(&portable, &cache_key, cache_type, &bytes, max_bytes);
    }));
}

async fn load_browse_page_with_cache(
    portable: &PortablePaths,
    custom_proxy: Option<CustomProxyConfig>,
    gamebanana_id: u64,
    _game_id: &str,
    query: Option<&str>,
    character_category_id: Option<u64>,
    page: usize,
    browse_sort: BrowseSort,
    search_sort: SearchSort,
    force_refresh: bool,
    cache_key: &str,
) -> Result<(
    gamebanana::ApiEnvelope<gamebanana::BrowseRecord>,
    Option<String>,
)> {
    if !force_refresh {
        if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await? {
            if let Ok(payload) =
                serde_json::from_slice::<gamebanana::ApiEnvelope<gamebanana::BrowseRecord>>(&cached)
            {
                return Ok((payload, None));
            }
        }
    }

    let query = query.map(str::to_owned);
    let fetch_result = race_interactive_json(force_refresh, move |nocache| {
        let query = query.clone();
        let custom_proxy = custom_proxy.clone();
        async move {
            let client = isolated_browse_json_client(&custom_proxy)?;
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
                    _ => {
                        gamebanana::fetch_browse_page_async(
                            &client,
                            gamebanana_id,
                            page,
                            browse_sort,
                            nocache,
                        )
                        .await
                    }
                }
            }
        }
    })
    .await;

    match fetch_result {
        Ok(payload) => {
            if let Ok(bytes) = serde_json::to_vec(&payload) {
                cache_put_blocking_detached(portable, cache_key, "browse-json", bytes, 0);
            }
            Ok((payload, None))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await?
            {
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
    custom_proxy: Option<CustomProxyConfig>,
    super_category_id: u64,
    force_refresh: bool,
    cache_key: &str,
) -> Result<(Vec<gamebanana::CharacterCategory>, bool)> {
    if !force_refresh {
        if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await? {
            if let Ok(categories) =
                serde_json::from_slice::<Vec<gamebanana::CharacterCategory>>(&cached)
            {
                return Ok((categories, false));
            }
        }
    }

    match race_interactive_json(force_refresh, move |nocache| {
        let custom_proxy = custom_proxy.clone();
        async move {
            let client = isolated_browse_json_client(&custom_proxy)?;
            gamebanana::fetch_character_categories_async(&client, super_category_id, nocache).await
        }
    })
    .await
    {
        Ok(categories) => {
            if let Ok(bytes) = serde_json::to_vec(&categories) {
                cache_put_blocking_detached(portable, cache_key, "browse-json", bytes, 0);
            }
            Ok((categories, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await?
            {
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
        if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await? {
            if let Ok(profile) = serde_json::from_slice::<gamebanana::ProfileResponse>(&cached) {
                return Ok((profile, false));
            }
        }
    }

    match gamebanana::fetch_profile_async(client, mod_id).await {
        Ok(profile) => {
            if let Ok(bytes) = serde_json::to_vec(&profile) {
                cache_put_blocking_detached(portable, cache_key, "browse-json", bytes, 0);
            }
            Ok((profile, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await?
            {
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
        if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await? {
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
                cache_put_blocking_detached(portable, cache_key, "browse-json", bytes, 0);
            }
            Ok((updates, false))
        }
        Err(err) if force_refresh => {
            if let Some(cached) = cache_get_blocking(portable.clone(), cache_key.to_string()).await?
            {
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

/// MY MODS freshness window: within this many seconds of the last successful fetch,
/// the cached update log is served without touching the network.
const MY_MOD_UPDATES_TTL_SECS: i64 = 30 * 60;

/// Whether a MY MODS update-log cache fetched `age_secs` ago is still fresh. A negative
/// age (a future `fetched_at` from clock skew) is treated as stale, so a bad clock can't
/// pin a cache open indefinitely.
fn my_mod_updates_cache_fresh(age_secs: i64) -> bool {
    (0..MY_MOD_UPDATES_TTL_SECS).contains(&age_secs)
}

/// Load a MY MODS update log with a persisted 30-minute freshness window.
///
/// A cache newer than [`MY_MOD_UPDATES_TTL_SECS`] is returned as-is (no network). A
/// stale or missing cache triggers a fresh fetch; on success the timestamped envelope
/// is rewritten, and on a fetch failure the last cached copy is returned as a
/// transparent fallback — only a failure with no cache at all surfaces as an error.
async fn load_my_mod_updates(
    portable: &PortablePaths,
    client: &ClientWithMiddleware,
    mod_id: u64,
) -> Result<gamebanana::ApiEnvelope<gamebanana::UpdateRecord>> {
    let cache_key = gamebanana::my_mod_updates_cache_key(mod_id);
    let cached = match cache_get_blocking(portable.clone(), cache_key.clone()).await? {
        Some(bytes) => serde_json::from_slice::<gamebanana::CachedModUpdates>(&bytes).ok(),
        None => None,
    };

    if let Some(envelope) = &cached {
        let age = chrono::Utc::now().timestamp() - envelope.fetched_at;
        if my_mod_updates_cache_fresh(age) {
            return Ok(envelope.payload.clone());
        }
    }

    match gamebanana::fetch_updates_async(client, mod_id).await {
        Ok(payload) => {
            let envelope = gamebanana::CachedModUpdates {
                fetched_at: chrono::Utc::now().timestamp(),
                payload: payload.clone(),
            };
            if let Ok(bytes) = serde_json::to_vec(&envelope) {
                cache_put_blocking_detached(portable, &cache_key, "browse-json", bytes, 0);
            }
            Ok(payload)
        }
        Err(err) => match cached {
            Some(envelope) => Ok(envelope.payload),
            None => Err(err),
        },
    }
}

fn spawn_browse_image_workers(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    cache_limit_bytes: Arc<std::sync::atomic::AtomicU64>,
    mut rx: WorkerRx<BrowseImageRequest>,
    tx: WorkerTx<BrowseImageResult>,
) {
    let runtime_services = runtime_services.clone();
    let handle = runtime_services.handle();
    let full_limiter = Arc::clone(&runtime_services.full_image_limiter);
    let thumb_limiter = Arc::clone(&runtime_services.thumb_image_limiter);
    let full_decode_limiter = Arc::clone(&runtime_services.full_decode_limiter);
    runtime_services.clone().spawn(async move {
        while let Some(request) = rx.recv().await {
            let client = runtime_services.http_client();
            let portable = portable.clone();
            let tx = tx.clone();
            let handle = handle.clone();
            let cache_limit_bytes = Arc::clone(&cache_limit_bytes);
            let full_limiter = Arc::clone(&full_limiter);
            let thumb_limiter = Arc::clone(&thumb_limiter);
            let full_decode_limiter = Arc::clone(&full_decode_limiter);
            tokio::spawn(async move {
                if request.cancel.load(Ordering::Relaxed) {
                    return;
                }

                let url = request.url.clone();
                let cache_key = request.cache_key.clone();
                let limit = cache_limit_bytes.load(Ordering::Relaxed);
                let _image_permit = if request.load_full {
                    full_limiter.acquire().await.ok()
                } else {
                    thumb_limiter.acquire().await.ok()
                };
                let bytes_result = async {
                    let portable_for_get = portable.clone();
                    let cache_key_for_get = cache_key.clone();
                    match handle
                        .spawn_blocking(move || {
                            persistence::cache_get(&portable_for_get, &cache_key_for_get)
                        })
                        .await
                    {
                        Ok(Ok(Some(cached))) => {
                            return Ok::<(Vec<u8>, bool), anyhow::Error>((cached, true));
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(err)) => return Err(err),
                        Err(err) => return Err(anyhow!("image cache read worker failed: {err}")),
                    }
                    let bytes = client
                        .get(&url)
                        .send()
                        .await?
                        .error_for_status()?
                        .bytes()
                        .await?
                        .to_vec();
                    Ok((bytes, false))
                };
                let bytes_result = tokio::select! {
                    _ = async {
                        while !request.cancel.load(Ordering::Relaxed) {
                            tokio::time::sleep(Duration::from_millis(25)).await;
                        }
                    } => return,
                    result = bytes_result => result,
                };

                match bytes_result {
                    Ok((bytes, from_cache)) => {
                        if request.cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let thumb_profile = request.thumb_profile;
                        let load_full = request.load_full;
                        let skip_texture = request.skip_texture;
                        let texture_key = request.texture_key.clone();
                        let thumb_texture_key = request.thumb_texture_key.clone();
                        let cancel_key = request.cancel_key;
                        let _decode_permit = if load_full {
                            full_decode_limiter.acquire().await.ok()
                        } else {
                            None
                        };
                        let decode_result = handle
                            .spawn_blocking(move || {
                                decode_browse_image_result(
                                    &bytes,
                                    load_full,
                                    skip_texture,
                                    thumb_profile,
                                )
                                .map(|decoded| (bytes, decoded))
                            })
                            .await;

                        match decode_result {
                            Ok(Ok((bytes, decoded))) => {
                                if request.cancel.load(Ordering::Relaxed) {
                                    return;
                                }
                                if !from_cache {
                                    let portable_for_put = portable.clone();
                                    let cache_key_for_put = cache_key.clone();
                                    let bytes_for_cache = bytes.clone();
                                    std::mem::drop(handle.spawn_blocking(move || {
                                        let _ = persistence::cache_put(
                                            &portable_for_put,
                                            &cache_key_for_put,
                                            "browse-img",
                                            &bytes_for_cache,
                                            limit,
                                        );
                                    }));
                                }
                                let _ = tx.send(BrowseImageResult {
                                    texture_key,
                                    thumb_texture_key,
                                    image_full: decoded.image_full,
                                    image_thumb: decoded.image_thumb,
                                    cancel_key,
                                    failure: None,
                                });
                            }
                            Ok(Err(err)) => {
                                if request.cancel.load(Ordering::Relaxed) {
                                    return;
                                }
                                if from_cache {
                                    let portable_for_remove = portable.clone();
                                    let cache_key_for_remove = cache_key.clone();
                                    std::mem::drop(handle.spawn_blocking(move || {
                                        let _ = persistence::cache_remove(
                                            &portable_for_remove,
                                            &cache_key_for_remove,
                                        );
                                    }));
                                }
                                let _ = tx.send(BrowseImageResult {
                                    texture_key,
                                    thumb_texture_key,
                                    image_full: None,
                                    image_thumb: None,
                                    cancel_key,
                                    failure: Some(BrowseImageFailure {
                                        url,
                                        timed_out: false,
                                        error: format!("{err:#}"),
                                    }),
                                });
                            }
                            Err(err) => {
                                if request.cancel.load(Ordering::Relaxed) {
                                    return;
                                }
                                let _ = tx.send(BrowseImageResult {
                                    texture_key,
                                    thumb_texture_key,
                                    image_full: None,
                                    image_thumb: None,
                                    cancel_key,
                                    failure: Some(BrowseImageFailure {
                                        url,
                                        timed_out: false,
                                        error: format!("image decode worker failed: {err}"),
                                    }),
                                });
                            }
                        }
                    }
                    Err(err) => {
                        if request.cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = tx.send(BrowseImageResult {
                            texture_key: request.texture_key,
                            thumb_texture_key: request.thumb_texture_key,
                            image_full: None,
                            image_thumb: None,
                            cancel_key: request.cancel_key,
                            failure: Some(BrowseImageFailure {
                                url,
                                timed_out: is_timeout_error(&err),
                                error: format!("{err:#}"),
                            }),
                        });
                    }
                }
            });
        }
    });
}

struct DecodedBrowseImage {
    image_full: Option<egui::ColorImage>,
    image_thumb: Option<egui::ColorImage>,
}

fn decode_browse_image_result(
    bytes: &[u8],
    load_full: bool,
    skip_texture: bool,
    thumb_profile: ThumbnailProfile,
) -> Result<DecodedBrowseImage> {
    if skip_texture {
        decode_limited_dynamic_image(bytes)?;
        return Ok(DecodedBrowseImage {
            image_full: None,
            image_thumb: None,
        });
    }

    let image_full = if load_full {
        let image = load_cover_color_image(bytes)
            .ok_or_else(|| anyhow!("failed to decode full image"))?;
        Some(image)
    } else {
        None
    };
    let image_thumb = load_cover_color_image_thumbnail(bytes, thumb_profile)
        .ok_or_else(|| anyhow!("failed to decode thumbnail image"))?;
    Ok(DecodedBrowseImage {
        image_full,
        image_thumb: Some(image_thumb),
    })
}
