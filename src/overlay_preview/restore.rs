//! Return from the lightweight overlay to the normal Hestia window.
//!
//! The overlay is intentionally a separate native window.  When the normal
//! window is already alive, restoring that window is preferable to starting a
//! second application instance.  If it cannot be found, launch the
//! current executable and let normal startup handle the requested profile.

use std::path::Path;

use anyhow::{Context, Result, bail};

/// Show the existing Hestia window, or start normal Hestia startup.
///
/// `state_path` identifies the settings currently used by the overlay, including
/// an installed or explicitly selected portable profile. Close the overlay after this
/// function returns `Ok(())`.
pub(super) fn show_main(state_path: Option<&Path>) -> Result<()> {
    #[cfg(windows)]
    if try_restore_existing_window()? {
        return Ok(());
    }

    spawn_main(state_path)
}

fn spawn_main(state_path: Option<&Path>) -> Result<()> {
    if let Some(path) = state_path {
        if !path.is_file() {
            bail!(
                "cannot restore Hestia with profile {:?}: the profile file does not exist",
                path
            );
        }
    }

    let executable = std::env::current_exe().context("failed to locate the Hestia executable")?;
    let mut command = std::process::Command::new(&executable);
    if let Some(path) = state_path {
        command.arg("--restore-from-overlay").arg(path);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        // The release executable is a GUI subsystem app, but debug builds may
        // still inherit a console.  The overlay must never flash a console
        // when it starts the normal window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .with_context(|| format!("failed to launch Hestia from {:?}", executable))?;
    Ok(())
}

#[cfg(windows)]
fn try_restore_existing_window() -> Result<bool> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::w;

    let Some(hwnd) = (unsafe { FindWindowW(None, w!("Hestia")) }).ok() else {
        return Ok(false);
    };
    if hwnd.0.is_null() {
        return Ok(false);
    }

    let mut pid = 0u32;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 || pid == std::process::id() || !is_current_hestia_process(pid) {
        return Ok(false);
    }

    if !restore_and_foreground(hwnd) {
        bail!("Hestia is running, but Windows did not allow its window to be activated");
    }
    Ok(true)
}

#[cfg(windows)]
fn is_current_hestia_process(pid: u32) -> bool {
    use std::os::windows::ffi::OsStringExt;

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::core::PWSTR;

    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return false;
    };

    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let queried = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .is_ok()
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if !queried || length == 0 {
        return false;
    }

    let target = std::ffi::OsString::from_wide(&buffer[..length as usize]);
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    let same_executable = target
        .to_string_lossy()
        .eq_ignore_ascii_case(&current.to_string_lossy());
    // The main app can be a release/installed copy rather than this preview build.
    let hestia_name = Path::new(&target)
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name == "hestia" || name.starts_with("hestia-")
        });
    same_executable || hestia_name
}

#[cfg(windows)]
fn restore_and_foreground(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
        SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    if hwnd.0.is_null() || unsafe { !IsWindow(Some(hwnd)).as_bool() } {
        return false;
    }

    let foreground = unsafe { GetForegroundWindow() };
    let current_thread = unsafe { GetCurrentThreadId() };
    let target_thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
    let foreground_thread = if foreground.0.is_null() {
        0
    } else {
        unsafe { GetWindowThreadProcessId(foreground, None) }
    };
    let attached_foreground = foreground_thread != 0
        && foreground_thread != current_thread
        && unsafe { AttachThreadInput(current_thread, foreground_thread, true).as_bool() };
    let attached_target = target_thread != 0
        && target_thread != current_thread
        && target_thread != foreground_thread
        && unsafe { AttachThreadInput(current_thread, target_thread, true).as_bool() };

    unsafe {
        // Restore only a minimized window.  SW_RESTORE on an already maximized
        // window can drop it back to its normal size, which makes this action
        // unexpectedly change the user's main GUI layout.
        let command = if IsIconic(hwnd).as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        let _ = ShowWindow(hwnd, command);
        let _ = BringWindowToTop(hwnd);
    }
    let _ = unsafe { SetForegroundWindow(hwnd) };

    if attached_target {
        unsafe {
            let _ = AttachThreadInput(current_thread, target_thread, false);
        }
    }
    if attached_foreground {
        unsafe {
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
        }
    }

    unsafe { GetForegroundWindow() == hwnd }
}
