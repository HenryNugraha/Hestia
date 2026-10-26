//! Strict Thalia update-matching transport and shared Altcha proof helpers.

use std::{
    collections::HashSet,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use argon2::{Algorithm, Argon2, Params, Version};
use futures_util::StreamExt;
use reqwest::{
    Response, StatusCode,
    header::{CACHE_CONTROL, RETRY_AFTER},
};
use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, IgnoredAny, MapAccess, Visitor},
};
use tokio::sync::Semaphore;
use tokio::time::timeout;

/// Failed or unavailable service responses leave local matching untouched.
pub(crate) const THALIA_UPDATE_MATCHING_ENABLED: bool = true;
const THALIA_UPDATE_MATCH_BASE_URL: &str = "https://thalia.hnawc.com";

const THALIA_AUTH_URL: &str = "https://thalia.hnawc.com/auth";
const UPDATE_MATCH_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const TEST_UPDATE_MATCH_TIMEOUT: Duration = Duration::from_millis(200);
const RESPONSE_BODY_LIMIT: usize = 4 * 1024;
const CHALLENGE_BODY_LIMIT: usize = 64 * 1024;
const JSON_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;
const ALTCHA_POW_WORKERS: u32 = 4;

struct AltchaProofGate {
    semaphore: Arc<Semaphore>,
}

impl AltchaProofGate {
    fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }
}

static ALTCHA_PROOF_GATE: std::sync::OnceLock<Arc<AltchaProofGate>> = std::sync::OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateMatchRequest {
    pub(crate) mod_id: u64,
    pub(crate) installed_file_id: u64,
}

impl UpdateMatchRequest {
    fn validate(&self) -> Result<(), UpdateMatchError> {
        if self.mod_id == 0 || self.mod_id > JSON_SAFE_INTEGER_MAX {
            return Err(UpdateMatchError::InvalidRequest(
                "mod_id must be a positive JSON-safe integer",
            ));
        }
        if self.installed_file_id == 0 || self.installed_file_id > JSON_SAFE_INTEGER_MAX {
            return Err(UpdateMatchError::InvalidRequest(
                "installed_file_id must be a positive JSON-safe integer",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateMatchAction {
    Update,
    Merge,
    Current,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UpdateMatchResponse {
    pub(crate) action: UpdateMatchAction,
    pub(crate) id: Option<u64>,
}

impl UpdateMatchResponse {
    fn validate(&self) -> Result<(), String> {
        match self.action {
            UpdateMatchAction::Update | UpdateMatchAction::Merge => {
                let Some(id) = self.id else {
                    return Err("update and merge require a positive id".to_string());
                };
                if id == 0 || id > JSON_SAFE_INTEGER_MAX {
                    return Err("action id must be a positive JSON-safe integer".to_string());
                }
            }
            UpdateMatchAction::Current | UpdateMatchAction::Uncertain => {
                if self.id.is_some() {
                    return Err("current and uncertain require a null id".to_string());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum UpdateMatchError {
    Cancelled,
    InvalidRequest(&'static str),
    Timeout,
    BodyTooLarge,
    InvalidResponse(String),
    Http {
        status: u16,
        retry_after: Option<Duration>,
    },
    Transport(String),
}

impl UpdateMatchError {
    pub(crate) fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Http {
                status,
                retry_after,
            } => retry_after.or_else(|| match *status {
                429 | 503 => Some(Duration::from_secs(30)),
                _ => None,
            }),
            _ => None,
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl fmt::Display for UpdateMatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("update matching cancelled"),
            Self::InvalidRequest(reason) => write!(f, "invalid update matching request: {reason}"),
            Self::Timeout => f.write_str("Thalia update matching timed out"),
            Self::BodyTooLarge => f.write_str("Thalia response exceeded the size limit"),
            Self::InvalidResponse(reason) => write!(f, "invalid Thalia response: {reason}"),
            Self::Http { status, .. } => write!(f, "Thalia returned HTTP {status}"),
            Self::Transport(reason) => write!(f, "Thalia transport failed: {reason}"),
        }
    }
}

impl std::error::Error for UpdateMatchError {}

/// Fetch an update classification. The application worker owns scheduling;
/// this function performs one direct, unauthenticated request.
pub(crate) async fn fetch_update_match(
    client: &reqwest::Client,
    request: UpdateMatchRequest,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
) -> Result<UpdateMatchResponse, UpdateMatchError> {
    request.validate()?;
    ensure_not_cancelled(&cancellation)?;
    let endpoint = update_match_url(THALIA_UPDATE_MATCH_BASE_URL, &request);
    fetch_update_match_at_endpoint(client, force_refresh, cancellation, &endpoint).await
}

async fn fetch_update_match_at_endpoint(
    client: &reqwest::Client,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
    endpoint: &str,
) -> Result<UpdateMatchResponse, UpdateMatchError> {
    fetch_update_match_at_endpoint_with_timeout(
        client,
        force_refresh,
        cancellation,
        endpoint,
        UPDATE_MATCH_TIMEOUT,
    )
    .await
}

async fn fetch_update_match_at_endpoint_with_timeout(
    client: &reqwest::Client,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
    endpoint: &str,
    request_timeout: Duration,
) -> Result<UpdateMatchResponse, UpdateMatchError> {
    let timeout_cancellation = Arc::clone(&cancellation);
    let result = timeout(
        request_timeout,
        fetch_update_match_inner(client, force_refresh, cancellation, endpoint),
    )
    .await;
    match result {
        Ok(result) => result,
        Err(_) => {
            timeout_cancellation.store(true, Ordering::Relaxed);
            Err(UpdateMatchError::Timeout)
        }
    }
}

#[cfg(test)]
async fn fetch_update_match_for_test(
    client: &reqwest::Client,
    request: UpdateMatchRequest,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
    endpoint: &str,
) -> Result<UpdateMatchResponse, UpdateMatchError> {
    fetch_update_match_for_test_with_timeout(
        client,
        request,
        force_refresh,
        cancellation,
        endpoint,
        UPDATE_MATCH_TIMEOUT,
    )
    .await
}

#[cfg(test)]
async fn fetch_update_match_for_test_with_timeout(
    client: &reqwest::Client,
    request: UpdateMatchRequest,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
    endpoint: &str,
    request_timeout: Duration,
) -> Result<UpdateMatchResponse, UpdateMatchError> {
    request.validate()?;
    ensure_not_cancelled(&cancellation)?;
    let endpoint = update_match_url(endpoint, &request);
    fetch_update_match_at_endpoint_with_timeout(
        client,
        force_refresh,
        cancellation,
        &endpoint,
        request_timeout,
    )
    .await
}

fn update_match_url(base_url: &str, request: &UpdateMatchRequest) -> String {
    format!(
        "{}/gamebanana/update/{}/from/{}",
        base_url.trim_end_matches('/'),
        request.mod_id,
        request.installed_file_id,
    )
}

async fn fetch_update_match_inner(
    client: &reqwest::Client,
    force_refresh: bool,
    cancellation: Arc<AtomicBool>,
    endpoint: &str,
) -> Result<UpdateMatchResponse, UpdateMatchError> {
    ensure_not_cancelled(&cancellation)?;
    let (status, retry_after, body) =
        get_update_match(client, endpoint, force_refresh, &cancellation).await?;
    if status == StatusCode::OK {
        ensure_not_cancelled(&cancellation)?;
        return parse_update_match_response(&body);
    }
    Err(UpdateMatchError::Http {
        status: status.as_u16(),
        retry_after,
    })
}

async fn get_update_match(
    client: &reqwest::Client,
    endpoint: &str,
    force_refresh: bool,
    cancellation: &AtomicBool,
) -> Result<(StatusCode, Option<Duration>, Vec<u8>), UpdateMatchError> {
    let mut builder = client.get(endpoint);
    if force_refresh {
        builder = builder.header(CACHE_CONTROL, "no-cache");
    }
    ensure_not_cancelled_atomic(cancellation)?;
    let request = async move {
        let response = builder
            .send()
            .await
            .map_err(|err| UpdateMatchError::Transport(err.to_string()))?;
        let status = response.status();
        let retry_after = parse_retry_after(response.headers().get(RETRY_AFTER));
        let body = read_limited_body(response, RESPONSE_BODY_LIMIT, cancellation).await?;
        Ok((status, retry_after, body))
    };
    tokio::select! {
        result = request => result,
        _ = wait_for_cancellation(cancellation) => Err(UpdateMatchError::Cancelled),
    }
}

async fn read_limited_body(
    response: Response,
    limit: usize,
    cancellation: &AtomicBool,
) -> Result<Vec<u8>, UpdateMatchError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(UpdateMatchError::BodyTooLarge);
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        ensure_not_cancelled_atomic(cancellation)?;
        let chunk = chunk.map_err(|err| UpdateMatchError::Transport(err.to_string()))?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(UpdateMatchError::BodyTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    ensure_not_cancelled_atomic(cancellation)?;
    Ok(body)
}

fn parse_retry_after(value: Option<&reqwest::header::HeaderValue>) -> Option<Duration> {
    let value = value?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds.min(15 * 60)));
    }
    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let seconds = (date.with_timezone(&chrono::Utc) - chrono::Utc::now())
        .num_seconds()
        .max(0) as u64;
    Some(Duration::from_secs(seconds.min(15 * 60)))
}

fn proof_gate() -> Arc<AltchaProofGate> {
    ALTCHA_PROOF_GATE
        .get_or_init(|| Arc::new(AltchaProofGate::new()))
        .clone()
}

/// Foreground translation keeps the existing solver semantics.
pub(crate) async fn solve_altcha_challenge_foreground(
    challenge: AltchaChallenge,
    cancellation: Arc<AtomicBool>,
) -> Result<AltchaSolution> {
    ensure_not_cancelled_anyhow(&cancellation)?;
    let gate = proof_gate();
    let permit = tokio::select! {
        result = gate.semaphore.clone().acquire_owned() => result.context("Altcha proof gate closed")?,
        _ = wait_for_cancellation(&cancellation) => bail!("translation cancelled"),
    };
    let solver_cancellation = Arc::clone(&cancellation);
    let solver = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        solve_altcha_challenge(&challenge, &solver_cancellation)
    });
    tokio::select! {
        result = solver => match result {
            Ok(result) => result,
            Err(err) => Err(anyhow!("Altcha solver task failed: {err}")),
        },
        _ = wait_for_cancellation(&cancellation) => Err(anyhow!("translation cancelled")),
    }
}

pub(crate) async fn fetch_altcha_challenge(client: &reqwest::Client) -> Result<AltchaChallenge> {
    let response = client
        .get(THALIA_AUTH_URL)
        .send()
        .await
        .context("failed to request Altcha challenge")?;
    if !response.status().is_success() {
        bail!(
            "Altcha challenge request returned HTTP {}",
            response.status()
        );
    }
    let body = read_limited_body_anyhow(response, CHALLENGE_BODY_LIMIT).await?;
    serde_json::from_slice(&body).context("failed to parse Altcha challenge")
}

async fn read_limited_body_anyhow(response: Response, limit: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        bail!("response exceeded the size limit");
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed to read response body")?;
        if body.len().saturating_add(chunk.len()) > limit {
            bail!("response exceeded the size limit");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Existing Altcha challenge shape shared by the translation and matcher
/// requests.  Keep the wire casing unchanged.
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct AltchaChallenge {
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
pub(crate) struct AltchaSolution {
    counter: u32,
    derived_key: String,
    time: f64,
}

#[derive(Serialize)]
pub(crate) struct AltchaVerification {
    pub(crate) challenge: AltchaChallenge,
    pub(crate) solution: AltchaSolution,
}

pub(crate) fn solve_altcha_challenge(
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
    let start = std::time::Instant::now();
    let solved = Arc::new(AtomicBool::new(false));
    let solution = Arc::new(std::sync::Mutex::new(None));
    let failure = Arc::new(std::sync::Mutex::new(None));

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

fn ensure_not_cancelled(cancellation: &AtomicBool) -> Result<(), UpdateMatchError> {
    ensure_not_cancelled_atomic(cancellation)
}

fn ensure_not_cancelled_atomic(cancellation: &AtomicBool) -> Result<(), UpdateMatchError> {
    if cancellation.load(Ordering::Relaxed) {
        return Err(UpdateMatchError::Cancelled);
    }
    Ok(())
}

fn ensure_not_cancelled_anyhow(cancellation: &AtomicBool) -> Result<()> {
    if cancellation.load(Ordering::Relaxed) {
        bail!("translation cancelled");
    }
    Ok(())
}

async fn wait_for_cancellation(cancellation: &AtomicBool) {
    while !cancellation.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn parse_update_match_response(body: &[u8]) -> Result<UpdateMatchResponse, UpdateMatchError> {
    let mut deserializer = serde_json::Deserializer::from_slice(body);
    let response = deserializer
        .deserialize_map(StrictResponseVisitor)
        .map_err(|err| UpdateMatchError::InvalidResponse(err.to_string()))?;
    deserializer
        .end()
        .map_err(|err| UpdateMatchError::InvalidResponse(err.to_string()))?;
    response
        .validate()
        .map_err(UpdateMatchError::InvalidResponse)?;
    Ok(response)
}

struct StrictResponseVisitor;

impl<'de> Visitor<'de> for StrictResponseVisitor {
    type Value = UpdateMatchResponse;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an object with exactly action and id fields")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        let mut action = None;
        let mut id = None;
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom("duplicate response key"));
            }
            match key.as_str() {
                "action" => action = Some(map.next_value::<String>()?),
                "id" => id = Some(map.next_value::<Option<u64>>()?),
                _ => {
                    let _ = map.next_value::<IgnoredAny>()?;
                    return Err(de::Error::custom("unknown response key"));
                }
            }
        }
        let action = action.ok_or_else(|| de::Error::custom("missing action"))?;
        let id = id.ok_or_else(|| de::Error::custom("missing id"))?;
        let action = match action.as_str() {
            "update" => UpdateMatchAction::Update,
            "merge" => UpdateMatchAction::Merge,
            "current" => UpdateMatchAction::Current,
            "uncertain" => UpdateMatchAction::Uncertain,
            _ => return Err(de::Error::custom("unknown action")),
        };
        Ok(UpdateMatchResponse { action, id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{ErrorKind, Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
        time::Instant,
    };

    #[test]
    fn response_requires_exact_shape_and_rejects_duplicate_or_unknown_keys() {
        let good = parse_update_match_response(br#"{"action":"update","id":123}"#).unwrap();
        assert_eq!(good.action, UpdateMatchAction::Update);
        assert_eq!(good.id, Some(123));
        for body in [
            br#"{"action":"current","id":null}"#.as_slice(),
            br#"{"action":"uncertain","id":null}"#.as_slice(),
            br#"{"action":"merge","id":123}"#.as_slice(),
            br#"{"id":123,"action":"update"}"#.as_slice(),
        ] {
            assert!(parse_update_match_response(body).is_ok(), "{body:?}");
        }

        for body in [
            br#"{"action":"update"}"#.as_slice(),
            br#"{"action":"update","id":123,"extra":null}"#.as_slice(),
            br#"{"action":"update","id":123,"id":124}"#.as_slice(),
            br#"{"action":"update","id":"123"}"#.as_slice(),
            br#"{"action":"update","id":1.5}"#.as_slice(),
            br#"{"action":"update","id":true}"#.as_slice(),
            br#"{"action":"update","id":9007199254740992}"#.as_slice(),
            br#"{"action":"update","id":-1}"#.as_slice(),
            br#"{"action":"current","id":1}"#.as_slice(),
            br#"{"action":"separate","id":null}"#.as_slice(),
            br#"{"action":"update","id":NaN}"#.as_slice(),
            br#"{"action":"update","id":Infinity}"#.as_slice(),
            br#"{"action":"update","id":123} {"action":"merge","id":124}"#.as_slice(),
            br#"[{"action":"update","id":123}]"#.as_slice(),
            br#"// note
            {"action":"update","id":123}"#
                .as_slice(),
            br#"```json
            {"action":"update","id":123}
            ```"#
                .as_slice(),
            br#"{"action":"uncertain","id":null} trailing"#.as_slice(),
            br#"<!-- html -->"#.as_slice(),
        ] {
            assert!(parse_update_match_response(body).is_err(), "{body:?}");
        }
    }

    #[test]
    fn request_rejects_non_json_safe_ids() {
        assert!(
            UpdateMatchRequest {
                mod_id: 0,
                installed_file_id: 1,
            }
            .validate()
            .is_err()
        );
        assert!(
            UpdateMatchRequest {
                mod_id: JSON_SAFE_INTEGER_MAX + 1,
                installed_file_id: 1,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_dates() {
        let seconds = reqwest::header::HeaderValue::from_static("45");
        assert_eq!(
            parse_retry_after(Some(&seconds)),
            Some(Duration::from_secs(45))
        );
        let date = (chrono::Utc::now() + chrono::Duration::seconds(45)).to_rfc2822();
        let date = reqwest::header::HeaderValue::from_str(&date).unwrap();
        let parsed = parse_retry_after(Some(&date)).unwrap();
        assert!(parsed <= Duration::from_secs(45));
        assert!(parsed >= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn foreground_solver_gate_is_available() {
        let gate = Arc::new(AltchaProofGate::new());
        let permit = gate.semaphore.clone().try_acquire_owned();
        assert!(permit.is_ok());
        drop(permit);
        assert!(gate.semaphore.clone().try_acquire_owned().is_ok());
    }

    fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        let mut body_start = None;
        let mut content_length = 0_usize;
        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
            if body_start.is_none() {
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let end = index + 4;
                    body_start = Some(end);
                    let headers = String::from_utf8_lossy(&request[..index]);
                    content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                }
            }
            if body_start.is_some_and(|start| request.len() >= start + content_length) {
                break;
            }
        }
        request
    }

    fn accept_with_timeout(listener: &TcpListener) -> TcpStream {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    return stream;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "stub did not receive a request");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("stub accept failed: {error}"),
            }
        }
    }

    fn write_http_response(stream: &mut TcpStream, status: &str, body: &[u8]) {
        write_http_response_with_headers(stream, status, &[], body);
    }

    fn write_http_response_with_headers(
        stream: &mut TcpStream,
        status: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) {
        let extra_headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body),
        );
        stream.write_all(response.as_bytes()).unwrap();
    }

    #[tokio::test]
    async fn successful_stub_request_is_exact_and_does_not_send_proof() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut stream = accept_with_timeout(&listener);
            let request = read_http_request(&mut stream);
            write_http_response(
                &mut stream,
                "200 OK",
                br#"{"action":"update","id":987654302}"#,
            );
            sender.send(request).unwrap();
        });

        let client = reqwest::Client::new();
        let cancellation = Arc::new(AtomicBool::new(false));
        let result = fetch_update_match_for_test(
            &client,
            UpdateMatchRequest {
                mod_id: 728699,
                installed_file_id: 987654301,
            },
            true,
            cancellation,
            &format!("http://{address}"),
        )
        .await
        .unwrap();
        assert_eq!(result.action, UpdateMatchAction::Update);
        assert_eq!(result.id, Some(987654302));

        server.join().unwrap();
        let request = String::from_utf8(receiver.recv().unwrap()).unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with("GET /gamebanana/update/728699/from/987654301 HTTP/1.1\r\n"));
        assert!(!request_lower.contains("content-type: application/json"));
        assert!(request_lower.contains("cache-control: no-cache"));
        assert!(!request_lower.contains("x-altcha-verification"));
        assert!(request.ends_with("\r\n\r\n"));
    }

    #[tokio::test]
    async fn oversized_response_is_rejected_before_parsing() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut stream = accept_with_timeout(&listener);
            let _ = read_http_request(&mut stream);
            write_http_response(&mut stream, "200 OK", &vec![b'x'; RESPONSE_BODY_LIMIT + 1]);
        });
        let result = fetch_update_match_for_test(
            &reqwest::Client::new(),
            UpdateMatchRequest {
                mod_id: 1,
                installed_file_id: 2,
            },
            false,
            Arc::new(AtomicBool::new(false)),
            &format!("http://{address}"),
        )
        .await;
        assert!(matches!(result, Err(UpdateMatchError::BodyTooLarge)));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_inflight_request() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let _stream = accept_with_timeout(&listener);
            thread::sleep(Duration::from_millis(300));
        });
        let cancellation = Arc::new(AtomicBool::new(false));
        let cancellation_for_task = Arc::clone(&cancellation);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancellation_for_task.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let result = fetch_update_match_for_test(
            &reqwest::Client::new(),
            UpdateMatchRequest {
                mod_id: 1,
                installed_file_id: 2,
            },
            false,
            cancellation,
            &format!("http://{address}"),
        )
        .await;
        assert!(matches!(result, Err(UpdateMatchError::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn get_timeout_is_bounded() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let _stream = accept_with_timeout(&listener);
            thread::sleep(Duration::from_millis(350));
        });
        let started = Instant::now();
        let result = fetch_update_match_for_test_with_timeout(
            &reqwest::Client::new(),
            UpdateMatchRequest {
                mod_id: 1,
                installed_file_id: 2,
            },
            false,
            Arc::new(AtomicBool::new(false)),
            &format!("http://{address}"),
            TEST_UPDATE_MATCH_TIMEOUT,
        )
        .await;
        assert!(matches!(result, Err(UpdateMatchError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn deadline_also_bounds_a_stalled_response_body() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait_for_release) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut stream = accept_with_timeout(&listener);
            let _ = read_http_request(&mut stream);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 32\r\nConnection: close\r\n\r\n{").unwrap();
            stream.flush().unwrap();
            let _ = wait_for_release.recv_timeout(Duration::from_secs(2));
        });
        let cancellation = Arc::new(AtomicBool::new(false));
        let result = fetch_update_match_for_test_with_timeout(
            &reqwest::Client::new(),
            UpdateMatchRequest {
                mod_id: 1,
                installed_file_id: 2,
            },
            false,
            Arc::clone(&cancellation),
            &format!("http://{address}"),
            TEST_UPDATE_MATCH_TIMEOUT,
        )
        .await;
        let _ = release.send(());
        server.join().unwrap();
        assert!(matches!(result, Err(UpdateMatchError::Timeout)));
        assert!(cancellation.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn rate_limit_returns_status_and_retry_after_without_retrying() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut stream = accept_with_timeout(&listener);
            let _ = read_http_request(&mut stream);
            write_http_response_with_headers(
                &mut stream,
                "429 Too Many Requests",
                &[("Retry-After", "45")],
                br#"{"action":"uncertain","id":null}"#,
            );
        });
        let result = fetch_update_match_for_test(
            &reqwest::Client::new(),
            UpdateMatchRequest {
                mod_id: 1,
                installed_file_id: 2,
            },
            false,
            Arc::new(AtomicBool::new(false)),
            &format!("http://{address}"),
        )
        .await;
        match result {
            Err(UpdateMatchError::Http {
                status,
                retry_after,
            }) => {
                assert_eq!(status, 429);
                assert_eq!(retry_after, Some(Duration::from_secs(45)));
            }
            other => panic!("expected bounded rate-limit error, got {other:?}"),
        }
        server.join().unwrap();
    }

    #[tokio::test]
    async fn auth_status_is_returned_without_auth_request_or_retry() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut stream = accept_with_timeout(&listener);
            sender.send(read_http_request(&mut stream)).unwrap();
            write_http_response(
                &mut stream,
                "428 Precondition Required",
                br#"{"action":"uncertain","id":null}"#,
            );
        });

        let client = reqwest::Client::new();
        let result = fetch_update_match_for_test(
            &client,
            UpdateMatchRequest {
                mod_id: 728709,
                installed_file_id: 987654301,
            },
            false,
            Arc::new(AtomicBool::new(false)),
            &format!("http://{address}"),
        )
        .await;
        assert!(matches!(
            result,
            Err(UpdateMatchError::Http {
                status: 428,
                retry_after: None
            })
        ));

        server.join().unwrap();
        let request = String::from_utf8(receiver.recv().unwrap()).unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(!request_lower.contains("x-altcha-verification"));
    }

    #[tokio::test]
    async fn unauthorized_status_is_returned_without_auth_request_or_retry() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut stream = accept_with_timeout(&listener);
            sender.send(read_http_request(&mut stream)).unwrap();
            write_http_response(
                &mut stream,
                "401 Unauthorized",
                br#"{"action":"uncertain","id":null}"#,
            );
        });

        let result = fetch_update_match_for_test(
            &reqwest::Client::new(),
            UpdateMatchRequest {
                mod_id: 728709,
                installed_file_id: 987654301,
            },
            false,
            Arc::new(AtomicBool::new(false)),
            &format!("http://{address}"),
        )
        .await;
        assert!(matches!(
            result,
            Err(UpdateMatchError::Http {
                status: 401,
                retry_after: None
            })
        ));

        server.join().unwrap();
        let request = String::from_utf8(receiver.recv().unwrap()).unwrap();
        assert!(
            !request
                .to_ascii_lowercase()
                .contains("x-altcha-verification")
        );
    }
}
