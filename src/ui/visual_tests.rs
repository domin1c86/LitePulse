//! Explicit native GDI visual checks. Images are inspected by a human/agent;
//! creating them alone does not constitute a visual pass.
use super::{
    layout::{PREVIEW_H_DIP, PREVIEW_W_DIP, Tab, panel_size},
    paint::{Hover, PanelView, Renderer},
    theme::ResolvedTheme,
};
use crate::{config::Settings, history::History, metrics::*, win32::dpi};
use std::{ffi::c_void, mem::size_of};
use windows::Win32::{Foundation::RECT, Graphics::Gdi::*};

fn sample(i: usize) -> Snapshot {
    let gb = 1024 * 1024 * 1024;
    Snapshot {
        cpu_pct: Some(18.0 + ((i * 13) % 27) as f32),
        memory: Some(MemoryInfo {
            used: 18 * gb,
            total: 32 * gb,
            percent: 56.25 + (i % 4) as f32,
        }),
        gpu: Some(GpuInfo {
            name: "NVIDIA GeForce RTX 5060 Laptop GPU".into(),
            util_pct: 12.0 + (i % 8) as f32,
            temp_c: None,
            vram_used: Some(3 * gb),
            vram_total: Some(8 * gb),
        }),
        disk: Some(DiskInfo {
            active_pct: Some((i % 12) as f32),
            read_bps: Some((i % 7) as f64 * 80_000.0),
            write_bps: Some((i % 13) as f64 * 120_000.0),
            volume_letter: "C:".into(),
            volume_used: 400 * gb,
            volume_total: 954 * gb,
        }),
        net: Some(NetInfo {
            down_bps: (i % 17) as f64 * 120_000.0,
            up_bps: (i % 11) as f64 * 30_000.0,
            alias: "WLAN".into(),
            connected: true,
        }),
        timings: SourceTimings::default(),
    }
}

fn bitmap(path: &str, dpi: u32, view: &PanelView<'_>) {
    let (w, h) = if view.preview {
        (
            dpi::scale(PREVIEW_W_DIP, dpi),
            dpi::scale(PREVIEW_H_DIP, dpi),
        )
    } else {
        panel_size(dpi)
    };
    unsafe {
        let dc = CreateCompatibleDC(None);
        let mut pixels: *mut c_void = std::ptr::null_mut();
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let bmp = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut pixels, None, 0).unwrap();
        let old = SelectObject(dc, HGDIOBJ(bmp.0));
        let mut renderer = Renderer::new();
        renderer.paint(dc, w, h, dpi, view);
        let _ = GdiFlush();
        let bytes = std::slice::from_raw_parts(pixels.cast::<u8>(), (w * h * 4) as usize);
        let mut output = Vec::new();
        output.extend_from_slice(b"BM");
        output.extend_from_slice(&(54 + bytes.len() as u32).to_le_bytes());
        output.extend_from_slice(&[0; 4]);
        output.extend_from_slice(&54u32.to_le_bytes());
        output.extend_from_slice(&40u32.to_le_bytes());
        output.extend_from_slice(&w.to_le_bytes());
        output.extend_from_slice(&(-h).to_le_bytes());
        output.extend_from_slice(&1u16.to_le_bytes());
        output.extend_from_slice(&32u16.to_le_bytes());
        output.extend_from_slice(&[0; 24]);
        output.extend_from_slice(bytes);
        std::fs::write(path, output).unwrap();
        drop(renderer);
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(dc);
    }
}

#[test]
#[ignore = "manual native-render visual matrix; inspect target/visual images"]
fn render_visual_matrix() {
    dpi::set_process_dpi_awareness();
    std::fs::create_dir_all("target/visual").unwrap();
    let mut history = History::new();
    for i in 0..60 {
        history.push(&sample(i));
    }
    let mut snapshot = sample(59);
    for (theme, dpi, family, name) in [
        (ResolvedTheme::Light, 96, "MiSans", "light-100"),
        (ResolvedTheme::Dark, 168, "MiSans", "dark-175"),
        (
            ResolvedTheme::Light,
            192,
            "Microsoft YaHei UI",
            "light-200-yahei",
        ),
    ] {
        for tab in Tab::ALL {
            let view = PanelView {
                snapshot: Some(&snapshot),
                history: &history,
                always_on_top: true,
                theme,
                font_family: family,
                hover: Hover::None,
                tab,
                interval_ms: 1000,
                preview: false,
            };
            bitmap(&format!("target/visual/{name}-{tab:?}.bmp"), dpi, &view);
        }
        let view = PanelView {
            snapshot: Some(&snapshot),
            history: &history,
            always_on_top: false,
            theme,
            font_family: family,
            hover: Hover::None,
            tab: Tab::Cpu,
            interval_ms: 1000,
            preview: true,
        };
        bitmap(&format!("target/visual/{name}-preview.bmp"), dpi, &view);
    }
    let empty = Snapshot::empty();
    let mut disconnected = sample(59);
    disconnected.net.as_mut().unwrap().connected = false;
    let blank = History::new();
    for (name, data) in [
        ("loading", None),
        ("unavailable", Some(&empty)),
        ("disconnected", Some(&disconnected)),
    ] {
        for tab in Tab::ALL {
            let view = PanelView {
                snapshot: data,
                history: &blank,
                always_on_top: false,
                theme: ResolvedTheme::Light,
                font_family: "MiSans",
                hover: Hover::None,
                tab,
                interval_ms: 5000,
                preview: false,
            };
            bitmap(&format!("target/visual/{name}-{tab:?}.bmp"), 96, &view);
        }
        let view = PanelView {
            snapshot: data,
            history: &blank,
            always_on_top: false,
            theme: ResolvedTheme::Light,
            font_family: "MiSans",
            hover: Hover::None,
            tab: Tab::Cpu,
            interval_ms: 1000,
            preview: true,
        };
        bitmap(&format!("target/visual/{name}-preview.bmp"), 96, &view);
    }
    snapshot.cpu_pct = Some(100.0);
    snapshot.memory.as_mut().unwrap().percent = 0.0;
    snapshot.net.as_mut().unwrap().up_bps = 100.0 * 1024.0 * 1024.0 * 1024.0;
    snapshot.net.as_mut().unwrap().down_bps = 999.0 * 1024.0 * 1024.0;
    let view = PanelView {
        snapshot: Some(&snapshot),
        history: &history,
        always_on_top: false,
        theme: ResolvedTheme::Dark,
        font_family: "MiSans",
        hover: Hover::None,
        tab: Tab::Cpu,
        interval_ms: 1000,
        preview: true,
    };
    bitmap("target/visual/extremes-preview.bmp", 168, &view);
    for (name, hover) in [
        ("pin", Hover::Pin),
        ("close", Hover::Close),
        ("tab", Hover::Tab(Tab::Memory)),
    ] {
        let view = PanelView {
            snapshot: Some(&snapshot),
            history: &history,
            always_on_top: hover == Hover::Pin,
            theme: ResolvedTheme::Light,
            font_family: "MiSans",
            hover,
            tab: Tab::Cpu,
            interval_ms: 1000,
            preview: false,
        };
        bitmap(&format!("target/visual/hover-{name}.bmp"), 96, &view);
    }
}

#[test]
#[ignore = "interactive desktop check; exit through LitePulse menu"]
fn interactive_visual_check() {
    dpi::set_process_dpi_awareness();
    assert_eq!(super::run(false, Settings::default()), 0);
}

#[test]
fn graph_points_keep_time_gaps_and_fixed_scale() {
    let series = [Some(0.0), None, Some(100.0), Some(200.0)].into();
    let segments = super::paint::chart_segments(
        &series,
        RECT {
            left: 0,
            top: 0,
            right: 60,
            bottom: 101,
        },
        100.0,
    );
    assert_eq!(segments.len(), 2);
    assert_eq!((segments[0][0].x, segments[0][0].y), (56, 100));
    assert_eq!((segments[1][0].x, segments[1][0].y), (58, 0));
    assert_eq!((segments[1][1].x, segments[1][1].y), (59, 0));
}
