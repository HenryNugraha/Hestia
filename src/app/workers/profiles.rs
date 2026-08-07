fn spawn_profile_worker(
    runtime_services: &RuntimeServices,
    mut rx: WorkerRx<ProfileRequest>,
    tx: WorkerTx<ProfileEvent>,
    archive_tx: WorkerTx<ProfileArchiveJob>,
    archive_coordinator: Arc<ProfileArchiveCoordinator>,
) {
    let handle = runtime_services.handle();
    runtime_services.spawn(async move {
        while let Some(ProfileRequest::Execute(spec)) = rx.recv().await {
            let tx = tx.clone();
            let archive_tx = archive_tx.clone();
            let operation_id = spec.operation_id;
            let game_id = spec.game_id.clone();
            let kind = spec.kind;
            let cancel = Arc::clone(&spec.cancel);
            let coordinator = Arc::clone(&archive_coordinator);
            let result = handle
                .spawn_blocking(move || execute_profile_operation(spec, coordinator))
                .await;
            match result {
                Ok(Ok(output)) => {
                    let background_archives = output.background_archives.clone();
                    let _ = tx.send(ProfileEvent::Completed {
                        operation_id,
                        game_id,
                        kind: output.kind,
                        profile_id: output.profile_id,
                        target_profile_id: output.target_profile_id,
                        display_name: output.display_name,
                        archive: output.archive,
                        active_profile_marker: output.active_profile_marker,
                        warnings: output.warnings,
                    });
                    for job in background_archives {
                        let _ = tx.send(ProfileEvent::ArchiveQueued {
                            game_id: job.game_id.clone(),
                            profile_id: job.profile_id,
                            delay_seconds: PROFILE_ARCHIVE_GRACE_PERIOD.as_secs(),
                        });
                        let _ = archive_tx.send(job);
                    }
                }
                Ok(Err(ProfileWorkerError::Canceled)) if cancel.load(Ordering::Relaxed) => {
                    let _ = tx.send(ProfileEvent::Canceled { operation_id });
                }
                Ok(Err(err)) if cancel.load(Ordering::Relaxed) => {
                    let _ = tx.send(ProfileEvent::Canceled { operation_id });
                    let _ = err;
                }
                Ok(Err(err)) => {
                    let _ = tx.send(ProfileEvent::Failed {
                        operation_id,
                        game_id,
                        error: err.to_string(),
                        recovery_blocking: err.recovery_blocking(),
                    });
                }
                Err(err) => {
                    let _ = tx.send(ProfileEvent::Failed {
                        operation_id,
                        game_id,
                        error: format!("profile worker join failed: {err}"),
                        recovery_blocking: kind == ProfileOperationKind::Recover,
                    });
                }
            }
        }
    });
}

fn spawn_profile_archive_worker(
    runtime_services: &RuntimeServices,
    mut rx: WorkerRx<ProfileArchiveJob>,
    tx: WorkerTx<ProfileEvent>,
    coordinator: Arc<ProfileArchiveCoordinator>,
) {
    let handle = runtime_services.handle();
    runtime_services.spawn(async move {
        while let Some(job) = rx.recv().await {
            let start_logged = Arc::new(AtomicBool::new(false));
            loop {
                let blocking_job = job.clone();
                let blocking_coordinator = Arc::clone(&coordinator);
                let start_logged = Arc::clone(&start_logged);
                let started_tx = tx.clone();
                let started_game_id = job.game_id.clone();
                let started_profile_id = job.profile_id;
                let result = handle
                    .spawn_blocking(move || {
                        let mut on_started = || {
                            if !start_logged.swap(true, Ordering::Relaxed) {
                                let _ = started_tx.send(ProfileEvent::ArchiveStarted {
                                    game_id: started_game_id.clone(),
                                    profile_id: started_profile_id,
                                });
                            }
                        };
                        execute_profile_archive_job(
                            &blocking_job,
                            &blocking_coordinator,
                            &mut on_started,
                        )
                    })
                    .await;
                match result {
                    Ok(Ok(ProfileArchiveJobOutcome::Paused)) => continue,
                    Ok(Ok(ProfileArchiveJobOutcome::Missing)) => {
                        let _ = tx.send(ProfileEvent::ArchiveSkipped {
                            game_id: job.game_id.clone(),
                            profile_id: job.profile_id,
                        });
                        break;
                    }
                    Ok(Ok(ProfileArchiveJobOutcome::Active)) => {
                        let _ = tx.send(ProfileEvent::ArchiveCanceled {
                            game_id: job.game_id.clone(),
                            profile_id: job.profile_id,
                        });
                        break;
                    }
                    Ok(Ok(ProfileArchiveJobOutcome::Completed(archive))) => {
                        let _ = tx.send(ProfileEvent::ArchiveCompleted {
                            game_id: job.game_id.clone(),
                            profile_id: job.profile_id,
                            archive,
                        });
                        break;
                    }
                    Ok(Err(error)) => {
                        let _ = tx.send(ProfileEvent::ArchiveFailed {
                            game_id: job.game_id.clone(),
                            profile_id: job.profile_id,
                            error: error.to_string(),
                        });
                        break;
                    }
                    Err(error) => {
                        let _ = tx.send(ProfileEvent::ArchiveFailed {
                            game_id: job.game_id.clone(),
                            profile_id: job.profile_id,
                            error: format!("profile archive worker join failed: {error}"),
                        });
                        break;
                    }
                }
            }
        }
    });
}

struct ProfileWorkerOutput {
    kind: ProfileOperationKind,
    profile_id: Option<Uuid>,
    target_profile_id: Option<Uuid>,
    display_name: Option<String>,
    archive: Option<crate::integrations::profiles::ArchiveResult>,
    active_profile_marker: Option<ActiveProfileMarker>,
    background_archives: Vec<ProfileArchiveJob>,
    warnings: Vec<String>,
}

enum ProfileArchiveJobOutcome {
    Paused,
    Missing,
    Active,
    Completed(crate::integrations::profiles::ArchiveResult),
}

const ACTIVE_PROFILE_MARKER_FILE: &str = "active_profile.json";
const PROFILE_ARCHIVE_GRACE_PERIOD: Duration = Duration::from_secs(3);
const PROFILE_SWITCH_FINISH_DELAY: Duration = Duration::from_millis(1_300);
const PROFILE_SWITCH_FINISH_PROGRESS_START: u64 = 70;
const PROFILE_SWITCH_FINISH_PROGRESS_STEPS: u32 = 25;

#[derive(Debug)]
enum ProfileWorkerError {
    Canceled,
    Other(anyhow::Error),
    RecoveryBlocked(anyhow::Error),
}

impl std::fmt::Display for ProfileWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canceled => f.write_str("profile operation canceled"),
            Self::Other(err) | Self::RecoveryBlocked(err) => err.fmt(f),
        }
    }
}

impl ProfileWorkerError {
    fn into_recovery_blocking(self) -> Self {
        match self {
            Self::Other(error) => Self::RecoveryBlocked(error),
            other => other,
        }
    }

    fn recovery_blocking(&self) -> bool {
        matches!(self, Self::RecoveryBlocked(_))
    }
}

impl From<anyhow::Error> for ProfileWorkerError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

impl From<std::io::Error> for ProfileWorkerError {
    fn from(value: std::io::Error) -> Self {
        Self::Other(value.into())
    }
}

struct ProfileStagingCleanup {
    staging_dir: PathBuf,
    extracting_dir: PathBuf,
}

impl ProfileStagingCleanup {
    fn new(roots: &crate::integrations::profiles::ProfileRoots, operation_id: u64) -> Self {
        let staging_dir = roots.staging_dir();
        Self {
            extracting_dir: staging_dir.join(format!("{operation_id:016x}.extracting")),
            staging_dir,
        }
    }
}

impl Drop for ProfileStagingCleanup {
    fn drop(&mut self) {
        // Extraction staging is disposable on every terminal outcome. Journals are preserved
        // when recovery is still required, so removing the root succeeds only when work is done.
        let _ = std::fs::remove_dir_all(&self.extracting_dir);
        let _ = std::fs::remove_dir(&self.staging_dir);
    }
}

impl ProfileArchiveCoordinator {
    fn pause_for_foreground(self: &Arc<Self>) -> ProfileArchiveForegroundGuard {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.foreground_active = true;
        self.changed.notify_all();
        while state.archive_running {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        ProfileArchiveForegroundGuard {
            coordinator: Arc::clone(self),
        }
    }

    fn begin_archive(self: &Arc<Self>) -> ProfileArchiveRunningGuard {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            while state.foreground_active {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if let Some(not_before) = state.archive_not_before
                && let Some(remaining) = not_before.checked_duration_since(Instant::now())
            {
                let (next_state, _) = self
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state = next_state;
                continue;
            }
            break;
        }
        state.archive_running = true;
        ProfileArchiveRunningGuard {
            coordinator: Arc::clone(self),
        }
    }

    fn foreground_requested(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .foreground_active
    }
}

struct ProfileArchiveForegroundGuard {
    coordinator: Arc<ProfileArchiveCoordinator>,
}

impl Drop for ProfileArchiveForegroundGuard {
    fn drop(&mut self) {
        let mut state = self
            .coordinator
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.foreground_active = false;
        state.archive_not_before = Some(Instant::now() + PROFILE_ARCHIVE_GRACE_PERIOD);
        self.coordinator.changed.notify_all();
    }
}

struct ProfileArchiveRunningGuard {
    coordinator: Arc<ProfileArchiveCoordinator>,
}

impl Drop for ProfileArchiveRunningGuard {
    fn drop(&mut self) {
        let mut state = self
            .coordinator
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.archive_running = false;
        self.coordinator.changed.notify_all();
    }
}

fn execute_profile_operation(
    mut spec: ProfileOperationSpec,
    archive_coordinator: Arc<ProfileArchiveCoordinator>,
) -> std::result::Result<ProfileWorkerOutput, ProfileWorkerError> {
    use crate::integrations::profiles;
    let _archive_pause = archive_coordinator.pause_for_foreground();
    let roots = match profiles::profile_roots(&spec.game, spec.use_default_mods_path) {
        Ok(roots) => roots,
        Err(_) if spec.kind == ProfileOperationKind::Recover => {
            return Ok(ProfileWorkerOutput {
                kind: spec.kind,
                profile_id: None,
                target_profile_id: None,
                display_name: None,
                archive: None,
                active_profile_marker: None,
                background_archives: Vec::new(),
                warnings: Vec::new(),
            });
        }
        Err(error) => return Err(ProfileWorkerError::Other(error)),
    };
    let _staging_cleanup = ProfileStagingCleanup::new(&roots, spec.operation_id);
    match spec.kind {
        ProfileOperationKind::Recover => {
            profiles::ensure_profile_storage_layout(&roots)
                .map_err(ProfileWorkerError::RecoveryBlocked)?;
            let marker = recover_profile_staging(&roots, &spec)
                .map_err(ProfileWorkerError::into_recovery_blocking)?;
            let (background_archives, warnings) = pending_profile_archive_jobs(
                &spec,
                &roots,
                marker.as_ref().map(|marker| marker.profile_id),
            )
            .map_err(ProfileWorkerError::into_recovery_blocking)?;
            return Ok(ProfileWorkerOutput {
                kind: spec.kind,
                profile_id: None,
                target_profile_id: marker.as_ref().map(|marker| marker.profile_id),
                display_name: marker.as_ref().map(|marker| marker.display_name.clone()),
                archive: None,
                active_profile_marker: marker,
                background_archives,
                warnings,
            });
        }
        _ => {}
    }
    profiles::ensure_profile_storage_layout(&roots).map_err(ProfileWorkerError::Other)?;

    let operation_id = spec.operation_id;
    let staging = roots
        .staging_dir()
        .join(format!("{operation_id:016x}.extracting"));
    let mut background_archives = Vec::new();
    let mut warnings = Vec::new();
    match spec.kind {
        ProfileOperationKind::Create => {
            prepare_empty_profile_target(&roots, &staging, &spec.cancel)?;
            let marker = target_profile_marker(&spec)?;
            check_profile_cancel(&spec.cancel)?;
            update_profile_progress(
                &spec.progress,
                &spec.stage,
                70,
                "Activating selected profile",
            );
            swap_roots(
                &roots,
                &staging,
                operation_id,
                outgoing_profile(&spec)?,
                &marker,
                &mut warnings,
            )?;
            finish_profile_switch(&spec);
            queue_outgoing_archive(&spec, &mut background_archives);
        }
        ProfileOperationKind::Duplicate => {
            let expected_source = spec
                .source_profile_id
                .ok_or_else(|| anyhow::anyhow!("source profile id is missing"))?;
            let source = prepare_duplicate_target(
                &mut spec,
                &roots,
                &staging,
                expected_source,
                &mut warnings,
            )?;
            let marker = target_profile_marker(&spec)?;
            check_profile_cancel(&spec.cancel)?;
            update_profile_progress(
                &spec.progress,
                &spec.stage,
                70,
                "Activating selected profile",
            );
            swap_roots(
                &roots,
                &source,
                operation_id,
                outgoing_profile(&spec)?,
                &marker,
                &mut warnings,
            )?;
            finish_profile_switch(&spec);
            queue_outgoing_archive(&spec, &mut background_archives);
        }
        ProfileOperationKind::Switch => {
            let expected_target = spec
                .target_profile_id
                .ok_or_else(|| anyhow::anyhow!("target profile id is missing"))?;
            let source =
                prepare_switch_target(&mut spec, &roots, &staging, expected_target, &mut warnings)?;
            let marker = target_profile_marker(&spec)?;
            check_profile_cancel(&spec.cancel)?;
            update_profile_progress(
                &spec.progress,
                &spec.stage,
                70,
                "Activating selected profile",
            );
            swap_roots(
                &roots,
                &source,
                operation_id,
                outgoing_profile(&spec)?,
                &marker,
                &mut warnings,
            )?;
            finish_profile_switch(&spec);
            queue_outgoing_archive(&spec, &mut background_archives);
        }
        ProfileOperationKind::Rename => {}
        ProfileOperationKind::Delete => {
            delete_profile_storage(&spec, &roots)?;
        }
        ProfileOperationKind::Recover => unreachable!(),
    }

    let active_profile_marker = if matches!(
        spec.kind,
        ProfileOperationKind::Create
            | ProfileOperationKind::Duplicate
            | ProfileOperationKind::Switch
    ) {
        Some(target_profile_marker(&spec)?)
    } else {
        None
    };
    Ok(ProfileWorkerOutput {
        kind: spec.kind,
        profile_id: spec.profile_id,
        target_profile_id: spec.target_profile_id,
        display_name: spec.target_display_name.or(spec.display_name),
        archive: None,
        active_profile_marker,
        background_archives,
        warnings,
    })
}

fn target_profile_marker(
    spec: &ProfileOperationSpec,
) -> std::result::Result<ActiveProfileMarker, ProfileWorkerError> {
    Ok(ActiveProfileMarker {
        profile_id: spec
            .target_profile_id
            .ok_or_else(|| anyhow::anyhow!("target profile id is missing"))?,
        display_name: spec
            .target_display_name
            .clone()
            .or_else(|| spec.display_name.clone())
            .unwrap_or_else(|| "Profile".to_string()),
        categories: spec.target_categories.clone(),
        tools: spec.target_tools.clone(),
        tool_blacklist: spec.target_tool_blacklist.clone(),
    })
}

/// Adopt profile-owned data from the container/archive we are about to activate, for anything the
/// request did not already supply. The catalog is authoritative when it has a value; this is the
/// fallback that lets a profile recovered from disk carry its own categories and tools.
fn backfill_target_profile_data(
    spec: &mut ProfileOperationSpec,
    metadata: &crate::integrations::profiles::ProfileArchiveMetadata,
) {
    if spec.target_categories.is_none() {
        spec.target_categories = metadata.categories.clone();
    }
    if spec.target_tools.is_none() {
        spec.target_tools = metadata.tools.clone();
    }
    if spec.target_tool_blacklist.is_none() {
        spec.target_tool_blacklist = metadata.tool_blacklist.clone();
    }
}

fn outgoing_profile(
    spec: &ProfileOperationSpec,
) -> std::result::Result<
    Option<(Uuid, crate::integrations::profiles::ProfileArchiveMetadata)>,
    ProfileWorkerError,
> {
    let Some(profile_id) = spec.profile_id else {
        return Ok(None);
    };
    let metadata = spec
        .metadata
        .clone()
        .ok_or_else(|| anyhow::anyhow!("active profile metadata is missing"))?;
    if metadata.profile_id != profile_id {
        return Err(anyhow::anyhow!("active profile metadata does not match its id").into());
    }
    Ok(Some((profile_id, metadata)))
}

fn queue_outgoing_archive(spec: &ProfileOperationSpec, jobs: &mut Vec<ProfileArchiveJob>) {
    let Some(profile_id) = spec.profile_id else {
        return;
    };
    jobs.push(ProfileArchiveJob {
        game_id: spec.game_id.clone(),
        game: spec.game.clone(),
        use_default_mods_path: spec.use_default_mods_path,
        profile_id,
    });
}

fn profile_container_roots(
    roots: &crate::integrations::profiles::ProfileRoots,
    container: &Path,
) -> crate::integrations::profiles::ProfileRoots {
    crate::integrations::profiles::ProfileRoots {
        profiles_dir: roots.profiles_dir.clone(),
        mods: container.join("Mods"),
        archived: roots
            .archived
            .as_ref()
            .map(|_| container.join("Mods_Archived")),
        disabled: roots.disabled.as_ref().map(|_| container.join("Disabled")),
    }
}

fn read_profile_container_metadata(
    container: &Path,
) -> std::result::Result<crate::integrations::profiles::ProfileArchiveMetadata, ProfileWorkerError>
{
    let bytes =
        std::fs::read(container.join(crate::integrations::profiles::PROFILE_METADATA_FILE))?;
    Ok(serde_json::from_slice(&bytes).map_err(anyhow::Error::from)?)
}

fn write_profile_container_metadata(
    container: &Path,
    metadata: &crate::integrations::profiles::ProfileArchiveMetadata,
) -> std::result::Result<(), ProfileWorkerError> {
    std::fs::create_dir_all(container)?;
    let path = container.join(crate::integrations::profiles::PROFILE_METADATA_FILE);
    let part = container.join("profile.json.part");
    let bytes = serde_json::to_vec_pretty(metadata).map_err(anyhow::Error::from)?;
    std::fs::write(&part, bytes)?;
    std::fs::rename(part, path)?;
    Ok(())
}

fn validate_profile_container_metadata(
    metadata: &crate::integrations::profiles::ProfileArchiveMetadata,
    spec: &ProfileOperationSpec,
    expected_profile_id: Uuid,
) -> std::result::Result<(), ProfileWorkerError> {
    if metadata.profile_id != expected_profile_id
        || metadata.game_id != spec.game_id
        || metadata.backend != spec.game.definition.backend
        || metadata.format_version == 0
    {
        return Err(
            anyhow::anyhow!("profile data does not match the requested game/profile").into(),
        );
    }
    Ok(())
}

fn prepare_empty_profile_target(
    roots: &crate::integrations::profiles::ProfileRoots,
    staging: &Path,
    cancel: &Arc<AtomicBool>,
) -> std::result::Result<(), ProfileWorkerError> {
    check_profile_cancel(cancel)?;
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    create_empty_roots(&profile_container_roots(roots, staging))
}

fn prepare_switch_target(
    spec: &mut ProfileOperationSpec,
    roots: &crate::integrations::profiles::ProfileRoots,
    staging: &Path,
    target_profile_id: Uuid,
    warnings: &mut Vec<String>,
) -> std::result::Result<PathBuf, ProfileWorkerError> {
    let loose = roots.profile_path(target_profile_id);
    if loose.exists() {
        let metadata = loose
            .is_dir()
            .then(|| read_profile_container_metadata(&loose))
            .and_then(Result::ok);
        if let Some(metadata) = metadata.filter(|metadata| {
            validate_profile_container_metadata(metadata, spec, target_profile_id).is_ok()
        }) {
            backfill_target_profile_data(spec, &metadata);
            return Ok(loose);
        }
        let conflict = next_profile_conflict_path(&loose, spec.operation_id);
        std::fs::rename(&loose, &conflict)?;
        warnings.push(format!(
            "Unrecognized profile data was preserved at {}",
            conflict.display()
        ));
    }

    let archive = spec
        .target_archive
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("target profile archive is missing"))?;
    let archive_metadata =
        match crate::integrations::profiles::read_profile_archive_metadata(archive) {
            Ok(metadata)
                if validate_profile_container_metadata(&metadata, spec, target_profile_id)
                    .is_ok() =>
            {
                metadata
            }
            Ok(_) | Err(_) => {
                let conflict = next_archive_conflict_path(archive);
                std::fs::rename(archive, &conflict)?;
                return Err(anyhow::anyhow!(
                    "target profile archive was invalid and was preserved at {}",
                    conflict.display()
                )
                .into());
            }
        };
    crate::integrations::profiles::ensure_profile_space(
        &roots.profiles_dir,
        archive_metadata
            .uncompressed_size
            .saturating_add(64 * 1024 * 1024),
    )?;
    let metadata = extract_to_staging(archive, staging, &spec.cancel, &spec.progress, &spec.stage)?;
    validate_profile_container_metadata(&metadata, spec, target_profile_id)?;
    backfill_target_profile_data(spec, &metadata);
    Ok(staging.to_path_buf())
}

fn prepare_duplicate_target(
    spec: &mut ProfileOperationSpec,
    roots: &crate::integrations::profiles::ProfileRoots,
    staging: &Path,
    source_profile_id: Uuid,
    warnings: &mut Vec<String>,
) -> std::result::Result<PathBuf, ProfileWorkerError> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    update_profile_progress(
        &spec.progress,
        &spec.stage,
        20,
        "Preparing selected profile",
    );

    if spec.profile_id == Some(source_profile_id) {
        let required = profile_tree_size(roots)?.saturating_add(64 * 1024 * 1024);
        crate::integrations::profiles::ensure_profile_space(&roots.profiles_dir, required)?;
        copy_profile_roots(
            roots,
            &profile_container_roots(roots, staging),
            &spec.cancel,
        )?;
        update_profile_progress(&spec.progress, &spec.stage, 60, "Selected profile prepared");
        return Ok(staging.to_path_buf());
    }

    let loose = roots.profile_path(source_profile_id);
    if loose.exists() {
        let metadata = loose
            .is_dir()
            .then(|| read_profile_container_metadata(&loose))
            .and_then(Result::ok);
        if let Some(metadata) = metadata.filter(|metadata| {
            validate_profile_container_metadata(metadata, spec, source_profile_id).is_ok()
        }) {
            let loose_roots = profile_container_roots(roots, &loose);
            let required = profile_tree_size(&loose_roots)?.saturating_add(64 * 1024 * 1024);
            crate::integrations::profiles::ensure_profile_space(&roots.profiles_dir, required)?;
            crate::importing::copy_dir_cancelable(&loose, staging, true, &spec.cancel)?;
            backfill_target_profile_data(spec, &metadata);
            update_profile_progress(&spec.progress, &spec.stage, 60, "Selected profile prepared");
            return Ok(staging.to_path_buf());
        }
        let conflict = next_profile_conflict_path(&loose, spec.operation_id);
        std::fs::rename(&loose, &conflict)?;
        warnings.push(format!(
            "Unrecognized profile data was preserved at {}",
            conflict.display()
        ));
    }

    let archive = spec
        .source_archive
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("source profile archive is missing"))?;
    let archive_metadata =
        match crate::integrations::profiles::read_profile_archive_metadata(archive) {
            Ok(metadata)
                if validate_profile_container_metadata(&metadata, spec, source_profile_id)
                    .is_ok() =>
            {
                metadata
            }
            Ok(_) | Err(_) => {
                let conflict = next_archive_conflict_path(archive);
                std::fs::rename(archive, &conflict)?;
                return Err(anyhow::anyhow!(
                    "source profile archive was invalid and was preserved at {}",
                    conflict.display()
                )
                .into());
            }
        };
    crate::integrations::profiles::ensure_profile_space(
        &roots.profiles_dir,
        archive_metadata
            .uncompressed_size
            .saturating_add(64 * 1024 * 1024),
    )?;
    let metadata = extract_to_staging(archive, staging, &spec.cancel, &spec.progress, &spec.stage)?;
    validate_profile_container_metadata(&metadata, spec, source_profile_id)?;
    backfill_target_profile_data(spec, &metadata);
    Ok(staging.to_path_buf())
}

fn copy_profile_roots(
    source: &crate::integrations::profiles::ProfileRoots,
    destination: &crate::integrations::profiles::ProfileRoots,
    cancel: &Arc<AtomicBool>,
) -> std::result::Result<(), ProfileWorkerError> {
    for ((_, source), (_, destination)) in root_locations(source)
        .into_iter()
        .zip(root_locations(destination))
    {
        if source.exists() {
            crate::importing::copy_dir_cancelable(&source, &destination, true, cancel)?;
        } else {
            std::fs::create_dir_all(destination)?;
        }
    }
    Ok(())
}

fn delete_profile_storage(
    spec: &ProfileOperationSpec,
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<(), ProfileWorkerError> {
    let profile_id = spec
        .profile_id
        .ok_or_else(|| anyhow::anyhow!("profile id is missing"))?;
    let legacy_archive = roots.profiles_dir.join(format!("{profile_id}.tzst"));
    let paths = [
        roots.profile_path(profile_id),
        roots.archive_path(profile_id),
        roots.archive_part_path(profile_id),
        roots.archive_backup_path(profile_id),
        legacy_archive,
    ];
    for path in paths {
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn check_profile_cancel(cancel: &Arc<AtomicBool>) -> std::result::Result<(), ProfileWorkerError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ProfileWorkerError::Canceled)
    } else {
        Ok(())
    }
}

fn profile_tree_size(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<u64, ProfileWorkerError> {
    fn size(path: &Path) -> std::io::Result<u64> {
        if !path.exists() {
            return Ok(0);
        }
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(path).follow_links(false) {
            let entry = entry.map_err(std::io::Error::other)?;
            if entry.file_type().is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
        Ok(total)
    }
    let mut total = size(&roots.mods)?;
    if let Some(path) = &roots.archived {
        total = total.saturating_add(size(path)?);
    }
    if let Some(path) = &roots.disabled {
        total = total.saturating_add(size(path)?);
    }
    Ok(total)
}

fn update_profile_progress(
    progress: &Arc<AtomicU64>,
    stage: &Arc<RwLock<String>>,
    value: u64,
    label: &str,
) {
    progress.store(value.min(100), Ordering::Relaxed);
    if let Ok(mut stage) = stage.write() {
        stage.clear();
        stage.push_str(label);
    }
}

fn finish_profile_switch(spec: &ProfileOperationSpec) {
    update_profile_progress(
        &spec.progress,
        &spec.stage,
        PROFILE_SWITCH_FINISH_PROGRESS_START,
        "Switching profile",
    );
    let step_delay = PROFILE_SWITCH_FINISH_DELAY / PROFILE_SWITCH_FINISH_PROGRESS_STEPS;
    for step in 1..=PROFILE_SWITCH_FINISH_PROGRESS_STEPS {
        std::thread::sleep(step_delay);
        update_profile_progress(
            &spec.progress,
            &spec.stage,
            PROFILE_SWITCH_FINISH_PROGRESS_START + u64::from(step),
            "Switching profile",
        );
    }
}

fn execute_profile_archive_job(
    job: &ProfileArchiveJob,
    coordinator: &Arc<ProfileArchiveCoordinator>,
    on_started: &mut dyn FnMut(),
) -> Result<ProfileArchiveJobOutcome> {
    use crate::integrations::profiles;

    let _running = coordinator.begin_archive();
    if coordinator.foreground_requested() {
        return Ok(ProfileArchiveJobOutcome::Paused);
    }
    let roots = profiles::profile_roots(&job.game, job.use_default_mods_path)?;
    profiles::ensure_profile_storage_layout(&roots)?;
    if profile_is_active(&roots, job.profile_id)? {
        return Ok(ProfileArchiveJobOutcome::Active);
    }
    let loose = roots.profile_path(job.profile_id);
    if !loose.exists() {
        return Ok(ProfileArchiveJobOutcome::Missing);
    }
    if !loose.is_dir() {
        bail!(
            "profile storage collision: {} is not a directory",
            loose.display()
        );
    }
    let mut metadata = read_profile_container_metadata(&loose)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if metadata.profile_id != job.profile_id
        || metadata.game_id != job.game_id
        || metadata.backend != job.game.definition.backend
        || metadata.format_version == 0
    {
        bail!("loose profile metadata does not match the queued profile");
    }
    on_started();
    let loose_roots = profile_container_roots(&roots, &loose);
    let inventory = profiles::inspect_profile_source(&loose_roots)?;
    let fingerprint = inventory.fingerprint().to_owned();
    metadata.source_fingerprint = Some(fingerprint.clone());
    let destination = roots.archive_path(job.profile_id);

    if destination.exists() {
        match profiles::read_profile_archive_metadata(&destination) {
            Ok(existing)
                if existing.profile_id == job.profile_id
                    && existing.game_id == job.game_id
                    && existing.backend == job.game.definition.backend =>
            {
                if profile_archive_can_be_reused(&existing, &metadata, &fingerprint) {
                    if let Err(error) = validate_zstd_profile_frame(&destination, coordinator) {
                        if coordinator.foreground_requested() {
                            return Ok(ProfileArchiveJobOutcome::Paused);
                        }
                        let conflict = next_archive_conflict_path(&destination);
                        std::fs::rename(&destination, &conflict)?;
                        let _ = error;
                    } else {
                        if coordinator.foreground_requested() {
                            return Ok(ProfileArchiveJobOutcome::Paused);
                        }
                        if profile_is_active(&roots, job.profile_id)? {
                            return Ok(ProfileArchiveJobOutcome::Active);
                        }
                        let bytes = std::fs::metadata(&destination)?.len();
                        remove_loose_profile_after_archive(&roots, job.profile_id, &loose)?;
                        return Ok(ProfileArchiveJobOutcome::Completed(
                            profiles::ArchiveResult {
                                archive_path: destination,
                                bytes,
                                uncompressed_size: existing.uncompressed_size,
                                file_count: existing.file_count,
                            },
                        ));
                    }
                }
            }
            Ok(_) | Err(_) => {
                let conflict = next_archive_conflict_path(&destination);
                std::fs::rename(&destination, conflict)?;
            }
        }
    }

    let mut callback = |_update: profiles::ArchiveProgress| -> Result<()> {
        if coordinator.foreground_requested() {
            bail!("profile background archive paused");
        }
        Ok(())
    };
    let archive = match profiles::create_profile_archive_from_inventory_with_progress(
        &inventory,
        &metadata,
        &destination,
        Some(&mut callback),
    ) {
        Ok(archive) => archive,
        Err(_error) if coordinator.foreground_requested() => {
            let _ = std::fs::remove_file(roots.archive_part_path(job.profile_id));
            return Ok(ProfileArchiveJobOutcome::Paused);
        }
        Err(error) => {
            let _ = std::fs::remove_file(roots.archive_part_path(job.profile_id));
            return Err(error);
        }
    };
    if coordinator.foreground_requested() {
        return Ok(ProfileArchiveJobOutcome::Paused);
    }
    if profile_is_active(&roots, job.profile_id)? {
        return Ok(ProfileArchiveJobOutcome::Active);
    }
    remove_loose_profile_after_archive(&roots, job.profile_id, &loose)?;
    Ok(ProfileArchiveJobOutcome::Completed(archive))
}

fn profile_is_active(
    roots: &crate::integrations::profiles::ProfileRoots,
    profile_id: Uuid,
) -> Result<bool> {
    let marker = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    if !marker.exists() {
        return Ok(false);
    }
    if !marker.is_file() {
        bail!("active profile marker is not a file: {}", marker.display());
    }
    let marker: ActiveProfileMarker =
        serde_json::from_slice(&std::fs::read(&marker)?).map_err(|error| {
            anyhow::anyhow!(
                "failed to read active profile marker {}: {error}",
                marker.display()
            )
        })?;
    Ok(marker.profile_id == profile_id)
}

struct ProfileArchiveCoordinatorReader<'a> {
    file: std::fs::File,
    coordinator: &'a ProfileArchiveCoordinator,
}

impl std::io::Read for ProfileArchiveCoordinatorReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.coordinator.foreground_requested() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "profile background archive paused",
            ));
        }
        std::io::Read::read(&mut self.file, buffer)
    }
}

fn validate_zstd_profile_frame(path: &Path, coordinator: &ProfileArchiveCoordinator) -> Result<()> {
    let reader = ProfileArchiveCoordinatorReader {
        file: std::fs::File::open(path)?,
        coordinator,
    };
    let mut decoder = zstd::stream::read::Decoder::new(reader)?;
    std::io::copy(&mut decoder, &mut std::io::sink())?;
    Ok(())
}

fn remove_loose_profile_after_archive(
    roots: &crate::integrations::profiles::ProfileRoots,
    profile_id: Uuid,
    loose: &Path,
) -> Result<()> {
    let deleting = roots
        .profiles_dir
        .join(format!("{profile_id}.profile.deleting"));
    if deleting.exists() {
        let conflict = next_profile_conflict_path(&deleting, 0);
        std::fs::rename(&deleting, conflict)?;
    }
    std::fs::rename(loose, &deleting)?;
    std::fs::remove_dir_all(deleting)?;
    Ok(())
}

fn profile_archive_can_be_reused(
    existing: &crate::integrations::profiles::ProfileArchiveMetadata,
    current: &crate::integrations::profiles::ProfileArchiveMetadata,
    fingerprint: &str,
) -> bool {
    existing.source_fingerprint.as_deref() == Some(fingerprint)
        && existing.profile_id == current.profile_id
        && existing.game_id == current.game_id
        && existing.display_name == current.display_name
        && existing.backend == current.backend
        && existing.created_at == current.created_at
        && existing.portable_metadata == current.portable_metadata
        && existing.categories == current.categories
        && existing.tools == current.tools
        && existing.tool_blacklist == current.tool_blacklist
}

fn next_archive_conflict_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("profile.tzst");
    let stem = file_name.strip_suffix(".tzst").unwrap_or(file_name);
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for suffix in 0u32.. {
        let candidate = if suffix == 0 {
            parent.join(format!("{stem}.conflict-{nonce}.tzst"))
        } else {
            parent.join(format!("{stem}.conflict-{nonce}-{suffix}.tzst"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn extract_to_staging(
    archive: &Path,
    staging: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<AtomicU64>,
    stage: &Arc<RwLock<String>>,
) -> std::result::Result<crate::integrations::profiles::ProfileArchiveMetadata, ProfileWorkerError>
{
    use crate::integrations::profiles;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 10, "Preparing selected profile");
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    let mut callback = |update: profiles::ArchiveReadProgress| -> Result<()> {
        if cancel.load(Ordering::Relaxed) {
            bail!("profile operation canceled");
        }
        let pct = if update.total_bytes == 0 {
            10
        } else {
            10 + (update.bytes_read.saturating_mul(50) / update.total_bytes).min(50)
        };
        if progress.load(Ordering::Relaxed) != pct {
            update_profile_progress(progress, stage, pct, "Extracting selected profile");
        }
        Ok(())
    };
    let metadata =
        profiles::extract_profile_archive_with_progress(archive, staging, Some(&mut callback))?;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 60, "Selected profile prepared");
    Ok(metadata)
}

fn create_empty_roots(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<(), ProfileWorkerError> {
    std::fs::create_dir_all(&roots.mods)?;
    if let Some(path) = &roots.archived {
        std::fs::create_dir_all(path)?;
    }
    if let Some(path) = &roots.disabled {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

fn swap_roots(
    roots: &crate::integrations::profiles::ProfileRoots,
    target_source: &Path,
    operation_id: u64,
    outgoing_profile: Option<(Uuid, crate::integrations::profiles::ProfileArchiveMetadata)>,
    marker: &ActiveProfileMarker,
    warnings: &mut Vec<String>,
) -> std::result::Result<(), ProfileWorkerError> {
    let (outgoing_id, outgoing_metadata) = outgoing_profile
        .ok_or_else(|| anyhow::anyhow!("active profile is missing during profile switch"))?;
    let journal = roots
        .staging_dir()
        .join(format!("{operation_id:016x}.journal"));
    std::fs::create_dir_all(roots.staging_dir())?;
    let outgoing = roots.profile_path(outgoing_id);
    if outgoing.exists() {
        let conflict = next_profile_conflict_path(&outgoing, operation_id);
        std::fs::rename(&outgoing, &conflict)?;
        warnings.push(format!(
            "Existing profile data was preserved at {}",
            conflict.display()
        ));
    }
    let mut journal_state = ProfileSwapJournal {
        phase: ProfileSwapPhase::BackingUp,
        outgoing: outgoing.clone(),
        target_source: target_source.to_path_buf(),
    };
    write_profile_swap_journal(&journal, &journal_state)?;
    if let Err(error) = write_profile_container_metadata(&outgoing, &outgoing_metadata) {
        let _ = std::fs::remove_dir_all(&outgoing);
        let _ = std::fs::remove_file(&journal);
        return Err(error);
    }

    let active_marker = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    if active_marker.exists() {
        if let Err(error) =
            std::fs::rename(&active_marker, outgoing.join(ACTIVE_PROFILE_MARKER_FILE))
        {
            let _ = std::fs::remove_dir_all(&outgoing);
            let _ = std::fs::remove_file(&journal);
            return Err(error.into());
        }
    }
    let locations = root_locations(roots);
    let outgoing_roots = profile_container_roots(roots, &outgoing);
    for (name, root) in &locations {
        if root.exists() {
            let outgoing_root = root_locations(&outgoing_roots)
                .into_iter()
                .find_map(|(candidate, path)| (candidate == *name).then_some(path))
                .ok_or_else(|| anyhow::anyhow!("profile root mapping is incomplete"))?;
            if let Err(error) = std::fs::rename(root, outgoing_root) {
                rollback_profile_swap(roots, &outgoing, target_source, false).map_err(
                    |rollback| {
                        anyhow::anyhow!(
                            "profile commit failed: {error}; rollback failed: {rollback}"
                        )
                    },
                )?;
                let _ = std::fs::remove_file(&journal);
                return Err(error.into());
            }
        }
    }
    journal_state.phase = ProfileSwapPhase::Installing;
    if let Err(error) = write_profile_swap_journal(&journal, &journal_state) {
        rollback_profile_swap(roots, &outgoing, target_source, false).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {error}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_file(&journal);
        return Err(error);
    }

    let target_roots = profile_container_roots(roots, target_source);
    for (name, destination) in &locations {
        let source = root_locations(&target_roots)
            .into_iter()
            .find_map(|(candidate, path)| (candidate == *name).then_some(path))
            .ok_or_else(|| anyhow::anyhow!("profile root mapping is incomplete"))?;
        if source.exists() {
            if let Err(error) = std::fs::rename(source, destination) {
                rollback_profile_swap(roots, &outgoing, target_source, true).map_err(
                    |rollback| {
                        anyhow::anyhow!(
                            "profile commit failed: {error}; rollback failed: {rollback}"
                        )
                    },
                )?;
                let _ = std::fs::remove_file(&journal);
                return Err(error.into());
            }
        }
    }
    if let Err(error) = create_empty_roots(roots) {
        rollback_profile_swap(roots, &outgoing, target_source, true).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {error}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_file(&journal);
        return Err(error);
    }
    if let Err(error) = write_active_profile_marker(roots, marker) {
        rollback_profile_swap(roots, &outgoing, target_source, true).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {error}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_file(&journal);
        return Err(error.into());
    }
    journal_state.phase = ProfileSwapPhase::Committed;
    if let Err(error) = write_profile_swap_journal(&journal, &journal_state) {
        rollback_profile_swap(roots, &outgoing, target_source, true).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {error}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_file(&journal);
        return Err(error);
    }

    let _ = std::fs::remove_file(outgoing.join(ACTIVE_PROFILE_MARKER_FILE));
    if std::fs::remove_dir_all(target_source).is_ok() {
        let _ = std::fs::remove_file(&journal);
    }
    Ok(())
}

fn write_active_profile_marker(
    roots: &crate::integrations::profiles::ProfileRoots,
    marker: &ActiveProfileMarker,
) -> std::io::Result<()> {
    let path = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    let part = roots.profiles_dir.join("active_profile.json.part");
    let bytes = serde_json::to_vec_pretty(marker).map_err(std::io::Error::other)?;
    std::fs::write(&part, bytes)?;
    std::fs::rename(part, path)
}

fn root_locations(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> Vec<(&'static str, PathBuf)> {
    let mut result = vec![("Mods", roots.mods.clone())];
    if let Some(path) = roots.archived.clone() {
        result.push(("Mods_Archived", path));
    }
    if let Some(path) = roots.disabled.clone() {
        result.push(("Disabled", path));
    }
    result
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ProfileSwapPhase {
    BackingUp,
    Installing,
    Committed,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ProfileSwapJournal {
    phase: ProfileSwapPhase,
    outgoing: PathBuf,
    target_source: PathBuf,
}

fn write_profile_swap_journal(
    path: &Path,
    journal: &ProfileSwapJournal,
) -> std::result::Result<(), ProfileWorkerError> {
    let part = path.with_extension("journal.part");
    let bytes = serde_json::to_vec_pretty(journal).map_err(anyhow::Error::from)?;
    std::fs::write(&part, bytes)?;
    std::fs::rename(part, path)?;
    Ok(())
}

fn next_profile_conflict_path(path: &Path, operation_id: u64) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("profile");
    for suffix in 0u32.. {
        let candidate = if suffix == 0 {
            parent.join(format!("{name}.conflict-{operation_id:016x}"))
        } else {
            parent.join(format!("{name}.conflict-{operation_id:016x}-{suffix}"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn rollback_profile_swap(
    roots: &crate::integrations::profiles::ProfileRoots,
    outgoing: &Path,
    target_source: &Path,
    remove_installed: bool,
) -> std::io::Result<()> {
    let locations = root_locations(roots);
    let target_roots = profile_container_roots(roots, target_source);
    let outgoing_roots = profile_container_roots(roots, outgoing);
    if remove_installed {
        for (name, live_root) in &locations {
            let target_root = root_locations(&target_roots)
                .into_iter()
                .find_map(|(candidate, path)| (candidate == *name).then_some(path))
                .ok_or_else(|| std::io::Error::other("profile root mapping is incomplete"))?;
            if live_root.exists() && !target_root.exists() {
                if let Some(parent) = target_root.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(live_root, target_root)?;
            }
        }
    }
    for (name, live_root) in &locations {
        let outgoing_root = root_locations(&outgoing_roots)
            .into_iter()
            .find_map(|(candidate, path)| (candidate == *name).then_some(path))
            .ok_or_else(|| std::io::Error::other("profile root mapping is incomplete"))?;
        if outgoing_root.exists() {
            if live_root.exists() {
                std::fs::remove_dir_all(live_root)?;
            }
            std::fs::rename(outgoing_root, live_root)?;
        }
    }
    let marker = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    let outgoing_marker = outgoing.join(ACTIVE_PROFILE_MARKER_FILE);
    if outgoing_marker.exists() {
        let _ = std::fs::remove_file(&marker);
        std::fs::rename(outgoing_marker, marker)?;
    } else if remove_installed {
        let _ = std::fs::remove_file(marker);
    }
    let _ = std::fs::remove_dir_all(outgoing);
    Ok(())
}

fn recover_profile_staging(
    roots: &crate::integrations::profiles::ProfileRoots,
    spec: &ProfileOperationSpec,
) -> std::result::Result<Option<ActiveProfileMarker>, ProfileWorkerError> {
    std::fs::create_dir_all(&roots.profiles_dir)?;
    migrate_legacy_profile_archives(roots)?;
    recover_profile_swap_journals(roots)?;
    recover_profile_archive_sidecars(roots, spec)?;
    recover_profile_deletions(roots, spec)?;
    let marker_path = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    if !marker_path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(marker_path)?;
    let marker = serde_json::from_slice(&bytes).map_err(anyhow::Error::from)?;
    Ok(Some(marker))
}

fn recover_profile_swap_journals(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<(), ProfileWorkerError> {
    let staging = roots.staging_dir();
    if !staging.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(&staging)?.collect::<std::result::Result<Vec<_>, _>>()?;
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(operation_id) = name.strip_suffix(".journal") else {
            continue;
        };
        let bytes = std::fs::read(entry.path()).unwrap_or_default();
        if let Ok(journal) = serde_json::from_slice::<ProfileSwapJournal>(&bytes) {
            match journal.phase {
                ProfileSwapPhase::Committed => {
                    let _ = std::fs::remove_file(journal.outgoing.join(ACTIVE_PROFILE_MARKER_FILE));
                    let _ = std::fs::remove_dir_all(&journal.target_source);
                }
                ProfileSwapPhase::BackingUp | ProfileSwapPhase::Installing => {
                    rollback_profile_swap(
                        roots,
                        &journal.outgoing,
                        &journal.target_source,
                        journal.phase == ProfileSwapPhase::Installing,
                    )?;
                }
            }
        } else if matches!(
            std::str::from_utf8(&bytes).unwrap_or_default().trim(),
            "backing-up" | "installing" | "committed"
        ) {
            recover_legacy_profile_swap(roots, operation_id, &bytes)?;
        } else {
            return Err(anyhow::anyhow!(
                "profile recovery journal is malformed and was preserved at {}",
                entry.path().display()
            )
            .into());
        }
        let _ = std::fs::remove_file(entry.path());
    }
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".extracting") {
            let _ = std::fs::remove_dir_all(entry.path());
        } else if name.ends_with(".journal.part") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let _ = std::fs::remove_dir(&staging);
    Ok(())
}

fn recover_legacy_profile_swap(
    roots: &crate::integrations::profiles::ProfileRoots,
    operation_id: &str,
    phase: &[u8],
) -> std::result::Result<(), ProfileWorkerError> {
    let backup = roots
        .mods
        .with_file_name(format!(".hestia-profile-backup-{operation_id}"));
    if !backup.exists() {
        return Ok(());
    }
    let locations = root_locations(roots);
    let remove_installed = std::str::from_utf8(phase).unwrap_or_default().trim() == "installing";
    if std::str::from_utf8(phase).unwrap_or_default().trim() != "committed" {
        for (name, root) in &locations {
            let backup_root = backup.join(name);
            if backup_root.exists() {
                let _ = std::fs::remove_dir_all(root);
                std::fs::rename(backup_root, root)?;
            } else if remove_installed {
                let _ = std::fs::remove_dir_all(root);
            }
        }
        let marker = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
        let _ = std::fs::remove_file(&marker);
        let backup_marker = backup.join(ACTIVE_PROFILE_MARKER_FILE);
        if backup_marker.exists() {
            std::fs::rename(backup_marker, marker)?;
        }
    }
    std::fs::remove_dir_all(backup)?;
    Ok(())
}

fn migrate_legacy_profile_archives(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<(), ProfileWorkerError> {
    for entry in std::fs::read_dir(&roots.profiles_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let legacy_sidecar = name
            .strip_suffix(".tzst.part")
            .map(|stem| (stem, "part"))
            .or_else(|| name.strip_suffix(".tzst.bak").map(|stem| (stem, "bak")));
        if let Some((stem, sidecar)) = legacy_sidecar {
            let Ok(profile_id) = Uuid::parse_str(stem) else {
                continue;
            };
            let legacy = entry.path();
            let canonical = if sidecar == "part" {
                roots.archive_part_path(profile_id)
            } else {
                roots.archive_backup_path(profile_id)
            };
            if canonical.exists() {
                std::fs::rename(
                    legacy,
                    next_archive_conflict_path(&roots.archive_path(profile_id)),
                )?;
            } else {
                std::fs::rename(legacy, canonical)?;
            }
            continue;
        }
        let Some(stem) = name.strip_suffix(".tzst") else {
            continue;
        };
        if stem.ends_with(".profile") {
            continue;
        }
        let Ok(profile_id) = Uuid::parse_str(stem) else {
            continue;
        };
        let legacy = entry.path();
        let canonical = roots.archive_path(profile_id);
        if !canonical.exists() {
            if profile_archive_matches(&legacy, profile_id, None) {
                std::fs::rename(legacy, canonical)?;
            } else {
                std::fs::rename(legacy, next_archive_conflict_path(&canonical))?;
            }
        } else if profile_archive_matches(&canonical, profile_id, None) {
            std::fs::rename(legacy, next_archive_conflict_path(&canonical))?;
        } else if profile_archive_matches(&legacy, profile_id, None) {
            let conflict = next_archive_conflict_path(&canonical);
            std::fs::rename(&canonical, conflict)?;
            std::fs::rename(legacy, canonical)?;
        } else {
            std::fs::rename(legacy, next_archive_conflict_path(&canonical))?;
        }
    }
    Ok(())
}

fn recover_profile_archive_sidecars(
    roots: &crate::integrations::profiles::ProfileRoots,
    spec: &ProfileOperationSpec,
) -> std::result::Result<(), ProfileWorkerError> {
    let entries =
        std::fs::read_dir(&roots.profiles_dir)?.collect::<std::result::Result<Vec<_>, _>>()?;
    for entry in entries {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if name == "active_profile.json.part" {
            let _ = std::fs::remove_file(path);
            continue;
        }
        if let Some(final_name) = name.strip_suffix(".profile.tzst.bak") {
            let Ok(profile_id) = Uuid::parse_str(final_name) else {
                continue;
            };
            let final_path = roots
                .profiles_dir
                .join(format!("{final_name}.profile.tzst"));
            let backup_valid = profile_archive_matches(&path, profile_id, Some(spec));
            if !final_path.exists() {
                if backup_valid {
                    std::fs::rename(path, final_path)?;
                } else {
                    std::fs::rename(path, next_archive_conflict_path(&final_path))?;
                }
            } else if profile_archive_matches(&final_path, profile_id, Some(spec)) {
                if backup_valid {
                    let _ = std::fs::remove_file(path);
                } else {
                    std::fs::rename(path, next_archive_conflict_path(&final_path))?;
                }
            } else {
                std::fs::rename(&final_path, next_archive_conflict_path(&final_path))?;
                if backup_valid {
                    std::fs::rename(path, final_path)?;
                } else {
                    std::fs::rename(path, next_archive_conflict_path(&final_path))?;
                }
            }
            continue;
        }
        let Some(profile_stem) = name.strip_suffix(".profile.tzst.part") else {
            continue;
        };
        let Ok(profile_id) = Uuid::parse_str(profile_stem) else {
            continue;
        };
        let loose = roots.profile_path(profile_id);
        let part_valid = profile_archive_matches(&path, profile_id, Some(spec));
        if loose.exists() {
            let loose_is_authoritative = loose.is_dir()
                && read_profile_container_metadata(&loose).is_ok_and(|metadata| {
                    validate_profile_container_metadata(&metadata, spec, profile_id).is_ok()
                });
            if loose_is_authoritative && part_valid {
                let _ = std::fs::remove_file(path);
            } else {
                std::fs::rename(
                    path,
                    next_archive_conflict_path(&roots.archive_path(profile_id)),
                )?;
            }
            continue;
        }
        let final_path = roots.archive_path(profile_id);
        if part_valid {
            if final_path.exists() && profile_archive_matches(&final_path, profile_id, Some(spec)) {
                let _ = std::fs::remove_file(path);
            } else {
                if final_path.exists() {
                    std::fs::rename(&final_path, next_archive_conflict_path(&final_path))?;
                }
                std::fs::rename(path, final_path)?;
            }
        } else {
            std::fs::rename(path, next_archive_conflict_path(&final_path))?;
        }
    }
    Ok(())
}

fn recover_profile_deletions(
    roots: &crate::integrations::profiles::ProfileRoots,
    spec: &ProfileOperationSpec,
) -> std::result::Result<(), ProfileWorkerError> {
    for entry in std::fs::read_dir(&roots.profiles_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".profile.deleting") else {
            continue;
        };
        let Ok(profile_id) = Uuid::parse_str(stem) else {
            continue;
        };
        let path = entry.path();
        let archive = roots.archive_path(profile_id);
        if archive.is_file() && profile_archive_matches(&archive, profile_id, Some(spec)) {
            std::fs::remove_dir_all(path)?;
            continue;
        }
        let canonical = roots.profile_path(profile_id);
        if canonical.exists() {
            std::fs::rename(&path, next_profile_conflict_path(&path, 0))?;
        } else {
            std::fs::rename(path, canonical)?;
        }
    }
    Ok(())
}

fn pending_profile_archive_jobs(
    spec: &ProfileOperationSpec,
    roots: &crate::integrations::profiles::ProfileRoots,
    active_profile_id: Option<Uuid>,
) -> std::result::Result<(Vec<ProfileArchiveJob>, Vec<String>), ProfileWorkerError> {
    let mut jobs = Vec::new();
    let mut warnings = Vec::new();
    for entry in std::fs::read_dir(&roots.profiles_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(".profile") else {
            continue;
        };
        let Ok(profile_id) = Uuid::parse_str(stem) else {
            continue;
        };
        let path = entry.path();
        if active_profile_id == Some(profile_id) {
            let conflict = next_profile_conflict_path(&path, spec.operation_id);
            std::fs::rename(&path, &conflict)?;
            warnings.push(format!(
                "Profile data colliding with the active profile was preserved at {}",
                conflict.display()
            ));
            continue;
        }
        let valid = entry.file_type()?.is_dir()
            && read_profile_container_metadata(&path).is_ok_and(|metadata| {
                metadata.profile_id == profile_id
                    && metadata.game_id == spec.game_id
                    && metadata.backend == spec.game.definition.backend
                    && metadata.format_version != 0
            });
        if valid {
            jobs.push(ProfileArchiveJob {
                game_id: spec.game_id.clone(),
                game: spec.game.clone(),
                use_default_mods_path: spec.use_default_mods_path,
                profile_id,
            });
        } else {
            let conflict = next_profile_conflict_path(&path, spec.operation_id);
            std::fs::rename(&path, &conflict)?;
            warnings.push(format!(
                "Unrecognized profile data was preserved at {}",
                conflict.display()
            ));
        }
    }
    Ok((jobs, warnings))
}

fn validated_profile_archive_metadata(
    path: &Path,
) -> Result<crate::integrations::profiles::ProfileArchiveMetadata> {
    let temp = tempfile::tempdir()?;
    let archive_path = if path.extension().and_then(|extension| extension.to_str()) == Some("tzst")
    {
        path.to_path_buf()
    } else {
        let copy = temp.path().join("recovery.profile.tzst");
        std::fs::copy(path, &copy)?;
        copy
    };
    let extracted = temp.path().join("extracted");
    crate::integrations::profiles::extract_profile_archive(&archive_path, &extracted)
}

fn profile_archive_matches(
    path: &Path,
    profile_id: Uuid,
    spec: Option<&ProfileOperationSpec>,
) -> bool {
    validated_profile_archive_metadata(path).is_ok_and(|metadata| {
        metadata.profile_id == profile_id
            && metadata.format_version != 0
            && spec.is_none_or(|spec| {
                metadata.game_id == spec.game_id && metadata.backend == spec.game.definition.backend
            })
    })
}

#[cfg(test)]
mod profile_worker_tests {
    use super::*;

    fn metadata(profile_id: Uuid, display_name: &str) -> profiles::ProfileArchiveMetadata {
        profiles::ProfileArchiveMetadata {
            format_version: profiles::PROFILE_ARCHIVE_FORMAT_VERSION,
            profile_id,
            game_id: "test".to_string(),
            display_name: display_name.to_string(),
            backend: GameBackend::Xxmi,
            created_at: Utc::now(),
            uncompressed_size: 0,
            file_count: 0,
            portable_metadata: HashMap::new(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
            source_fingerprint: None,
        }
    }

    fn xxmi_game(mods: &Path) -> GameInstall {
        let mut game = crate::model::seeded_games()
            .into_iter()
            .find(|game| game.is_xxmi())
            .unwrap();
        game.definition.id = "test".to_string();
        game.mods_path_override = Some(mods.to_path_buf());
        game
    }

    fn recovery_spec(game: GameInstall) -> ProfileOperationSpec {
        ProfileOperationSpec {
            operation_id: 99,
            game_id: game.definition.id.clone(),
            game,
            use_default_mods_path: false,
            kind: ProfileOperationKind::Recover,
            profile_id: None,
            source_profile_id: None,
            target_profile_id: None,
            display_name: None,
            target_display_name: None,
            source_archive: None,
            target_archive: None,
            target_categories: None,
            target_tools: None,
            target_tool_blacklist: None,
            metadata: None,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicU64::new(0)),
            stage: Arc::new(RwLock::new(String::new())),
        }
    }

    fn tool(id: &str, launch_args: &str) -> ToolEntry {
        ToolEntry {
            id: id.to_string(),
            game_id: "test".to_string(),
            label: id.to_string(),
            path: PathBuf::from(format!("C:\\Mods\\{id}.exe")),
            launch_args: launch_args.to_string(),
            source_mod_id: None,
            auto_detected: true,
            show_in_titlebar: false,
            window_order: 0,
            titlebar_order: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn target_marker_carries_the_profiles_tools_and_blacklist_to_the_ui() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = recovery_spec(xxmi_game(&dir.path().join("Mods")));
        spec.target_profile_id = Some(Uuid::new_v4());
        spec.target_display_name = Some("Warm".to_string());
        spec.target_tools = Some(vec![tool("gimi", "--nogui")]);
        spec.target_tool_blacklist = Some(vec!["c:\\mods\\hidden.exe".to_string()]);

        let marker = target_profile_marker(&spec).unwrap();

        assert_eq!(marker.tools.as_ref().unwrap()[0].launch_args, "--nogui");
        assert_eq!(
            marker.tool_blacklist.as_deref(),
            Some(["c:\\mods\\hidden.exe".to_string()].as_slice())
        );
    }

    #[test]
    fn backfill_prefers_the_catalog_over_the_container_and_adopts_what_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = recovery_spec(xxmi_game(&dir.path().join("Mods")));
        spec.target_tools = Some(vec![tool("gimi", "--from-catalog")]);

        let mut container = metadata(Uuid::new_v4(), "Warm");
        container.tools = Some(vec![tool("gimi", "--from-container")]);
        container.tool_blacklist = Some(vec!["c:\\mods\\hidden.exe".to_string()]);

        backfill_target_profile_data(&mut spec, &container);

        assert_eq!(
            spec.target_tools.as_ref().unwrap()[0].launch_args,
            "--from-catalog",
            "the catalog is authoritative when it already has tools"
        );
        assert_eq!(
            spec.target_tool_blacklist.as_deref(),
            Some(["c:\\mods\\hidden.exe".to_string()].as_slice()),
            "a blacklist the catalog lacks is adopted from the container"
        );
    }

    #[test]
    fn backfill_leaves_pre_tool_archives_without_a_tool_set() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = recovery_spec(xxmi_game(&dir.path().join("Mods")));
        let legacy = metadata(Uuid::new_v4(), "Legacy");

        backfill_target_profile_data(&mut spec, &legacy);

        assert!(
            spec.target_tools.is_none(),
            "None must stay None so the live tool set is left untouched on activation"
        );
        assert!(spec.target_tool_blacklist.is_none());
    }

    #[test]
    fn archive_reuse_is_rejected_when_only_the_tools_changed() {
        let profile_id = Uuid::new_v4();
        let mut existing = metadata(profile_id, "Warm");
        existing.source_fingerprint = Some("fp".to_string());
        existing.tools = Some(vec![tool("gimi", "")]);
        let mut current = existing.clone();
        current.tools = Some(vec![tool("gimi", "--nogui")]);

        assert!(profile_archive_can_be_reused(&existing, &existing, "fp"));
        assert!(
            !profile_archive_can_be_reused(&existing, &current, "fp"),
            "a launch-option edit must rewrite the archive metadata"
        );
    }

    #[test]
    fn foreground_profile_work_resets_the_three_second_archive_grace_period() {
        let coordinator = Arc::new(ProfileArchiveCoordinator::default());
        let before = Instant::now();
        drop(coordinator.pause_for_foreground());
        let state = coordinator.state.lock().unwrap();
        let not_before = state.archive_not_before.unwrap();

        assert_eq!(PROFILE_ARCHIVE_GRACE_PERIOD, Duration::from_secs(3));
        assert!(not_before >= before + PROFILE_ARCHIVE_GRACE_PERIOD);
        assert!(not_before <= Instant::now() + PROFILE_ARCHIVE_GRACE_PERIOD);
    }

    #[test]
    fn profile_deletion_permanently_removes_every_container() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        std::fs::create_dir_all(&mods).unwrap();
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let profile_id = Uuid::new_v4();
        let loose = roots.profile_path(profile_id);
        let archive = roots.archive_path(profile_id);
        let part = roots.archive_part_path(profile_id);
        let backup = roots.archive_backup_path(profile_id);
        let legacy = roots.profiles_dir.join(format!("{profile_id}.tzst"));
        std::fs::create_dir_all(&loose).unwrap();
        std::fs::write(loose.join("payload.bin"), b"profile").unwrap();
        for path in [&archive, &part, &backup, &legacy] {
            std::fs::write(path, b"profile archive").unwrap();
        }
        let mut spec = recovery_spec(game);
        spec.kind = ProfileOperationKind::Delete;
        spec.profile_id = Some(profile_id);

        delete_profile_storage(&spec, &roots).unwrap();

        for path in [loose, archive, part, backup, legacy] {
            assert!(
                !path.exists(),
                "{} should be permanently deleted",
                path.display()
            );
        }
    }

    #[test]
    fn active_profile_loose_collision_is_never_queued_or_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let active_id = Uuid::new_v4();
        let loose = roots.profile_path(active_id);
        std::fs::create_dir_all(loose.join("Mods")).unwrap();
        std::fs::create_dir_all(loose.join("Mods_Archived")).unwrap();
        std::fs::write(loose.join("Mods").join("preserve.txt"), b"preserve").unwrap();
        write_profile_container_metadata(&loose, &metadata(active_id, "Active")).unwrap();
        write_active_profile_marker(
            &roots,
            &ActiveProfileMarker {
                profile_id: active_id,
                display_name: "Active".to_string(),
                categories: Some(Vec::new()),
                tools: None,
                tool_blacklist: None,
            },
        )
        .unwrap();

        let mut on_started = || {};
        let outcome = execute_profile_archive_job(
            &ProfileArchiveJob {
                game_id: game.definition.id.clone(),
                game: game.clone(),
                use_default_mods_path: false,
                profile_id: active_id,
            },
            &Arc::new(ProfileArchiveCoordinator::default()),
            &mut on_started,
        )
        .unwrap();
        assert!(matches!(outcome, ProfileArchiveJobOutcome::Active));
        assert!(loose.join("Mods").join("preserve.txt").is_file());

        let (jobs, warnings) =
            pending_profile_archive_jobs(&recovery_spec(game), &roots, Some(active_id)).unwrap();
        assert!(jobs.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(!loose.exists());
        assert!(
            std::fs::read_dir(&roots.profiles_dir)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{active_id}.profile.conflict-")))
        );
    }

    #[test]
    fn rollback_before_marker_backup_preserves_the_live_marker() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let marker = ActiveProfileMarker {
            profile_id: Uuid::new_v4(),
            display_name: "Active".to_string(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
        };
        write_active_profile_marker(&roots, &marker).unwrap();
        let outgoing = roots.profile_path(Uuid::new_v4());
        std::fs::create_dir_all(outgoing.join("Mods")).unwrap();

        rollback_profile_swap(
            &roots,
            &outgoing,
            &roots.profiles_dir.join("target.profile"),
            false,
        )
        .unwrap();

        let restored: ActiveProfileMarker = serde_json::from_slice(
            &std::fs::read(roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.profile_id, marker.profile_id);
    }

    #[test]
    fn malformed_swap_journal_is_preserved_and_blocks_unsafe_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        let journal = roots.staging_dir().join("0000000000000001.journal");
        std::fs::create_dir_all(roots.staging_dir()).unwrap();
        std::fs::write(&journal, b"{torn").unwrap();

        let error = recover_profile_swap_journals(&roots).unwrap_err();

        assert!(error.to_string().contains("journal is malformed"));
        assert!(journal.is_file());
    }

    #[test]
    fn malformed_swap_journal_is_reported_as_startup_blocking() {
        let temp = tempfile::tempdir().unwrap();
        let game = xxmi_game(&temp.path().join("Mods"));
        let roots = profiles::profile_roots(&game, false).unwrap();
        let journal = roots.staging_dir().join("0000000000000001.journal");
        std::fs::create_dir_all(roots.staging_dir()).unwrap();
        std::fs::write(&journal, b"{torn").unwrap();

        let error = execute_profile_operation(
            recovery_spec(game),
            Arc::new(ProfileArchiveCoordinator::default()),
        )
        .err()
        .expect("malformed journal should fail recovery");

        assert!(error.recovery_blocking());
        assert!(journal.is_file());
    }

    #[test]
    fn recovery_without_a_configured_mods_path_is_a_safe_noop() {
        let game = crate::model::seeded_games()
            .into_iter()
            .find(|game| game.is_unreal_engine())
            .unwrap();

        let output = execute_profile_operation(
            recovery_spec(game),
            Arc::new(ProfileArchiveCoordinator::default()),
        )
        .unwrap();

        assert_eq!(output.kind, ProfileOperationKind::Recover);
        assert!(output.background_archives.is_empty());
        assert!(output.active_profile_marker.is_none());
    }

    #[test]
    fn invalid_loose_collision_does_not_destroy_a_profile_archive_part() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let profile_id = Uuid::new_v4();
        std::fs::create_dir_all(roots.profile_path(profile_id)).unwrap();
        let part = roots.archive_part_path(profile_id);
        std::fs::write(&part, b"preserve staged bytes").unwrap();

        recover_profile_archive_sidecars(&roots, &recovery_spec(game)).unwrap();

        assert!(!part.exists());
        assert!(
            std::fs::read_dir(&roots.profiles_dir)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with(&format!("{profile_id}.profile.conflict-"))
                        && name.ends_with(".tzst")
                })
        );
    }

    #[test]
    fn legacy_profile_archive_sidecars_migrate_to_canonical_names() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let part_id = Uuid::new_v4();
        let backup_id = Uuid::new_v4();
        std::fs::write(
            roots.profiles_dir.join(format!("{part_id}.tzst.part")),
            b"part",
        )
        .unwrap();
        std::fs::write(
            roots.profiles_dir.join(format!("{backup_id}.tzst.bak")),
            b"backup",
        )
        .unwrap();

        migrate_legacy_profile_archives(&roots).unwrap();

        assert_eq!(
            std::fs::read(roots.archive_part_path(part_id)).unwrap(),
            b"part"
        );
        assert_eq!(
            std::fs::read(roots.archive_backup_path(backup_id)).unwrap(),
            b"backup"
        );
    }

    #[test]
    fn sidecar_recovery_never_discards_a_matching_backup_for_a_wrong_id_final() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let expected_id = Uuid::new_v4();
        let wrong_id = Uuid::new_v4();
        let source = temp.path().join("source");
        std::fs::create_dir_all(source.join("Mods")).unwrap();
        std::fs::create_dir_all(source.join("Mods_Archived")).unwrap();
        std::fs::write(source.join("Mods").join("payload.txt"), b"payload").unwrap();
        let source_roots = profile_container_roots(&roots, &source);
        let final_path = roots.archive_path(expected_id);
        profiles::create_profile_archive_with_progress(
            &source_roots,
            &metadata(wrong_id, "Wrong"),
            &final_path,
            None,
        )
        .unwrap();
        let backup_source = roots.profiles_dir.join("matching.profile.tzst");
        profiles::create_profile_archive_with_progress(
            &source_roots,
            &metadata(expected_id, "Expected"),
            &backup_source,
            None,
        )
        .unwrap();
        std::fs::rename(backup_source, roots.archive_backup_path(expected_id)).unwrap();

        recover_profile_archive_sidecars(&roots, &recovery_spec(game)).unwrap();

        let restored = validated_profile_archive_metadata(&final_path).unwrap();
        assert_eq!(restored.profile_id, expected_id);
        assert!(
            std::fs::read_dir(&roots.profiles_dir)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with(&format!("{expected_id}.profile.conflict-"))
                        && name.ends_with(".tzst")
                })
        );
    }

    #[test]
    fn sidecar_recovery_never_discards_a_matching_part_for_a_wrong_id_final() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let expected_id = Uuid::new_v4();
        let wrong_id = Uuid::new_v4();
        let source = temp.path().join("source");
        std::fs::create_dir_all(source.join("Mods")).unwrap();
        std::fs::create_dir_all(source.join("Mods_Archived")).unwrap();
        std::fs::write(source.join("Mods").join("payload.txt"), b"payload").unwrap();
        let source_roots = profile_container_roots(&roots, &source);
        let final_path = roots.archive_path(expected_id);
        profiles::create_profile_archive_with_progress(
            &source_roots,
            &metadata(wrong_id, "Wrong"),
            &final_path,
            None,
        )
        .unwrap();
        let part_source = roots.profiles_dir.join("matching.profile.tzst");
        profiles::create_profile_archive_with_progress(
            &source_roots,
            &metadata(expected_id, "Expected"),
            &part_source,
            None,
        )
        .unwrap();
        std::fs::rename(part_source, roots.archive_part_path(expected_id)).unwrap();

        recover_profile_archive_sidecars(&roots, &recovery_spec(game)).unwrap();

        let restored = validated_profile_archive_metadata(&final_path).unwrap();
        assert_eq!(restored.profile_id, expected_id);
        assert!(
            std::fs::read_dir(&roots.profiles_dir)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with(&format!("{expected_id}.profile.conflict-"))
                        && name.ends_with(".tzst")
                })
        );
    }

    #[test]
    fn switch_prefers_a_valid_loose_profile_over_its_archive_baseline() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let current_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let loose = roots.profile_path(target_id);
        std::fs::create_dir_all(loose.join("Mods")).unwrap();
        std::fs::create_dir_all(loose.join("Mods_Archived")).unwrap();
        std::fs::write(loose.join("Mods").join("warm.txt"), b"warm").unwrap();
        write_profile_container_metadata(&loose, &metadata(target_id, "Warm")).unwrap();
        let archive = roots.archive_path(target_id);
        std::fs::write(&archive, b"archive baseline must remain untouched").unwrap();
        let mut spec = ProfileOperationSpec {
            operation_id: 11,
            game_id: game.definition.id.clone(),
            game,
            use_default_mods_path: false,
            kind: ProfileOperationKind::Switch,
            profile_id: Some(current_id),
            source_profile_id: None,
            target_profile_id: Some(target_id),
            display_name: None,
            target_display_name: Some("Warm".to_string()),
            source_archive: None,
            target_archive: Some(archive.clone()),
            target_categories: None,
            target_tools: None,
            target_tool_blacklist: None,
            metadata: Some(metadata(current_id, "Current")),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(AtomicU64::new(0)),
            stage: Arc::new(RwLock::new(String::new())),
        };
        let staging = roots.staging_dir().join("switch.extracting");
        let mut warnings = Vec::new();

        let source =
            prepare_switch_target(&mut spec, &roots, &staging, target_id, &mut warnings).unwrap();

        assert_eq!(source, loose);
        assert!(warnings.is_empty());
        assert_eq!(
            std::fs::read(archive).unwrap(),
            b"archive baseline must remain untouched"
        );
    }

    #[test]
    fn background_archive_replaces_a_stale_baseline_then_removes_loose_profile() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let profile_id = Uuid::new_v4();
        let archive = roots.archive_path(profile_id);

        let old_source = temp.path().join("old");
        std::fs::create_dir_all(old_source.join("Mods")).unwrap();
        std::fs::create_dir_all(old_source.join("Mods_Archived")).unwrap();
        std::fs::write(old_source.join("Mods").join("payload.txt"), b"old").unwrap();
        let old_roots = profile_container_roots(&roots, &old_source);
        profiles::create_profile_archive_with_progress(
            &old_roots,
            &metadata(profile_id, "Profile"),
            &archive,
            None,
        )
        .unwrap();

        let loose = roots.profile_path(profile_id);
        std::fs::create_dir_all(loose.join("Mods")).unwrap();
        std::fs::create_dir_all(loose.join("Mods_Archived")).unwrap();
        std::fs::write(loose.join("Mods").join("payload.txt"), b"new payload").unwrap();
        write_profile_container_metadata(&loose, &metadata(profile_id, "Profile")).unwrap();
        let mut started = false;
        let mut on_started = || started = true;
        let outcome = execute_profile_archive_job(
            &ProfileArchiveJob {
                game_id: game.definition.id.clone(),
                game,
                use_default_mods_path: false,
                profile_id,
            },
            &Arc::new(ProfileArchiveCoordinator::default()),
            &mut on_started,
        )
        .unwrap();

        assert!(started);
        assert!(matches!(outcome, ProfileArchiveJobOutcome::Completed(_)));
        assert!(!loose.exists());
        let extracted = temp.path().join("extracted");
        profiles::extract_profile_archive(&archive, &extracted).unwrap();
        assert_eq!(
            std::fs::read(extracted.join("Mods").join("payload.txt")).unwrap(),
            b"new payload"
        );
    }

    #[test]
    fn recovery_finishes_or_rolls_back_interrupted_loose_profile_deletion() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();

        let restore_id = Uuid::new_v4();
        let restore = roots
            .profiles_dir
            .join(format!("{restore_id}.profile.deleting"));
        std::fs::create_dir_all(restore.join("Mods")).unwrap();
        std::fs::write(restore.join("Mods").join("restore.txt"), b"restore").unwrap();

        let finish_id = Uuid::new_v4();
        let finish = roots
            .profiles_dir
            .join(format!("{finish_id}.profile.deleting"));
        std::fs::create_dir_all(finish.join("Mods")).unwrap();
        std::fs::write(finish.join("Mods").join("discard.txt"), b"discard").unwrap();
        let finish_roots = profile_container_roots(&roots, &finish);
        profiles::create_profile_archive_with_progress(
            &finish_roots,
            &metadata(finish_id, "Finished"),
            &roots.archive_path(finish_id),
            None,
        )
        .unwrap();

        recover_profile_deletions(&roots, &recovery_spec(xxmi_game(&roots.mods))).unwrap();

        assert!(
            roots
                .profile_path(restore_id)
                .join("Mods")
                .join("restore.txt")
                .is_file()
        );
        assert!(!finish.exists());
        assert!(roots.archive_path(finish_id).is_file());
    }

    #[test]
    fn profile_swap_activates_target_and_keeps_outgoing_loose() {
        let temp = tempfile::tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let mods = temp.path().join("Mods");
        let archived = temp.path().join("Mods_Archived");
        let roots = profiles::ProfileRoots {
            profiles_dir: profiles_dir.clone(),
            mods: mods.clone(),
            archived: Some(archived.clone()),
            disabled: None,
        };
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&archived).unwrap();
        std::fs::write(mods.join("current.txt"), b"current").unwrap();
        std::fs::write(archived.join("current-old.txt"), b"current-old").unwrap();
        std::fs::create_dir_all(&profiles_dir).unwrap();

        let outgoing_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let target = profiles_dir.join("target");
        std::fs::create_dir_all(target.join("Mods")).unwrap();
        std::fs::create_dir_all(target.join("Mods_Archived")).unwrap();
        std::fs::write(target.join("Mods").join("target.txt"), b"target").unwrap();
        std::fs::write(
            target.join("Mods_Archived").join("target-old.txt"),
            b"target-old",
        )
        .unwrap();
        let old_marker = ActiveProfileMarker {
            profile_id: outgoing_id,
            display_name: "Current".to_string(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
        };
        write_active_profile_marker(&roots, &old_marker).unwrap();
        let target_marker = ActiveProfileMarker {
            profile_id: target_id,
            display_name: "Target".to_string(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
        };
        let mut warnings = Vec::new();

        swap_roots(
            &roots,
            &target,
            1,
            Some((outgoing_id, metadata(outgoing_id, "Current"))),
            &target_marker,
            &mut warnings,
        )
        .unwrap();

        assert!(warnings.is_empty());
        assert_eq!(std::fs::read(mods.join("target.txt")).unwrap(), b"target");
        assert_eq!(
            std::fs::read(archived.join("target-old.txt")).unwrap(),
            b"target-old"
        );
        let outgoing = roots.profile_path(outgoing_id);
        assert_eq!(
            std::fs::read(outgoing.join("Mods").join("current.txt")).unwrap(),
            b"current"
        );
        assert_eq!(
            std::fs::read(outgoing.join("Mods_Archived").join("current-old.txt")).unwrap(),
            b"current-old"
        );
        assert!(outgoing.join(profiles::PROFILE_METADATA_FILE).is_file());
        assert!(!target.exists());
        let active: ActiveProfileMarker = serde_json::from_slice(
            &std::fs::read(profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(active.profile_id, target_id);
    }

    #[test]
    fn profile_swap_quarantines_an_existing_outgoing_container() {
        let temp = tempfile::tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let mods = temp.path().join("Mods");
        let roots = profiles::ProfileRoots {
            profiles_dir: profiles_dir.clone(),
            mods: mods.clone(),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("current.txt"), b"current").unwrap();
        let outgoing_id = Uuid::new_v4();
        let existing = roots.profile_path(outgoing_id);
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("collision.txt"), b"preserve").unwrap();
        let target_id = Uuid::new_v4();
        let target = profiles_dir.join("target");
        std::fs::create_dir_all(target.join("Mods")).unwrap();
        let mut warnings = Vec::new();

        swap_roots(
            &roots,
            &target,
            7,
            Some((outgoing_id, metadata(outgoing_id, "Current"))),
            &ActiveProfileMarker {
                profile_id: target_id,
                display_name: "Target".to_string(),
                categories: Some(Vec::new()),
                tools: None,
                tool_blacklist: None,
            },
            &mut warnings,
        )
        .unwrap();

        assert_eq!(warnings.len(), 1);
        let conflict = profiles_dir.join(format!("{outgoing_id}.profile.conflict-{:016x}", 7));
        assert_eq!(
            std::fs::read(conflict.join("collision.txt")).unwrap(),
            b"preserve"
        );
        assert!(
            roots
                .profile_path(outgoing_id)
                .join("Mods")
                .join("current.txt")
                .is_file()
        );
    }

    #[test]
    fn profile_rollback_preserves_roots_not_yet_backed_up() {
        let temp = tempfile::tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let outgoing = profiles_dir.join("old.profile");
        let target = profiles_dir.join("target.profile");
        let mods = temp.path().join("Mods");
        let archived = temp.path().join("Mods_Archived");
        std::fs::create_dir_all(outgoing.join("Mods")).unwrap();
        std::fs::write(outgoing.join("Mods").join("old.txt"), b"old").unwrap();
        std::fs::create_dir_all(&archived).unwrap();
        std::fs::write(archived.join("untouched.txt"), b"untouched").unwrap();
        let roots = crate::integrations::profiles::ProfileRoots {
            profiles_dir,
            mods: mods.clone(),
            archived: Some(archived.clone()),
            disabled: None,
        };
        rollback_profile_swap(&roots, &outgoing, &target, false).unwrap();

        assert!(mods.join("old.txt").is_file());
        assert!(archived.join("untouched.txt").is_file());
    }

    #[test]
    fn profile_staging_cleanup_removes_empty_root_but_preserves_recovery_journal() {
        let temp = tempfile::tempdir().unwrap();
        let roots = crate::integrations::profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        let operation_id = 42;
        let staging = roots.staging_dir();
        let extracting = staging.join(format!("{operation_id:016x}.extracting"));
        std::fs::create_dir_all(&extracting).unwrap();
        drop(ProfileStagingCleanup::new(&roots, operation_id));
        assert!(!staging.exists());

        std::fs::create_dir_all(&extracting).unwrap();
        std::fs::write(staging.join("recovery.journal"), "installing").unwrap();
        drop(ProfileStagingCleanup::new(&roots, operation_id));
        assert!(!extracting.exists());
        assert!(staging.join("recovery.journal").is_file());
    }

    #[test]
    fn profile_rollback_removes_new_root_when_original_was_absent() {
        let temp = tempfile::tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let outgoing = profiles_dir.join("old.profile");
        let target = profiles_dir.join("target.profile");
        let mods = temp.path().join("Mods");
        let archived = temp.path().join("Mods_Archived");
        std::fs::create_dir_all(outgoing.join("Mods")).unwrap();
        std::fs::write(outgoing.join("Mods").join("old.txt"), b"old").unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("new.txt"), b"new").unwrap();
        std::fs::create_dir_all(&archived).unwrap();
        std::fs::write(archived.join("new.txt"), b"new").unwrap();
        let roots = crate::integrations::profiles::ProfileRoots {
            profiles_dir,
            mods: mods.clone(),
            archived: Some(archived.clone()),
            disabled: None,
        };
        rollback_profile_swap(&roots, &outgoing, &target, true).unwrap();

        assert!(mods.join("old.txt").is_file());
        assert!(!mods.join("new.txt").exists());
        assert!(!archived.exists());
        assert!(target.join("Mods_Archived").join("new.txt").is_file());
    }

    #[test]
    fn profile_rollback_restores_previous_active_marker() {
        let temp = tempfile::tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let outgoing = profiles_dir.join("old.profile");
        let target = profiles_dir.join("target.profile");
        let mods = temp.path().join("Mods");
        std::fs::create_dir_all(&profiles_dir).unwrap();
        std::fs::create_dir_all(outgoing.join("Mods")).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        let old = ActiveProfileMarker {
            profile_id: Uuid::new_v4(),
            display_name: "Old".to_string(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
        };
        let new = ActiveProfileMarker {
            profile_id: Uuid::new_v4(),
            display_name: "New".to_string(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
        };
        std::fs::write(
            outgoing.join(ACTIVE_PROFILE_MARKER_FILE),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        std::fs::write(
            profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE),
            serde_json::to_vec(&new).unwrap(),
        )
        .unwrap();
        let roots = crate::integrations::profiles::ProfileRoots {
            profiles_dir: profiles_dir.clone(),
            mods: mods.clone(),
            archived: None,
            disabled: None,
        };

        rollback_profile_swap(&roots, &outgoing, &target, true).unwrap();

        let restored: ActiveProfileMarker = serde_json::from_slice(
            &std::fs::read(profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.profile_id, old.profile_id);
    }
}
