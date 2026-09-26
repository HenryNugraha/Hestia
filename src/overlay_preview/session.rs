//! Alt+H session rules for the native overlay preview.
//!
//! Alt+H opens the overlay with the keyboard.  Alt+H again, Esc, or a click
//! on another window closes it.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Session {
    #[default]
    Closed,
    Open,
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
        self == Self::Open
    }

    /// Alt+H toggles.
    pub(super) fn hotkey(&mut self) -> Transition {
        if self.is_open() {
            self.close()
        } else {
            *self = Self::Open;
            Transition::Opened
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

    #[test]
    fn hotkey_toggles() {
        let mut session = Session::default();
        assert_eq!(session.hotkey(), Transition::Opened);
        assert!(session.is_open());
        assert_eq!(session.hotkey(), Transition::Closed { return_focus: true });
        assert!(!session.is_open());
        assert_eq!(session.hotkey(), Transition::Opened);
    }

    #[test]
    fn escape_closes_only_an_open_session() {
        let mut session = Session::default();
        assert_eq!(session.escape(), Transition::None);
        session.hotkey();
        assert_eq!(session.escape(), Transition::Closed { return_focus: true });
        assert_eq!(session.escape(), Transition::None);
    }

    #[test]
    fn deactivation_closes_without_returning_focus() {
        let mut session = Session::default();
        assert_eq!(session.deactivated(), Transition::None);
        session.hotkey();
        assert_eq!(
            session.deactivated(),
            Transition::Closed {
                return_focus: false
            }
        );
        assert_eq!(session.deactivated(), Transition::None);
        assert_eq!(session.hotkey(), Transition::Opened);
    }
}
