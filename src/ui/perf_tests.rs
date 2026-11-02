//! Opt-in native profiling; never runs in the normal test suite.
//! Run an optimized build in the user's session, where PDH is available:
//! cargo test --release profile_sampling_interaction -- --ignored --nocapture
use super::*;
use std::time::Duration;
use windows::Win32::Foundation::FILETIME;

use windows::Win32::System::Threading::{
    GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources, GetProcessTimes,
};
use windows::Win32::UI::WindowsAndMessaging::{PM_REMOVE, PeekMessageW};

fn cpu_seconds() -> f64 {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .unwrap();
    }
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    (ticks(kernel) + ticks(user)) as f64 / 10_000_000.0
}

fn report(label: &str, values: &mut [f64]) {
    if values.is_empty() {
        println!("{label}: no observations");
        return;
    }
    values.sort_by(f64::total_cmp);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let p95 = values[((values.len() - 1) as f64 * 0.95).ceil() as usize];
    println!(
        "{label}: n={} mean={mean:.3} p95={p95:.3} max={:.3} ms",
        values.len(),
        values.last().unwrap()
    );
}

#[test]
#[ignore = "90-second native performance profile; adds a temporary tray icon"]
fn profile_sampling_interaction() {
    dpi::set_process_dpi_awareness();
    // Bind the real App message handler without attach(): no settings writes,
    // autostart repair, foreground activation or changes to an existing app.
    let class = to_wide("LitePulsePerformanceCheck");
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.unwrap();
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: HINSTANCE(module.0),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassExW(&wc) };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            620,
            470,
            None,
            None,
            Some(HINSTANCE(module.0)),
            None,
        )
    }
    .unwrap();
    let mut app = Box::new(App::new(Settings::default(), true));
    app.hwnd = hwnd;
    app.dpi = dpi::for_window(hwnd);
    app.font_family = font::resolve(&app.settings.font_family);
    app.tray = Some(Tray::new(hwnd, app.dpi, app.settings.tray_metric));
    assert!(app.tray.as_mut().unwrap().add());
    app.sampler = Some(SamplerHandle::start(hwnd.0 as isize, 30_000));
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *app as *mut App) as isize) };

    for (phase, seconds, interactive) in [
        ("background-30s", 33, false),
        ("interactive-1s-no-window", 23, true),
        ("background-restored-30s", 33, false),
    ] {
        // The menu flag exercises the real boost policy without showing UI.
        app.menu_open = interactive;
        app.update_sampling_policy();
        assert_eq!(
            app.effective_interval_ms,
            if interactive { 1000 } else { 30_000 }
        );
        let started = Instant::now();
        let cpu = cpu_seconds();
        let mut last_poll = started;
        let mut gaps = Vec::new();
        let mut handlers = Vec::new();
        let mut collection = Vec::new();
        let mut gpu = Vec::new();
        while started.elapsed() < Duration::from_secs(seconds) {
            let now = Instant::now();
            gaps.push(now.duration_since(last_poll).as_secs_f64() * 1000.0);
            last_poll = now;
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                let sample = message.message == WM_APP_SAMPLE;
                let handler = Instant::now();
                unsafe { DispatchMessageW(&message) };
                if sample {
                    handlers.push(handler.elapsed().as_secs_f64() * 1000.0);
                    if let Some(s) = &app.snapshot {
                        let t = &s.timings;
                        gpu.push(t.gpu_us as f64 / 1000.0);
                        collection.push(
                            (t.cpu_us + t.mem_us + t.gpu_us + t.disk_us + t.net_us) as f64 / 1000.0,
                        );
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        println!(
            "\n{phase}: elapsed={:.2}s cpu={:.3}% of one core gdi={} user={}",
            started.elapsed().as_secs_f64(),
            (cpu_seconds() - cpu) / started.elapsed().as_secs_f64() * 100.0,
            unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) },
            unsafe { GetGuiResources(GetCurrentProcess(), GR_USEROBJECTS) }
        );
        report("collection", &mut collection);
        report("gpu", &mut gpu);
        report(
            "UI sample handler (including Shell tray update)",
            &mut handlers,
        );
        report("10ms polling gaps (not mouse/DPC latency)", &mut gaps);
    }
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
    drop(app);
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}
