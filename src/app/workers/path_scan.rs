fn spawn_startup_path_scan_worker(
    runtime_services: &RuntimeServices,
    targets: Vec<StartupPathScanTarget>,
    cancel: Arc<AtomicBool>,
    event_tx: WorkerTx<StartupPathScanEvent>,
) {
    let scan_runtime = runtime_services.handle();
    runtime_services.spawn(async move {
        let cancel_for_scan = Arc::clone(&cancel);
        let recovery_event_tx = event_tx.clone();
        let result = scan_runtime
            .spawn_blocking(move || {
                run_startup_path_scan(targets, cancel_for_scan, event_tx);
            })
            .await;
        if result.is_err() {
            cancel.store(true, Ordering::Relaxed);
            let _ = recovery_event_tx.send(StartupPathScanEvent::Finished { stopped: true });
        }
    });
}

fn run_startup_path_scan(
    targets: Vec<StartupPathScanTarget>,
    cancel: Arc<AtomicBool>,
    event_tx: WorkerTx<StartupPathScanEvent>,
) {
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

    let stopped = scan_path_roots(file_targets, startup_scan_roots(), &cancel, &event_tx);
    let _ = event_tx.send(StartupPathScanEvent::Finished { stopped });
}

fn scan_path_roots(
    file_targets: HashMap<String, Vec<StartupPathTargetKind>>,
    roots: Vec<PathBuf>,
    cancel: &AtomicBool,
    event_tx: &WorkerTx<StartupPathScanEvent>,
) -> bool {
    let mut seen = HashSet::with_capacity(128);

    for root in roots {
        if cancel.load(Ordering::Relaxed) {
            return true;
        }
        let entries = WalkDir::new(root).follow_links(false).into_iter();
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return true;
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

    cancel.load(Ordering::Relaxed)
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

#[cfg(test)]
mod path_scan_tests {
    use super::*;
    use std::{fs, sync::atomic::AtomicBool};

    fn file_targets() -> HashMap<String, Vec<StartupPathTargetKind>> {
        HashMap::from([(
            "target.exe".to_string(),
            vec![StartupPathTargetKind::Game("test-game".to_string())],
        )])
    }

    #[test]
    fn finds_matching_executable_while_walking() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("nested").join("target.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, []).unwrap();
        let (event_tx, mut event_rx) = tokio_mpsc::unbounded_channel();
        let cancel = AtomicBool::new(false);

        let stopped = scan_path_roots(
            file_targets(),
            vec![temp.path().to_path_buf()],
            &cancel,
            &event_tx,
        );

        assert!(!stopped);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(StartupPathScanEvent::Found { path, .. }) if path == executable
        ));
    }

    #[test]
    fn stops_before_walking_when_cancellation_is_requested() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("target.exe"), []).unwrap();
        let (event_tx, mut event_rx) = tokio_mpsc::unbounded_channel();
        let cancel = AtomicBool::new(true);

        let stopped = scan_path_roots(
            file_targets(),
            vec![temp.path().to_path_buf()],
            &cancel,
            &event_tx,
        );

        assert!(stopped);
        assert!(event_rx.try_recv().is_err());
    }
}
