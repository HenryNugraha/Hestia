//! Keyboard navigation for the native overlay preview.
//!
//! Windows uses a low-level keyboard hook because the transparent, no-activate
//! overlay deliberately does not take focus from the game.  The hook only
//! consumes the four navigation keys and Space when Alt is held.  Alt itself
//! and every other key continue through the normal Windows input path.

#[cfg(any(windows, test))]
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::sync::Mutex;

use egui::Context;

#[cfg(any(windows, test))]
const VK_A: u32 = 0x41;
#[cfg(any(windows, test))]
const VK_D: u32 = 0x44;
#[cfg(any(windows, test))]
const VK_S: u32 = 0x53;
#[cfg(any(windows, test))]
const VK_W: u32 = 0x57;
#[cfg(any(windows, test))]
const VK_SPACE: u32 = 0x20;
#[cfg(any(windows, test))]
const VK_LMENU: u32 = 0xA4;
#[cfg(any(windows, test))]
const VK_RMENU: u32 = 0xA5;
#[cfg(any(windows, test))]
const VK_MENU: u32 = 0x12;

#[cfg(any(windows, test))]
const REPEAT_DELAY: Duration = Duration::from_millis(300);
#[cfg(any(windows, test))]
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    Mod(i32),
    Category(i32),
    Activate,
}

/// Physical state machine shared by the Windows hook and its tests.
///
/// `down` records keys that were already held before Alt was pressed.  Such a
/// key must not become a captured navigation key halfway through its physical
/// press; in particular, its eventual key-up must still reach the game.
#[cfg(any(windows, test))]
#[derive(Debug)]
struct Router {
    alt_keys: u8,
    down: u8,
    captured: u8,
    next_repeat: [Option<Instant>; 5],
    commands: VecDeque<Command>,
}

#[cfg(any(windows, test))]
impl Router {
    fn new(alt_keys: u8, down: u8) -> Self {
        Self {
            alt_keys,
            down,
            captured: 0,
            next_repeat: [None; 5],
            commands: VecDeque::new(),
        }
    }

    fn alt_active(&self, alt_context: bool) -> bool {
        self.alt_keys != 0 || alt_context
    }

    /// Routes one physical keyboard event and returns whether it is consumed.
    /// `blocked_modifier` means Ctrl or either Windows key is down.
    fn event(
        &mut self,
        vk: u32,
        pressed: bool,
        blocked_modifier: bool,
        alt_context: bool,
        now: Instant,
    ) -> bool {
        if let Some(alt_bit) = alt_bit(vk) {
            if pressed {
                self.alt_keys |= alt_bit;
            } else {
                self.alt_keys &= !alt_bit;
            }
            return false;
        }

        let Some(index) = navigation_index(vk) else {
            return false;
        };
        let bit = 1 << index;

        if !pressed {
            let was_captured = self.captured & bit != 0;
            self.down &= !bit;
            self.captured &= !bit;
            self.next_repeat[index] = None;
            // This is the important key-up half of Alt release handling: a
            // captured key remains swallowed until its own physical release.
            return was_captured;
        }

        let was_down = self.down & bit != 0;
        self.down |= bit;

        if self.captured & bit != 0 {
            if vk != VK_SPACE
                && self.alt_active(alt_context)
                && !blocked_modifier
                && self.next_repeat[index].is_some_and(|next| now >= next)
            {
                self.commands.push_back(command_for(vk));
                self.next_repeat[index] = Some(now + REPEAT_INTERVAL);
            }
            // Once captured, repeat and release events must remain swallowed,
            // even after Alt itself is released, so the game cannot receive a
            // mismatched key-up/down sequence.
            return true;
        }

        // A key that was already down before Alt was pressed belongs to the
        // game.  Do not steal it when its auto-repeat happens under Alt.
        if was_down || !self.alt_active(alt_context) || blocked_modifier {
            return false;
        }

        self.captured |= bit;
        if vk == VK_SPACE {
            self.commands.push_back(Command::Activate);
            self.next_repeat[index] = None;
        } else {
            self.commands.push_back(command_for(vk));
            self.next_repeat[index] = Some(now + REPEAT_DELAY);
        }
        true
    }

    fn take_commands(&mut self) -> Vec<Command> {
        self.commands.drain(..).collect()
    }
}

#[cfg(any(windows, test))]
fn navigation_index(vk: u32) -> Option<usize> {
    match vk {
        VK_A => Some(0),
        VK_D => Some(1),
        VK_W => Some(2),
        VK_S => Some(3),
        VK_SPACE => Some(4),
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn alt_bit(vk: u32) -> Option<u8> {
    match vk {
        VK_LMENU => Some(1),
        VK_RMENU => Some(2),
        VK_MENU => Some(3),
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn command_for(vk: u32) -> Command {
    match vk {
        VK_A => Command::Mod(-1),
        VK_D => Command::Mod(1),
        VK_W => Command::Category(-1),
        VK_S => Command::Category(1),
        _ => unreachable!("command requested for a non-navigation key"),
    }
}

pub(super) struct Monitor {
    #[cfg(windows)]
    state: std::sync::Arc<HookState>,
    #[cfg(windows)]
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Monitor {
    pub(super) fn new(ctx: Context) -> std::io::Result<Self> {
        #[cfg(windows)]
        {
            return windows_monitor(ctx);
        }
        #[cfg(not(windows))]
        {
            let _ = ctx;
            Ok(Self {})
        }
    }

    pub(super) fn held(&self, ctx: &Context) -> bool {
        #[cfg(windows)]
        {
            let _ = ctx;
            self.state
                .alt_held
                .load(std::sync::atomic::Ordering::Acquire)
        }
        #[cfg(not(windows))]
        {
            ctx.input(|input| input.modifiers.alt)
        }
    }

    pub(super) fn drain(&self) -> Vec<Command> {
        #[cfg(windows)]
        {
            return self
                .state
                .router
                .lock()
                .map_or_else(|_| Vec::new(), |mut router| router.take_commands());
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
}

#[cfg(windows)]
struct HookState {
    ctx: Context,
    router: Mutex<Router>,
    alt_held: std::sync::atomic::AtomicBool,
    stop: std::sync::atomic::AtomicBool,
    thread_id: std::sync::atomic::AtomicU32,
}

#[cfg(windows)]
impl HookState {
    fn new(ctx: Context) -> Self {
        Self {
            ctx,
            router: Mutex::new(Router::new(0, 0)),
            alt_held: std::sync::atomic::AtomicBool::new(false),
            stop: std::sync::atomic::AtomicBool::new(false),
            thread_id: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn route(
        &self,
        vk: u32,
        pressed: bool,
        blocked_modifier: bool,
        alt_context: bool,
        now: Instant,
    ) -> bool {
        let Ok(mut router) = self.router.lock() else {
            return false;
        };
        let old_alt = self.alt_held.load(std::sync::atomic::Ordering::Acquire);
        let consumed = router.event(vk, pressed, blocked_modifier, alt_context, now);
        let new_alt = router.alt_keys != 0;
        self.alt_held
            .store(new_alt, std::sync::atomic::Ordering::Release);
        if old_alt != new_alt || consumed || !router.commands.is_empty() {
            self.ctx.request_repaint();
        }
        consumed
    }
}

#[cfg(windows)]
static ACTIVE_HOOK: std::sync::OnceLock<Mutex<Option<std::sync::Weak<HookState>>>> =
    std::sync::OnceLock::new();

#[cfg(windows)]
fn active_hook() -> &'static Mutex<Option<std::sync::Weak<HookState>>> {
    ACTIVE_HOOK.get_or_init(|| Mutex::new(None))
}

#[cfg(windows)]
fn windows_monitor(ctx: Context) -> std::io::Result<Monitor> {
    use std::sync::{Arc, mpsc};

    let state = Arc::new(HookState::new(ctx));
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let thread_state = Arc::clone(&state);
    let thread = std::thread::Builder::new()
        .name("hestia-overlay-keyboard".into())
        .spawn(move || keyboard_thread(thread_state, ready_tx))?;

    match ready_rx.recv() {
        Ok(Ok(_)) => Ok(Monitor {
            state,
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "overlay keyboard hook thread exited during startup",
            ))
        }
    }
}

#[cfg(windows)]
fn keyboard_thread(
    state: std::sync::Arc<HookState>,
    ready: std::sync::mpsc::SyncSender<std::io::Result<u32>>,
) {
    use std::sync::atomic::Ordering;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL,
    };

    let thread_id = unsafe { GetCurrentThreadId() };
    state.thread_id.store(thread_id, Ordering::Release);

    // Force creation of this thread's message queue before the owner can post
    // WM_QUIT during Drop.
    let mut queue_probe = MSG::default();
    unsafe {
        let _ = PeekMessageW(&mut queue_probe, None, 0, 0, PM_NOREMOVE);
    }

    initialize_physical_state(&state);

    let module = match current_module() {
        Ok(module) => module,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    {
        let Ok(mut active) = active_hook().lock() else {
            let _ = ready.send(Err(std::io::Error::other(
                "overlay keyboard hook state is poisoned",
            )));
            return;
        };
        if active.as_ref().is_some_and(|weak| weak.upgrade().is_some()) {
            let _ = ready.send(Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "overlay keyboard hook is already running",
            )));
            return;
        }
        *active = Some(std::sync::Arc::downgrade(&state));
    }

    let hook =
        match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(module), 0) } {
            Ok(hook) => hook,
            Err(error) => {
                clear_active_hook(&state);
                let _ = ready.send(Err(std::io::Error::other(error.to_string())));
                return;
            }
        };

    if ready.send(Ok(thread_id)).is_err() {
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
        clear_active_hook(&state);
        return;
    }

    let mut message = MSG::default();
    while !state.stop.load(Ordering::Acquire) {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 == 0 || result.0 == -1 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
    clear_active_hook(&state);
}

#[cfg(windows)]
fn initialize_physical_state(state: &HookState) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_A, VK_D, VK_LMENU, VK_RMENU, VK_S, VK_SPACE, VK_W,
    };

    let down = [VK_A, VK_D, VK_W, VK_S, VK_SPACE]
        .into_iter()
        .enumerate()
        .fold(0u8, |mask, (index, key)| {
            let pressed = unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0;
            mask | (u8::from(pressed) << index)
        });
    let alt_keys = u8::from(unsafe { GetAsyncKeyState(i32::from(VK_LMENU.0)) } < 0)
        | (u8::from(unsafe { GetAsyncKeyState(i32::from(VK_RMENU.0)) } < 0) << 1);
    if let Ok(mut router) = state.router.lock() {
        router.alt_keys = alt_keys;
        router.down = down;
        state
            .alt_held
            .store(alt_keys != 0, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(windows)]
fn clear_active_hook(state: &std::sync::Arc<HookState>) {
    let Ok(mut active) = active_hook().lock() else {
        return;
    };
    let same = active
        .as_ref()
        .and_then(std::sync::Weak::upgrade)
        .is_some_and(|candidate| std::sync::Arc::ptr_eq(&candidate, state));
    if same {
        *active = None;
    }
}

#[cfg(windows)]
fn active_state() -> Option<std::sync::Arc<HookState>> {
    active_hook()
        .lock()
        .ok()
        .and_then(|active| active.as_ref().and_then(std::sync::Weak::upgrade))
}

#[cfg(windows)]
unsafe extern "system" fn keyboard_hook(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, LLKHF_INJECTED, WM_KEYDOWN, WM_KEYUP,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    if code < 0 || lparam.0 == 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let Some(state) = active_state() else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };
    let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if event.flags.contains(LLKHF_INJECTED) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let pressed = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    let released = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
    if !pressed && !released {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let blocked_modifier = {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_RWIN,
        };
        [VK_CONTROL, VK_LWIN, VK_RWIN]
            .into_iter()
            .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
    };
    let consumed = state.route(
        event.vkCode,
        pressed,
        blocked_modifier,
        event.flags.contains(LLKHF_ALTDOWN),
        Instant::now(),
    );
    if consumed {
        windows::Win32::Foundation::LRESULT(1)
    } else {
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

#[cfg(windows)]
fn current_module() -> std::io::Result<windows::Win32::Foundation::HINSTANCE> {
    use windows::Win32::Foundation::{HINSTANCE, HMODULE};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> HMODULE;
    }

    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    if module.0.is_null() {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(HINSTANCE(module.0))
    }
}

#[cfg(windows)]
impl Drop for Monitor {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

        self.state.stop.store(true, Ordering::Release);
        let thread_id = self.state.thread_id.load(Ordering::Acquire);
        if thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(offset_ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(offset_ms)
    }

    #[test]
    fn alt_navigation_maps_to_expected_commands() {
        let mut router = Router::new(1, 0);
        assert!(router.event(VK_A, true, false, false, t(0)));
        assert!(router.event(VK_D, true, false, false, t(0)));
        assert!(router.event(VK_W, true, false, false, t(0)));
        assert!(router.event(VK_S, true, false, false, t(0)));
        assert_eq!(
            router.take_commands(),
            vec![
                Command::Mod(-1),
                Command::Mod(1),
                Command::Category(-1),
                Command::Category(1),
            ]
        );
    }

    #[test]
    fn ctrl_or_windows_chords_reach_the_game() {
        let mut router = Router::new(1, 0);
        assert!(!router.event(VK_A, true, true, false, t(0)));
        assert!(!router.event(VK_D, true, true, false, t(0)));
        assert!(router.take_commands().is_empty());
    }

    #[test]
    fn pre_alt_keyup_is_never_consumed() {
        let mut router = Router::new(0, 1 << 0);
        assert!(!router.event(VK_A, true, false, false, t(0)));
        assert!(!router.event(VK_LMENU, true, false, false, t(0)));
        assert!(!router.event(VK_A, false, false, false, t(0)));
    }

    #[test]
    fn captured_keyup_stays_consumed_after_alt_release() {
        let mut router = Router::new(1, 0);
        assert!(router.event(VK_A, true, false, false, t(0)));
        assert!(!router.event(VK_LMENU, false, false, false, t(1)));
        assert!(router.event(VK_A, false, false, false, t(2)));
    }

    #[cfg(windows)]
    #[test]
    fn alt_keyup_clears_held_even_with_alt_context_flag() {
        let state = HookState::new(Context::default());
        assert!(!state.route(VK_LMENU, true, false, false, Instant::now()));
        assert!(state.alt_held.load(std::sync::atomic::Ordering::Acquire));
        assert!(!state.route(VK_LMENU, false, false, true, Instant::now()));
        assert!(!state.alt_held.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn navigation_repeats_after_delay_but_space_is_edge_only() {
        let mut router = Router::new(1, 0);
        let start = Instant::now();
        assert!(router.event(VK_D, true, false, false, start));
        assert!(router.event(VK_D, true, false, false, start + Duration::from_millis(299)));
        assert_eq!(router.commands.len(), 1);
        assert!(router.event(VK_D, true, false, false, start + REPEAT_DELAY));
        assert!(router.event(
            VK_D,
            true,
            false,
            false,
            start + REPEAT_DELAY + Duration::from_millis(119)
        ));
        assert_eq!(router.commands.len(), 2);
        assert!(router.event(
            VK_D,
            true,
            false,
            false,
            start + REPEAT_DELAY + REPEAT_INTERVAL
        ));
        assert_eq!(
            router.take_commands(),
            vec![Command::Mod(1), Command::Mod(1), Command::Mod(1)]
        );

        let mut router = Router::new(1, 0);
        assert!(router.event(VK_SPACE, true, false, false, start));
        assert!(router.event(VK_SPACE, true, false, false, start + Duration::from_secs(1)));
        assert_eq!(router.take_commands(), vec![Command::Activate]);
    }
}
