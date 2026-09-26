use std::time::Duration;

use super::layouts::ShortcutAvailability;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hint {
    Navigate,
    Mods,
    Categories,
    Exclusive,
    Toggle,
    Search,
    /// Enter while typing a search.
    TypedExclusive,
    /// Shift+Enter while typing a search.
    TypedToggle,
    DoneTyping,
    EditSearch,
    ClearSearch,
}

/// Which keys the hints describe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Mode {
    /// The normal keys.
    #[default]
    Normal,
    /// Typing a search: letters and Space type.
    Typing,
    /// The normal keys, on search results.
    Results,
}

impl Mode {
    pub(super) fn new(typing: bool, filtering: bool) -> Self {
        if typing {
            Self::Typing
        } else if filtering {
            Self::Results
        } else {
            Self::Normal
        }
    }

    pub(super) fn sequence(self) -> &'static [Hint] {
        match self {
            Self::Normal => &[
                Hint::Navigate,
                Hint::Mods,
                Hint::Categories,
                Hint::Exclusive,
                Hint::Toggle,
                Hint::Search,
            ],
            Self::Typing => &[
                Hint::TypedExclusive,
                Hint::TypedToggle,
                Hint::DoneTyping,
                Hint::ClearSearch,
            ],
            Self::Results => &[
                Hint::Navigate,
                Hint::Mods,
                Hint::Categories,
                Hint::Exclusive,
                Hint::Toggle,
                Hint::EditSearch,
                Hint::ClearSearch,
            ],
        }
    }
}

impl Hint {
    pub(super) fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Navigate => &["W", "A", "S", "D"],
            Self::Mods => &["Q", "E"],
            Self::Categories => &["Z", "C"],
            Self::Exclusive => &["Space"],
            Self::Toggle => &["X"],
            Self::Search | Self::DoneTyping | Self::EditSearch => &["Tab"],
            Self::TypedExclusive => &["Enter"],
            Self::TypedToggle => &["Shift", "Enter"],
            Self::ClearSearch => &["Esc"],
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Navigate => "Navigate",
            Self::Mods => "Browse mods",
            Self::Categories => "Browse categories",
            Self::Exclusive | Self::TypedExclusive => "Enable this mod, disable others",
            Self::Toggle | Self::TypedToggle => "Toggle Enable/Disable",
            Self::Search => "Search",
            Self::DoneTyping => "Done typing",
            Self::EditSearch => "Edit search",
            Self::ClearSearch => "Clear search",
        }
    }

    /// Whether the keys do anything right now.  Dimmed otherwise.
    pub(super) fn enabled(self, available: ShortcutAvailability) -> bool {
        match self {
            Self::Navigate => available.categories || available.mods,
            Self::Mods => available.mods,
            Self::Categories => available.categories,
            Self::Exclusive | Self::TypedExclusive => available.exclusive,
            Self::Toggle | Self::TypedToggle => available.toggle,
            Self::Search | Self::DoneTyping | Self::EditSearch | Self::ClearSearch => true,
        }
    }
}

/// A letter gets a square keycap, a key name a wider one.
pub(super) fn key_width(key: &str) -> f32 {
    if key.chars().count() == 1 { 19.0 } else { 36.0 }
}

const SPEED: f64 = 55.0;
const IDLE_REPAINT: Duration = Duration::from_secs(60);
const ANIMATED_REPAINT: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(super) struct Ticker {
    opened_at: Option<f64>,
    mode: Mode,
}

pub(super) struct Frame {
    pub offset: f32,
    pub repaint_after: Duration,
}

impl Ticker {
    /// Starts the hints from the first one when the overlay opens, and again
    /// when they change.
    pub fn update(&mut self, expanded: bool, mode: Mode, now: f64) {
        if !expanded || mode != self.mode {
            self.opened_at = None;
        }
        self.mode = mode;
        if expanded {
            self.opened_at.get_or_insert(now);
        }
    }

    pub fn frame(&self, now: f64, animate: bool, cycle_width: f32) -> Frame {
        if !animate || !cycle_width.is_finite() || cycle_width <= 0.0 {
            return Frame {
                offset: 0.0,
                repaint_after: IDLE_REPAINT,
            };
        }

        let elapsed = (now - self.opened_at.unwrap_or(now)).max(0.0);
        let offset = ((elapsed * SPEED) % f64::from(cycle_width)) as f32;

        Frame {
            offset,
            repaint_after: ANIMATED_REPAINT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marquee_moves_linearly_and_wraps_without_a_jump() {
        let mut ticker = Ticker::default();
        ticker.update(true, Mode::Normal, 10.0);

        assert_eq!(ticker.frame(10.0, true, 100.0).offset, 0.0);
        assert_eq!(ticker.frame(11.0, true, 100.0).offset, 55.0);
        assert!((ticker.frame(11.1, true, 100.0).offset - 60.5).abs() < 0.001);
        assert!((ticker.frame(11.9, true, 100.0).offset - 4.5).abs() < 0.001);
        assert!((ticker.frame(11.9, true, 100.0).offset - 100.0).abs() > 0.1);
        assert_eq!(ticker.frame(12.0, true, 100.0).offset, 10.0);
    }

    #[test]
    fn navigation_does_not_restart_but_reopening_does() {
        let mut ticker = Ticker::default();
        ticker.update(true, Mode::Normal, 10.0);
        ticker.update(true, Mode::Normal, 15.0);
        assert_eq!(ticker.frame(15.0, true, 100.0).offset, 75.0);

        ticker.update(false, Mode::Normal, 16.0);
        ticker.update(true, Mode::Normal, 20.0);
        assert_eq!(ticker.frame(20.0, true, 100.0).offset, 0.0);
    }

    #[test]
    fn changing_hints_restarts_the_marquee() {
        let mut ticker = Ticker::default();
        ticker.update(true, Mode::Normal, 10.0);
        ticker.update(true, Mode::Typing, 15.0);
        assert_eq!(ticker.frame(15.0, true, 100.0).offset, 0.0);
        ticker.update(true, Mode::Typing, 16.0);
        assert_eq!(ticker.frame(16.0, true, 100.0).offset, 55.0);

        ticker.update(true, Mode::Results, 17.0);
        assert_eq!(ticker.frame(17.0, true, 100.0).offset, 0.0);
    }

    #[test]
    fn disabled_animation_is_static_and_does_not_request_frequent_repaints() {
        let mut ticker = Ticker::default();
        ticker.update(true, Mode::Normal, 10.0);

        let frame = ticker.frame(12.0, false, 100.0);
        assert_eq!(frame.offset, 0.0);
        assert_eq!(frame.repaint_after, IDLE_REPAINT);
    }

    #[test]
    fn invalid_cycle_width_is_static() {
        let mut ticker = Ticker::default();
        ticker.update(true, Mode::Normal, 10.0);

        for cycle_width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let frame = ticker.frame(12.0, true, cycle_width);
            assert_eq!(frame.offset, 0.0);
            assert_eq!(frame.repaint_after, IDLE_REPAINT);
        }
    }

    #[test]
    fn the_search_decides_which_hints_show() {
        assert_eq!(Mode::new(false, false), Mode::Normal);
        assert_eq!(Mode::new(true, false), Mode::Typing);
        assert_eq!(Mode::new(true, true), Mode::Typing);
        assert_eq!(Mode::new(false, true), Mode::Results);

        let hints = |mode: Mode| {
            mode.sequence()
                .iter()
                .map(|hint| (hint.keys().join("+"), hint.label()))
                .collect::<Vec<_>>()
        };
        let normal = [
            ("W+A+S+D", "Navigate"),
            ("Q+E", "Browse mods"),
            ("Z+C", "Browse categories"),
            ("Space", "Enable this mod, disable others"),
            ("X", "Toggle Enable/Disable"),
        ];
        let expected = |extra: &[(&'static str, &'static str)]| {
            normal
                .iter()
                .chain(extra)
                .map(|&(keys, label)| (keys.to_owned(), label))
                .collect::<Vec<_>>()
        };
        assert_eq!(hints(Mode::Normal), expected(&[("Tab", "Search")]));
        assert_eq!(
            hints(Mode::Results),
            expected(&[("Tab", "Edit search"), ("Esc", "Clear search")])
        );
        assert_eq!(
            hints(Mode::Typing),
            [
                ("Enter", "Enable this mod, disable others"),
                ("Shift+Enter", "Toggle Enable/Disable"),
                ("Tab", "Done typing"),
                ("Esc", "Clear search"),
            ]
            .map(|(keys, label)| (keys.to_owned(), label))
        );
    }

    #[test]
    fn typed_actions_dim_like_their_keys_and_search_keys_never_dim() {
        let none = ShortcutAvailability {
            categories: false,
            mods: false,
            exclusive: false,
            toggle: false,
        };
        let all = ShortcutAvailability {
            categories: true,
            mods: true,
            exclusive: true,
            toggle: true,
        };
        for (typed, key) in [
            (Hint::TypedExclusive, Hint::Exclusive),
            (Hint::TypedToggle, Hint::Toggle),
        ] {
            assert_eq!(typed.enabled(none), key.enabled(none));
            assert_eq!(typed.enabled(all), key.enabled(all));
        }
        for hint in [
            Hint::Search,
            Hint::DoneTyping,
            Hint::EditSearch,
            Hint::ClearSearch,
        ] {
            assert!(hint.enabled(none));
        }
        assert!(!Hint::Navigate.enabled(none));
        assert!(Hint::Navigate.enabled(ShortcutAvailability { mods: true, ..none }));
    }

    #[test]
    fn letters_get_square_keycaps_and_names_wide_ones() {
        for key in ["W", "Q", "X"] {
            assert_eq!(key_width(key), 19.0);
        }
        for key in ["Space", "Tab", "Esc", "Enter", "Shift"] {
            assert_eq!(key_width(key), 36.0);
        }
    }
}
