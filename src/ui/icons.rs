use super::layout::Tab;
use std::mem::size_of;
use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HALFTONE, HDC, SRCCOPY, STRETCH_BLT_MODE,
    SetBrushOrgEx, SetStretchBltMode, StretchDIBits,
};

#[derive(Clone, Copy)]
pub enum Icon {
    Cpu,
    Memory,
    Gpu,
    Disk,
    Up,
    Down,
    Pin,
    Close,
    Settings,
    Network,
}
impl From<Tab> for Icon {
    fn from(tab: Tab) -> Self {
        match tab {
            Tab::Cpu => Self::Cpu,
            Tab::Memory => Self::Memory,
            Tab::Gpu => Self::Gpu,
            Tab::Disk => Self::Disk,
            Tab::Network => Self::Network,
        }
    }
}
pub fn draw(dc: HDC, rect: RECT, icon: Icon, color: COLORREF, bg: COLORREF) {
    if matches!(icon, Icon::Network) {
        let middle = (rect.left + rect.right) / 2;
        draw(
            dc,
            RECT {
                right: middle,
                ..rect
            },
            Icon::Up,
            color,
            bg,
        );
        draw(
            dc,
            RECT {
                left: middle,
                ..rect
            },
            Icon::Down,
            super::theme::download_color(),
            bg,
        );
        return;
    }
    let mask: &[u8; 4096] = match icon {
        Icon::Cpu => include_bytes!("../../assets/ui/cpu.alpha"),
        Icon::Memory => include_bytes!("../../assets/ui/memory.alpha"),
        Icon::Gpu => include_bytes!("../../assets/ui/gpu.alpha"),
        Icon::Disk => include_bytes!("../../assets/ui/disk.alpha"),
        Icon::Up => include_bytes!("../../assets/ui/up.alpha"),
        Icon::Down => include_bytes!("../../assets/ui/down.alpha"),
        Icon::Pin => include_bytes!("../../assets/ui/pin.alpha"),
        Icon::Close => include_bytes!("../../assets/ui/close.alpha"),
        Icon::Settings => include_bytes!("../../assets/ui/settings.alpha"),
        Icon::Network => unreachable!(),
    };
    let mut pixels = [0u8; 64 * 64 * 4];
    for (i, &alpha) in mask.iter().enumerate() {
        for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
            let fg = (color.0 >> shift) & 255;
            let bg = (bg.0 >> shift) & 255;
            pixels[i * 4 + channel] = ((fg * alpha as u32 + bg * (255 - alpha as u32)) / 255) as u8;
        }
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: 64,
            biHeight: -64,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let old_mode = SetStretchBltMode(dc, HALFTONE);
        let mut origin = POINT::default();
        let _ = SetBrushOrgEx(dc, 0, 0, Some(&mut origin));
        StretchDIBits(
            dc,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            0,
            0,
            64,
            64,
            Some(pixels.as_ptr().cast()),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        let _ = SetStretchBltMode(dc, STRETCH_BLT_MODE(old_mode));
        let _ = SetBrushOrgEx(dc, origin.x, origin.y, None);
    }
}
