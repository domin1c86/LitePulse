use super::{
    layout::{PREVIEW_H_DIP, PREVIEW_W_DIP, Tab},
    paint::{Hover, PanelView, Renderer},
    theme,
};
use crate::{
    config::Settings,
    history::History,
    metrics::Snapshot,
    win32::{dpi, to_wide},
};
use std::mem::size_of;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

pub struct Preview {
    hwnd: HWND,
    renderer: Renderer,
    snapshot: Option<Snapshot>,
    history: History,
    settings: Settings,
    family: String,
    dpi: u32,
    pub visible: bool,
}
impl Preview {
    pub fn new(owner: HWND) -> Option<Box<Self>> {
        let class = to_wide("LitePulsePreview");
        let title = to_wide("LitePulse 概览");
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?;
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        unsafe {
            RegisterClassExW(&wc);
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                PCWSTR(class.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_POPUP | WS_BORDER,
                0,
                0,
                1,
                1,
                Some(owner),
                None,
                None,
                None,
            )
        }
        .ok()?;
        let mut preview = Box::new(Self {
            hwnd,
            renderer: Renderer::new(),
            snapshot: None,
            history: History::new(),
            settings: Settings::default(),
            family: String::new(),
            dpi: 96,
            visible: false,
        });
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *preview as *mut Self) as isize);
        }
        Some(preview)
    }
    pub fn update(&mut self, snapshot: Option<&Snapshot>, settings: &Settings, family: &str) {
        self.snapshot = snapshot.cloned();
        self.settings = settings.clone();
        self.family = family.into();
        if self.visible {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }
    pub fn show(&mut self, anchor: RECT, work: RECT, dpi: u32) {
        self.dpi = dpi;
        let w = dpi::scale(PREVIEW_W_DIP, dpi);
        let h = dpi::scale(PREVIEW_H_DIP, dpi);
        let gap = dpi::scale(8.0, dpi);
        let x = (anchor.left + (anchor.right - anchor.left) / 2 - w / 2)
            .clamp(work.left, (work.right - w).max(work.left));
        let y = if anchor.top - gap - h >= work.top {
            anchor.top - gap - h
        } else {
            anchor.bottom + gap
        };
        let y = y.clamp(work.top, (work.bottom - h).max(work.top));
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        self.visible = true;
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    pub fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
        self.renderer.release_backbuffer();
        self.renderer.invalidate_fonts();
    }
    fn paint(&mut self) {
        let mut ps = PAINTSTRUCT::default();
        let mut r = RECT::default();
        let dc = unsafe { BeginPaint(self.hwnd, &mut ps) };
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut r);
        }
        let view = PanelView {
            snapshot: self.snapshot.as_ref(),
            history: &self.history,
            always_on_top: false,
            theme: theme::resolve(self.settings.theme),
            font_family: &self.family,
            hover: Hover::None,
            tab: Tab::Cpu,
            interval_ms: self.settings.interval_ms,
            preview: true,
        };
        self.renderer.paint(dc, r.right, r.bottom, self.dpi, &view);
        unsafe {
            let _ = EndPaint(self.hwnd, &ps);
        }
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Preview;
    if !pointer.is_null() {
        let preview = unsafe { &mut *pointer };
        if message == WM_PAINT {
            preview.paint();
            return LRESULT(0);
        }
        if message == WM_ERASEBKGND {
            return LRESULT(1);
        }
        if message == WM_MOUSEACTIVATE {
            return LRESULT(MA_NOACTIVATE as isize);
        }
    }
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}
