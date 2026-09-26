//! Keyboard input for the native overlay preview.
//!
//! Games that run elevated or under anti-cheat stop other processes from
//! hooking, reading, or injecting their keyboard input, so the overlay does not
//! try.  On Windows, Alt+H is a system hotkey that still arrives over such
//! games, and `platform` uses it to move the keyboard focus to the overlay.  Key
//! messages then reach the overlay's own window procedure, and this module
//! turns them into commands and session events.

use std::time::Instant;

#[cfg(any(windows, test))]
use std::{collections::VecDeque, time::Duration};

use egui::Context;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    Mod(i32),
    Category(i32),
    EnableExclusive,
    EnableAdditive,
    Toggle,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Event {
    /// Alt+H arrived.  `focused` reports whether the overlay has the keyboard,
    /// `alt_down` whether Alt was still held once it had.
    Hotkey {
        at: Instant,
        focused: bool,
        alt_down: bool,
    },
    Command(Command),
    /// The last held Alt key was released.
    AltReleased(Instant),
    Escape,
    /// Another window took the keyboard.
    Deactivated,
}

#[cfg(any(windows, test))]
mod vk {
    pub(super) const TAB: u32 = 0x09;
    pub(super) const RETURN: u32 = 0x0D;
    pub(super) const SHIFT: u32 = 0x10;
    pub(super) const CONTROL: u32 = 0x11;
    pub(super) const MENU: u32 = 0x12;
    pub(super) const ESCAPE: u32 = 0x1B;
    pub(super) const SPACE: u32 = 0x20;
    pub(super) const A: u32 = 0x41;
    pub(super) const D: u32 = 0x44;
    pub(super) const E: u32 = 0x45;
    pub(super) const H: u32 = 0x48;
    pub(super) const Q: u32 = 0x51;
    pub(super) const X: u32 = 0x58;
    pub(super) const LWIN: u32 = 0x5B;
    pub(super) const RWIN: u32 = 0x5C;
    pub(super) const F10: u32 = 0x79;
    pub(super) const LSHIFT: u32 = 0xA0;
    pub(super) const RSHIFT: u32 = 0xA1;
    pub(super) const LCONTROL: u32 = 0xA2;
    pub(super) const RCONTROL: u32 = 0xA3;
    pub(super) const LMENU: u32 = 0xA4;
    pub(super) const RMENU: u32 = 0xA5;
}

#[cfg(any(windows, test))]
const WM_KEYDOWN: u32 = 0x0100;
#[cfg(any(windows, test))]
const WM_KEYUP: u32 = 0x0101;
#[cfg(any(windows, test))]
const WM_SYSKEYDOWN: u32 = 0x0104;
#[cfg(any(windows, test))]
const WM_SYSKEYUP: u32 = 0x0105;

/// Key messages report Shift with a generic virtual key and tell the sides
/// apart only by scan code.
#[cfg(any(windows, test))]
const RIGHT_SHIFT_SCAN_CODE: u32 = 0x36;

#[cfg(any(windows, test))]
const REPEAT_DELAY: Duration = Duration::from_millis(300);
#[cfg(any(windows, test))]
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

#[cfg(any(windows, test))]
#[derive(Clone, Copy)]
enum Role {
    /// Fires on press, then repeats while held.
    Navigate(Command),
    /// Fires once per press.
    Activate(Command),
    Escape,
    Alt,
    /// Swallowed without a command, so a press that starts in the overlay
    /// also ends there: H, whose press went to the hotkey, F10, the reload
    /// key, and Tab, Enter and Space, which would otherwise move or press
    /// egui's keyboard focus.
    Swallow,
    /// Passed through.  Other keys do nothing while one is held.
    Windows,
}

/// Every key the overlay tracks.  A key's bit in the router masks is
/// `1 << index`, and the navigation keys come first so their index also
/// addresses `Router::next_repeat`.
#[cfg(any(windows, test))]
const KEYS: [(u32, Role); 19] = [
    (vk::Q, Role::Navigate(Command::Mod(-1))),
    (vk::E, Role::Navigate(Command::Mod(1))),
    (vk::A, Role::Navigate(Command::Category(-1))),
    (vk::D, Role::Navigate(Command::Category(1))),
    (vk::LSHIFT, Role::Activate(Command::EnableAdditive)),
    (vk::RSHIFT, Role::Activate(Command::EnableAdditive)),
    (vk::LCONTROL, Role::Activate(Command::EnableExclusive)),
    (vk::RCONTROL, Role::Activate(Command::EnableExclusive)),
    (vk::X, Role::Activate(Command::Toggle)),
    (vk::ESCAPE, Role::Escape),
    (vk::LMENU, Role::Alt),
    (vk::RMENU, Role::Alt),
    (vk::H, Role::Swallow),
    (vk::F10, Role::Swallow),
    (vk::TAB, Role::Swallow),
    (vk::RETURN, Role::Swallow),
    (vk::SPACE, Role::Swallow),
    (vk::LWIN, Role::Windows),
    (vk::RWIN, Role::Windows),
];

#[cfg(any(windows, test))]
const fn bit(key: u32) -> u32 {
    let mut index = 0;
    while index < KEYS.len() {
        if KEYS[index].0 == key {
            return 1 << index;
        }
        index += 1;
    }
    panic!("untracked virtual key");
}

#[cfg(any(windows, test))]
const ALT_KEYS: u32 = bit(vk::LMENU) | bit(vk::RMENU);
#[cfg(any(windows, test))]
const WINDOWS_KEYS: u32 = bit(vk::LWIN) | bit(vk::RWIN);

#[cfg(any(windows, test))]
fn slot(key: u32) -> Option<(usize, Role)> {
    KEYS.iter()
        .position(|&(tracked, _)| tracked == key)
        .map(|index| (index, KEYS[index].1))
}

/// Whether the overlay window swallows the key instead of passing it on.
#[cfg(any(windows, test))]
fn consumes(key: u32) -> bool {
    slot(key).is_some_and(|(_, role)| !matches!(role, Role::Windows))
}

#[cfg(any(windows, test))]
fn resolve(key: u32, scan_code: u32, extended: bool) -> u32 {
    match key {
        vk::SHIFT if scan_code == RIGHT_SHIFT_SCAN_CODE => vk::RSHIFT,
        vk::SHIFT => vk::LSHIFT,
        vk::CONTROL if extended => vk::RCONTROL,
        vk::CONTROL => vk::LCONTROL,
        vk::MENU if extended => vk::RMENU,
        vk::MENU => vk::LMENU,
        other => other,
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KeyMessage {
    pub(super) key: u32,
    pub(super) pressed: bool,
}

#[cfg(any(windows, test))]
impl KeyMessage {
    /// Reads a key-down or key-up window message, with Shift, Ctrl and Alt
    /// resolved to their left or right key.
    pub(super) fn parse(message: u32, wparam: usize, lparam: isize) -> Option<Self> {
        let pressed = match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => return None,
        };
        let flags = (lparam as usize >> 16) as u32;
        let key = resolve(wparam as u32, flags & 0xFF, flags & 0x100 != 0);
        Some(Self { key, pressed })
    }

    /// AltGr reports a fake left Ctrl right before right Alt, on both press
    /// and release.  `next` peeks at the following key message.
    fn is_altgr_control(self, next: impl FnOnce() -> Option<Self>) -> bool {
        self.key == vk::LCONTROL
            && next().is_some_and(|next| next.key == vk::RMENU && next.pressed == self.pressed)
    }
}

/// Tracks the keys held while the overlay has the keyboard.
///
/// `ours` marks keys whose press reached the overlay.  A key already held when
/// the overlay took the keyboard belongs to the game: its repeats do nothing,
/// and it does not hold up handing the keyboard back.
#[cfg(any(windows, test))]
#[derive(Debug, Default)]
struct Router {
    down: u32,
    ours: u32,
    next_repeat: [Option<Instant>; 4],
    events: VecDeque<Event>,
}

#[cfg(any(windows, test))]
impl Router {
    /// Starts over from the physical key state.  Of `ours`, only keys that
    /// are still held are kept.
    fn reset(&mut self, held: u32, ours: u32) {
        self.down = held;
        self.ours = ours & held;
        self.next_repeat = [None; 4];
    }

    fn clear(&mut self) {
        self.reset(0, 0);
    }

    /// The hotkey swallowed H's press while the overlay had the keyboard, so
    /// its repeats and release are the overlay's too.
    fn hotkey_pressed(&mut self) {
        self.down |= bit(vk::H);
        self.ours |= bit(vk::H);
    }

    fn alt_down(&self) -> bool {
        self.down & ALT_KEYS != 0
    }

    /// Whether a key that went down in the overlay is still `held`.  Handing
    /// the keyboard back now would send its repeats and release to the game.
    fn busy(&self, held: u32) -> bool {
        self.ours & held != 0
    }

    fn key(&mut self, key: u32, pressed: bool, now: Instant) {
        let Some((index, role)) = slot(key) else {
            return;
        };
        let mask: u32 = 1 << index;
        let repeat = self.next_repeat.get_mut(index);

        if !pressed {
            let was_down = self.down & mask != 0;
            self.down &= !mask;
            self.ours &= !mask;
            if let Some(next) = repeat {
                *next = None;
            }
            if was_down && matches!(role, Role::Alt) && !self.alt_down() {
                self.events.push_back(Event::AltReleased(now));
            }
            return;
        }

        let chord = self.down & WINDOWS_KEYS != 0;
        if self.down & mask != 0 {
            // An auto-repeat.  Only a navigation press that moved the
            // selection repeats, on its own schedule.
            if let (Role::Navigate(command), Some(next)) = (role, repeat)
                && !chord
                && next.is_some_and(|at| now >= at)
            {
                *next = Some(now + REPEAT_INTERVAL);
                self.events.push_back(Event::Command(command));
            }
            return;
        }

        self.down |= mask;
        if !matches!(role, Role::Windows) {
            self.ours |= mask;
        }
        if chord {
            return;
        }
        match role {
            Role::Navigate(command) => {
                if let Some(next) = repeat {
                    *next = Some(now + REPEAT_DELAY);
                }
                self.events.push_back(Event::Command(command));
            }
            Role::Activate(command) => self.events.push_back(Event::Command(command)),
            Role::Escape => self.events.push_back(Event::Escape),
            Role::Alt | Role::Swallow | Role::Windows => {}
        }
    }
}

#[cfg(windows)]
struct Shared {
    ctx: Context,
    router: Router,
}

// The window procedure runs on the thread that owns the window, which is also
// the thread that runs the UI.
#[cfg(windows)]
thread_local! {
    static SHARED: std::cell::RefCell<Option<Shared>> = const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
fn with_shared<T>(f: impl FnOnce(&mut Shared) -> T) -> Option<T> {
    SHARED
        .try_with(|shared| {
            let mut shared = shared.try_borrow_mut().ok()?;
            shared.as_mut().map(f)
        })
        .ok()
        .flatten()
}

/// Updates the router, then asks for a frame once the borrow has ended.
#[cfg(windows)]
fn notify(update: impl FnOnce(&mut Router)) {
    let ctx = with_shared(|shared| {
        update(&mut shared.router);
        shared.ctx.clone()
    });
    if let Some(ctx) = ctx {
        ctx.request_repaint();
    }
}

#[cfg(windows)]
fn physically_down(key: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    (unsafe { GetAsyncKeyState(key as i32) }) < 0
}

/// Reads the tracked keys.  This only sees the truth while the overlay has
/// the keyboard: a game that blocks input also hides it from this call.
#[cfg(windows)]
fn physical_keys() -> u32 {
    KEYS.iter()
        .enumerate()
        .filter(|&(_, &(key, _))| physically_down(key))
        .fold(0, |held, (index, _)| held | (1 << index))
}

/// Whether a `Keyboard` is alive on this thread.
#[cfg(windows)]
pub(super) fn installed() -> bool {
    SHARED
        .try_with(|shared| shared.try_borrow().map_or(true, |shared| shared.is_some()))
        .unwrap_or(false)
}

/// Routes a key message sent to the overlay window and returns whether the
/// window should swallow it.  `next` peeks at the following key message.
#[cfg(windows)]
pub(super) fn key_message(message: KeyMessage, next: impl FnOnce() -> Option<KeyMessage>) -> bool {
    if !installed() || slot(message.key).is_none() {
        return false;
    }
    let consumed = consumes(message.key);
    // Peek before borrowing: PeekMessageW can dispatch sent messages into the
    // window procedure.
    if message.is_altgr_control(next) {
        return consumed;
    }
    // With both Shift keys held, Windows reports only the last release.
    let other_shift = match message.key {
        vk::LSHIFT if !message.pressed => Some(vk::RSHIFT),
        vk::RSHIFT if !message.pressed => Some(vk::LSHIFT),
        _ => None,
    }
    .filter(|&key| !physically_down(key));
    let now = Instant::now();
    notify(|router| {
        router.key(message.key, message.pressed, now);
        if let Some(key) = other_shift {
            router.key(key, false, now);
        }
    });
    consumed
}

/// Alt+H arrived.  `took_focus` is true when the overlay has just taken the
/// keyboard from another window.
#[cfg(windows)]
pub(super) fn hotkey(focused: bool, took_focus: bool) {
    let held = if took_focus { physical_keys() } else { 0 };
    let at = Instant::now();
    notify(|router| {
        if took_focus {
            router.reset(held, bit(vk::H));
        } else if focused {
            router.hotkey_pressed();
        }
        let alt_down = focused && router.alt_down();
        router.events.push_back(Event::Hotkey {
            at,
            focused,
            alt_down,
        });
    });
}

#[cfg(windows)]
pub(super) fn activated() {
    if installed() {
        let held = physical_keys();
        notify(|router| router.reset(held, 0));
    }
}

#[cfg(windows)]
pub(super) fn deactivated() {
    notify(|router| {
        router.clear();
        router.events.push_back(Event::Deactivated);
    });
}

/// Keyboard state for the overlay window.  Create it on the thread that runs
/// the window, and keep one at a time.
pub(super) struct Keyboard {
    _private: (),
}

impl Keyboard {
    pub(super) fn new(ctx: Context) -> Self {
        #[cfg(windows)]
        SHARED.with(|shared| {
            *shared.borrow_mut() = Some(Shared {
                ctx,
                router: Router::default(),
            });
        });
        #[cfg(not(windows))]
        let _ = ctx;
        Self { _private: () }
    }

    pub(super) fn drain(&self) -> Vec<Event> {
        #[cfg(windows)]
        {
            with_shared(|shared| shared.router.events.drain(..).collect()).unwrap_or_default()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }

    /// Whether a key pressed in the overlay is still physically held.
    pub(super) fn busy(&self) -> bool {
        #[cfg(windows)]
        {
            let held = physical_keys();
            with_shared(|shared| shared.router.busy(held)).unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

#[cfg(windows)]
impl Drop for Keyboard {
    fn drop(&mut self) {
        let _ = SHARED.try_with(|shared| {
            if let Ok(mut shared) = shared.try_borrow_mut() {
                *shared = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: u32 = u32::MAX;

    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    fn drain(router: &mut Router) -> Vec<Event> {
        router.events.drain(..).collect()
    }

    fn command(command: Command) -> Event {
        Event::Command(command)
    }

    #[test]
    fn navigation_keys_map_to_commands() {
        let now = Instant::now();
        let mut router = Router::default();
        for key in [vk::Q, vk::E, vk::A, vk::D] {
            assert!(consumes(key));
            router.key(key, true, now);
        }
        assert_eq!(
            drain(&mut router),
            vec![
                command(Command::Mod(-1)),
                command(Command::Mod(1)),
                command(Command::Category(-1)),
                command(Command::Category(1)),
            ]
        );
    }

    #[test]
    fn other_keys_pass_through_without_commands() {
        let now = Instant::now();
        let mut router = Router::default();
        // W, S, F4, an arrow for the opacity slider, and the Windows keys.
        for key in [0x57, 0x53, 0x73, 0x25, vk::LWIN, vk::RWIN] {
            assert!(!consumes(key));
            router.key(key, true, now);
            router.key(key, false, now);
        }
        assert!(drain(&mut router).is_empty());
        assert!(!router.busy(ALL));
    }

    #[test]
    fn swallowed_keys_have_no_commands_but_hold_up_focus_return() {
        let now = Instant::now();
        let keys = [
            vk::LMENU,
            vk::RMENU,
            vk::H,
            vk::F10,
            vk::TAB,
            vk::RETURN,
            vk::SPACE,
        ];
        let mut router = Router::default();
        for key in keys {
            assert!(consumes(key));
            router.key(key, true, now);
        }
        assert!(drain(&mut router).is_empty());
        assert!(router.busy(ALL));
        for key in keys {
            router.key(key, false, now);
        }
        assert!(!router.busy(ALL));
    }

    #[test]
    fn activation_keys_are_edge_triggered() {
        let now = Instant::now();
        let mut router = Router::default();
        for (key, expected) in [
            (vk::LSHIFT, Command::EnableAdditive),
            (vk::RSHIFT, Command::EnableAdditive),
            (vk::LCONTROL, Command::EnableExclusive),
            (vk::RCONTROL, Command::EnableExclusive),
            (vk::X, Command::Toggle),
        ] {
            assert!(consumes(key));
            router.key(key, true, now);
            router.key(key, true, after(now, 500));
            router.key(key, true, after(now, 1000));
            assert_eq!(drain(&mut router), vec![command(expected)]);
            router.key(key, false, after(now, 1100));
            router.key(key, true, after(now, 1200));
            assert_eq!(drain(&mut router), vec![command(expected)]);
            router.key(key, false, after(now, 1300));
        }
    }

    #[test]
    fn keys_held_before_the_overlay_took_the_keyboard_wait_for_release() {
        let now = Instant::now();
        let mut router = Router::default();
        router.reset(bit(vk::A) | bit(vk::LSHIFT), 0);
        router.key(vk::A, true, after(now, 400));
        router.key(vk::A, true, after(now, 800));
        router.key(vk::LSHIFT, true, after(now, 800));
        assert!(drain(&mut router).is_empty());
        router.key(vk::A, false, after(now, 900));
        router.key(vk::A, true, after(now, 1000));
        assert_eq!(drain(&mut router), vec![command(Command::Category(-1))]);
    }

    #[test]
    fn windows_key_chords_are_ignored() {
        let now = Instant::now();
        let mut router = Router::default();
        router.key(vk::LWIN, true, now);
        router.key(vk::Q, true, now);
        router.key(vk::X, true, now);
        router.key(vk::ESCAPE, true, now);
        router.key(vk::LWIN, false, now);
        // Q was pressed as part of the chord, so its repeats stay quiet.
        router.key(vk::Q, true, after(now, 1000));
        assert!(drain(&mut router).is_empty());
        router.key(vk::Q, false, after(now, 1100));
        router.key(vk::Q, true, after(now, 1200));
        assert_eq!(drain(&mut router), vec![command(Command::Mod(-1))]);
    }

    #[test]
    fn alt_release_is_reported_after_the_last_alt_key() {
        let now = Instant::now();
        let mut router = Router::default();
        router.reset(bit(vk::LMENU), 0);
        assert!(router.alt_down());
        router.key(vk::RMENU, true, now);
        router.key(vk::LMENU, false, after(now, 10));
        assert!(router.alt_down());
        assert!(drain(&mut router).is_empty());
        router.key(vk::RMENU, false, after(now, 20));
        assert!(!router.alt_down());
        assert_eq!(drain(&mut router), vec![Event::AltReleased(after(now, 20))]);
    }

    #[test]
    fn alt_release_without_a_press_is_ignored() {
        let mut router = Router::default();
        router.key(vk::LMENU, false, Instant::now());
        assert!(drain(&mut router).is_empty());
    }

    #[test]
    fn escape_is_edge_triggered() {
        let now = Instant::now();
        let mut router = Router::default();
        router.key(vk::ESCAPE, true, now);
        router.key(vk::ESCAPE, true, after(now, 500));
        assert_eq!(drain(&mut router), vec![Event::Escape]);
    }

    #[test]
    fn navigation_repeats_after_a_delay() {
        let start = Instant::now();
        let mut router = Router::default();
        router.key(vk::D, true, start);
        router.key(vk::D, true, after(start, 299));
        assert_eq!(router.events.len(), 1);
        router.key(vk::D, true, start + REPEAT_DELAY);
        router.key(
            vk::D,
            true,
            start + REPEAT_DELAY + Duration::from_millis(119),
        );
        assert_eq!(router.events.len(), 2);
        router.key(vk::D, true, start + REPEAT_DELAY + REPEAT_INTERVAL);
        assert_eq!(drain(&mut router), vec![command(Command::Category(1)); 3]);
        router.key(vk::D, false, after(start, 1000));
        router.key(vk::D, true, after(start, 1001));
        router.key(vk::D, true, after(start, 1002));
        assert_eq!(drain(&mut router), vec![command(Command::Category(1))]);
    }

    #[test]
    fn busy_counts_only_keys_pressed_in_the_overlay() {
        let now = Instant::now();
        let mut router = Router::default();
        router.reset(bit(vk::A) | bit(vk::LMENU), 0);
        assert!(!router.busy(ALL));
        router.key(vk::Q, true, now);
        assert!(router.busy(ALL));
        // A key whose release never arrived is not held any more.
        assert!(!router.busy(ALL & !bit(vk::Q)));
        router.key(vk::Q, false, now);
        assert!(!router.busy(ALL));
    }

    #[test]
    fn the_hotkey_owns_h_until_it_is_released() {
        let now = Instant::now();
        let mut router = Router::default();
        router.hotkey_pressed();
        assert!(router.busy(ALL));
        router.key(vk::H, true, after(now, 500));
        assert!(drain(&mut router).is_empty());
        router.key(vk::H, false, after(now, 600));
        assert!(!router.busy(ALL));

        router.reset(bit(vk::LMENU) | bit(vk::H), bit(vk::H));
        assert!(router.busy(ALL));
        router.reset(bit(vk::LMENU), bit(vk::H));
        assert!(!router.busy(ALL));
        assert!(router.alt_down());
    }

    #[test]
    fn clear_forgets_held_keys() {
        let now = Instant::now();
        let mut router = Router::default();
        router.key(vk::LMENU, true, now);
        router.key(vk::D, true, now);
        router.clear();
        assert!(!router.alt_down());
        assert!(!router.busy(ALL));
        // Releases that arrive after the reset are not reported.
        router.key(vk::LMENU, false, now);
        assert_eq!(drain(&mut router), vec![command(Command::Category(1))]);
    }

    #[test]
    fn generic_modifiers_resolve_to_a_side() {
        assert_eq!(resolve(vk::SHIFT, 0x2A, false), vk::LSHIFT);
        assert_eq!(resolve(vk::SHIFT, RIGHT_SHIFT_SCAN_CODE, false), vk::RSHIFT);
        assert_eq!(resolve(vk::CONTROL, 0x1D, false), vk::LCONTROL);
        assert_eq!(resolve(vk::CONTROL, 0x1D, true), vk::RCONTROL);
        assert_eq!(resolve(vk::MENU, 0x38, false), vk::LMENU);
        assert_eq!(resolve(vk::MENU, 0x38, true), vk::RMENU);
        assert_eq!(resolve(vk::Q, 0x10, false), vk::Q);
    }

    #[test]
    fn key_messages_parse_side_and_direction() {
        let right_shift_down = ((RIGHT_SHIFT_SCAN_CODE << 16) | 1) as isize;
        assert_eq!(
            KeyMessage::parse(WM_KEYDOWN, vk::SHIFT as usize, right_shift_down),
            Some(KeyMessage {
                key: vk::RSHIFT,
                pressed: true,
            })
        );
        // Right Alt up: extended, context and transition bits set.  Windows
        // may sign-extend the 32-bit flags.
        let right_alt_up = 0xE138_0001_u32;
        for lparam in [right_alt_up as isize, right_alt_up as i32 as isize] {
            assert_eq!(
                KeyMessage::parse(WM_SYSKEYUP, vk::MENU as usize, lparam),
                Some(KeyMessage {
                    key: vk::RMENU,
                    pressed: false,
                })
            );
        }
        assert_eq!(
            KeyMessage::parse(WM_SYSKEYDOWN, vk::Q as usize, 0x2010_0001),
            Some(KeyMessage {
                key: vk::Q,
                pressed: true,
            })
        );
        // WM_CHAR is not a key message.
        assert_eq!(KeyMessage::parse(0x0102, 0x71, 0x0010_0001), None);
    }

    #[test]
    fn altgr_fake_control_is_recognised() {
        let message = |key, pressed| KeyMessage { key, pressed };
        let left_control = message(vk::LCONTROL, true);
        assert!(left_control.is_altgr_control(|| Some(message(vk::RMENU, true))));
        assert!(message(vk::LCONTROL, false).is_altgr_control(|| Some(message(vk::RMENU, false))));
        assert!(!left_control.is_altgr_control(|| Some(message(vk::RMENU, false))));
        assert!(!left_control.is_altgr_control(|| Some(message(vk::LMENU, true))));
        assert!(!left_control.is_altgr_control(|| None));
        assert!(!message(vk::RCONTROL, true).is_altgr_control(|| Some(message(vk::RMENU, true))));
        assert!(!message(vk::Q, true).is_altgr_control(|| unreachable!()));
    }

    #[cfg(windows)]
    #[test]
    fn window_messages_reach_the_installed_keyboard() {
        let message = |key, pressed| KeyMessage { key, pressed };
        assert!(!installed());
        let keyboard = Keyboard::new(Context::default());
        assert!(installed());

        assert!(key_message(message(vk::Q, true), || None));
        assert!(!key_message(message(0x57, true), || None));
        assert!(!key_message(message(vk::LWIN, true), || None));
        assert_eq!(keyboard.drain(), vec![command(Command::Mod(-1))]);

        // AltGr's fake Ctrl is swallowed without enabling anything.
        let right_alt = || Some(message(vk::RMENU, true));
        assert!(key_message(message(vk::LCONTROL, true), right_alt));
        assert!(keyboard.drain().is_empty());

        hotkey(true, false);
        deactivated();
        assert!(matches!(
            keyboard.drain().as_slice(),
            [
                Event::Hotkey {
                    focused: true,
                    alt_down: false,
                    ..
                },
                Event::Deactivated,
            ]
        ));

        drop(keyboard);
        assert!(!installed());
        assert!(!key_message(message(vk::Q, true), || None));
    }
}
