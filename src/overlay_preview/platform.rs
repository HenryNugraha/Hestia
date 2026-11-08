#[cfg(windows)]
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, AtomicIsize, Ordering},
};

#[cfg(windows)]
use egui::Rect;

#[cfg(windows)]
use super::keyboard;
use crate::model::OverlayHotkey;

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetDpiForWindow(hwnd: windows::Win32::Foundation::HWND) -> u32;
}

#[cfg(windows)]
static PREVIOUS_WNDPROC: AtomicIsize = AtomicIsize::new(0);

#[cfg(windows)]
static SUBCLASS_HWND: AtomicIsize = AtomicIsize::new(0);

#[cfg(windows)]
const HOTKEY_ID: i32 = 1;

#[cfg(windows)]
const BOTTOM_GAP: f32 = 18.0;

#[cfg(windows)]
const RAISE_TIMER_ID: usize = 1;

#[cfg(windows)]
const RAISE_CHECK_MS: u32 = 250;

#[cfg(windows)]
static RETURN_TARGET: AtomicIsize = AtomicIsize::new(0);

#[cfg(windows)]
static CLICK_TAKES_KEYBOARD: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
static SWALLOW_CHARACTERS: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
static GAME_PROCESSES: Mutex<Vec<u32>> = Mutex::new(Vec::new());

#[cfg(windows)]
static WAITING_FOR_GAME: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
static GAME_ARRIVED: AtomicBool = AtomicBool::new(false);

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
    configure_window(hwnd, cc.egui_ctx.pixels_per_point())?;
    keep_above_taskbar(hwnd);
    Ok(())
}

#[cfg(windows)]
fn keep_above_taskbar(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::SetTimer;

    if let Err(error) = mark_fullscreen(hwnd) {
        tracing::warn!(%error, "Could not mark the overlay as fullscreen");
    }
    if unsafe { SetTimer(Some(hwnd), RAISE_TIMER_ID, RAISE_CHECK_MS, None) } == 0 {
        let error = std::io::Error::last_os_error();
        tracing::warn!(%error, "Could not start the overlay taskbar check");
    }
}

#[cfg(windows)]
fn mark_fullscreen(hwnd: windows::Win32::Foundation::HWND) -> windows::core::Result<()> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::{ITaskbarList2, TaskbarList};

    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = CoCreateInstance::<_, ITaskbarList2>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
            .and_then(|taskbar| {
                taskbar.HrInit()?;
                taskbar.MarkFullscreenWindow(hwnd, true)
            });
        if initialized {
            CoUninitialize();
        }
        result
    }
}

#[cfg(windows)]
unsafe fn raise_above_taskbar(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER,
        SWP_NOSIZE, SetWindowPos,
    };

    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null()
        || foreground == hwnd
        || !unsafe { taskbar_above(hwnd) }
        || unsafe { is_shell_window(foreground) }
    {
        return;
    }
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )
    };
}

#[cfg(windows)]
unsafe fn taskbar_above(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GW_HWNDPREV, GetWindow};

    let mut window = hwnd;
    for _ in 0..256 {
        match unsafe { GetWindow(window, GW_HWNDPREV) } {
            Ok(above) => window = above,
            Err(_) => return false,
        }
        if matches!(
            unsafe { class_name(window) }.as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return true;
        }
    }
    false
}

#[cfg(windows)]
unsafe fn is_shell_window(window: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
    use windows::core::w;

    let class = unsafe { class_name(window) };
    let taskbar_process = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.is_ok_and(|taskbar| {
        let mut taskbar_pid = 0;
        let mut window_pid = 0;
        unsafe {
            GetWindowThreadProcessId(taskbar, Some(&mut taskbar_pid));
            GetWindowThreadProcessId(window, Some(&mut window_pid));
        }
        taskbar_pid != 0 && taskbar_pid == window_pid
    });
    is_shell_class(&class, taskbar_process)
}

#[cfg(windows)]
fn is_shell_class(class: &str, taskbar_process: bool) -> bool {
    match class {
        "Windows.UI.Core.CoreWindow" => true,
        "CabinetWClass" | "Progman" | "WorkerW" => false,
        _ => taskbar_process,
    }
}

#[cfg(windows)]
unsafe fn class_name(window: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

    let mut buffer = [0u16; 64];
    let length = unsafe { GetClassNameW(window, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..length])
}

#[cfg(windows)]
fn configure_window(
    hwnd: windows::Win32::Foundation::HWND,
    pixels_per_point: f32,
) -> std::io::Result<()> {
    use windows::Win32::Foundation::{GetLastError, SetLastError, WIN32_ERROR};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWLP_WNDPROC, GetForegroundWindow, GetWindowLongPtrW, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos,
        WS_EX_NOACTIVATE,
    };

    unsafe {
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

        SetLastError(WIN32_ERROR(0));
        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, callback);
        if previous == 0 {
            let error = GetLastError();
            return Err(std::io::Error::from_raw_os_error(error.0 as i32));
        }
        PREVIOUS_WNDPROC.store(previous, Ordering::Release);
        SUBCLASS_HWND.store(hwnd.0 as isize, Ordering::Release);

        set_window_input_region(hwnd, &[])?;

        let foreground = GetForegroundWindow();
        place_on_monitor_of(hwnd, window_or(foreground, hwnd), pixels_per_point)?;
    }

    Ok(())
}

#[cfg(windows)]
fn window_or(
    window: windows::Win32::Foundation::HWND,
    fallback: windows::Win32::Foundation::HWND,
) -> windows::Win32::Foundation::HWND {
    if window.0.is_null() { fallback } else { window }
}

#[cfg(windows)]
unsafe fn place_on_monitor_of(
    hwnd: windows::Win32::Foundation::HWND,
    target: windows::Win32::Foundation::HWND,
    pixels_per_point: f32,
) -> std::io::Result<()> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };

    unsafe {
        let monitor = MonitorFromWindow(target, MONITOR_DEFAULTTONEAREST);
        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if monitor.0.is_null() || !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
            return Ok(());
        }
        for _ in 0..2 {
            let mut window_rect = RECT::default();
            if GetWindowRect(hwnd, &mut window_rect).is_err() {
                break;
            }
            let dpi = GetDpiForWindow(hwnd);
            let scale = if dpi == 0 {
                pixels_per_point.max(0.5)
            } else {
                dpi as f32 / 96.0
            };
            let gap = (BOTTOM_GAP * scale).round() as i32;
            let (x, y, width, height) = fit_window_to_monitor(
                monitor_info.rcMonitor,
                window_rect.right - window_rect.left,
                window_rect.bottom - window_rect.top,
                gap,
            );
            if (x, y, x + width, y + height)
                == (
                    window_rect.left,
                    window_rect.top,
                    window_rect.right,
                    window_rect.bottom,
                )
            {
                break;
            }
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
    Ok(())
}

#[cfg(windows)]
unsafe fn show_on_monitor_of(
    hwnd: windows::Win32::Foundation::HWND,
    target: windows::Win32::Foundation::HWND,
) {
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, IsWindowVisible, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowPos, ShowWindow,
    };

    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return;
    }
    let moved = unsafe {
        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
            != MonitorFromWindow(target, MONITOR_DEFAULTTONEAREST)
    };
    if moved && let Err(error) = unsafe { place_on_monitor_of(hwnd, target, 1.0) } {
        tracing::warn!(%error, "Could not place the overlay on the game's monitor");
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

#[cfg(windows)]
fn overlay_window() -> Option<windows::Win32::Foundation::HWND> {
    let hwnd = SUBCLASS_HWND.load(Ordering::Acquire);
    (hwnd != 0).then(|| windows::Win32::Foundation::HWND(hwnd as *mut std::ffi::c_void))
}

#[cfg(windows)]
pub(super) fn hide() {
    use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    lend_title(false);
    if let Some(hwnd) = overlay_window() {
        let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

#[cfg(windows)]
use crate::overlay_protocol::XXMI_TITLE;

#[cfg(windows)]
static LENT_TITLE: Mutex<Option<String>> = Mutex::new(None);

#[cfg(windows)]
pub(super) fn lend_title(lend: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, SetWindowTextW};
    use windows::core::PCWSTR;

    let Some(hwnd) = overlay_window() else {
        return;
    };
    let set = |title: &str| {
        let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { SetWindowTextW(hwnd, PCWSTR::from_raw(title.as_ptr())) }.is_ok()
    };
    let mut lent = LENT_TITLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if lend {
        if lent.is_some() {
            return;
        }
        let mut buffer = [0_u16; 256];
        let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        let title = String::from_utf16_lossy(&buffer[..usize::try_from(length).unwrap_or(0)]);
        if title != XXMI_TITLE && set(XXMI_TITLE) {
            tracing::debug!("The overlay lent itself XXMI's title for a key");
            *lent = Some(title);
        }
    } else if let Some(title) = lent.take() {
        set(&title);
    }
}

#[cfg(windows)]
pub(super) fn is_visible() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;

    overlay_window().is_some_and(|hwnd| unsafe { IsWindowVisible(hwnd) }.as_bool())
}

#[cfg(windows)]
pub(super) fn wait_for_game(processes: &[u32]) {
    set_game_processes(processes);
    WAITING_FOR_GAME.store(true, Ordering::Release);
}

#[cfg(windows)]
pub(super) fn set_game_processes(processes: &[u32]) {
    *GAME_PROCESSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = processes.to_vec();
}

#[cfg(windows)]
pub(super) fn take_game_arrived() -> bool {
    GAME_ARRIVED.swap(false, Ordering::AcqRel)
}

#[cfg(windows)]
unsafe fn watch_for_game(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    if !WAITING_FOR_GAME.load(Ordering::Acquire) {
        return;
    }
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return;
    }
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(foreground, Some(&mut process)) };
    let game = process != 0
        && GAME_PROCESSES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&process);
    if game && WAITING_FOR_GAME.swap(false, Ordering::AcqRel) {
        unsafe { show_on_monitor_of(hwnd, foreground) };
        GAME_ARRIVED.store(true, Ordering::Release);
        keyboard::wake();
    }
}

#[cfg(windows)]
pub(super) fn register_hotkey(hotkey: OverlayHotkey) -> std::io::Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};

    let hwnd = SUBCLASS_HWND.load(Ordering::Acquire);
    if hwnd == 0 {
        return Err(std::io::Error::other("overlay window is not configured"));
    }
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
        RegisterHotKey(
            Some(hwnd),
            HOTKEY_ID,
            hotkey_modifiers(hotkey),
            u32::from(hotkey.key),
        )
    }
    .map_err(os_error)
}

#[cfg(windows)]
fn hotkey_modifiers(
    hotkey: OverlayHotkey,
) -> windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
    };

    [
        (hotkey.ctrl, MOD_CONTROL),
        (hotkey.alt, MOD_ALT),
        (hotkey.shift, MOD_SHIFT),
    ]
    .into_iter()
    .filter(|&(held, _)| held)
    .fold(MOD_NOREPEAT, |modifiers, (_, modifier)| {
        modifiers | modifier
    })
}

#[cfg(windows)]
pub(crate) fn hotkey_taken(hotkey: OverlayHotkey) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};

    const PROBE_ID: i32 = 0xB00;
    let registered = unsafe {
        RegisterHotKey(
            None,
            PROBE_ID,
            hotkey_modifiers(hotkey),
            u32::from(hotkey.key),
        )
    };
    match registered {
        Ok(()) => {
            let _ = unsafe { UnregisterHotKey(None, PROBE_ID) };
            false
        }
        Err(error) => os_error(error).raw_os_error() == Some(super::HOTKEY_ALREADY_REGISTERED),
    }
}

#[cfg(windows)]
pub(super) fn resize_canvas(size: egui::Vec2) {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };

    let hwnd = SUBCLASS_HWND.load(Ordering::Acquire);
    if hwnd == 0 {
        return;
    }
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return;
    }
    let scale = match unsafe { GetDpiForWindow(hwnd) } {
        0 => 1.0,
        dpi => dpi as f32 / 96.0,
    };
    let width = (size.x * scale).round() as i32;
    let height = (size.y * scale).round() as i32;
    let x = (rect.left + rect.right - width) / 2;
    let y = rect.bottom - height;
    if let Err(error) = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
        )
    } {
        tracing::warn!(%error, "Could not resize the overlay");
    }
}

#[cfg(windows)]
pub(super) fn set_click_takes_keyboard(takes: bool) {
    CLICK_TAKES_KEYBOARD.store(takes, Ordering::Relaxed);
}

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
    !target.0.is_null()
        && unsafe { IsWindow(Some(target)) }.as_bool()
        && unsafe { SetForegroundWindow(target) }.as_bool()
}

#[cfg(windows)]
fn os_error(error: windows::core::Error) -> std::io::Error {
    let code = error.code().0 as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        std::io::Error::from_raw_os_error((code & 0xFFFF) as i32)
    } else {
        std::io::Error::other(error.to_string())
    }
}

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
            let _ = DeleteObject(HGDIOBJ(combined.0));
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn bottom_center_position(
    monitor: windows::Win32::Foundation::RECT,
    window_width: i32,
    window_height: i32,
    gap: i32,
) -> (i32, i32) {
    let monitor_width = (monitor.right - monitor.left).max(0);
    let monitor_height = (monitor.bottom - monitor.top).max(0);
    let width = window_width.max(1);
    let height = window_height.max(1);
    let x = if width >= monitor_width {
        monitor.left
    } else {
        (monitor.left + (monitor_width - width) / 2).clamp(monitor.left, monitor.right - width)
    };
    let desired_y = monitor.bottom - height - gap.max(0);
    let y = if height >= monitor_height {
        monitor.top
    } else {
        desired_y.clamp(monitor.top, monitor.bottom - height)
    };
    (x, y)
}

#[cfg(windows)]
fn fit_window_to_monitor(
    monitor: windows::Win32::Foundation::RECT,
    window_width: i32,
    window_height: i32,
    gap: i32,
) -> (i32, i32, i32, i32) {
    let monitor_width = (monitor.right - monitor.left).max(1);
    let monitor_height = (monitor.bottom - monitor.top).max(1);
    let gap = gap.max(0);
    let max_width = (monitor_width - gap.saturating_mul(2)).max(1);
    let max_height = (monitor_height - gap).max(1);
    let width = window_width.max(1).min(max_width);
    let height = window_height.max(1).min(max_height);
    let (x, y) = bottom_center_position(monitor, width, height, gap);
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

#[cfg(not(windows))]
pub(super) fn configure(_: &eframe::CreationContext<'_>) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn register_hotkey(_: OverlayHotkey) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn hotkey_taken(_: OverlayHotkey) -> bool {
    false
}

#[cfg(not(windows))]
pub(super) fn resize_canvas(_: egui::Vec2) {}

#[cfg(not(windows))]
pub(super) fn set_click_takes_keyboard(_: bool) {}

#[cfg(not(windows))]
pub(super) fn return_focus() -> bool {
    true
}

#[cfg(not(windows))]
pub(super) fn hide() {}

#[cfg(not(windows))]
pub(super) fn is_visible() -> bool {
    true
}

#[cfg(not(windows))]
pub(super) fn wait_for_game(_: &[u32]) {}

#[cfg(not(windows))]
pub(super) fn set_game_processes(_: &[u32]) {}

#[cfg(not(windows))]
pub(super) fn take_game_arrived() -> bool {
    false
}

#[cfg(windows)]
unsafe fn take_focus_for_hotkey(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindowVisible};

    let foreground = unsafe { GetForegroundWindow() };
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        WAITING_FOR_GAME.store(false, Ordering::Release);
        unsafe { show_on_monitor_of(hwnd, window_or(foreground, hwnd)) };
    }
    let already_focused = foreground == hwnd;
    let focused = unsafe { take_focus(hwnd, foreground) };
    tracing::info!(focused, already_focused, "overlay hotkey");
    keyboard::hotkey(focused, focused && !already_focused);
}

#[cfg(windows)]
unsafe fn take_focus_for_click(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let foreground = unsafe { GetForegroundWindow() };
    if foreground == hwnd {
        return;
    }
    let focused = unsafe { take_focus(hwnd, foreground) };
    tracing::info!(focused, "overlay click");
    if focused {
        keyboard::clicked();
    }
}

#[cfg(windows)]
unsafe fn take_focus(
    hwnd: windows::Win32::Foundation::HWND,
    foreground: windows::Win32::Foundation::HWND,
) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

    if foreground != hwnd {
        if !foreground.0.is_null() {
            RETURN_TARGET.store(foreground.0 as isize, Ordering::Release);
        }
        let _ = unsafe { SetForegroundWindow(hwnd) };
    }
    let focused = unsafe { GetForegroundWindow() } == hwnd;
    if focused {
        let _ = unsafe { SetFocus(Some(hwnd)) };
    }
    focused
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

#[cfg(windows)]
unsafe fn keyboard_message(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> Option<windows::Win32::Foundation::LRESULT> {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMessageExtraInfo, SC_KEYMENU, WA_INACTIVE, WM_ACTIVATE, WM_CHAR, WM_DEADCHAR, WM_HOTKEY,
        WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSCHAR, WM_SYSCOMMAND, WM_SYSDEADCHAR,
        WM_XBUTTONDOWN,
    };

    match msg {
        WM_HOTKEY if wparam.0 == HOTKEY_ID as usize && keyboard::installed() => {
            unsafe { take_focus_for_hotkey(hwnd) };
            Some(LRESULT(0))
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
            if CLICK_TAKES_KEYBOARD.load(Ordering::Relaxed) && keyboard::installed() =>
        {
            unsafe { take_focus_for_click(hwnd) };
            None
        }
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
            if unsafe { GetMessageExtraInfo() }.0 as usize
                == crate::overlay_protocol::HESTIA_INPUT_MARK
            {
                if key.pressed {
                    SWALLOW_CHARACTERS.store(true, Ordering::Relaxed);
                }
                return Some(LRESULT(0));
            }
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
        CallWindowProcW, DefWindowProcW, GWL_EXSTYLE, IsWindowVisible, MA_NOACTIVATE, STYLESTRUCT,
        WM_MOUSEACTIVATE, WM_NCDESTROY, WM_STYLECHANGING, WM_TIMER, WS_EX_NOACTIVATE,
    };

    if msg == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 && lparam.0 != 0 {
        let style = unsafe { &mut *(lparam.0 as *mut STYLESTRUCT) };
        style.styleNew |= WS_EX_NOACTIVATE.0;
    }

    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }

    let previous = PREVIOUS_WNDPROC.load(Ordering::Acquire);
    let subclass_hwnd = SUBCLASS_HWND.load(Ordering::Acquire);
    let ours = previous != 0 && subclass_hwnd == hwnd.0 as isize;
    if ours && msg == WM_TIMER && wparam.0 == RAISE_TIMER_ID {
        unsafe { watch_for_game(hwnd) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            unsafe { raise_above_taskbar(hwnd) };
        }
        return LRESULT(0);
    }
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
        ctx.begin_pass(egui::RawInput::default());
        ctx.end_pass().drop_without_applying_deltas();
        assert!((ctx.pixels_per_point() - 2.0).abs() < f32::EPSILON);
        let rects = physical_region_rects(
            &ctx,
            &[
                Rect::from_min_max(pos2(-4.25, 3.0), pos2(10.0, 20.5)),
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
    fn default_position_centres_on_negative_monitor() {
        let monitor = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        assert_eq!(bottom_center_position(monitor, 560, 660, 18), (-1240, 402));
    }

    #[test]
    fn default_position_keeps_gap_on_offset_monitor() {
        let monitor = RECT {
            left: 2560,
            top: 363,
            right: 4480,
            bottom: 1443,
        };
        let (x, y, width, height) = fit_window_to_monitor(monitor, 560, 660, 18);
        assert_eq!((x, y, width, height), (3240, 765, 560, 660));
        assert_eq!(monitor.bottom - (y + height), 18);
    }

    #[test]
    fn default_position_clamps_oversized_canvas_to_monitor() {
        let monitor = RECT {
            left: -300,
            top: 40,
            right: 900,
            bottom: 700,
        };
        assert_eq!(bottom_center_position(monitor, 1600, 900, 18), (-300, 40));
    }

    #[test]
    fn fitted_geometry_shrinks_canvas_and_keeps_bottom_gap() {
        let monitor = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let (x, y, width, height) = fit_window_to_monitor(monitor, 560, 1320, 27);
        assert_eq!((x, width), (-1240, 560));
        assert_eq!((y, height), (0, 1053));
        assert_eq!(y + height, monitor.bottom - 27);
    }

    #[test]
    fn shell_windows_hold_off_raising_the_overlay() {
        assert!(is_shell_class("Shell_TrayWnd", true));
        assert!(is_shell_class("Shell_SecondaryTrayWnd", true));
        assert!(is_shell_class("TopLevelWindowForOverflowXamlIsland", true));
        assert!(is_shell_class("Windows.UI.Core.CoreWindow", false));
        for moved_on in ["CabinetWClass", "Progman", "WorkerW"] {
            assert!(!is_shell_class(moved_on, true));
        }
        assert!(!is_shell_class("UnityWndClass", false));
    }
}
