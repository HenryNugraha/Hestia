// Finding files that other programs hold open below a directory tree.
//
// On Windows (NTFS) a directory cannot be renamed while any file or directory below it has an
// open handle, whatever share mode that handle was opened with: the rename fails with
// `ERROR_ACCESS_DENIED`. A profile switch renames the live mod roots wholesale, so the worker
// probes the tree before mutating anything and again when a rename has failed. Opening an entry
// with an exclusive share mode fails with a sharing violation exactly when some other handle is
// open, which makes the probe cheap and side-effect free. The Restart Manager then names the
// processes holding the reported files, which is what the user needs to unblock the switch.
//
// Other platforms rename freely around open handles, so the probe reports nothing there.

/// A process holding one of the probed files open.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FileLockHolder {
    name: String,
    pid: u32,
}

/// Entries below `root` that other handles keep open, with the processes holding them. `holders`
/// can be empty even when `paths` is not: directories held by a shell's working directory and
/// files held by the kernel or a driver are not attributed by the Restart Manager.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OpenHandleReport {
    root: PathBuf,
    paths: Vec<PathBuf>,
    holders: Vec<FileLockHolder>,
}

impl OpenHandleReport {
    /// Distinct holder names in first-seen order.
    fn holder_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for holder in &self.holders {
            if !names.iter().any(|name| name.eq_ignore_ascii_case(&holder.name)) {
                names.push(&holder.name);
            }
        }
        names
    }

    /// The root's own folder name, which is what a user recognizes (`Mods`, `Mods_Archived`).
    fn root_label(&self) -> String {
        self.root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.display().to_string())
    }
}

impl std::fmt::Display for OpenHandleReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "files below {} are in use", self.root.display())?;
        let names = self.holder_names();
        if !names.is_empty() {
            write!(f, " by {}", names.join(", "))?;
        }
        if !self.paths.is_empty() {
            let listed: Vec<String> = self
                .paths
                .iter()
                .map(|path| {
                    path.strip_prefix(&self.root)
                        .unwrap_or(path)
                        .display()
                        .to_string()
                })
                .collect();
            write!(f, " ({})", listed.join(", "))?;
        }
        Ok(())
    }
}

/// How many held entries a report lists. Enough to tell the user what to close; the Restart
/// Manager call that follows scales with this count, so it stays small.
const OPEN_HANDLE_REPORT_LIMIT: usize = 16;

/// Whether an io error means another handle is open on the path (or below it, for a directory
/// rename) rather than a permissions problem. Only Windows reports these codes for open handles.
fn io_error_is_file_in_use(error: &std::io::Error) -> bool {
    // ERROR_ACCESS_DENIED is what a directory rename returns for an open handle below it;
    // ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION are what file opens and renames return.
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

/// Probes every entry below `root` (including `root` itself) and reports the ones another handle
/// keeps open, with the processes holding them. `None` when nothing is held or when the platform
/// does not care. Honors `cancel` between probes so a canceled operation returns promptly.
fn detect_open_handles(root: &Path, cancel: &AtomicBool) -> Option<OpenHandleReport> {
    let paths = find_open_handles(root, OPEN_HANDLE_REPORT_LIMIT, cancel);
    if paths.is_empty() {
        return None;
    }
    let files: Vec<PathBuf> = paths
        .iter()
        .filter(|path| path.is_file())
        .cloned()
        .collect();
    let own_pid = std::process::id();
    let holders = lock_holders(&files)
        .into_iter()
        .filter(|holder| holder.pid != own_pid)
        .collect();
    Some(OpenHandleReport {
        root: root.to_path_buf(),
        paths,
        holders,
    })
}

/// Entries below `root` that some other handle keeps open, up to `limit`. The tree is listed
/// first and probed afterwards: the directory iterator holds its own handle on every ancestor
/// while walking, and probing during the walk would report those as held.
#[cfg(windows)]
fn find_open_handles(root: &Path, limit: usize, cancel: &AtomicBool) -> Vec<PathBuf> {
    if limit == 0 || !root.exists() {
        return Vec::new();
    }
    // Files first: they are what the Restart Manager can attribute, and the deepest held file
    // is more useful to a user than the chain of directories above it. The walker already knows
    // each entry's type; asking the filesystem again per entry would cost more than the probes.
    let mut entries: Vec<PathBuf> = Vec::new();
    let mut directories: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if entry.file_type().is_dir() {
            directories.push(entry.into_path());
        } else {
            entries.push(entry.into_path());
        }
    }
    entries.append(&mut directories);
    // Each probe is one open syscall that spends most of its time in filter drivers, so the
    // walk is embarrassingly parallel; a big library probes in a fraction of a second instead
    // of several. Threads stop as soon as the limit is reached or the operation is canceled.
    let threads = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, OPEN_HANDLE_PROBE_THREADS);
    let chunk_size = entries.len().div_ceil(threads).max(1);
    let found = std::sync::atomic::AtomicUsize::new(0);
    let mut held: Vec<(usize, PathBuf)> = std::thread::scope(|scope| {
        let workers: Vec<_> = entries
            .chunks(chunk_size)
            .enumerate()
            .map(|(chunk_index, chunk)| {
                let found = &found;
                scope.spawn(move || {
                    let mut held = Vec::new();
                    for (offset, path) in chunk.iter().enumerate() {
                        if cancel.load(Ordering::Relaxed) || found.load(Ordering::Relaxed) >= limit
                        {
                            break;
                        }
                        if path_has_open_handle(path) {
                            found.fetch_add(1, Ordering::Relaxed);
                            held.push((chunk_index * chunk_size + offset, path.clone()));
                        }
                    }
                    held
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    });
    held.sort_by_key(|(index, _)| *index);
    held.truncate(limit);
    held.into_iter().map(|(_, path)| path).collect()
}

#[cfg(windows)]
const OPEN_HANDLE_PROBE_THREADS: usize = 8;

#[cfg(not(windows))]
fn find_open_handles(_root: &Path, _limit: usize, _cancel: &AtomicBool) -> Vec<PathBuf> {
    Vec::new()
}

/// Opens `path` for read with no sharing. An existing handle with read, write or delete access
/// makes that fail with a sharing violation; `FILE_FLAG_BACKUP_SEMANTICS` lets the same probe
/// open directories. Access-denied (ACLs) and not-found are not "held" and are ignored.
#[cfg(windows)]
fn path_has_open_handle(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    match std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    {
        Ok(_) => false,
        Err(error) => matches!(error.raw_os_error(), Some(32 | 33)),
    }
}

/// Asks the Restart Manager which processes hold `files` open. Best effort: any failure yields
/// an empty list and the caller still reports the paths.
#[cfg(windows)]
fn lock_holders(files: &[PathBuf]) -> Vec<FileLockHolder> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows::Win32::System::RestartManager::{
        CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
        RmStartSession,
    };
    use windows::core::PWSTR;

    if files.is_empty() {
        return Vec::new();
    }
    let wide: Vec<Vec<u16>> = files
        .iter()
        .map(|path| {
            path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();
    let resources: Vec<PCWSTR> = wide.iter().map(|name| PCWSTR(name.as_ptr())).collect();

    let mut session = 0u32;
    let mut session_key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
    // SAFETY: plain Win32 calls with valid, live buffers; the session is always ended below.
    unsafe {
        if RmStartSession(&mut session, None, PWSTR(session_key.as_mut_ptr())) != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut holders = Vec::new();
        if RmRegisterResources(session, Some(&resources), None, None) == ERROR_SUCCESS {
            let mut needed = 0u32;
            let mut count = 0u32;
            let mut reasons = 0u32;
            let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
            let mut status = RmGetList(session, &mut needed, &mut count, None, &mut reasons);
            // The list can grow between the size query and the fetch; a few rounds cover it.
            for _ in 0..4 {
                if status != ERROR_MORE_DATA {
                    break;
                }
                infos.clear();
                infos.resize(needed as usize, std::mem::zeroed());
                count = needed;
                status = RmGetList(
                    session,
                    &mut needed,
                    &mut count,
                    Some(infos.as_mut_ptr()),
                    &mut reasons,
                );
            }
            if status == ERROR_SUCCESS {
                infos.truncate(count as usize);
                for info in &infos {
                    let len = info
                        .strAppName
                        .iter()
                        .position(|unit| *unit == 0)
                        .unwrap_or(info.strAppName.len());
                    let name = String::from_utf16_lossy(&info.strAppName[..len]);
                    if !name.is_empty() {
                        holders.push(FileLockHolder {
                            name,
                            pid: info.Process.dwProcessId,
                        });
                    }
                }
            }
        }
        let _ = RmEndSession(session);
        holders
    }
}

#[cfg(not(windows))]
fn lock_holders(_files: &[PathBuf]) -> Vec<FileLockHolder> {
    Vec::new()
}

#[cfg(test)]
mod file_lock_tests {
    use super::*;

    #[test]
    fn report_lists_distinct_holders_and_paths_relative_to_the_root() {
        let report = OpenHandleReport {
            root: PathBuf::from("C:/Game/Mods"),
            paths: vec![
                PathBuf::from("C:/Game/Mods/Skin/a.ini"),
                PathBuf::from("C:/Game/Mods/Skin"),
            ],
            holders: vec![
                FileLockHolder {
                    name: "Notepad".to_string(),
                    pid: 1,
                },
                FileLockHolder {
                    name: "notepad".to_string(),
                    pid: 2,
                },
                FileLockHolder {
                    name: "Explorer".to_string(),
                    pid: 3,
                },
            ],
        };

        assert_eq!(report.holder_names(), vec!["Notepad", "Explorer"]);
        assert_eq!(report.root_label(), "Mods");
        let text = report.to_string();
        assert!(text.contains("in use by Notepad, Explorer"), "{text}");
        assert!(text.contains("Skin"), "{text}");
        assert!(!text.contains("C:/Game/Mods/Skin/a.ini"), "{text}");
    }

    #[test]
    fn in_use_errors_are_the_windows_sharing_codes_only() {
        let denied = std::io::Error::from_raw_os_error(5);
        let sharing = std::io::Error::from_raw_os_error(32);
        let missing = std::io::Error::from_raw_os_error(2);
        assert_eq!(io_error_is_file_in_use(&denied), cfg!(windows));
        assert_eq!(io_error_is_file_in_use(&sharing), cfg!(windows));
        assert!(!io_error_is_file_in_use(&missing));
        assert!(!io_error_is_file_in_use(&std::io::Error::other("nope")));
    }

    #[cfg(windows)]
    #[test]
    fn probe_reports_held_files_and_directories_and_nothing_else() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Mods");
        std::fs::create_dir_all(root.join("Skin")).unwrap();
        std::fs::create_dir_all(root.join("Idle")).unwrap();
        std::fs::write(root.join("Skin").join("held.ini"), b"x").unwrap();
        std::fs::write(root.join("Skin").join("free.ini"), b"y").unwrap();
        std::fs::write(root.join("Idle").join("free.dds"), b"z").unwrap();
        let cancel = AtomicBool::new(false);

        assert!(find_open_handles(&root, 16, &cancel).is_empty());
        assert!(detect_open_handles(&root, &cancel).is_none());

        // A plain read handle, opened with every share flag, still counts: that is exactly the
        // kind of handle that blocks the directory rename.
        let held = std::fs::File::open(root.join("Skin").join("held.ini")).unwrap();
        let report = detect_open_handles(&root, &cancel).expect("held file is reported");
        assert_eq!(report.root, root);
        assert_eq!(report.paths, vec![root.join("Skin").join("held.ini")]);
        // Our own handle is filtered out of the holders, so nothing is attributed.
        assert!(report.holders.is_empty(), "{:?}", report.holders);
        drop(held);

        // A directory handle (what a shell's working directory or a watcher holds).
        let watcher = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x0200_0000)
            .open(root.join("Idle"))
            .unwrap();
        let held_now = find_open_handles(&root, 16, &cancel);
        assert_eq!(held_now, vec![root.join("Idle")]);
        drop(watcher);

        assert!(find_open_handles(&root, 16, &cancel).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn probe_stops_at_the_limit_and_on_cancel() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Mods");
        std::fs::create_dir_all(&root).unwrap();
        let mut handles = Vec::new();
        for index in 0..4 {
            let path = root.join(format!("{index}.ini"));
            std::fs::write(&path, b"x").unwrap();
            handles.push(std::fs::File::open(&path).unwrap());
        }
        let cancel = AtomicBool::new(false);
        assert_eq!(find_open_handles(&root, 2, &cancel).len(), 2);
        cancel.store(true, Ordering::Relaxed);
        assert!(find_open_handles(&root, 16, &cancel).is_empty());
        drop(handles);
    }
}
