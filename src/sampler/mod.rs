pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod gpu_pdh;
pub mod memory;
pub mod network;

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::metrics::{
    GpuInfo, Snapshot, SourceTimings, format_capacity, format_percent, format_rate,
};
use crate::win32::{WM_APP_SAMPLE, logging};

enum GpuSource {
    Disabled,
    Pdh(gpu_pdh::PdhGpuSampler),
    Nvml(gpu::GpuSampler),
}

impl GpuSource {
    fn new() -> Self {
        if std::env::var_os("LITEPULSE_DISABLE_GPU").is_some() {
            return Self::Disabled;
        }
        let backend = std::env::var("LITEPULSE_GPU_BACKEND")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if backend == "nvml" {
            logging::log("GPU backend forced to NVML");
            return Self::Nvml(gpu::GpuSampler::new());
        }

        let pdh = gpu_pdh::PdhGpuSampler::new();
        if pdh.is_available() {
            Self::Pdh(pdh)
        } else {
            logging::log("GPU PDH counters unavailable; falling back to NVML");
            Self::Nvml(gpu::GpuSampler::new())
        }
    }

    fn reset(&mut self) {
        if let Self::Nvml(sampler) = self {
            sampler.reset();
        }
    }

    fn status(&self) -> String {
        match self {
            Self::Disabled => "gpu disabled".to_string(),
            Self::Pdh(sampler) => sampler.status(),
            Self::Nvml(sampler) => sampler.status(),
        }
    }

    fn sample(&mut self) -> Option<GpuInfo> {
        match self {
            Self::Disabled => None,
            Self::Pdh(sampler) => sampler.sample(),
            Self::Nvml(sampler) => sampler.sample(),
        }
    }
}

pub struct Sources {
    cpu: cpu::CpuSampler,
    gpu: GpuSource,
    disk: Option<disk::DiskSampler>,
    net: network::NetSampler,
}

impl Sources {
    pub fn new() -> Self {
        // Order matters: GPU/PDH/DXGI initialisation happens here, on the
        // sampler thread, not on the UI thread.
        let disk = disk::DiskSampler::new();
        let gpu = GpuSource::new();
        Self {
            cpu: cpu::CpuSampler::new(),
            gpu,
            disk: Some(disk),
            net: network::NetSampler::new(),
        }
    }

    pub fn reset(&mut self) {
        self.cpu.reset();
        self.gpu.reset();
        self.net.reset();
    }

    pub fn sample(&mut self) -> Snapshot {
        let mut timings = SourceTimings::default();

        let started = Instant::now();
        let cpu_pct = self.cpu.sample();
        timings.cpu_us = micros(started);

        let started = Instant::now();
        let memory = memory::sample();
        timings.mem_us = micros(started);

        let started = Instant::now();
        let gpu = self.gpu.sample();
        timings.gpu_us = micros(started);

        let started = Instant::now();
        let disk = self.disk.as_mut().and_then(|sampler| sampler.sample());
        timings.disk_us = micros(started);

        let started = Instant::now();
        let net = self.net.sample();
        timings.net_us = micros(started);

        Snapshot {
            cpu_pct,
            memory,
            gpu,
            disk,
            net,
            timings,
        }
    }

    pub fn describe(&self) -> String {
        let disk = self
            .disk
            .as_ref()
            .map(|_| "pdh physical-disk + system volume")
            .unwrap_or("unavailable");
        format!(
            "{} | disk: {} | net: {}",
            self.gpu.status(),
            disk,
            self.net.alias()
        )
    }
}

fn micros(started: Instant) -> u64 {
    started.elapsed().as_micros() as u64
}

/// Human-readable probe used to verify every source on the target machine.
pub fn run_probe(seconds: u64) {
    println!("LitePulse {} probe", env!("CARGO_PKG_VERSION"));
    let mut sources = Sources::new();
    println!("sources: {}", sources.describe());
    std::thread::sleep(Duration::from_millis(250));

    for tick in 0..seconds {
        let snapshot = sources.sample();
        let cpu = snapshot
            .cpu_pct
            .map(format_percent)
            .unwrap_or_else(|| "--".to_string());
        let memory = snapshot
            .memory
            .as_ref()
            .map(|m| {
                format!(
                    "{} ({}/{})",
                    format_percent(m.percent),
                    format_capacity(m.used),
                    format_capacity(m.total)
                )
            })
            .unwrap_or_else(|| "--".to_string());
        let gpu = match &snapshot.gpu {
            Some(gpu) => {
                let temp = gpu.temp_c.map(|t| format!(" {t}C")).unwrap_or_default();
                let vram = match (gpu.vram_used, gpu.vram_total) {
                    (Some(used), Some(total)) => {
                        format!(
                            ", vram {}/{},",
                            format_capacity(used),
                            format_capacity(total)
                        )
                    }
                    _ => ", vram --,".to_string(),
                };
                format!(
                    "{}{}{} {}",
                    format_percent(gpu.util_pct),
                    temp,
                    vram,
                    gpu.short_name()
                )
            }
            None => "--".to_string(),
        };
        let disk = snapshot
            .disk
            .as_ref()
            .map(|disk| {
                let active = disk
                    .active_pct
                    .map(format_percent)
                    .unwrap_or_else(|| "--".to_string());
                format!(
                    "active {active}, r {}, w {}, {} {:.0}% used",
                    disk.read_bps
                        .map(format_rate)
                        .unwrap_or_else(|| "--".into()),
                    disk.write_bps
                        .map(format_rate)
                        .unwrap_or_else(|| "--".into()),
                    disk.volume_letter,
                    disk.volume_percent()
                )
            })
            .unwrap_or_else(|| "--".to_string());
        let net = snapshot
            .net
            .as_ref()
            .map(|net| {
                if net.connected {
                    format!(
                        "down {}, up {}, if {}",
                        format_rate(net.down_bps),
                        format_rate(net.up_bps),
                        net.alias
                    )
                } else {
                    format!("disconnected ({})", net.alias)
                }
            })
            .unwrap_or_else(|| "--".to_string());

        println!(
            "t={:>2}s cpu={:<5} mem={:<28} gpu={:<46} disk={:<48} net={}",
            tick + 1,
            cpu,
            memory,
            gpu,
            disk,
            net
        );
        println!(
            "      timings_us cpu={} mem={} gpu={} disk={} net={}",
            snapshot.timings.cpu_us,
            snapshot.timings.mem_us,
            snapshot.timings.gpu_us,
            snapshot.timings.disk_us,
            snapshot.timings.net_us
        );

        if tick + 1 < seconds {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

enum Control {
    Wake,
    Stop,
}

pub struct SamplerHandle {
    latest: Arc<Mutex<Option<Snapshot>>>,
    paused: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    interval_ms: Arc<AtomicU64>,
    stop_tx: Option<mpsc::Sender<Control>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SamplerHandle {
    pub fn start(hwnd: isize, interval_ms: u32) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let paused = Arc::new(AtomicBool::new(false));
        let reset = Arc::new(AtomicBool::new(false));
        let interval = Arc::new(AtomicU64::new(bounded_interval(interval_ms)));
        let (stop_tx, stop_rx) = mpsc::channel::<Control>();

        let thread_latest = Arc::clone(&latest);
        let thread_paused = Arc::clone(&paused);
        let thread_reset = Arc::clone(&reset);
        let thread_interval = Arc::clone(&interval);

        let thread = std::thread::Builder::new()
            .name("litepulse-sampler".to_string())
            .stack_size(256 * 1024)
            .spawn(move || {
                sampler_loop(
                    hwnd,
                    stop_rx,
                    thread_latest,
                    thread_paused,
                    thread_reset,
                    thread_interval,
                );
            })
            .map_err(|err| {
                logging::log(&format!("could not start sampler thread: {err}"));
                err
            })
            .ok();

        Self {
            latest,
            paused,
            reset,
            interval_ms: interval,
            stop_tx: Some(stop_tx),
            thread,
        }
    }

    pub fn latest(&self) -> Option<Snapshot> {
        self.latest.lock().ok().and_then(|slot| slot.clone())
    }

    pub fn set_interval_ms(&self, interval_ms: u32) {
        self.interval_ms
            .store(bounded_interval(interval_ms), Ordering::Relaxed);
        if let Some(sender) = &self.stop_tx {
            let _ = sender.send(Control::Wake);
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        if !paused {
            self.reset.store(true, Ordering::Relaxed);
        }
    }

    pub fn request_reset(&self) {
        self.reset.store(true, Ordering::Relaxed);
    }

    pub fn stop(&mut self) {
        if let Some(sender) = self.stop_tx.take() {
            let _ = sender.send(Control::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SamplerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn sampler_loop(
    hwnd: isize,
    stop_rx: mpsc::Receiver<Control>,
    latest: Arc<Mutex<Option<Snapshot>>>,
    paused: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
    interval_ms: Arc<AtomicU64>,
) {
    let hwnd = HWND(hwnd as *mut c_void);
    let mut sources = Sources::new();

    // Give the rate counters a baseline before the first sample.
    if wait_or_stop(&stop_rx, 250).is_err() {
        return;
    }

    loop {
        if paused.load(Ordering::Relaxed) {
            if wait_or_stop(&stop_rx, 500).is_err() {
                return;
            }
            continue;
        }

        if reset.swap(false, Ordering::Relaxed) {
            sources.reset();
            if wait_or_stop(&stop_rx, 200).is_err() {
                return;
            }
        }

        sources
            .net
            .set_interval_ms(interval_ms.load(Ordering::Relaxed) as u32);
        let snapshot = sources.sample();
        if let Ok(mut slot) = latest.lock() {
            *slot = Some(snapshot);
        }
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_SAMPLE, WPARAM(0), LPARAM(0));
        }

        let wait = interval_ms.load(Ordering::Relaxed);
        if wait_or_stop(&stop_rx, wait).is_err() {
            return;
        }
    }
}

fn bounded_interval(interval_ms: u32) -> u64 {
    interval_ms.clamp(500, crate::config::MAX_INTERVAL_MS) as u64
}

/// `Ok` means continue, `Err` means stop.
fn wait_or_stop(stop_rx: &mpsc::Receiver<Control>, millis: u64) -> Result<(), ()> {
    match stop_rx.recv_timeout(Duration::from_millis(millis)) {
        Ok(Control::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => Err(()),
        Ok(Control::Wake) => Ok(()),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_long_intervals_wakes_the_sampler_and_stop_remains_interruptible() {
        let (tx, rx) = mpsc::channel();
        let mut handle = SamplerHandle {
            latest: Arc::new(Mutex::new(None)),
            paused: Arc::new(AtomicBool::new(false)),
            reset: Arc::new(AtomicBool::new(false)),
            interval_ms: Arc::new(AtomicU64::new(1000)),
            stop_tx: Some(tx),
            thread: None,
        };
        for ms in [30_000, 60_000, 1000] {
            handle.set_interval_ms(ms);
            assert_eq!(handle.interval_ms.load(Ordering::Relaxed), ms as u64);
            assert!(matches!(rx.try_recv(), Ok(Control::Wake)));
        }
        handle.stop();
        assert!(wait_or_stop(&rx, 60_000).is_err());
        assert_eq!(bounded_interval(120_000), 60_000);
    }
}
