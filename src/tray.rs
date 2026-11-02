use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject, HGDIOBJ,
};
use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICON_VERSION_4, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    HICON, HMENU, ICONINFO, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW,
    SM_CXSMICON, SetForegroundWindow, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TPM_TOPALIGN, TrackPopupMenu, WM_NULL,
};
use windows::core::PCWSTR;

use crate::config::{Settings, ThemeSetting, TrayMetric};
use crate::metrics::{Snapshot, tray_tooltip, truncate_utf16};
use crate::win32::{WM_APP_TRAY, autostart, font, logging, to_wide};

const ICON_ID: u32 = 1;
const MENU_TOGGLE: u32 = 1001;
const MENU_TOPMOST: u32 = 1002;
const MENU_AUTOSTART: u32 = 1003;
const MENU_INTERVAL_1S: u32 = 1011;
const MENU_INTERVAL_2S: u32 = 1012;
const MENU_INTERVAL_5S: u32 = 1013;
const MENU_INTERVAL_30S: u32 = 1014;
const MENU_INTERVAL_1M: u32 = 1015;
const MENU_THEME_SYSTEM: u32 = 1021;
const MENU_THEME_DARK: u32 = 1022;
const MENU_THEME_LIGHT: u32 = 1023;
const MENU_FONT_MISANS: u32 = 1031;
const MENU_FONT_YAHEI: u32 = 1032;
const MENU_METRIC_CPU: u32 = 1041;
const MENU_METRIC_MEMORY: u32 = 1042;
const MENU_METRIC_GPU: u32 = 1043;
const MENU_METRIC_DISK: u32 = 1044;
const MENU_EXIT: u32 = 1099;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    TogglePanel,
    ToggleTopmost,
    ToggleAutostart,
    SetInterval(u32),
    SetTheme(ThemeSetting),
    SetFontFamily(String),
    SetTrayMetric(TrayMetric),
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Severity {
    Normal,
    Warn,
    Critical,
}

pub struct Tray {
    hwnd: HWND,
    icon: Option<HICON>,
    added: bool,
    dpi: u32,
    icon_size: i32,
    last_bucket: Option<u8>,
    last_severity: Severity,
    last_tip: String,
    taskbar_created: u32,
    metric: TrayMetric,
}

impl Tray {
    pub fn new(hwnd: HWND, dpi: u32, metric: TrayMetric) -> Self {
        let name = to_wide("TaskbarCreated");
        let taskbar_created = unsafe {
            windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(PCWSTR(name.as_ptr()))
        };
        Self {
            hwnd,
            icon: None,
            added: false,
            dpi,
            icon_size: 0,
            last_bucket: None,
            last_severity: Severity::Normal,
            last_tip: String::new(),
            taskbar_created,
            metric,
        }
    }

    pub fn taskbar_created_message(&self) -> u32 {
        self.taskbar_created
    }

    pub fn set_metric(&mut self, metric: TrayMetric) {
        self.metric = metric;
        self.last_bucket = None;
    }

    pub fn rect(&self) -> Option<windows::Win32::Foundation::RECT> {
        let id = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: self.hwnd,
            uID: ICON_ID,
            ..Default::default()
        };
        unsafe { Shell_NotifyIconGetRect(&id) }.ok()
    }

    pub fn set_dpi(&mut self, dpi: u32) {
        if self.dpi != dpi {
            self.dpi = dpi;
            self.icon_size = 0;
            self.last_bucket = None;
        }
    }

    pub fn add(&mut self) -> bool {
        if self.added {
            return true;
        }
        if self.icon.is_none() {
            self.icon = render_ring(self.icon_size(), 0.0, Severity::Normal);
        }
        let Some(icon) = self.icon else {
            logging::log("could not render tray icon");
            return false;
        };

        let mut data = self.base_data();
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_APP_TRAY;
        data.hIcon = icon;
        self.copy_tip(&mut data, "LitePulse · 正在读取数据…");

        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.0 != 0;
        if !added {
            logging::log("Shell_NotifyIconW(NIM_ADD) failed");
            return false;
        }

        let mut version = self.base_data();
        version.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &version) };
        self.added = true;
        self.last_tip = "LitePulse · 正在读取数据…".to_string();
        true
    }

    pub fn update(&mut self, snapshot: Option<&Snapshot>) {
        if !self.added && !self.add() {
            return;
        }

        let (load, severity, tip) = match snapshot {
            Some(snapshot) => {
                let load = self.metric.load(snapshot).unwrap_or(0.0);
                let severity = severity(load);
                (load, severity, tray_tooltip(snapshot))
            }
            None => (
                0.0,
                Severity::Normal,
                "LitePulse · 正在读取数据…".to_string(),
            ),
        };

        let size = self.icon_size();
        let bucket = load.round().clamp(0.0, 100.0) as u8;
        let icon_changed = self.icon.is_none()
            || self.last_bucket != Some(bucket)
            || self.last_severity != severity
            || self.icon_size != size;
        let tip_changed = tip != self.last_tip;

        if !icon_changed && !tip_changed {
            return;
        }

        let new_icon = if icon_changed {
            render_ring(size, load, severity)
        } else {
            None
        };

        let mut data = self.base_data();
        data.uFlags = NIF_TIP;
        if let Some(icon) = new_icon {
            data.uFlags |= NIF_ICON;
            data.hIcon = icon;
        } else if let Some(icon) = self.icon {
            data.uFlags |= NIF_ICON;
            data.hIcon = icon;
        }
        self.copy_tip(&mut data, &tip);

        let updated = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) }.0 != 0;
        if !updated {
            if let Some(icon) = new_icon {
                unsafe {
                    let _ = DestroyIcon(icon);
                }
            }
            logging::log("Shell_NotifyIconW(NIM_MODIFY) failed; re-adding icon");
            self.added = false;
            if let Some(icon) = self.icon {
                unsafe {
                    let _ = DestroyIcon(icon);
                }
            }
            self.icon = None;
            let _ = self.add();
            return;
        }

        if let Some(icon) = new_icon {
            if let Some(old) = self.icon.replace(icon) {
                unsafe {
                    let _ = DestroyIcon(old);
                }
            }
            self.icon_size = size;
            self.last_bucket = Some(bucket);
            self.last_severity = severity;
        }
        if tip_changed {
            self.last_tip = tip;
        }
    }

    pub fn re_add(&mut self, snapshot: Option<&Snapshot>) {
        // Explorer restarted: the old icon is gone even though our HICON is
        // still valid, so force a NIM_ADD with the current icon.
        self.added = false;
        self.last_bucket = None;
        self.last_tip.clear();
        if self.add() {
            if let Some(snapshot) = snapshot {
                self.update(Some(snapshot));
            }
        }
    }

    pub fn remove(&mut self) {
        if self.added {
            let data = self.base_data();
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            self.added = false;
        }
        if let Some(icon) = self.icon.take() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
        self.last_bucket = None;
    }

    pub fn show_menu(
        &self,
        visible: bool,
        settings: &Settings,
        font_family: &str,
    ) -> Option<TrayCommand> {
        unsafe {
            let menu = CreatePopupMenu().ok()?;
            let toggle_text = if visible {
                "隐藏面板"
            } else {
                "显示面板"
            };
            append_item(menu, MF_STRING, MENU_TOGGLE as usize, toggle_text);

            let mut topmost_flags = MF_STRING;
            if settings.always_on_top {
                topmost_flags |= MF_CHECKED;
            }
            append_item(menu, topmost_flags, MENU_TOPMOST as usize, "总在最前");

            let mut autostart_flags = MF_STRING;
            if autostart::is_enabled() {
                autostart_flags |= MF_CHECKED;
            }
            append_item(
                menu,
                autostart_flags,
                MENU_AUTOSTART as usize,
                "开机自动启动",
            );

            append_separator(menu);

            let intervals = CreatePopupMenu().ok();
            let themes = CreatePopupMenu().ok();
            if let Some(intervals) = intervals {
                push_radio(
                    intervals,
                    MENU_INTERVAL_1S,
                    "1 秒",
                    settings.interval_ms == 1000,
                );
                push_radio(
                    intervals,
                    MENU_INTERVAL_2S,
                    "2 秒",
                    settings.interval_ms == 2000,
                );
                push_radio(
                    intervals,
                    MENU_INTERVAL_5S,
                    "5 秒",
                    settings.interval_ms == 5000,
                );
                let wide = to_wide("采样间隔");
                push_radio(
                    intervals,
                    MENU_INTERVAL_30S,
                    "30 秒",
                    settings.interval_ms == 30_000,
                );
                push_radio(
                    intervals,
                    MENU_INTERVAL_1M,
                    "1 分钟",
                    settings.interval_ms == 60_000,
                );
                let _ = AppendMenuW(menu, MF_POPUP, intervals.0 as usize, PCWSTR(wide.as_ptr()));
            }
            if let Some(themes) = themes {
                push_radio(
                    themes,
                    MENU_THEME_SYSTEM,
                    "跟随系统",
                    settings.theme == ThemeSetting::System,
                );
                push_radio(
                    themes,
                    MENU_THEME_DARK,
                    "深色",
                    settings.theme == ThemeSetting::Dark,
                );
                push_radio(
                    themes,
                    MENU_THEME_LIGHT,
                    "浅色",
                    settings.theme == ThemeSetting::Light,
                );
                let wide = to_wide("主题");
                let _ = AppendMenuW(menu, MF_POPUP, themes.0 as usize, PCWSTR(wide.as_ptr()));
            }
            if let Ok(fonts) = CreatePopupMenu() {
                let mut misans_flags = MF_STRING;
                if !font::family_available(font::DEFAULT_FAMILY) {
                    misans_flags |= MF_GRAYED;
                }
                if font_family.eq_ignore_ascii_case(font::DEFAULT_FAMILY) {
                    misans_flags |= MF_CHECKED;
                }
                append_item(
                    fonts,
                    misans_flags,
                    MENU_FONT_MISANS as usize,
                    font::DEFAULT_FAMILY,
                );

                let mut yahei_flags = MF_STRING;
                if font_family.eq_ignore_ascii_case(font::FALLBACK_FAMILY) {
                    yahei_flags |= MF_CHECKED;
                }
                append_item(fonts, yahei_flags, MENU_FONT_YAHEI as usize, "微软雅黑 UI");
                let wide = to_wide("字体");
                let _ = AppendMenuW(menu, MF_POPUP, fonts.0 as usize, PCWSTR(wide.as_ptr()));
            }

            if let Ok(metrics) = CreatePopupMenu() {
                for (id, metric) in [
                    (MENU_METRIC_CPU, TrayMetric::Cpu),
                    (MENU_METRIC_MEMORY, TrayMetric::Memory),
                    (MENU_METRIC_GPU, TrayMetric::Gpu),
                    (MENU_METRIC_DISK, TrayMetric::Disk),
                ] {
                    push_radio(metrics, id, metric.label(), settings.tray_metric == metric);
                }
                let wide = to_wide("托盘图标指标");
                let _ = AppendMenuW(menu, MF_POPUP, metrics.0 as usize, PCWSTR(wide.as_ptr()));
            }

            append_separator(menu);
            append_item(menu, MF_STRING, MENU_EXIT as usize, "退出 LitePulse");

            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let _ = SetForegroundWindow(self.hwnd);
            let flags =
                TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY;
            let command = TrackPopupMenu(menu, flags, point.x, point.y, None, self.hwnd, None);
            let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            let command = command.0 as u32;
            match command {
                MENU_TOGGLE => Some(TrayCommand::TogglePanel),
                MENU_TOPMOST => Some(TrayCommand::ToggleTopmost),
                MENU_AUTOSTART => Some(TrayCommand::ToggleAutostart),
                MENU_INTERVAL_1S => Some(TrayCommand::SetInterval(1000)),
                MENU_INTERVAL_2S => Some(TrayCommand::SetInterval(2000)),
                MENU_INTERVAL_5S => Some(TrayCommand::SetInterval(5000)),
                MENU_INTERVAL_30S => Some(TrayCommand::SetInterval(30_000)),
                MENU_INTERVAL_1M => Some(TrayCommand::SetInterval(60_000)),
                MENU_THEME_SYSTEM => Some(TrayCommand::SetTheme(ThemeSetting::System)),
                MENU_THEME_DARK => Some(TrayCommand::SetTheme(ThemeSetting::Dark)),
                MENU_THEME_LIGHT => Some(TrayCommand::SetTheme(ThemeSetting::Light)),
                MENU_FONT_MISANS => {
                    Some(TrayCommand::SetFontFamily(font::DEFAULT_FAMILY.to_string()))
                }
                MENU_FONT_YAHEI => Some(TrayCommand::SetFontFamily(
                    font::FALLBACK_FAMILY.to_string(),
                )),
                MENU_EXIT => Some(TrayCommand::Exit),
                MENU_METRIC_CPU => Some(TrayCommand::SetTrayMetric(TrayMetric::Cpu)),
                MENU_METRIC_MEMORY => Some(TrayCommand::SetTrayMetric(TrayMetric::Memory)),
                MENU_METRIC_GPU => Some(TrayCommand::SetTrayMetric(TrayMetric::Gpu)),
                MENU_METRIC_DISK => Some(TrayCommand::SetTrayMetric(TrayMetric::Disk)),
                _ => None,
            }
        }
    }

    fn base_data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ICON_ID,
            ..Default::default()
        }
    }

    fn copy_tip(&self, data: &mut NOTIFYICONDATAW, tip: &str) {
        let tip = truncate_utf16(tip, 127);
        let units: Vec<u16> = tip.encode_utf16().collect();
        data.szTip.fill(0);
        for (index, unit) in units.into_iter().take(127).enumerate() {
            data.szTip[index] = unit;
        }
        data.szTip[127] = 0;
    }

    fn icon_size(&self) -> i32 {
        let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, self.dpi) };
        if size <= 0 { 16 } else { size }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}

fn append_item(
    menu: HMENU,
    flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS,
    id: usize,
    text: &str,
) {
    let wide = to_wide(text);
    unsafe {
        let _ = AppendMenuW(menu, flags, id, PCWSTR(wide.as_ptr()));
    }
}

fn append_separator(menu: HMENU) {
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
}

fn push_radio(menu: HMENU, id: u32, text: &str, checked: bool) {
    let mut flags = MF_STRING;
    if checked {
        flags |= MF_CHECKED;
    }
    append_item(menu, flags, id as usize, text);
}

fn severity(load: f32) -> Severity {
    if load >= 90.0 {
        Severity::Critical
    } else if load >= 70.0 {
        Severity::Warn
    } else {
        Severity::Normal
    }
}

fn render_ring(size: i32, load: f32, severity: Severity) -> Option<HICON> {
    let n = size.max(8);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: n,
            biHeight: n,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut pixels: *mut c_void = null_mut();
    let color =
        unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut pixels, None, 0) }.ok()?;
    if pixels.is_null() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        return None;
    }

    let color_bytes: &mut [u8] =
        unsafe { std::slice::from_raw_parts_mut(pixels as *mut u8, (n * n * 4) as usize) };
    color_bytes.fill(0);

    let fill_color = match severity {
        Severity::Normal => [92.0, 157.0, 255.0],
        Severity::Warn => [245.0, 168.0, 55.0],
        Severity::Critical => [239.0, 88.0, 88.0],
    };
    let track_color = [150.0, 158.0, 170.0];
    let track_alpha = 0.42;
    let fraction = (load / 100.0).clamp(0.0, 1.0);
    let center = n as f32 / 2.0;
    let outer = center - 0.7;
    let inner = (outer - (n as f32 * 0.15).max(2.0)).max(0.0);

    let mask_stride = ((n + 31) / 32 * 4) as usize;
    let mut mask = vec![0u8; mask_stride * n as usize];

    for y in 0..n {
        for x in 0..n {
            let mut premul = [0.0_f32; 3];
            let mut alpha_sum = 0.0_f32;
            for sample_y in 0..4 {
                let py = y as f32 + (sample_y as f32 + 0.5) / 4.0;
                for sample_x in 0..4 {
                    let px = x as f32 + (sample_x as f32 + 0.5) / 4.0;
                    let dx = px - center;
                    let dy = py - center;
                    let distance = (dx * dx + dy * dy).sqrt();
                    if distance < inner || distance > outer {
                        continue;
                    }
                    let mut degrees = dx.atan2(-dy).to_degrees();
                    if degrees < 0.0 {
                        degrees += 360.0;
                    }
                    let (color, alpha) = if degrees <= fraction * 360.0 {
                        (fill_color, 1.0)
                    } else {
                        (track_color, track_alpha)
                    };
                    premul[0] += color[0] * alpha;
                    premul[1] += color[1] * alpha;
                    premul[2] += color[2] * alpha;
                    alpha_sum += alpha;
                }
            }
            if alpha_sum <= 0.0 {
                continue;
            }
            let alpha = (alpha_sum / 16.0 * 255.0).round().clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                continue;
            }
            let red = (premul[0] / alpha_sum).round().clamp(0.0, 255.0) as u8;
            let green = (premul[1] / alpha_sum).round().clamp(0.0, 255.0) as u8;
            let blue = (premul[2] / alpha_sum).round().clamp(0.0, 255.0) as u8;

            // DIB sections are bottom-up unless given a negative height.
            let row = (n - 1 - y) as usize;
            let offset = (row * n as usize + x as usize) * 4;
            color_bytes[offset] = blue;
            color_bytes[offset + 1] = green;
            color_bytes[offset + 2] = red;
            color_bytes[offset + 3] = alpha;

            if alpha < 16 {
                let byte = row * mask_stride + (x as usize) / 8;
                let bit = 7 - (x as usize % 8);
                mask[byte] |= 1 << bit;
            }
        }
    }

    let mask_bitmap = unsafe { CreateBitmap(n, n, 1, 1, Some(mask.as_ptr() as *const c_void)) };
    if mask_bitmap.0.is_null() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        return None;
    }

    let icon_info = ICONINFO {
        fIcon: TRUE,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bitmap,
        hbmColor: color,
    };
    let icon = unsafe { CreateIconIndirect(&icon_info) }.ok();
    unsafe {
        let _ = DeleteObject(HGDIOBJ(mask_bitmap.0));
        let _ = DeleteObject(HGDIOBJ(color.0));
    }
    icon
}
