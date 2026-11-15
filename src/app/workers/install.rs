fn spawn_install_workers(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<InstallRequest>,
    tx: WorkerTx<InstallEvent>,
) {
    let runtime_services = runtime_services.clone();
    let prepared_cache: Arc<std::sync::Mutex<HashMap<u64, PreparedImport>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));
    let cancel_flags: Arc<std::sync::Mutex<HashMap<u64, Arc<AtomicBool>>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));

    let mut worker_txs = Vec::new();
    for _ in 0..INSTALL_WORKER_COUNT {
        let (worker_tx, mut worker_rx) = tokio_mpsc::unbounded_channel::<InstallRequest>();
        worker_txs.push(worker_tx);
        let tx = tx.clone();
        let prepared_cache = Arc::clone(&prepared_cache);
        let cancel_flags = Arc::clone(&cancel_flags);
        let portable = portable.clone();
        let handle = runtime_services.handle();
        let runtime_services = runtime_services.clone();
        runtime_services.clone().spawn(async move {
            while let Some(request) = worker_rx.recv().await {
                match request {
                    InstallRequest::Inspect {
                        job_id,
                        game_id,
                        source,
                        gb_profile,
                        framework_context,
                    } => {
                        let cancel = {
                            let mut flags = cancel_flags.lock().unwrap();
                            flags
                                .entry(job_id)
                                .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                                .clone()
                        };
                        let result = handle
                            .spawn_blocking(move || {
                                let prepared = importing::inspect_source_cancelable(&game_id, source, &cancel)?;
                                let framework_scans = framework_context
                                    .map(|context| framework_scans_for_prepared_import(&prepared, &context))
                                    .unwrap_or_default();
                                Ok::<_, anyhow::Error>((prepared, framework_scans))
                            })
                            .await;
                        match result {
                            Ok(Ok((prepared, framework_scans))) => {
                                let inspection = prepared.inspection.clone();
                                prepared_cache.lock().unwrap().insert(job_id, prepared);
                                let _ = tx.send(InstallEvent::InspectReady {
                                    job_id,
                                    inspection,
                                    gb_profile,
                                    framework_scans,
                                });
                            }
                            Ok(Err(err)) => {
                                if err.to_string() == importing::CANCELLED_ERROR {
                                    let _ = tx.send(InstallEvent::InstallCanceled { job_id });
                                } else {
                                    let _ = tx.send(InstallEvent::InspectFailed {
                                        job_id,
                                        error: format!("{err:#}"),
                                    });
                                }
                                cancel_flags.lock().unwrap().remove(&job_id);
                            }
                            Err(err) => {
                                let _ = tx.send(InstallEvent::InspectFailed {
                                    job_id,
                                    error: format!("inspect worker join error: {err}"),
                                });
                                cancel_flags.lock().unwrap().remove(&job_id);
                            }
                        }
                    }
                    InstallRequest::SplitFrameworkParts { job_id, parts } => {
                        let prepared = prepared_cache.lock().unwrap().remove(&job_id);
                        let Some(prepared) = prepared else {
                            let _ = tx.send(InstallEvent::InspectFailed {
                                job_id,
                                error: "missing prepared import".to_string(),
                            });
                            continue;
                        };
                        let result = handle
                            .spawn_blocking(move || split_framework_parts(prepared, parts))
                            .await;
                        match result {
                            Ok((prepared, split_children)) => {
                                let inspection = prepared.inspection.clone();
                                let mut children = Vec::with_capacity(split_children.len());
                                let mut cache = prepared_cache.lock().unwrap();
                                for (child, child_prepared) in split_children {
                                    cache.insert(child.job_id, child_prepared);
                                    children.push(child);
                                }
                                cache.insert(job_id, prepared);
                                drop(cache);
                                let _ = tx.send(InstallEvent::FrameworkPartsSplit {
                                    job_id,
                                    inspection,
                                    children,
                                });
                            }
                            Err(err) => {
                                let _ = tx.send(InstallEvent::InspectFailed {
                                    job_id,
                                    error: format!("framework split worker join error: {err}"),
                                });
                                cancel_flags.lock().unwrap().remove(&job_id);
                            }
                        }
                    }
                    InstallRequest::Install {
                        job_id,
                        candidate_indices,
                        preferred_names,
                        candidate_install_states,
                        game_backend,
                        disabled_target_root,
                        choice,
                        target_root,
                        gb_profile,
                        expected_manifest,
                    } => {
                        let prepared = prepared_cache.lock().unwrap().remove(&job_id);
                        let Some(prepared) = prepared else {
                            let _ = tx.send(InstallEvent::InstallFailed {
                                job_id,
                                preferred_name: "mod".to_string(),
                                error: "missing prepared import".to_string(),
                            });
                            continue;
                        };
                        let cancel = {
                            let mut flags = cancel_flags.lock().unwrap();
                            flags
                                .entry(job_id)
                                .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                                .clone()
                        };
                        let preferred_for_err = preferred_names
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "mod".to_string());
                        let result = handle
                            .spawn_blocking(
                                move || -> Result<(
                                    Vec<PathBuf>,
                                    Vec<(PathBuf, String)>,
                                    Vec<String>,
                                )> {
                                fs::create_dir_all(&target_root)?;
                                if candidate_indices.len() != preferred_names.len()
                                    || candidate_indices.len() != candidate_install_states.len()
                                {
                                    bail!("install candidate data is misaligned");
                                }
                                let mut installed_paths = Vec::new();
                                let mut installed_candidate_labels = Vec::new();
                                let mut target_cleaned =
                                    HashSet::with_capacity(preferred_names.len());
                                let source_is_archive = prepared.source_is_archive;

                                for (i, &idx) in candidate_indices.iter().enumerate() {
                                    let preferred_name = &preferred_names[i];
                                    importing::validate_install_folder_name(preferred_name)?;
                                    let candidate = prepared
                                        .inspection
                                        .candidates
                                        .get(idx)
                                        .ok_or_else(|| anyhow!("invalid candidate index"))?;

                                    let current_choice = if choice == ConflictChoice::Replace {
                                        if target_cleaned.insert(preferred_name.clone()) {
                                            ConflictChoice::Replace
                                        } else {
                                            ConflictChoice::Merge
                                        }
                                    } else {
                                        choice
                                    };

                                    let installed_path =
                                        importing::install_candidate_with_state_verified_cancelable(
                                            &candidate.path,
                                            preferred_name,
                                            &target_root,
                                            disabled_target_root.as_deref(),
                                            current_choice,
                                            source_is_archive,
                                            game_backend,
                                            candidate_install_states[i].clone(),
                                            expected_manifest
                                                .as_deref()
                                                .filter(|_| candidate_indices.len() == 1),
                                            &cancel,
                                        )?
                                        .ok_or_else(|| anyhow!("Import cancelled"))?;
                                    if !installed_paths.contains(&installed_path) {
                                        installed_paths.push(installed_path.clone());
                                    }
                                    installed_candidate_labels
                                        .push((installed_path, candidate.label.clone()));
                                }

                                Ok((installed_paths, installed_candidate_labels, Vec::new()))
                            },
                            )
                            .await;
                        match result {
                            Ok(Ok((installed_paths, installed_candidate_labels, rel_paths))) => {
                                let _ = tx.send(InstallEvent::InstallDone {
                                    job_id,
                                    installed_paths,
                                    installed_candidate_labels,
                                    gb_profile,
                                    rel_paths,
                                });
                            }
                            Ok(Err(err)) => {
                                if err.to_string() == importing::CANCELLED_ERROR {
                                    let _ = tx.send(InstallEvent::InstallCanceled { job_id });
                                } else {
                                    let _ = tx.send(InstallEvent::InstallFailed {
                                        job_id,
                                        preferred_name: preferred_for_err,
                                        error: format!("{err:#}"),
                                    });
                                }
                            }
                            Err(err) => {
                                let _ = tx.send(InstallEvent::InstallFailed {
                                    job_id,
                                    preferred_name: preferred_for_err,
                                    error: format!("install worker join error: {err}"),
                                });
                            }
                        }
                        cancel_flags.lock().unwrap().remove(&job_id);
                    }
                    InstallRequest::PrepareThaliaBundle {
                        job_id,
                        game_id,
                        parts,
                        framework_context,
                        update_target,
                    } => {
                        let cancel = {
                            let mut flags = cancel_flags.lock().unwrap();
                            flags
                                .entry(job_id)
                                .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                                .clone()
                        };
                        let result = handle
                            .spawn_blocking(move || {
                                let frameworks_among_merge_parts =
                                    |extracted: &[(importing::bundle::BundlePartRole, &Path)]| {
                                        let Some(context) = framework_context.as_deref() else {
                                            return vec![false; extracted.len()];
                                        };
                                        let folders: Vec<frameworks::BundleFolder> = extracted
                                            .iter()
                                            .map(|(role, root)| frameworks::BundleFolder {
                                                root,
                                                merges_into_the_update_target: *role
                                                    != importing::bundle::BundlePartRole::Separate,
                                            })
                                            .collect();
                                        frameworks::frameworks_among_bundle_folders(
                                            &folders,
                                            update_target.as_ref(),
                                            context,
                                        )
                                    };
                                importing::bundle::prepare_bundle(
                                    &game_id,
                                    &parts,
                                    &frameworks_among_merge_parts,
                                    &cancel,
                                )
                            })
                            .await;
                        let event = match result {
                            Ok(Ok((root, folders))) => InstallEvent::ThaliaBundlePrepared {
                                job_id,
                                root,
                                folders,
                            },
                            Ok(Err(failure)) => {
                                InstallEvent::ThaliaBundlePrepareFailed { job_id, failure }
                            }
                            Err(err) => InstallEvent::ThaliaBundlePrepareFailed {
                                job_id,
                                failure: importing::bundle::BundleFailure::Other {
                                    file_name: None,
                                    error: format!("bundle worker join error: {err}"),
                                },
                            },
                        };
                        let _ = tx.send(event);
                        cancel_flags.lock().unwrap().remove(&job_id);
                    }
                    InstallRequest::SyncImages {
                        job_id,
                        mod_entry_id,
                        mod_root_path,
                        profile,
                        profile_cached_at,
                    } => {
                        let profile_for_work = profile.clone();
                        let portable = portable.clone();
                        let custom_proxy = runtime_services.custom_proxy();
                        let cover_tx = tx.clone();
                        let cover_mod_entry_id = mod_entry_id.clone();
                        let result = handle
                            .spawn_blocking(move || -> Result<Vec<String>> {
                                std::thread::Builder::new()
                                    .name("hestia-image-sync".to_string())
                                    .spawn(move || -> Result<Vec<String>> {
                                        let builder = reqwest::blocking::Client::builder()
                                            .user_agent(gamebanana::USER_AGENT)
                                            .timeout(Duration::from_secs(60));
                                        let builder = match custom_proxy {
                                            Some(proxy) => builder.proxy(reqwest::Proxy::all(proxy.endpoint())?),
                                            None => builder,
                                        };
                                        let blocking_http_client = builder
                                            .build()
                                            .context("failed to create blocking image-sync client")?;
                                        let snapshot = profile_to_snapshot(&profile_for_work);
                                        let meta_dir = mod_root_path.join(crate::model::MOD_META_DIR);
                                        fs::create_dir_all(&meta_dir)?;
                                        let valid_names: HashSet<String> = snapshot
                                            .preview_urls
                                            .iter()
                                            .enumerate()
                                            .map(|(idx, url)| {
                                                let path_no_query = url.split('?').next().unwrap_or(url);
                                                let ext = Path::new(path_no_query)
                                                    .extension()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or("jpg");
                                                format!("gb_{}_{}.{ext}", profile_for_work.id, idx + 1)
                                            })
                                            .collect();
                                        for entry in fs::read_dir(&meta_dir)? {
                                            let entry = entry?;
                                            let name = entry.file_name().to_string_lossy().to_string();
                                            if name.starts_with("gb_")
                                                && !valid_names.contains(&name)
                                                && !valid_names.is_empty()
                                            {
                                                let _ = fs::remove_file(entry.path());
                                            }
                                        }
                                        persist_source_images_bg(
                                            &portable,
                                            &mod_root_path,
                                            &profile_for_work,
                                            &blocking_http_client,
                                            |cover_rel_path| {
                                                let _ = cover_tx.send(
                                                    InstallEvent::SyncImagesCover {
                                                        mod_entry_id: cover_mod_entry_id
                                                            .clone(),
                                                        cover_rel_path: cover_rel_path
                                                            .to_string(),
                                                    },
                                                );
                                            },
                                        )
                                    })
                                    .map_err(|err| anyhow!("failed to start image-sync thread: {err}"))?
                                    .join()
                                    .map_err(|_| anyhow!("image-sync thread panicked"))?
                            })
                            .await;
                        match result {
                            Ok(Ok(rel_paths)) => {
                                let _ = tx.send(InstallEvent::SyncImagesDone {
                                    _job_id: job_id,
                                    mod_entry_id,
                                    profile,
                                    rel_paths,
                                    profile_cached_at,
                                });
                            }
                            Ok(Err(err)) => {
                                let _ = tx.send(InstallEvent::InstallFailed {
                                    job_id,
                                    preferred_name: "Mod Sync".to_string(),
                                    error: format!("{err:#}"),
                                });
                            }
                            Err(err) => {
                                let _ = tx.send(InstallEvent::InstallFailed {
                                    job_id,
                                    preferred_name: "Mod Sync".to_string(),
                                    error: format!("sync-images worker join error: {err}"),
                                });
                            }
                        }
                    }
                    InstallRequest::Cancel { job_id } => {
                        if let Some(flag) = cancel_flags.lock().unwrap().get(&job_id) {
                            flag.store(true, Ordering::Relaxed);
                        }
                    }
                    InstallRequest::Drop { job_id } => {
                        prepared_cache.lock().unwrap().remove(&job_id);
                        cancel_flags.lock().unwrap().remove(&job_id);
                    }
                }
            }
        });
    }

    runtime_services.spawn(async move {
        while let Some(request) = rx.recv().await {
            if worker_txs.is_empty() {
                break;
            }
            let job_id = match &request {
                InstallRequest::Inspect { job_id, .. } => *job_id,
                InstallRequest::SplitFrameworkParts { job_id, .. } => *job_id,
                InstallRequest::Install { job_id, .. } => *job_id,
                InstallRequest::PrepareThaliaBundle { job_id, .. } => *job_id,
                InstallRequest::SyncImages { job_id, .. } => *job_id,
                InstallRequest::Cancel { job_id } => *job_id,
                InstallRequest::Drop { job_id } => *job_id,
            };
            let index = (job_id as usize) % worker_txs.len();
            let _ = worker_txs[index].send(request);
        }
    });
}

fn framework_scans_for_prepared_import(
    prepared: &PreparedImport,
    context: &frameworks::InstallContext,
) -> Vec<frameworks::CandidateScan> {
    let candidate_roots: Vec<PathBuf> = prepared
        .inspection
        .candidates
        .iter()
        .map(|candidate| candidate.path.clone())
        .collect();
    let extraction_root = prepared
        ._temp_dir
        .as_ref()
        .filter(|_| prepared.source_is_archive)
        .map(|temp_dir| temp_dir.path());
    frameworks::scan_candidates(&candidate_roots, extraction_root, context)
}

struct SplitFrameworkChild {
    job_id: u64,
    part_index: usize,
    path: PathBuf,
    label: String,
}

const SPLIT_FRAMEWORK_FOLDER: &str = "framework";

fn split_framework_parts(
    mut prepared: PreparedImport,
    parts: Vec<frameworks::SplitPart>,
) -> (PreparedImport, Vec<(SplitFrameworkChild, PreparedImport)>) {
    let extraction_root = prepared
        ._temp_dir
        .as_ref()
        .filter(|_| prepared.source_is_archive)
        .map(|temp_dir| temp_dir.path().to_path_buf());
    let mut children = Vec::new();
    let mut split_candidates = Vec::new();
    for (part_index, part) in parts.iter().enumerate() {
        let Some(candidate) = prepared.inspection.candidates.get(part.candidate_index) else {
            continue;
        };
        let source = match &part.relative_dir {
            Some(relative_dir) => candidate.path.join(relative_dir),
            None => candidate.path.clone(),
        };
        let source_is_movable = extraction_root
            .as_deref()
            .is_some_and(|root| source != root && source.starts_with(root));
        if (part.relative_dir.is_some() && !source_is_movable)
            || extraction_root.as_deref() == Some(source.as_path())
            || !source.is_dir()
        {
            continue;
        }
        let label = match &part.relative_dir {
            Some(_) => source
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .unwrap_or_default(),
            None => candidate.label.clone(),
        };
        if label.trim().is_empty() {
            continue;
        }
        let extract_dir = persistence::runtime_temp_extract_dir();
        if fs::create_dir_all(&extract_dir).is_err() {
            continue;
        }
        let Ok(temp_dir) = tempfile::Builder::new()
            .prefix("framework-")
            .tempdir_in(&extract_dir)
        else {
            continue;
        };
        let destination = temp_dir.path().join(SPLIT_FRAMEWORK_FOLDER);
        let moved = if source_is_movable {
            fs::rename(&source, &destination).is_ok()
                || (importing::copy_dir(&source, &destination, true).is_ok()
                    && fs::remove_dir_all(&source).is_ok())
        } else {
            importing::copy_dir(&source, &destination, true).is_ok()
        };
        if !moved {
            continue;
        }
        if part.relative_dir.is_none() {
            split_candidates.push(part.candidate_index);
        }
        children.push((
            SplitFrameworkChild {
                job_id: part.new_job_id,
                part_index,
                path: destination.clone(),
                label: label.clone(),
            },
            PreparedImport {
                _temp_dir: Some(temp_dir),
                inspection: ImportInspection {
                    game_id: prepared.inspection.game_id.clone(),
                    candidates: vec![crate::model::ImportCandidate {
                        label,
                        path: destination,
                    }],
                    notice: None,
                },
                source_is_archive: true,
            },
        ));
    }
    split_candidates.sort_unstable();
    split_candidates.dedup();
    for candidate_index in split_candidates.into_iter().rev() {
        prepared.inspection.candidates.remove(candidate_index);
    }
    (prepared, children)
}
