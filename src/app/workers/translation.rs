pub(crate) struct TranslationRequest {
    pub mod_id: u64,
    pub lang: String,
    pub source_hash: String,
    pub force_refresh: bool,
}

pub(crate) struct TranslationEvent {
    pub mod_id: u64,
    pub lang: String,
    pub source_hash: String,
    pub result: Result<gamebanana::ProfileResponse>,
}

pub(crate) fn spawn_translation_worker(
    runtime_services: &RuntimeServices,
    _portable: &PortablePaths,
    client: ClientWithMiddleware,
    mut rx: WorkerRx<TranslationRequest>,
    tx: WorkerTx<TranslationEvent>,
) {
    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            let result = fetch_translation(
                &client,
                request.mod_id,
                &request.lang,
                &request.source_hash,
                request.force_refresh,
            )
            .await;
            let _ = tx.send(TranslationEvent {
                mod_id: request.mod_id,
                lang: request.lang,
                source_hash: request.source_hash,
                result,
            });
        }
    });
}

async fn fetch_translation(
    client: &ClientWithMiddleware,
    mod_id: u64,
    lang: &str,
    source_hash: &str,
    force_refresh: bool,
) -> Result<gamebanana::ProfileResponse> {
    // Try to load from cache first
    let cache_key = translation_cache_key(mod_id, lang, source_hash);
    let cache_path = persistence::cache_file_path(&cache_key);
    
    if !force_refresh && cache_path.exists() {
        if let Ok(json) = std::fs::read_to_string(&cache_path) {
            if let Ok(profile) = serde_json::from_str::<gamebanana::ProfileResponse>(&json) {
                return Ok(profile);
            }
        }
    }

    // Fetch from API
    let url = format!("https://thalia.hnawc.com/gamebanana/mod/{}/lang/{}", mod_id, lang);
    let response = client
        .get(&url)
        .send()
        .await
        .context("failed to fetch translation")?;
    
    let profile = response
        .error_for_status()
        .context("translation API returned an error")?
        .json::<gamebanana::ProfileResponse>()
        .await
        .context("failed to parse translation response")?;

    // Cache the result
    if let Ok(json) = serde_json::to_string(&profile) {
        let _ = std::fs::write(&cache_path, json);
        record_translation_cache_key(&cache_key);
    }

    Ok(profile)
}

fn translation_cache_key(mod_id: u64, lang: &str, source_hash: &str) -> String {
    format!("gb_profile_{mod_id}-{lang}-{source_hash}.json")
}

fn translation_cache_index_path() -> std::path::PathBuf {
    persistence::runtime_temp_cache_dir().join("translation-cache-index.json")
}

fn record_translation_cache_key(cache_key: &str) {
    let path = translation_cache_index_path();
    let mut keys = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default();
    if keys.iter().any(|key| key == cache_key) {
        return;
    }
    keys.push(cache_key.to_string());
    if let Ok(json) = serde_json::to_string(&keys) {
        let _ = std::fs::write(path, json);
    }
}
