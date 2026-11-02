use crate::win32::dpi;
use windows::Win32::Foundation::RECT;
pub const PANEL_W_DIP: f32 = 620.0;
pub const PANEL_H_DIP: f32 = 470.0;
pub const PREVIEW_W_DIP: f32 = 280.0;
pub const PREVIEW_H_DIP: f32 = 266.0;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Cpu,
    Memory,
    Gpu,
    Disk,
    Network,
}
impl Tab {
    pub const ALL: [Self; 5] = [
        Self::Cpu,
        Self::Memory,
        Self::Gpu,
        Self::Disk,
        Self::Network,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "内存",
            Self::Gpu => "显卡",
            Self::Disk => "磁盘",
            Self::Network => "网络",
        }
    }
}
pub fn panel_size(dpi: u32) -> (i32, i32) {
    (dpi::scale(PANEL_W_DIP, dpi), dpi::scale(PANEL_H_DIP, dpi))
}
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub width: i32,
    pub dpi: u32,
    pub close: RECT,
    pub pin: RECT,
    pub tabs: [RECT; 5],
    pub header_bottom: i32,
    pub content_x: i32,
    pub content_right: i32,
}
impl Layout {
    pub fn new(width: i32, _height: i32, dpi: u32) -> Self {
        let s = |v| dpi::scale(v, dpi);
        let pad = s(24.0);
        let close = RECT {
            left: width - s(44.0),
            top: s(8.0),
            right: width - s(12.0),
            bottom: s(36.0),
        };
        let pin = RECT {
            left: close.left - s(38.0),
            top: close.top,
            right: close.left - s(6.0),
            bottom: close.bottom,
        };
        let tabs = std::array::from_fn(|i| RECT {
            left: pad + (width - 2 * pad) * i as i32 / 5,
            right: pad + (width - 2 * pad) * (i + 1) as i32 / 5,
            top: s(52.0),
            bottom: s(102.0),
        });
        Self {
            width,
            dpi,
            close,
            pin,
            tabs,
            header_bottom: s(44.0),
            content_x: pad,
            content_right: width - pad,
        }
    }
    pub fn s(&self, dip: f32) -> i32 {
        dpi::scale(dip, self.dpi)
    }
    pub fn contains(rect: &RECT, x: i32, y: i32) -> bool {
        x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
    }
    pub fn tab_at(&self, x: i32, y: i32) -> Option<Tab> {
        self.tabs
            .iter()
            .position(|r| Self::contains(r, x, y))
            .map(|i| Tab::ALL[i])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tabs_and_header_buttons_hit_test_at_supported_scales() {
        for dpi in [96, 120, 144, 168, 192] {
            let (w, h) = panel_size(dpi);
            let l = Layout::new(w, h, dpi);
            for (i, rect) in l.tabs.iter().enumerate() {
                assert_eq!(
                    l.tab_at((rect.left + rect.right) / 2, rect.top),
                    Some(Tab::ALL[i])
                );
                assert!(rect.bottom < h);
            }
            assert_eq!(l.tab_at(l.pin.left, l.pin.top), None);
            assert!(l.pin.right < l.close.left);
            assert_eq!(l.tab_at(l.tabs[4].right, l.tabs[4].top), None);
        }
    }
}
