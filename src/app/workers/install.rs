fn spawn_install_workers(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<InstallRequest>,
    tx: WorkerTx<InstallEvent>,
) {
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
        runtime_services.spawn(async move {
            while let Some(request) = worker_rx.recv().await {
                match request {
                    InstallRequest::Inspect {
                        job_id,
                        game_id,
                        source,
                        gb_profile,
                    } => {
                        let cancel = {
                            let mut flags = cancel_flags.lock().unwrap();
                            flags
                                .entry(job_id)
                                .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                                .clone()
                        };
                        let result = handle
                            .spawn_blocking(move || importing::inspect_source_cancelable(&game_id, source, &cancel))
                            .await;
                        match result {
                            Ok(Ok(prepared)) => {
                                let inspection = prepared.inspection.clone();
                                prepared_cache.lock().unwrap().insert(job_id, prepared);
                                let _ = tx.send(InstallEvent::InspectReady { job_id, inspection, gb_profile });
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
                    InstallRequest::Install {
                        job_id,
                        candidate_indices,
                        preferred_names,
                        choice,
                        target_root,
                        gb_profile,
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
                            .spawn_blocking(move || -> Result<(Vec<PathBuf>, Vec<String>)> {
                                fs::create_dir_all(&target_root)?;
                                let mut installed_paths = Vec::new();
                                let mut target_cleaned = HashSet::new();

                                for (i, &idx) in candidate_indices.iter().enumerate() {
                                    let preferred_name = &preferred_names[i];
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

                                    let installed_path = if current_choice == ConflictChoice::Replace {
                                        let temp_target =
                                            target_root.join(format!(".hestia_tmp_{}_{}", job_id, i));
                                        if temp_target.exists() {
                                            fs::remove_dir_all(&temp_target)?;
                                        }
                                        importing::copy_dir_cancelable(
                                            &candidate.path,
                                            &temp_target,
                                            false,
                                            &cancel,
                                        )?;
                                        if cancel.load(Ordering::Relaxed) {
                                            let _ = fs::remove_dir_all(&temp_target);
                                            bail!(importing::CANCELLED_ERROR);
                                        }
                                        let live_target = target_root.join(preferred_name);
                                        if live_target.exists() {
                                            trash::delete(&live_target)?;
                                        }
                                        fs::rename(&temp_target, &live_target)?;
                                        live_target
                                    } else {
                                        importing::install_candidate_cancelable(
                                            &candidate.path,
                                            preferred_name,
                                            &target_root,
                                            current_choice,
                                            &cancel,
                                        )?
                                        .ok_or_else(|| anyhow!("Import cancelled"))?
                                    };
                                    if !installed_paths.contains(&installed_path) {
                                        installed_paths.push(installed_path);
                                    }
                                }

                                Ok((installed_paths, Vec::new()))
                            })
                            .await;
                        match result {
                            Ok(Ok((installed_paths, rel_paths))) => {
                                let _ = tx.send(InstallEvent::InstallDone {
                                    job_id,
                                    installed_paths,
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
                    InstallRequest::SyncImages {
                        job_id,
                        mod_entry_id,
                        mod_root_path,
                        profile,
                    } => {
                        let profile_for_work = profile.clone();
                        let portable = portable.clone();
                        let result = handle
                            .spawn_blocking(move || -> Result<Vec<String>> {
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
                                let client = shared_blocking_http_client()?;
                                persist_source_images_bg(
                                    &portable,
                                    &mod_root_path,
                                    &profile_for_work,
                                    &client,
                                )
                            })
                            .await;
                        match result {
                            Ok(Ok(rel_paths)) => {
                                let _ = tx.send(InstallEvent::SyncImagesDone {
                                    _job_id: job_id,
                                    mod_entry_id,
                                    profile,
                                    rel_paths,
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
                InstallRequest::Install { job_id, .. } => *job_id,
                InstallRequest::SyncImages { job_id, .. } => *job_id,
                InstallRequest::Cancel { job_id } => *job_id,
                InstallRequest::Drop { job_id } => *job_id,
            };
            let index = (job_id as usize) % worker_txs.len();
            let _ = worker_txs[index].send(request);
        }
    });
}

