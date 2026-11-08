use std::io::BufRead as _;

use crate::overlay_protocol;

const GAME_WATCH_INTERVAL: Duration = Duration::from_millis(1500);

const GAME_OVERLAY_CLOSE_GRACE: Duration = Duration::from_secs(3);

const SHARED_GAME_PROCESS_NAMES: &[&str] = &["client-win64-shipping.exe"];

fn other_game_process_names(game_id: &str) -> &'static [&'static str] {
    match game_id {
        "wuwa" => &["client-win64-shipping.exe"],
        _ => &[],
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchedGame {
    id: String,
    exe_names: Vec<String>,
    shared_names: Vec<String>,
    folder: Option<PathBuf>,
}

impl WatchedGame {
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct RunningGame {
    id: String,
    pids: Vec<u32>,
}

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

fn path_is_within(path: &Path, folder: &Path) -> bool {
    let mut parts = path.components();
    folder.components().all(|part| {
        parts
            .next()
            .is_some_and(|other| other.as_os_str().eq_ignore_ascii_case(part.as_os_str()))
    })
}

fn running_games(watched: &[WatchedGame]) -> Vec<RunningGame> {
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
    character: Option<u64>,
    mods: Vec<OverlayModDraft>,
}

#[derive(Clone, Debug, PartialEq)]
struct OverlayModDraft {
    id: String,
    name: String,
    active: bool,
    pictures: OverlayModPictures,
    censored: bool,
    gamebanana_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
struct OverlayModPictures {
    root: PathBuf,
    cover: Option<String>,
    screenshots: Vec<String>,
    gamebanana_preview: Option<String>,
    thumbnails: [Option<DateTime<Utc>>; 2],
}

impl OverlayLibraryDraft {
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

struct OverlayStartDraft {
    host_window: Option<i64>,
    game_pids: Vec<u32>,
    library: OverlayLibraryDraft,
    settings: overlay_protocol::Settings,
    selection: Option<overlay_protocol::Selection>,
}

#[derive(Default)]
struct OverlayOutgoing {
    start: Option<OverlayStartDraft>,
    library: Option<OverlayLibraryDraft>,
    game_pids: Option<Vec<u32>>,
    answers: Vec<overlay_protocol::Changed>,
    messages: Vec<overlay_protocol::ToOverlay>,
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

    fn close(&self) {
        self.update(|pending| pending.closed = true);
    }

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
    Exited,
}

struct OverlayProcess {
    game_id: String,
    game_pids: Vec<u32>,
    started: Instant,
    child: std::process::Child,
    outbox: Arc<OverlayOutbox>,
    events: std::sync::mpsc::Receiver<OverlayEvent>,
    reads: std::sync::mpsc::Sender<OverlayReadRequest>,
}

enum OverlayReadRequest {
    Hotkeys(String, PathBuf, HashMap<String, String>),
    Info(OverlayInfoDraft),
}

struct OverlayInfoDraft {
    mod_id: String,
    description_html: Option<String>,
    note_path: String,
    text_sources: Vec<crate::model::ExtractedMetadataTextSource>,
    selected: Option<MetadataSourceKind>,
    readme_path: Option<String>,
}

impl OverlayInfoDraft {
    fn new(entry: &ModEntry) -> Self {
        Self {
            mod_id: entry.id.clone(),
            description_html: mod_primary_description_html(entry).map(|(html, _)| html),
            note_path: xxmi::personal_note_relative_path(),
            text_sources: entry.metadata.extracted.text_sources.clone(),
            selected: entry.metadata.user.selected_metadata_source,
            readme_path: entry.metadata.extracted.readme_path.clone(),
        }
    }

    fn resolve(self) -> overlay_protocol::ModInfo {
        use overlay_protocol::{InfoKind, InfoSource};

        const MAX_CHARS: usize = 20_000;
        let clip = |text: String| match text.char_indices().nth(MAX_CHARS) {
            Some((end, _)) => format!("{}…", &text[..end]),
            None => text,
        };
        let description = self
            .description_html
            .as_deref()
            .map(|html| markdown_plain_text(&prepare_markdown_content(html)))
            .unwrap_or_default();
        let mut sources = vec![InfoSource {
            kind: InfoKind::Description,
            path: String::new(),
            label: String::new(),
            text: clip(description),
        }];
        let mut files = self.text_sources;
        files.sort_by_key(|source| source.path != self.note_path);
        for source in files {
            let note = source.path == self.note_path;
            let markdown = note || source.path.to_ascii_lowercase().ends_with(".md");
            let text = if markdown {
                markdown_plain_text(&source.content)
            } else {
                source.content.trim().to_owned()
            };
            let label = if source.label.trim().is_empty() {
                source.path.clone()
            } else {
                source.label
            };
            sources.push(InfoSource {
                kind: if note { InfoKind::Note } else { InfoKind::File },
                path: source.path,
                label,
                text: clip(text),
            });
        }
        let readme = self
            .readme_path
            .as_deref()
            .and_then(|path| sources.iter().position(|source| source.path == path));
        let shown = match self.selected {
            Some(MetadataSourceKind::Description) => Some(0),
            Some(MetadataSourceKind::TextFile) => readme,
            _ => None,
        }
        .or_else(|| (!sources[0].text.is_empty()).then_some(0))
        .or(readme)
        .or_else(|| sources.iter().position(|source| !source.text.is_empty()))
        .unwrap_or(0);
        overlay_protocol::ModInfo {
            mod_id: self.mod_id,
            sources,
            shown,
        }
    }
}

impl OverlayProcess {
    fn spawn(game: &RunningGame, start: OverlayStartDraft) -> std::io::Result<Self> {
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .arg(overlay_protocol::OVERLAY_ARG)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .env_remove("HESTIA_PROFILE_DUMP")
            .spawn()?;
        let outbox = Arc::new(OverlayOutbox::default());
        outbox.update(|pending| pending.start = Some(start));
        let (report, events) = std::sync::mpsc::channel();
        let (reads, read_requests) = std::sync::mpsc::channel();
        if let Err(error) = Self::link(&mut child, &outbox, report, read_requests) {
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
            reads,
        })
    }

    fn link(
        child: &mut std::process::Child,
        outbox: &Arc<OverlayOutbox>,
        report: std::sync::mpsc::Sender<OverlayEvent>,
        read_requests: std::sync::mpsc::Receiver<OverlayReadRequest>,
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
            .name("hestia-overlay-reads".to_owned())
            .spawn(move || read_mods(&outbox, &read_requests))?;
        Ok(())
    }

    fn outbox(&self) -> Arc<OverlayOutbox> {
        Arc::clone(&self.outbox)
    }

    fn send_library(&self, library: OverlayLibraryDraft) {
        self.outbox
            .update(|pending| pending.library = Some(library));
    }

    fn answer(&self, library: Option<OverlayLibraryDraft>, answer: overlay_protocol::Changed) {
        self.outbox.update(|pending| {
            if library.is_some() {
                pending.library = library;
            }
            pending.answers.push(answer);
        });
    }

    fn send_hotkeys(&self, mod_id: String, root: PathBuf, values: HashMap<String, String>) {
        let _ = self
            .reads
            .send(OverlayReadRequest::Hotkeys(mod_id, root, values));
    }

    fn send_info(&self, entry: &ModEntry) {
        let _ = self
            .reads
            .send(OverlayReadRequest::Info(OverlayInfoDraft::new(entry)));
    }

    fn answer_hotkey(&self, id: u64, error: Option<String>) {
        self.outbox
            .send(overlay_protocol::ToOverlay::HotkeyPressed { id, error });
    }

    fn send_settings(&self, settings: overlay_protocol::Settings) {
        self.outbox
            .send(overlay_protocol::ToOverlay::Settings(settings));
    }

    fn set_game_pids(&mut self, pids: Vec<u32>) {
        self.game_pids = pids.clone();
        self.outbox.update(|pending| pending.game_pids = Some(pids));
    }

    fn close(self) {
        xxmi_persist::set_game_overlay_process(None);
        self.outbox.close();
        let mut child = self.child;
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

fn read_mods(outbox: &OverlayOutbox, requests: &std::sync::mpsc::Receiver<OverlayReadRequest>) {
    for request in requests {
        let (mod_id, root, values) = match request {
            OverlayReadRequest::Hotkeys(mod_id, root, values) => (mod_id, root, values),
            OverlayReadRequest::Info(draft) => {
                outbox.send(overlay_protocol::ToOverlay::Info(draft.resolve()));
                continue;
            }
        };
        let rows = hotkeys_list_rows(&parse_mod_config_inis(&root));
        let hotkeys = rows
            .iter()
            .map(|row| overlay_protocol::ModHotkey {
                key: row.key.clone(),
                label: row.label.clone(),
                raw: row.raw_key.clone(),
                unavailable: hotkey_row_unavailable(row, &rows, &values),
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
