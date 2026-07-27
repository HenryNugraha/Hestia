fn spawn_profile_worker(
    runtime_services: &RuntimeServices,
    mut rx: WorkerRx<ProfileRequest>,
    tx: WorkerTx<ProfileEvent>,
) {
    let handle = runtime_services.handle();
    runtime_services.spawn(async move {
        while let Some(ProfileRequest::Execute(spec)) = rx.recv().await {
            let tx = tx.clone();
            let operation_id = spec.operation_id;
            let game_id = spec.game_id.clone();
            let cancel = Arc::clone(&spec.cancel);
            let result = handle
                .spawn_blocking(move || execute_profile_operation(spec))
                .await;
            match result {
                Ok(Ok(output)) => {
                    let _ = tx.send(ProfileEvent::Completed {
                        operation_id,
                        game_id,
                        kind: output.kind,
                        profile_id: output.profile_id,
                        target_profile_id: output.target_profile_id,
                        display_name: output.display_name,
                        archive: output.archive,
                        active_profile_marker: output.active_profile_marker,
                    });
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
                    });
                }
                Err(err) => {
                    let _ = tx.send(ProfileEvent::Failed {
                        operation_id,
                        game_id,
                        error: format!("profile worker join failed: {err}"),
                    });
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
}

const ACTIVE_PROFILE_MARKER_FILE: &str = "active_profile.json";

#[derive(Debug)]
enum ProfileWorkerError {
    Canceled,
    Other(anyhow::Error),
}

impl std::fmt::Display for ProfileWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canceled => f.write_str("profile operation canceled"),
            Self::Other(err) => err.fmt(f),
        }
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

fn execute_profile_operation(
    mut spec: ProfileOperationSpec,
) -> std::result::Result<ProfileWorkerOutput, ProfileWorkerError> {
    use crate::integrations::profiles;
    use std::fs;

    let roots = profiles::profile_roots(&spec.game, spec.use_default_mods_path)
        .map_err(ProfileWorkerError::Other)?;
    profiles::ensure_profile_storage_layout(&roots).map_err(ProfileWorkerError::Other)?;
    let _staging_cleanup = ProfileStagingCleanup::new(&roots, spec.operation_id);
    match spec.kind {
        ProfileOperationKind::Recover => {
            let marker = recover_profile_staging(&roots)?;
            return Ok(ProfileWorkerOutput {
                kind: spec.kind,
                profile_id: None,
                target_profile_id: marker.as_ref().map(|marker| marker.profile_id),
                display_name: marker.as_ref().map(|marker| marker.display_name.clone()),
                archive: None,
                active_profile_marker: marker,
            });
        }
        _ => {}
    }

    let current_size = profile_tree_size(&roots)?;
    let staged_size = match spec.kind {
        ProfileOperationKind::Switch => spec
            .target_archive
            .as_ref()
            .map(|path| profiles::read_profile_archive_metadata(path))
            .transpose()
            .map_err(ProfileWorkerError::Other)?
            .map(|metadata| metadata.uncompressed_size)
            .unwrap_or(0),
        ProfileOperationKind::Duplicate if spec.source_profile_id == spec.profile_id => {
            current_size
        }
        ProfileOperationKind::Duplicate => spec
            .source_archive
            .as_ref()
            .map(|path| profiles::read_profile_archive_metadata(path))
            .transpose()
            .map_err(ProfileWorkerError::Other)?
            .map(|metadata| metadata.uncompressed_size)
            .unwrap_or(0),
        _ => 0,
    };
    let archive_budget = if matches!(
        spec.kind,
        ProfileOperationKind::Create
            | ProfileOperationKind::Duplicate
            | ProfileOperationKind::Switch
    ) {
        current_size
    } else {
        0
    };
    let required = archive_budget
        .saturating_add(staged_size)
        .saturating_add(64 * 1024 * 1024);
    profiles::ensure_profile_space(&roots.profiles_dir, required)
        .map_err(ProfileWorkerError::Other)?;

    let operation_id = spec.operation_id;
    let staging = roots
        .staging_dir()
        .join(format!("{operation_id:016x}.extracting"));
    let mut archive_result = None;
    let active_archive = spec.profile_id.and_then(|id| {
        profiles::profile_archive_path(&spec.game, spec.use_default_mods_path, id).ok()
    });
    let mut pre_extracted = false;
    if matches!(
        spec.kind,
        ProfileOperationKind::Switch | ProfileOperationKind::Duplicate
    ) && spec
        .source_archive
        .as_ref()
        .or(spec.target_archive.as_ref())
        .is_some()
    {
        let source = if spec.kind == ProfileOperationKind::Switch {
            spec.target_archive.as_ref().unwrap()
        } else {
            spec.source_archive.as_ref().unwrap()
        };
        let should_pre_extract = spec.kind == ProfileOperationKind::Switch
            || spec
                .source_archive
                .as_ref()
                .is_some_and(|path| Some(path) != active_archive.as_ref());
        if should_pre_extract {
            let metadata = extract_to_staging(
                source,
                &staging,
                spec.target_archive_sha256.as_deref(),
                &spec.cancel,
                &spec.progress,
                &spec.stage,
            )?;
            if spec.target_categories.is_none() {
                spec.target_categories = metadata.categories.clone();
            }
            let expected = if spec.kind == ProfileOperationKind::Switch {
                spec.target_profile_id
            } else {
                spec.source_profile_id.or(spec.profile_id)
            };
            if metadata.game_id != spec.game_id
                || metadata.backend != spec.game.definition.backend
                || metadata.format_version == 0
                || expected.is_some_and(|id| metadata.profile_id != id)
            {
                return Err(ProfileWorkerError::Other(anyhow::anyhow!(
                    "profile archive metadata does not match the requested game/profile"
                )));
            }
            pre_extracted = true;
        }
    }

    if matches!(
        spec.kind,
        ProfileOperationKind::Create
            | ProfileOperationKind::Duplicate
            | ProfileOperationKind::Switch
    ) {
        if let (Some(path), Some(metadata)) = (&active_archive, spec.metadata.as_ref()) {
            archive_result = Some(archive_roots_transactional(
                &roots,
                metadata,
                path,
                operation_id,
                &spec.cancel,
                &spec.progress,
                &spec.stage,
            )?);
        }
    }
    check_profile_cancel(&spec.cancel)?;

    match spec.kind {
        ProfileOperationKind::Create => {
            let marker = target_profile_marker(&spec)?;
            update_profile_progress(&spec.progress, &spec.stage, 75, "Committing profile switch");
            reset_roots_transactional(&roots, operation_id, &spec.cancel, &marker)?;
            update_profile_progress(&spec.progress, &spec.stage, 95, "Profile switch committed");
        }
        ProfileOperationKind::Duplicate => {
            let marker = target_profile_marker(&spec)?;
            let source_archive = spec
                .source_archive
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("source profile archive is missing"))?;
            if pre_extracted {
                update_profile_progress(
                    &spec.progress,
                    &spec.stage,
                    75,
                    "Committing profile switch",
                );
                swap_roots(&roots, &staging, operation_id, &marker)?;
                update_profile_progress(
                    &spec.progress,
                    &spec.stage,
                    95,
                    "Profile switch committed",
                );
            } else {
                extract_and_swap(
                    source_archive,
                    &roots,
                    &staging,
                    operation_id,
                    &spec.cancel,
                    &spec.progress,
                    &spec.stage,
                    &marker,
                )?;
            }
        }
        ProfileOperationKind::Switch => {
            let marker = target_profile_marker(&spec)?;
            let target_archive = spec
                .target_archive
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("target profile archive is missing"))?;
            if pre_extracted {
                update_profile_progress(
                    &spec.progress,
                    &spec.stage,
                    75,
                    "Committing profile switch",
                );
                swap_roots(&roots, &staging, operation_id, &marker)?;
                update_profile_progress(
                    &spec.progress,
                    &spec.stage,
                    95,
                    "Profile switch committed",
                );
            } else {
                extract_and_swap(
                    target_archive,
                    &roots,
                    &staging,
                    operation_id,
                    &spec.cancel,
                    &spec.progress,
                    &spec.stage,
                    &marker,
                )?;
            }
        }
        ProfileOperationKind::Rename => {}
        ProfileOperationKind::Delete => {
            if let Some(path) = spec.target_archive.as_ref() {
                if path.exists() {
                    match spec.delete_behavior {
                        crate::model::DeleteBehavior::RecycleBin => {
                            trash::delete(path).map_err(|err| {
                                anyhow::anyhow!(
                                    "failed to move profile archive to recycle bin: {err}"
                                )
                            })?
                        }
                        crate::model::DeleteBehavior::Permanent => fs::remove_file(path)?,
                    }
                }
            }
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
        archive: archive_result,
        active_profile_marker,
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
    })
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

fn archive_roots_transactional(
    roots: &crate::integrations::profiles::ProfileRoots,
    metadata: &crate::integrations::profiles::ProfileArchiveMetadata,
    destination: &Path,
    _operation_id: u64,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<AtomicU64>,
    stage: &Arc<RwLock<String>>,
) -> std::result::Result<crate::integrations::profiles::ArchiveResult, ProfileWorkerError> {
    use crate::integrations::profiles;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 10, "Archiving current profile");
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut callback = |update: profiles::ArchiveProgress| -> Result<()> {
        if cancel.load(Ordering::Relaxed) {
            bail!("profile operation canceled");
        }
        let pct = if update.total_bytes == 0 {
            10
        } else {
            10 + (update.bytes_processed.saturating_mul(35) / update.total_bytes).min(35)
        };
        update_profile_progress(progress, stage, pct, "Archiving current profile");
        Ok(())
    };
    let result = profiles::create_profile_archive_with_progress(
        roots,
        metadata,
        destination,
        Some(&mut callback),
    )?;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 45, "Current profile archived");
    Ok(result)
}

fn extract_and_swap(
    archive: &Path,
    roots: &crate::integrations::profiles::ProfileRoots,
    staging: &Path,
    operation_id: u64,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<AtomicU64>,
    stage: &Arc<RwLock<String>>,
    marker: &ActiveProfileMarker,
) -> std::result::Result<(), ProfileWorkerError> {
    use crate::integrations::profiles;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 55, "Extracting target profile");
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    let mut callback = |update: profiles::ArchiveReadProgress| -> Result<()> {
        if cancel.load(Ordering::Relaxed) {
            bail!("profile operation canceled");
        }
        let pct = if update.total_bytes == 0 {
            55
        } else {
            55 + (update.bytes_read.saturating_mul(20) / update.total_bytes).min(20)
        };
        if progress.load(Ordering::Relaxed) != pct {
            update_profile_progress(progress, stage, pct, "Extracting target profile");
        }
        Ok(())
    };
    profiles::extract_profile_archive_verified_with_progress(
        archive,
        staging,
        None,
        Some(&mut callback),
    )?;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 75, "Committing profile switch");
    swap_roots(roots, staging, operation_id, marker)?;
    update_profile_progress(progress, stage, 95, "Profile switch committed");
    Ok(())
}

fn extract_to_staging(
    archive: &Path,
    staging: &Path,
    expected_sha256: Option<&str>,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<AtomicU64>,
    stage: &Arc<RwLock<String>>,
) -> std::result::Result<crate::integrations::profiles::ProfileArchiveMetadata, ProfileWorkerError>
{
    use crate::integrations::profiles;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 25, "Validating target profile");
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging)?;
    let mut callback = |update: profiles::ArchiveReadProgress| -> Result<()> {
        if cancel.load(Ordering::Relaxed) {
            bail!("profile operation canceled");
        }
        let pct = if update.total_bytes == 0 {
            25
        } else {
            25 + (update.bytes_read.saturating_mul(15) / update.total_bytes).min(15)
        };
        if progress.load(Ordering::Relaxed) != pct {
            update_profile_progress(progress, stage, pct, "Extracting target profile");
        }
        Ok(())
    };
    let metadata = profiles::extract_profile_archive_verified_with_progress(
        archive,
        staging,
        expected_sha256,
        Some(&mut callback),
    )?;
    check_profile_cancel(cancel)?;
    update_profile_progress(progress, stage, 40, "Target profile staged");
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

fn reset_roots_transactional(
    roots: &crate::integrations::profiles::ProfileRoots,
    operation_id: u64,
    cancel: &Arc<AtomicBool>,
    marker: &ActiveProfileMarker,
) -> std::result::Result<(), ProfileWorkerError> {
    check_profile_cancel(cancel)?;
    let staging = roots
        .staging_dir()
        .join(format!("{operation_id:016x}.extracting"));
    let _ = std::fs::remove_dir_all(&staging);
    let staged_roots = crate::integrations::profiles::ProfileRoots {
        profiles_dir: staging.clone(),
        mods: staging.join("Mods"),
        archived: roots
            .archived
            .as_ref()
            .map(|_| staging.join("Mods_Archived")),
        disabled: roots.disabled.as_ref().map(|_| staging.join("Disabled")),
    };
    create_empty_roots(&staged_roots)?;
    swap_roots(roots, &staging, operation_id, marker)
}

fn swap_roots(
    roots: &crate::integrations::profiles::ProfileRoots,
    staging: &Path,
    operation_id: u64,
    marker: &ActiveProfileMarker,
) -> std::result::Result<(), ProfileWorkerError> {
    let journal = roots
        .staging_dir()
        .join(format!("{operation_id:016x}.journal"));
    std::fs::create_dir_all(roots.staging_dir())?;
    std::fs::write(&journal, "backing-up")?;
    let backup = roots
        .mods
        .with_file_name(format!(".hestia-profile-backup-{operation_id:016x}"));
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::create_dir_all(&backup)?;
    let active_marker = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    if active_marker.exists() {
        if let Err(error) = std::fs::rename(&active_marker, backup.join(ACTIVE_PROFILE_MARKER_FILE))
        {
            let _ = std::fs::remove_dir_all(&backup);
            let _ = std::fs::remove_file(&journal);
            return Err(error.into());
        }
    }
    let locations = root_locations(roots);
    for (name, root) in &locations {
        if root.exists() {
            if let Err(err) = std::fs::rename(root, backup.join(name)) {
                rollback_transaction(roots, &backup, &locations, false).map_err(|rollback| {
                    anyhow::anyhow!("profile commit failed: {err}; rollback failed: {rollback}")
                })?;
                let _ = std::fs::remove_dir_all(&backup);
                let _ = std::fs::remove_dir_all(staging);
                let _ = std::fs::remove_file(&journal);
                return Err(err.into());
            }
        }
    }
    if let Err(err) = std::fs::write(&journal, "installing") {
        rollback_transaction(roots, &backup, &locations, false).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {err}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_dir_all(&backup);
        let _ = std::fs::remove_dir_all(staging);
        let _ = std::fs::remove_file(&journal);
        return Err(err.into());
    }
    let staged_locations = [
        ("Mods", staging.join("Mods")),
        ("Mods_Archived", staging.join("Mods_Archived")),
        ("Disabled", staging.join("Disabled")),
    ];
    let mut moved = Vec::new();
    for (name, destination) in &locations {
        let Some((_, source)) = staged_locations
            .iter()
            .find(|(candidate, _)| candidate == name)
        else {
            continue;
        };
        if source.exists() {
            if let Err(err) = std::fs::rename(source, destination) {
                rollback_transaction(roots, &backup, &locations, true).map_err(|rollback| {
                    anyhow::anyhow!("profile commit failed: {err}; rollback failed: {rollback}")
                })?;
                let _ = std::fs::remove_dir_all(&backup);
                let _ = std::fs::remove_dir_all(staging);
                let _ = std::fs::remove_file(&journal);
                return Err(err.into());
            }
            moved.push(name);
        }
    }
    if let Err(err) = create_empty_roots(roots) {
        rollback_transaction(roots, &backup, &locations, true).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {err}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_dir_all(&backup);
        let _ = std::fs::remove_dir_all(staging);
        let _ = std::fs::remove_file(&journal);
        return Err(err);
    }
    if let Err(err) = write_active_profile_marker(roots, marker) {
        rollback_transaction(roots, &backup, &locations, true).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {err}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_dir_all(&backup);
        let _ = std::fs::remove_dir_all(staging);
        let _ = std::fs::remove_file(&journal);
        return Err(err.into());
    }
    if let Err(err) = std::fs::write(&journal, "committed") {
        rollback_transaction(roots, &backup, &locations, true).map_err(|rollback| {
            anyhow::anyhow!("profile commit failed: {err}; rollback failed: {rollback}")
        })?;
        let _ = std::fs::remove_dir_all(&backup);
        let _ = std::fs::remove_dir_all(staging);
        let _ = std::fs::remove_file(&journal);
        return Err(err.into());
    }
    if std::fs::remove_dir_all(&backup).is_ok() {
        let _ = std::fs::remove_file(&journal);
    }
    let _ = std::fs::remove_dir_all(staging);
    let _ = moved;
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

fn rollback_roots(
    backup: &Path,
    locations: &[(&'static str, PathBuf)],
    remove_installed: bool,
) -> std::io::Result<()> {
    for (name, root) in locations {
        let backup_root = backup.join(name);
        if backup_root.exists() {
            let _ = std::fs::remove_dir_all(root);
            std::fs::rename(backup_root, root)?;
        } else if remove_installed {
            let _ = std::fs::remove_dir_all(root);
        }
    }
    Ok(())
}

fn rollback_transaction(
    roots: &crate::integrations::profiles::ProfileRoots,
    backup: &Path,
    locations: &[(&'static str, PathBuf)],
    remove_installed: bool,
) -> std::io::Result<()> {
    rollback_roots(backup, locations, remove_installed)?;
    let marker = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    let _ = std::fs::remove_file(&marker);
    let backup_marker = backup.join(ACTIVE_PROFILE_MARKER_FILE);
    if backup_marker.exists() {
        std::fs::rename(backup_marker, marker)?;
    }
    Ok(())
}

fn recover_profile_staging(
    roots: &crate::integrations::profiles::ProfileRoots,
) -> std::result::Result<Option<ActiveProfileMarker>, ProfileWorkerError> {
    if roots.profiles_dir.exists() {
        let mut journals = std::collections::HashSet::new();
        for entry in std::fs::read_dir(roots.staging_dir())
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".extracting") {
                let _ = std::fs::remove_dir_all(entry.path());
                continue;
            }
            if let Some(id) = name.strip_suffix(".journal") {
                journals.insert(id.to_string());
                let backup = roots
                    .mods
                    .with_file_name(format!(".hestia-profile-backup-{id}"));
                if backup.exists() {
                    let phase = std::fs::read_to_string(entry.path()).unwrap_or_default();
                    if phase.trim() == "committed" {
                        std::fs::remove_dir_all(&backup)?;
                    } else {
                        let locations = root_locations(roots);
                        let remove_installed = phase.trim() == "installing";
                        rollback_transaction(roots, &backup, &locations, remove_installed)?;
                        std::fs::remove_dir_all(&backup)?;
                    }
                }
                let _ = std::fs::remove_file(entry.path());
            }
        }
        for entry in std::fs::read_dir(&roots.profiles_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if name.ends_with(".extracting") {
                let _ = std::fs::remove_dir_all(path);
                continue;
            }
            if name.ends_with(".part") {
                if name == "active_profile.json.part" || journals.is_empty() {
                    let _ = std::fs::remove_file(path);
                }
                continue;
            }
            if name.ends_with(".tzst.bak") {
                let final_path = roots.profiles_dir.join(name.trim_end_matches(".bak"));
                if !final_path.exists() {
                    std::fs::rename(path, final_path)?;
                } else if validate_profile_archive(&final_path).is_ok() {
                    let _ = std::fs::remove_file(path);
                } else {
                    let _ = std::fs::remove_file(&final_path);
                    std::fs::rename(path, final_path)?;
                }
            }
        }
    }
    let marker_path = roots.profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE);
    if !marker_path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(marker_path)?;
    let marker = serde_json::from_slice(&bytes).map_err(anyhow::Error::from)?;
    Ok(Some(marker))
}

fn validate_profile_archive(path: &Path) -> Result<()> {
    let temp = tempfile::tempdir()?;
    crate::integrations::profiles::extract_profile_archive(path, temp.path())?;
    Ok(())
}

#[cfg(test)]
mod profile_worker_tests {
    use super::*;

    #[test]
    fn profile_rollback_preserves_roots_not_yet_backed_up() {
        let temp = tempfile::tempdir().unwrap();
        let backup = temp.path().join("backup");
        let mods = temp.path().join("Mods");
        let archived = temp.path().join("Mods_Archived");
        std::fs::create_dir_all(backup.join("Mods")).unwrap();
        std::fs::write(backup.join("Mods").join("old.txt"), b"old").unwrap();
        std::fs::create_dir_all(&archived).unwrap();
        std::fs::write(archived.join("untouched.txt"), b"untouched").unwrap();

        let locations = vec![("Mods", mods.clone()), ("Mods_Archived", archived.clone())];
        rollback_roots(&backup, &locations, false).unwrap();

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
        let backup = temp.path().join("backup");
        let mods = temp.path().join("Mods");
        let archived = temp.path().join("Mods_Archived");
        std::fs::create_dir_all(backup.join("Mods")).unwrap();
        std::fs::write(backup.join("Mods").join("old.txt"), b"old").unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("new.txt"), b"new").unwrap();
        std::fs::create_dir_all(&archived).unwrap();
        std::fs::write(archived.join("new.txt"), b"new").unwrap();

        let locations = vec![("Mods", mods.clone()), ("Mods_Archived", archived.clone())];
        rollback_roots(&backup, &locations, true).unwrap();

        assert!(mods.join("old.txt").is_file());
        assert!(!mods.join("new.txt").exists());
        assert!(!archived.exists());
    }

    #[test]
    fn profile_rollback_restores_previous_active_marker() {
        let temp = tempfile::tempdir().unwrap();
        let profiles_dir = temp.path().join("Mods_Profiles");
        let backup = temp.path().join("backup");
        let mods = temp.path().join("Mods");
        std::fs::create_dir_all(&profiles_dir).unwrap();
        std::fs::create_dir_all(backup.join("Mods")).unwrap();
        std::fs::create_dir_all(&mods).unwrap();
        let old = ActiveProfileMarker {
            profile_id: Uuid::new_v4(),
            display_name: "Old".to_string(),
            categories: Some(Vec::new()),
        };
        let new = ActiveProfileMarker {
            profile_id: Uuid::new_v4(),
            display_name: "New".to_string(),
            categories: Some(Vec::new()),
        };
        std::fs::write(
            backup.join(ACTIVE_PROFILE_MARKER_FILE),
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

        rollback_transaction(&roots, &backup, &[("Mods", mods)], true).unwrap();

        let restored: ActiveProfileMarker = serde_json::from_slice(
            &std::fs::read(profiles_dir.join(ACTIVE_PROFILE_MARKER_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.profile_id, old.profile_id);
    }
}
