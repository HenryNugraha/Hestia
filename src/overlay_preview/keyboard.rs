//! Keyboard input for the native overlay preview.
//!
//! Games that run elevated or under anti-cheat stop other processes from
//! hooking, reading, or injecting their keyboard input, so the overlay does not
//! try.  On Windows, the overlay's key (Alt+H unless Settings changed it) is a
//! system hotkey that still arrives over such games, and `platform` uses it
//! to move the keyboard focus to the overlay.  Key
//! messages then reach the overlay's own window procedure, and this module
//! turns them into commands and session events.  While a search is being
//! typed, letters and Space pass on to the search field instead.  Capture runs
//! and other platforms read the same keys from egui.
//!
//! The keys the overlay doesn't use still reach the game: XXMI, the mod
//! loader, takes keys from a window titled "Hestia" as well as from the game,
//! so the overlay lends itself that title while one of them is down.

#[cfg(any(windows, test))]
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use std::cell::Cell;

use egui::Context;

use crate::model::OverlayHotkey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    /// Up to the mods row, or down to the categories row.
    Row(i32),
    /// Left or right within the current row.
    Move(i32),
    Mod(i32),
    Category(i32),
    /// Enable the highlighted mod and disable the others.
    Exclusive,
    Toggle,
    /// Turn the focused card over to its hotkeys, or back.
    Flip,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Event {
    /// The hotkey arrived.  `focused` reports whether the overlay has the
    /// keyboard.
    Hotkey {
        focused: bool,
    },
    Command(Command),
    /// Tab, F or Ctrl+F: start typing a search, or, from Tab while typing,
    /// stop typing and keep the results.
    Search,
    Escape,
    /// A click on the pinned overlay took the keyboard from another window.
    Clicked,
    /// Another window took the keyboard.
    Deactivated,
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
mod vk {
    pub(super) const TAB: u32 = 0x09;
    pub(super) const RETURN: u32 = 0x0D;
    pub(super) const SHIFT: u32 = 0x10;
    pub(super) const CONTROL: u32 = 0x11;
    pub(super) const MENU: u32 = 0x12;
    pub(super) const ESCAPE: u32 = 0x1B;
    pub(super) const SPACE: u32 = 0x20;
    pub(super) const LEFT: u32 = 0x25;
    pub(super) const UP: u32 = 0x26;
    pub(super) const RIGHT: u32 = 0x27;
    pub(super) const DOWN: u32 = 0x28;
    pub(super) const A: u32 = 0x41;
    pub(super) const C: u32 = 0x43;
    pub(super) const D: u32 = 0x44;
    pub(super) const E: u32 = 0x45;
    pub(super) const F: u32 = 0x46;
    #[cfg(test)]
    pub(super) const H: u32 = 0x48;
    pub(super) const Q: u32 = 0x51;
    pub(super) const R: u32 = 0x52;
    pub(super) const S: u32 = 0x53;
    pub(super) const W: u32 = 0x57;
    pub(super) const X: u32 = 0x58;
    pub(super) const Z: u32 = 0x5A;
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
/// How long the overlay keeps XXMI's title after the last key it lent it for
/// is up.  Longer than a slow game frame.
#[cfg(any(windows, test))]
const TITLE_LINGER: Duration = Duration::from_millis(200);
/// How often to look whether to give XXMI's title back.
#[cfg(windows)]
const TITLE_CHECK: Duration = Duration::from_millis(50);
#[cfg(any(windows, test))]
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Clone, Copy)]
enum Role {
    /// Fires on press, then repeats while held.
    Navigate(Command),
    /// Fires once per press.
    Activate(Command),
    /// Space and Enter: Exclusive, or Toggle while Shift is held.
    Enable,
    /// Tab, F and Ctrl+F start typing a search.  F types while searching,
    /// so only Tab stops.
    Search,
    Escape,
    /// Swallowed without a command, so a press that starts in the overlay
    /// also ends there: the modifiers, the hotkey's key, whose press went to
    /// the hotkey, and F10, the reload key.
    Swallow,
    /// Passed through.  Other keys do nothing while one is held.
    Windows,
}

impl Role {
    /// The event for a new press.  Repeats follow their own rules.
    fn event(self, shift: bool) -> Option<Event> {
        match self {
            Role::Navigate(command) | Role::Activate(command) => Some(Event::Command(command)),
            Role::Enable => Some(Event::Command(if shift {
                Command::Toggle
            } else {
                Command::Exclusive
            })),
            Role::Search => Some(Event::Search),
            Role::Escape => Some(Event::Escape),
            Role::Swallow | Role::Windows => None,
        }
    }
}

/// Every key the overlay tracks.  A key's bit in the router masks is
/// `1 << index`.
const KEYS: [(u32, Role); 29] = [
    (vk::W, Role::Activate(Command::Row(-1))),
    (vk::UP, Role::Activate(Command::Row(-1))),
    (vk::S, Role::Activate(Command::Row(1))),
    (vk::DOWN, Role::Activate(Command::Row(1))),
    (vk::A, Role::Navigate(Command::Move(-1))),
    (vk::LEFT, Role::Navigate(Command::Move(-1))),
    (vk::D, Role::Navigate(Command::Move(1))),
    (vk::RIGHT, Role::Navigate(Command::Move(1))),
    (vk::Q, Role::Navigate(Command::Mod(-1))),
    (vk::E, Role::Navigate(Command::Mod(1))),
    (vk::Z, Role::Navigate(Command::Category(-1))),
    (vk::C, Role::Navigate(Command::Category(1))),
    (vk::SPACE, Role::Enable),
    (vk::RETURN, Role::Enable),
    (vk::X, Role::Activate(Command::Toggle)),
    (vk::R, Role::Activate(Command::Flip)),
    (vk::TAB, Role::Search),
    (vk::F, Role::Search),
    (vk::ESCAPE, Role::Escape),
    (vk::LSHIFT, Role::Swallow),
    (vk::RSHIFT, Role::Swallow),
    (vk::LCONTROL, Role::Swallow),
    (vk::RCONTROL, Role::Swallow),
    (vk::LMENU, Role::Swallow),
    (vk::RMENU, Role::Swallow),
    (HOTKEY, Role::Swallow),
    (vk::F10, Role::Swallow),
    (vk::LWIN, Role::Windows),
    (vk::RWIN, Role::Windows),
];

/// Stands in `KEYS` for the hotkey's key, which Settings can change.  No
/// real key has code 0.
const HOTKEY: u32 = 0;

// Set and read on the thread that runs the UI and the window procedure.
thread_local! {
    static HOTKEY_KEYS: Cell<OverlayHotkey> = const { Cell::new(OverlayHotkey::DEFAULT) };
}

/// Makes `hotkey` the key the overlay tracks as its hotkey.
pub(super) fn set_hotkey(hotkey: OverlayHotkey) {
    HOTKEY_KEYS.set(hotkey);
}

fn hotkey_key() -> u32 {
    u32::from(HOTKEY_KEYS.get().key)
}

/// The key a `KEYS` entry stands for.
#[cfg(windows)]
fn real_key(key: u32) -> u32 {
    if key == HOTKEY { hotkey_key() } else { key }
}

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
const SHIFT_KEYS: u32 = bit(vk::LSHIFT) | bit(vk::RSHIFT);
#[cfg(any(windows, test))]
const WINDOWS_KEYS: u32 = bit(vk::LWIN) | bit(vk::RWIN);
/// Shift, Ctrl and Alt, which the game may take with a key it gets.
#[cfg(any(windows, test))]
const MODIFIER_KEYS: u32 =
    SHIFT_KEYS | bit(vk::LCONTROL) | bit(vk::RCONTROL) | bit(vk::LMENU) | bit(vk::RMENU);

fn slot(key: u32) -> Option<(usize, Role)> {
    let key = if key == hotkey_key() { HOTKEY } else { key };
    KEYS.iter()
        .position(|&(tracked, _)| tracked == key)
        .map(|index| (index, KEYS[index].1))
}

/// Whether the overlay window swallows the key instead of passing it on,
/// when no search is being typed.
#[cfg(any(windows, test))]
fn consumes(key: u32) -> bool {
    slot(key).is_some_and(|(_, role)| !matches!(role, Role::Windows))
}

/// Keys that type into the search while it is being typed, although they
/// are commands otherwise.  The hotkey's key types unless it's an F key.
fn types(key: u32) -> bool {
    let typed = matches!(key, 0x30..=0x39 | 0x41..=0x5A);
    (key == hotkey_key() && typed)
        || matches!(
            key,
            vk::W
                | vk::A
                | vk::S
                | vk::D
                | vk::Q
                | vk::E
                | vk::Z
                | vk::C
                | vk::X
                | vk::R
                | vk::F
                | vk::SPACE
        )
}

/// Whether the search is being typed after `event`.
fn typing_after(typing: bool, event: Event) -> bool {
    match event {
        Event::Search => !typing,
        Event::Command(_) => typing,
        Event::Hotkey { .. } | Event::Escape | Event::Clicked | Event::Deactivated => false,
    }
}

/// The virtual key behind a key that reached egui.
fn egui_vk(key: egui::Key) -> Option<u32> {
    use egui::Key;

    Some(match key {
        Key::W => vk::W,
        Key::ArrowUp => vk::UP,
        Key::S => vk::S,
        Key::ArrowDown => vk::DOWN,
        Key::A => vk::A,
        Key::ArrowLeft => vk::LEFT,
        Key::D => vk::D,
        Key::ArrowRight => vk::RIGHT,
        Key::Q => vk::Q,
        Key::E => vk::E,
        Key::Z => vk::Z,
        Key::C => vk::C,
        Key::Space => vk::SPACE,
        Key::Enter => vk::RETURN,
        Key::X => vk::X,
        Key::R => vk::R,
        Key::Escape => vk::ESCAPE,
        Key::F10 => vk::F10,
        Key::Tab => vk::TAB,
        Key::F => vk::F,
        // The letter, digit or F key the hotkey can be.
        _ => return OverlayHotkey::key_code(key.name()).map(u32::from),
    })
}

/// Reads the overlay's keys from egui, for capture runs and other platforms,
/// where no window procedure sees them first.  It takes the keys the window
/// would swallow out of egui's input, with the hotkey's keys standing in for
/// the hotkey.
#[derive(Default)]
pub(super) struct EguiKeys {
    typing: bool,
    /// Bits as in `KEYS`.  egui marks repeats only after this filter runs.
    held: u32,
    /// Keys whose press typed into the search.
    passed: u32,
    events: Vec<Event>,
}

impl EguiKeys {
    pub(super) fn filter(&mut self, input: &mut Vec<egui::Event>) {
        // egui reports a key's character right after its press.
        let mut took_press = false;
        input.retain(|event| match *event {
            egui::Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } => {
                let keep = self.key(key, pressed, modifiers);
                took_press = pressed && !keep;
                keep
            }
            _ => {
                let keep = !(took_press && matches!(event, egui::Event::Text(_)));
                took_press = false;
                keep
            }
        });
    }

    pub(super) fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Whether letters and Space type into the search.  The app has the
    /// final say, since it can ignore a key that asked to start typing.
    pub(super) fn set_typing(&mut self, typing: bool) {
        self.typing = typing;
    }

    /// Routes a key like the window procedure does and returns whether egui
    /// keeps it.
    fn key(&mut self, key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> bool {
        let Some(vk) = egui_vk(key) else {
            return true;
        };
        let Some((index, role)) = slot(vk) else {
            return true;
        };
        let mask: u32 = 1 << index;
        let passed = self.passed & mask != 0;

        if !pressed {
            self.held &= !mask;
            self.passed &= !mask;
            return passed;
        }
        if self.held & mask != 0 {
            if let Role::Navigate(command) = role
                && !passed
                && !(self.typing && types(vk))
            {
                self.push(Event::Command(command));
            }
            return passed;
        }

        self.held |= mask;
        let hotkey = HOTKEY_KEYS.get();
        if vk == hotkey_key()
            && (modifiers.ctrl, modifiers.alt, modifiers.shift)
                == (hotkey.ctrl, hotkey.alt, hotkey.shift)
        {
            self.push(Event::Hotkey { focused: true });
            return false;
        }
        if self.typing && types(vk) {
            self.passed |= mask;
            return true;
        }
        if let Some(event) = role.event(modifiers.shift) {
            self.push(event);
        }
        false
    }

    fn push(&mut self, event: Event) {
        self.typing = typing_after(self.typing, event);
        self.events.push(event);
    }
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
///
/// While a search is being typed, letters and Space go to the search field.
/// `passed` marks those presses, so their repeats and release go there too.
#[cfg(any(windows, test))]
#[derive(Debug, Default)]
struct Router {
    down: u32,
    ours: u32,
    passed: u32,
    typing: bool,
    next_repeat: [Option<Instant>; KEYS.len()],
    events: VecDeque<Event>,
}

#[cfg(any(windows, test))]
impl Router {
    /// Starts over from the physical key state.  Of `ours`, only keys that
    /// are still held are kept.
    fn reset(&mut self, held: u32, ours: u32) {
        self.down = held;
        self.ours = ours & held;
        self.passed = 0;
        self.typing = false;
        self.next_repeat = [None; KEYS.len()];
    }

    fn clear(&mut self) {
        self.reset(0, 0);
    }

    /// The hotkey swallowed its key's press while the overlay had the
    /// keyboard, so its repeats and release are the overlay's too.
    fn hotkey_pressed(&mut self) {
        self.down |= bit(HOTKEY);
        self.ours |= bit(HOTKEY);
        self.passed &= !bit(HOTKEY);
    }

    /// Whether a key that went down in the overlay is still `held`.  Handing
    /// the keyboard back now would send its repeats and release to the game.
    fn busy(&self, held: u32) -> bool {
        self.ours & held != 0
    }

    fn push(&mut self, event: Event) {
        self.typing = typing_after(self.typing, event);
        self.events.push_back(event);
    }

    /// Routes a key message and returns whether the window swallows it.
    fn key(&mut self, key: u32, pressed: bool, now: Instant) -> bool {
        let Some((index, role)) = slot(key) else {
            return false;
        };
        let mask: u32 = 1 << index;
        let swallow = !matches!(role, Role::Windows) && self.passed & mask == 0;

        if !pressed {
            self.down &= !mask;
            self.ours &= !mask;
            self.passed &= !mask;
            self.next_repeat[index] = None;
            return swallow;
        }

        let chord = self.down & WINDOWS_KEYS != 0;
        if self.down & mask != 0 {
            // An auto-repeat.  Only a navigation press that moved the
            // selection repeats, on its own schedule, and not while its key
            // types.
            if let Role::Navigate(command) = role
                && swallow
                && !chord
                && !(self.typing && types(key))
                && self.next_repeat[index].is_some_and(|at| now >= at)
            {
                self.next_repeat[index] = Some(now + REPEAT_INTERVAL);
                self.push(Event::Command(command));
            }
            return swallow;
        }

        self.down |= mask;
        if !matches!(role, Role::Windows) {
            self.ours |= mask;
        }
        if chord {
            return swallow;
        }
        if self.typing && types(key) {
            self.passed |= mask;
            return false;
        }
        if let Role::Navigate(_) = role {
            self.next_repeat[index] = Some(now + REPEAT_DELAY);
        }
        if let Some(event) = role.event(self.down & SHIFT_KEYS != 0) {
            self.push(event);
        }
        swallow
    }
}

/// A change to the overlay's title.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Title {
    /// Take the title XXMI takes keys from.
    Lend,
    /// Take the overlay's own back.
    GiveBack,
}

/// Keeps the title XXMI takes keys from on the overlay while keys the overlay
/// doesn't use are down, so the game gets them.  Not while a search is typed
/// or one of the overlay's own keys is down, since the game would get those
/// too.  A key that went down then holds the title back until it's up, and so
/// does a key that was down before the overlay had the keyboard.
///
/// The title stays a moment after the last key is up, for XXMI to see it go
/// up: XXMI only acts on a key that goes down after it saw it up.
#[cfg(any(windows, test))]
#[derive(Debug, Default)]
struct Lender {
    /// Down, and the game sees them.
    lent: Vec<u32>,
    /// Down, and the game mustn't see them.
    held_back: Vec<u32>,
    /// Whether the overlay has the title.
    titled: bool,
    /// When the last lent key went up, while the overlay still has the title.
    emptied: Option<Instant>,
}

#[cfg(any(windows, test))]
impl Lender {
    fn titled(&self) -> bool {
        self.titled
    }

    /// A key the overlay doesn't use went down.  `blocked` while a search is
    /// typed or one of the overlay's keys is down.
    fn press(&mut self, key: u32, blocked: bool) -> Option<Title> {
        if self.lent.contains(&key) || self.held_back.contains(&key) {
            // A repeat.
            return None;
        }
        if blocked || !self.held_back.is_empty() {
            self.held_back.push(key);
            return None;
        }
        self.lent.push(key);
        self.emptied = None;
        (!std::mem::replace(&mut self.titled, true)).then_some(Title::Lend)
    }

    fn release(&mut self, key: u32, now: Instant) {
        self.held_back.retain(|&held| held != key);
        self.lent.retain(|&lent| lent != key);
        self.note_emptied(now);
    }

    /// One of the overlay's own keys went down.  The keys lent so far stay
    /// held back until they're up.
    fn stop(&mut self) -> Option<Title> {
        self.held_back.append(&mut self.lent);
        self.give_back()
    }

    /// Starts over with `held` down, none of them lent.
    fn reset(&mut self, held: Vec<u32>) -> Option<Title> {
        self.lent.clear();
        self.held_back = held;
        self.give_back()
    }

    /// Forgets the keys that are up, for a release that never arrived, and
    /// gives the title back once the last lent key has been up a moment.
    fn tick(&mut self, down: impl Fn(u32) -> bool, now: Instant) -> Option<Title> {
        self.held_back.retain(|&held| down(held));
        self.lent.retain(|&lent| down(lent));
        self.note_emptied(now);
        if self
            .emptied
            .is_some_and(|emptied| now.saturating_duration_since(emptied) >= TITLE_LINGER)
        {
            return self.give_back();
        }
        None
    }

    fn note_emptied(&mut self, now: Instant) {
        if self.titled && self.lent.is_empty() && self.emptied.is_none() {
            self.emptied = Some(now);
        }
    }

    fn give_back(&mut self) -> Option<Title> {
        self.emptied = None;
        std::mem::take(&mut self.titled).then_some(Title::GiveBack)
    }
}

#[cfg(windows)]
struct Shared {
    ctx: Context,
    router: Router,
    lender: Lender,
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
fn notify<T>(update: impl FnOnce(&mut Router) -> T) -> Option<T> {
    notify_shared(|shared| update(&mut shared.router))
}

/// Updates the router and the lender, then asks for a frame once the borrow
/// has ended.
#[cfg(windows)]
fn notify_shared<T>(update: impl FnOnce(&mut Shared) -> T) -> Option<T> {
    let (result, ctx) = with_shared(|shared| (update(shared), shared.ctx.clone()))?;
    ctx.request_repaint();
    Some(result)
}

/// Changes the overlay's title, after the borrow of the shared state has
/// ended: the change goes through the window procedure.
#[cfg(windows)]
fn apply_title(title: Option<Title>) {
    if let Some(title) = title {
        super::platform::lend_title(title == Title::Lend);
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
        .filter(|&(_, &(key, _))| physically_down(real_key(key)))
        .fold(0, |held, (index, _)| held | (1 << index))
}

/// The keys the overlay doesn't use that are down, but Shift, Ctrl and Alt.
#[cfg(windows)]
fn untracked_keys_down() -> Vec<u32> {
    (0x08..=0xFE)
        .filter(|&key| {
            !matches!(key, vk::SHIFT | vk::CONTROL | vk::MENU)
                && slot(key).is_none()
                && physically_down(key)
        })
        .collect()
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
    if !installed() {
        return false;
    }
    let Some((index, _)) = slot(message.key) else {
        // The game gets it while it's down.
        let now = Instant::now();
        let title = with_shared(|shared| {
            if message.pressed {
                let router = &shared.router;
                let blocked = router.typing || router.down & !MODIFIER_KEYS != 0;
                shared.lender.press(message.key, blocked)
            } else {
                // The title goes back on a later frame.
                shared.lender.release(message.key, now);
                None
            }
        })
        .flatten();
        apply_title(title);
        return false;
    };
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
    // The game mustn't get the overlay's own keys.
    let own_press = message.pressed && (1_u32 << index) & MODIFIER_KEYS == 0;
    let (swallow, title) = notify_shared(|shared| {
        let swallow = shared.router.key(message.key, message.pressed, now);
        if let Some(key) = other_shift {
            shared.router.key(key, false, now);
        }
        (swallow, own_press.then(|| shared.lender.stop()).flatten())
    })
    .unwrap_or((consumed, None));
    apply_title(title);
    swallow
}

/// The hotkey arrived.  `took_focus` is true when the overlay has just taken
/// the keyboard from another window.
#[cfg(windows)]
pub(super) fn hotkey(focused: bool, took_focus: bool) {
    let held = if took_focus { physical_keys() } else { 0 };
    let others = if took_focus {
        untracked_keys_down()
    } else {
        Vec::new()
    };
    let title = notify_shared(|shared| {
        let router = &mut shared.router;
        if took_focus {
            router.reset(held, bit(HOTKEY));
        } else if focused {
            router.hotkey_pressed();
        }
        router.push(Event::Hotkey { focused });
        // The hotkey is one of the overlay's keys.
        if took_focus {
            shared.lender.reset(others)
        } else {
            shared.lender.stop()
        }
    })
    .flatten();
    apply_title(title);
}

/// Asks for a frame, for news from the window procedure that isn't a key.
#[cfg(windows)]
pub(super) fn wake() {
    notify(|_| ());
}

#[cfg(windows)]
pub(super) fn activated() {
    if installed() {
        let held = physical_keys();
        let others = untracked_keys_down();
        let title = notify_shared(|shared| {
            shared.router.reset(held, 0);
            shared.lender.reset(others)
        })
        .flatten();
        apply_title(title);
    }
}

/// A click took the keyboard.  Activation has already reset the keys.
#[cfg(windows)]
pub(super) fn clicked() {
    notify(|router| router.push(Event::Clicked));
}

#[cfg(windows)]
pub(super) fn deactivated() {
    notify_shared(|shared| {
        shared.router.clear();
        shared.router.push(Event::Deactivated);
        shared.lender.reset(Vec::new());
    });
    // Whatever the lender knew, a window in the back keeps its own title.
    apply_title(Some(Title::GiveBack));
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
                lender: Lender::default(),
            });
        });
        #[cfg(not(windows))]
        let _ = ctx;
        Self { _private: () }
    }

    pub(super) fn drain(&self) -> Vec<Event> {
        #[cfg(windows)]
        {
            let now = Instant::now();
            let Some((events, title, titled)) = with_shared(|shared| {
                let title = shared.lender.tick(physically_down, now);
                let titled = shared.lender.titled().then(|| shared.ctx.clone());
                (shared.router.events.drain(..).collect(), title, titled)
            }) else {
                return Vec::new();
            };
            apply_title(title);
            if let Some(ctx) = titled {
                // Look again soon, to give the title back.
                ctx.request_repaint_after(TITLE_CHECK);
            }
            events
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }

    /// Whether letters and Space type into the search.  The app has the final
    /// say, since it can ignore a key that asked to start typing.
    pub(super) fn set_typing(&self, typing: bool) {
        #[cfg(windows)]
        with_shared(|shared| shared.router.typing = typing);
        #[cfg(not(windows))]
        let _ = typing;
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
        apply_title(Some(Title::GiveBack));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: u32 = u32::MAX;
    /// The B key, which the overlay does not use.
    const UNUSED: u32 = 0x42;

    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    fn drain(router: &mut Router) -> Vec<Event> {
        router.events.drain(..).collect()
    }

    fn command(command: Command) -> Event {
        Event::Command(command)
    }

    fn press(router: &mut Router, key: u32, at: Instant) {
        router.key(key, true, at);
        router.key(key, false, at);
    }

    #[test]
    fn row_and_move_keys_map_to_commands() {
        let now = Instant::now();
        let mut router = Router::default();
        for key in [vk::W, vk::UP, vk::S, vk::DOWN] {
            assert!(consumes(key));
            press(&mut router, key, now);
        }
        for key in [vk::A, vk::LEFT, vk::D, vk::RIGHT] {
            assert!(consumes(key));
            press(&mut router, key, now);
        }
        assert_eq!(
            drain(&mut router),
            vec![
                command(Command::Row(-1)),
                command(Command::Row(-1)),
                command(Command::Row(1)),
                command(Command::Row(1)),
                command(Command::Move(-1)),
                command(Command::Move(-1)),
                command(Command::Move(1)),
                command(Command::Move(1)),
            ]
        );
    }

    #[test]
    fn mod_and_category_keys_map_to_commands() {
        let now = Instant::now();
        let mut router = Router::default();
        for key in [vk::Q, vk::E, vk::Z, vk::C] {
            assert!(consumes(key));
            press(&mut router, key, now);
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
    fn space_and_enter_enable_or_toggle_with_shift() {
        let now = Instant::now();
        for key in [vk::SPACE, vk::RETURN] {
            let mut router = Router::default();
            press(&mut router, key, now);
            assert_eq!(drain(&mut router), vec![command(Command::Exclusive)]);
            for shift in [vk::LSHIFT, vk::RSHIFT] {
                router.key(shift, true, now);
                press(&mut router, key, now);
                router.key(shift, false, now);
                assert_eq!(drain(&mut router), vec![command(Command::Toggle)]);
            }
            // Ctrl and Alt do not change what Space and Enter do.
            for modifier in [vk::LCONTROL, vk::LMENU] {
                router.key(modifier, true, now);
                press(&mut router, key, now);
                router.key(modifier, false, now);
                assert_eq!(drain(&mut router), vec![command(Command::Exclusive)]);
            }
        }
    }

    #[test]
    fn other_keys_pass_through_without_commands() {
        let now = Instant::now();
        let mut router = Router::default();
        // B, F4, and the Windows keys.
        for key in [UNUSED, 0x73, vk::LWIN, vk::RWIN] {
            assert!(!consumes(key));
            assert!(!router.key(key, true, now));
            assert!(!router.key(key, false, now));
        }
        assert!(drain(&mut router).is_empty());
        assert!(!router.busy(ALL));
    }

    #[test]
    fn swallowed_keys_have_no_commands_but_hold_up_focus_return() {
        let now = Instant::now();
        let keys = [
            vk::LSHIFT,
            vk::RSHIFT,
            vk::LCONTROL,
            vk::RCONTROL,
            vk::LMENU,
            vk::RMENU,
            vk::H,
            vk::F10,
        ];
        let mut router = Router::default();
        for key in keys {
            assert!(consumes(key));
            assert!(router.key(key, true, now));
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
            (vk::W, Command::Row(-1)),
            (vk::DOWN, Command::Row(1)),
            (vk::SPACE, Command::Exclusive),
            (vk::RETURN, Command::Exclusive),
            (vk::X, Command::Toggle),
            (vk::R, Command::Flip),
        ] {
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
        router.reset(bit(vk::A) | bit(vk::SPACE), 0);
        router.key(vk::A, true, after(now, 400));
        router.key(vk::A, true, after(now, 800));
        router.key(vk::SPACE, true, after(now, 800));
        assert!(drain(&mut router).is_empty());
        router.key(vk::A, false, after(now, 900));
        router.key(vk::A, true, after(now, 1000));
        assert_eq!(drain(&mut router), vec![command(Command::Move(-1))]);
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
        assert_eq!(drain(&mut router), vec![command(Command::Move(1)); 3]);
        router.key(vk::D, false, after(start, 1000));
        router.key(vk::D, true, after(start, 1001));
        router.key(vk::D, true, after(start, 1002));
        assert_eq!(drain(&mut router), vec![command(Command::Move(1))]);
    }

    #[test]
    fn held_navigation_keys_repeat_independently() {
        let start = Instant::now();
        let mut router = Router::default();
        router.key(vk::C, true, start);
        router.key(vk::RIGHT, true, after(start, 100));
        router.key(vk::C, true, start + REPEAT_DELAY);
        router.key(vk::RIGHT, true, start + REPEAT_DELAY);
        router.key(vk::RIGHT, true, after(start, 100) + REPEAT_DELAY);
        assert_eq!(
            drain(&mut router),
            vec![
                command(Command::Category(1)),
                command(Command::Move(1)),
                command(Command::Category(1)),
                command(Command::Move(1)),
            ]
        );
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

        router.reset(bit(vk::LMENU) | bit(HOTKEY), bit(HOTKEY));
        assert!(router.busy(ALL));
        router.reset(bit(vk::LMENU), bit(HOTKEY));
        assert!(!router.busy(ALL));
    }

    #[test]
    fn clear_forgets_held_keys() {
        let now = Instant::now();
        let mut router = Router::default();
        router.key(vk::LMENU, true, now);
        router.key(vk::D, true, now);
        router.clear();
        assert!(!router.busy(ALL));
        assert_eq!(drain(&mut router), vec![command(Command::Move(1))]);
        // Nothing is held any more, so the next message for D is a new press.
        router.key(vk::D, true, after(now, 50));
        assert_eq!(drain(&mut router), vec![command(Command::Move(1))]);
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

    #[test]
    fn tab_and_f_start_a_search_and_only_tab_ends_it() {
        let now = Instant::now();
        for key in [vk::TAB, vk::F] {
            let mut router = Router::default();
            assert!(consumes(key));
            assert!(router.key(key, true, now));
            assert!(router.key(key, false, now));
            assert_eq!(drain(&mut router), vec![Event::Search]);
            assert!(router.typing);
            // F types while searching.
            assert!(!router.key(vk::F, true, now));
            assert!(!router.key(vk::F, false, now));
            assert!(drain(&mut router).is_empty());
            assert!(router.typing);
            press(&mut router, vk::TAB, now);
            assert_eq!(drain(&mut router), vec![Event::Search]);
            assert!(!router.typing);
        }
        // Ctrl+F starts it too.
        let mut router = Router::default();
        router.key(vk::LCONTROL, true, now);
        press(&mut router, vk::F, now);
        assert_eq!(drain(&mut router), vec![Event::Search]);
    }

    #[test]
    fn typing_passes_letters_and_space_to_the_search() {
        let now = Instant::now();
        let mut router = Router::default();
        press(&mut router, vk::F, now);
        drain(&mut router);
        for key in [
            vk::W,
            vk::A,
            vk::S,
            vk::D,
            vk::Q,
            vk::E,
            vk::Z,
            vk::C,
            vk::X,
            vk::H,
            vk::F,
            vk::SPACE,
        ] {
            assert!(!router.key(key, true, now), "{key:#x}");
            assert!(!router.key(key, true, after(now, 500)), "{key:#x}");
            // A key that typed holds up handing the keyboard back like any
            // other key pressed in the overlay.
            assert!(router.busy(ALL));
            assert!(!router.key(key, false, after(now, 600)), "{key:#x}");
        }
        assert!(drain(&mut router).is_empty());
        assert!(router.typing);
        assert!(!router.busy(ALL));
    }

    #[test]
    fn typing_keeps_arrows_enter_escape_and_tab() {
        let now = Instant::now();
        let mut router = Router {
            typing: true,
            ..Router::default()
        };
        for key in [vk::UP, vk::DOWN, vk::LEFT, vk::RIGHT, vk::RETURN] {
            assert!(router.key(key, true, now), "{key:#x}");
            assert!(router.key(key, false, now), "{key:#x}");
        }
        router.key(vk::LSHIFT, true, now);
        press(&mut router, vk::RETURN, now);
        router.key(vk::LSHIFT, false, now);
        assert_eq!(
            drain(&mut router),
            vec![
                command(Command::Row(-1)),
                command(Command::Row(1)),
                command(Command::Move(-1)),
                command(Command::Move(1)),
                command(Command::Exclusive),
                command(Command::Toggle),
            ]
        );
        assert!(router.typing);
        // Held arrows still repeat.
        router.key(vk::RIGHT, true, now);
        router.key(vk::RIGHT, true, now + REPEAT_DELAY);
        router.key(vk::RIGHT, false, now + REPEAT_DELAY);
        assert_eq!(drain(&mut router), vec![command(Command::Move(1)); 2]);

        assert!(router.key(vk::ESCAPE, true, now));
        router.key(vk::ESCAPE, false, now);
        assert_eq!(drain(&mut router), vec![Event::Escape]);
        assert!(!router.typing);
    }

    #[test]
    fn a_key_that_typed_is_released_to_the_search() {
        let now = Instant::now();
        let mut router = Router {
            typing: true,
            ..Router::default()
        };
        assert!(!router.key(vk::D, true, now));
        press(&mut router, vk::TAB, now);
        assert_eq!(drain(&mut router), vec![Event::Search]);
        assert!(!router.typing);
        // D went down in the search, so it stays there until released.
        assert!(!router.key(vk::D, true, now + REPEAT_DELAY));
        assert!(!router.key(vk::D, false, after(now, 400)));
        assert!(drain(&mut router).is_empty());
        // Its next press is a command again.
        assert!(router.key(vk::D, true, after(now, 500)));
        assert_eq!(drain(&mut router), vec![command(Command::Move(1))]);
    }

    #[test]
    fn a_key_held_before_typing_started_stays_quiet() {
        let now = Instant::now();
        let mut router = Router::default();
        router.key(vk::A, true, now);
        press(&mut router, vk::F, after(now, 100));
        assert!(router.key(vk::A, true, now + REPEAT_DELAY));
        assert!(router.key(vk::A, false, after(now, 400)));
        assert_eq!(
            drain(&mut router),
            vec![command(Command::Move(-1)), Event::Search]
        );
    }

    #[test]
    fn only_search_keys_start_typing_and_session_events_end_it() {
        for typing in [false, true] {
            assert_eq!(typing_after(typing, Event::Search), !typing);
            assert_eq!(typing_after(typing, command(Command::Exclusive)), typing);
            for event in [
                Event::Escape,
                Event::Hotkey { focused: true },
                Event::Deactivated,
            ] {
                assert!(!typing_after(typing, event), "{event:?}");
            }
        }
        // Taking or losing the keyboard starts over.
        let mut router = Router {
            typing: true,
            ..Router::default()
        };
        router.reset(0, 0);
        assert!(!router.typing);
    }

    fn egui_key(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        }
    }

    /// Presses and releases `key` through `keys`.  Returns the events, and
    /// whether egui still sees the key.
    fn tap(keys: &mut EguiKeys, key: egui::Key, modifiers: egui::Modifiers) -> (Vec<Event>, bool) {
        let mut input = vec![
            egui_key(key, true, modifiers),
            egui_key(key, false, modifiers),
        ];
        keys.filter(&mut input);
        let kept = match input.len() {
            0 => false,
            2 => true,
            _ => panic!("{key:?} lost only its press or its release"),
        };
        (keys.drain(), kept)
    }

    #[test]
    fn egui_keys_map_like_window_keys() {
        use egui::{Key, Modifiers};

        let none = Modifiers::NONE;
        for (key, modifiers, expected) in [
            (Key::W, none, Some(command(Command::Row(-1)))),
            (Key::ArrowUp, none, Some(command(Command::Row(-1)))),
            (Key::S, none, Some(command(Command::Row(1)))),
            (Key::ArrowDown, none, Some(command(Command::Row(1)))),
            (Key::A, none, Some(command(Command::Move(-1)))),
            (Key::ArrowLeft, none, Some(command(Command::Move(-1)))),
            (Key::D, none, Some(command(Command::Move(1)))),
            (Key::ArrowRight, none, Some(command(Command::Move(1)))),
            (Key::Q, none, Some(command(Command::Mod(-1)))),
            (Key::E, none, Some(command(Command::Mod(1)))),
            (Key::Z, none, Some(command(Command::Category(-1)))),
            (Key::C, none, Some(command(Command::Category(1)))),
            (Key::Space, none, Some(command(Command::Exclusive))),
            (Key::Enter, none, Some(command(Command::Exclusive))),
            (Key::Space, Modifiers::SHIFT, Some(command(Command::Toggle))),
            (Key::Enter, Modifiers::SHIFT, Some(command(Command::Toggle))),
            (Key::X, none, Some(command(Command::Toggle))),
            (Key::R, none, Some(command(Command::Flip))),
            (Key::Tab, none, Some(Event::Search)),
            (Key::F, none, Some(Event::Search)),
            (Key::F, Modifiers::CTRL, Some(Event::Search)),
            (Key::Escape, none, Some(Event::Escape)),
            (
                Key::H,
                Modifiers::ALT,
                Some(Event::Hotkey { focused: true }),
            ),
            (Key::H, none, None),
            (Key::F10, none, None),
        ] {
            let mut keys = EguiKeys::default();
            assert_eq!(
                tap(&mut keys, key, modifiers),
                (expected.into_iter().collect(), false),
                "{key:?} {modifiers:?}"
            );
        }
        for key in [Key::B, Key::F4] {
            let mut keys = EguiKeys::default();
            assert_eq!(tap(&mut keys, key, none), (Vec::new(), true), "{key:?}");
        }
    }

    #[test]
    fn only_egui_navigation_keys_repeat() {
        use egui::{Key, Modifiers};

        let none = Modifiers::NONE;
        for (key, repeats) in [
            (Key::D, true),
            (Key::Q, true),
            (Key::W, false),
            (Key::Space, false),
            (Key::Enter, false),
            (Key::X, false),
            (Key::R, false),
            (Key::Tab, false),
            (Key::Escape, false),
        ] {
            let mut keys = EguiKeys::default();
            // egui-winit reports a repeat as another press, and egui marks
            // it as a repeat only after the filter.
            let mut input = vec![
                egui_key(key, true, none),
                egui_key(key, true, none),
                egui_key(key, false, none),
            ];
            keys.filter(&mut input);
            assert!(input.is_empty(), "{key:?}");
            assert_eq!(keys.drain().len(), if repeats { 2 } else { 1 }, "{key:?}");
        }
        let mut keys = EguiKeys::default();
        let mut input = vec![
            egui_key(Key::H, true, Modifiers::ALT),
            egui_key(Key::H, true, Modifiers::ALT),
        ];
        keys.filter(&mut input);
        assert_eq!(keys.drain(), vec![Event::Hotkey { focused: true }]);
    }

    #[test]
    fn egui_typing_keeps_letters_and_space_for_the_search() {
        use egui::{Key, Modifiers};

        let none = Modifiers::NONE;
        let mut keys = EguiKeys::default();
        assert_eq!(tap(&mut keys, Key::F, none), (vec![Event::Search], false));
        for key in [
            Key::W,
            Key::A,
            Key::S,
            Key::D,
            Key::Q,
            Key::E,
            Key::Z,
            Key::C,
            Key::X,
            Key::H,
            Key::F,
            Key::Space,
        ] {
            assert_eq!(tap(&mut keys, key, none), (Vec::new(), true), "{key:?}");
        }
        for (key, modifiers, expected) in [
            (Key::ArrowDown, none, command(Command::Row(1))),
            (Key::Enter, none, command(Command::Exclusive)),
            (Key::Enter, Modifiers::SHIFT, command(Command::Toggle)),
            (Key::Tab, none, Event::Search),
            (Key::W, none, command(Command::Row(-1))),
        ] {
            assert_eq!(
                tap(&mut keys, key, modifiers),
                (vec![expected], false),
                "{key:?} {modifiers:?}"
            );
        }
        // Esc and Alt+H end typing.
        for (key, modifiers, event) in [
            (Key::Escape, none, Event::Escape),
            (Key::H, Modifiers::ALT, Event::Hotkey { focused: true }),
        ] {
            keys.set_typing(true);
            assert_eq!(tap(&mut keys, key, modifiers), (vec![event], false));
            assert_eq!(
                tap(&mut keys, Key::S, none),
                (vec![command(Command::Row(1))], false)
            );
        }
    }

    #[test]
    fn egui_characters_follow_their_keys() {
        use egui::{Key, Modifiers};

        let none = Modifiers::NONE;
        let text = |text: &str| egui::Event::Text(text.into());
        let mut keys = EguiKeys::default();
        let mut input = vec![
            egui_key(Key::F, true, none),
            text("f"),
            egui_key(Key::F, false, none),
            egui_key(Key::V, true, none),
            text("v"),
            egui_key(Key::V, false, none),
            egui_key(Key::F, true, none),
            text("f"),
            egui_key(Key::Tab, true, none),
            egui_key(Key::Space, true, none),
            text(" "),
            egui_key(Key::Tab, false, none),
            // Text that follows no key, such as a finished IME word, stays.
            text("語"),
        ];
        keys.filter(&mut input);
        assert_eq!(
            input,
            vec![
                egui_key(Key::V, true, none),
                text("v"),
                egui_key(Key::V, false, none),
                egui_key(Key::F, true, none),
                text("f"),
                text("語"),
            ]
        );
        assert_eq!(
            keys.drain(),
            vec![Event::Search, Event::Search, command(Command::Exclusive)]
        );
    }

    #[test]
    fn keys_the_overlay_does_not_use_lend_the_title_while_down() {
        const K: u32 = 0x4B;
        let start = Instant::now();
        let later = |millis| start + Duration::from_millis(millis);
        let down = |_| true;
        let mut lender = Lender::default();
        assert_eq!(lender.press(UNUSED, false), Some(Title::Lend));
        // A repeat, and a second key, keep it.
        assert_eq!(lender.press(UNUSED, false), None);
        assert_eq!(lender.press(K, false), None);
        lender.release(UNUSED, later(10));
        assert_eq!(lender.tick(down, later(500)), None);
        lender.release(K, later(500));
        // XXMI gets a moment to see the key go up.
        assert_eq!(lender.tick(down, later(600)), None);
        assert!(lender.titled());
        assert_eq!(lender.tick(down, later(700)), Some(Title::GiveBack));
        assert_eq!(lender.tick(down, later(800)), None);
        assert!(!lender.titled());
    }

    #[test]
    fn a_key_in_the_moment_after_keeps_the_title() {
        let start = Instant::now();
        let later = |millis| start + Duration::from_millis(millis);
        let down = |_| true;
        let mut lender = Lender::default();
        lender.press(UNUSED, false);
        lender.release(UNUSED, later(10));
        assert_eq!(lender.press(UNUSED, false), None);
        assert_eq!(lender.tick(down, later(300)), None);
        lender.release(UNUSED, later(300));
        assert_eq!(lender.tick(down, later(500)), Some(Title::GiveBack));
    }

    #[test]
    fn a_key_pressed_while_blocked_holds_the_title_back_until_up() {
        const K: u32 = 0x4B;
        let now = Instant::now();
        let mut lender = Lender::default();
        assert_eq!(lender.press(UNUSED, true), None);
        // The search ended or the overlay's key is up, but the game would
        // see the first key go down now.
        assert_eq!(lender.press(K, false), None);
        lender.release(UNUSED, now);
        lender.release(K, now);
        assert!(!lender.titled());
        assert_eq!(lender.press(K, false), Some(Title::Lend));
    }

    #[test]
    fn the_overlays_own_key_takes_the_title_back_at_once() {
        const K: u32 = 0x4B;
        let now = Instant::now();
        let mut lender = Lender::default();
        lender.press(UNUSED, false);
        assert_eq!(lender.stop(), Some(Title::GiveBack));
        assert_eq!(lender.stop(), None);
        // Down when the overlay's key went down, so held back.
        assert_eq!(lender.press(K, false), None);
        lender.release(UNUSED, now);
        lender.release(K, now);
        assert_eq!(lender.press(K, false), Some(Title::Lend));
        // Also in the moment after the last key went up.
        lender.release(K, now);
        assert_eq!(lender.stop(), Some(Title::GiveBack));
    }

    #[test]
    fn keys_down_before_the_keyboard_came_hold_the_title_back() {
        const K: u32 = 0x4B;
        let start = Instant::now();
        let later = |millis| start + Duration::from_millis(millis);
        let mut lender = Lender::default();
        lender.press(UNUSED, false);
        assert_eq!(lender.reset(vec![K]), Some(Title::GiveBack));
        assert_eq!(lender.press(UNUSED, false), None);
        // Up without a release reaching the overlay.
        assert_eq!(lender.tick(|key| key == UNUSED, later(0)), None);
        assert_eq!(lender.held_back, vec![UNUSED]);
        assert_eq!(lender.tick(|_| false, later(0)), None);
        assert_eq!(lender.press(K, false), Some(Title::Lend));
        assert_eq!(lender.tick(|_| false, later(10)), None);
        assert_eq!(lender.tick(|_| false, later(300)), Some(Title::GiveBack));
    }

    #[test]
    fn modifiers_do_not_block_lending() {
        let mut router = Router::default();
        let now = Instant::now();
        router.key(vk::LCONTROL, true, now);
        router.key(vk::RSHIFT, true, now);
        assert_eq!(router.down & !MODIFIER_KEYS, 0);
        router.key(vk::W, true, now);
        assert_ne!(router.down & !MODIFIER_KEYS, 0);
        router.key(vk::W, false, now);
        router.key(vk::LWIN, true, now);
        assert_ne!(router.down & !MODIFIER_KEYS, 0);
    }

    #[cfg(windows)]
    #[test]
    fn window_messages_reach_the_installed_keyboard() {
        let message = |key, pressed| KeyMessage { key, pressed };
        assert!(!installed());
        let keyboard = Keyboard::new(Context::default());
        assert!(installed());

        assert!(key_message(message(vk::Q, true), || None));
        assert!(!key_message(message(UNUSED, true), || None));
        assert!(!key_message(message(vk::LWIN, true), || None));
        assert_eq!(keyboard.drain(), vec![command(Command::Mod(-1))]);

        // AltGr's fake Ctrl is swallowed without reaching the router.
        let right_alt = || Some(message(vk::RMENU, true));
        assert!(key_message(message(vk::LCONTROL, true), right_alt));
        assert!(keyboard.drain().is_empty());

        hotkey(true, false);
        deactivated();
        assert_eq!(
            keyboard.drain(),
            vec![Event::Hotkey { focused: true }, Event::Deactivated]
        );

        // F starts a search, and letters then type into it until the app
        // says otherwise.
        assert!(key_message(message(vk::F, true), || None));
        assert!(key_message(message(vk::F, false), || None));
        assert_eq!(keyboard.drain(), vec![Event::Search]);
        assert!(!key_message(message(vk::Q, true), || None));
        assert!(!key_message(message(vk::Q, false), || None));
        keyboard.set_typing(false);
        assert!(key_message(message(vk::Q, true), || None));
        assert_eq!(keyboard.drain(), vec![command(Command::Mod(-1))]);

        drop(keyboard);
        assert!(!installed());
        assert!(!key_message(message(vk::Q, true), || None));
    }
}
