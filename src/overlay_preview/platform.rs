//! Platform adjustments for the always-on-top overlay.
//!
//! The overlay is a mouse target while the game remains the keyboard target.  On
//! Windows, `ViewportBuilder::with_active(false)` only controls the initial
//! `ShowWindow` call in winit.  `WS_EX_NOACTIVATE` and `WM_MOUSEACTIVATE` are
//! needed as well so a click, pin, or drag does not move foreground focus to the
//! overlay.
//!
//! The keyboard moves to the overlay only through Alt+H.  Windows delivers the
//! hotkey even over a game that blocks other processes from its input, and
//! receiving it allows the overlay to take the foreground.  Key messages then
//! reach this window procedure, which hands them to `keyboard`, and
//! `return_focus` gives the keyboard back afterwards.

#[cfg(windows)]
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, AtomicIsize, Ordering},
};

#[cfg(windows)]
use egui::Rect;

#[cfg(windows)]
use super::keyboard;

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetDpiForWindow(hwnd: windows::Win32::Foundation::HWND) -> u32;
}

/// The preview has one native window per process.  Keep the original winit
/// window procedure so the subclass can forward every message it does not own.
#[cfg(windows)]
static PREVIOUS_WNDPROC: AtomicIsize = AtomicIsize::new(0);

#[cfg(windows)]
static SUBCLASS_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(windows)]
const HOTKEY_ID: i32 = 1;

/// The window that had the keyboard when Alt+H took it.
#[cfg(windows)]
static RETURN_TARGET: AtomicIsize = AtomicIsize::new(0);

/// Whether the last key press was swallowed.  `TranslateMessage` has already
/// queued its character messages by then, and they must not reach winit
/// alone: an orphan `WM_SYSCHAR` makes `DefWindowProc` beep.
#[cfg(windows)]
static SWALLOW_CHARACTERS: AtomicBool = AtomicBool::new(false);

/// The native region is owned by USER32 after `SetWindowRgn` succeeds.  Keep
/// only the input used to build it so a normal repaint does not allocate and
/// replace an HRGN every frame.
#[cfg(windows)]
static INPUT_REGION_CACHE: OnceLock<Mutex<Option<InputRegionCache>>> = OnceLock::new();

#[cfg(windows)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct InputRegionCache {
    hwnd: isize,
    rects: Vec<PixelRect>,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(windows)]
impl PixelRect {
    fn new(left: i32, top: i32, right: i32, bottom: i32) -> Option<Self> {
        (right > left && bottom > top).then_some(Self {
            left,
            top,
            right,
            bottom,
        })
    }
}

/// Apply the non-activating window behavior after eframe has created the winit
/// window.  The native window is still hit-testable: `MA_NOACTIVATE` preserves
/// the mouse message instead of using `MA_NOACTIVATEANDEAT`, so egui clicks,
/// wheel input, and native window dragging keep working.
///
/// This does not register the hotkey, synthesize input, call
/// `SetForegroundWindow`, or move focus to another window.  `register_hotkey`
/// adds the only path that moves the keyboard to the overlay.
#[cfg(windows)]
pub(super) fn configure(cc: &eframe::CreationContext<'_>) -> std::io::Result<()> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;

    let Some(_window) = cc.winit_window() else {
        return Ok(());
    };
    let Ok(window_handle) = cc.window_handle() else {
        return Ok(());
    };
    let RawWindowHandle::Win32(handle) = window_handle.as_raw() else {
        return Ok(());
    };
    let hwnd = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
    configure_window(hwnd, cc.egui_ctx.pixels_per_point())
}

#[cfg(windows)]
fn configure_window(
    hwnd: windows::Win32::Foundation::HWND,
    pixels_per_point: f32,
) -> std::io::Result<()> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Foundation::{GetLastError, SetLastError, WIN32_ERROR};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWLP_WNDPROC, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
        SetWindowLongPtrW, SetWindowPos, WS_EX_NOACTIVATE,
    };

    unsafe {
        // Keep all existing winit flags (topmost, layered, tool-window, etc.)
        // and add only the activation policy required by this overlay.
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex_style & WS_EX_NOACTIVATE.0 as isize == 0 {
            SetLastError(WIN32_ERROR(0));
            let previous =
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as isize);
            if previous == 0 {
                let error = GetLastError();
                if error.0 != 0 {
                    return Err(std::io::Error::from_raw_os_error(error.0 as i32));
                }
            }
            if SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            )
            .is_err()
            {
                return Err(std::io::Error::last_os_error());
            }
        }

        let callback = overlay_window_proc as *const () as usize as isize;
        let current = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        if current == callback {
            return Ok(());
        }

        // SetWindowLongPtrW reports the old procedure, so clear last error to
        // distinguish a genuine zero result from a normal replacement.
        SetLastError(WIN32_ERROR(0));
        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, callback);
        if previous == 0 {
            let error = GetLastError();
            return Err(std::io::Error::from_raw_os_error(error.0 as i32));
        }
        PREVIOUS_WNDPROC.store(previous, Ordering::Release);
        SUBCLASS_HWND.store(hwnd.0 as isize, Ordering::Release);

        // Keep the fixed canvas click-through until the first egui frame has
        // supplied its actual visible rectangles. Without this, the unused
        // part of the 560x660 startup canvas briefly intercepts game input.
        set_window_input_region(hwnd, &[])?;

        // The preview uses a fixed-size native canvas so expanding from the
        // mini strip never changes its anchor.  Place that canvas once, at the
        // bottom centre of the monitor containing the foreground game.  The
        // work area already excludes the taskbar and can have negative origins
        // on a monitor arranged to the left of the primary display.
        let mut window_rect = RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_ok() {
            let width = (window_rect.right - window_rect.left).max(1);
            let height = (window_rect.bottom - window_rect.top).max(1);
            let foreground = GetForegroundWindow();
            let monitor_window = if foreground.0.is_null() {
                hwnd
            } else {
                foreground
            };
            let monitor = MonitorFromWindow(monitor_window, MONITOR_DEFAULTTONEAREST);
            if !monitor.0.is_null() {
                let mut monitor_info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
                    let dpi = GetDpiForWindow(monitor_window);
                    let monitor_scale = if dpi == 0 {
                        pixels_per_point.max(0.5)
                    } else {
                        dpi as f32 / 96.0
                    };
                    let margin = (24.0 * monitor_scale).round() as i32;
                    let (x, y, width, height) =
                        fit_window_to_work_area(monitor_info.rcWork, width, height, margin);
                    SetWindowPos(
                        hwnd,
                        None,
                        x,
                        y,
                        width,
                        height,
                        SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
                    )?;
                }
            }
        }
    }

    Ok(())
}

/// Register Alt+H for the configured overlay window.  Windows delivers it even
/// while a game that blocks other input has the keyboard.
#[cfg(windows)]
pub(super) fn register_hotkey() -> std::io::Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_NOREPEAT, RegisterHotKey, VK_H,
    };

    let hwnd = SUBCLASS_HWND.load(Ordering::Acquire);
    if hwnd == 0 {
        return Err(std::io::Error::other("overlay window is not configured"));
    }
    unsafe {
        RegisterHotKey(
            Some(HWND(hwnd as *mut std::ffi::c_void)),
            HOTKEY_ID,
            MOD_ALT | MOD_NOREPEAT,
            u32::from(VK_H.0),
        )
    }
    .map_err(os_error)
}

/// Hand the keyboard back to the window that had it before Alt+H.  Returns
/// false when the overlay still has it.
#[cfg(windows)]
pub(super) fn return_focus() -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };

    let overlay = SUBCLASS_HWND.load(Ordering::Acquire);
    if overlay == 0 || unsafe { GetForegroundWindow() }.0 as isize != overlay {
        return true;
    }
    let target = HWND(RETURN_TARGET.load(Ordering::Acquire) as *mut std::ffi::c_void);
    // The foreground process may give the foreground away.
    !target.0.is_null()
        && unsafe { IsWindow(Some(target)) }.as_bool()
        && unsafe { SetForegroundWindow(target) }.as_bool()
}

/// `windows::core::Error` converts into an `io::Error` that keeps the HRESULT,
/// so recover the Win32 code that callers compare against.
#[cfg(windows)]
fn os_error(error: windows::core::Error) -> std::io::Error {
    let code = error.code().0 as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        std::io::Error::from_raw_os_error((code & 0xFFFF) as i32)
    } else {
        std::io::Error::other(error.to_string())
    }
}

/// Update the native hit-test region from the visible egui rectangles.  A
/// window region is used instead of `HTTRANSPARENT`: the latter only forwards
/// to windows owned by the same thread and therefore cannot make transparent
/// overlay gaps reach the game in another process.
#[cfg(windows)]
pub(super) fn update_input_regions(
    ctx: &egui::Context,
    rects: &[egui::Rect],
) -> std::io::Result<()> {
    let hwnd_value = SUBCLASS_HWND.load(Ordering::Acquire);
    if hwnd_value == 0 {
        return Ok(());
    }
    let hwnd = windows::Win32::Foundation::HWND(hwnd_value as *mut std::ffi::c_void);
    let pixel_rects = physical_region_rects(ctx, rects);
    let cache = INPUT_REGION_CACHE.get_or_init(|| Mutex::new(None));
    {
        let cached = cache.lock().expect("overlay input region cache poisoned");
        if cached.as_ref()
            == Some(&InputRegionCache {
                hwnd: hwnd_value,
                rects: pixel_rects.clone(),
            })
        {
            return Ok(());
        }
    }

    set_window_input_region(hwnd, &pixel_rects)?;
    *cache.lock().expect("overlay input region cache poisoned") = Some(InputRegionCache {
        hwnd: hwnd_value,
        rects: pixel_rects,
    });
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn update_input_regions(_: &egui::Context, _: &[egui::Rect]) -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn physical_region_rects(ctx: &egui::Context, rects: &[Rect]) -> Vec<PixelRect> {
    let scale = ctx.pixels_per_point().max(0.5);
    rects
        .iter()
        .filter_map(|rect| {
            let left = (rect.min.x * scale).floor();
            let top = (rect.min.y * scale).floor();
            let right = (rect.max.x * scale).ceil();
            let bottom = (rect.max.y * scale).ceil();
            PixelRect::new(
                left.clamp(i32::MIN as f32, i32::MAX as f32) as i32,
                top.clamp(i32::MIN as f32, i32::MAX as f32) as i32,
                right.clamp(i32::MIN as f32, i32::MAX as f32) as i32,
                bottom.clamp(i32::MIN as f32, i32::MAX as f32) as i32,
            )
        })
        .collect()
}

#[cfg(windows)]
fn set_window_input_region(
    hwnd: windows::Win32::Foundation::HWND,
    rects: &[PixelRect],
) -> std::io::Result<()> {
    use windows::Win32::Graphics::Gdi::SetWindowRgn;
    use windows::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, HGDIOBJ, RGN_ERROR, RGN_OR,
    };

    unsafe {
        // An empty region makes the whole transparent canvas click through. A
        // null HRGN would mean the opposite: USER32 restores the full window.
        let combined = CreateRectRgn(0, 0, 0, 0);
        if combined.is_invalid() {
            return Err(std::io::Error::last_os_error());
        }

        for rect in rects {
            let part = CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom);
            if part.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(combined.0));
                return Err(std::io::Error::last_os_error());
            }
            let result = CombineRgn(Some(combined), Some(combined), Some(part), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(part.0));
            if result == RGN_ERROR {
                let _ = DeleteObject(HGDIOBJ(combined.0));
                return Err(std::io::Error::last_os_error());
            }
        }

        if SetWindowRgn(hwnd, Some(combined), true) == 0 {
            // USER32 did not take ownership on failure.
            let _ = DeleteObject(HGDIOBJ(combined.0));
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn bottom_center_position(
    work_area: windows::Win32::Foundation::RECT,
    window_width: i32,
    window_height: i32,
    margin: i32,
) -> (i32, i32) {
    let work_width = (work_area.right - work_area.left).max(0);
    let work_height = (work_area.bottom - work_area.top).max(0);
    let width = window_width.max(1);
    let height = window_height.max(1);
    let x = if width >= work_width {
        work_area.left
    } else {
        (work_area.left + (work_width - width) / 2).clamp(work_area.left, work_area.right - width)
    };
    let desired_y = work_area.bottom - height - margin.max(0);
    let y = if height >= work_height {
        work_area.top
    } else {
        desired_y.clamp(work_area.top, work_area.bottom - height)
    };
    (x, y)
}

#[cfg(windows)]
fn fit_window_to_work_area(
    work_area: windows::Win32::Foundation::RECT,
    window_width: i32,
    window_height: i32,
    margin: i32,
) -> (i32, i32, i32, i32) {
    let work_width = (work_area.right - work_area.left).max(1);
    let work_height = (work_area.bottom - work_area.top).max(1);
    let margin = margin.max(0);
    let max_width = (work_width - margin.saturating_mul(2)).max(1);
    let max_height = (work_height - margin).max(1);
    let width = window_width.max(1).min(max_width);
    let height = window_height.max(1).min(max_height);
    let (x, y) = bottom_center_position(work_area, width, height, margin);
    (x, y, width, height)
}

#[cfg(windows)]
fn clear_input_region_cache(hwnd: isize) {
    if let Some(cache) = INPUT_REGION_CACHE.get() {
        let mut cached = cache.lock().expect("overlay input region cache poisoned");
        if cached.as_ref().is_some_and(|entry| entry.hwnd == hwnd) {
            *cached = None;
        }
    }
}

/// Non-Windows preview builds retain the same call site without introducing a
/// platform-specific branch in the eframe app creator.
#[cfg(not(windows))]
pub(super) fn configure(_: &eframe::CreationContext<'_>) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn register_hotkey() -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn return_focus() -> bool {
    true
}

/// Take the keyboard for Alt+H.  Receiving the hotkey is what allows
/// `SetForegroundWindow` to succeed while another app has the foreground.
#[cfg(windows)]
unsafe fn take_focus_for_hotkey(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

    let foreground = unsafe { GetForegroundWindow() };
    let already_focused = foreground == hwnd;
    if !already_focused {
        if !foreground.0.is_null() {
            RETURN_TARGET.store(foreground.0 as isize, Ordering::Release);
        }
        // Activation sends WM_ACTIVATE through this window procedure before
        // the call returns.
        let _ = unsafe { SetForegroundWindow(hwnd) };
    }
    let focused = unsafe { GetForegroundWindow() } == hwnd;
    if focused {
        let _ = unsafe { SetFocus(Some(hwnd)) };
    }
    tracing::info!(focused, already_focused, "overlay hotkey");
    keyboard::hotkey(focused, focused && !already_focused);
}

#[cfg(windows)]
unsafe fn peek_key_message(hwnd: windows::Win32::Foundation::HWND) -> Option<keyboard::KeyMessage> {
    use windows::Win32::UI::WindowsAndMessaging::{
        MSG, PM_NOREMOVE, PeekMessageW, WM_KEYFIRST, WM_KEYLAST,
    };

    let mut next = MSG::default();
    let found =
        unsafe { PeekMessageW(&mut next, Some(hwnd), WM_KEYFIRST, WM_KEYLAST, PM_NOREMOVE) };
    if found.as_bool() {
        keyboard::KeyMessage::parse(next.message, next.wParam.0, next.lParam.0)
    } else {
        None
    }
}

/// Keyboard handling for the subclassed overlay window.  Returns a result for
/// the messages it swallows.
#[cfg(windows)]
unsafe fn keyboard_message(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> Option<windows::Win32::Foundation::LRESULT> {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SC_KEYMENU, WA_INACTIVE, WM_ACTIVATE, WM_CHAR, WM_DEADCHAR, WM_HOTKEY, WM_SYSCHAR,
        WM_SYSCOMMAND, WM_SYSDEADCHAR,
    };

    match msg {
        WM_HOTKEY if wparam.0 == HOTKEY_ID as usize && keyboard::installed() => {
            unsafe { take_focus_for_hotkey(hwnd) };
            Some(LRESULT(0))
        }
        // The overlay has no menu.  Keep Alt+Space and Alt+letter from
        // entering menu mode.
        WM_SYSCOMMAND if wparam.0 & 0xFFF0 == SC_KEYMENU as usize => Some(LRESULT(0)),
        WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR
            if SWALLOW_CHARACTERS.load(Ordering::Relaxed) =>
        {
            Some(LRESULT(0))
        }
        WM_ACTIVATE => {
            if wparam.0 & 0xFFFF == WA_INACTIVE as usize {
                keyboard::deactivated();
            } else {
                keyboard::activated();
            }
            None
        }
        _ => {
            let key = keyboard::KeyMessage::parse(msg, wparam.0, lparam.0)?;
            let swallow = keyboard::key_message(key, || unsafe { peek_key_message(hwnd) });
            if key.pressed {
                SWALLOW_CHARACTERS.store(swallow, Ordering::Relaxed);
            }
            swallow.then_some(LRESULT(0))
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn overlay_window_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GWL_EXSTYLE, MA_NOACTIVATE, STYLESTRUCT, WM_MOUSEACTIVATE,
        WM_NCDESTROY, WM_STYLECHANGING, WS_EX_NOACTIVATE,
    };

    // winit may refresh its extended styles when viewport flags change.  Keep
    // this one bit in those pending style changes as well as in the initial
    // native style set by `configure`.
    if msg == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 && lparam.0 != 0 {
        let style = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
        style.styleNew |= WS_EX_NOACTIVATE.0;
    }

    // The extended style normally makes this unnecessary, but returning
    // MA_NOACTIVATE covers mouse activation paths that go through the window
    // procedure.  Do not return MA_NOACTIVATEANDEAT: the click still belongs to
    // the overlay and must reach egui.
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }

    let previous = PREVIOUS_WNDPROC.load(Ordering::Acquire);
    let subclass_hwnd = SUBCLASS_HWND.load(Ordering::Acquire);
    let ours = previous != 0 && subclass_hwnd == hwnd.0 as isize;
    if ours && let Some(result) = unsafe { keyboard_message(hwnd, msg, wparam, lparam) } {
        return result;
    }
    if ours && msg == WM_NCDESTROY {
        let _ = unsafe { UnregisterHotKey(Some(hwnd), HOTKEY_ID) };
    }

    let result = if ours {
        let previous: windows::Win32::UI::WindowsAndMessaging::WNDPROC =
            Some(unsafe { std::mem::transmute(previous) });
        unsafe { CallWindowProcW(previous, hwnd, msg, wparam, lparam) }
    } else {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    };

    if msg == WM_NCDESTROY {
        clear_input_region_cache(hwnd.0 as isize);
        PREVIOUS_WNDPROC.store(0, Ordering::Release);
        SUBCLASS_HWND.store(0, Ordering::Release);
        RETURN_TARGET.store(0, Ordering::Release);
    }

    result
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use egui::{Rect, pos2};
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, RECT, WPARAM},
            Graphics::Gdi::{DeleteObject, GetWindowRgn, HGDIOBJ, PtInRegion, RGN_ERROR},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GWL_EXSTYLE, GetForegroundWindow,
                GetWindowLongPtrW, MA_NOACTIVATE, SendMessageW, SetWindowLongPtrW, WINDOW_EX_STYLE,
                WM_GETTEXTLENGTH, WM_MOUSEACTIVATE, WS_EX_NOACTIVATE, WS_POPUP,
            },
        },
        core::w,
    };

    #[test]
    fn native_window_preserves_nonactivation_and_forwards_other_messages() {
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                unsafe { DestroyWindow(self.0).expect("destroy hidden test window") };
            }
        }
        unsafe {
            let foreground = GetForegroundWindow();
            // An invisible, process-owned window exercises real native message
            // dispatch without clicking or changing focus in another app.
            let window = TestWindow(
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("Overlay test"),
                    WS_POPUP,
                    0,
                    0,
                    100,
                    100,
                    None,
                    None,
                    None,
                    None,
                )
                .expect("create hidden test window"),
            );
            configure_window(window.0, 1.0).expect("configure hidden test window");
            let visible = [
                PixelRect {
                    left: 0,
                    top: 0,
                    right: 20,
                    bottom: 20,
                },
                PixelRect {
                    left: 60,
                    top: 0,
                    right: 80,
                    bottom: 20,
                },
            ];
            set_window_input_region(window.0, &visible).expect("set visible input regions");
            let probe = windows::Win32::Graphics::Gdi::CreateRectRgn(0, 0, 100, 100);
            assert!(!probe.is_invalid());
            let region_kind = GetWindowRgn(window.0, probe);
            assert_ne!(region_kind, RGN_ERROR);
            assert!(PtInRegion(probe, 10, 10).as_bool());
            assert!(!PtInRegion(probe, 40, 10).as_bool());
            assert!(PtInRegion(probe, 70, 10).as_bool());
            set_window_input_region(window.0, &[]).expect("set empty input region");
            assert_ne!(GetWindowRgn(window.0, probe), RGN_ERROR);
            assert!(!PtInRegion(probe, 10, 10).as_bool());
            assert!(!PtInRegion(probe, 70, 10).as_bool());
            let _ = DeleteObject(HGDIOBJ(probe.0));
            assert_eq!(
                SendMessageW(window.0, WM_MOUSEACTIVATE, Some(WPARAM(0)), Some(LPARAM(0))).0,
                MA_NOACTIVATE as isize,
            );
            let style = GetWindowLongPtrW(window.0, GWL_EXSTYLE);
            assert_ne!(style & WS_EX_NOACTIVATE.0 as isize, 0);
            SetWindowLongPtrW(
                window.0,
                GWL_EXSTYLE,
                style & !(WS_EX_NOACTIVATE.0 as isize),
            );
            assert_ne!(
                GetWindowLongPtrW(window.0, GWL_EXSTYLE) & WS_EX_NOACTIVATE.0 as isize,
                0
            );
            assert_eq!(SendMessageW(window.0, WM_GETTEXTLENGTH, None, None).0, 12);
            assert_eq!(GetForegroundWindow(), foreground);
            drop(window);
            assert_eq!(PREVIOUS_WNDPROC.load(Ordering::Acquire), 0);
            assert_eq!(SUBCLASS_HWND.load(Ordering::Acquire), 0);
        }
    }

    #[test]
    fn input_regions_scale_negative_origins_and_keep_holes() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(2.0);
        // egui applies scale changes at the next pass rather than immediately.
        ctx.begin_pass(egui::RawInput::default());
        let _ = ctx.end_pass();
        assert!((ctx.pixels_per_point() - 2.0).abs() < f32::EPSILON);
        let rects = physical_region_rects(
            &ctx,
            &[
                Rect::from_min_max(pos2(-4.25, 3.0), pos2(10.0, 20.5)),
                // A separate rectangle leaves the space between these two
                // entries out of the native region.
                Rect::from_min_max(pos2(20.0, 3.0), pos2(24.5, 20.5)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 10.0)),
            ],
        );
        assert_eq!(
            rects,
            vec![
                PixelRect {
                    left: -9,
                    top: 6,
                    right: 20,
                    bottom: 41,
                },
                PixelRect {
                    left: 40,
                    top: 6,
                    right: 49,
                    bottom: 41,
                },
            ]
        );
    }

    #[test]
    fn default_position_centres_on_negative_monitor_work_area() {
        let work_area = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        assert_eq!(
            bottom_center_position(work_area, 560, 660, 24),
            (-1240, 356)
        );
    }

    #[test]
    fn default_position_clamps_oversized_canvas_to_work_area() {
        let work_area = RECT {
            left: -300,
            top: 40,
            right: 900,
            bottom: 700,
        };
        assert_eq!(bottom_center_position(work_area, 1600, 900, 24), (-300, 40));
    }

    #[test]
    fn fitted_geometry_shrinks_canvas_and_keeps_bottom_margin() {
        let work_area = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let (x, y, width, height) = fit_window_to_work_area(work_area, 560, 1320, 24);
        assert_eq!((x, width), (-1240, 560));
        assert_eq!(height, 1016);
        assert_eq!(y + height, work_area.bottom - 24);
    }
}
