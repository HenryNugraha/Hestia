fn spawn_startup_path_scan_worker(
    runtime_services: &RuntimeServices,
    targets: Vec<StartupPathScanTarget>,
    cancel: Arc<AtomicBool>,
    event_tx: WorkerTx<StartupPathScanEvent>,
) {
    let scan_runtime = runtime_services.handle();
    runtime_services.spawn(async move {
        let cancel_for_scan = Arc::clone(&cancel);
        let result = scan_runtime
            .spawn_blocking(move || {
                run_startup_path_scan(targets, cancel_for_scan, event_tx);
            })
            .await;
        if result.is_err() {
            cancel.store(true, Ordering::Relaxed);
        }
    });
}

fn run_startup_path_scan(
    targets: Vec<StartupPathScanTarget>,
    cancel: Arc<AtomicBool>,
    event_tx: WorkerTx<StartupPathScanEvent>,
) {
    let mut file_targets: HashMap<String, Vec<StartupPathTargetKind>> = HashMap::new();
    for target in targets {
        for file_name in target.file_names {
            file_targets
                .entry(file_name.to_ascii_lowercase())
                .or_default()
                .push(target.kind.clone());
        }
    }

    let mut seen = HashSet::new();
    for root in startup_scan_roots() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        for entry in WalkDir::new(root).follow_links(false).into_iter() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let Some(file_name) = entry.file_name().to_str() else {
                continue;
            };
            let Some(kinds) = file_targets.get(&file_name.to_ascii_lowercase()) else {
                continue;
            };
            let path = entry.path().to_path_buf();
            for kind in kinds {
                if seen.insert((kind.clone(), path.clone())) {
                    let _ = event_tx.send(StartupPathScanEvent::Found {
                        kind: kind.clone(),
                        path: path.clone(),
                    });
                }
            }
        }
    }

    let _ = event_tx.send(StartupPathScanEvent::Finished {
        stopped: cancel.load(Ordering::Relaxed),
    });
}

fn startup_scan_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        (b'A'..=b'Z')
            .filter_map(|letter| {
                let root = PathBuf::from(format!("{}:\\", letter as char));
                root.is_dir().then_some(root)
            })
            .collect()
    }

    #[cfg(not(windows))]
    {
        vec![PathBuf::from("/")]
    }
}
