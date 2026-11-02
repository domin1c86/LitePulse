mod icons;
pub mod layout;
pub mod paint;
mod preview;
mod settings;
pub mod theme;
#[cfg(test)]
mod visual_tests;

use std::ffi::c_void;
use std::mem::size_of;
use std::time::Instant;

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, DeleteObject, EndPaint, GetMonitorInfoW, HGDIOBJ,
    InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, PAINTSTRUCT, ScreenToClient, SetWindowRgn,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::Shell::NIN_SELECT;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetClientRect, GetCursorPos, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
    HTCAPTION, HTCLIENT, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, IDC_HAND, LoadCursorW,
    MB_ICONERROR, MB_OK, MSG, MessageBoxW, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND,
    PostQuitMessage, RegisterClassExW, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOW, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetCursor, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_KEYDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_PAINT, WM_POWERBROADCAST, WM_QUERYENDSESSION,
    WM_RBUTTONUP, WM_SETCURSOR, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WNDCLASSEXW,
    WS_EX_TOOLWINDOW, WS_POPUP, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};
use windows::core::PCWSTR;

use crate::config::Settings;
use crate::history::History;
use crate::metrics::Snapshot;
use crate::sampler::SamplerHandle;
use crate::tray::{Tray, TrayCommand};
use crate::ui::layout::{Layout, Tab, panel_size};
use crate::ui::paint::{Hover, PanelView, Renderer};
use crate::ui::preview::Preview;
use crate::win32::{
    WINDOW_CLASS, WINDOW_TITLE, WM_APP_SAMPLE, WM_APP_SHOW, WM_APP_TRAY, autostart, dpi, font,
    logging, to_wide,
};

const NIN_KEYSELECT: u32 = 0x0401;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_NCMOUSEMOVE: u32 = 0x00A0;
const VK_ESCAPE: usize = 0x1B;
const NIN_POPUPOPEN: u32 = 0x0406;
const NIN_POPUPCLOSE: u32 = 0x0407;

pub fn run(startup_hidden: bool, settings: Settings) -> i32 {
    unsafe {
        let class_name = to_wide(WINDOW_CLASS);
        let title = to_wide(WINDOW_TITLE);
        let module = match GetModuleHandleW(PCWSTR::null()) {
            Ok(module) => module,
            Err(err) => {
                logging::log(&format!("GetModuleHandleW failed: {err}"));
                return 1;
            }
        };
        let hinstance = HINSTANCE(module.0);
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance,
            hCursor: cursor,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };

        if RegisterClassExW(&class) == 0 {
            let error = GetLastError();
            if error != ERROR_CLASS_ALREADY_EXISTS {
                logging::log(&format!("RegisterClassExW failed: win32 error {}", error.0));
                fatal("无法注册窗口类，LitePulse 不能启动。");
                return 1;
            }
        }

        let initial_dpi = dpi::system();
        let (width, height) = panel_size(initial_dpi);
        // Test builds expose a taskbar window so desktop automation can bind
        // this otherwise tool-only panel. Production keeps its tray behavior.
        let ex_style = if cfg!(test) {
            windows::Win32::UI::WindowsAndMessaging::WS_EX_APPWINDOW
        } else {
            WS_EX_TOOLWINDOW
        };
        let hwnd = match CreateWindowExW(
            ex_style,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            width,
            height,
            None,
            None,
            None,
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(err) => {
                logging::log(&format!("CreateWindowExW failed: {err}"));
                fatal("无法创建 LitePulse 窗口。");
                return 1;
            }
        };

        let mut app = Box::new(App::new(settings, startup_hidden));
        let app_ptr = &mut *app as *mut App;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize);
        app.attach(hwnd);

        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0);
            if result.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        drop(app);
        let _ = DestroyWindow(hwnd);
    }
    0
}

fn fatal(text: &str) {
    let text = to_wide(text);
    let caption = to_wide("LitePulse");
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_ICONERROR | MB_OK,
        );
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut App;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let app = unsafe { &mut *pointer };
    match app.handle(hwnd, message, wparam, lparam) {
        Some(result) => result,
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

struct App {
    hwnd: HWND,
    settings: Settings,
    sampler: Option<SamplerHandle>,
    tray: Option<Tray>,
    history: History,
    snapshot: Option<Snapshot>,
    renderer: Renderer,
    font_family: String,
    visible: bool,
    dpi: u32,
    hover: Hover,
    mouse_tracking: bool,
    hand_cursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR,
    last_toggle: Option<Instant>,
    startup_hidden: bool,
    session_notifications: bool,
    taskbar_created: u32,
    cleaned_up: bool,
    tab: Tab,
    preview: Option<Box<Preview>>,
    settings_open: bool,
    autostart_enabled: bool,
    misans_available: bool,
}

impl App {
    fn new(settings: Settings, startup_hidden: bool) -> Self {
        Self {
            hwnd: HWND(std::ptr::null_mut()),
            settings,
            sampler: None,
            tray: None,
            history: History::new(),
            snapshot: None,
            renderer: Renderer::new(),
            font_family: String::new(),
            visible: false,
            dpi: 96,
            hover: Hover::None,
            mouse_tracking: false,
            hand_cursor: Default::default(),
            last_toggle: None,
            startup_hidden,
            session_notifications: false,
            taskbar_created: 0,
            cleaned_up: false,
            tab: Tab::Cpu,
            preview: None,
            settings_open: false,
            autostart_enabled: autostart::is_enabled(),
            misans_available: font::family_available(font::DEFAULT_FAMILY),
        }
    }

    fn attach(&mut self, hwnd: HWND) {
        self.hwnd = hwnd;
        self.dpi = dpi::for_window(hwnd);
        self.hand_cursor = unsafe { LoadCursorW(None, IDC_HAND) }.unwrap_or_default();
        self.font_family = font::resolve(&self.settings.font_family);
        if !self
            .font_family
            .eq_ignore_ascii_case(&self.settings.font_family)
        {
            logging::log(&format!(
                "font \"{}\" unavailable; using \"{}\"",
                self.settings.font_family, self.font_family
            ));
        } else {
            logging::log(&format!("font family: {}", self.font_family));
        }
        self.place_initially();
        self.apply_window_style();

        self.preview = Preview::new(hwnd);
        let mut tray = Tray::new(hwnd, self.dpi, self.settings.tray_metric);
        if !tray.add() {
            logging::log("failed to add tray icon");
        }
        tray.update(None);
        self.taskbar_created = tray.taskbar_created_message();
        self.tray = Some(tray);

        self.sampler = Some(SamplerHandle::start(
            hwnd.0 as isize,
            self.settings.interval_ms,
        ));

        if unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }.is_ok() {
            self.session_notifications = true;
        } else {
            logging::log("WTSRegisterSessionNotification failed");
        }

        autostart::ensure_current();
        if !self.startup_hidden {
            self.show_panel();
        }
        logging::log(&format!(
            "attached (dpi={}, startup_hidden={})",
            self.dpi, self.startup_hidden
        ));
    }

    fn place_initially(&mut self) {
        let (width, height) = panel_size(self.dpi);
        let (x, y) = initial_position(&self.settings, width, height, self.dpi);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    fn apply_window_style(&mut self) {
        let preference = DWMWCP_ROUND;
        let result = unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
                size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            )
        };
        if result.is_err() {
            self.apply_window_region();
        }
    }

    fn apply_window_region(&self) {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        if rect.right <= 0 || rect.bottom <= 0 {
            return;
        }
        let radius = dpi::scale(12.0, self.dpi);
        let region = unsafe {
            CreateRoundRectRgn(
                0,
                0,
                rect.right + 1,
                rect.bottom + 1,
                radius * 2,
                radius * 2,
            )
        };
        if region.0.is_null() {
            return;
        }
        let applied = unsafe { SetWindowRgn(self.hwnd, Some(region), true) };
        if applied == 0 {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(region.0));
            }
        }
    }

    fn apply_topmost(&self) {
        let insert_after = if self.settings.always_on_top {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        let result = unsafe {
            SetWindowPos(
                self.hwnd,
                Some(insert_after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
        if let Err(err) = result {
            logging::log(&format!(
                "SetWindowPos(topmost={}) failed: {err}",
                self.settings.always_on_top
            ));
        }
    }

    fn show_panel(&mut self) {
        self.hide_preview();
        self.ensure_on_screen();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
        }
        // Apply topmost after showing the window; some DWM configurations
        // ignore the z-order change while a popup is still hidden.
        self.apply_topmost();
        unsafe {
            let _ = SetForegroundWindow(self.hwnd);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        self.visible = true;
        logging::log(&format!(
            "panel shown (topmost={})",
            self.settings.always_on_top
        ));
    }

    fn hide_panel(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
        self.renderer.release_backbuffer();
        self.renderer.invalidate_fonts();
        self.save_position();
        self.save_settings();
        logging::log("panel hidden");
    }

    fn toggle_panel(&mut self) {
        if self.visible {
            self.hide_panel();
        } else {
            self.show_panel();
        }
    }

    fn toggle_panel_debounced(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_toggle {
            if now.duration_since(last).as_millis() < 350 {
                return;
            }
        }
        self.last_toggle = Some(now);
        self.toggle_panel();
    }

    fn ensure_on_screen(&mut self) {
        let mut rect = RECT::default();
        unsafe {
            if GetWindowRect(self.hwnd, &mut rect).is_err() {
                return;
            }
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return;
        }
        let point = POINT {
            x: rect.left + width / 2,
            y: rect.top + height / 2,
        };
        let work = work_area_near(Some(point));
        let (x, y) = clamp_position(rect.left, rect.top, width, height, work);
        if x != rect.left || y != rect.top {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    fn save_position(&mut self) {
        let mut rect = RECT::default();
        unsafe {
            if GetWindowRect(self.hwnd, &mut rect).is_ok() {
                self.settings.window_x = Some(rect.left);
                self.settings.window_y = Some(rect.top);
            }
        }
    }

    fn save_settings(&self) {
        if cfg!(test) {
            return;
        }
        if let Err(err) = self.settings.save() {
            logging::log(&format!("could not save settings: {err}"));
        }
    }

    fn on_sample(&mut self) {
        if let Some(snapshot) = self.sampler.as_ref().and_then(|sampler| sampler.latest()) {
            self.history.push(&snapshot);
            self.snapshot = Some(snapshot);
        }
        if let Some(tray) = self.tray.as_mut() {
            tray.update(self.snapshot.as_ref());
        }
        if self.visible {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
        self.update_preview();
    }

    fn update_preview(&mut self) {
        if let Some(preview) = self.preview.as_mut() {
            if preview.visible {
                preview.update(self.snapshot.as_ref(), &self.settings, &self.font_family);
            }
        }
    }

    fn hide_preview(&mut self) {
        if let Some(preview) = self.preview.as_mut() {
            if preview.visible {
                preview.hide();
            }
        }
    }

    fn show_preview(&mut self) {
        let anchor = self
            .tray
            .as_ref()
            .and_then(|t| t.rect())
            .unwrap_or_else(|| {
                let mut point = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut point);
                }
                RECT {
                    left: point.x,
                    top: point.y,
                    right: point.x + 1,
                    bottom: point.y + 1,
                }
            });
        let work = work_area_near(Some(POINT {
            x: anchor.left,
            y: anchor.top,
        }));
        if let Some(preview) = self.preview.as_mut() {
            preview.update(self.snapshot.as_ref(), &self.settings, &self.font_family);
            preview.show(
                anchor,
                work,
                dpi::at_point(POINT {
                    x: anchor.left,
                    y: anchor.top,
                }),
            );
        }
    }

    fn paint(&mut self) {
        let mut paint = PAINTSTRUCT::default();
        let dc = unsafe { BeginPaint(self.hwnd, &mut paint) };
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        let view = PanelView {
            snapshot: self.snapshot.as_ref(),
            history: &self.history,
            always_on_top: self.settings.always_on_top,
            theme: theme::resolve(self.settings.theme),
            font_family: &self.font_family,
            hover: self.hover,
            tab: self.tab,
            interval_ms: self.settings.interval_ms,
            preview: false,
            settings: self.settings_open.then_some(&self.settings),
            autostart: self.autostart_enabled,
            misans: self.misans_available,
            glass: false,
        };
        self.renderer
            .paint(dc, rect.right, rect.bottom, self.dpi, &view);
        unsafe {
            let _ = EndPaint(self.hwnd, &paint);
        }
    }

    fn custom_message(&mut self, message: u32) -> Option<LRESULT> {
        if message == WM_APP_SAMPLE {
            self.on_sample();
            return Some(LRESULT(0));
        }
        if message == WM_APP_SHOW {
            self.show_panel();
            return Some(LRESULT(0));
        }
        if self.taskbar_created != 0 && message == self.taskbar_created {
            let snapshot = self.snapshot.clone();
            if let Some(tray) = self.tray.as_mut() {
                tray.re_add(snapshot.as_ref());
            }
            return Some(LRESULT(0));
        }
        None
    }

    fn handle(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        self.hwnd = hwnd;
        if let Some(result) = self.custom_message(message) {
            return Some(result);
        }
        match message {
            WM_PAINT => {
                self.paint();
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_MOUSEMOVE => {
                self.on_mouse_move(lparam);
                Some(LRESULT(0))
            }
            WM_NCMOUSEMOVE => {
                if self.hover != Hover::None {
                    self.hover = Hover::None;
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.mouse_tracking = false;
                if self.hover != Hover::None {
                    self.hover = Hover::None;
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let point = lparam_point(lparam);
                let layout = Layout::new(self.client_width(), self.client_height(), self.dpi);
                if Layout::contains(&layout.close, point.x, point.y) {
                    self.hide_panel();
                } else if Layout::contains(&layout.pin, point.x, point.y) {
                    self.toggle_topmost();
                } else if Layout::contains(&layout.settings, point.x, point.y) {
                    self.settings_open = !self.settings_open;
                    self.autostart_enabled = autostart::is_enabled();
                    self.hover = Hover::Settings;
                    self.invalidate();
                } else if self.settings_open {
                    if let Some(command) = settings::command_at(
                        &layout,
                        &self.settings,
                        self.autostart_enabled,
                        self.misans_available,
                        point.x,
                        point.y,
                    ) {
                        self.handle_tray_command(command);
                    }
                } else if let Some(tab) = layout.tab_at(point.x, point.y) {
                    self.tab = tab;
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                #[cfg(test)]
                if wparam.0 == 0x77 {
                    // F8 exercises the exact shell hover callback in the
                    // interactive check, without synthesizing OS input.
                    let event = if self.preview.as_ref().is_some_and(|p| p.visible) {
                        NIN_POPUPCLOSE
                    } else {
                        NIN_POPUPOPEN
                    };
                    return self.handle_tray_callback(LPARAM(event as isize));
                }
                if wparam.0 == VK_ESCAPE {
                    if self.settings_open {
                        self.settings_open = false;
                        self.hover = Hover::None;
                        self.invalidate();
                    } else {
                        self.hide_panel();
                    }
                } else if !self.settings_open && (wparam.0 == 0x25 || wparam.0 == 0x27) {
                    let index = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
                    let delta = if wparam.0 == 0x27 { 1 } else { 4 };
                    self.tab = Tab::ALL[(index + delta) % 5];
                    self.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_NCHITTEST => {
                let point = lparam_point(lparam);
                let mut client = point;
                unsafe {
                    let _ = ScreenToClient(self.hwnd, &mut client);
                }
                let layout = Layout::new(self.client_width(), self.client_height(), self.dpi);
                if Layout::contains(&layout.close, client.x, client.y)
                    || Layout::contains(&layout.pin, client.x, client.y)
                    || Layout::contains(&layout.settings, client.x, client.y)
                {
                    Some(LRESULT(HTCLIENT as isize))
                } else if client.y < layout.header_bottom {
                    Some(LRESULT(HTCAPTION as isize))
                } else {
                    Some(LRESULT(HTCLIENT as isize))
                }
            }
            WM_SETCURSOR => {
                if self.cursor_over_button() {
                    if !self.hand_cursor.0.is_null() {
                        unsafe {
                            SetCursor(Some(self.hand_cursor));
                        }
                    }
                    Some(LRESULT(1))
                } else {
                    None
                }
            }
            WM_CLOSE => {
                self.hide_panel();
                Some(LRESULT(0))
            }
            WM_CONTEXTMENU | WM_RBUTTONUP => {
                self.show_tray_menu();
                Some(LRESULT(0))
            }
            WM_DESTROY => {
                unsafe {
                    PostQuitMessage(0);
                }
                Some(LRESULT(0))
            }
            WM_EXITSIZEMOVE => {
                self.save_position();
                self.save_settings();
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                self.on_dpi_changed(wparam, lparam);
                Some(LRESULT(0))
            }
            WM_DISPLAYCHANGE => {
                self.dpi = dpi::for_window(self.hwnd);
                if let Some(tray) = self.tray.as_mut() {
                    tray.set_dpi(self.dpi);
                }
                self.ensure_on_screen();
                self.invalidate();
                Some(LRESULT(0))
            }
            WM_SETTINGCHANGE => {
                self.invalidate();
                self.update_preview();
                Some(LRESULT(0))
            }
            WM_POWERBROADCAST => {
                let event = wparam.0 as u32;
                if event == PBT_APMRESUMEAUTOMATIC || event == PBT_APMRESUMESUSPEND {
                    if let Some(sampler) = self.sampler.as_ref() {
                        sampler.request_reset();
                    }
                    self.invalidate();
                    return Some(LRESULT(1));
                }
                None
            }
            WM_WTSSESSION_CHANGE => {
                let event = wparam.0 as u32;
                if let Some(sampler) = self.sampler.as_ref() {
                    if event == WTS_SESSION_LOCK {
                        sampler.set_paused(true);
                    } else if event == WTS_SESSION_UNLOCK {
                        sampler.set_paused(false);
                    }
                }
                Some(LRESULT(0))
            }
            WM_QUERYENDSESSION => {
                self.save_position();
                self.save_settings();
                Some(LRESULT(1))
            }
            WM_ENDSESSION => {
                if wparam.0 != 0 {
                    self.save_position();
                    self.save_settings();
                    unsafe {
                        PostQuitMessage(0);
                    }
                }
                Some(LRESULT(0))
            }
            WM_APP_TRAY => self.handle_tray_callback(lparam),
            _ => None,
        }
    }

    fn handle_tray_callback(&mut self, lparam: LPARAM) -> Option<LRESULT> {
        let event = (lparam.0 as u32) & 0xFFFF;
        if event == NIN_POPUPOPEN {
            self.show_preview();
        } else if event == NIN_POPUPCLOSE {
            self.hide_preview();
        } else if event == WM_CONTEXTMENU || event == WM_RBUTTONUP {
            self.hide_preview();
            self.show_tray_menu();
        } else if event == NIN_SELECT || event == NIN_KEYSELECT || event == WM_LBUTTONUP {
            self.toggle_panel_debounced();
        }
        Some(LRESULT(0))
    }

    fn show_tray_menu(&mut self) {
        self.autostart_enabled = autostart::is_enabled();
        let command = self
            .tray
            .as_ref()
            .and_then(|tray| tray.show_menu(self.visible, &self.settings, &self.font_family));
        if let Some(command) = command {
            self.handle_tray_command(command);
        }
    }

    fn handle_tray_command(&mut self, command: TrayCommand) {
        match command {
            TrayCommand::TogglePanel => self.toggle_panel(),
            TrayCommand::ToggleTopmost => self.toggle_topmost(),
            TrayCommand::ToggleAutostart => {
                let enabled = !self.autostart_enabled;
                if cfg!(test) || autostart::set_enabled(enabled) {
                    self.autostart_enabled = enabled;
                    logging::log(&format!(
                        "autostart {}",
                        if enabled { "enabled" } else { "disabled" }
                    ));
                }
            }
            TrayCommand::SetInterval(interval_ms) => {
                if self.settings.interval_ms != interval_ms {
                    self.history = History::new();
                }
                self.settings.interval_ms = interval_ms;
                self.settings.sanitize();
                if let Some(sampler) = self.sampler.as_ref() {
                    sampler.set_interval_ms(self.settings.interval_ms);
                }
                self.save_settings();
            }
            TrayCommand::SetTheme(theme_setting) => {
                self.settings.theme = theme_setting;
                self.save_settings();
                self.invalidate();
            }
            TrayCommand::SetFontFamily(family) => {
                self.font_family = font::resolve(&family);
                self.settings.font_family = family;
                self.renderer.invalidate_fonts();
                self.save_settings();
                self.invalidate();
                logging::log(&format!("font family set to {}", self.font_family));
            }
            TrayCommand::SetTrayMetric(metric) => {
                self.settings.tray_metric = metric;
                if let Some(tray) = self.tray.as_mut() {
                    tray.set_metric(metric);
                    tray.update(self.snapshot.as_ref());
                }
                self.save_settings();
            }
            TrayCommand::Exit => {
                self.save_position();
                self.save_settings();
                if let Some(tray) = self.tray.as_mut() {
                    tray.remove();
                }
                unsafe {
                    PostQuitMessage(0);
                }
            }
        }
        self.invalidate();
        self.update_preview();
    }

    fn toggle_topmost(&mut self) {
        self.settings.always_on_top = !self.settings.always_on_top;
        self.apply_topmost();
        self.save_settings();
        self.invalidate();
    }

    fn on_mouse_move(&mut self, lparam: LPARAM) {
        let point = lparam_point(lparam);
        let layout = Layout::new(self.client_width(), self.client_height(), self.dpi);
        let hover = if Layout::contains(&layout.pin, point.x, point.y) {
            Hover::Pin
        } else if Layout::contains(&layout.close, point.x, point.y) {
            Hover::Close
        } else if Layout::contains(&layout.settings, point.x, point.y) {
            Hover::Settings
        } else if self.settings_open {
            settings::choices(
                &layout,
                &self.settings,
                self.autostart_enabled,
                self.misans_available,
            )
            .iter()
            .position(|c| c.enabled && Layout::contains(&c.rect, point.x, point.y))
            .map(Hover::Choice)
            .unwrap_or(Hover::None)
        } else if let Some(tab) = layout.tab_at(point.x, point.y) {
            Hover::Tab(tab)
        } else {
            Hover::None
        };
        if hover != self.hover {
            self.hover = hover;
            self.invalidate();
        }
        if !self.mouse_tracking {
            let mut event = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut event);
            }
            self.mouse_tracking = true;
        }
    }

    fn on_dpi_changed(&mut self, wparam: WPARAM, lparam: LPARAM) {
        let new_dpi = ((wparam.0 >> 16) & 0xFFFF) as u32;
        if new_dpi == 0 {
            return;
        }
        self.dpi = new_dpi;
        if let Some(tray) = self.tray.as_mut() {
            tray.set_dpi(new_dpi);
        }
        self.renderer.invalidate_fonts();
        self.renderer.release_backbuffer();

        let (width, height) = panel_size(new_dpi);
        let suggested = if lparam.0 != 0 {
            unsafe { *(lparam.0 as *const RECT) }
        } else {
            RECT::default()
        };
        let work = work_area_near(Some(POINT {
            x: suggested.left + width / 2,
            y: suggested.top + height / 2,
        }));
        let (x, y) = clamp_position(suggested.left, suggested.top, width, height, work);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.apply_window_style();
        self.invalidate();
    }

    fn cursor_over_button(&self) -> bool {
        let mut point = POINT::default();
        unsafe {
            if GetCursorPos(&mut point).is_err() {
                return false;
            }
            let _ = ScreenToClient(self.hwnd, &mut point);
        }
        let layout = Layout::new(self.client_width(), self.client_height(), self.dpi);
        Layout::contains(&layout.pin, point.x, point.y)
            || Layout::contains(&layout.close, point.x, point.y)
            || Layout::contains(&layout.settings, point.x, point.y)
            || if self.settings_open {
                settings::command_at(
                    &layout,
                    &self.settings,
                    self.autostart_enabled,
                    self.misans_available,
                    point.x,
                    point.y,
                )
                .is_some()
            } else {
                layout.tab_at(point.x, point.y).is_some()
            }
    }

    fn client_width(&self) -> i32 {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        rect.right
    }

    fn client_height(&self) -> i32 {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        rect.bottom
    }

    fn invalidate(&self) {
        if self.visible {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }

    fn cleanup(&mut self) {
        if self.cleaned_up {
            return;
        }
        self.cleaned_up = true;
        self.preview.take();
        self.save_position();
        self.save_settings();
        if let Some(mut tray) = self.tray.take() {
            tray.remove();
        }
        if let Some(mut sampler) = self.sampler.take() {
            sampler.stop();
        }
        if self.session_notifications {
            unsafe {
                let _ = WTSUnRegisterSessionNotification(self.hwnd);
            }
        }
        self.renderer.destroy();
        logging::log("shutdown complete");
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn lparam_point(lparam: LPARAM) -> POINT {
    let packed = lparam.0 as i32;
    POINT {
        x: (packed & 0xFFFF) as i16 as i32,
        y: ((packed >> 16) & 0xFFFF) as i16 as i32,
    }
}

fn work_area_near(point: Option<POINT>) -> RECT {
    let monitor = match point {
        Some(point) => unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) },
        None => unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) },
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let got = unsafe { GetMonitorInfoW(monitor, &mut info) };
    if got.0 != 0 {
        return info.rcWork;
    }
    // Extremely unlikely fallback: the primary screen rectangle.
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    RECT {
        left: 0,
        top: 0,
        right: width.max(1),
        bottom: height.max(1),
    }
}

fn initial_position(settings: &Settings, width: i32, height: i32, dpi: u32) -> (i32, i32) {
    if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
        let work = work_area_near(Some(POINT {
            x: x + width / 2,
            y: y + height / 2,
        }));
        clamp_position(x, y, width, height, work)
    } else {
        let work = work_area_near(None);
        let margin = dpi::scale(12.0, dpi);
        let x = work.right - width - margin;
        let y = work.bottom - height - margin;
        clamp_position(x, y, width, height, work)
    }
}

fn clamp_position(x: i32, y: i32, width: i32, height: i32, work: RECT) -> (i32, i32) {
    let max_x = (work.right - width).max(work.left);
    let max_y = (work.bottom - height).max(work.top);
    (x.clamp(work.left, max_x), y.clamp(work.top, max_y))
}
