use serde::{Deserialize, Serialize};

use crate::integrations::thalia::{
    AltchaVerification, fetch_altcha_challenge, solve_altcha_challenge_foreground,
};

pub(crate) enum TranslationRequest {
    GameBanana {
        request_id: u64,
        mod_id: u64,
        lang: String,
        source_hash: String,
        force_refresh: bool,
    },
    UnlinkedText {
        request_id: u64,
        cancellation: Arc<AtomicBool>,
        mod_entry_id: String,
        lang: String,
        content: String,
        content_hash: String,
        force_refresh: bool,
    },
}

pub(crate) enum TranslationEvent {
    GameBanana {
        request_id: u64,
        mod_id: u64,
        lang: String,
        source_hash: String,
        result: Result<gamebanana::ProfileResponse>,
    },
    UnlinkedText {
        request_id: u64,
        mod_entry_id: String,
        lang: String,
        content_hash: String,
        result: Result<String>,
    },
}

#[derive(Serialize)]
struct TranslationPayload<'a> {
    text: &'a str,
    target_lang: &'a str,
    verification: AltchaVerification,
}

#[derive(Deserialize)]
struct TextTranslationResponse {
    translation: String,
}

#[derive(Serialize, Deserialize)]
struct CachedUnlinkedTranslation {
    language: String,
    content_hash: String,
    translation: String,
}

pub(crate) fn spawn_translation_worker(
    runtime_services: &RuntimeServices,
    _portable: &PortablePaths,
    mut rx: WorkerRx<TranslationRequest>,
    tx: WorkerTx<TranslationEvent>,
) {
    let runtime_services = runtime_services.clone();
    runtime_services.clone().spawn(async move {
        while let Some(request) = rx.recv().await {
            let client = runtime_services.http_client();
            let direct_client = runtime_services
                .async_client_builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("translation HTTP client configuration must be valid");
            let event = match request {
                TranslationRequest::GameBanana {
                    request_id,
                    mod_id,
                    lang,
                    source_hash,
                    force_refresh,
                } => TranslationEvent::GameBanana {
                    result: fetch_translation(&client, mod_id, &lang, &source_hash, force_refresh)
                        .await,
                    mod_id,
                    request_id,
                    lang,
                    source_hash,
                },
                TranslationRequest::UnlinkedText {
                    request_id,
                    cancellation,
                    mod_entry_id,
                    lang,
                    content,
                    content_hash,
                    force_refresh,
                } => TranslationEvent::UnlinkedText {
                    result: fetch_unlinked_text_translation(
                        &direct_client,
                        &content,
                        &lang,
                        &content_hash,
                        force_refresh,
                        cancellation,
                    )
                    .await,
                    mod_entry_id,
                    request_id,
                    lang,
                    content_hash,
                },
            };
            let _ = tx.send(event);
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
    let cache_key = translation_cache_key(mod_id, lang, source_hash);
    let cache_path = persistence::cache_file_path(&cache_key);
    if !force_refresh && cache_path.exists() {
        if let Ok(json) = std::fs::read_to_string(&cache_path) {
            if let Ok(profile) = serde_json::from_str::<gamebanana::ProfileResponse>(&json) {
                return Ok(profile);
            }
        }
    }

    let url = format!("https://thalia.hnawc.com/gamebanana/mod/{mod_id}/lang/{lang}");
    let profile = client
        .get(&url)
        .send()
        .await
        .context("failed to fetch translation")?
        .error_for_status()
        .context("translation API returned an error")?
        .json::<gamebanana::ProfileResponse>()
        .await
        .context("failed to parse translation response")?;

    if let Ok(json) = serde_json::to_string(&profile) {
        let _ = std::fs::write(&cache_path, json);
        record_translation_cache_key(&cache_key);
    }
    Ok(profile)
}

async fn fetch_unlinked_text_translation(
    client: &reqwest::Client,
    content: &str,
    lang: &str,
    content_hash: &str,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
) -> Result<String> {
    ensure_translation_not_cancelled(&cancellation)?;
    if content.chars().count() > 12_000 {
        bail!("text exceeds the translation API limit of 12,000 characters");
    }

    let cache_key = unlinked_text_cache_key(lang, content_hash);
    let cache_path = persistence::cache_file_path(&cache_key);
    if !force_refresh {
        if let Some(translation) = cached_unlinked_text_translation(lang, content_hash) {
            return Ok(translation);
        }
    }

    let challenge = fetch_altcha_challenge(client)
        .await
        .context("failed to obtain Altcha challenge")?;
    ensure_translation_not_cancelled(&cancellation)?;

    let challenge_for_solver = challenge.clone();
    let cancellation_for_solver = Arc::clone(&cancellation);
    let solution = solve_altcha_challenge_foreground(challenge_for_solver, cancellation_for_solver)
        .await
        .context("Altcha solver task failed")?;
    ensure_translation_not_cancelled(&cancellation)?;
    let response = client
        .post("https://thalia.hnawc.com/translate")
        .json(&TranslationPayload {
            text: content,
            target_lang: lang,
            verification: AltchaVerification {
                challenge,
                solution,
            },
        })
        .send()
        .await
        .context("failed to request text translation")?
        .error_for_status()
        .context("text translation API returned an error")?
        .json::<TextTranslationResponse>()
        .await
        .context("failed to parse text translation response")?;

    let cached = CachedUnlinkedTranslation {
        language: lang.to_string(),
        content_hash: content_hash.to_string(),
        translation: response.translation.clone(),
    };
    if let Ok(raw) = serde_json::to_string(&cached) {
        let _ = std::fs::write(&cache_path, raw);
        record_translation_cache_key(&cache_key);
    }
    Ok(response.translation)
}

fn ensure_translation_not_cancelled(cancellation: &AtomicBool) -> Result<()> {
    if cancellation.load(Ordering::Relaxed) {
        bail!("translation cancelled");
    }
    Ok(())
}

fn translation_cache_key(mod_id: u64, lang: &str, source_hash: &str) -> String {
    format!("gb_profile_{mod_id}-{lang}-{source_hash}.json")
}

pub(crate) fn unlinked_text_cache_key(lang: &str, content_hash: &str) -> String {
    format!("unlinked_text-v1-{lang}-{content_hash}.json")
}

pub(crate) fn cached_unlinked_text_translation(lang: &str, content_hash: &str) -> Option<String> {
    let cache_key = unlinked_text_cache_key(lang, content_hash);
    let cache_path = persistence::cache_file_path(&cache_key);
    let raw = std::fs::read_to_string(cache_path).ok()?;
    let cached = serde_json::from_str::<CachedUnlinkedTranslation>(&raw).ok()?;
    (cached.language == lang && cached.content_hash == content_hash).then_some(cached.translation)
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
