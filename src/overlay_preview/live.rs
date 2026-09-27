//! The in-game overlay's link to Hestia, over its standard input and output.
//! `crate::overlay_protocol` has the messages.

use std::{
    io::{BufRead, Write},
    sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc},
    thread,
    time::Duration,
};

use anyhow::{Context, bail};

use super::platform;
use crate::overlay_protocol::{self, FromOverlay, Library, Selection, Start, ToOverlay};

/// How long the strip shows after the overlay closes.
const STRIP_SECONDS: f64 = 3.0;

/// How long the strip shows when the game first comes to the front, long
/// enough to notice Alt+H.
const ARRIVAL_SECONDS: f64 = 7.0;

/// How long the strip stays after the pointer leaves it.
const HOVER_SECONDS: f64 = 1.5;

/// The end of the strip's time, where it fades out.
const FADE_SECONDS: f64 = 0.4;

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
    library: Option<Library>,
    closed: bool,
}

pub(super) struct Link {
    inbox: Arc<Mutex<Inbox>>,
    outbox: mpsc::Sender<FromOverlay>,
    host_window: Option<i64>,
    reported: Option<Selection>,
    closing: bool,
}

impl Link {
    /// Starts reading Hestia's messages and writing the overlay's.
    pub(super) fn start(ctx: egui::Context, host_window: Option<i64>) -> std::io::Result<Self> {
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
            closing: false,
        })
    }

    pub(super) fn host_window(&self) -> Option<i64> {
        self.host_window
    }

    /// The newest library since the last call.
    pub(super) fn take_library(&self) -> Option<Library> {
        lock(&self.inbox).library.take()
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
            Ok(ToOverlay::Library(library)) => {
                lock(inbox).library = Some(library);
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

#[cfg(test)]
mod tests {
    use super::*;

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
