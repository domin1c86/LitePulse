use std::path::PathBuf;

use windows::Win32::UI::WindowsAndMessaging::WM_APP;

pub mod autostart;
pub mod dpi;
pub mod font;
pub mod logging;
pub mod single_instance;
pub mod theme;

pub const WINDOW_CLASS: &str = "LitePulsePanel";
pub const WINDOW_TITLE: &str = "LitePulse";
pub const WM_APP_TRAY: u32 = WM_APP + 1;
pub const WM_APP_SHOW: u32 = WM_APP + 2;
pub const WM_APP_SAMPLE: u32 = WM_APP + 3;

/// UTF-16, NUL-terminated.
pub fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("LitePulse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_strings_are_nul_terminated() {
        let wide = to_wide("测试");
        assert_eq!(wide.last().copied(), Some(0));
        assert!(wide.len() > 2);
    }
}
