// The in-game overlay's side in Hestia: a thread that watches for supported
// games, and the overlay process with the threads that talk to it.  The
// overlay is `hestia --overlay`; `crate::overlay_protocol` has the messages.

use std::io::BufRead as _;

use crate::overlay_protocol;

/// How long the game watcher waits between looks at the running processes.
const GAME_WATCH_INTERVAL: Duration = Duration::from_millis(1500);

/// How long the overlay gets to close after Hestia asks, before Hestia ends
/// it.
const GAME_OVERLAY_CLOSE_GRACE: Duration = Duration::from_secs(3);

/// Unreal Engine names a game's process after its project, so other games
/// can have these names too.  They count only when they run from the game's
/// folder.
const SHARED_GAME_PROCESS_NAMES: &[&str] = &["client-win64-shipping.exe"];

/// Processes a game runs as besides its exe, lowercase.  Wuthering Waves' exe
/// starts the game as another process.
fn other_game_process_names(game_id: &str) -> &'static [&'static str] {
    match game_id {
        "wuwa" => &["client-win64-shipping.exe"],
        _ => &[],
    }
}

/// A supported game Hestia watches for, to show the in-game overlay while it
/// runs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchedGame {
    id: String,
    /// The game's own process names, lowercase.  Never its launcher's.
    exe_names: Vec<String>,
    /// The ones other games use too.
    shared_names: Vec<String>,
    /// The folder of the game's exe, when Hestia knows it.
    folder: Option<PathBuf>,
}

impl WatchedGame {
    /// None for a game without the overlay: a disabled one, or one Hestia
    /// doesn't mod with XXMI.
    fn for_game(game: &GameInstall) -> Option<Self> {
        if !game.enabled || !game.is_xxmi() {
            return None;
        }
        let id = game.definition.id.as_str();
        let vanilla = game.vanilla_exe_path();
        let launchers: Vec<String> = xxmi_launcher_file_names()
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .chain(
                game.modded_exe_path()
                    .as_deref()
                    .and_then(lowercase_file_name),
            )
            .collect();
        let mut exe_names: Vec<String> = other_game_process_names(id)
            .iter()
            .map(|name| (*name).to_owned())
            .chain(vanilla.as_deref().and_then(lowercase_file_name))
            .chain(
                vanilla_exe_file_names(id)
                    .into_iter()
                    .map(str::to_ascii_lowercase),
            )
            .filter(|name| !launchers.contains(name))
            .collect();
        exe_names.sort();
        exe_names.dedup();
        if exe_names.is_empty() {
            return None;
        }
        let shared_names = exe_names
            .iter()
            .filter(|name| SHARED_GAME_PROCESS_NAMES.contains(&name.as_str()))
            .cloned()
            .collect();
        Some(Self {
            id: id.to_owned(),
            exe_names,
            shared_names,
            folder: vanilla
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
        })
    }
}

fn lowercase_file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
}

/// A watched game that runs now.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RunningGame {
    id: String,
    /// In ascending order.
    pids: Vec<u32>,
}

/// The watched games that run now.  `processes` pairs each process id with
/// its lowercase name, in id order.  `exe_of` finds where a process runs from.
fn match_running_games(
    watched: &[WatchedGame],
    processes: &[(u32, String)],
    mut exe_of: impl FnMut(u32) -> Option<PathBuf>,
) -> Vec<RunningGame> {
    let mut running = Vec::new();
    for game in watched {
        let mut pids = Vec::new();
        for (pid, name) in processes {
            if !game.exe_names.contains(name) {
                continue;
            }
            // A game may not say where it runs from.  Count it then.
            if game.shared_names.contains(name)
                && let Some(folder) = &game.folder
                && exe_of(*pid).is_some_and(|exe| !path_is_within(&exe, folder))
            {
                continue;
            }
            pids.push(*pid);
        }
        if !pids.is_empty() {
            running.push(RunningGame {
                id: game.id.clone(),
                pids,
            });
        }
    }
    running
}

/// Whether `path` is in `folder`, ignoring case like Windows does.
fn path_is_within(path: &Path, folder: &Path) -> bool {
    let mut parts = path.components();
    folder.components().all(|part| {
        parts
            .next()
            .is_some_and(|other| other.as_os_str().eq_ignore_ascii_case(part.as_os_str()))
    })
}

fn running_games(watched: &[WatchedGame]) -> Vec<RunningGame> {
    // A fresh list each time: sysinfo keeps a process's first name, so a
    // reused process id would keep a game's name.
    let mut system = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );
    let mut processes: Vec<(u32, String)> = system
        .processes()
        .iter()
        .map(|(pid, process)| {
            (
                pid.as_u32(),
                process.name().to_string_lossy().to_ascii_lowercase(),
            )
        })
        .collect();
    processes.sort_unstable();
    match_running_games(watched, &processes, |pid| {
        let pid = sysinfo::Pid::from_u32(pid);
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            false,
            sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
        );
        system
            .process(pid)
            .and_then(|process| process.exe())
            .map(Path::to_path_buf)
    })
}

/// Watches for the supported games on its own thread, and reports the ones
/// that run whenever that changes.
struct GameWatcher {
    games: Arc<Mutex<Vec<WatchedGame>>>,
    running: std::sync::mpsc::Receiver<Vec<RunningGame>>,
}

impl GameWatcher {
    fn start(games: Vec<WatchedGame>) -> std::io::Result<Self> {
        let games = Arc::new(Mutex::new(games));
        let watched = Arc::clone(&games);
        let (report, running) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("hestia-game-watch".to_owned())
            .spawn(move || watch_games(&watched, &report))?;
        Ok(Self { games, running })
    }

    fn watch(&self, games: Vec<WatchedGame>) {
        *self
            .games
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = games;
    }
}

fn watch_games(
    games: &Mutex<Vec<WatchedGame>>,
    report: &std::sync::mpsc::Sender<Vec<RunningGame>>,
) {
    let mut reported = Vec::new();
    loop {
        let watched = games
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let running = if watched.is_empty() {
            Vec::new()
        } else {
            running_games(&watched)
        };
        if running != reported {
            if report.send(running.clone()).is_err() {
                return;
            }
            wake_ui();
            reported = running;
        }
        std::thread::sleep(GAME_WATCH_INTERVAL);
    }
}

/// The library for the in-game overlay, before its pictures are found.
/// Building it reads no files, so Hestia can compare it every few frames.
#[derive(Clone, Debug, Default, PartialEq)]
struct OverlayLibraryDraft {
    game_id: String,
    game_name: String,
    categories: Vec<OverlayCategoryDraft>,
}

#[derive(Clone, Debug, PartialEq)]
struct OverlayCategoryDraft {
    id: String,
    name: String,
    /// The GameBanana character the category stands for, whose icon is its
    /// picture.
    character: Option<u64>,
    mods: Vec<OverlayModDraft>,
}

#[derive(Clone, Debug, PartialEq)]
struct OverlayModDraft {
    id: String,
    name: String,
    active: bool,
    pictures: OverlayModPictures,
    /// An unsafe mod the library censors.
    censored: bool,
    /// The GameBanana mod it was installed from.
    gamebanana_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
struct OverlayModPictures {
    root: PathBuf,
    cover: Option<String>,
    screenshots: Vec<String>,
    gamebanana_preview: Option<String>,
    /// When Hestia last made the mod's thumbnails, so new ones get sent.
    thumbnails: [Option<DateTime<Utc>>; 2],
}

impl OverlayLibraryDraft {
    /// Finds the pictures, which reads the disk.
    fn resolve(self) -> overlay_protocol::Library {
        let characters = if self
            .categories
            .iter()
            .any(|category| category.id != overlay_protocol::UNCATEGORIZED_ID)
        {
            saved_characters(&self.game_id)
        } else {
            Vec::new()
        };
        overlay_protocol::Library {
            game_id: self.game_id,
            game_name: self.game_name,
            categories: self
                .categories
                .into_iter()
                .map(|category| {
                    // Without a link, the character the name stands for.
                    let character = match category.character {
                        Some(id) => characters.iter().find(|character| character.id == id),
                        None if category.id == overlay_protocol::UNCATEGORIZED_ID => None,
                        None => characters.iter().find(|character| {
                            overlay_protocol::names_character(&category.name, &character.name)
                        }),
                    };
                    overlay_protocol::Category {
                        image: character
                            .and_then(|character| character.icon_url.as_deref())
                            .map(|url| {
                                persistence::cache_file_path(&HestiaApp::browse_image_cache_key(
                                    url,
                                ))
                            })
                            .filter(|path| path.is_file()),
                        character: category
                            .character
                            .or(character.map(|character| character.id)),
                        id: category.id,
                        name: category.name,
                        mods: category
                            .mods
                            .into_iter()
                            .map(OverlayModDraft::resolve)
                            .collect(),
                    }
                })
                .collect(),
        }
    }
}

impl OverlayModDraft {
    fn resolve(self) -> overlay_protocol::Mod {
        let pictures = self.pictures;
        let downloaded = pictures
            .gamebanana_preview
            .as_deref()
            .map(|url| persistence::cache_file_path(&HestiaApp::browse_image_cache_key(url)));
        let image = crate::overlay_preview::mod_image(
            &pictures.root,
            pictures.cover.as_deref(),
            &pictures.screenshots,
            downloaded,
        );
        overlay_protocol::Mod {
            id: self.id,
            name: self.name,
            image,
            active: self.active,
            censored: self.censored,
            gamebanana_id: self.gamebanana_id,
        }
    }
}

/// The game's GameBanana characters, from what Hestia saved when Browse or
/// the overlay listed them.  Empty until one has.
fn saved_characters(game_id: &str) -> Vec<gamebanana::CharacterCategory> {
    let Some(super_category) = gamebanana::character_super_category_id_for_hestia(game_id) else {
        return Vec::new();
    };
    let key = gamebanana::character_categories_cache_key(game_id, super_category);
    let Ok(bytes) = fs::read(persistence::cache_file_path(&key)) else {
        return Vec::new();
    };
    let mut characters: Vec<gamebanana::CharacterCategory> =
        serde_json::from_slice(&bytes).unwrap_or_default();
    characters.retain(|character| !character.is_obsolete);
    for character in &mut characters {
        character.icon_url = character
            .icon_url
            .take()
            .filter(|url| !url.trim().is_empty());
    }
    characters
}

/// What the overlay needs first.
struct OverlayStartDraft {
    host_window: Option<i64>,
    game_pids: Vec<u32>,
    library: OverlayLibraryDraft,
    settings: overlay_protocol::Settings,
    selection: Option<overlay_protocol::Selection>,
}

/// What Hestia has for the overlay that isn't written yet.  Only the newest
/// library matters, so a new one replaces one still waiting.  Answers go
/// after the library, so the overlay has the change when its answer arrives.
#[derive(Default)]
struct OverlayOutgoing {
    start: Option<OverlayStartDraft>,
    library: Option<OverlayLibraryDraft>,
    game_pids: Option<Vec<u32>>,
    answers: Vec<overlay_protocol::Changed>,
    /// GameBanana pages and characters, in the order Hestia got them.
    messages: Vec<overlay_protocol::ToOverlay>,
    /// GameBanana pictures downloaded since the last write.
    pictures: overlay_protocol::Pictures,
    closed: bool,
}

impl OverlayOutgoing {
    fn is_empty(&self) -> bool {
        self.start.is_none()
            && self.library.is_none()
            && self.game_pids.is_none()
            && self.answers.is_empty()
            && self.messages.is_empty()
            && self.pictures.mods.is_empty()
            && self.pictures.characters.is_empty()
            && !self.closed
    }
}

#[derive(Default)]
struct OverlayOutbox {
    pending: Mutex<OverlayOutgoing>,
    news: std::sync::Condvar,
}

impl OverlayOutbox {
    fn update(&self, change: impl FnOnce(&mut OverlayOutgoing)) {
        change(
            &mut self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        self.news.notify_one();
    }

    fn send(&self, message: overlay_protocol::ToOverlay) {
        self.update(|pending| pending.messages.push(message));
    }

    fn add_pictures(&self, pictures: overlay_protocol::Pictures) {
        self.update(|pending| {
            pending.pictures.mods.extend(pictures.mods);
            pending.pictures.characters.extend(pictures.characters);
        });
    }

    /// Ends the link, which tells the overlay to close.
    fn close(&self) {
        self.update(|pending| pending.closed = true);
    }

    /// Waits for something to write.
    fn take(&self) -> OverlayOutgoing {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while pending.is_empty() {
            pending = self
                .news
                .wait(pending)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        std::mem::take(&mut *pending)
    }
}

enum OverlayEvent {
    Message(overlay_protocol::FromOverlay),
    /// The overlay's output closed, which happens when it exits.
    Exited,
}

/// The in-game overlay, a second Hestia process that shows over one game.
struct OverlayProcess {
    game_id: String,
    game_pids: Vec<u32>,
    started: Instant,
    child: std::process::Child,
    outbox: Arc<OverlayOutbox>,
    events: std::sync::mpsc::Receiver<OverlayEvent>,
    /// Mods whose hotkeys the overlay asked for, with their folders.
    hotkeys: std::sync::mpsc::Sender<(String, PathBuf)>,
}

impl OverlayProcess {
    fn spawn(game: &RunningGame, start: OverlayStartDraft) -> std::io::Result<Self> {
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .arg(overlay_protocol::OVERLAY_ARG)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            // A profile capture is of the main window.
            .env_remove("HESTIA_PROFILE_DUMP")
            .spawn()?;
        let outbox = Arc::new(OverlayOutbox::default());
        outbox.update(|pending| pending.start = Some(start));
        let (report, events) = std::sync::mpsc::channel();
        let (hotkeys, hotkey_requests) = std::sync::mpsc::channel();
        if let Err(error) = Self::link(&mut child, &outbox, report, hotkey_requests) {
            outbox.close();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        xxmi_persist::set_game_overlay_process(Some(child.id()));
        Ok(Self {
            game_id: game.id.clone(),
            game_pids: game.pids.clone(),
            started: Instant::now(),
            child,
            outbox,
            events,
            hotkeys,
        })
    }

    fn link(
        child: &mut std::process::Child,
        outbox: &Arc<OverlayOutbox>,
        report: std::sync::mpsc::Sender<OverlayEvent>,
        hotkey_requests: std::sync::mpsc::Receiver<(String, PathBuf)>,
    ) -> std::io::Result<()> {
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(std::io::Error::other("the overlay has no pipes"));
        };
        let writer_outbox = Arc::clone(outbox);
        std::thread::Builder::new()
            .name("hestia-overlay-writer".to_owned())
            .spawn(move || write_to_overlay(&writer_outbox, stdin))?;
        std::thread::Builder::new()
            .name("hestia-overlay-reader".to_owned())
            .spawn(move || read_from_overlay(stdout, &report))?;
        let outbox = Arc::clone(outbox);
        std::thread::Builder::new()
            .name("hestia-overlay-hotkeys".to_owned())
            .spawn(move || read_hotkeys(&outbox, &hotkey_requests))?;
        Ok(())
    }

    fn outbox(&self) -> Arc<OverlayOutbox> {
        Arc::clone(&self.outbox)
    }

    fn send_library(&self, library: OverlayLibraryDraft) {
        self.outbox
            .update(|pending| pending.library = Some(library));
    }

    /// Answers a change, after the library when there is one.
    fn answer(&self, library: Option<OverlayLibraryDraft>, answer: overlay_protocol::Changed) {
        self.outbox.update(|pending| {
            if library.is_some() {
                pending.library = library;
            }
            pending.answers.push(answer);
        });
    }

    /// Reads the mod's hotkeys and sends them to the overlay.
    fn send_hotkeys(&self, mod_id: String, root: PathBuf) {
        let _ = self.hotkeys.send((mod_id, root));
    }

    fn send_settings(&self, settings: overlay_protocol::Settings) {
        self.outbox
            .send(overlay_protocol::ToOverlay::Settings(settings));
    }

    fn set_game_pids(&mut self, pids: Vec<u32>) {
        self.game_pids = pids.clone();
        self.outbox.update(|pending| pending.game_pids = Some(pids));
    }

    /// Asks the overlay to close, and ends it if it hasn't after a moment.
    fn close(self) {
        xxmi_persist::set_game_overlay_process(None);
        self.outbox.close();
        let mut child = self.child;
        // Without this thread the overlay still closes, since it exits by
        // itself once Hestia's link ends.
        let _ = std::thread::Builder::new()
            .name("hestia-overlay-close".to_owned())
            .spawn(move || {
                let deadline = Instant::now() + GAME_OVERLAY_CLOSE_GRACE;
                while Instant::now() < deadline {
                    if !matches!(child.try_wait(), Ok(None)) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                tracing::warn!("The in-game overlay did not close, ending it");
                let _ = child.kill();
                let _ = child.wait();
            });
    }

    /// Ends the overlay now.
    fn end(mut self) {
        xxmi_persist::set_game_overlay_process(None);
        self.outbox.close();
        let _ = self.child.kill();
    }
}

fn write_to_overlay(outbox: &OverlayOutbox, mut stdin: std::process::ChildStdin) {
    loop {
        let pending = outbox.take();
        if pending.closed {
            // Dropping stdin ends the link.
            return;
        }
        let pictures = pending.pictures;
        let pictures = (!pictures.mods.is_empty() || !pictures.characters.is_empty())
            .then_some(overlay_protocol::ToOverlay::Pictures(pictures));
        let later = pending
            .answers
            .into_iter()
            .map(overlay_protocol::ToOverlay::Changed)
            .chain(pending.messages)
            .chain(pictures)
            .map(Some);
        let messages = [
            pending.start.map(|start| {
                overlay_protocol::ToOverlay::Start(overlay_protocol::Start {
                    host_pid: std::process::id(),
                    host_window: start.host_window,
                    game_pids: start.game_pids,
                    library: start.library.resolve(),
                    settings: start.settings,
                    selection: start.selection,
                })
            }),
            pending
                .library
                .map(|library| overlay_protocol::ToOverlay::Library(library.resolve())),
            pending
                .game_pids
                .map(|pids| overlay_protocol::ToOverlay::GameProcesses { pids }),
        ];
        for message in messages.into_iter().chain(later).flatten() {
            let written = overlay_protocol::encode(&message)
                .map_err(std::io::Error::from)
                .and_then(|line| writeln!(stdin, "{line}"))
                .and_then(|()| stdin.flush());
            if let Err(error) = written {
                tracing::debug!(%error, "The in-game overlay stopped reading");
                return;
            }
        }
    }
}

/// Reads mods' hotkeys, the way the library lists them, one mod at a time,
/// so the last answer about a mod is about it as it is now.  Ends with the
/// overlay.
fn read_hotkeys(outbox: &OverlayOutbox, requests: &std::sync::mpsc::Receiver<(String, PathBuf)>) {
    for (mod_id, root) in requests {
        let hotkeys = hotkeys_list_rows(&parse_mod_config_inis(&root))
            .into_iter()
            .map(|row| overlay_protocol::ModHotkey {
                key: row.key,
                label: row.label,
            })
            .collect();
        outbox.send(overlay_protocol::ToOverlay::Hotkeys(
            overlay_protocol::ModHotkeys { mod_id, hotkeys },
        ));
    }
}

fn read_from_overlay(
    stdout: std::process::ChildStdout,
    report: &std::sync::mpsc::Sender<OverlayEvent>,
) {
    for line in std::io::BufReader::new(stdout).lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(_) => break,
        };
        match overlay_protocol::decode::<overlay_protocol::FromOverlay>(&line) {
            Ok(message) => {
                if report.send(OverlayEvent::Message(message)).is_err() {
                    return;
                }
                wake_ui();
            }
            Err(error) => tracing::debug!(%error, "Ignoring a line from the in-game overlay"),
        }
    }
    let _ = report.send(OverlayEvent::Exited);
    wake_ui();
}

#[cfg(test)]
mod game_overlay_watch_tests {
    use super::*;

    fn game(
        id: &str,
        backend: GameBackend,
        vanilla: Option<&str>,
        modded: Option<&str>,
    ) -> GameInstall {
        GameInstall {
            definition: crate::model::GameDefinition {
                id: id.to_owned(),
                name: id.to_owned(),
                backend,
                xxmi_code: String::new(),
            },
            mods_path_override: None,
            modded_exe_path_override: modded.map(PathBuf::from),
            vanilla_exe_path_override: vanilla.map(PathBuf::from),
            apply_mod_changes_in_game: false,
            enabled: true,
        }
    }

    #[test]
    fn only_enabled_xxmi_games_are_watched_and_never_by_their_launcher() {
        let genshin = game(
            "genshin",
            GameBackend::Xxmi,
            Some(r"D:\Genshin Impact\Genshin Impact Game\GenshinImpact.exe"),
            Some(r"D:\XXMI\Resources\Bin\XXMI Launcher.exe"),
        );
        let watched = WatchedGame::for_game(&genshin).unwrap();
        assert!(watched.exe_names.contains(&"genshinimpact.exe".to_owned()));
        assert!(watched.exe_names.contains(&"yuanshen.exe".to_owned()));
        assert!(watched.shared_names.is_empty());
        assert_eq!(
            watched.folder.as_deref(),
            Some(Path::new(r"D:\Genshin Impact\Genshin Impact Game"))
        );

        // Pointed at the launcher by mistake, and a modded exe with another
        // name: neither is taken for the game.
        let endfield = game(
            "endfield",
            GameBackend::Xxmi,
            Some(r"D:\XXMI\XXMI Launcher.exe"),
            Some(r"D:\Loader\EFMI Loader.exe"),
        );
        let watched = WatchedGame::for_game(&endfield).unwrap();
        assert!(watched.exe_names.contains(&"endfield.exe".to_owned()));
        assert!(!watched.exe_names.contains(&"xxmi launcher.exe".to_owned()));
        assert!(!watched.exe_names.contains(&"efmi loader.exe".to_owned()));

        let mut disabled = genshin.clone();
        disabled.enabled = false;
        assert_eq!(WatchedGame::for_game(&disabled), None);
        let unreal = game(
            "nte",
            GameBackend::UnrealEngine,
            Some(r"D:\NTE\HTGame.exe"),
            None,
        );
        assert_eq!(WatchedGame::for_game(&unreal), None);
    }

    #[test]
    fn shared_process_names_count_only_from_the_game_folder() {
        let wuwa = game(
            "wuwa",
            GameBackend::Xxmi,
            Some(r"D:\Wuthering Waves\Wuthering Waves Game\Wuthering Waves.exe"),
            None,
        );
        let watched = [WatchedGame::for_game(&wuwa).unwrap()];
        assert_eq!(watched[0].shared_names, ["client-win64-shipping.exe"]);
        let processes = [
            (4, "wuthering waves.exe".to_owned()),
            (8, "client-win64-shipping.exe".to_owned()),
            (12, "client-win64-shipping.exe".to_owned()),
            (16, "client-win64-shipping.exe".to_owned()),
            (20, "notepad.exe".to_owned()),
        ];
        let running = match_running_games(&watched, &processes, |pid| match pid {
            8 => Some(PathBuf::from(
                r"d:\wuthering waves\Wuthering Waves Game\Client\Binaries\Win64\Client-Win64-Shipping.exe",
            )),
            12 => Some(PathBuf::from(
                r"E:\Other Game\Client\Binaries\Win64\Client-Win64-Shipping.exe",
            )),
            // Where it runs from is unknown.
            _ => None,
        });
        assert_eq!(
            running,
            [RunningGame {
                id: "wuwa".to_owned(),
                pids: vec![4, 8, 16],
            }]
        );
        assert!(match_running_games(&watched, &processes[4..], |_| None).is_empty());
    }
}
