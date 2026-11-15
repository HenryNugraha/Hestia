const FRAMEWORK_MISSING_FILE_SCAN_LIMIT: usize = 30;
const FRAMEWORK_QUIET_FETCH_CONCURRENCY: usize = 2;

enum FrameworkRequest {
    WarmFacts {
        game_id: String,
        folders: Vec<frameworks::InstalledFolder>,
        importer_root: Option<PathBuf>,
        mods_root: Option<PathBuf>,
    },
    Classify {
        page_id: u64,
        file_id: u64,
    },
    QuietFetch {
        mod_id: String,
        game_id: String,
        page_id: u64,
        is_tool: bool,
        file: Box<gamebanana::ModFile>,
        installed: frameworks::InstalledFolder,
        cache_limit_bytes: u64,
    },
    AskNewer {
        mod_id: String,
        page_id: u64,
        is_tool: bool,
        installed_file_id: u64,
        installed_page_id: u64,
    },
    FindMissing {
        game_id: String,
        mod_id: String,
        page_id: u64,
        names: Vec<String>,
    },
}

enum FrameworkNewerAnswer {
    Answered(Box<thalia::UpdateMatchResponse>),
    CannotTell,
    TryLater,
}

enum FrameworkEvent {
    FactsReady {
        game_id: String,
        loader_namespaces: std::collections::BTreeSet<String>,
    },
    Classified {
        page_id: u64,
        file_id: u64,
        answer: Option<Box<thalia::UpdateMatchResponse>>,
    },
    QuietFetched {
        mod_id: String,
        page_id: u64,
        is_tool: bool,
        file_id: u64,
        compared_ini_hash: Option<String>,
        inside_check: Option<frameworks::InsideCheck>,
    },
    NewerAnswered {
        mod_id: String,
        page_id: u64,
        is_tool: bool,
        compared_file_id: u64,
        answer: FrameworkNewerAnswer,
    },
    MissingFound {
        game_id: String,
        mod_id: String,
        page_id: u64,
        names: Vec<String>,
        found: Option<Vec<(String, gamebanana::ModFile)>>,
    },
}

fn spawn_framework_worker(
    runtime_services: &RuntimeServices,
    portable: PortablePaths,
    mut rx: WorkerRx<FrameworkRequest>,
    tx: WorkerTx<FrameworkEvent>,
) {
    let runtime_services = runtime_services.clone();
    let json_limiter = Arc::clone(&runtime_services.json_limiter);
    runtime_services.clone().spawn(async move {
        let handle = runtime_services.handle();
        let thalia_lane = Arc::new(Semaphore::new(1));
        let fetch_lane = Arc::new(Semaphore::new(FRAMEWORK_QUIET_FETCH_CONCURRENCY));
        while let Some(request) = rx.recv().await {
            let tx = tx.clone();
            match request {
                FrameworkRequest::WarmFacts {
                    game_id,
                    folders,
                    importer_root,
                    mods_root,
                } => {
                    let handle = handle.clone();
                    tokio::spawn(async move {
                        let loader_namespaces = handle
                            .spawn_blocking(move || {
                                for folder in &folders {
                                    folder.facts();
                                }
                                match (importer_root, mods_root) {
                                    (Some(importer_root), Some(mods_root)) => {
                                        frameworks::loader_namespace_keys(
                                            &importer_root,
                                            &mods_root,
                                        )
                                    }
                                    _ => std::collections::BTreeSet::new(),
                                }
                            })
                            .await
                            .unwrap_or_default();
                        let _ = tx.send(FrameworkEvent::FactsReady {
                            game_id,
                            loader_namespaces,
                        });
                    });
                }
                FrameworkRequest::Classify { page_id, file_id } => {
                    let services = runtime_services.clone();
                    let thalia_lane = Arc::clone(&thalia_lane);
                    tokio::spawn(async move {
                        let _lane = thalia_lane.acquire().await.ok();
                        let answer = framework_thalia_answer(&services, page_id, file_id, None)
                            .await
                            .ok()
                            .map(Box::new);
                        let _ = tx.send(FrameworkEvent::Classified {
                            page_id,
                            file_id,
                            answer,
                        });
                    });
                }
                FrameworkRequest::QuietFetch {
                    mod_id,
                    game_id,
                    page_id,
                    is_tool,
                    file,
                    installed,
                    cache_limit_bytes,
                } => {
                    let services = runtime_services.clone();
                    let portable = portable.clone();
                    let fetch_lane = Arc::clone(&fetch_lane);
                    tokio::spawn(async move {
                        let _lane = fetch_lane.acquire().await.ok();
                        let file_id = file.id;
                        let compared_ini_hash = installed.ini_hash.clone();
                        let inside_check = quiet_framework_fetch(
                            &services,
                            &portable,
                            game_id,
                            page_id,
                            &file,
                            installed,
                            cache_limit_bytes,
                        )
                        .await
                        .ok();
                        let _ = tx.send(FrameworkEvent::QuietFetched {
                            mod_id,
                            page_id,
                            is_tool,
                            file_id,
                            compared_ini_hash,
                            inside_check,
                        });
                    });
                }
                FrameworkRequest::AskNewer {
                    mod_id,
                    page_id,
                    is_tool,
                    installed_file_id,
                    installed_page_id,
                } => {
                    let services = runtime_services.clone();
                    let thalia_lane = Arc::clone(&thalia_lane);
                    tokio::spawn(async move {
                        let _lane = thalia_lane.acquire().await.ok();
                        let also_page_id =
                            (installed_page_id != page_id).then_some(installed_page_id);
                        let answer = match framework_thalia_answer(
                            &services,
                            page_id,
                            installed_file_id,
                            also_page_id,
                        )
                        .await
                        {
                            Ok(response) => FrameworkNewerAnswer::Answered(Box::new(response)),
                            Err(error) if error.is_permanent() => FrameworkNewerAnswer::CannotTell,
                            Err(_) => FrameworkNewerAnswer::TryLater,
                        };
                        let _ = tx.send(FrameworkEvent::NewerAnswered {
                            mod_id,
                            page_id,
                            is_tool,
                            compared_file_id: installed_file_id,
                            answer,
                        });
                    });
                }
                FrameworkRequest::FindMissing {
                    game_id,
                    mod_id,
                    page_id,
                    names,
                } => {
                    let client = runtime_services.http_client();
                    let portable = portable.clone();
                    let json_limiter = Arc::clone(&json_limiter);
                    tokio::spawn(async move {
                        let _permit = json_limiter.acquire().await.ok();
                        let found =
                            find_missing_frameworks(&client, &portable, page_id, &names).await;
                        let _ = tx.send(FrameworkEvent::MissingFound {
                            game_id,
                            mod_id,
                            page_id,
                            names,
                            found,
                        });
                    });
                }
            }
        }
    });
}

async fn framework_thalia_answer(
    services: &RuntimeServices,
    page_id: u64,
    file_id: u64,
    also_page_id: Option<u64>,
) -> Result<thalia::UpdateMatchResponse, thalia::UpdateMatchError> {
    let client = services
        .async_client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| thalia::UpdateMatchError::Transport(error.to_string()))?;
    let pacer = thalia_shared_pacer();
    let cancellation = Arc::new(AtomicBool::new(false));
    let started = AtomicBool::new(false);
    fetch_paced_thalia_answer(&pacer, &cancellation, &started, || {
        thalia::fetch_update_match(
            &client,
            thalia::UpdateMatchRequest {
                mod_id: page_id,
                installed_file_id: file_id,
                also_mod_id: also_page_id,
            },
            false,
            Arc::clone(&cancellation),
        )
    })
    .await
}

async fn quiet_framework_fetch(
    services: &RuntimeServices,
    portable: &PortablePaths,
    game_id: String,
    page_id: u64,
    file: &gamebanana::ModFile,
    installed: frameworks::InstalledFolder,
    cache_limit_bytes: u64,
) -> Result<frameworks::InsideCheck> {
    let url = file
        .download_url
        .clone()
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| anyhow!("the file has no download address"))?;
    let job = BrowseDownloadJob {
        task_id: 0,
        title: file.file_name.clone(),
        url,
        cache_key: HestiaApp::browse_download_cache_key(page_id, &file.file_name, file.file_size),
        file_name: file.file_name.clone(),
        cache_limit_bytes,
        total_size: (file.file_size > 0).then_some(file.file_size),
        md5: file.md5.clone(),
    };
    let handle = services.handle();
    let cancel = Arc::new(AtomicBool::new(false));
    let cached = {
        let portable = portable.clone();
        let key = job.cache_key.clone();
        let total_size = job.total_size;
        let md5 = job.md5.clone();
        handle
            .spawn_blocking(move || {
                persistence::cache_get(&portable, &key)
                    .ok()
                    .flatten()
                    .is_some_and(|bytes| {
                        total_size.is_none_or(|expected| bytes.len() as u64 == expected)
                            && md5
                                .as_deref()
                                .is_none_or(|md5| importing::bundle::bytes_match_md5(&bytes, md5))
                    })
            })
            .await
            .unwrap_or(false)
    };
    if !cached {
        let progress = Arc::new(RwLock::new(DownloadProgress {
            downloaded: 0,
            total: job.total_size,
            speed: 0.0,
            last_update: std::time::Instant::now(),
            bytes_since_last: 0,
        }));
        let client = services.download_http_client();
        download_browse_file_to_cache_with_retries(
            &client,
            portable,
            &job,
            &cancel,
            progress,
            handle.clone(),
        )
        .await?;
        verify_browse_download_md5(portable, &job, &cancel, &handle).await?;
    }
    let check_portable = portable.clone();
    let cache_key = job.cache_key.clone();
    let file_name = job.file_name.clone();
    let inside_check = handle
        .spawn_blocking(move || -> Result<frameworks::InsideCheck> {
            let temp_dir = persistence::runtime_temp_downloads_dir();
            fs::create_dir_all(&temp_dir)?;
            let archive = temp_dir.join(format!(
                "framework-check-{}-{}",
                hash64_hex(cache_key.as_bytes()),
                sanitize_folder_name(&file_name)
            ));
            persistence::cache_link_or_copy_to_file(&check_portable, &cache_key, &archive)?;
            let inspected = importing::inspect_source_cancelable(
                &game_id,
                ImportSource::Archive(archive.clone()),
                &cancel,
            );
            let _ = fs::remove_file(&archive);
            let prepared = inspected?;
            let mine = installed.facts();
            let inside_check = prepared
                .inspection
                .candidates
                .iter()
                .map(|candidate| frameworks::scan_folder(&candidate.path))
                .filter(frameworks::FolderFacts::framework_shaped)
                .map(|arriving| frameworks::inside_check(&mine, &arriving))
                .find(|check| *check != frameworks::InsideCheck::DifferentFramework)
                .unwrap_or(frameworks::InsideCheck::DifferentFramework);
            if matches!(
                inside_check,
                frameworks::InsideCheck::SameOrOlder | frameworks::InsideCheck::DifferentFramework
            ) {
                let _ = persistence::cache_remove(&check_portable, &cache_key);
            }
            Ok(inside_check)
        })
        .await??;
    Ok(inside_check)
}

async fn find_missing_frameworks(
    client: &ClientWithMiddleware,
    portable: &PortablePaths,
    page_id: u64,
    names: &[String],
) -> Option<Vec<(String, gamebanana::ModFile)>> {
    let profile = gamebanana::fetch_profile_async_typed(client, page_id, false)
        .await
        .ok()?;
    if gamebanana::is_unavailable(&profile) {
        return Some(Vec::new());
    }
    let mut files: Vec<gamebanana::ModFile> = profile
        .files
        .iter()
        .filter(|file| file.id != 0 && file.download_url.is_some())
        .cloned()
        .collect();
    files.sort_by_key(|file| std::cmp::Reverse(file.date_added));
    files.truncate(FRAMEWORK_MISSING_FILE_SCAN_LIMIT);
    let listings =
        framework_file_listings(client, portable, files.iter().map(|file| file.id).collect()).await;
    Some(
        names
            .iter()
            .filter_map(|name| {
                let signature = frameworks::FrameworkSignature::new([name.as_str()], std::iter::empty());
                files
                    .iter()
                    .find(|file| {
                        matches!(
                            listings.get(&file.id),
                            Some(gamebanana::RawFileList::Paths(paths))
                                if frameworks::listing_holds_standalone_copy(paths, &file.file_name, &signature)
                        )
                    })
                    .map(|file| (name.clone(), file.clone()))
            })
            .collect(),
    )
}

async fn framework_file_listings(
    client: &ClientWithMiddleware,
    portable: &PortablePaths,
    file_ids: Vec<u64>,
) -> HashMap<u64, gamebanana::RawFileList> {
    let mut listings: HashMap<u64, gamebanana::RawFileList> =
        cached_file_listings(portable, file_ids.clone())
            .await
            .into_iter()
            .collect();
    let to_fetch: Vec<u64> = file_ids
        .into_iter()
        .filter(|id| !listings.contains_key(id) && !file_listing_failed_recently(*id))
        .take(FILE_LISTING_FETCH_LIMIT)
        .collect();
    if to_fetch.is_empty() {
        return listings;
    }
    let deadline = tokio::time::Instant::now() + FILE_LISTING_FETCH_BUDGET;
    let mut answers =
        futures_util::stream::iter(to_fetch.iter().copied().map(|id| async move {
            (id, gamebanana::fetch_raw_file_list_async(client, id).await)
        }))
        .buffer_unordered(FILE_LISTING_FETCH_CONCURRENCY);
    let mut finished = Vec::new();
    while let Ok(Some((id, answer))) = tokio::time::timeout_at(deadline, answers.next()).await {
        if let Ok(answer) = answer {
            finished.push((id, answer));
        }
    }
    drop(answers);

    let now = std::time::Instant::now();
    let finished_ids: HashSet<u64> = finished.iter().map(|(id, _)| *id).collect();
    file_listing_failures().extend(
        to_fetch
            .iter()
            .filter(|id| !finished_ids.contains(id))
            .map(|id| (*id, now)),
    );
    let writes: Vec<(u64, Vec<u8>)> = finished
        .iter()
        .filter_map(|(id, answer)| Some((*id, serde_json::to_vec(answer).ok()?)))
        .collect();
    let write_portable = portable.clone();
    let _ = tokio::task::spawn_blocking(move || {
        for (id, bytes) in writes {
            let _ = persistence::cache_put(
                &write_portable,
                &file_listing_cache_key(id),
                "gamebanana-file-list",
                &bytes,
                0,
            );
        }
    })
    .await;
    listings.extend(finished);
    listings
}

fn framework_page_no_longer_exists(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<reqwest::Error>())
        .any(|cause| {
            matches!(
                cause.status().map(|status| status.as_u16()),
                Some(404 | 410)
            )
        })
}

fn framework_page_report(watch: &FrameworkWatch, page_gone: bool) -> FrameworkPageReport {
    FrameworkPageReport {
        mod_id: watch.mod_id.clone(),
        game_id: watch.game_id.clone(),
        page_id: watch.page_id,
        is_tool: watch.is_tool,
        new_copies: Vec::new(),
        other_ids: Vec::new(),
        known_copy_files: Vec::new(),
        page_gone,
    }
}

fn framework_page_gone_reports(watches: &[FrameworkWatch]) -> Vec<FrameworkPageReport> {
    watches
        .iter()
        .map(|watch| framework_page_report(watch, true))
        .collect()
}

fn framework_copy_ids_covered(watches: &[FrameworkWatch]) -> HashSet<u64> {
    watches
        .iter()
        .flat_map(|watch| watch.copy_ids.iter().copied())
        .collect()
}

async fn framework_page_reports(
    client: &ClientWithMiddleware,
    portable: &PortablePaths,
    profile: &gamebanana::ProfileResponse,
    watches: &[FrameworkWatch],
    tracked_by_outfits: &HashSet<u64>,
) -> (Vec<FrameworkPageReport>, HashSet<u64>) {
    let mut covered = framework_copy_ids_covered(watches);
    if watches.is_empty() {
        return (Vec::new(), covered);
    }
    if profile.is_deleted || profile.is_trashed {
        return (framework_page_gone_reports(watches), covered);
    }
    if gamebanana::is_unavailable(profile) {
        return (Vec::new(), covered);
    }
    let live_files: Vec<&gamebanana::ModFile> = profile
        .files
        .iter()
        .filter(|file| file.id != 0 && !file.is_archived && !tracked_by_outfits.contains(&file.id))
        .collect();
    let unseen_by = |watch: &FrameworkWatch, file_id: u64| {
        !watch.copy_ids.contains(&file_id) && !watch.other_ids.contains(&file_id)
    };
    let unseen_ids: Vec<u64> = live_files
        .iter()
        .map(|file| file.id)
        .filter(|file_id| watches.iter().any(|watch| unseen_by(watch, *file_id)))
        .collect();
    let listings = if unseen_ids.is_empty() {
        HashMap::new()
    } else {
        framework_file_listings(client, portable, unseen_ids).await
    };
    let mut reports = Vec::with_capacity(watches.len());
    for watch in watches {
        let mut report = framework_page_report(watch, false);
        for file in live_files.iter().filter(|file| unseen_by(watch, file.id)) {
            let holds_copy = match listings.get(&file.id) {
                Some(gamebanana::RawFileList::Paths(paths)) => {
                    frameworks::listing_holds_standalone_copy(
                        paths,
                        &file.file_name,
                        &watch.signature,
                    )
                }
                Some(gamebanana::RawFileList::Unavailable) => {
                    file.file_size <= frameworks::QUIET_FETCH_LIMIT_BYTES
                        && frameworks::title_names_framework(
                            &file.file_name,
                            watch.signature.names.iter().map(String::as_str),
                        )
                }
                None => continue,
            };
            if holds_copy {
                covered.insert(file.id);
                report.new_copies.push((*file).clone());
            } else {
                report.other_ids.push(file.id);
            }
        }
        report.known_copy_files = profile
            .files
            .iter()
            .chain(&profile.archived_files)
            .filter(|file| watch.copy_ids.contains(&file.id))
            .cloned()
            .collect();
        reports.push(report);
    }
    (reports, covered)
}
