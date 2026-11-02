use std::ffi::c_void;

use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegDeleteValueW,
    RegGetValueW, RegOpenKeyExW, RegSetKeyValueW,
};
use windows::core::PCWSTR;

use crate::win32::{logging, to_wide};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "LitePulse";

pub fn current_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" --startup", exe.display()))
}

pub fn existing_command() -> Option<String> {
    let key = to_wide(RUN_KEY);
    let value = to_wide(VALUE_NAME);
    let mut size: u32 = 0;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if rc.0 != 0 || size < 2 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
    let mut written = (buffer.len() * 2) as u32;
    let rc: WIN32_ERROR = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut c_void),
            Some(&mut written),
        )
    };
    if rc.0 != 0 {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..end]))
}

pub fn is_enabled() -> bool {
    existing_command().is_some()
}

pub fn set_enabled(enabled: bool) -> bool {
    if enabled {
        let Some(command) = current_command() else {
            logging::log("could not resolve current executable path for autostart");
            return false;
        };
        let key = to_wide(RUN_KEY);
        let value = to_wide(VALUE_NAME);
        let data = to_wide(&command);
        let rc = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                REG_SZ.0,
                Some(data.as_ptr() as *const c_void),
                (data.len() * 2) as u32,
            )
        };
        if rc.0 != 0 {
            logging::log(&format!("failed to enable autostart: win32 error {}", rc.0));
            return false;
        }
        true
    } else {
        let key = to_wide(RUN_KEY);
        let value = to_wide(VALUE_NAME);
        let mut handle = windows::Win32::System::Registry::HKEY::default();
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                KEY_SET_VALUE,
                &mut handle,
            )
        };
        if opened.0 != 0 {
            return opened.0 == 2;
        }
        let rc = unsafe { RegDeleteValueW(handle, PCWSTR(value.as_ptr())) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        // ERROR_FILE_NOT_FOUND (2) simply means it was not enabled.
        rc.0 == 0 || rc.0 == 2
    }
}

/// If autostart is on but points at an old path, update it to this executable.
pub fn ensure_current() {
    let Some(current) = current_command() else {
        return;
    };
    if let Some(existing) = existing_command() {
        if existing != current {
            logging::log("refreshing stale autostart path");
            let _ = set_enabled(true);
        }
    }
}
