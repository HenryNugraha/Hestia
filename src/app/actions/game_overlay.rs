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
    /// The settings the overlay has.
    sent_settings: Option<overlay_protocol::Settings>,
    next_library_check: Option<Instant>,
    /// Where the overlay was in each game's library, by game, for this
    /// session.
    selections: HashMap<String, overlay_protocol::Selection>,
    /// How many times in a row the overlay closed unexpectedly or couldn't
    /// start while its game ran.
    failures: usize,
    /// When to start it again after that.
    restart_at: Option<Instant>,
    /// The GameBanana installs it asked for that aren't done.
    installs: Vec<OverlayInstall>,
    /// The game the preview in Settings shows the overlay for, while no
    /// game runs.  Hestia's own window stands in for the game.
    preview: Option<String>,
}

impl HestiaApp {
    /// Starts the in-game overlay when a supported game starts, keeps its
    /// library current, and closes it when the game exits.
    fn poll_game_overlay(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        // With the overlay turned off in Settings, no game counts, so a
        // running overlay closes.
        let watched: Vec<WatchedGame> = if self.state.static_prefs.game_overlay {
            self.state
                .games
                .iter()
                .filter_map(WatchedGame::for_game)
                .collect()
        } else {
            Vec::new()
        };
        if watched != self.game_overlay.watched {
            if let Some(watcher) = &self.game_overlay.watcher {
                watcher.watch(watched.clone());
            }
            self.game_overlay.watched = watched;
        }
        // The preview ends when its game has no overlay any more.
        if let Some(id) = &self.game_overlay.preview
            && !self.game_overlay.watched.iter().any(|game| &game.id == id)
        {
            self.game_overlay.preview = None;
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
        self.sync_game_overlay_installs(ctx);

        // The overlay's game while it runs, or else the first one that runs,
        // or else the preview's.
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
            .map(|running| (*running).clone())
            // The overlay shows over Hestia then, and works as in the game.
            .or_else(|| {
                overlay.preview.clone().map(|id| RunningGame {
                    id,
                    pids: vec![std::process::id()],
                })
            });
        let settings = self.game_overlay_settings();

        match (&mut self.game_overlay.process, game) {
            (Some(process), Some(game)) if process.game_id == game.id => {
                if process.game_pids != game.pids {
                    process.set_game_pids(game.pids);
                }
                if self.game_overlay.sent_settings != Some(settings) {
                    process.send_settings(settings);
                    self.game_overlay.sent_settings = Some(settings);
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
        let game_id = process.game_id.clone();
        let mut changes = Vec::new();
        let mut browse = Vec::new();
        let mut installs = Vec::new();
        let mut opacity_changed = None;
        let mut all_characters_changed = None;
        let mut exited = false;
        while let Ok(event) = process.events.try_recv() {
            match event {
                OverlayEvent::Message(overlay_protocol::FromOverlay::Selection(selection)) => {
                    self.game_overlay
                        .selections
                        .insert(game_id.clone(), selection);
                }
                OverlayEvent::Message(overlay_protocol::FromOverlay::Change(change)) => {
                    changes.push(change);
                }
                OverlayEvent::Message(overlay_protocol::FromOverlay::Typing { typing }) => {
                    xxmi_persist::set_game_overlay_typing(typing);
                }
                OverlayEvent::Message(overlay_protocol::FromOverlay::Opacity { opacity }) => {
                    opacity_changed = Some(opacity);
                }
                OverlayEvent::Message(overlay_protocol::FromOverlay::AllCharacters { show }) => {
                    all_characters_changed = Some(show);
                }
                OverlayEvent::Message(overlay_protocol::FromOverlay::Browse(request)) => {
                    browse.push(Some(request));
                }
                OverlayEvent::Message(overlay_protocol::FromOverlay::ListCharacters) => {
                    browse.push(None);
                }
                OverlayEvent::Message(
                    message @ (overlay_protocol::FromOverlay::Install(_)
                    | overlay_protocol::FromOverlay::PickFile { .. }
                    | overlay_protocol::FromOverlay::SameName { .. }
                    | overlay_protocol::FromOverlay::CancelInstall { .. }),
                ) => installs.push(message),
                OverlayEvent::Exited => exited = true,
            }
        }
        if !exited && let Some(overlay_browse) = self.game_overlay_browse() {
            for request in browse {
                match request {
                    Some(request) => overlay_browse.clone().fetch_page(request),
                    None => overlay_browse.clone().list_characters(),
                }
            }
        }
        let prefs = &mut self.state.static_prefs;
        let opacity_changed =
            opacity_changed.filter(|&opacity| prefs.game_overlay_opacity != Some(opacity));
        let all_characters_changed =
            all_characters_changed.filter(|&show| prefs.game_overlay_all_characters != show);
        if let Some(opacity) = opacity_changed {
            prefs.game_overlay_opacity = Some(opacity);
        }
        if let Some(show) = all_characters_changed {
            prefs.game_overlay_all_characters = show;
        }
        if opacity_changed.is_some() || all_characters_changed.is_some() {
            // The overlay has them already.
            self.game_overlay.sent_settings = Some(self.game_overlay_settings());
            self.save_state();
        }
        if !exited {
            for message in installs {
                self.handle_game_overlay_install(&game_id, message);
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
            return;
        }
        for change in changes {
            let (error, press_key) = match self.apply_game_overlay_change(&game_id, &change) {
                Ok(press_key) => (None, press_key),
                Err(error) => (Some(error), None),
            };
            let Some(game) = self
                .state
                .games
                .iter()
                .find(|game| game.definition.id == game_id)
            else {
                return;
            };
            let library = game_overlay_library(&self.state, game, self.text().uncategorized());
            let library = (self.game_overlay.sent.as_ref() != Some(&library)).then_some(library);
            let Some(process) = &self.game_overlay.process else {
                return;
            };
            process.answer(
                library.clone(),
                overlay_protocol::Changed {
                    id: change.id,
                    error,
                    press_key,
                },
            );
            if library.is_some() {
                self.game_overlay.sent = library;
            }
        }
    }

    /// The settings the overlay takes from Hestia.
    fn game_overlay_settings(&self) -> overlay_protocol::Settings {
        let prefs = &self.state.static_prefs;
        overlay_protocol::Settings {
            language: prefs.language,
            opacity: prefs.game_overlay_opacity,
            hotkey: prefs.game_overlay_hotkey,
            arrival_strip: prefs.game_overlay_arrival_strip,
            close_strip: prefs.game_overlay_close_strip,
            gamebanana: prefs.game_overlay_gamebanana,
            all_characters: prefs.game_overlay_all_characters,
            key_hints: prefs.game_overlay_key_hints,
            zoom: prefs.interface_zoom,
        }
    }

    /// What answering the overlay's GameBanana requests needs, while it runs.
    fn game_overlay_browse(&self) -> Option<OverlayBrowse> {
        let process = self.game_overlay.process.as_ref()?;
        let prefs = &self.state.static_prefs;
        Some(OverlayBrowse {
            runtime: self.runtime_services.clone(),
            portable: self.portable.clone(),
            game_id: process.game_id.clone(),
            outbox: process.outbox(),
            browse_sort: prefs.browse_sort,
            unsafe_content_mode: prefs.unsafe_content_mode,
            cache_limit_bytes: self.cache_limit_bytes.load(Ordering::Relaxed),
        })
    }

    /// Makes a change the in-game overlay asked for with the steps of the
    /// library's Enable and Disable, as one change with one reload.  Gives the
    /// game's reload key when the reload settings leave pressing it to the
    /// user.  The error is for the overlay's warning strip.
    fn apply_game_overlay_change(
        &mut self,
        game_id: &str,
        change: &overlay_protocol::Change,
    ) -> Result<Option<String>, String> {
        use overlay_protocol::ChangeAction;

        let text = self.text();
        let Some(game) = self
            .state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .cloned()
        else {
            return Ok(None);
        };
        // A mod that's gone leaves the overlay with the library sent next.
        let Some(target) = self.state.mods.iter().find(|entry| {
            entry.id == change.mod_id
                && entry.game_id == game_id
                && entry.status != ModStatus::Archived
        }) else {
            return Ok(None);
        };
        let mut turn_off = Vec::new();
        let mut turn_on = Vec::new();
        match change.action {
            ChangeAction::Use | ChangeAction::TurnOn if target.status == ModStatus::Disabled => {
                turn_on.push(target.id.clone());
            }
            ChangeAction::TurnOff if target.status == ModStatus::Active => {
                turn_off.push(target.id.clone());
            }
            _ => {}
        }
        // Uncategorized mods have nothing to do with each other, so using one
        // leaves the rest on.  Unsafe mods the overlay hides count too.
        if change.action == ChangeAction::Use
            && let Some(category_id) = effective_category_id(&self.state.categories, target)
        {
            turn_off.extend(
                self.state
                    .mods
                    .iter()
                    .filter(|entry| {
                        entry.game_id == game_id
                            && entry.id != change.mod_id
                            && entry.status == ModStatus::Active
                            && effective_category_id(&self.state.categories, entry)
                                == Some(category_id)
                    })
                    .map(|entry| entry.id.clone()),
            );
        }
        if turn_off.is_empty() && turn_on.is_empty() {
            return Ok(None);
        }
        let locked = turn_off.iter().any(|mod_id| {
            self.mod_action_lock_reason_by_id(mod_id, ModMutationKind::DisableActive)
                .is_some()
        }) || turn_on.iter().any(|mod_id| {
            self.mod_action_lock_reason_by_id(mod_id, ModMutationKind::EnableIntoActive)
                .is_some()
        });
        if locked {
            self.report_locked_mods(None);
            return Err(text.mods_locked_probably_by_game().to_owned());
        }

        let use_default = self.state.static_prefs.use_default_mods_path;
        let mut ptx = self.begin_xxmi_persist_tx(&game);
        // (name, turned on)
        let mut done: Vec<(String, bool)> = Vec::new();
        let mut failure = None;
        // Off first.  If one of the others won't turn off, the used mod stays
        // off too, so two never run together.
        for (mod_ids, on) in [(&turn_off, false), (&turn_on, true)] {
            if on && failure.is_some() {
                break;
            }
            for mod_id in mod_ids {
                let Some(entry) = self.state.mods.iter_mut().find(|entry| &entry.id == mod_id)
                else {
                    continue;
                };
                let result = match (game.definition.backend, on) {
                    (GameBackend::Xxmi, false) => Self::persisted_xxmi_disable(&mut ptx, entry),
                    (GameBackend::Xxmi, true) => Self::persisted_xxmi_enable(&mut ptx, entry),
                    (GameBackend::UnrealEngine, false) => {
                        unrealengine::disable_mod(entry, &game, use_default)
                    }
                    (GameBackend::UnrealEngine, true) => {
                        unrealengine::enable_mod(entry, &game, use_default)
                    }
                };
                match result {
                    Ok(()) => done.push((entry.folder_name.clone(), on)),
                    Err(error) => {
                        failure.get_or_insert((error, on));
                    }
                }
            }
        }
        let request_reload = [
            (ReloadHotkeyTrigger::EnablingMods, true),
            (ReloadHotkeyTrigger::DisablingMods, false),
        ]
        .into_iter()
        .find(|(trigger, on)| {
            done.iter().any(|(_, done_on)| done_on == on)
                && self.xxmi_reload_enabled_for_game(&game, *trigger)
        })
        .map(|(trigger, _)| trigger);
        // With auto-reload off for this, the overlay tells the user which key
        // shows the change.
        let press_key =
            (game.is_xxmi() && !done.is_empty() && request_reload.is_none()).then(|| {
                xxmi_persist::importer_root_for(&game, use_default)
                    .map(|root| xxmi_persist::reload_hotkey_name(&root))
                    .unwrap_or_else(|| "F10".to_owned())
            });
        self.finish_xxmi_persist_tx(&game, ptx, request_reload);

        for (name, on) in &done {
            let action = if *on {
                text.action_enabled()
            } else {
                text.action_disabled()
            };
            self.log_action(action, name);
        }
        if let Some((name, on)) = done.last() {
            let action = if *on {
                text.action_enabled()
            } else {
                text.action_disabled()
            };
            self.set_message_ok(text.action_message(action, name));
            self.save_state();
            self.refresh();
        }
        match failure {
            Some((error, on)) => {
                let fallback = if on {
                    text.enable_failed()
                } else {
                    text.disable_failed()
                };
                let toast = self.mod_action_error_toast(&error, fallback);
                self.report_error(error, Some(toast));
                Err(toast.to_owned())
            }
            None => Ok(press_key),
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
        let start_settings = self.game_overlay_settings();
        let start = OverlayStartDraft {
            host_window: game_overlay_host_window(frame),
            game_pids: running.pids.clone(),
            library: library.clone(),
            settings: start_settings,
            selection: self.game_overlay.selections.get(&running.id).cloned(),
        };
        match OverlayProcess::spawn(running, start) {
            Ok(process) => {
                tracing::info!(game = %running.id, "Started the in-game overlay");
                self.game_overlay.process = Some(process);
                self.game_overlay.sent = Some(library);
                self.game_overlay.sent_settings = Some(start_settings);
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
        gamebanana_id: entry
            .source
            .as_ref()
            .and_then(|source| source.gamebanana.as_ref())
            .map(|link| link.mod_id),
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

    /// Saves the installed Hestia's library as the overlay preview's sample,
    /// for `--overlay-preview` and its screenshots.  The game is
    /// HESTIA_OVERLAY_PREVIEW_GAME, Endfield by default.
    #[test]
    #[ignore = "reads the real library and writes target/overlay-preview/sample-library.json"]
    fn save_overlay_preview_sample() {
        let state_path = std::env::var_os("HESTIA_OVERLAY_PREVIEW_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA"))
                    .join("Hestia/hestia.toml")
            });
        let mut state = crate::persistence::load_app_state(&crate::persistence::PortablePaths {
            history_db: state_path.with_file_name("history.db"),
            state_archive: state_path,
            state_source: None,
        })
        .expect("load the library");
        let game_id =
            std::env::var("HESTIA_OVERLAY_PREVIEW_GAME").unwrap_or_else(|_| "endfield".to_owned());
        let game = state
            .games
            .iter()
            .find(|game| game.definition.id == game_id)
            .expect("the game is in the library")
            .clone();
        // Only reads the mods, unlike Hestia's scan, which also writes their
        // metadata.  The overlay leaves archived mods out anyway.
        state.mods = crate::integrations::xxmi::scan_live_mods(
            &game,
            state.static_prefs.use_default_mods_path,
            false,
        )
        .expect("read the mods");
        let game = &game;
        let text = TextCatalog::new(state.static_prefs.language);
        let start = overlay_protocol::Start {
            host_pid: 0,
            host_window: None,
            game_pids: Vec::new(),
            settings: overlay_protocol::Settings {
                language: state.static_prefs.language,
                opacity: state.static_prefs.game_overlay_opacity,
                ..Default::default()
            },
            library: game_overlay_library(&state, game, text.uncategorized()).resolve(),
            selection: None,
        };
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/overlay-preview/sample-library.json");
        std::fs::write(&path, serde_json::to_string_pretty(&start).unwrap()).unwrap();
        println!("Saved {}", path.display());
    }
}
