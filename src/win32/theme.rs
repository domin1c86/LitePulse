use std::ffi::c_void;

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::PCWSTR;

use crate::win32::to_wide;

/// Follow the Windows app theme. Defaults to light if the value cannot be read.
pub fn system_uses_light() -> bool {
    let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = to_wide("AppsUseLightTheme");
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    result.0 == 0 && data != 0
}
