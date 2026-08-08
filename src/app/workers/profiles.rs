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
                        completed: Box::new(ProfileCompleted {
                            game_id,
                            kind: output.kind,
                            profile_id: output.profile_id,
                            target_profile_id: output.target_profile_id,
                            display_name: output.display_name,
                            archive: output.archive,
                            active_profile_marker: output.active_profile_marker,
                            orphaned_profiles: output.orphaned_profiles,
                            renamed_profiles: output.renamed_profiles,
                            duplicate_profiles: output.duplicate_profiles,
                            warnings: output.warnings,
                        }),
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
    profile_id: Option<ProfileId>,
    target_profile_id: Option<ProfileId>,
    display_name: Option<String>,
    archive: Option<crate::integrations::profiles::ArchiveResult>,
    active_profile_marker: Option<ActiveProfileMarker>,
    background_archives: Vec<ProfileArchiveJob>,
    orphaned_profiles: Vec<OrphanedProfile>,
    renamed_profiles: Vec<RecoveredProfileLabel>,
    duplicate_profiles: Vec<ProfileDuplicateEntry>,
    warnings: Vec<String>,
}

enum ProfileArchiveJobOutcome {
    Paused,
    Missing,
    Completed(crate::integrations::profiles::ArchiveResult),
}

const LEGACY_ACTIVE_PROFILE_MARKER_FILE: &str = "active_profile.json";
const PROFILE_ARCHIVE_GRACE_PERIOD: Duration = Duration::from_secs(3);
const PROFILE_SWITCH_FINISH_DELAY: Duration = Duration::from_millis(1_300);
const PROFILE_SWITCH_FINISH_PROGRESS_START: u64 = 70;
const PROFILE_SWITCH_FINISH_PROGRESS_STEPS: u32 = 25;
/// Preparing the target profile owns the 10..60 band, whether that means extracting an archive or
/// copying a directory tree. The UI maps the same band onto one timeline row.
const PROFILE_PREPARE_PROGRESS_START: u64 = 10;
const PROFILE_PREPARE_PROGRESS_SPAN: u64 = 50;

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
    extracting_dir: PathBuf,
    legacy_staging_dir: PathBuf,
}

impl ProfileStagingCleanup {
    fn new(roots: &crate::integrations::profiles::ProfileRoots, spec: &ProfileOperationSpec) -> Self {
        Self {
            extracting_dir: profile_extracting_path(roots, spec),
            legacy_staging_dir: roots.staging_dir(),
        }
    }
}

impl Drop for ProfileStagingCleanup {
    fn drop(&mut self) {
        // Extraction staging is disposable on every terminal outcome. Journals are preserved
        // when recovery is still required, so only the extracting folder is removed here.
        let _ = std::fs::remove_dir_all(&self.extracting_dir);
        let _ = std::fs::remove_dir(&self.legacy_staging_dir);
    }
}

fn profile_operation_storage_stem(spec: &ProfileOperationSpec) -> String {
    if let (Some(profile_id), Some(display_name)) = (spec.target_profile_id, spec.target_display_name.as_deref()) {
        return crate::integrations::profiles::profile_storage_stem(display_name, profile_id);
    }
    if let (Some(profile_id), Some(display_name)) = (spec.profile_id, spec.display_name.as_deref()) {
        return crate::integrations::profiles::profile_storage_stem(display_name, profile_id);
    }
    format!("Profile operation {:016x}", spec.operation_id)
}

fn profile_extracting_path(
    roots: &crate::integrations::profiles::ProfileRoots,
    spec: &ProfileOperationSpec,
) -> PathBuf {
    roots
        .profiles_dir
        .join(format!("{}.extracting", profile_operation_storage_stem(spec)))
}

fn profile_journal_path(
    roots: &crate::integrations::profiles::ProfileRoots,
    marker: &ActiveProfileMarker,
) -> PathBuf {
    roots.profiles_dir.join(format!(
        "{}.journal",
        crate::integrations::profiles::profile_storage_stem(
            &marker.display_name,
            marker.profile_id
        )
    ))
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
                orphaned_profiles: Vec::new(),
                renamed_profiles: Vec::new(),
                duplicate_profiles: Vec::new(),
                warnings: Vec::new(),
            });
        }
        Err(error) => return Err(ProfileWorkerError::Other(error)),
    };
    let _staging_cleanup = ProfileStagingCleanup::new(&roots, &spec);
    match spec.kind {
        ProfileOperationKind::Recover => {
            profiles::ensure_profile_storage_layout(&roots)
                .map_err(ProfileWorkerError::RecoveryBlocked)?;
            let mut warnings = Vec::new();
            let recovered_active = recover_profile_staging(&roots, &spec, &mut warnings)
                .map_err(ProfileWorkerError::into_recovery_blocking)?;
            let (background_archives, archive_warnings) =
                pending_profile_archive_jobs(&spec, &roots)
                    .map_err(ProfileWorkerError::into_recovery_blocking)?;
            warnings.extend(archive_warnings);
            // After the recovery above, so legacy names and interrupted sidecars have already been
            // normalized into their canonical form and are recognizable here.
            let (orphaned_profiles, renamed_profiles, duplicate_profiles) =
                discover_orphaned_profiles(&roots, &spec, &mut warnings)
                    .map_err(ProfileWorkerError::into_recovery_blocking)?;
            return Ok(ProfileWorkerOutput {
                kind: spec.kind,
                profile_id: None,
                target_profile_id: recovered_active.as_ref().map(|marker| marker.profile_id),
                display_name: recovered_active
                    .as_ref()
                    .map(|marker| marker.display_name.clone()),
                archive: None,
                active_profile_marker: recovered_active,
                background_archives,
                orphaned_profiles,
                renamed_profiles,
                duplicate_profiles,
                warnings,
            });
        }
        _ => {}
    }
    profiles::ensure_profile_storage_layout(&roots).map_err(ProfileWorkerError::Other)?;

    let operation_id = spec.operation_id;
    let staging = profile_extracting_path(&roots, &spec);
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
        ProfileOperationKind::Reidentify => {
            reidentify_duplicate_profile(&spec, &roots)?;
        }
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
        orphaned_profiles: Vec::new(),
        renamed_profiles: Vec::new(),
        duplicate_profiles: Vec::new(),
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
    Option<(ProfileId, crate::integrations::profiles::ProfileArchiveMetadata)>,
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
    expected_profile_id: ProfileId,
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
    target_profile_id: ProfileId,
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
    source_profile_id: ProfileId,
    warnings: &mut Vec<String>,
) -> std::result::Result<PathBuf, ProfileWorkerError> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    // Start at the floor of the preparation band, not above it: `extract_to_staging` sets the same
    // floor when it takes over below, and a higher value here would make the bar jump backwards.
    update_profile_progress(
        &spec.progress,
        &spec.stage,
        PROFILE_PREPARE_PROGRESS_START,
        "Preparing selected profile",
    );

    if spec.profile_id == Some(source_profile_id) {
        // Claim the copy label before sizing the tree; on a large profile that walk alone takes
        // long enough that the row would otherwise sit on the wrong wording.
        update_profile_progress(
            &spec.progress,
            &spec.stage,
            PROFILE_PREPARE_PROGRESS_START,
            "Copying selected profile",
        );
        let total_bytes = profile_tree_size(roots)?;
        crate::integrations::profiles::ensure_profile_space(
            &roots.profiles_dir,
            total_bytes.saturating_add(64 * 1024 * 1024),
        )?;
        copy_profile_roots(
            roots,
            &profile_container_roots(roots, staging),
            &spec.cancel,
            &mut copy_progress_reporter(spec, total_bytes),
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
            update_profile_progress(
                &spec.progress,
                &spec.stage,
                PROFILE_PREPARE_PROGRESS_START,
                "Copying selected profile",
            );
            let total_bytes = profile_tree_size(&loose_roots)?;
            crate::integrations::profiles::ensure_profile_space(
                &roots.profiles_dir,
                total_bytes.saturating_add(64 * 1024 * 1024),
            )?;
            crate::importing::copy_dir_cancelable_with_progress(
                &loose,
                staging,
                true,
                &spec.cancel,
                &mut copy_progress_reporter(spec, total_bytes),
            )?;
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
    on_bytes: &mut dyn FnMut(u64) -> Result<()>,
) -> std::result::Result<(), ProfileWorkerError> {
    // Each root restarts `copy_dir_cancelable_with_progress`'s own counter, so carry a running
    // total across them or the caller's percentage would reset per root.
    let mut copied_before_root = 0u64;
    for ((_, source), (_, destination)) in root_locations(source)
        .into_iter()
        .zip(root_locations(destination))
    {
        if source.exists() {
            let mut root_total = 0u64;
            crate::importing::copy_dir_cancelable_with_progress(
                &source,
                &destination,
                true,
                cancel,
                &mut |bytes| {
                    root_total = bytes;
                    on_bytes(copied_before_root.saturating_add(bytes))
                },
            )?;
            copied_before_root = copied_before_root.saturating_add(root_total);
        } else {
            std::fs::create_dir_all(destination)?;
        }
    }
    Ok(())
}

/// Feed the shared 10..60 preparation band from a byte count. `total_bytes` of 0 means the size
/// probe found nothing to copy, so the band stays at its floor rather than dividing by zero.
fn copy_progress_reporter<'a>(
    spec: &'a ProfileOperationSpec,
    total_bytes: u64,
) -> impl FnMut(u64) -> Result<()> + 'a {
    move |copied| {
        if spec.cancel.load(Ordering::Relaxed) {
            bail!("profile operation canceled");
        }
        let pct = if total_bytes == 0 {
            PROFILE_PREPARE_PROGRESS_START
        } else {
            PROFILE_PREPARE_PROGRESS_START
                + (copied.saturating_mul(PROFILE_PREPARE_PROGRESS_SPAN) / total_bytes)
                    .min(PROFILE_PREPARE_PROGRESS_SPAN)
        };
        if spec.progress.load(Ordering::Relaxed) != pct {
            update_profile_progress(&spec.progress, &spec.stage, pct, "Copying selected profile");
        }
        Ok(())
    }
}

/// Move a renamed profile's stored data to match its new name.
///
/// Best effort by design: the label is decoration and lookups key on the short id, so a failure
/// here leaves a stale name and nothing more. The profile itself is renamed in the catalog either
/// way, and the next compression writes the corrected name.
fn rename_profile_storage_label(
    roots: &crate::integrations::profiles::ProfileRoots,
    profile_id: ProfileId,
    display_name: &str,
) {
    for archive in [false, true] {
        let Some(current) = roots.find_profile_storage(profile_id, archive) else {
            continue;
        };
        let canonical = if archive {
            roots.archive_path_for(profile_id, display_name)
        } else {
            roots.profile_path_for(profile_id, display_name)
        };
        if canonical == current || canonical.exists() {
            continue;
        }
        // Ignoring the error is deliberate: a locked file leaves a stale label, which costs
        // nothing because lookups key on the short id.
        let _ = std::fs::rename(&current, &canonical);
    }
}

/// Rewrite a duplicated archive under a fresh identity so both copies can be kept.
///
/// The source is left exactly as it is: the copy becomes the new profile, so a failure part-way
/// costs nothing but the partial output, which the repack removes itself.
fn reidentify_duplicate_profile(
    spec: &ProfileOperationSpec,
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<(), ProfileWorkerError> {
    let source = spec
        .reidentify_source
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no archive to re-identify"))?;
    let target_id = spec
        .target_profile_id
        .ok_or_else(|| anyhow::anyhow!("target profile id is missing"))?;
    let display_name = spec
        .target_display_name
        .clone()
        .unwrap_or_else(|| "Profile".to_string());
    let destination = roots.archive_path_for(target_id, &display_name);
    crate::integrations::profiles::ensure_profile_space(
        &roots.profiles_dir,
        std::fs::metadata(source).map(|meta| meta.len()).unwrap_or(0),
    )?;
    let mut report = |copied: u64, total: u64| -> Result<()> {
        if spec.cancel.load(Ordering::Relaxed) {
            bail!("profile operation canceled");
        }
        let pct = if total == 0 {
            PROFILE_PREPARE_PROGRESS_START
        } else {
            PROFILE_PREPARE_PROGRESS_START
                + (copied.saturating_mul(90 - PROFILE_PREPARE_PROGRESS_START) / total)
                    .min(90 - PROFILE_PREPARE_PROGRESS_START)
        };
        if spec.progress.load(Ordering::Relaxed) != pct {
            update_profile_progress(&spec.progress, &spec.stage, pct, "Generating new profile ID");
        }
        Ok(())
    };
    update_profile_progress(
        &spec.progress,
        &spec.stage,
        PROFILE_PREPARE_PROGRESS_START,
        "Generating new profile ID",
    );
    crate::integrations::profiles::reidentify_profile_archive(
        source,
        &destination,
        target_id,
        &display_name,
        Some(&mut report),
    )?;
    update_profile_progress(&spec.progress, &spec.stage, 100, "Profile ID generated");
    Ok(())
}

fn delete_profile_storage(
    spec: &ProfileOperationSpec,
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<(), ProfileWorkerError> {
    let profile_id = spec
        .profile_id
        .ok_or_else(|| anyhow::anyhow!("profile id is missing"))?;
    remove_profile_storage_entries(roots, profile_id)?;
    Ok(())
}

fn remove_profile_storage_entries(
    roots: &crate::integrations::profiles::ProfileRoots,
    profile_id: ProfileId,
) -> std::io::Result<()> {
    // Sweep every entry carrying this profile's short id rather than only the one the resolver
    // would pick: hand-made copies should not survive explicit cleanup and get re-adopted later.
    let mut paths = vec![
        roots.archive_part_path(profile_id),
        roots.archive_backup_path(profile_id),
    ];
    if let Ok(entries) = std::fs::read_dir(&roots.profiles_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let archive_suffix = format!(
                ".{}",
                crate::integrations::profiles::PROFILE_ARCHIVE_EXTENSION
            );
            let stem = name
                .strip_suffix(&format!("{archive_suffix}.part"))
                .or_else(|| name.strip_suffix(&format!("{archive_suffix}.bak")))
                .or_else(|| name.strip_suffix(&archive_suffix))
                .or_else(|| name.strip_suffix(".deleting"))
                .unwrap_or(name.as_ref());
            let legacy_deleting = name
                .strip_suffix(".profile.deleting")
                .and_then(|stem| stem.parse::<ProfileId>().ok())
                == Some(profile_id);
            if legacy_deleting || profile_storage_or_conflict_stem_matches(stem, profile_id) {
                paths.push(entry.path());
            }
        }
    }
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

fn remove_profile_conflict_entries(
    roots: &crate::integrations::profiles::ProfileRoots,
    profile_id: ProfileId,
) -> std::io::Result<()> {
    if let Ok(entries) = std::fs::read_dir(&roots.profiles_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let archive_suffix = format!(
                ".{}",
                crate::integrations::profiles::PROFILE_ARCHIVE_EXTENSION
            );
            let Some(stem) = name.strip_suffix(&archive_suffix) else {
                continue;
            };
            let Some((base_stem, _)) = stem.split_once(".conflict-") else {
                continue;
            };
            if crate::integrations::profiles::profile_storage_stem_matches(base_stem, profile_id) {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn profile_storage_or_conflict_stem_matches(stem: &str, profile_id: ProfileId) -> bool {
    let live_stem = stem
        .split_once(".conflict-")
        .map(|(base, _)| base)
        .unwrap_or(stem);
    crate::integrations::profiles::profile_storage_stem_matches(live_stem, profile_id)
}

fn archive_sidecar_path(archive: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = archive.to_path_buf();
    let extension = archive
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or(crate::integrations::profiles::PROFILE_ARCHIVE_EXTENSION);
    sidecar.set_extension(format!("{extension}.{suffix}"));
    sidecar
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
    // The container's own metadata is the authority on the label, so a profile renamed while it was
    // loose gets the corrected name the moment it is compressed.
    let destination = roots.archive_path_for(job.profile_id, &metadata.display_name);

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
                        let bytes = std::fs::metadata(&destination)?.len();
                        remove_loose_profile_after_archive(&roots, job.profile_id, &loose)?;
                        remove_profile_conflict_entries(&roots, job.profile_id)?;
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
            let _ = std::fs::remove_file(archive_sidecar_path(&destination, "part"));
            return Ok(ProfileArchiveJobOutcome::Paused);
        }
        Err(error) => {
            let _ = std::fs::remove_file(archive_sidecar_path(&destination, "part"));
            return Err(error);
        }
    };
    if coordinator.foreground_requested() {
        return Ok(ProfileArchiveJobOutcome::Paused);
    }
    remove_loose_profile_after_archive(&roots, job.profile_id, &loose)?;
    remove_profile_conflict_entries(&roots, job.profile_id)?;
    Ok(ProfileArchiveJobOutcome::Completed(archive))
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
    profile_id: ProfileId,
    loose: &Path,
) -> Result<()> {
    let deleting = profile_deleting_path(roots, profile_id);
    if deleting.exists() {
        let conflict = next_profile_conflict_path(&deleting, 0);
        std::fs::rename(&deleting, conflict)?;
    }
    std::fs::rename(loose, &deleting)?;
    std::fs::remove_dir_all(deleting)?;
    Ok(())
}

fn profile_deleting_path(
    roots: &crate::integrations::profiles::ProfileRoots,
    profile_id: ProfileId,
) -> PathBuf {
    let loose = roots.profile_path(profile_id);
    let file_name = loose
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.deleting"))
        .unwrap_or_else(|| format!("Profile [{profile_id}].deleting"));
    roots.profiles_dir.join(file_name)
}

fn profile_archive_can_be_reused(
    existing: &crate::integrations::profiles::ProfileArchiveMetadata,
    current: &crate::integrations::profiles::ProfileArchiveMetadata,
    fingerprint: &str,
) -> bool {
    // `display_name` is deliberately absent: the storage name carries the current name and is
    // updated by the rename itself, so a rename must not cost a full recompression of the payload.
    // The embedded name is only a fallback for a profile whose catalog record was lost.
    existing.source_fingerprint.as_deref() == Some(fingerprint)
        && existing.profile_id == current.profile_id
        && existing.game_id == current.game_id
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
    outgoing_profile: Option<(ProfileId, crate::integrations::profiles::ProfileArchiveMetadata)>,
    marker: &ActiveProfileMarker,
    warnings: &mut Vec<String>,
) -> std::result::Result<(), ProfileWorkerError> {
    let (outgoing_id, outgoing_metadata) = outgoing_profile
        .ok_or_else(|| anyhow::anyhow!("active profile is missing during profile switch"))?;
    let journal = profile_journal_path(roots, marker);
    std::fs::create_dir_all(&roots.profiles_dir)?;
    // Name the outgoing container from the profile being archived, not the resolver fallback:
    // nothing exists on disk for it yet, so this is the moment its label is decided.
    let outgoing = roots.profile_path_for(outgoing_id, &outgoing_metadata.display_name);
    // Match on the short id, not the label: data left by an earlier interrupted swap may carry a
    // different label and would otherwise be silently overwritten.
    if let Some(existing) = roots
        .find_profile_storage(outgoing_id, false)
        .or_else(|| outgoing.exists().then(|| outgoing.clone()))
    {
        let conflict = next_profile_conflict_path(&existing, operation_id);
        std::fs::rename(&existing, &conflict)?;
        warnings.push(format!(
            "Existing profile data was preserved at {}",
            conflict.display()
        ));
    }
    let mut journal_state = ProfileSwapJournal {
        phase: ProfileSwapPhase::BackingUp,
        outgoing_profile_id: Some(outgoing_id),
        target_profile: Some(marker.clone()),
        outgoing: outgoing.clone(),
        target_source: target_source.to_path_buf(),
    };
    write_profile_swap_journal(&journal, &journal_state)?;
    if let Err(error) = write_profile_container_metadata(&outgoing, &outgoing_metadata) {
        let _ = std::fs::remove_dir_all(&outgoing);
        let _ = std::fs::remove_file(&journal);
        return Err(error);
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
    journal_state.phase = ProfileSwapPhase::Committed;
    if let Err(error) = write_profile_swap_journal(&journal, &journal_state) {
        rollback_profile_swap(roots, &outgoing, target_source, true).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {error}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_file(&journal);
        return Err(error);
    }

    if let Err(error) = remove_profile_storage_entries(roots, marker.profile_id) {
        warnings.push(format!(
            "Active profile storage cleanup could not remove all stale entries: {error}"
        ));
    }
    match std::fs::remove_dir_all(target_source) {
        Ok(()) => {
            let _ = std::fs::remove_file(&journal);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = std::fs::remove_file(&journal);
        }
        Err(_) => {}
    }
    Ok(())
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
    #[serde(default)]
    outgoing_profile_id: Option<ProfileId>,
    #[serde(default)]
    target_profile: Option<ActiveProfileMarker>,
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
    let _ = std::fs::remove_dir_all(outgoing);
    Ok(())
}

fn recover_profile_staging(
    roots: &crate::integrations::profiles::ProfileRoots,
    spec: &ProfileOperationSpec,
    warnings: &mut Vec<String>,
) -> std::result::Result<Option<ActiveProfileMarker>, ProfileWorkerError> {
    std::fs::create_dir_all(&roots.profiles_dir)?;
    let recovered_active = recover_profile_swap_journals(roots)?;
    recover_profile_archive_sidecars(roots, spec)?;
    recover_profile_conflicts(roots, spec, warnings)?;
    recover_profile_deletions(roots, spec)?;
    remove_legacy_active_profile_marker(roots)?;
    Ok(recovered_active)
}

/// Classify a storage entry name, rejecting every sidecar and quarantine variant (`.part`, `.bak`,
/// `.deleting`, `.conflict-*`) so only live profile data is considered.
///
fn classify_profile_storage(name: &str) -> Option<ProfileStorageKind> {
    use crate::integrations::profiles::{PROFILE_ARCHIVE_EXTENSION, parse_profile_storage_stem};
    // Any archive counts: putting a .tzst in this folder is an unambiguous statement of intent, and
    // its metadata is checked before anything is done with it. In-flight writes are `.part` and
    // replaced originals are `.bak`, so anything still ending in .tzst is complete.
    if let Some(stem) = name.strip_suffix(&format!(".{PROFILE_ARCHIVE_EXTENSION}")) {
        // Quarantined copies are excluded: they were set aside precisely because they did not
        // belong where they were found, and re-adopting one would shadow the good copy.
        if stem.contains(".conflict-") {
            return None;
        }
        return Some(ProfileStorageKind::Archive);
    }
    // Folders are stricter. Their dangerous lookalikes — `.deleting`, `.conflict-*` — hold valid
    // metadata too, so content cannot clear them and only shapes we create ourselves are accepted.
    if parse_profile_storage_stem(name).is_some() {
        return Some(ProfileStorageKind::Container);
    }
    name.strip_suffix(".profile")
        .and_then(|stem| stem.parse::<ProfileId>().ok())
        .map(|_| ProfileStorageKind::Container)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ProfileStorageKind {
    /// Loose `<label> [<id>]` directory: the crash-safe copy that exists while compression is
    /// pending.
    Container,
    /// Compressed `<label> [<id>].tzst`.
    Archive,
}

/// What two entries claiming the same profile actually disagree about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ProfileDuplicateKind {
    /// Byte-identical content. Redundant, so one can be dropped to reclaim the space.
    Redundant,
    /// Different snapshots of the same profile, or a fingerprint too old to compare. Only the user
    /// can say which they want, so this is never resolved automatically.
    Divergent,
}

/// Group stored entries that claim the same profile.
///
/// Entries only duplicate each other when they are the **same kind**. A container and an archive
/// for one profile is the routine mid-compression state — `execute_profile_archive_job` writes the
/// archive and removes the container only afterwards, and that window survives a close or a crash —
/// so treating it as a duplicate would prompt about the app's own normal behaviour.
fn duplicate_profile_storage_groups(
    entries: &[(ProfileId, ProfileStorageKind, Option<String>)],
) -> Vec<(Vec<usize>, ProfileDuplicateKind)> {
    let mut groups: Vec<(ProfileId, ProfileStorageKind, Vec<usize>)> = Vec::new();
    for (index, (profile_id, kind, _)) in entries.iter().enumerate() {
        match groups
            .iter_mut()
            .find(|(id, group_kind, _)| id == profile_id && group_kind == kind)
        {
            Some((_, _, members)) => members.push(index),
            None => groups.push((*profile_id, *kind, vec![index])),
        }
    }
    groups
        .into_iter()
        .filter(|(_, _, members)| members.len() > 1)
        .map(|(_, _, members)| {
            let first = entries[members[0]].2.as_deref();
            // An absent fingerprint predates content tracking, so it can never prove sameness.
            let redundant = first.is_some()
                && members
                    .iter()
                    .all(|index| entries[*index].2.as_deref() == first);
            let kind = if redundant {
                ProfileDuplicateKind::Redundant
            } else {
                ProfileDuplicateKind::Divergent
            };
            (members, kind)
        })
        .collect()
}

/// Move stored profile data to the name its own metadata says it should have, returning the new
/// path when it moved.
///
/// This is what carries older `<uuid>.profile.tzst` data over to readable names, and what corrects
/// a label left stale by a rename. It never overwrites: if the canonical name is taken by something
/// else, the entry keeps the name it has, since a wrong label is cosmetic and a clobbered archive
/// is not.
fn rename_profile_storage_to_canonical(
    roots: &crate::integrations::profiles::ProfileRoots,
    path: &Path,
    metadata: &crate::integrations::profiles::ProfileArchiveMetadata,
    is_archive: bool,
    warnings: &mut Vec<String>,
) -> Option<PathBuf> {
    // A name that already carries this profile's short id is current, because renames keep it so.
    // Rewriting it from the embedded name would undo a rename made while the profile was inactive.
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if crate::integrations::profiles::profile_storage_stem_matches(stem, metadata.profile_id) {
        return None;
    }
    let canonical = if is_archive {
        roots.archive_path_for(metadata.profile_id, &metadata.display_name)
    } else {
        roots.profile_path_for(metadata.profile_id, &metadata.display_name)
    };
    if canonical == path {
        return None;
    }
    if canonical.exists() {
        warnings.push(format!(
            "{} was left under its current name because {} already exists",
            path.display(),
            canonical.display()
        ));
        return None;
    }
    match std::fs::rename(path, &canonical) {
        Ok(()) => Some(canonical),
        Err(error) => {
            warnings.push(format!(
                "{} could not be renamed and was left as it is: {error}",
                path.display()
            ));
            None
        }
    }
}

/// Tell the user when more than one stored entry claims the same profile.
///
/// Reporting rather than resolving: identical copies are safe to drop but that is the user's call,
/// and differing copies are two real snapshots only they can choose between.
fn report_duplicate_profile_storage(
    seen: &[(ProfileId, ProfileStorageKind, Option<String>, PathBuf)],
    warnings: &mut Vec<String>,
) -> Vec<ProfileDuplicateEntry> {
    let mut entries = Vec::new();
    let keys: Vec<(ProfileId, ProfileStorageKind, Option<String>)> = seen
        .iter()
        .map(|(id, kind, fingerprint, _)| (*id, *kind, fingerprint.clone()))
        .collect();
    for (members, duplicate_kind) in duplicate_profile_storage_groups(&keys) {
        let paths: Vec<String> = members
            .iter()
            .map(|index| seen[*index].3.display().to_string())
            .collect();
        warnings.push(match duplicate_kind {
            ProfileDuplicateKind::Redundant => format!(
                "{} identical copies of one profile are stored: {}. Deleting the extras frees space without losing anything.",
                paths.len(),
                paths.join(", ")
            ),
            ProfileDuplicateKind::Divergent => format!(
                "{} different copies of one profile are stored: {}. Only one can be used; the rest were left untouched.",
                paths.len(),
                paths.join(", ")
            ),
        });
        // The first is the copy in use; the rest are what the user is asked about.
        for index in members.iter().skip(1) {
            let (_, _, _, path) = &seen[*index];
            entries.push(ProfileDuplicateEntry {
                game_id: String::new(),
                display_name: String::new(),
                path: path.clone(),
                bytes: std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
                redundant: duplicate_kind == ProfileDuplicateKind::Redundant,
            });
        }
    }
    entries
}

/// Profile data present in storage that the catalog has no record of. Its embedded `profile.json`
/// carries everything a record needs, so the profile can be restored exactly rather than guessed
/// at — the only fields taken from the filesystem are the on-disk size and the modification time.
#[derive(Clone)]
struct OrphanedProfile {
    metadata: crate::integrations::profiles::ProfileArchiveMetadata,
    /// Name taken from the storage name, which a rename keeps current. The embedded name is only
    /// what it was called when the archive was last written.
    label: Option<String>,
    archive_size: Option<u64>,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
struct RecoveredProfileLabel {
    profile_id: ProfileId,
    label: String,
}

/// Find profile data whose id is absent from the catalog. Without this an archive whose record is
/// lost stays invisible forever while still occupying its full size on disk, with no way to reach
/// or reclaim it from inside the app.
fn discover_orphaned_profiles(
    roots: &crate::integrations::profiles::ProfileRoots,
    spec: &ProfileOperationSpec,
    warnings: &mut Vec<String>,
) -> std::result::Result<
    (
        Vec<OrphanedProfile>,
        Vec<RecoveredProfileLabel>,
        Vec<ProfileDuplicateEntry>,
    ),
    ProfileWorkerError,
> {
    if !roots.profiles_dir.is_dir() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    let mut found: Vec<OrphanedProfile> = Vec::new();
    let mut renamed: Vec<(ProfileId, ProfileStorageKind, String)> = Vec::new();
    // Every valid entry, known or not, so copies of a profile already in the catalog are reported
    // too — that is the case where a user has quietly duplicated an archive.
    let mut seen: Vec<(ProfileId, ProfileStorageKind, Option<String>, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&roots.profiles_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(kind) = classify_profile_storage(&name) else {
            continue;
        };
        let mut path = entry.path();
        let is_archive = kind == ProfileStorageKind::Archive;
        let metadata = if is_archive {
            crate::integrations::profiles::read_profile_archive_metadata(&path)
                .map_err(ProfileWorkerError::Other)
        } else {
            read_profile_container_metadata(&path)
        };
        let metadata = match metadata {
            Ok(metadata) => metadata,
            Err(error) => {
                // Unreadable data is left exactly where it is: it may still be salvageable by
                // hand, and deleting or renaming someone's mods on a hunch is far worse than
                // leaving an entry out of the list.
                warnings.push(format!(
                    "Profile data at {} could not be read and was left untouched: {error}",
                    path.display()
                ));
                continue;
            }
        };
        // The embedded metadata is the identity, never the file name.
        if metadata.game_id != spec.game_id
            || metadata.backend != spec.game.definition.backend
            || metadata.format_version == 0
        {
            continue;
        }
        let label_before_rename = storage_label_from_path(&path);
        if let Some(renamed_path) = rename_profile_storage_to_canonical(
            roots,
            &path,
            &metadata,
            is_archive,
            warnings,
        ) {
            path = renamed_path;
        }
        seen.push((metadata.profile_id, kind, metadata.source_fingerprint.clone(), path.clone()));
        let label = storage_label_from_path(&path).or(label_before_rename);
        if spec.known_profile_ids.contains(&metadata.profile_id) {
            if let Some(label) = label.filter(|label| !label.trim().is_empty()) {
                renamed.push((metadata.profile_id, kind, label));
            }
            continue;
        }
        let file_metadata = std::fs::metadata(&path).ok();
        found.push(OrphanedProfile {
            label,
            archive_size: is_archive
                .then(|| file_metadata.as_ref().map(std::fs::Metadata::len))
                .flatten(),
            updated_at: file_metadata
                .and_then(|file_metadata| file_metadata.modified().ok())
                .map(DateTime::<Utc>::from),
            metadata,
        });
    }
    let duplicates = report_duplicate_profile_storage(&seen, warnings);
    // One record per profile, however many copies of it are on disk. The extras are reported
    // above and left alone; adopting each would put the same profile in the list several times.
    let mut adopted_ids: HashSet<ProfileId> = HashSet::new();
    found.retain(|orphan| adopted_ids.insert(orphan.metadata.profile_id));
    // Stable order so repeated recoveries present the list the same way.
    found.sort_by(|a, b| {
        a.metadata
            .created_at
            .cmp(&b.metadata.created_at)
            .then_with(|| a.metadata.profile_id.cmp(&b.metadata.profile_id))
    });
    let renamed = recovered_profile_labels_from_storage(renamed);
    let duplicates: Vec<ProfileDuplicateEntry> = duplicates
        .into_iter()
        .map(|entry| ProfileDuplicateEntry {
            game_id: spec.game_id.clone(),
            display_name: entry
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(crate::integrations::profiles::parse_profile_storage_stem)
                .map(|(label, _)| label.to_string())
                .unwrap_or_else(|| entry.display_name.clone()),
            ..entry
        })
        .collect();
    Ok((found, renamed, duplicates))
}

fn recovered_profile_labels_from_storage(
    mut candidates: Vec<(ProfileId, ProfileStorageKind, String)>,
) -> Vec<RecoveredProfileLabel> {
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| profile_storage_kind_priority(a.1).cmp(&profile_storage_kind_priority(b.1)))
            .then_with(|| a.2.cmp(&b.2))
    });
    let mut profile_ids: Vec<ProfileId> = candidates
        .iter()
        .map(|(profile_id, _, _)| *profile_id)
        .collect();
    profile_ids.dedup();

    let mut renamed = Vec::new();
    for profile_id in profile_ids {
        for kind in [ProfileStorageKind::Container, ProfileStorageKind::Archive] {
            let mut labels: Vec<String> = candidates
                .iter()
                .filter(|(candidate_id, candidate_kind, _)| {
                    *candidate_id == profile_id && *candidate_kind == kind
                })
                .map(|(_, _, label)| label.clone())
                .collect();
            labels.sort();
            labels.dedup();
            match labels.as_slice() {
                [label] => {
                    renamed.push(RecoveredProfileLabel {
                        profile_id,
                        label: label.clone(),
                    });
                    break;
                }
                [] => {}
                _ => {
                    // Ambiguous same-kind duplicates are reported separately and left for the user
                    // to resolve. Do not let an arbitrary directory iteration order pick the app
                    // name.
                }
            }
        }
    }
    renamed
}

fn profile_storage_kind_priority(kind: ProfileStorageKind) -> u8 {
    match kind {
        ProfileStorageKind::Container => 0,
        ProfileStorageKind::Archive => 1,
    }
}

fn storage_label_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(crate::integrations::profiles::parse_profile_storage_stem)
        .map(|(label, _)| label.to_string())
}

fn recover_profile_swap_journals(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<Option<ActiveProfileMarker>, ProfileWorkerError> {
    let mut recovered_active = None;

    if roots.profiles_dir.exists() {
        let entries =
            std::fs::read_dir(&roots.profiles_dir)?.collect::<std::result::Result<Vec<_>, _>>()?;
        recover_profile_swap_journal_entries(roots, &entries, false, &mut recovered_active)?;
        cleanup_profile_operation_sidecars(&entries);
    }

    let legacy_staging = roots.staging_dir();
    if legacy_staging.exists() {
        let entries =
            std::fs::read_dir(&legacy_staging)?.collect::<std::result::Result<Vec<_>, _>>()?;
        recover_profile_swap_journal_entries(roots, &entries, true, &mut recovered_active)?;
        cleanup_profile_operation_sidecars(&entries);
        let _ = std::fs::remove_dir(&legacy_staging);
    }

    Ok(recovered_active)
}

fn recover_profile_swap_journal_entries(
    roots: &crate::integrations::profiles::ProfileRoots,
    entries: &[std::fs::DirEntry],
    legacy_operation_names: bool,
    recovered_active: &mut Option<ActiveProfileMarker>,
) -> std::result::Result<(), ProfileWorkerError> {
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(journal_stem) = name.strip_suffix(".journal") else {
            continue;
        };
        let bytes = std::fs::read(entry.path()).unwrap_or_default();
        if let Ok(journal) = serde_json::from_slice::<ProfileSwapJournal>(&bytes) {
            match journal.phase {
                ProfileSwapPhase::Committed => {
                    let _ = std::fs::remove_dir_all(&journal.target_source);
                    *recovered_active = journal.target_profile;
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
        ) && legacy_operation_names
        {
            recover_legacy_profile_swap(roots, journal_stem, &bytes)?;
        } else {
            return Err(anyhow::anyhow!(
                "profile recovery journal is malformed and was preserved at {}",
                entry.path().display()
            )
            .into());
        }
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

fn cleanup_profile_operation_sidecars(entries: &[std::fs::DirEntry]) {
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".extracting") {
            let _ = std::fs::remove_dir_all(entry.path());
        } else if name.ends_with(".journal.part") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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
    }
    std::fs::remove_dir_all(backup)?;
    Ok(())
}

fn remove_legacy_active_profile_marker(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::io::Result<()> {
    let marker = roots.profiles_dir.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE);
    if marker.exists() {
        std::fs::remove_file(marker)?;
    }
    let part = roots.profiles_dir.join("active_profile.json.part");
    if part.exists() {
        std::fs::remove_file(part)?;
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
        if name == "active_profile.json.part" || name == LEGACY_ACTIVE_PROFILE_MARKER_FILE {
            let _ = std::fs::remove_file(path);
            continue;
        }
        // A sidecar is always `<final archive>.bak` / `.part`, so stripping the suffix gives the
        // archive it belongs to whatever that archive is called. Rebuilding the name from an id
        // instead would tie recovery to one naming scheme and, worse, to a directory lookup whose
        // answer depends on what happens to exist at the time.
        if let Some(final_name) = name.strip_suffix(".bak") {
            let final_path = roots.profiles_dir.join(final_name);
            let Some(profile_id) = validated_profile_archive_metadata(&path)
                .ok()
                .map(|metadata| metadata.profile_id)
            else {
                // Unreadable, so it can never be promoted. Quarantine rather than leave it: a
                // stale sidecar would otherwise block the next atomic replace forever.
                std::fs::rename(&path, next_archive_conflict_path(&final_path))?;
                continue;
            };
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
        let Some(final_name) = name.strip_suffix(".part") else {
            continue;
        };
        let Some(profile_id) = validated_profile_archive_metadata(&path)
            .ok()
            .map(|metadata| metadata.profile_id)
        else {
            // A partial write that never completed. Preserve the bytes under a conflict name
            // instead of discarding them, but get them out of the canonical path.
            std::fs::rename(&path, next_archive_conflict_path(&roots.profiles_dir.join(final_name)))?;
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

fn recover_profile_conflicts(
    roots: &crate::integrations::profiles::ProfileRoots,
    spec: &ProfileOperationSpec,
    warnings: &mut Vec<String>,
) -> std::result::Result<(), ProfileWorkerError> {
    use crate::integrations::profiles::{
        PROFILE_ARCHIVE_EXTENSION, parse_profile_storage_stem, read_profile_archive_metadata,
    };

    let entries =
        std::fs::read_dir(&roots.profiles_dir)?.collect::<std::result::Result<Vec<_>, _>>()?;
    for entry in entries {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = name.strip_suffix(&format!(".{PROFILE_ARCHIVE_EXTENSION}")) else {
            continue;
        };
        let Some((base_stem, _)) = stem.split_once(".conflict-") else {
            continue;
        };
        let Some((_, short_id)) = parse_profile_storage_stem(base_stem) else {
            warnings.push(format!(
                "Profile conflict archive was preserved because its name is not recognizable: {}",
                entry.path().display()
            ));
            continue;
        };
        let Ok(profile_id) = short_id.parse::<ProfileId>() else {
            warnings.push(format!(
                "Profile conflict archive was preserved because its id is not recognizable: {}",
                entry.path().display()
            ));
            continue;
        };
        let conflict_path = entry.path();
        let conflict_metadata = match read_profile_archive_metadata(&conflict_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                warnings.push(format!(
                    "Profile conflict archive was preserved because it could not be read: {} ({error})",
                    conflict_path.display()
                ));
                continue;
            }
        };
        if validate_profile_archive_metadata_for_recovery(&conflict_metadata, spec, profile_id)
            .is_err()
        {
            warnings.push(format!(
                "Profile conflict archive was preserved because it does not match this game: {}",
                conflict_path.display()
            ));
            continue;
        }
        let live_archive = roots.archive_path(profile_id);
        if !live_archive.is_file() {
            warnings.push(format!(
                "Profile conflict archive was preserved because no live archive exists for comparison: {}",
                conflict_path.display()
            ));
            continue;
        }
        let live_metadata = match read_profile_archive_metadata(&live_archive) {
            Ok(metadata) => metadata,
            Err(error) => {
                warnings.push(format!(
                    "Profile conflict archive was preserved because the live archive could not be checked: {} ({error})",
                    conflict_path.display()
                ));
                continue;
            }
        };
        if profile_archive_metadata_is_redundant(&live_metadata, &conflict_metadata) {
            std::fs::remove_file(conflict_path)?;
        } else {
            warnings.push(format!(
                "Profile conflict archive was preserved because it may contain a different snapshot: {}",
                conflict_path.display()
            ));
        }
    }
    Ok(())
}

fn validate_profile_archive_metadata_for_recovery(
    metadata: &crate::integrations::profiles::ProfileArchiveMetadata,
    spec: &ProfileOperationSpec,
    expected_profile_id: ProfileId,
) -> std::result::Result<(), ProfileWorkerError> {
    if metadata.profile_id != expected_profile_id
        || metadata.game_id != spec.game_id
        || metadata.backend != spec.game.definition.backend
        || metadata.format_version == 0
    {
        return Err(anyhow::anyhow!("profile archive metadata does not match this game").into());
    }
    Ok(())
}

fn profile_archive_metadata_is_redundant(
    live: &crate::integrations::profiles::ProfileArchiveMetadata,
    conflict: &crate::integrations::profiles::ProfileArchiveMetadata,
) -> bool {
    live.profile_id == conflict.profile_id
        && live.game_id == conflict.game_id
        && live.backend == conflict.backend
        && live.format_version == conflict.format_version
        && live.created_at == conflict.created_at
        && live.uncompressed_size == conflict.uncompressed_size
        && live.file_count == conflict.file_count
        && live.portable_metadata == conflict.portable_metadata
        && live.categories == conflict.categories
        && live.tools == conflict.tools
        && live.tool_blacklist == conflict.tool_blacklist
        && live.source_fingerprint.is_some()
        && live.source_fingerprint == conflict.source_fingerprint
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
        let profile_id = if let Some(stem) = name.strip_suffix(".profile.deleting") {
            stem.parse::<ProfileId>().ok()
        } else if let Some(stem) = name.strip_suffix(".deleting") {
            crate::integrations::profiles::parse_profile_storage_stem(stem)
                .and_then(|(_, id)| id.parse::<ProfileId>().ok())
        } else {
            None
        };
        let Some(profile_id) = profile_id else {
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
) -> std::result::Result<(Vec<ProfileArchiveJob>, Vec<String>), ProfileWorkerError> {
    let mut jobs = Vec::new();
    let mut warnings = Vec::new();
    for entry in std::fs::read_dir(&roots.profiles_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        // Containers only; an archive here is already compressed and needs no job.
        if classify_profile_storage(&name) != Some(ProfileStorageKind::Container) {
            continue;
        }
        let path = entry.path();
        // The container's own metadata is the identity, so a hand-renamed folder still resolves.
        let container = read_profile_container_metadata(&path).ok().filter(|metadata| {
            metadata.game_id == spec.game_id
                && metadata.backend == spec.game.definition.backend
                && metadata.format_version != 0
        });
        let profile_id = container.as_ref().map(|metadata| metadata.profile_id);
        let valid = entry.file_type()?.is_dir() && container.is_some();
        if let (true, Some(profile_id)) = (valid, profile_id) {
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
    profile_id: ProfileId,
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

    fn metadata(profile_id: ProfileId, display_name: &str) -> profiles::ProfileArchiveMetadata {
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
            known_profile_ids: Vec::new(),
            reidentify_source: None,
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
            relative_path: None,
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
        spec.target_profile_id = Some(ProfileId::random());
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

        let mut container = metadata(ProfileId::random(), "Warm");
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
        let legacy = metadata(ProfileId::random(), "Legacy");

        backfill_target_profile_data(&mut spec, &legacy);

        assert!(
            spec.target_tools.is_none(),
            "None must stay None so the live tool set is left untouched on activation"
        );
        assert!(spec.target_tool_blacklist.is_none());
    }

    #[test]
    fn archive_reuse_is_rejected_when_only_the_tools_changed() {
        let profile_id = ProfileId::random();
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
    fn copying_profile_roots_reports_progress_across_every_root() {
        let temp = tempfile::tempdir().unwrap();
        let source = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: Some(temp.path().join("Mods_Archived")),
            disabled: None,
        };
        std::fs::create_dir_all(&source.mods).unwrap();
        std::fs::create_dir_all(source.archived.as_ref().unwrap()).unwrap();
        std::fs::write(source.mods.join("a.txt"), vec![b'a'; 400]).unwrap();
        std::fs::write(source.mods.join("b.txt"), vec![b'b'; 400]).unwrap();
        std::fs::write(
            source.archived.as_ref().unwrap().join("c.txt"),
            vec![b'c'; 200],
        )
        .unwrap();

        let destination = profile_container_roots(&source, &temp.path().join("staging"));
        let mut reported = Vec::new();
        copy_profile_roots(
            &source,
            &destination,
            &Arc::new(AtomicBool::new(false)),
            &mut |bytes| {
                reported.push(bytes);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(reported.len(), 3, "one report per copied file");
        assert!(
            reported.windows(2).all(|pair| pair[0] < pair[1]),
            "the running total must not restart when the copy moves to the next root: {reported:?}"
        );
        assert_eq!(
            reported.last().copied(),
            Some(1000),
            "the final total must cover every root, not just the last one"
        );
    }

    #[test]
    fn copy_progress_reporter_spans_the_preparation_band_without_dividing_by_zero() {
        let dir = tempfile::tempdir().unwrap();
        let spec = recovery_spec(xxmi_game(&dir.path().join("Mods")));

        let mut report = copy_progress_reporter(&spec, 1000);
        report(0).unwrap();
        assert_eq!(spec.progress.load(Ordering::Relaxed), 10);
        report(500).unwrap();
        assert_eq!(spec.progress.load(Ordering::Relaxed), 35);
        report(1000).unwrap();
        assert_eq!(spec.progress.load(Ordering::Relaxed), 60);
        assert_eq!(
            spec.stage.read().unwrap().as_str(),
            "Copying selected profile"
        );

        // An empty profile still reports, and must stay inside the band.
        let empty = recovery_spec(xxmi_game(&dir.path().join("Mods")));
        copy_progress_reporter(&empty, 0)(0).unwrap();
        assert_eq!(empty.progress.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn copy_progress_reporter_stops_on_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let spec = recovery_spec(xxmi_game(&dir.path().join("Mods")));
        spec.cancel.store(true, Ordering::Relaxed);

        assert!(
            copy_progress_reporter(&spec, 1000)(500).is_err(),
            "a canceled operation must abort the copy rather than run to completion"
        );
    }

    /// Write a loose profile container with valid embedded metadata.
    fn write_container(roots: &profiles::ProfileRoots, id: ProfileId, name: &str) -> PathBuf {
        let container = roots.profile_path(id);
        std::fs::create_dir_all(container.join("Mods")).unwrap();
        std::fs::write(container.join("Mods").join("mod.txt"), b"payload").unwrap();
        write_profile_container_metadata(&container, &metadata(id, name)).unwrap();
        container
    }

    fn orphan_spec(roots: &profiles::ProfileRoots, known: Vec<ProfileId>) -> ProfileOperationSpec {
        let mut spec = recovery_spec(xxmi_game(&roots.mods));
        spec.known_profile_ids = known;
        spec
    }

    #[test]
    fn a_container_and_its_archive_are_not_duplicates_while_compression_is_pending() {
        let profile_id = ProfileId::random();
        let entries = [
            (profile_id, ProfileStorageKind::Container, Some("fp".into())),
            (profile_id, ProfileStorageKind::Archive, Some("fp".into())),
        ];

        assert!(
            duplicate_profile_storage_groups(&entries).is_empty(),
            "the archive is written before the container is removed, so this pair is routine"
        );
    }

    #[test]
    fn recovered_profile_label_prefers_a_loose_container_over_an_archive() {
        let profile_id = ProfileId::random();

        let renamed = recovered_profile_labels_from_storage(vec![
            (
                profile_id,
                ProfileStorageKind::Archive,
                "Archived Name".to_string(),
            ),
            (
                profile_id,
                ProfileStorageKind::Container,
                "Loose Name".to_string(),
            ),
        ]);

        assert_eq!(renamed.len(), 1);
        assert_eq!(renamed[0].profile_id, profile_id);
        assert_eq!(renamed[0].label, "Loose Name");
    }

    #[test]
    fn recovered_profile_label_ignores_ambiguous_same_kind_duplicates() {
        let profile_id = ProfileId::random();

        let renamed = recovered_profile_labels_from_storage(vec![
            (profile_id, ProfileStorageKind::Archive, "One".to_string()),
            (profile_id, ProfileStorageKind::Archive, "Two".to_string()),
        ]);

        assert!(
            renamed.is_empty(),
            "two same-kind copies with different labels must be resolved by the user"
        );
    }

    #[test]
    fn same_kind_copies_are_grouped_and_classified_by_content() {
        let profile_id = ProfileId::random();
        let other_id = ProfileId::random();
        let entries = [
            (profile_id, ProfileStorageKind::Archive, Some("fp".into())),
            (profile_id, ProfileStorageKind::Archive, Some("fp".into())),
            (other_id, ProfileStorageKind::Archive, Some("aaa".into())),
            (other_id, ProfileStorageKind::Archive, Some("bbb".into())),
        ];

        let groups = duplicate_profile_storage_groups(&entries);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0], (vec![0, 1], ProfileDuplicateKind::Redundant));
        assert_eq!(
            groups[1],
            (vec![2, 3], ProfileDuplicateKind::Divergent),
            "different content means only the user can choose"
        );
    }

    #[test]
    fn an_uncomparable_fingerprint_is_never_treated_as_redundant() {
        let profile_id = ProfileId::random();
        let entries = [
            (profile_id, ProfileStorageKind::Archive, None),
            (profile_id, ProfileStorageKind::Archive, None),
        ];

        assert_eq!(
            duplicate_profile_storage_groups(&entries),
            vec![(vec![0, 1], ProfileDuplicateKind::Divergent)],
            "an archive too old to fingerprint must never be deleted as redundant"
        );
    }

    #[test]
    fn duplicate_containers_are_grouped_and_distinct_profiles_are_left_alone() {
        let profile_id = ProfileId::random();
        let entries = [
            (profile_id, ProfileStorageKind::Container, Some("fp".into())),
            (profile_id, ProfileStorageKind::Container, Some("fp".into())),
            (ProfileId::random(), ProfileStorageKind::Archive, Some("x".into())),
            (ProfileId::random(), ProfileStorageKind::Container, Some("y".into())),
        ];

        let groups = duplicate_profile_storage_groups(&entries);

        assert_eq!(groups, vec![(vec![0, 1], ProfileDuplicateKind::Redundant)]);
    }

    #[test]
    fn recovery_adopts_stored_profiles_the_catalog_lost_but_never_known_ones() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();

        let known_id = ProfileId::random();
        let orphan_id = ProfileId::random();
        let known = write_container(&roots, known_id, "Known");
        write_container(&roots, orphan_id, "Lost");
        let orphan_archive_id = ProfileId::random();
        let orphan_archive = write_container(&roots, orphan_archive_id, "Lost Archive");
        profiles::create_profile_archive_with_progress(
            &profile_container_roots(&roots, &orphan_archive),
            &metadata(orphan_archive_id, "Lost Archive"),
            &roots.archive_path(orphan_archive_id),
            None,
        )
        .unwrap();
        std::fs::remove_dir_all(&orphan_archive).unwrap();
        let _ = known;

        let mut warnings = Vec::new();
        let (found, _, _) = discover_orphaned_profiles(
            &roots,
            &orphan_spec(&roots, vec![known_id]),
            &mut warnings,
        )
        .unwrap();

        let names: Vec<&str> = found
            .iter()
            .map(|orphan| orphan.metadata.display_name.as_str())
            .collect();
        assert!(
            !names.contains(&"Known"),
            "a profile the catalog already lists must not be adopted twice"
        );
        assert!(names.contains(&"Lost"), "a loose container must be adopted");
        assert!(
            names.contains(&"Lost Archive"),
            "an archive must be adopted, which is the case that stranded gigabytes"
        );
        let archived = found
            .iter()
            .find(|orphan| orphan.metadata.profile_id == orphan_archive_id)
            .unwrap();
        assert!(
            archived.archive_size.is_some_and(|size| size > 0),
            "the on-disk size must be recorded so the profile reports its footprint"
        );
    }

    #[test]
    fn recovery_ignores_sidecars_quarantines_and_foreign_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();

        // Interrupted-operation leftovers: adopting these would resurrect half-written data.
        let sidecar_id = ProfileId::random();
        let container = write_container(&roots, sidecar_id, "Interrupted");
        for suffix in [
            format!("{sidecar_id}.profile.deleting"),
            format!("{sidecar_id}.profile.conflict-17"),
        ] {
            let path = roots.profiles_dir.join(suffix);
            std::fs::create_dir_all(&path).unwrap();
            write_profile_container_metadata(&path, &metadata(sidecar_id, "Interrupted")).unwrap();
        }
        std::fs::write(
            roots.profiles_dir.join(format!("{sidecar_id}.profile.tzst.part")),
            b"partial",
        )
        .unwrap();
        std::fs::remove_dir_all(container).unwrap();

        // Another game's profile that somehow shares the directory.
        let foreign_id = ProfileId::random();
        let foreign = roots.profile_path(foreign_id);
        std::fs::create_dir_all(&foreign).unwrap();
        let mut foreign_metadata = metadata(foreign_id, "Other game");
        foreign_metadata.game_id = "someone-else".to_string();
        write_profile_container_metadata(&foreign, &foreign_metadata).unwrap();

        // Renamed by hand, so its label disagrees with its embedded identity. This one IS adopted:
        // the metadata is the identity, and the label is corrected on the way in.
        let embedded_id = ProfileId::random();
        let renamed = roots.profiles_dir.join("Whatever I Called It [aaaaaaaa]");
        std::fs::create_dir_all(&renamed).unwrap();
        write_profile_container_metadata(&renamed, &metadata(embedded_id, "Renamed")).unwrap();

        let mut warnings = Vec::new();
        let (found, _, _) =
            discover_orphaned_profiles(&roots, &orphan_spec(&roots, Vec::new()), &mut warnings)
                .unwrap();

        let adopted: Vec<&str> = found
            .iter()
            .map(|orphan| orphan.metadata.display_name.as_str())
            .collect();
        assert_eq!(
            adopted,
            vec!["Renamed"],
            "sidecars, quarantines and another game's data must be left alone; only the              hand-renamed profile is adopted, under the name its metadata carries"
        );
        assert!(
            roots
                .profiles_dir
                .join(profiles::profile_storage_stem("Renamed", embedded_id))
                .is_dir(),
            "adoption must also correct the label on disk"
        );
        assert!(!renamed.exists(), "the old label must not be left behind");
    }

    #[test]
    fn archives_stored_under_the_previous_id_only_layout_migrate_to_readable_names() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();

        // Exactly what an existing install has on disk: `<uuid>.profile.tzst`, no readable name.
        let profile_id = ProfileId::random();
        let source = write_container(&roots, profile_id, "Patch 1.4");
        let legacy = roots.profiles_dir.join(format!("{profile_id}.profile.tzst"));
        profiles::create_profile_archive_with_progress(
            &profile_container_roots(&roots, &source),
            &metadata(profile_id, "Patch 1.4"),
            &legacy,
            None,
        )
        .unwrap();
        std::fs::remove_dir_all(&source).unwrap();

        let mut warnings = Vec::new();
        let (found, _, _) =
            discover_orphaned_profiles(&roots, &orphan_spec(&roots, Vec::new()), &mut warnings)
                .unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].metadata.profile_id, profile_id);
        assert!(!legacy.exists(), "the id-only name must not be left behind");
        assert!(
            roots
                .archive_path_for(profile_id, "Patch 1.4")
                .is_file(),
            "the archive must be renamed using the name stored inside it"
        );
        assert_eq!(
            roots.archive_path(profile_id),
            roots.archive_path_for(profile_id, "Patch 1.4"),
            "and must still resolve by short id afterwards"
        );
    }

    #[test]
    fn renaming_an_inactive_profile_costs_nothing_and_survives_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let profile_id = ProfileId::random();
        let source = write_container(&roots, profile_id, "Patch 1.4");
        profiles::create_profile_archive_with_progress(
            &profile_container_roots(&roots, &source),
            &metadata(profile_id, "Patch 1.4"),
            &roots.archive_path_for(profile_id, "Patch 1.4"),
            None,
        )
        .unwrap();
        std::fs::remove_dir_all(&source).unwrap();

        // Renaming touches only the storage name; the payload is never rewritten.
        rename_profile_storage_label(&roots, profile_id, "Patch 1.5");
        let renamed = roots.archive_path_for(profile_id, "Patch 1.5");
        assert!(renamed.is_file());
        assert!(!roots.archive_path_for(profile_id, "Patch 1.4").exists());
        assert_eq!(
            profiles::read_profile_archive_metadata(&renamed)
                .unwrap()
                .display_name,
            "Patch 1.4",
            "the embedded name is deliberately left alone - rewriting it would mean repacking"
        );

        let mut warnings = Vec::new();
        let (found, _, _) =
            discover_orphaned_profiles(&roots, &orphan_spec(&roots, Vec::new()), &mut warnings)
                .unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].label.as_deref(),
            Some("Patch 1.5"),
            "recovery must use the name on disk, which the rename kept current"
        );
        assert!(renamed.is_file(), "and must not rename it back");
    }

    #[test]
    fn storage_label_rename_moves_existing_storage_to_the_requested_name() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let profile_id = ProfileId::random();
        let short = profiles::profile_short_id(profile_id);
        // Renamed in Explorer, keeping a valid marker so it still resolves.
        let hand_renamed = roots.profiles_dir.join(format!("aaaDefault [{short}].tzst"));
        std::fs::write(&hand_renamed, b"archive").unwrap();

        rename_profile_storage_label(&roots, profile_id, "Default");

        assert!(
            roots.archive_path_for(profile_id, "Default").is_file(),
            "an in-app rename moves existing inactive storage to the requested name"
        );
        assert!(
            !hand_renamed.exists(),
            "the hand-chosen name must not be left to drift against the app"
        );
    }

    #[test]
    fn known_inactive_profile_uses_the_storage_label_as_its_recovered_name() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let profile_id = ProfileId::random();
        let source = write_container(&roots, profile_id, "xDefault");
        profiles::create_profile_archive_with_progress(
            &profile_container_roots(&roots, &source),
            &metadata(profile_id, "xDefault"),
            &roots.archive_path_for(profile_id, "xDefault"),
            None,
        )
        .unwrap();
        std::fs::remove_dir_all(&source).unwrap();
        let renamed = roots.archive_path_for(profile_id, "Default");
        std::fs::rename(roots.archive_path_for(profile_id, "xDefault"), &renamed).unwrap();

        let mut warnings = Vec::new();
        let (found, renamed_profiles, _) = discover_orphaned_profiles(
            &roots,
            &orphan_spec(&roots, vec![profile_id]),
            &mut warnings,
        )
        .unwrap();

        assert!(found.is_empty(), "known profiles must not be adopted twice");
        assert_eq!(renamed_profiles.len(), 1);
        assert_eq!(renamed_profiles[0].profile_id, profile_id);
        assert_eq!(renamed_profiles[0].label, "Default");
        assert!(
            renamed.is_file(),
            "manual inactive-profile rename must not be rewritten from embedded metadata"
        );
    }

    #[test]
    fn a_copied_archive_is_reported_rather_than_silently_picked() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let profile_id = ProfileId::random();
        let source = write_container(&roots, profile_id, "Patch 1.4");
        profiles::create_profile_archive_with_progress(
            &profile_container_roots(&roots, &source),
            &metadata(profile_id, "Patch 1.4"),
            &roots.archive_path_for(profile_id, "Patch 1.4"),
            None,
        )
        .unwrap();
        std::fs::remove_dir_all(&source).unwrap();
        // What copy-pasting an archive in Explorer leaves behind.
        std::fs::copy(
            roots.archive_path_for(profile_id, "Patch 1.4"),
            roots.profiles_dir.join("Patch 1.4 - Copy [aaaaaaaa].tzst"),
        )
        .unwrap();

        let mut warnings = Vec::new();
        let (found, _, _) =
            discover_orphaned_profiles(&roots, &orphan_spec(&roots, Vec::new()), &mut warnings)
                .unwrap();

        assert_eq!(
            found.len(),
            1,
            "one profile is adopted, not one per copy of it"
        );
        assert!(
            warnings.iter().any(|warning| warning.contains("identical copies")),
            "the extra copy must be reported so its space can be reclaimed: {warnings:?}"
        );
    }

    #[test]
    fn unreadable_profile_data_is_reported_and_left_in_place() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        let broken_id = ProfileId::random();
        let broken = roots.archive_path(broken_id);
        std::fs::write(&broken, b"not a zstd archive").unwrap();

        let mut warnings = Vec::new();
        let (found, _, _) =
            discover_orphaned_profiles(&roots, &orphan_spec(&roots, Vec::new()), &mut warnings)
                .unwrap();

        assert!(found.is_empty());
        assert_eq!(warnings.len(), 1, "the user must be told it was skipped");
        assert!(broken.exists(), "unreadable data must never be removed");
    }

    #[test]
    fn any_archive_counts_but_only_known_folder_shapes_do() {
        let id = ProfileId::random();
        for archive in [
            "Patch 1.4 [1fe9ec7a].tzst".to_string(),
            "Default [7a15244d].tzst".to_string(),
            // Older layouts and hand-copied archives are archives too; their metadata decides.
            format!("{id}.profile.tzst"),
            format!("{id}.tzst"),
            "something anyone dropped here.tzst".to_string(),
        ] {
            assert_eq!(
                classify_profile_storage(&archive),
                Some(ProfileStorageKind::Archive),
                "{archive} should be read and judged on its metadata"
            );
        }

        for container in [
            "Patch 1.4 [1fe9ec7a]".to_string(),
            format!("{id}.profile"),
        ] {
            assert_eq!(
                classify_profile_storage(&container),
                Some(ProfileStorageKind::Container),
                "{container} is a shape we create ourselves"
            );
        }

        for rejected in [
            // In-flight and replaced writes never end in .tzst.
            format!("{id}.profile.tzst.part"),
            format!("{id}.profile.tzst.bak"),
            // Quarantined copies were set aside deliberately; re-adopting one shadows the good copy.
            "Patch 1.4 [1fe9ec7a].conflict-000000000000007b.tzst".to_string(),
            "Patch 1.4 [1fe9ec7a].conflict-000000000000007b".to_string(),
            "Patch 1.4 [1fe9ec7a].extracting".to_string(),
            "Patch 1.4 [1fe9ec7a].journal".to_string(),
            "Patch 1.4 [1fe9ec7a].journal.part".to_string(),
            format!("{id}.profile.deleting"),
            format!("{id}.profile.conflict-17"),
            // A folder with no recognizable shape could be anything the user put here.
            "Some folder of mine".to_string(),
            "active_profile.json".to_string(),
            "readme.txt".to_string(),
            ".staging".to_string(),
        ] {
            assert_eq!(
                classify_profile_storage(&rejected),
                None,
                "{rejected} must not be treated as adoptable profile data"
            );
        }
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
        let profile_id = ProfileId::random();
        let loose = roots.profile_path(profile_id);
        let archive = roots.archive_path(profile_id);
        let part = roots.archive_part_path(profile_id);
        let backup = roots.archive_backup_path(profile_id);
        let labeled_part =
            archive_sidecar_path(&roots.archive_path_for(profile_id, "Patch 1.4.3"), "part");
        let conflict = roots.profiles_dir.join(format!(
            "{}.conflict-0000000000000001.tzst",
            profiles::profile_storage_stem("Patch 1.4.3", profile_id)
        ));
        // A hand-made copy under a different label: deletion must sweep it too, or the profile
        // returns on the next launch when the scan re-adopts it.
        let copy = roots.archive_path_for(profile_id, "Someone's backup");
        std::fs::create_dir_all(&loose).unwrap();
        std::fs::write(loose.join("payload.bin"), b"profile").unwrap();
        for path in [&archive, &part, &backup, &labeled_part, &conflict, &copy] {
            std::fs::write(path, b"profile archive").unwrap();
        }
        let mut spec = recovery_spec(game);
        spec.kind = ProfileOperationKind::Delete;
        spec.profile_id = Some(profile_id);

        delete_profile_storage(&spec, &roots).unwrap();

        for path in [loose, archive, part, backup, labeled_part, conflict, copy] {
            assert!(
                !path.exists(),
                "{} should be permanently deleted",
                path.display()
            );
        }
    }

    #[test]
    fn archive_sidecar_path_keeps_the_destination_archive_label() {
        let profile_id: ProfileId = "1fe9ec7a".parse().unwrap();
        let archive = PathBuf::from(format!("Patch 1.4.3 [{profile_id}].tzst"));

        assert_eq!(
            archive_sidecar_path(&archive, "part"),
            PathBuf::from(format!("Patch 1.4.3 [{profile_id}].tzst.part"))
        );
    }

    #[test]
    fn loose_profile_containers_are_inactive_even_when_ids_collide() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let active_id = ProfileId::random();
        let loose = roots.profile_path(active_id);
        std::fs::create_dir_all(loose.join("Mods")).unwrap();
        std::fs::create_dir_all(loose.join("Mods_Archived")).unwrap();
        std::fs::write(loose.join("Mods").join("preserve.txt"), b"preserve").unwrap();
        write_profile_container_metadata(&loose, &metadata(active_id, "Active")).unwrap();

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
        assert!(matches!(outcome, ProfileArchiveJobOutcome::Completed(_)));
        assert!(!loose.exists());
        assert!(roots.archive_path(active_id).is_file());

        let (jobs, warnings) = pending_profile_archive_jobs(&recovery_spec(game), &roots).unwrap();
        assert!(jobs.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn recovery_removes_legacy_active_profile_marker() {
        let temp = tempfile::tempdir().unwrap();
        let roots = profiles::ProfileRoots {
            profiles_dir: temp.path().join("Mods_Profiles"),
            mods: temp.path().join("Mods"),
            archived: None,
            disabled: None,
        };
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        std::fs::write(
            roots.profiles_dir.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE),
            b"{}",
        )
        .unwrap();
        std::fs::write(roots.profiles_dir.join("active_profile.json.part"), b"{}").unwrap();

        remove_legacy_active_profile_marker(&roots).unwrap();

        assert!(!roots.profiles_dir.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE).exists());
        assert!(!roots.profiles_dir.join("active_profile.json.part").exists());
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
        let journal = roots.profiles_dir.join("Patch 1.4 [1fe9ec7a].journal");
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
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
        let journal = roots.profiles_dir.join("Patch 1.4 [1fe9ec7a].journal");
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
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
    fn committed_swap_journal_reports_the_active_profile_without_marker_file() {
        let temp = tempfile::tempdir().unwrap();
        let game = xxmi_game(&temp.path().join("Mods"));
        let roots = profiles::profile_roots(&game, false).unwrap();
        let target_source = roots.profiles_dir.join("target.profile");
        std::fs::create_dir_all(target_source.join("Mods")).unwrap();
        std::fs::write(target_source.join("Mods").join("cleanup.txt"), b"done").unwrap();
        let target_id = ProfileId::random();
        let target_profile = ActiveProfileMarker {
            profile_id: target_id,
            display_name: "Recovered target".to_string(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
        };
        let journal = profile_journal_path(&roots, &target_profile);
        std::fs::create_dir_all(&roots.profiles_dir).unwrap();
        write_profile_swap_journal(
            &journal,
            &ProfileSwapJournal {
                phase: ProfileSwapPhase::Committed,
                outgoing_profile_id: Some(ProfileId::random()),
                target_profile: Some(target_profile.clone()),
                outgoing: roots.profile_path(ProfileId::random()),
                target_source: target_source.clone(),
            },
        )
        .unwrap();

        let mut warnings = Vec::new();
        let recovered = recover_profile_staging(&roots, &recovery_spec(game), &mut warnings)
            .unwrap()
            .expect("committed journal should identify the active profile");

        assert_eq!(recovered.profile_id, target_id);
        assert_eq!(recovered.display_name, "Recovered target");
        assert!(!target_source.exists());
        assert!(!journal.exists());
        assert!(
            !roots.profiles_dir.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE).exists(),
            "committed recovery must not create active_profile.json"
        );
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
        let profile_id = ProfileId::random();
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
                    name.contains(".conflict-") && name.ends_with(".tzst")
                })
        );
    }


    #[test]
    fn sidecar_recovery_never_discards_a_matching_backup_for_a_wrong_id_final() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let expected_id = ProfileId::random();
        let wrong_id = ProfileId::random();
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
                    name.contains(".conflict-") && name.ends_with(".tzst")
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
        let expected_id = ProfileId::random();
        let wrong_id = ProfileId::random();
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
                    name.contains(".conflict-") && name.ends_with(".tzst")
                })
        );
    }

    #[test]
    fn recovery_removes_redundant_conflict_archives() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let profile_id = ProfileId::random();
        std::fs::create_dir_all(mods.join("ProfileMod")).unwrap();
        std::fs::write(mods.join("ProfileMod").join("payload.txt"), b"same").unwrap();
        let live = roots.archive_path_for(profile_id, "Default");
        profiles::create_profile_archive_with_progress(
            &roots,
            &metadata(profile_id, "Default"),
            &live,
            None,
        )
        .unwrap();
        let conflict = roots.profiles_dir.join(format!(
            "{}.conflict-0000000000000001.tzst",
            profiles::profile_storage_stem("Default", profile_id)
        ));
        std::fs::copy(&live, &conflict).unwrap();
        let mut warnings = Vec::new();

        recover_profile_conflicts(&roots, &recovery_spec(game), &mut warnings).unwrap();

        assert!(!conflict.exists(), "an identical conflict is just wasted space");
        assert!(warnings.is_empty());
    }

    #[test]
    fn recovery_preserves_divergent_conflict_archives() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let profile_id = ProfileId::random();
        std::fs::create_dir_all(mods.join("ProfileMod")).unwrap();
        std::fs::write(mods.join("ProfileMod").join("payload.txt"), b"live").unwrap();
        let live = roots.archive_path_for(profile_id, "Default");
        let archive_metadata = metadata(profile_id, "Default");
        profiles::create_profile_archive_with_progress(&roots, &archive_metadata, &live, None)
            .unwrap();
        std::fs::write(mods.join("ProfileMod").join("payload.txt"), b"conflict").unwrap();
        let conflict = roots.profiles_dir.join(format!(
            "{}.conflict-0000000000000001.tzst",
            profiles::profile_storage_stem("Default", profile_id)
        ));
        profiles::create_profile_archive_with_progress(&roots, &archive_metadata, &conflict, None)
            .unwrap();
        let mut warnings = Vec::new();

        recover_profile_conflicts(&roots, &recovery_spec(game), &mut warnings).unwrap();

        assert!(conflict.exists());
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("may contain a different snapshot")),
            "{warnings:?}"
        );
    }

    #[test]
    fn switch_prefers_a_valid_loose_profile_over_its_archive_baseline() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("Mods");
        let game = xxmi_game(&mods);
        let roots = profiles::profile_roots(&game, false).unwrap();
        profiles::ensure_profile_storage_layout(&roots).unwrap();
        let current_id = ProfileId::random();
        let target_id = ProfileId::random();
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
            known_profile_ids: Vec::new(),
            reidentify_source: None,
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
        let profile_id = ProfileId::random();
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
        let conflict = roots.profiles_dir.join(format!(
            "{}.conflict-0000000000000001.tzst",
            profiles::profile_storage_stem("Profile", profile_id)
        ));
        std::fs::write(&conflict, b"old conflict").unwrap();
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
        assert!(
            !conflict.exists(),
            "a fresh completed archive supersedes old conflict archives for this profile"
        );
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

        let restore_id = ProfileId::random();
        let restore = roots.profiles_dir.join(format!(
            "{}.deleting",
            profiles::profile_storage_stem("Restore", restore_id)
        ));
        std::fs::create_dir_all(restore.join("Mods")).unwrap();
        std::fs::write(restore.join("Mods").join("restore.txt"), b"restore").unwrap();

        let finish_id = ProfileId::random();
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

        let outgoing_id = ProfileId::random();
        let target_id = ProfileId::random();
        let target = profiles_dir.join("target");
        std::fs::create_dir_all(target.join("Mods")).unwrap();
        std::fs::create_dir_all(target.join("Mods_Archived")).unwrap();
        std::fs::write(target.join("Mods").join("target.txt"), b"target").unwrap();
        std::fs::write(
            target.join("Mods_Archived").join("target-old.txt"),
            b"target-old",
        )
        .unwrap();
        let stale_target_archive = roots.archive_path_for(target_id, "Target");
        std::fs::write(&stale_target_archive, b"stale active profile archive").unwrap();
        let stale_target_conflict = profiles_dir.join(format!(
            "{}.conflict-0000000000000001.tzst",
            profiles::profile_storage_stem("Target", target_id)
        ));
        std::fs::write(&stale_target_conflict, b"stale active profile conflict").unwrap();
        let target_marker = ActiveProfileMarker {
            profile_id: target_id,
            display_name: "Target".to_string(),
            categories: Some(Vec::new()),
            tools: None,
            tool_blacklist: None,
        };
        let target_journal = profile_journal_path(&roots, &target_marker);
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
        assert!(
            !stale_target_archive.exists(),
            "the active profile must not keep a stale archive in Mods_Profiles"
        );
        assert!(
            !stale_target_conflict.exists(),
            "the active profile must not keep stale conflict archives in Mods_Profiles"
        );
        assert!(
            !target_journal.exists(),
            "a successful committed swap must not leave a recovery journal behind"
        );
        assert!(
            !profiles_dir.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE).exists(),
            "active profile identity is app/journal state, not a permanent storage file"
        );
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
        let outgoing_id = ProfileId::random();
        let existing = roots.profile_path(outgoing_id);
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("collision.txt"), b"preserve").unwrap();
        let target_id = ProfileId::random();
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
        let conflict = std::fs::read_dir(&profiles_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(".conflict-"))
            })
            .expect("the colliding container must be quarantined");
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
        let mut spec = recovery_spec(xxmi_game(&roots.mods));
        spec.operation_id = 42;
        spec.target_profile_id = Some("1fe9ec7a".parse().unwrap());
        spec.target_display_name = Some("Patch 1.4.3".to_string());
        let staging = roots.staging_dir();
        let extracting = profile_extracting_path(&roots, &spec);
        std::fs::create_dir_all(&extracting).unwrap();
        drop(ProfileStagingCleanup::new(&roots, &spec));
        assert!(!extracting.exists());
        assert!(!staging.exists());

        std::fs::create_dir_all(&extracting).unwrap();
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("recovery.journal"), "installing").unwrap();
        drop(ProfileStagingCleanup::new(&roots, &spec));
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
    fn profile_rollback_does_not_recreate_legacy_active_marker() {
        let temp = tempfile::tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let outgoing = profiles_dir.join("old.profile");
        let target = profiles_dir.join("target.profile");
        let mods = temp.path().join("Mods");
        std::fs::create_dir_all(&profiles_dir).unwrap();
        std::fs::create_dir_all(outgoing.join("Mods")).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(
            outgoing.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE),
            b"legacy outgoing marker",
        )
        .unwrap();
        std::fs::write(
            profiles_dir.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE),
            b"legacy active marker",
        )
        .unwrap();
        let roots = crate::integrations::profiles::ProfileRoots {
            profiles_dir: profiles_dir.clone(),
            mods: mods.clone(),
            archived: None,
            disabled: None,
        };

        rollback_profile_swap(&roots, &outgoing, &target, true).unwrap();
        remove_legacy_active_profile_marker(&roots).unwrap();

        assert!(
            !profiles_dir.join(LEGACY_ACTIVE_PROFILE_MARKER_FILE).exists(),
            "rollback should not restore the removed active marker format"
        );
    }
}
