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
    self, Change, Changed, FromOverlay, Library, ModHotkeys, ModInfo, Selection, Settings, Start,
    ToOverlay,
};

const STRIP_SECONDS: f64 = 3.0;

const ARRIVAL_SECONDS: f64 = 7.0;

pub(super) const WARNING_SECONDS: f64 = 6.0;

const HOVER_SECONDS: f64 = 1.5;

const FADE_SECONDS: f64 = 0.4;

const ANSWER_SECONDS: f64 = 10.0;

const EXIT_GRACE: Duration = Duration::from_secs(3);

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

#[derive(Default)]
pub(super) struct News {
    pub settings: Option<Settings>,
    pub library: Option<Library>,
    pub answers: Vec<Changed>,
    pub gamebanana: Vec<ToOverlay>,
    pub hotkeys: Vec<ModHotkeys>,
    pub info: Vec<ModInfo>,
    pub pressed: Vec<(u64, Option<String>)>,
}

pub(super) struct Link {
    inbox: Arc<Mutex<Inbox>>,
    outbox: mpsc::Sender<FromOverlay>,
    host_window: Option<i64>,
    reported: Option<Selection>,
    typing: bool,
    opacity: u8,
    closing: bool,
}

impl Link {
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

    pub(super) fn take_news(&self) -> News {
        std::mem::take(&mut lock(&self.inbox).news)
    }

    pub(super) fn send_change(&self, change: Change) {
        let _ = self.outbox.send(FromOverlay::Change(change));
    }

    pub(super) fn ask(&self, request: FromOverlay) {
        let _ = self.outbox.send(request);
    }

    pub(super) fn take_closed(&mut self) -> bool {
        if self.closing || !lock(&self.inbox).closed {
            return false;
        }
        self.closing = true;
        true
    }

    pub(super) fn closing(&self) -> bool {
        self.closing
    }

    pub(super) fn report(&mut self, selection: Selection) {
        if self.reported.as_ref() != Some(&selection) {
            let _ = self.outbox.send(FromOverlay::Selection(selection.clone()));
            self.reported = Some(selection);
        }
    }

    pub(super) fn report_typing(&mut self, typing: bool) {
        if self.typing != typing {
            let _ = self.outbox.send(FromOverlay::Typing { typing });
            self.typing = typing;
        }
    }

    pub(super) fn report_opacity(&mut self, opacity: u8) {
        if self.opacity != opacity {
            let _ = self.outbox.send(FromOverlay::Opacity { opacity });
            self.opacity = opacity;
        }
    }

    pub(super) fn received_opacity(&mut self, opacity: u8) {
        self.opacity = opacity;
    }

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
            Ok(ToOverlay::Hotkeys(hotkeys)) => {
                lock(inbox).news.hotkeys.push(hotkeys);
                ctx.request_repaint();
            }
            Ok(ToOverlay::Info(info)) => {
                lock(inbox).news.info.push(info);
                ctx.request_repaint();
            }
            Ok(ToOverlay::HotkeyPressed { id, error }) => {
                lock(inbox).news.pressed.push((id, error));
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

#[derive(Debug, Default)]
pub(super) struct Strip {
    until: f64,
    dismissed: bool,
}

impl Strip {
    pub(super) fn show(&mut self, now: f64) {
        self.show_for(now, STRIP_SECONDS);
    }

    pub(super) fn arrive(&mut self, now: f64) {
        self.show_for(now, ARRIVAL_SECONDS);
    }

    fn show_for(&mut self, now: f64, seconds: f64) {
        self.until = self.until.max(now + seconds);
        self.dismissed = false;
    }

    pub(super) fn warn(&mut self, now: f64) {
        self.show_for(now, WARNING_SECONDS);
    }

    pub(super) fn hover(&mut self, now: f64) {
        if !self.dismissed {
            self.until = self.until.max(now + HOVER_SECONDS);
        }
    }

    pub(super) fn dismiss(&mut self) {
        self.until = f64::NEG_INFINITY;
        self.dismissed = true;
    }

    pub(super) fn shown(&self, now: f64) -> bool {
        self.until > now
    }

    pub(super) fn opacity(&self, now: f64) -> f32 {
        ((self.until - now) / FADE_SECONDS).clamp(0.0, 1.0) as f32
    }

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

pub(super) struct Changes {
    library: Library,
    shown: Library,
    asked: Vec<Asked>,
    next_id: u64,
}

struct Asked {
    id: u64,
    at: f64,
    mod_id: String,
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

    pub(super) fn answer(&mut self, answer: Changed) -> Option<String> {
        let index = self.asked.iter().position(|asked| asked.id == answer.id)?;
        self.asked.remove(index);
        answer.error
    }

    pub(super) fn expire(&mut self, now: f64) -> bool {
        let before = self.asked.len();
        self.asked.retain(|asked| now - asked.at < ANSWER_SECONDS);
        self.asked.len() != before
    }

    pub(super) fn take_update(&mut self) -> Option<Library> {
        let shown = self.with_asked();
        (shown != self.shown).then(|| {
            self.shown = shown.clone();
            shown
        })
    }

    pub(super) fn waiting(&self) -> HashSet<String> {
        self.asked
            .iter()
            .map(|asked| asked.mod_id.clone())
            .collect()
    }

    pub(super) fn next_frame(&self, now: f64) -> Option<Duration> {
        self.asked
            .iter()
            .map(|asked| asked.at + ANSWER_SECONDS)
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
        assert_eq!(changes.take_update(), None);
        assert_eq!(changes.waiting(), HashSet::from(["b".to_owned()]));

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
        assert!(changes.waiting().is_empty());
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
            Some(Duration::from_secs_f64(ANSWER_SECONDS))
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
        strip.show(1.0);
        assert!(strip.shown(ARRIVAL_SECONDS - 0.1));
    }

    #[test]
    fn hovering_keeps_the_strip_and_never_shortens_it() {
        let mut strip = Strip::default();
        strip.show(0.0);
        strip.hover(0.0);
        assert!(strip.shown(STRIP_SECONDS - 0.1));
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
        strip.hover(0.1);
        assert!(!strip.shown(0.1));
        strip.show(1.0);
        assert!(strip.shown(1.0));
        strip.hover(STRIP_SECONDS + 0.5);
        assert!(strip.shown(STRIP_SECONDS + 1.5));
    }
}
