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
    use rayon::prelude::*;
    use std::sync::Mutex;

    let mut file_targets: HashMap<String, Vec<StartupPathTargetKind>> =
        HashMap::with_capacity(targets.len());
    for target in targets {
        for file_name in target.file_names {
            file_targets
                .entry(file_name.to_ascii_lowercase())
                .or_default()
                .push(target.kind.clone());
        }
    }

    let seen = Mutex::new(HashSet::with_capacity(128));
    let file_targets = Arc::new(file_targets);
    
    for root in startup_scan_roots() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        
        // Collect entries first to avoid contention with WalkDir iterator
        let entries: Vec<_> = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();
        
        // Process entries in parallel
        entries.par_iter().for_each(|entry| {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            
            let Some(file_name) = entry.file_name().to_str() else {
                return;
            };
            let Some(kinds) = file_targets.get(&file_name.to_ascii_lowercase()) else {
                return;
            };
            
            let path = entry.path().to_path_buf();
            let mut seen = seen.lock().unwrap();
            for kind in kinds {
                if seen.insert((kind.clone(), path.clone())) {
                    let _ = event_tx.send(StartupPathScanEvent::Found {
                        kind: kind.clone(),
                        path: path.clone(),
                    });
                }
            }
        });
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
