use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, CreatePen, CreateSolidBrush,
    DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_NOPREFIX,
    DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FF_DONTCARE, FillRect,
    FrameRect, GdiFlush, GetStockObject, HBITMAP, HDC, HFONT, HGDIOBJ, NULL_PEN,
    OUT_DEFAULT_PRECIS, PS_DOT, PS_SOLID, Polygon, Polyline, RoundRect, SRCCOPY, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::core::PCWSTR;

use crate::history::{HISTORY_CAP, History, Series};
use crate::metrics::{
    Snapshot, format_capacity, format_capacity_pair, format_percent, format_rate,
};
use crate::ui::icons::{self, Icon};
use crate::ui::layout::{Layout, Tab};
use crate::ui::theme::{self, Palette, ResolvedTheme};
use crate::win32::{dpi, to_wide};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hover {
    None,
    Pin,
    Close,
    Tab(Tab),
    Settings,
    Choice(usize),
}

pub struct PanelView<'a> {
    pub snapshot: Option<&'a Snapshot>,
    pub history: &'a History,
    pub always_on_top: bool,
    pub theme: ResolvedTheme,
    pub font_family: &'a str,
    pub hover: Hover,
    pub tab: Tab,
    pub interval_ms: u32,
    pub preview: bool,
    pub settings: Option<&'a crate::config::Settings>,
    pub autostart: bool,
    pub misans: bool,
    pub glass: bool,
}

struct Fonts {
    title: HFONT,
    label: HFONT,
    value: HFONT,
    value_small: HFONT,
    small: HFONT,
}

impl Fonts {
    fn new(dpi: u32, family: &str) -> Self {
        Self {
            title: make_font(14.0, 600, dpi, family),
            label: make_font(12.0, 400, dpi, family),
            value: make_font(24.0, 400, dpi, family),
            value_small: make_font(12.0, 600, dpi, family),
            small: make_font(10.5, 400, dpi, family),
        }
    }

    fn destroy(&mut self) {
        for font in [
            self.title,
            self.label,
            self.value,
            self.value_small,
            self.small,
        ] {
            delete_font(font);
        }
    }
}

fn make_font(size_dip: f32, weight: i32, dpi: u32, family: &str) -> HFONT {
    let height = -dpi::scale(size_dip, dpi).max(8);
    let face = to_wide(family);
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
            PCWSTR(face.as_ptr()),
        )
    }
}

fn delete_font(font: HFONT) {
    if !font.0.is_null() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
    }
}

pub struct Renderer {
    mem_dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    size: (i32, i32),
    fonts: Option<Fonts>,
    fonts_dpi: u32,
    fonts_family: String,
    pixels: *mut u8,
}

impl Default for Renderer {
    fn default() -> Self {
        Self {
            mem_dc: HDC(std::ptr::null_mut()),
            bitmap: HBITMAP(std::ptr::null_mut()),
            old_bitmap: HGDIOBJ(std::ptr::null_mut()),
            size: (0, 0),
            fonts: None,
            fonts_dpi: 0,
            fonts_family: String::new(),
            pixels: std::ptr::null_mut(),
        }
    }
}

impl Renderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn release_backbuffer(&mut self) {
        if !self.mem_dc.0.is_null() {
            unsafe {
                if !self.old_bitmap.0.is_null() {
                    SelectObject(self.mem_dc, self.old_bitmap);
                }
                if !self.bitmap.0.is_null() {
                    let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
                }
                let _ = DeleteDC(self.mem_dc);
            }
        }
        self.mem_dc = HDC(std::ptr::null_mut());
        self.bitmap = HBITMAP(std::ptr::null_mut());
        self.old_bitmap = HGDIOBJ(std::ptr::null_mut());
        self.size = (0, 0);
        self.pixels = std::ptr::null_mut();
    }

    pub fn invalidate_fonts(&mut self) {
        if let Some(mut fonts) = self.fonts.take() {
            fonts.destroy();
        }
        self.fonts_dpi = 0;
        self.fonts_family.clear();
    }

    pub fn destroy(&mut self) {
        self.release_backbuffer();
        self.invalidate_fonts();
    }

    fn ensure_fonts(&mut self, dpi: u32, family: &str) -> bool {
        if self.fonts.is_none() || self.fonts_dpi != dpi || self.fonts_family != family {
            self.invalidate_fonts();
            self.fonts = Some(Fonts::new(dpi, family));
            self.fonts_dpi = dpi;
            self.fonts_family = family.to_string();
        }
        self.fonts.is_some()
    }

    fn ensure_backbuffer(&mut self, target: HDC, width: i32, height: i32) -> bool {
        if self.size == (width, height) && !self.mem_dc.0.is_null() {
            return true;
        }
        self.release_backbuffer();
        let mem_dc = unsafe { CreateCompatibleDC(Some(target)) };
        if mem_dc.0.is_null() {
            return false;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = std::ptr::null_mut();
        let bitmap =
            unsafe { CreateDIBSection(Some(target), &info, DIB_RGB_COLORS, &mut pixels, None, 0) }
                .unwrap_or_default();
        if bitmap.0.is_null() {
            unsafe {
                let _ = DeleteDC(mem_dc);
            }
            return false;
        }
        let old = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
        self.mem_dc = mem_dc;
        self.bitmap = bitmap;
        self.pixels = pixels.cast();
        self.old_bitmap = old;
        self.size = (width, height);
        true
    }

    pub fn paint(&mut self, target: HDC, width: i32, height: i32, dpi: u32, view: &PanelView<'_>) {
        if width <= 0 || height <= 0 {
            return;
        }
        if !self.ensure_fonts(dpi, view.font_family)
            || !self.ensure_backbuffer(target, width, height)
        {
            return;
        }
        let dc = self.mem_dc;
        let fonts = self.fonts.as_ref().expect("fonts checked above");
        let mut palette = theme::palette(view.theme);
        if view.glass {
            palette.bg = COLORREF(0);
        }
        let layout = Layout::new(width, height, dpi);

        // Background and outer border.
        fill_rect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            palette.bg,
        );
        if !view.preview {
            frame_rect(
                dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
                palette.border,
            );
        }

        if view.preview {
            draw_preview(dc, &layout, fonts, &palette, view);
        } else {
            draw_header(dc, &layout, fonts, &palette, view);
            if view.settings.is_some() {
                draw_settings(dc, &layout, fonts, &palette, view);
            } else {
                draw_body(dc, &layout, fonts, &palette, view);
            }
        }

        unsafe {
            // Recover premultiplied alpha using black/white background passes.
            // Preserve text and SVG edge coverage over the live Acrylic.
            if view.glass {
                let _ = GdiFlush();
                let len = (width * height * 4) as usize;
                let black = std::slice::from_raw_parts(self.pixels, len).to_vec();
                let mut white_palette = palette;
                white_palette.bg = theme::rgb(255, 255, 255);
                fill_rect(
                    dc,
                    &RECT {
                        left: 0,
                        top: 0,
                        right: width,
                        bottom: height,
                    },
                    white_palette.bg,
                );
                draw_preview(dc, &layout, fonts, &white_palette, view);
                let _ = GdiFlush();
                for (pixel, black) in std::slice::from_raw_parts_mut(self.pixels, len)
                    .chunks_exact_mut(4)
                    .zip(black.chunks_exact(4))
                {
                    let transparency = (0..3)
                        .map(|i| pixel[i].saturating_sub(black[i]) as u32)
                        .sum::<u32>()
                        / 3;
                    pixel[..3].copy_from_slice(&black[..3]);
                    pixel[3] = 255 - transparency as u8;
                    if view.theme == ResolvedTheme::Light {
                        brighten_glass_pixel(pixel);
                    }
                }
            }
            let _ = BitBlt(target, 0, 0, width, height, Some(dc), 0, 0, SRCCOPY);
        }
    }
}

// Place a soft white tint behind the foreground, over the native Acrylic.
// Fully opaque glyphs and bars keep their original color and contrast.
fn brighten_glass_pixel(pixel: &mut [u8]) {
    const TINT_ALPHA: u32 = 192;
    let tint = (255 - pixel[3] as u32) * TINT_ALPHA / 255;
    for channel in pixel.iter_mut() {
        *channel = (*channel as u32 + tint).min(255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_labels_cover_long_sampling_windows() {
        assert_eq!(duration_label(30), "30 秒");
        assert_eq!(duration_label(60), "1 分钟");
        assert_eq!(duration_label(1770), "29 分 30 秒");
        assert_eq!(duration_label(3540), "59 分钟");
    }

    #[test]
    fn glass_tint_lightens_the_background_without_fading_opaque_content() {
        let mut background = [0, 0, 0, 0];
        brighten_glass_pixel(&mut background);
        assert_eq!(background, [192, 192, 192, 192]);
        let mut glyph = [30, 80, 200, 255];
        brighten_glass_pixel(&mut glyph);
        assert_eq!(glyph, [30, 80, 200, 255]);
        let mut edge = [20, 40, 80, 128];
        brighten_glass_pixel(&mut edge);
        assert_eq!(edge, [115, 135, 175, 223]);
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.destroy();
    }
}

fn rect(l: &Layout, x: f32, y: f32, w: f32, h: f32) -> RECT {
    RECT {
        left: l.s(x),
        top: l.s(y),
        right: l.s(x + w),
        bottom: l.s(y + h),
    }
}
fn row(l: &Layout, y: f32, h: f32) -> RECT {
    RECT {
        left: l.content_x,
        top: l.s(y),
        right: l.content_right,
        bottom: l.s(y + h),
    }
}
fn percent(value: Option<f32>) -> String {
    value.map(format_percent).unwrap_or_else(|| "—".into())
}
fn duration_label(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (0, _) => format!("{seconds} 秒"),
        (minutes, 0) => format!("{minutes} 分钟"),
        (minutes, remainder) => format!("{minutes} 分 {remainder} 秒"),
    }
}
fn rate(value: Option<f64>) -> String {
    value.map(format_rate).unwrap_or_else(|| "—".into())
}
fn accent(tab: Tab, p: &Palette) -> COLORREF {
    match tab {
        Tab::Cpu => p.cpu,
        Tab::Memory => p.mem,
        Tab::Gpu => p.gpu,
        Tab::Disk => p.disk,
        Tab::Network => p.net,
    }
}
fn blend(fg: COLORREF, bg: COLORREF, amount: u32) -> COLORREF {
    let mut value = 0;
    for shift in [0, 8, 16] {
        value |= ((((fg.0 >> shift) & 255) * amount + ((bg.0 >> shift) & 255) * (100 - amount))
            / 100)
            << shift;
    }
    COLORREF(value)
}
fn draw_header(dc: HDC, l: &Layout, f: &Fonts, p: &Palette, v: &PanelView<'_>) {
    draw_text(
        dc,
        "LitePulse",
        &row(l, 0.0, 44.0),
        f.title,
        p.text,
        Align::Left,
        false,
    );
    let pin_bg = if v.hover == Hover::Pin || v.always_on_top {
        p.hover
    } else {
        p.bg
    };
    fill_round_rect(dc, &l.pin, l.s(5.0), pin_bg);
    let icon = l.header_icon(l.pin, 1.0);
    icons::draw(
        dc,
        icon,
        Icon::Pin,
        if v.always_on_top { p.accent } else { p.dim },
        pin_bg,
    );
    if v.hover == Hover::Close {
        fill_round_rect(dc, &l.close, l.s(5.0), p.hover);
    }
    let close_bg = if v.hover == Hover::Close {
        p.hover
    } else {
        p.bg
    };
    icons::draw(
        dc,
        l.header_icon(l.close, 0.0),
        Icon::Close,
        p.dim,
        close_bg,
    );
    let settings_bg = if v.hover == Hover::Settings || v.settings.is_some() {
        p.hover
    } else {
        p.bg
    };
    fill_round_rect(dc, &l.settings, l.s(5.0), settings_bg);
    icons::draw(
        dc,
        l.header_icon(l.settings, 0.0),
        Icon::Settings,
        if v.settings.is_some() {
            p.accent
        } else {
            p.dim
        },
        settings_bg,
    );
    fill_rect(
        dc,
        &RECT {
            left: 1,
            top: l.header_bottom - 1,
            right: l.width - 1,
            bottom: l.header_bottom,
        },
        p.track,
    );
    if v.settings.is_some() {
        return;
    }
    for (i, tab) in Tab::ALL.into_iter().enumerate() {
        let r = l.tabs[i];
        let selected = tab == v.tab;
        let bg = if selected || v.hover == Hover::Tab(tab) {
            p.hover
        } else {
            p.bg
        };
        fill_round_rect(dc, &r, l.s(6.0), bg);
        let color = if selected { accent(tab, p) } else { p.dim };
        icons::draw(
            dc,
            RECT {
                left: r.left + l.s(15.0),
                top: r.top + l.s(14.0),
                right: r.left + l.s(37.0),
                bottom: r.top + l.s(36.0),
            },
            tab.into(),
            color,
            bg,
        );
        draw_text(
            dc,
            tab.label(),
            &RECT {
                left: r.left + l.s(45.0),
                ..r
            },
            f.label,
            if selected { p.text } else { p.dim },
            Align::Left,
            false,
        );
        if selected {
            fill_rect(
                dc,
                &RECT {
                    left: r.left + l.s(13.0),
                    right: r.right - l.s(13.0),
                    top: r.bottom - l.s(3.0),
                    bottom: r.bottom,
                },
                color,
            );
        }
    }
}

fn stat(dc: HDC, l: &Layout, f: &Fonts, p: &Palette, x: f32, label: &str, value: &str) {
    draw_text(
        dc,
        label,
        &rect(l, x, 406.0, 180.0, 18.0),
        f.small,
        p.dim,
        Align::Left,
        false,
    );
    draw_text(
        dc,
        value,
        &rect(l, x, 425.0, 180.0, 30.0),
        if value.chars().count() > 14 {
            f.value_small
        } else {
            f.value
        },
        p.text,
        Align::Left,
        true,
    );
}
fn draw_body(dc: HDC, l: &Layout, f: &Fonts, p: &Palette, v: &PanelView<'_>) {
    let s = v.snapshot;
    let gpu = s.and_then(|s| s.gpu.as_ref());
    let mem = s.and_then(|s| s.memory.as_ref());
    let disk = s.and_then(|s| s.disk.as_ref());
    let net = s.and_then(|s| s.net.as_ref());
    let detail = match v.tab {
        Tab::Cpu => "总利用率".into(),
        Tab::Memory => mem
            .map(|m| format!("{} 总内存", format_capacity(m.total)))
            .unwrap_or_default(),
        Tab::Gpu => gpu.map(|g| g.short_name()).unwrap_or_default(),
        Tab::Disk => disk
            .map(|d| format!("全部物理盘 · {} 空间", d.volume_letter))
            .unwrap_or_default(),
        Tab::Network => net.map(|n| n.alias.clone()).unwrap_or_default(),
    };
    draw_text(
        dc,
        v.tab.label(),
        &row(l, 116.0, 36.0),
        f.value,
        p.text,
        Align::Left,
        false,
    );
    draw_text(
        dc,
        &detail,
        &RECT {
            left: l.s(150.0),
            ..row(l, 120.0, 30.0)
        },
        f.label,
        p.dim,
        Align::Right,
        true,
    );
    let c = accent(v.tab, p);
    match v.tab {
        Tab::Cpu => {
            chart(
                dc,
                l,
                f,
                p,
                &v.history.cpu,
                None,
                100.0,
                c,
                168.0,
                214.0,
                "利用率",
                "100%",
                v,
            );
            stat(
                dc,
                l,
                f,
                p,
                24.0,
                "利用率",
                &percent(s.and_then(|s| s.cpu_pct)),
            );
            stat(
                dc,
                l,
                f,
                p,
                220.0,
                "采样间隔",
                &duration_label(v.interval_ms / 1000),
            );
        }
        Tab::Memory => {
            chart(
                dc,
                l,
                f,
                p,
                &v.history.memory,
                None,
                100.0,
                c,
                168.0,
                178.0,
                "内存使用率",
                "100%",
                v,
            );
            draw_text(
                dc,
                "内存组成 · 已用 / 可用",
                &row(l, 369.0, 18.0),
                f.small,
                p.dim,
                Align::Left,
                false,
            );
            progress(dc, &row(l, 390.0, 8.0), mem.map(|m| m.percent), c, p.track);
            stat(
                dc,
                l,
                f,
                p,
                24.0,
                "正在使用",
                &mem.map(|m| format_capacity(m.used))
                    .unwrap_or_else(|| "—".into()),
            );
            stat(
                dc,
                l,
                f,
                p,
                220.0,
                "可用",
                &mem.map(|m| format_capacity(m.total.saturating_sub(m.used)))
                    .unwrap_or_else(|| "—".into()),
            );
            stat(dc, l, f, p, 416.0, "占用", &percent(mem.map(|m| m.percent)));
        }
        Tab::Gpu => {
            chart(
                dc,
                l,
                f,
                p,
                &v.history.gpu,
                None,
                100.0,
                c,
                168.0,
                90.0,
                "GPU 利用率",
                "100%",
                v,
            );
            chart(
                dc,
                l,
                f,
                p,
                &v.history.vram,
                None,
                100.0,
                c,
                294.0,
                90.0,
                "专用显存占用",
                "100%",
                v,
            );
            stat(
                dc,
                l,
                f,
                p,
                24.0,
                "GPU 利用率",
                &percent(gpu.map(|g| g.util_pct)),
            );
            stat(
                dc,
                l,
                f,
                p,
                220.0,
                "专用显存 · 已用 / 总量",
                &gpu.and_then(|g| Some(format_capacity_pair(g.vram_used?, g.vram_total?)))
                    .unwrap_or_else(|| "—".into()),
            );
            stat(
                dc,
                l,
                f,
                p,
                416.0,
                "温度",
                &gpu.and_then(|g| g.temp_c)
                    .map(|t| format!("{t} °C"))
                    .unwrap_or_else(|| "不支持".into()),
            );
        }
        Tab::Disk => {
            chart(
                dc,
                l,
                f,
                p,
                &v.history.disk,
                None,
                100.0,
                c,
                168.0,
                90.0,
                "活动时间",
                "100%",
                v,
            );
            let max = auto_scale(&v.history.disk_read, Some(&v.history.disk_write));
            chart(
                dc,
                l,
                f,
                p,
                &v.history.disk_read,
                Some(&v.history.disk_write),
                max,
                c,
                294.0,
                90.0,
                "传输速率 · 实线读取 / 虚线写入",
                &format_rate(max as f64),
                v,
            );
            stat(
                dc,
                l,
                f,
                p,
                24.0,
                "读取",
                &rate(disk.and_then(|d| d.read_bps)),
            );
            stat(
                dc,
                l,
                f,
                p,
                220.0,
                "写入",
                &rate(disk.and_then(|d| d.write_bps)),
            );
            stat(
                dc,
                l,
                f,
                p,
                416.0,
                "系统盘已用",
                &disk
                    .map(|d| format_percent(d.volume_percent()))
                    .unwrap_or_else(|| "—".into()),
            );
        }
        Tab::Network => {
            let connected = net.filter(|n| n.connected);
            let max = auto_scale(&v.history.net_down, Some(&v.history.net_up));
            chart(
                dc,
                l,
                f,
                p,
                &v.history.net_down,
                Some(&v.history.net_up),
                max,
                c,
                168.0,
                214.0,
                "吞吐量 · 实线下载 / 虚线上传",
                &format_rate(max as f64),
                v,
            );
            stat(
                dc,
                l,
                f,
                p,
                24.0,
                "下载",
                &rate(connected.map(|n| n.down_bps)),
            );
            stat(
                dc,
                l,
                f,
                p,
                220.0,
                "上传",
                &rate(connected.map(|n| n.up_bps)),
            );
            stat(
                dc,
                l,
                f,
                p,
                416.0,
                "连接状态",
                if connected.is_some() {
                    "已连接"
                } else if net.is_some() {
                    "无连接"
                } else if s.is_none() {
                    "读取中"
                } else {
                    "不可用"
                },
            );
        }
    }
}

fn draw_settings(dc: HDC, l: &Layout, f: &Fonts, p: &Palette, v: &PanelView<'_>) {
    draw_text(
        dc,
        "设置",
        &row(l, 54.0, 30.0),
        f.title,
        p.text,
        Align::Left,
        false,
    );
    draw_text(
        dc,
        "修改立即生效 · 点击齿轮返回监控",
        &row(l, 84.0, 18.0),
        f.small,
        p.dim,
        Align::Left,
        false,
    );
    for (i, label) in super::settings::ROW_LABELS.iter().enumerate() {
        draw_text(
            dc,
            label,
            &rect(
                l,
                24.0,
                110.0 + i as f32 * super::settings::ROW_PITCH,
                130.0,
                35.0,
            ),
            f.label,
            p.dim,
            Align::Left,
            false,
        );
    }
    for (i, c) in super::settings::choices(l, v.settings.unwrap(), v.autostart, v.misans)
        .into_iter()
        .enumerate()
    {
        let bg = if c.selected {
            blend(p.accent, p.bg, 16)
        } else if v.hover == Hover::Choice(i) && c.enabled {
            p.hover
        } else {
            p.track
        };
        fill_round_rect(dc, &c.rect, l.s(5.0), bg);
        draw_text(
            dc,
            c.label,
            &c.rect,
            f.label,
            if !c.enabled {
                p.dim
            } else if c.selected {
                p.accent
            } else {
                p.text
            },
            Align::Center,
            false,
        );
    }
}

fn draw_preview(dc: HDC, l: &Layout, f: &Fonts, p: &Palette, v: &PanelView<'_>) {
    let s = v.snapshot;
    let values = [
        s.and_then(|s| s.cpu_pct),
        s.and_then(|s| s.memory.as_ref()).map(|m| m.percent),
        s.and_then(|s| s.gpu.as_ref())
            .and_then(|g| g.vram_percent()),
        s.and_then(|s| s.disk.as_ref()).and_then(|d| d.active_pct),
    ];
    for (i, tab) in [Tab::Cpu, Tab::Memory, Tab::Gpu, Tab::Disk]
        .into_iter()
        .enumerate()
    {
        let y = 10.0 + i as f32 * 31.0;
        let color = accent(tab, p);
        icons::draw(
            dc,
            rect(l, 10.0, y + 3.0, 16.0, 16.0),
            tab.into(),
            color,
            p.bg,
        );
        draw_text(
            dc,
            if tab == Tab::Gpu {
                "显存"
            } else {
                tab.label()
            },
            &rect(l, 32.0, y, 54.0, 18.0),
            f.small,
            p.dim,
            Align::Left,
            false,
        );
        draw_text(
            dc,
            &percent(values[i]),
            &rect(l, 85.0, y, 45.0, 18.0),
            f.small,
            p.text,
            Align::Right,
            false,
        );
        progress(
            dc,
            &rect(l, 32.0, y + 20.0, 98.0, 4.0),
            values[i],
            color,
            p.track,
        );
    }
    let net = s.and_then(|s| s.net.as_ref()).filter(|n| n.connected);
    for (x, icon, value, color) in [
        (8.0, Icon::Up, net.map(|n| n.up_bps), theme::upload_color()),
        (
            72.0,
            Icon::Down,
            net.map(|n| n.down_bps),
            theme::download_color(),
        ),
    ] {
        icons::draw(dc, rect(l, x, 140.0, 8.0, 14.0), icon, color, p.bg);
        draw_text(
            dc,
            &rate(value).replace(' ', ""),
            &rect(l, x + 10.0, 139.0, 49.0, 18.0),
            f.small,
            p.text,
            Align::Left,
            false,
        );
    }
}
fn progress(dc: HDC, r: &RECT, value: Option<f32>, color: COLORREF, track: COLORREF) {
    fill_round_rect(dc, r, (r.bottom - r.top) / 2, track);
    if let Some(value) = value.filter(|v| v.is_finite()) {
        let filled = RECT {
            right: r.left + ((r.right - r.left) as f32 * (value / 100.0).clamp(0.0, 1.0)) as i32,
            ..*r
        };
        fill_round_rect(dc, &filled, (r.bottom - r.top) / 2, color);
    }
}
fn auto_scale(a: &Series, b: Option<&Series>) -> f32 {
    a.iter()
        .chain(b.into_iter().flatten())
        .flatten()
        .copied()
        .fold(1024.0, f32::max)
        * 1.15
}
#[allow(clippy::too_many_arguments)]
fn chart(
    dc: HDC,
    l: &Layout,
    f: &Fonts,
    p: &Palette,
    values: &Series,
    secondary: Option<&Series>,
    max: f32,
    color: COLORREF,
    top: f32,
    height: f32,
    title: &str,
    scale: &str,
    view: &PanelView<'_>,
) {
    draw_text(
        dc,
        title,
        &row(l, top - 18.0, 17.0),
        f.small,
        p.dim,
        Align::Left,
        false,
    );
    draw_text(
        dc,
        scale,
        &row(l, top - 18.0, 17.0),
        f.small,
        p.dim,
        Align::Right,
        false,
    );
    let r = row(l, top, height);
    let segments = chart_segments(values, r, max);
    for points in &segments {
        if points.len() < 2 {
            continue;
        }
        let mut polygon = Vec::with_capacity(points.len() + 2);
        polygon.push(POINT {
            x: points[0].x,
            y: r.bottom - 1,
        });
        polygon.extend_from_slice(points);
        polygon.push(POINT {
            x: points[points.len() - 1].x,
            y: r.bottom - 1,
        });
        let brush = unsafe { CreateSolidBrush(blend(color, p.bg, 22)) };
        unsafe {
            let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
            let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
            let _ = Polygon(dc, &polygon);
            SelectObject(dc, old_pen);
            SelectObject(dc, old_brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
    }
    for i in 1..12 {
        let x = r.left + (r.right - r.left) * i / 12;
        draw_line(dc, x, r.top, x, r.bottom, 1, p.track);
    }
    for i in 1..6 {
        let y = r.top + (r.bottom - r.top) * i / 6;
        draw_line(dc, r.left, y, r.right, y, 1, p.track);
    }
    frame_rect(dc, &r, blend(color, p.bg, 50));
    draw_segments(dc, &segments, color, false);
    if let Some(secondary) = secondary {
        draw_segments(dc, &chart_segments(secondary, r, max), color, true);
    }
    if values.back().copied().flatten().is_none()
        && secondary
            .and_then(|s| s.back().copied().flatten())
            .is_none()
    {
        let message = if view.snapshot.is_none() {
            "正在读取数据…"
        } else {
            "暂无数据"
        };
        draw_text(dc, message, &r, f.label, p.dim, Align::Center, false);
    }
    draw_text(
        dc,
        &duration_label((HISTORY_CAP as u32 - 1) * view.interval_ms / 1000),
        &row(l, top + height + 2.0, 17.0),
        f.small,
        p.dim,
        Align::Left,
        false,
    );
    draw_text(
        dc,
        "0",
        &row(l, top + height + 2.0, 17.0),
        f.small,
        p.dim,
        Align::Right,
        false,
    );
}
pub(super) fn chart_segments(values: &Series, rect: RECT, max: f32) -> Vec<Vec<POINT>> {
    let mut segments = Vec::new();
    let mut segment = Vec::new();
    for (i, value) in values.iter().enumerate() {
        if let Some(value) = value.filter(|v| v.is_finite()) {
            let slot = HISTORY_CAP.saturating_sub(values.len()) + i;
            segment.push(POINT {
                x: rect.left
                    + slot as i32 * (rect.right - rect.left - 1) / (HISTORY_CAP - 1) as i32,
                y: rect.bottom
                    - 1
                    - ((value / max.max(1.0)).clamp(0.0, 1.0) * (rect.bottom - rect.top - 1) as f32)
                        as i32,
            });
        } else if !segment.is_empty() {
            segments.push(std::mem::take(&mut segment));
        }
    }
    if !segment.is_empty() {
        segments.push(segment);
    }
    segments
}
fn draw_segments(dc: HDC, segments: &[Vec<POINT>], color: COLORREF, dotted: bool) {
    let pen = unsafe { CreatePen(if dotted { PS_DOT } else { PS_SOLID }, 1, color) };
    if pen.0.is_null() {
        return;
    }
    unsafe {
        let old = SelectObject(dc, HGDIOBJ(pen.0));
        SetBkMode(dc, TRANSPARENT);
        for points in segments {
            if points.len() >= 2 {
                let _ = Polyline(dc, points);
            } else if let Some(point) = points.first() {
                fill_rect(
                    dc,
                    &RECT {
                        left: point.x,
                        top: point.y,
                        right: point.x + 1,
                        bottom: point.y + 1,
                    },
                    color,
                );
            }
        }
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}
#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
    Center,
}

fn draw_text(
    dc: HDC,
    text: &str,
    rect: &RECT,
    font: HFONT,
    color: COLORREF,
    align: Align,
    ellipsis: bool,
) {
    if text.is_empty() || rect.right <= rect.left {
        return;
    }
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut flags = DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0;
    match align {
        Align::Left => {}
        Align::Right => flags |= DT_RIGHT.0,
        Align::Center => flags |= DT_CENTER.0,
    }
    if ellipsis {
        flags |= DT_END_ELLIPSIS.0;
    }
    unsafe {
        let old_font = SelectObject(dc, HGDIOBJ(font.0));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, color);
        let mut target = *rect;
        let _ = DrawTextW(
            dc,
            &mut wide,
            &mut target,
            windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(flags),
        );
        SelectObject(dc, old_font);
    }
}

fn fill_rect(dc: HDC, rect: &RECT, color: COLORREF) {
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.0.is_null() {
        return;
    }
    unsafe {
        let _ = FillRect(dc, rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn frame_rect(dc: HDC, rect: &RECT, color: COLORREF) {
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.0.is_null() {
        return;
    }
    unsafe {
        let _ = FrameRect(dc, rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn fill_round_rect(dc: HDC, rect: &RECT, radius: i32, color: COLORREF) {
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.0.is_null() {
        return;
    }
    unsafe {
        let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
        let _ = RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius * 2,
            radius * 2,
        );
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn draw_line(dc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, width: i32, color: COLORREF) {
    let pen = unsafe { CreatePen(PS_SOLID, width.max(1), color) };
    if pen.0.is_null() {
        return;
    }
    unsafe {
        let old = SelectObject(dc, HGDIOBJ(pen.0));
        let _ = windows::Win32::Graphics::Gdi::MoveToEx(dc, x1, y1, None);
        let _ = windows::Win32::Graphics::Gdi::LineTo(dc, x2, y2);
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}
