#![cfg_attr(not(feature = "console"), windows_subsystem = "windows")]

mod config;
mod diag;
mod history;
mod metrics;
mod sampler;
mod tray;
mod ui;
mod win32;

fn main() {
    win32::logging::init();
    win32::dpi::set_process_dpi_awareness();

    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|arg| arg == "--version") {
        println!("LitePulse {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args.iter().any(|arg| arg == "--probe") {
        let seconds = args
            .iter()
            .position(|arg| arg == "--probe")
            .and_then(|index| args.get(index + 1))
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5);
        diag::run_probe(seconds);
        return;
    }

    if !win32::single_instance::acquire() {
        win32::single_instance::activate_existing();
        return;
    }

    let startup_hidden = args.iter().any(|arg| arg == "--startup");
    let settings = config::Settings::load();
    let code = ui::run(startup_hidden, settings);
    if code != 0 {
        std::process::exit(code);
    }
}
