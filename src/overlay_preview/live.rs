//! The in-game overlay's link to Hestia, over its standard input and output.
//! `crate::overlay_protocol` has the messages.

use std::{
    collections::HashSet,
    io::{BufRead, Write},
    sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc},
    thread,
    time::Duration,
};

use anyhow::{Context, bail};

use super::{layouts::ModRequest, platform};
use crate::overlay_protocol::{
    self, Change, Changed, FromOverlay, Library, Selection, Settings, Start, ToOverlay,
};

/// How long the strip shows after the overlay closes.
const STRIP_SECONDS: f64 = 3.0;

/// How long the strip shows when the game first comes to the front, long
/// enough to notice Alt+H.
const ARRIVAL_SECONDS: f64 = 7.0;

/// How long the strip shows a change Hestia refused.
pub(super) const WARNING_SECONDS: f64 = 6.0;

/// How long the strip stays after the pointer leaves it.
const HOVER_SECONDS: f64 = 1.5;

/// The end of the strip's time, where it fades out.
const FADE_SECONDS: f64 = 0.4;

/// How long a change waits for Hestia before its card shows that it waits.
/// Most answers come sooner, so their cards never flicker.
const WAITING_LOOK_SECONDS: f64 = 0.3;

/// How long the overlay waits for Hestia to answer a change.
const ANSWER_SECONDS: f64 = 10.0;

/// How long the overlay waits for its window to close once Hestia is gone.
const EXIT_GRACE: Duration = Duration::from_secs(3);

/// Reads the start message, which Hestia sends first.
pub(super) fn read_start() -> anyhow::Result<Start> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .context("could not read from Hestia")?;
    match overlay_protocol::decode::<ToOverlay>(&line)
        .context("could not read Hestia's start message")?
    {
        ToOverlay::Start(start) => Ok(start),
        _ => bail!("Hestia sent another message before the start message"),
    }
}

#[derive(Default)]
struct Inbox {
    news: News,
    closed: bool,
}

/// What Hestia sent since the overlay last looked.
#[derive(Default)]
pub(super) struct News {
    /// The newest settings.
    pub settings: Option<Settings>,
    /// The newest library.
    pub library: Option<Library>,
    /// Answers to changes, each after the library that has its change.
    pub answers: Vec<Changed>,
    /// GameBanana pages, characters and pictures, in the order they came.
    pub gamebanana: Vec<ToOverlay>,
}

pub(super) struct Link {
    inbox: Arc<Mutex<Inbox>>,
    outbox: mpsc::Sender<FromOverlay>,
    host_window: Option<i64>,
    reported: Option<Selection>,
    typing: bool,
    /// The opacity Hestia last knew.
    opacity: u8,
    closing: bool,
}

impl Link {
    /// Starts reading Hestia's messages and writing the overlay's.
    pub(super) fn start(
        ctx: egui::Context,
        host_window: Option<i64>,
        opacity: u8,
    ) -> std::io::Result<Self> {
        let inbox = Arc::new(Mutex::new(Inbox::default()));
        let reader_inbox = Arc::clone(&inbox);
        thread::Builder::new()
            .name("hestia-overlay-link-in".to_owned())
            .spawn(move || read_messages(&reader_inbox, &ctx))?;
        let (outbox, outgoing) = mpsc::channel();
        thread::Builder::new()
            .name("hestia-overlay-link-out".to_owned())
            .spawn(move || write_messages(&outgoing))?;
        Ok(Self {
            inbox,
            outbox,
            host_window,
            reported: None,
            typing: false,
            opacity,
            closing: false,
        })
    }

    pub(super) fn host_window(&self) -> Option<i64> {
        self.host_window
    }

    /// What Hestia sent since the last call.  Taken together, so an answer
    /// never comes without the library that has its change.
    pub(super) fn take_news(&self) -> News {
        std::mem::take(&mut lock(&self.inbox).news)
    }

    /// Asks Hestia for a change.
    pub(super) fn send_change(&self, change: Change) {
        let _ = self.outbox.send(FromOverlay::Change(change));
    }

    /// Asks Hestia for GameBanana mods or characters.
    pub(super) fn ask(&self, request: FromOverlay) {
        let _ = self.outbox.send(request);
    }

    /// Whether Hestia has closed the link.  True once.
    pub(super) fn take_closed(&mut self) -> bool {
        if self.closing || !lock(&self.inbox).closed {
            return false;
        }
        self.closing = true;
        true
    }

    /// Whether the overlay is closing because Hestia closed the link.
    pub(super) fn closing(&self) -> bool {
        self.closing
    }

    /// Tells Hestia where the overlay is in the library, when that changed.
    pub(super) fn report(&mut self, selection: Selection) {
        if self.reported.as_ref() != Some(&selection) {
            let _ = self.outbox.send(FromOverlay::Selection(selection.clone()));
            self.reported = Some(selection);
        }
    }

    /// Tells Hestia whether the search takes the keys, when that changed.
    pub(super) fn report_typing(&mut self, typing: bool) {
        if self.typing != typing {
            let _ = self.outbox.send(FromOverlay::Typing { typing });
            self.typing = typing;
        }
    }

    /// Tells Hestia the opacity to save, when that changed.
    pub(super) fn report_opacity(&mut self, opacity: u8) {
        if self.opacity != opacity {
            let _ = self.outbox.send(FromOverlay::Opacity { opacity });
            self.opacity = opacity;
        }
    }

    /// Notes an opacity that came from Hestia, so it isn't sent back.
    pub(super) fn received_opacity(&mut self, opacity: u8) {
        self.opacity = opacity;
    }

    /// Tells Hestia to save the header's "show all characters" button.
    pub(super) fn report_all_characters(&self, show: bool) {
        let _ = self.outbox.send(FromOverlay::AllCharacters { show });
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read_messages(inbox: &Mutex<Inbox>, ctx: &egui::Context) {
    for line in std::io::stdin().lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                tracing::warn!(%error, "Could not read a message from Hestia");
                continue;
            }
            Err(_) => break,
        };
        match overlay_protocol::decode::<ToOverlay>(&line) {
            Ok(ToOverlay::Settings(settings)) => {
                lock(inbox).news.settings = Some(settings);
                ctx.request_repaint();
            }
            Ok(ToOverlay::Library(library)) => {
                lock(inbox).news.library = Some(library);
                ctx.request_repaint();
            }
            Ok(ToOverlay::Changed(answer)) => {
                lock(inbox).news.answers.push(answer);
                ctx.request_repaint();
            }
            Ok(
                message @ (ToOverlay::BrowsePage(_)
                | ToOverlay::Characters { .. }
                | ToOverlay::Pictures(_)
                | ToOverlay::Install(_)),
            ) => {
                lock(inbox).news.gamebanana.push(message);
                ctx.request_repaint();
            }
            Ok(ToOverlay::GameProcesses { pids }) => platform::set_game_processes(&pids),
            Ok(ToOverlay::Start(_)) => tracing::warn!("Hestia sent a second start message"),
            Err(error) => tracing::warn!(%error, "Could not read a message from Hestia"),
        }
    }
    tracing::info!("Hestia closed the overlay's link");
    lock(inbox).closed = true;
    ctx.request_repaint();
    // The window closes on its next frame.  Leave anyway if it can't.
    thread::sleep(EXIT_GRACE);
    tracing::warn!("The overlay window did not close, exiting");
    std::process::exit(0);
}

fn write_messages(outgoing: &mpsc::Receiver<FromOverlay>) {
    let mut stdout = std::io::stdout().lock();
    for message in outgoing {
        let written = overlay_protocol::encode(&message)
            .map_err(std::io::Error::from)
            .and_then(|line| writeln!(stdout, "{line}"))
            .and_then(|()| stdout.flush());
        if let Err(error) = written {
            tracing::warn!(%error, "Could not write to Hestia");
            break;
        }
    }
}

/// When the in-game overlay's strip shows.  It shows for a few seconds when
/// the game first comes to the front and after the overlay closes, then fades
/// out, and the window hides until Alt+H.
#[derive(Debug, Default)]
pub(super) struct Strip {
    /// Until when the strip shows, in egui time.
    until: f64,
    /// The strip's X hid it, so the pointer still resting there doesn't keep
    /// it.
    dismissed: bool,
}

impl Strip {
    /// Shows the strip for its full time from now.
    pub(super) fn show(&mut self, now: f64) {
        self.show_for(now, STRIP_SECONDS);
    }

    /// Shows the strip for the game's first time in front.
    pub(super) fn arrive(&mut self, now: f64) {
        self.show_for(now, ARRIVAL_SECONDS);
    }

    fn show_for(&mut self, now: f64, seconds: f64) {
        self.until = self.until.max(now + seconds);
        self.dismissed = false;
    }

    /// Shows the strip long enough to read a warning.
    pub(super) fn warn(&mut self, now: f64) {
        self.show_for(now, WARNING_SECONDS);
    }

    /// Keeps the strip a little longer while the pointer is on it.
    pub(super) fn hover(&mut self, now: f64) {
        if !self.dismissed {
            self.until = self.until.max(now + HOVER_SECONDS);
        }
    }

    /// Hides the strip now.
    pub(super) fn dismiss(&mut self) {
        self.until = f64::NEG_INFINITY;
        self.dismissed = true;
    }

    pub(super) fn shown(&self, now: f64) -> bool {
        self.until > now
    }

    /// 1 while the strip shows, falling to 0 as it fades out.
    pub(super) fn opacity(&self, now: f64) -> f32 {
        ((self.until - now) / FADE_SECONDS).clamp(0.0, 1.0) as f32
    }

    /// When the next frame is needed: now while fading, at the start of the
    /// fade before that, and never once the strip is gone.
    pub(super) fn next_frame(&self, now: f64) -> Option<Duration> {
        let fade_start = self.until - FADE_SECONDS;
        if !self.shown(now) {
            None
        } else if now >= fade_start {
            Some(Duration::ZERO)
        } else {
            Some(Duration::from_secs_f64(fade_start - now))
        }
    }
}

/// The changes to mods the overlay asked Hestia for.  A change shows at once
/// and stays while Hestia works on it, so a library Hestia sent before making
/// it doesn't undo it on screen.  Once Hestia answers, its library shows.
pub(super) struct Changes {
    /// The newest library from Hestia.
    library: Library,
    /// What the overlay shows: the library with the changes still waiting.
    shown: Library,
    asked: Vec<Asked>,
    next_id: u64,
}

struct Asked {
    id: u64,
    /// When the overlay asked, in egui time.
    at: f64,
    mod_id: String,
    /// The mods the change turned on or off on screen.
    states: Vec<(String, bool)>,
}

impl Changes {
    pub(super) fn new(library: Library) -> Self {
        Self {
            shown: library.clone(),
            library,
            asked: Vec::new(),
            next_id: 0,
        }
    }

    /// Asks Hestia for a change the overlay already shows.
    pub(super) fn ask(&mut self, request: ModRequest, now: f64) -> Change {
        self.next_id += 1;
        self.asked.push(Asked {
            id: self.next_id,
            at: now,
            mod_id: request.mod_id.clone(),
            states: request.states,
        });
        self.shown = self.with_asked();
        Change {
            id: self.next_id,
            mod_id: request.mod_id,
            action: request.action,
        }
    }

    pub(super) fn receive(&mut self, library: Library) {
        self.library = library;
    }

    /// Takes Hestia's answer.  The error says why Hestia refused the change.
    pub(super) fn answer(&mut self, answer: Changed) -> Option<String> {
        let index = self.asked.iter().position(|asked| asked.id == answer.id)?;
        self.asked.remove(index);
        answer.error
    }

    /// Gives up on changes Hestia hasn't answered in time.  True if any.
    pub(super) fn expire(&mut self, now: f64) -> bool {
        let before = self.asked.len();
        self.asked.retain(|asked| now - asked.at < ANSWER_SECONDS);
        self.asked.len() != before
    }

    /// The library to show, when it's not the one showing.
    pub(super) fn take_update(&mut self) -> Option<Library> {
        let shown = self.with_asked();
        (shown != self.shown).then(|| {
            self.shown = shown.clone();
            shown
        })
    }

    /// The mods whose change has waited long enough to show that it waits.
    pub(super) fn waiting(&self, now: f64) -> HashSet<String> {
        self.asked
            .iter()
            .filter(|asked| now - asked.at >= WAITING_LOOK_SECONDS)
            .map(|asked| asked.mod_id.clone())
            .collect()
    }

    /// When a card starts to look like it waits, or a change runs out of
    /// time.
    pub(super) fn next_frame(&self, now: f64) -> Option<Duration> {
        self.asked
            .iter()
            .flat_map(|asked| [asked.at + WAITING_LOOK_SECONDS, asked.at + ANSWER_SECONDS])
            .filter(|at| *at > now)
            .min_by(f64::total_cmp)
            .map(|at| Duration::from_secs_f64(at - now))
    }

    fn with_asked(&self) -> Library {
        let mut library = self.library.clone();
        for (mod_id, active) in self.asked.iter().flat_map(|asked| &asked.states) {
            for entry in library
                .categories
                .iter_mut()
                .flat_map(|category| &mut category.mods)
                .filter(|entry| &entry.id == mod_id)
            {
                entry.active = *active;
            }
        }
        library
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_protocol::{Category, ChangeAction, Mod};

    fn library(active: &[&str]) -> Library {
        Library {
            game_id: "zzz".into(),
            game_name: "ZZZ".into(),
            categories: vec![Category {
                id: "ellen".into(),
                name: "Ellen".into(),
                image: None,
                character: None,
                mods: ["a", "b", "c"]
                    .into_iter()
                    .map(|id| Mod {
                        id: id.into(),
                        name: id.into(),
                        image: None,
                        active: active.contains(&id),
                        censored: false,
                        gamebanana_id: None,
                    })
                    .collect(),
            }],
        }
    }

    fn active(library: &Library) -> Vec<&str> {
        library.categories[0]
            .mods
            .iter()
            .filter(|entry| entry.active)
            .map(|entry| entry.id.as_str())
            .collect()
    }

    fn use_b() -> ModRequest {
        ModRequest {
            mod_id: "b".into(),
            action: ChangeAction::Use,
            states: vec![("a".into(), false), ("b".into(), true)],
        }
    }

    #[test]
    fn a_change_shows_until_hestia_answers() {
        let mut changes = Changes::new(library(&["a"]));
        let change = changes.ask(use_b(), 1.0);
        assert_eq!(change.mod_id, "b");
        assert_eq!(change.action, ChangeAction::Use);
        // The overlay already shows the change, so nothing to update.
        assert_eq!(changes.take_update(), None);
        assert!(changes.waiting(1.1).is_empty());
        assert_eq!(changes.waiting(1.4), HashSet::from(["b".to_owned()]));

        // A library Hestia sent before making the change doesn't undo it.
        changes.receive(library(&["a", "c"]));
        assert_eq!(active(&changes.take_update().unwrap()), ["b", "c"]);

        changes.receive(library(&["b", "c"]));
        assert_eq!(
            changes.answer(Changed {
                id: change.id,
                error: None,
                press_key: None,
            }),
            None
        );
        assert_eq!(changes.take_update(), None);
        assert!(changes.waiting(2.0).is_empty());
        assert_eq!(changes.next_frame(2.0), None);
    }

    #[test]
    fn a_refused_change_goes_back_to_hestias_library() {
        let mut changes = Changes::new(library(&["a"]));
        let change = changes.ask(use_b(), 1.0);
        let error = changes.answer(Changed {
            id: change.id,
            error: Some("Mods are locked".into()),
            press_key: None,
        });
        assert_eq!(error.as_deref(), Some("Mods are locked"));
        assert_eq!(active(&changes.take_update().unwrap()), ["a"]);
        // An answer to nothing the overlay asked is ignored.
        assert_eq!(
            changes.answer(Changed {
                id: 99,
                error: Some("late".into()),
                press_key: None,
            }),
            None
        );
    }

    #[test]
    fn a_change_hestia_never_answers_runs_out() {
        let mut changes = Changes::new(library(&["a"]));
        changes.ask(use_b(), 1.0);
        assert_eq!(
            changes.next_frame(1.0),
            Some(Duration::from_secs_f64(WAITING_LOOK_SECONDS))
        );
        assert!(!changes.expire(1.0 + ANSWER_SECONDS - 0.1));
        assert!(changes.expire(1.0 + ANSWER_SECONDS));
        assert_eq!(active(&changes.take_update().unwrap()), ["a"]);
    }

    #[test]
    fn the_strip_shows_for_its_time_then_fades_out() {
        let mut strip = Strip::default();
        assert!(!strip.shown(0.5));
        assert_eq!(strip.opacity(0.5), 0.0);
        assert_eq!(strip.next_frame(0.5), None);

        strip.show(1.0);
        assert!(strip.shown(1.0));
        assert_eq!(strip.opacity(1.0), 1.0);
        assert_eq!(
            strip.next_frame(1.0),
            Some(Duration::from_secs_f64(STRIP_SECONDS - FADE_SECONDS))
        );
        // Halfway through the fade.
        let fading = 1.0 + STRIP_SECONDS - FADE_SECONDS / 2.0;
        assert!((strip.opacity(fading) - 0.5).abs() < 1e-6);
        assert_eq!(strip.next_frame(fading), Some(Duration::ZERO));
        let gone = 1.0 + STRIP_SECONDS;
        assert!(!strip.shown(gone));
        assert_eq!(strip.opacity(gone), 0.0);
        assert_eq!(strip.next_frame(gone), None);
    }

    #[test]
    fn the_strip_shows_longer_when_the_game_arrives() {
        let mut strip = Strip::default();
        strip.arrive(0.0);
        assert!(strip.shown(ARRIVAL_SECONDS - 0.1));
        assert!(!strip.shown(ARRIVAL_SECONDS));
        // Closing the overlay soon after doesn't cut it short.
        strip.show(1.0);
        assert!(strip.shown(ARRIVAL_SECONDS - 0.1));
    }

    #[test]
    fn hovering_keeps_the_strip_and_never_shortens_it() {
        let mut strip = Strip::default();
        strip.show(0.0);
        strip.hover(0.0);
        assert!(strip.shown(STRIP_SECONDS - 0.1));
        // Near the end, the pointer on the strip holds it a little longer.
        strip.hover(STRIP_SECONDS - 0.1);
        assert!(strip.shown(STRIP_SECONDS + HOVER_SECONDS - 0.2));
        assert!(!strip.shown(STRIP_SECONDS + HOVER_SECONDS));
    }

    #[test]
    fn dismissing_hides_the_strip_at_once() {
        let mut strip = Strip::default();
        strip.show(0.0);
        strip.dismiss();
        assert!(!strip.shown(0.0));
        assert_eq!(strip.opacity(0.0), 0.0);
        // The pointer is still on the X that was just clicked.
        strip.hover(0.1);
        assert!(!strip.shown(0.1));
        strip.show(1.0);
        assert!(strip.shown(1.0));
        strip.hover(STRIP_SECONDS + 0.5);
        assert!(strip.shown(STRIP_SECONDS + 1.5));
    }
}
