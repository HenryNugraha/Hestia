use sha2::{Digest, Sha256};
use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

impl HestiaApp {
    fn request_app_update_check(&mut self, now: f64) {
        if self.app_update_button_state == AppUpdateButtonState::Checking {
            return;
        }
        if self.app_update_verified_path.is_some() || self.app_update_download_active() {
            self.app_update_button_state = AppUpdateButtonState::UpdateAvailable;
            return;
        }

        self.app_update_button_state = AppUpdateButtonState::Checking;
        self.app_update_button_spin_until = now + 1.5;
        let tx = self.app_update_event_tx.clone();
        let client = self.runtime_services.http_client.clone();
        self.runtime_services.spawn(async move {
            let result = async {
                let manifest = fetch_app_update_manifest(&client).await?;
                if !is_manifest_newer(&manifest.version)? {
                    return Ok::<AppUpdateEvent, anyhow::Error>(AppUpdateEvent::UpToDate);
                }

                cleanup_old_app_update_dirs(&manifest.version);
                let update_path = app_update_exe_path(&manifest.version);
                let verified_path = if verify_app_update_file(&update_path, &manifest).is_ok() {
                    Some(update_path)
                } else {
                    None
                };
                Ok(AppUpdateEvent::CheckDone {
                    manifest,
                    verified_path,
                })
            }
            .await;
            let event = match result {
                Ok(event) => event,
                Err(err) => AppUpdateEvent::CheckFailed {
                    error: format!("{err:#}"),
                },
            };
            let _ = tx.send(event);
        });
    }

    fn request_automatic_app_update_check(&mut self, now: f64) {
        if self.state.automatically_check_for_update {
            self.request_app_update_check(now);
        }
    }

    fn consume_app_update_events(&mut self) {
        while let Ok(event) = self.app_update_event_rx.try_recv() {
            match event {
                AppUpdateEvent::UpToDate => {
                    self.app_update_manifest = None;
                    self.app_update_verified_path = None;
                    self.app_update_button_state = AppUpdateButtonState::UpToDate;
                }
                AppUpdateEvent::CheckDone {
                    manifest,
                    verified_path,
                } => {
                    self.app_update_manifest = Some(manifest.clone());
                    self.app_update_button_state = AppUpdateButtonState::UpdateAvailable;
                    if let Some(path) = verified_path {
                        self.stage_verified_app_update(&manifest, path.clone());
                        self.app_update_verified_path = Some(path);
                    } else if !self.app_update_download_active() {
                        self.queue_app_update_download(manifest);
                    }
                }
                AppUpdateEvent::CheckFailed { error } => {
                    self.app_update_manifest = None;
                    self.app_update_verified_path = None;
                    self.app_update_button_state = AppUpdateButtonState::Failed;
                    self.log_warn(format!("app update check failed: {error}"));
                }
                AppUpdateEvent::DownloadDone {
                    task_id,
                    manifest,
                    path,
                    bytes,
                } => {
                    self.app_update_download_inflight = None;
                    self.app_update_task_id = Some(task_id);
                    self.stage_verified_app_update(&manifest, path.clone());
                    self.app_update_manifest = Some(manifest);
                    self.app_update_verified_path = Some(path);
                    if let Some(task) = self.state.tasks.iter_mut().find(|task| task.id == task_id) {
                        task.total_size = Some(bytes);
                    }
                    self.update_task_status(task_id, TaskStatus::Completed);
                    self.app_update_button_state = AppUpdateButtonState::UpdateAvailable;
                    self.set_message_ok("Update ready");
                }
                AppUpdateEvent::DownloadFailed { task_id, error } => {
                    self.app_update_download_inflight = None;
                    self.app_update_task_id = None;
                    self.app_update_verified_path = None;
                    self.update_task_status(task_id, TaskStatus::Failed);
                    self.app_update_button_state = AppUpdateButtonState::Failed;
                    self.report_error_message(
                        format!("app update download failed: {error}"),
                        Some("Update failed"),
                    );
                }
                AppUpdateEvent::DownloadCanceled { task_id } => {
                    self.app_update_download_inflight = None;
                    self.app_update_task_id = None;
                    self.app_update_verified_path = None;
                    self.update_task_status(task_id, TaskStatus::Canceled);
                    self.app_update_button_state = AppUpdateButtonState::Check;
                    self.set_message_ok("Update download canceled");
                }
            }
        }
    }

    fn queue_app_update_download(&mut self, manifest: AppUpdateManifest) {
        if self.app_update_download_active() {
            self.app_update_button_state = AppUpdateButtonState::UpdateAvailable;
            return;
        }
        let destination = app_update_exe_path(&manifest.version);
        if verify_app_update_file(&destination, &manifest).is_ok() {
            self.stage_verified_app_update(&manifest, destination.clone());
            self.app_update_manifest = Some(manifest);
            self.app_update_verified_path = Some(destination);
            self.app_update_button_state = AppUpdateButtonState::UpdateAvailable;
            return;
        }

        let task_id = self.next_background_job_id();
        let title = format!("Hestia {}", manifest.version);
        self.add_task(
            task_id,
            TaskKind::Download,
            TaskStatus::Queued,
            title,
            None,
            Some(manifest.bytes),
            false,
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(RwLock::new(DownloadProgress {
            downloaded: 0,
            total: Some(manifest.bytes),
            speed: 0.0,
            last_update: Instant::now(),
            bytes_since_last: 0,
        }));
        self.app_update_download_inflight = Some(AppUpdateDownloadInflight {
            task_id,
            destination,
            manifest,
            cancel,
            progress,
        });
        self.app_update_task_id = Some(task_id);
        self.app_update_button_state = AppUpdateButtonState::UpdateAvailable;
    }

    fn process_app_update_download(&mut self) {
        let Some(inflight) = self.app_update_download_inflight.clone() else {
            return;
        };
        if self
            .state
            .tasks
            .iter()
            .find(|task| task.id == inflight.task_id)
            .is_some_and(|task| task.status == TaskStatus::Downloading)
        {
            return;
        }

        self.update_task_status(inflight.task_id, TaskStatus::Downloading);
        let tx = self.app_update_event_tx.clone();
        let client = self.runtime_services.http_client.clone();
        self.runtime_services.spawn(async move {
            let result = download_app_update_file(&client, &inflight).await;
            let event = match result {
                Ok(bytes) => AppUpdateEvent::DownloadDone {
                    task_id: inflight.task_id,
                    manifest: inflight.manifest,
                    path: inflight.destination,
                    bytes,
                },
                Err(err) if err.to_string() == importing::CANCELLED_ERROR => {
                    AppUpdateEvent::DownloadCanceled {
                        task_id: inflight.task_id,
                    }
                }
                Err(err) => AppUpdateEvent::DownloadFailed {
                    task_id: inflight.task_id,
                    error: format!("{err:#}"),
                },
            };
            let _ = tx.send(event);
        });
    }

    fn app_update_download_active(&self) -> bool {
        self.app_update_download_inflight.is_some()
            || self.app_update_task_id.is_some_and(|task_id| {
                self.state.tasks.iter().any(|task| {
                    task.id == task_id
                        && matches!(
                            task.status,
                            TaskStatus::Queued | TaskStatus::Downloading | TaskStatus::Canceling
                        )
                })
            })
    }

    fn app_update_task_progress(&self, task_id: u64) -> Option<Arc<RwLock<DownloadProgress>>> {
        self.app_update_download_inflight
            .as_ref()
            .filter(|inflight| inflight.task_id == task_id)
            .map(|inflight| Arc::clone(&inflight.progress))
    }

    fn is_app_update_task(&self, task: &TaskEntry) -> bool {
        self.app_update_task_id == Some(task.id) || task.title.starts_with("Hestia ")
    }

    fn cancel_app_update_task(&mut self, job_id: u64) -> bool {
        let Some(inflight) = self.app_update_download_inflight.as_ref() else {
            return false;
        };
        if inflight.task_id != job_id {
            return false;
        }
        inflight.cancel.store(true, Ordering::Relaxed);
        self.update_task_status(job_id, TaskStatus::Canceling);
        true
    }

    fn app_update_button_label(&self, now: f64) -> &'static str {
        if self.app_update_verified_path.is_some() {
            return "Restart to Update";
        }
        if self.app_update_button_state == AppUpdateButtonState::Checking
            || now < self.app_update_button_spin_until
        {
            return "Checking...";
        }
        match self.app_update_button_state {
            AppUpdateButtonState::Check | AppUpdateButtonState::Checking => "Check for Update",
            AppUpdateButtonState::UpToDate => "Up to Date",
            AppUpdateButtonState::Failed => "Failed to Check",
            AppUpdateButtonState::UpdateAvailable => "Update Available",
        }
    }

    fn app_update_button_enabled(&self, now: f64) -> bool {
        if self.app_update_verified_path.is_some() {
            return true;
        }
        if self.app_update_button_state == AppUpdateButtonState::Checking
            || now < self.app_update_button_spin_until
        {
            return false;
        }
        !self.app_update_download_active()
    }

    fn restart_to_update(&mut self) {
        let Some(path) = self.app_update_verified_path.clone() else {
            return;
        };
        if self.has_active_mod_tasks() {
            self.report_warn(
                "update restart blocked while tasks are active",
                Some("Wait for active tasks before updating"),
            );
            return;
        }
        match self_replace::self_replace(&path) {
            Ok(()) => {
                clear_staged_app_update_folder(&path);
                self.state.staged_app_update = None;
                self.save_state();
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe)
                        .arg("--after-update")
                        .spawn();
                }
                std::process::exit(0);
            }
            Err(err) => self.report_error_message(
                format!("failed to apply app update: {err:#}"),
                Some("Could not apply update"),
            ),
        }
    }

    fn has_active_mod_tasks(&self) -> bool {
        self.state.tasks.iter().any(|task| {
            task.id != self.app_update_task_id.unwrap_or(u64::MAX)
                && matches!(
                    task.status,
                    TaskStatus::Queued
                        | TaskStatus::Installing
                        | TaskStatus::Downloading
                        | TaskStatus::Canceling
                )
        })
    }

    fn stage_verified_app_update(&mut self, manifest: &AppUpdateManifest, path: PathBuf) {
        self.state.staged_app_update = Some(StagedAppUpdate {
            version: manifest.version.clone(),
            path,
            bytes: manifest.bytes,
            sha256: manifest.sha256.clone(),
        });
        self.save_state();
    }
}

pub(crate) fn apply_staged_app_update_before_gui(
    portable: &PortablePaths,
    state: &mut AppState,
) -> anyhow::Result<bool> {
    let Some(staged) = state.staged_app_update.clone() else {
        return Ok(false);
    };
    let should_apply = semver::Version::parse(&staged.version)
        .ok()
        .zip(semver::Version::parse(env!("CARGO_PKG_VERSION")).ok())
        .is_some_and(|(staged_version, current_version)| staged_version > current_version);
    if !should_apply {
        clear_staged_app_update_folder(&staged.path);
        state.staged_app_update = None;
        persistence::save_app_state(portable, state)?;
        return Ok(false);
    }

    if verify_staged_app_update_file(&staged).is_err() {
        clear_staged_app_update_folder(&staged.path);
        state.staged_app_update = None;
        persistence::save_app_state(portable, state)?;
        return Ok(false);
    }

    match self_replace::self_replace(&staged.path) {
        Ok(()) => {
            clear_staged_app_update_folder(&staged.path);
            state.staged_app_update = None;
            let _ = persistence::save_app_state(portable, state);
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe)
                    .arg("--after-update")
                    .spawn();
            }
            Ok(true)
        }
        Err(err) => {
            clear_staged_app_update_folder(&staged.path);
            state.staged_app_update = None;
            persistence::save_app_state(portable, state)?;
            Err(err).context("failed to apply staged app update")
        }
    }
}

async fn fetch_app_update_manifest(
    client: &ClientWithMiddleware,
) -> anyhow::Result<AppUpdateManifest> {
    let mut errors = Vec::new();
    for url in crate::UPDATE_MANIFEST_URL
        .iter()
        .map(|url| url.trim())
        .filter(|url| !url.is_empty())
    {
        match fetch_manifest_from_url(client, url).await {
            Ok(manifest) => return Ok(manifest),
            Err(err) => {
                errors.push(format!("{url}: {err:#}"));
            }
        }
    }

    if errors.is_empty() {
        bail!("no update manifest URLs are configured");
    }

    bail!("all update manifest URLs failed:\n{}", errors.join("\n"))
}

async fn fetch_manifest_from_url(
    client: &ClientWithMiddleware,
    url: &str,
) -> anyhow::Result<AppUpdateManifest> {
    if url.trim().is_empty() {
        bail!("manifest URL is empty");
    }
    let bytes = if let Some(path) = file_url_to_path(url) {
        tokio::fs::read(&path)
            .await
            .with_context(|| format!("failed to read manifest {}", path.display()))?
    } else {
        client
            .get(url.to_string())
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec()
    };
    let manifest: AppUpdateManifest =
        serde_json::from_slice(&bytes).context("failed to parse update manifest")?;
    verify_app_update_manifest_signature(&manifest)?;
    Ok(manifest)
}

fn verify_app_update_manifest_signature(manifest: &AppUpdateManifest) -> anyhow::Result<()> {
    let public_key = crate::UPDATE_MANIFEST_PUBLIC_KEY_BASE64.trim();
    if public_key.is_empty() {
        bail!("update manifest public key is not configured");
    }

    let public_key_bytes = BASE64
        .decode(public_key)
        .context("invalid update manifest public key encoding")?;
    let public_key_bytes: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| anyhow!("update manifest public key must be 32 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .context("invalid update manifest public key")?;

    let signature_bytes = BASE64
        .decode(manifest.signature.trim())
        .context("invalid update manifest signature encoding")?;
    let signature = Signature::from_slice(&signature_bytes)
        .context("invalid update manifest signature")?;
    let canonical = serde_json::to_vec(&AppUpdateManifestPayload::from(manifest))
        .context("failed to serialize update manifest payload")?;

    verifying_key
        .verify(&canonical, &signature)
        .context("update manifest signature verification failed")
}

fn is_manifest_newer(version: &str) -> anyhow::Result<bool> {
    let remote = semver::Version::parse(version)
        .with_context(|| format!("invalid manifest version {version}"))?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .context("current app version is not semver-compatible")?;
    Ok(remote > current)
}

fn app_update_exe_path(version: &str) -> PathBuf {
    std::env::temp_dir()
        .join("update")
        .join(sanitize_folder_name(version))
        .join("hestia.exe")
}

async fn download_app_update_file(
    client: &ClientWithMiddleware,
    inflight: &AppUpdateDownloadInflight,
) -> anyhow::Result<u64> {
    if let Some(parent) = inflight.destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = download_update_bytes(
        client,
        &inflight.manifest.download,
        &inflight.cancel,
        Arc::clone(&inflight.progress),
    )
    .await?;
    let byte_size = bytes.len() as u64;
    let partial = inflight.destination.with_extension("exe.partial");
    tokio::fs::write(&partial, bytes)
        .await
        .with_context(|| format!("failed to write {}", partial.display()))?;
    if tokio::fs::rename(&partial, &inflight.destination).await.is_err() {
        let _ = tokio::fs::remove_file(&inflight.destination).await;
        tokio::fs::rename(&partial, &inflight.destination)
            .await
            .with_context(|| {
                format!(
                    "failed to move update into place: {}",
                    inflight.destination.display()
                )
            })?;
    }
    if let Err(err) = verify_app_update_file(&inflight.destination, &inflight.manifest) {
        let _ = tokio::fs::remove_file(&inflight.destination).await;
        return Err(err);
    }
    Ok(byte_size)
}

async fn download_update_bytes(
    client: &ClientWithMiddleware,
    downloads: &[String],
    cancel: &Arc<AtomicBool>,
    progress: Arc<RwLock<DownloadProgress>>,
) -> anyhow::Result<Vec<u8>> {
    let links = downloads
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty());
    let mut last_error: Option<anyhow::Error> = None;

    for url in links {
        match download_update_bytes_from_url(client, url, cancel, Arc::clone(&progress)).await {
            Ok(bytes) => return Ok(bytes),
            Err(err) if err.to_string() == importing::CANCELLED_ERROR => return Err(err),
            Err(err) => {
                last_error = Some(err.context(format!("update download failed: {url}")));
            }
        }
    }

    match last_error {
        Some(err) => Err(err),
        None => bail!("update manifest has no download links"),
    }
}

async fn download_update_bytes_from_url(
    client: &ClientWithMiddleware,
    url: &str,
    cancel: &Arc<AtomicBool>,
    progress: Arc<RwLock<DownloadProgress>>,
) -> anyhow::Result<Vec<u8>> {
    if let Some(path) = file_url_to_path(url) {
        if cancel.load(Ordering::Relaxed) {
            bail!(importing::CANCELLED_ERROR);
        }
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("failed to read update file {}", path.display()))?;
        if let Ok(mut guard) = progress.write() {
            guard.total = Some(bytes.len() as u64);
            guard.downloaded = bytes.len() as u64;
        }
        return Ok(bytes);
    }
    download_to_bytes_async(client, url, cancel, progress).await
}

fn verify_app_update_file(path: &Path, manifest: &AppUpdateManifest) -> anyhow::Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to read {}", path.display()))?;
    if !metadata.is_file() {
        bail!("update path is not a file: {}", path.display());
    }
    if metadata.len() != manifest.bytes {
        bail!(
            "update size mismatch: expected {}, got {}",
            manifest.bytes,
            metadata.len()
        );
    }
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(manifest.sha256.trim()) {
        bail!("update hash mismatch");
    }
    Ok(())
}

fn verify_staged_app_update_file(staged: &StagedAppUpdate) -> anyhow::Result<()> {
    let metadata =
        fs::metadata(&staged.path).with_context(|| format!("failed to read {}", staged.path.display()))?;
    if !metadata.is_file() {
        bail!("staged update path is not a file: {}", staged.path.display());
    }
    if metadata.len() != staged.bytes {
        bail!(
            "staged update size mismatch: expected {}, got {}",
            staged.bytes,
            metadata.len()
        );
    }
    let actual = sha256_file(&staged.path)?;
    if !actual.eq_ignore_ascii_case(staged.sha256.trim()) {
        bail!("staged update hash mismatch");
    }
    Ok(())
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let raw = url
        .strip_prefix("file://")
        .or_else(|| url.strip_prefix("file:\\\\"))
        .or_else(|| url.strip_prefix("file:\\"))?;
    let trimmed = raw.trim_start_matches('/');
    if trimmed.len() >= 2 && trimmed.as_bytes().get(1) == Some(&b':') {
        Some(PathBuf::from(trimmed.replace('/', "\\")))
    } else {
        Some(PathBuf::from(raw.replace('/', "\\")))
    }
}

fn cleanup_old_app_update_dirs(current_version: &str) {
    let root = std::env::temp_dir().join("update");
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let keep = sanitize_folder_name(current_version);
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name != keep {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn clear_staged_app_update_folder(path: &Path) {
    let Some(parent) = path.parent() else {
        let _ = fs::remove_file(path);
        return;
    };
    let root = std::env::temp_dir().join("update");
    if parent.starts_with(&root) {
        let _ = fs::remove_dir_all(parent);
    } else {
        let _ = fs::remove_file(path);
    }
}
