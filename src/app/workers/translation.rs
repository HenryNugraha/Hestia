use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Deserialize, Serialize)]
struct AltchaChallenge {
    parameters: AltchaParameters,
    signature: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AltchaParameters {
    algorithm: String,
    cost: u32,
    expires_at: i64,
    key_length: u32,
    key_prefix: String,
    key_signature: String,
    memory_cost: u32,
    nonce: String,
    parallelism: u32,
    salt: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AltchaSolution {
    counter: u32,
    derived_key: String,
    time: f64,
}

#[derive(Serialize)]
struct TranslationPayload<'a> {
    text: &'a str,
    target_lang: &'a str,
    verification: AltchaVerification,
}

#[derive(Serialize)]
struct AltchaVerification {
    challenge: AltchaChallenge,
    solution: AltchaSolution,
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

    let challenge = client
        .get("https://cerberus.hnawc.com/challenge?algo=argon2id&cost=2&counter=32&memory=131072")
        .send()
        .await
        .context("failed to request Altcha challenge")?
        .error_for_status()
        .context("Altcha challenge request failed")?
        .json::<AltchaChallenge>()
        .await
        .context("failed to parse Altcha challenge")?;
    ensure_translation_not_cancelled(&cancellation)?;

    let challenge_for_solver = challenge.clone();
    let cancellation_for_solver = Arc::clone(&cancellation);
    let solution = tokio::task::spawn_blocking(move || {
        solve_altcha_challenge(&challenge_for_solver, &cancellation_for_solver)
    })
    .await
    .context("Altcha solver task failed")??;
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

fn solve_altcha_challenge(
    challenge: &AltchaChallenge,
    cancellation: &AtomicBool,
) -> Result<AltchaSolution> {
    let params = &challenge.parameters;
    if !params.algorithm.eq_ignore_ascii_case("argon2id") {
        bail!("unsupported Altcha algorithm: {}", params.algorithm);
    }
    if !(2..=8).contains(&params.cost) || params.memory_cost < 131_072 || params.parallelism == 0 {
        bail!("Altcha challenge parameters are not acceptable");
    }
    let nonce = decode_hex(&params.nonce).context("invalid Altcha nonce")?;
    let salt = decode_hex(&params.salt).context("invalid Altcha salt")?;
    let prefix = decode_hex(&params.key_prefix).context("invalid Altcha key prefix")?;
    let output_len = usize::try_from(params.key_length).context("invalid Altcha key length")?;
    let argon_params = Params::new(
        params.memory_cost,
        params.cost,
        params.parallelism,
        Some(output_len),
    )
    .map_err(|err| anyhow!("invalid Altcha Argon2 parameters: {err}"))?;
    let start = Instant::now();
    let solved = Arc::new(AtomicBool::new(false));
    let solution = Arc::new(Mutex::new(None));
    let failure = Arc::new(Mutex::new(None));

    std::thread::scope(|scope| {
        for lane in 0..ALTCHA_POW_WORKERS {
            let nonce = &nonce;
            let salt = &salt;
            let prefix = &prefix;
            let solved = Arc::clone(&solved);
            let solution = Arc::clone(&solution);
            let failure = Arc::clone(&failure);
            let argon_params = argon_params.clone();
            scope.spawn(move || {
                let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
                let mut counter = 16_u32 + lane;
                while !solved.load(Ordering::Relaxed) && !cancellation.load(Ordering::Relaxed) {
                    if start.elapsed() > Duration::from_secs(90) {
                        solved.store(true, Ordering::Relaxed);
                        break;
                    }
                    let mut password = Vec::with_capacity(nonce.len() + 4);
                    password.extend_from_slice(nonce);
                    password.extend_from_slice(&counter.to_be_bytes());
                    let mut derived_key = vec![0_u8; output_len];
                    if let Err(err) = argon2.hash_password_into(&password, salt, &mut derived_key) {
                        *failure.lock().expect("Altcha failure lock poisoned") =
                            Some(anyhow!("Altcha Argon2 derivation failed: {err}"));
                        solved.store(true, Ordering::Relaxed);
                        break;
                    }
                    if derived_key.starts_with(prefix) {
                        if !solved.swap(true, Ordering::Relaxed) {
                            *solution.lock().expect("Altcha solution lock poisoned") =
                                Some(AltchaSolution {
                                    counter,
                                    derived_key: encode_hex(&derived_key),
                                    time: start.elapsed().as_secs_f64() * 1000.0,
                                });
                        }
                        break;
                    }
                    let Some(next_counter) = counter.checked_add(ALTCHA_POW_WORKERS) else {
                        *failure.lock().expect("Altcha failure lock poisoned") =
                            Some(anyhow!("Altcha counter overflow"));
                        solved.store(true, Ordering::Relaxed);
                        break;
                    };
                    counter = next_counter;
                }
            });
        }
    });

    if let Some(solution) = solution
        .lock()
        .expect("Altcha solution lock poisoned")
        .take()
    {
        return Ok(solution);
    }
    if cancellation.load(Ordering::Relaxed) {
        bail!("translation cancelled");
    }
    if let Some(err) = failure.lock().expect("Altcha failure lock poisoned").take() {
        return Err(err);
    }
    bail!("Altcha challenge solver timed out")
}

fn ensure_translation_not_cancelled(cancellation: &AtomicBool) -> Result<()> {
    if cancellation.load(Ordering::Relaxed) {
        bail!("translation cancelled");
    }
    Ok(())
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 {
        bail!("hex value has an odd length");
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let hi = (pair[0] as char)
                .to_digit(16)
                .context("invalid hex digit")?;
            let lo = (pair[1] as char)
                .to_digit(16)
                .context("invalid hex digit")?;
            Ok(((hi << 4) | lo) as u8)
        })
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
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
