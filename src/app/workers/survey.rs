fn spawn_feedback_survey_submit_worker(
    runtime_services: &RuntimeServices,
    mut rx: WorkerRx<FeedbackSurveySubmitRequest>,
    tx: WorkerTx<FeedbackSurveySubmitEvent>,
) {
    let runtime_services = runtime_services.clone();
    runtime_services.clone().spawn(async move {
        while let Some(request) = rx.recv().await {
            let client = match runtime_services
                .async_client_builder()
                .user_agent(gamebanana::USER_AGENT)
                .timeout(Duration::from_secs(15))
                .build()
            {
                Ok(client) => client,
                Err(err) => {
                    let _ = tx.send(FeedbackSurveySubmitEvent::Failed {
                        version: request.version,
                        pending_path: request.pending_path,
                        error: format!("failed to create survey HTTP client: {err}"),
                        discard_on_failure: request.discard_on_failure,
                    });
                    continue;
                }
            };
            let result = tokio::select! {
                _ = async {
                    while !request.cancel.load(Ordering::Relaxed) {
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                } => {
                    let _ = tx.send(FeedbackSurveySubmitEvent::Canceled);
                    continue;
                }
                result = post_feedback_survey_with_retries(&client, &request.payload_json) => result,
            };
            match result {
                Ok(()) => {
                    let _ = tx.send(FeedbackSurveySubmitEvent::Submitted {
                        version: request.version,
                        pending_path: request.pending_path,
                    });
                }
                Err(err) => {
                    let _ = tx.send(FeedbackSurveySubmitEvent::Failed {
                        version: request.version,
                        pending_path: request.pending_path,
                        error: format!("{err:#}"),
                        discard_on_failure: request.discard_on_failure,
                    });
                }
            }
        }
    });
}

async fn post_feedback_survey_with_retries(
    client: &reqwest::Client,
    payload_json: &str,
) -> Result<()> {
    const ATTEMPTS: usize = 3;

    let mut last_error = None;
    for attempt in 1..=ATTEMPTS {
        match post_feedback_survey_once(client, payload_json).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
                if attempt < ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("feedback survey submission failed")))
}

async fn post_feedback_survey_once(client: &reqwest::Client, payload_json: &str) -> Result<()> {
    let url = FEEDBACK_SURVEY_SERVER_URL.trim();
    if url.is_empty() {
        bail!("feedback survey server URL is not configured");
    }

    let response = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload_json.to_string())
        .send()
        .await
        .map_err(|err| anyhow!("request failed: {err}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| anyhow!("failed to read response body: {err}"))?;
    if !status.is_success() {
        bail!(
            "server returned {status}: {}",
            body.chars().take(240).collect::<String>()
        );
    }

    let payload: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| anyhow!("invalid response JSON: {err}"))?;
    if payload
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let error = payload
        .get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("server rejected survey response");
    bail!("{error}");
}
