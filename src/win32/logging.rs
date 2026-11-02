use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::win32::data_dir;

static LOGGER: OnceLock<Mutex<File>> = OnceLock::new();

pub fn log_path() -> PathBuf {
    data_dir().join("litepulse.log")
}

fn timestamp() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(elapsed) => {
            let secs = elapsed.as_secs();
            format!("{secs}.{:03}", elapsed.subsec_millis())
        }
        Err(_) => "0.000".to_string(),
    }
}

pub fn init() {
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    let path = log_path();
    if let Ok(meta) = fs::metadata(&path) {
        if meta.len() > 256 * 1024 {
            let _ = fs::remove_file(&path);
        }
    }
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = LOGGER.set(Mutex::new(file));
    }

    std::panic::set_hook(Box::new(|info| {
        log(&format!("PANIC: {info}"));
    }));

    log(&format!(
        "starting LitePulse {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown exe".to_string())
    ));
}

pub fn log(message: &str) {
    let line = format!("{} {}\n", timestamp(), message);
    if let Some(lock) = LOGGER.get() {
        if let Ok(mut file) = lock.lock() {
            let _ = file.write_all(line.as_bytes());
        }
    }
    #[cfg(feature = "console")]
    {
        eprint!("{line}");
    }
}
