//! Alt+H session rules for the native overlay preview.
//!
//! Holding Alt after Alt+H browses like a held modifier and hands focus back
//! when Alt is released.  A quick tap leaves the overlay open for plain keys
//! until Esc, Alt+H again, or a click on another window.

use std::time::{Duration, Instant};

/// Releasing Alt sooner than this, without a command, counts as a tap.
const TAP: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Session {
    #[default]
    Closed,
    /// Opened by Alt+H with Alt still held; closes when Alt is released.
    Held { since: Instant, used: bool },
    /// Opened by a tap, or without the keyboard; stays open until dismissed.
    Latched,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Transition {
    None,
    Opened,
    /// `return_focus` asks to hand the keyboard back, which is a no-op when
    /// the overlay does not have it.  It is false when another window
    /// already took the keyboard.
    Closed {
        return_focus: bool,
    },
}

impl Session {
    pub(super) fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }

    /// Alt+H toggles.  `focused` reports whether the overlay took the keyboard,
    /// `alt_down` whether Alt was still held once it had.
    pub(super) fn hotkey(&mut self, at: Instant, focused: bool, alt_down: bool) -> Transition {
        if self.is_open() {
            return self.close();
        }
        *self = if focused && alt_down {
            Self::Held {
                since: at,
                used: false,
            }
        } else {
            Self::Latched
        };
        Transition::Opened
    }

    pub(super) fn command(&mut self) {
        if let Self::Held { used, .. } = self {
            *used = true;
        }
    }

    pub(super) fn alt_released(&mut self, at: Instant) -> Transition {
        let Self::Held { since, used } = *self else {
            return Transition::None;
        };
        if used || at.saturating_duration_since(since) >= TAP {
            self.close()
        } else {
            *self = Self::Latched;
            Transition::None
        }
    }

    pub(super) fn escape(&mut self) -> Transition {
        if self.is_open() {
            self.close()
        } else {
            Transition::None
        }
    }

    /// Another window took the keyboard, for example after a click on the game.
    pub(super) fn deactivated(&mut self) -> Transition {
        if self.is_open() {
            *self = Self::Closed;
            Transition::Closed {
                return_focus: false,
            }
        } else {
            Transition::None
        }
    }

    fn close(&mut self) -> Transition {
        *self = Self::Closed;
        Transition::Closed { return_focus: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn held_alt_closes_on_release_after_a_command() {
        let start = Instant::now();
        let mut session = Session::default();
        assert_eq!(session.hotkey(start, true, true), Transition::Opened);
        assert!(matches!(session, Session::Held { .. }));
        session.command();
        assert_eq!(
            session.alt_released(after(start, 100)),
            Transition::Closed { return_focus: true }
        );
        assert!(!session.is_open());
    }

    #[test]
    fn long_hold_without_a_command_closes_on_release() {
        let start = Instant::now();
        let mut session = Session::default();
        session.hotkey(start, true, true);
        assert_eq!(
            session.alt_released(after(start, 250)),
            Transition::Closed { return_focus: true }
        );
    }

    #[test]
    fn quick_tap_latches_open() {
        let start = Instant::now();
        let mut session = Session::default();
        session.hotkey(start, true, true);
        assert_eq!(session.alt_released(after(start, 249)), Transition::None);
        assert_eq!(session, Session::Latched);
        // Alt no longer matters once latched.
        assert_eq!(session.alt_released(after(start, 900)), Transition::None);
        assert!(session.is_open());
    }

    #[test]
    fn alt_released_before_focus_arrived_latches_open() {
        let mut session = Session::default();
        assert_eq!(
            session.hotkey(Instant::now(), true, false),
            Transition::Opened
        );
        assert_eq!(session, Session::Latched);
    }

    #[test]
    fn failed_focus_latches_open() {
        let start = Instant::now();
        let mut session = Session::default();
        assert_eq!(session.hotkey(start, false, true), Transition::Opened);
        assert_eq!(session, Session::Latched);
        assert_eq!(session.alt_released(after(start, 10)), Transition::None);
    }

    #[test]
    fn hotkey_while_open_closes() {
        let start = Instant::now();
        for alt_down in [true, false] {
            let mut session = Session::default();
            session.hotkey(start, true, alt_down);
            assert_eq!(
                session.hotkey(after(start, 50), true, true),
                Transition::Closed { return_focus: true }
            );
            assert!(!session.is_open());
        }
    }

    #[test]
    fn escape_closes_only_an_open_session() {
        let mut session = Session::default();
        assert_eq!(session.escape(), Transition::None);
        session.hotkey(Instant::now(), true, false);
        assert_eq!(session.escape(), Transition::Closed { return_focus: true });
        assert_eq!(session.escape(), Transition::None);
    }

    #[test]
    fn deactivation_closes_without_returning_focus() {
        let mut session = Session::default();
        assert_eq!(session.deactivated(), Transition::None);
        session.hotkey(Instant::now(), true, true);
        assert_eq!(
            session.deactivated(),
            Transition::Closed {
                return_focus: false
            }
        );
        assert_eq!(session.deactivated(), Transition::None);
    }

    #[test]
    fn commands_only_mark_a_held_session() {
        let start = Instant::now();
        let mut session = Session::default();
        session.command();
        assert_eq!(session, Session::Closed);
        session.hotkey(start, true, false);
        session.command();
        assert_eq!(session, Session::Latched);
    }
}
