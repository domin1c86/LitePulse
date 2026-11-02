use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, WPARAM,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, HWND_BROADCAST, PostMessageW};
use windows::core::PCWSTR;

use crate::win32::{WINDOW_CLASS, WM_APP_SHOW, logging, to_wide};

/// Returns true when this process owns the single-instance mutex.
pub fn acquire() -> bool {
    let name = to_wide(r"Local\LitePulse.SingleInstance");
    // CreateMutexW does not clear the last error on a fresh handle, so clear
    // it explicitly before checking for ERROR_ALREADY_EXISTS.
    unsafe {
        windows::Win32::Foundation::SetLastError(windows::Win32::Foundation::WIN32_ERROR(0));
    }
    match unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) } {
        Ok(handle) => {
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                false
            } else {
                // The mutex must live for the whole process lifetime. Keeping the
                // handle alive is intentional; the OS closes it when we exit.
                let _ = handle;
                true
            }
        }
        Err(err) => {
            logging::log(&format!("could not create single-instance mutex: {err}"));
            true
        }
    }
}

/// Ask an already-running instance to show its panel, then let this process exit.
pub fn activate_existing() {
    let class = to_wide(WINDOW_CLASS);
    for _ in 0..30 {
        let found: windows::core::Result<HWND> =
            unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) };
        if let Ok(hwnd) = found {
            if !hwnd.0.is_null() {
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0));
                }
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Cross-process FindWindow can fail in unusual shell/window-station
    // configurations. Broadcasting our private wake-up message is a reliable
    // fallback: only LitePulse understands WM_APP_SHOW.
    logging::log("existing instance window not found; broadcasting show request");
    unsafe {
        let _ = PostMessageW(Some(HWND_BROADCAST), WM_APP_SHOW, WPARAM(0), LPARAM(0));
    }
}
