static STARTUP_PATH_SCAN_SEMAPHORE: Semaphore = Semaphore::const_new(1);

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
        if cancel_for_scan.load(Ordering::Relaxed) {
            let _ = event_tx.send(StartupPathScanEvent::Finished { stopped: true });
            return;
        }
        let Ok(scan_permit) = STARTUP_PATH_SCAN_SEMAPHORE.acquire().await else {
            cancel_for_scan.store(true, Ordering::Relaxed);
            let _ = event_tx.send(StartupPathScanEvent::Finished { stopped: true });
            return;
        };
        if cancel_for_scan.load(Ordering::Relaxed) {
            drop(scan_permit);
            let _ = event_tx.send(StartupPathScanEvent::Finished { stopped: true });
            return;
        }
        let result = scan_runtime
            .spawn_blocking(move || {
                let _scan_permit = scan_permit;
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

    let stopped = if cancel.load(Ordering::Relaxed) {
        true
    } else {
        scan_path_roots(file_targets, startup_scan_roots(), &cancel, &event_tx)
    };
    let _ = event_tx.send(StartupPathScanEvent::Finished { stopped });
}

fn scan_path_roots(
    file_targets: HashMap<String, Vec<StartupPathTargetKind>>,
    roots: Vec<PathBuf>,
    cancel: &AtomicBool,
    event_tx: &WorkerTx<StartupPathScanEvent>,
) -> bool {
    scan_path_roots_with_entry_hook(file_targets, roots, cancel, event_tx, |_, _| {})
}

fn scan_path_roots_with_entry_hook<F>(
    file_targets: HashMap<String, Vec<StartupPathTargetKind>>,
    roots: Vec<PathBuf>,
    cancel: &AtomicBool,
    event_tx: &WorkerTx<StartupPathScanEvent>,
    mut after_file_type: F,
) -> bool
where
    F: FnMut(&fs::DirEntry, &fs::FileType),
{
    let mut seen = HashSet::with_capacity(128);

    for root in roots {
        if cancel.load(Ordering::Relaxed) {
            return true;
        }

        // Keep only paths in this stack. At most one directory iterator is alive
        // at a time, so cancellation is checked between every filesystem step.
        let mut pending_directories = vec![root];
        while let Some(directory) = pending_directories.pop() {
            if cancel.load(Ordering::Relaxed) {
                return true;
            }

            let mut entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            if cancel.load(Ordering::Relaxed) {
                return true;
            }

            loop {
                let entry = match next_scan_entry(&mut entries, cancel) {
                    ScanStep::Cancelled => return true,
                    ScanStep::End => break,
                    ScanStep::Item(Ok(entry)) => entry,
                    ScanStep::Item(Err(_)) => continue,
                };

                if cancel.load(Ordering::Relaxed) {
                    return true;
                }
                let file_type = entry.file_type();
                let Ok(file_type) = file_type else {
                    if cancel.load(Ordering::Relaxed) {
                        return true;
                    }
                    continue;
                };
                if cancel.load(Ordering::Relaxed) {
                    return true;
                }
                after_file_type(&entry, &file_type);
                if cancel.load(Ordering::Relaxed) {
                    return true;
                }

                if file_type.is_symlink() {
                    continue;
                }
                if file_type.is_dir() {
                    // `file_type` does not follow directory symlinks or junctions,
                    // matching WalkDir::follow_links(false).
                    pending_directories.push(entry.path());
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }

                let file_name = entry.file_name();
                let Some(file_name) = file_name.to_str() else {
                    continue;
                };
                let Some(kinds) = file_targets.get(&file_name.to_ascii_lowercase()) else {
                    continue;
                };

                let path = entry.path().to_path_buf();
                for kind in kinds {
                    if cancel.load(Ordering::Relaxed) {
                        return true;
                    }
                    if seen.insert((kind.clone(), path.clone())) {
                        if cancel.load(Ordering::Relaxed) {
                            return true;
                        }
                        let _ = event_tx.send(StartupPathScanEvent::Found {
                            kind: kind.clone(),
                            path: path.clone(),
                        });
                    }
                }
            }
        }
    }

    cancel.load(Ordering::Relaxed)
}

enum ScanStep<T> {
    Item(T),
    End,
    Cancelled,
}

fn next_scan_entry<I>(entries: &mut I, cancel: &AtomicBool) -> ScanStep<I::Item>
where
    I: Iterator<Item = std::io::Result<fs::DirEntry>>,
{
    if cancel.load(Ordering::Relaxed) {
        return ScanStep::Cancelled;
    }

    let next = entries.next();
    if cancel.load(Ordering::Relaxed) {
        return ScanStep::Cancelled;
    }

    match next {
        Some(entry) => ScanStep::Item(entry),
        None => ScanStep::End,
    }
}

fn startup_scan_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let drive_mask = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
        (0..26u32)
            .filter_map(|index| {
                (drive_mask & (1u32 << index) != 0).then(|| {
                    let letter = (b'A' + index as u8) as char;
                    PathBuf::from(format!("{letter}:\\"))
                })
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
    use std::{
        fs,
        sync::atomic::{AtomicBool, AtomicUsize},
    };

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
        let (event_tx, mut event_rx) = worker_channel();
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
    fn scans_nested_siblings_case_insensitively_and_deduplicates() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first").join("TARGET.EXE");
        let second = temp.path().join("second").join("target.exe");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(second.parent().unwrap()).unwrap();
        fs::write(&first, []).unwrap();
        fs::write(&second, []).unwrap();
        let (event_tx, mut event_rx) = worker_channel();
        let cancel = AtomicBool::new(false);
        let kind = StartupPathTargetKind::Game("test-game".to_string());
        let targets = HashMap::from([("target.exe".to_string(), vec![kind.clone(), kind])]);

        let stopped = scan_path_roots(targets, vec![temp.path().to_path_buf()], &cancel, &event_tx);

        assert!(!stopped);
        let mut found_paths = Vec::new();
        while let Ok(StartupPathScanEvent::Found { path, .. }) = event_rx.try_recv() {
            found_paths.push(path);
        }
        found_paths.sort();
        assert_eq!(found_paths, vec![first, second]);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let executable = outside.path().join("nested").join("target.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, []).unwrap();
        symlink(outside.path(), temp.path().join("linked")).unwrap();
        let (event_tx, mut event_rx) = worker_channel();
        let cancel = AtomicBool::new(false);

        let stopped = scan_path_roots(
            file_targets(),
            vec![temp.path().to_path_buf()],
            &cancel,
            &event_tx,
        );

        assert!(!stopped);
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn stops_before_more_filesystem_steps_when_entry_processing_cancels() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("nested").join("target.exe"), []).unwrap();
        let (event_tx, mut event_rx) = worker_channel();
        let cancel = AtomicBool::new(false);
        let mut file_type_calls = 0;

        let stopped = scan_path_roots_with_entry_hook(
            file_targets(),
            vec![temp.path().to_path_buf()],
            &cancel,
            &event_tx,
            |_, _| {
                file_type_calls += 1;
                cancel.store(true, Ordering::Relaxed);
            },
        );

        assert!(stopped);
        assert_eq!(file_type_calls, 1);
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn cancellation_during_entry_error_stops_before_next_entry() {
        let cancel = AtomicBool::new(false);
        let next_calls = AtomicUsize::new(0);
        let mut entries = std::iter::once(Err::<fs::DirEntry, std::io::Error>(
            std::io::Error::new(std::io::ErrorKind::Other, "test entry error"),
        ))
        .map(|entry| {
            next_calls.fetch_add(1, Ordering::Relaxed);
            cancel.store(true, Ordering::Relaxed);
            entry
        });

        assert!(matches!(
            next_scan_entry(&mut entries, &cancel),
            ScanStep::Cancelled
        ));
        assert_eq!(next_calls.load(Ordering::Relaxed), 1);
        assert!(matches!(
            next_scan_entry(&mut entries, &cancel),
            ScanStep::Cancelled
        ));
        assert_eq!(next_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stops_before_walking_when_cancellation_is_requested() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("target.exe"), []).unwrap();
        let (event_tx, mut event_rx) = worker_channel();
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
