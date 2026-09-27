// Runs the in-game overlay: Hestia starts it when a supported game starts,
// keeps its library current, and closes it when the game exits.  The game
// watcher and the overlay process are in workers/game_overlay.rs.

/// How often Hestia looks for library changes to send to the in-game overlay.
const GAME_OVERLAY_LIBRARY_INTERVAL: Duration = Duration::from_millis(250);

/// How long Hestia waits to start the in-game overlay again after it closed
/// unexpectedly or couldn't start, by how many times in a row that happened.
const GAME_OVERLAY_RESTART_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(30),
];

/// How long the overlay must run for a later close to count as a new
/// failure, not one more in a row.
const GAME_OVERLAY_GOOD_RUN: Duration = Duration::from_secs(60);

/// The in-game overlay, and what Hestia keeps for it.
#[derive(Default)]
struct GameOverlay {
    watcher: Option<GameWatcher>,
    /// The watcher's thread couldn't start, so there is no overlay.
    watcher_failed: bool,
    /// The games the watcher looks for.
    watched: Vec<WatchedGame>,
    /// The watched games that run now.
    running: Vec<RunningGame>,
    process: Option<OverlayProcess>,
    /// The library the overlay has.
    sent: Option<OverlayLibraryDraft>,
    next_library_check: Option<Instant>,
    /// Where the overlay was in each game's library, by game, for this
    /// session.
    selections: HashMap<String, overlay_protocol::Selection>,
    /// How many times in a row the overlay closed unexpectedly or couldn't
    /// start while its game ran.
    failures: usize,
    /// When to start it again after that.
    restart_at: Option<Instant>,
}

impl HestiaApp {
    /// Starts the in-game overlay when a supported game starts, keeps its
    /// library current, and closes it when the game exits.
    fn poll_game_overlay(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let watched: Vec<WatchedGame> = self
            .state
            .games
            .iter()
            .filter_map(WatchedGame::for_game)
            .collect();
        if watched != self.game_overlay.watched {
            if let Some(watcher) = &self.game_overlay.watcher {
                watcher.watch(watched.clone());
            }
            self.game_overlay.watched = watched;
        }
        if self.game_overlay.watcher.is_none()
            && !self.game_overlay.watcher_failed
            && !self.game_overlay.watched.is_empty()
        {
            match GameWatcher::start(self.game_overlay.watched.clone()) {
                Ok(watcher) => self.game_overlay.watcher = Some(watcher),
                Err(error) => {
                    self.game_overlay.watcher_failed = true;
                    self.log_warn(format!(
                        "Couldn't watch for games, so the in-game overlay is off: {error}"
                    ));
                }
            }
        }
        if let Some(watcher) = &self.game_overlay.watcher {
            while let Ok(running) = watcher.running.try_recv() {
                self.game_overlay.running = running;
            }
        }
        self.consume_game_overlay_events(ctx);

        // The overlay's game while it runs, or else the first one that runs.
        let overlay = &self.game_overlay;
        let shown = overlay
            .process
            .as_ref()
            .map(|process| process.game_id.as_str());
        let running: Vec<&RunningGame> = overlay
            .running
            .iter()
            .filter(|running| overlay.watched.iter().any(|game| game.id == running.id))
            .collect();
        let game = running
            .iter()
            .find(|running| Some(running.id.as_str()) == shown)
            .or(running.first())
            .map(|running| (*running).clone());

        match (&mut self.game_overlay.process, game) {
            (Some(process), Some(game)) if process.game_id == game.id => {
                if process.game_pids != game.pids {
                    process.set_game_pids(game.pids);
                }
                self.send_game_overlay_library(ctx);
            }
            (process @ Some(_), game) => {
                // Its game exited, or stopped being a supported one.
                if let Some(process) = process.take() {
                    tracing::info!(game = %process.game_id, "Closing the in-game overlay");
                    process.close();
                }
                self.game_overlay.sent = None;
                self.game_overlay.failures = 0;
                self.game_overlay.restart_at = None;
                if game.is_some() {
                    ctx.request_repaint();
                }
            }
            (None, Some(game)) => self.start_game_overlay(ctx, frame, &game),
            (None, None) => {
                self.game_overlay.failures = 0;
                self.game_overlay.restart_at = None;
            }
        }
    }

    fn consume_game_overlay_events(&mut self, ctx: &egui::Context) {
        let Some(process) = &self.game_overlay.process else {
            return;
        };
        let mut exited = false;
        while let Ok(event) = process.events.try_recv() {
            match event {
                OverlayEvent::Message(overlay_protocol::FromOverlay::Selection(selection)) => {
                    self.game_overlay
                        .selections
                        .insert(process.game_id.clone(), selection);
                }
                OverlayEvent::Exited => exited = true,
            }
        }
        if exited && let Some(process) = self.game_overlay.process.take() {
            // After a good run, start again with the shortest wait.
            if process.started.elapsed() > GAME_OVERLAY_GOOD_RUN {
                self.game_overlay.failures = 0;
            }
            process.close();
            self.game_overlay.sent = None;
            self.game_overlay_failed(ctx, "The in-game overlay closed unexpectedly".to_owned());
        }
    }

    fn start_game_overlay(
        &mut self,
        ctx: &egui::Context,
        frame: &eframe::Frame,
        running: &RunningGame,
    ) {
        let now = Instant::now();
        if let Some(restart_at) = self.game_overlay.restart_at
            && now < restart_at
        {
            ctx.request_repaint_after(restart_at - now);
            return;
        }
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == running.id)
        else {
            return;
        };
        let library = game_overlay_library(&self.state, game, self.text().uncategorized());
        let start = OverlayStartDraft {
            host_window: game_overlay_host_window(frame),
            game_pids: running.pids.clone(),
            library: library.clone(),
            selection: self.game_overlay.selections.get(&running.id).cloned(),
        };
        match OverlayProcess::spawn(running, start) {
            Ok(process) => {
                tracing::info!(game = %running.id, "Started the in-game overlay");
                self.game_overlay.process = Some(process);
                self.game_overlay.sent = Some(library);
                self.game_overlay.next_library_check = Some(now + GAME_OVERLAY_LIBRARY_INTERVAL);
            }
            Err(error) => {
                self.game_overlay_failed(
                    ctx,
                    format!("Couldn't start the in-game overlay: {error}"),
                );
            }
        }
    }

    /// Waits longer each time before the overlay starts again.
    fn game_overlay_failed(&mut self, ctx: &egui::Context, detail: String) {
        let overlay = &mut self.game_overlay;
        let delay = GAME_OVERLAY_RESTART_DELAYS
            [overlay.failures.min(GAME_OVERLAY_RESTART_DELAYS.len() - 1)];
        overlay.failures += 1;
        overlay.restart_at = Some(Instant::now() + delay);
        ctx.request_repaint_after(delay);
        tracing::warn!(retry_in = ?delay, "{detail}");
        // The log gets the first of a run of failures.
        if overlay.failures == 1 {
            self.log_warn(detail);
        }
    }

    /// Sends the library when it changed, a few times a second at most.
    fn send_game_overlay_library(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if let Some(check_at) = self.game_overlay.next_library_check
            && now < check_at
        {
            // Look once more then, for a change made meanwhile.
            ctx.request_repaint_after(check_at - now);
            return;
        }
        self.game_overlay.next_library_check = Some(now + GAME_OVERLAY_LIBRARY_INTERVAL);
        let Some(process) = &self.game_overlay.process else {
            return;
        };
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == process.game_id)
        else {
            return;
        };
        let library = game_overlay_library(&self.state, game, self.text().uncategorized());
        if self.game_overlay.sent.as_ref() != Some(&library) {
            process.send_library(library.clone());
            self.game_overlay.sent = Some(library);
        }
    }

    /// Ends the in-game overlay with Hestia.
    fn shut_down_game_overlay(&mut self) {
        if let Some(process) = self.game_overlay.process.take() {
            process.end();
        }
    }
}

/// The library the in-game overlay shows for `game`, sorted like the library:
/// the categories in their order and Uncategorized last, each with its mods.
/// Archived mods aren't in it, nor unsafe ones the library hides.
fn game_overlay_library(
    state: &AppState,
    game: &GameInstall,
    uncategorized: &str,
) -> OverlayLibraryDraft {
    let game_id = game.definition.id.as_str();
    let prefs = &state.static_prefs;
    let mut mods: Vec<&ModEntry> = state
        .mods
        .iter()
        .filter(|entry| entry.game_id == game_id)
        .collect();
    mods.sort_by(|a, b| compare_by_library_sort(prefs.library_sort, a, b));
    let mut members: HashMap<&str, Vec<&ModEntry>> = HashMap::new();
    let mut loose = Vec::new();
    for entry in mods {
        match effective_category_id(&state.categories, entry) {
            Some(category_id) => members.entry(category_id).or_default().push(entry),
            None => loose.push(entry),
        }
    }

    let hide_unsafe = matches!(
        prefs.unsafe_content_mode,
        UnsafeContentMode::HideNoCounter | UnsafeContentMode::HideShowCounter
    );
    let censor_unsafe = matches!(prefs.unsafe_content_mode, UnsafeContentMode::Censor);
    let shown_mods = |entries: Vec<&ModEntry>| -> Vec<OverlayModDraft> {
        entries
            .into_iter()
            .filter(|entry| {
                entry.status != ModStatus::Archived && !(hide_unsafe && entry.unsafe_content)
            })
            .map(|entry| overlay_mod_draft(entry, censor_unsafe && entry.unsafe_content))
            .collect()
    };

    let mut categories: Vec<ModCategory> = state
        .categories
        .iter()
        .filter(|category| category.game_id == game_id)
        .cloned()
        .collect();
    sort_categories_with_counts(
        &mut categories,
        state
            .category_sort_mode_by_game
            .get(game_id)
            .copied()
            .unwrap_or_default(),
        |category_id| category_member_count(&state.mods, game_id, category_id),
    );
    let super_category = gamebanana::character_super_category_id_for_hestia(game_id);
    let mut drafts: Vec<OverlayCategoryDraft> = categories
        .into_iter()
        .filter_map(|category| {
            let members = members.remove(category.id.as_str()).unwrap_or_default();
            let character = category
                .resolved_gamebanana_character(
                    members
                        .iter()
                        .filter_map(|entry| entry.source.as_ref()?.snapshot.as_ref()),
                    super_category,
                )
                .map(|link| link.id);
            let mods = shown_mods(members);
            if mods.is_empty() && !prefs.library_show_empty_category_folders {
                return None;
            }
            Some(OverlayCategoryDraft {
                id: category.id,
                name: category.name,
                character,
                mods,
            })
        })
        .collect();
    let loose = shown_mods(loose);
    if !loose.is_empty() {
        drafts.push(OverlayCategoryDraft {
            id: overlay_protocol::UNCATEGORIZED_ID.to_owned(),
            name: uncategorized.to_owned(),
            character: None,
            mods: loose,
        });
    }
    OverlayLibraryDraft {
        game_id: game_id.to_owned(),
        game_name: game.definition.name.clone(),
        categories: drafts,
    }
}

fn overlay_mod_draft(entry: &ModEntry, censored: bool) -> OverlayModDraft {
    let user = &entry.metadata.user;
    OverlayModDraft {
        id: entry.id.clone(),
        name: user
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&entry.folder_name)
            .to_owned(),
        active: entry.status == ModStatus::Active,
        pictures: OverlayModPictures {
            root: entry.root_path.clone(),
            cover: user.cover_image.clone(),
            screenshots: user.screenshots.clone(),
            gamebanana_preview: entry
                .source
                .as_ref()
                .and_then(|source| source.snapshot.as_ref())
                .and_then(|snapshot| snapshot.preview_urls.first().cloned()),
            thumbnails: [user.card_thumb_generated_at, user.rail_thumb_generated_at],
        },
        censored,
    }
}

/// Hestia's window, which the overlay's restore button brings back.
#[cfg(windows)]
fn game_overlay_host_window(frame: &eframe::Frame) -> Option<i64> {
    match frame.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as i64),
        _ => None,
    }
}

#[cfg(not(windows))]
fn game_overlay_host_window(_frame: &eframe::Frame) -> Option<i64> {
    None
}

#[cfg(test)]
mod game_overlay_library_tests {
    use super::*;

    fn game(id: &str) -> GameInstall {
        GameInstall {
            definition: crate::model::GameDefinition {
                id: id.to_owned(),
                name: format!("Game {id}"),
                backend: GameBackend::Xxmi,
                xxmi_code: String::new(),
            },
            mods_path_override: None,
            modded_exe_path_override: None,
            vanilla_exe_path_override: None,
            apply_mod_changes_in_game: false,
            enabled: true,
        }
    }

    fn category(id: &str, name: &str, order: i32) -> ModCategory {
        ModCategory {
            id: id.to_owned(),
            game_id: "zzz".to_owned(),
            name: name.to_owned(),
            order,
            gamebanana_character: None,
        }
    }

    fn mod_entry(id: &str, title: &str, category_id: Option<&str>, status: ModStatus) -> ModEntry {
        let mut metadata = crate::model::ModMetadata::default();
        metadata.user.title = Some(title.to_owned());
        metadata.user.category_id = category_id.map(str::to_owned);
        ModEntry {
            id: id.to_owned(),
            game_id: "zzz".to_owned(),
            folder_name: id.to_owned(),
            root_path: PathBuf::from(format!(r"D:\Mods\{id}")),
            status,
            metadata,
            discovered_tools: Vec::new(),
            archive_original_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            content_mtime: None,
            ini_hash: None,
            content_size_bytes: 0,
            unsafe_content: false,
            unsafe_content_auto: false,
            unsafe_content_preference: Default::default(),
            source: None,
            update_state: ModUpdateState::Unlinked,
        }
    }

    fn state() -> AppState {
        let mut state = AppState::default();
        state.categories = vec![
            category("c-ellen", "Ellen", 2),
            category("c-anby", "Anby", 1),
            category("c-empty", "Empty", 0),
        ];
        state.mods = vec![
            mod_entry("m1", "Zed outfit", Some("c-ellen"), ModStatus::Active),
            mod_entry("m2", "Alpha outfit", Some("c-ellen"), ModStatus::Disabled),
            mod_entry("m3", "Stored", Some("c-ellen"), ModStatus::Archived),
            mod_entry("m4", "Anby hair", Some("c-anby"), ModStatus::Disabled),
            mod_entry("m5", "Loose", None, ModStatus::Active),
            mod_entry("m6", "Missing category", Some("gone"), ModStatus::Disabled),
        ];
        // Another game's mods stay out.
        let mut other = mod_entry("m7", "Other game", None, ModStatus::Active);
        other.game_id = "genshin".to_owned();
        state.mods.push(other);
        state
    }

    fn layout(library: &OverlayLibraryDraft) -> Vec<(String, Vec<String>)> {
        library
            .categories
            .iter()
            .map(|category| {
                (
                    category.name.clone(),
                    category
                        .mods
                        .iter()
                        .map(|entry| entry.name.clone())
                        .collect(),
                )
            })
            .collect()
    }

    fn names(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        pairs
            .iter()
            .map(|(category, mods)| {
                (
                    (*category).to_owned(),
                    mods.iter().map(|name| (*name).to_owned()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn categories_keep_the_library_order_with_uncategorized_last() {
        let mut state = state();
        let library = game_overlay_library(&state, &game("zzz"), "Uncategorized");
        assert_eq!(library.game_name, "Game zzz");
        assert_eq!(
            layout(&library),
            names(&[
                ("Empty", &[]),
                ("Anby", &["Anby hair"]),
                ("Ellen", &["Alpha outfit", "Zed outfit"]),
                ("Uncategorized", &["Loose", "Missing category"]),
            ])
        );
        let uncategorized = library.categories.last().unwrap();
        assert_eq!(uncategorized.id, overlay_protocol::UNCATEGORIZED_ID);
        assert!(library.categories[2].mods[1].active);
        assert!(!library.categories[2].mods[0].active);

        state
            .category_sort_mode_by_game
            .insert("zzz".to_owned(), ModCategorySortMode::ByModCountDesc);
        state.static_prefs.library_sort = LibrarySort::NameDesc;
        state.static_prefs.library_show_empty_category_folders = false;
        let library = game_overlay_library(&state, &game("zzz"), "Uncategorized");
        assert_eq!(
            layout(&library),
            names(&[
                ("Ellen", &["Zed outfit", "Alpha outfit"]),
                ("Anby", &["Anby hair"]),
                ("Uncategorized", &["Missing category", "Loose"]),
            ])
        );
    }

    #[test]
    fn mods_saved_with_a_category_name_join_that_category() {
        let mut state = state();
        let mut legacy = mod_entry("m8", "Legacy", None, ModStatus::Disabled);
        legacy.metadata.user.category = " anby ".to_owned();
        state.mods.push(legacy);
        let library = game_overlay_library(&state, &game("zzz"), "Uncategorized");
        assert_eq!(
            layout(&library)[1],
            (
                "Anby".to_owned(),
                vec!["Anby hair".to_owned(), "Legacy".to_owned()]
            )
        );
    }

    #[test]
    fn unsafe_mods_follow_the_library_setting() {
        let mut state = state();
        state.mods[0].unsafe_content = true;
        state.static_prefs.unsafe_content_mode = UnsafeContentMode::HideShowCounter;
        let library = game_overlay_library(&state, &game("zzz"), "Uncategorized");
        assert_eq!(layout(&library)[2].1, ["Alpha outfit"]);

        state.static_prefs.unsafe_content_mode = UnsafeContentMode::Censor;
        let library = game_overlay_library(&state, &game("zzz"), "Uncategorized");
        let ellen = &library.categories[2].mods;
        assert_eq!(ellen[1].name, "Zed outfit");
        assert!(ellen[1].censored);
        assert!(!ellen[0].censored);

        state.static_prefs.unsafe_content_mode = UnsafeContentMode::Show;
        let library = game_overlay_library(&state, &game("zzz"), "Uncategorized");
        assert!(!library.categories[2].mods[1].censored);
    }

    #[test]
    fn a_category_linked_to_a_character_carries_it() {
        let mut state = state();
        state.categories[0].gamebanana_character = Some(crate::model::GameBananaCategoryLink {
            id: 42,
            name: "Ellen".to_owned(),
        });
        let library = game_overlay_library(&state, &game("zzz"), "Uncategorized");
        assert_eq!(library.categories[2].character, Some(42));
        assert_eq!(library.categories[1].character, None);
    }
}
