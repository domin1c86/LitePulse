use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, GetDpiForSystem, GetDpiForWindow,
    MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};

pub fn set_process_dpi_awareness() {
    // Per-monitor v2 keeps the panel crisp while moving between monitors.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn for_window(hwnd: HWND) -> u32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 { 96 } else { dpi }
}

pub fn system() -> u32 {
    let dpi = unsafe { GetDpiForSystem() };
    if dpi == 0 { 96 } else { dpi }
}

pub fn at_point(point: POINT) -> u32 {
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let (mut x, mut y) = (0, 0);
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }.is_ok() && x > 0 {
        x
    } else {
        system()
    }
}

pub fn scale(value_dip: f32, dpi: u32) -> i32 {
    (value_dip * dpi as f32 / 96.0).round() as i32
}
