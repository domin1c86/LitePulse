/// Per-source sampling cost, useful for the `--probe` diagnostics.
#[derive(Clone, Copy, Debug, Default)]
pub struct SourceTimings {
    pub cpu_us: u64,
    pub mem_us: u64,
    pub gpu_us: u64,
    pub disk_us: u64,
    pub net_us: u64,
}

#[derive(Clone, Debug)]
pub struct MemoryInfo {
    pub used: u64,
    pub total: u64,
    pub percent: f32,
}

#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    pub util_pct: f32,
    pub temp_c: Option<u32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
}

impl GpuInfo {
    pub fn vram_percent(&self) -> Option<f32> {
        match (self.vram_used, self.vram_total) {
            (Some(used), Some(total)) if total > 0 => {
                Some((used as f32 / total as f32 * 100.0).clamp(0.0, 100.0))
            }
            _ => None,
        }
    }

    pub fn short_name(&self) -> String {
        shorten_gpu_name(&self.name)
    }
}

#[derive(Clone, Debug)]
pub struct DiskInfo {
    pub active_pct: Option<f32>,
    pub read_bps: Option<f64>,
    pub write_bps: Option<f64>,
    pub volume_letter: String,
    pub volume_used: u64,
    pub volume_total: u64,
}

impl DiskInfo {
    pub fn volume_percent(&self) -> f32 {
        if self.volume_total == 0 {
            0.0
        } else {
            (self.volume_used as f32 / self.volume_total as f32 * 100.0).clamp(0.0, 100.0)
        }
    }
}

#[derive(Clone, Debug)]
pub struct NetInfo {
    pub down_bps: f64,
    pub up_bps: f64,
    pub alias: String,
    pub connected: bool,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub cpu_pct: Option<f32>,
    pub memory: Option<MemoryInfo>,
    pub gpu: Option<GpuInfo>,
    pub disk: Option<DiskInfo>,
    pub net: Option<NetInfo>,
    pub timings: SourceTimings,
}

impl Snapshot {
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            cpu_pct: None,
            memory: None,
            gpu: None,
            disk: None,
            net: None,
            timings: SourceTimings::default(),
        }
    }
}

pub fn format_percent(value: f32) -> String {
    format!("{:.0}%", value.clamp(0.0, 100.0))
}

pub fn format_capacity(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else {
        format!("{:.0} MB", value / MB)
    }
}

pub fn format_capacity_pair(used: u64, total: u64) -> String {
    format!("{} / {}", format_capacity(used), format_capacity(total))
}

pub fn format_rate(bytes_per_second: f64) -> String {
    let bps = if bytes_per_second.is_finite() && bytes_per_second > 0.0 {
        bytes_per_second
    } else {
        0.0
    };
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    if bps < KB {
        "0 KB/s".to_string()
    } else if bps < 10.0 * KB {
        format!("{:.1} KB/s", bps / KB)
    } else if bps < MB {
        format!("{:.0} KB/s", bps / KB)
    } else if bps < 10.0 * MB {
        format!("{:.1} MB/s", bps / MB)
    } else if bps < GB {
        format!("{:.0} MB/s", bps / MB)
    } else {
        format!("{:.1} GB/s", bps / GB)
    }
}

pub fn shorten_gpu_name(name: &str) -> String {
    let mut short = name.trim();
    for prefix in [
        "NVIDIA GeForce ",
        "NVIDIA ",
        "AMD Radeon ",
        "Intel(R) Arc(TM) ",
    ] {
        if let Some(rest) = short.strip_prefix(prefix) {
            short = rest;
            break;
        }
    }
    short.to_string()
}

/// Build the single-line tray tooltip. The shell limits tip text to
/// 127 UTF-16 units plus the terminator, so the result is truncated safely.
pub fn tray_tooltip(snapshot: &Snapshot) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(7);
    parts.push("LitePulse".to_string());

    parts.push(match snapshot.cpu_pct {
        Some(v) => format!("CPU {}", format_percent(v)),
        None => "CPU --".to_string(),
    });

    match &snapshot.memory {
        Some(mem) => parts.push(format!("内存 {}", format_percent(mem.percent))),
        None => parts.push("内存 --".to_string()),
    }

    match &snapshot.gpu {
        Some(gpu) => {
            parts.push(format!("GPU {}", format_percent(gpu.util_pct)));
            if let Some(vram) = gpu.vram_percent() {
                parts.push(format!("显存 {}", format_percent(vram)));
            }
        }
        None => {
            parts.push("GPU --".to_string());
            parts.push("显存 --".to_string());
        }
    }

    match &snapshot.disk {
        Some(disk) => match disk.active_pct {
            Some(active) => parts.push(format!("磁盘 {}", format_percent(active))),
            None => parts.push("磁盘 --".to_string()),
        },
        None => parts.push("磁盘 --".to_string()),
    }

    match &snapshot.net {
        Some(net) if net.connected => parts.push(format!(
            "↓{} ↑{}",
            format_rate(net.down_bps),
            format_rate(net.up_bps)
        )),
        _ => parts.push("网络 --".to_string()),
    }

    truncate_utf16(&parts.join(" · "), 127)
}

/// Truncate to at most `max_units` UTF-16 code units without splitting a
/// surrogate pair. Used for the fixed-size NOTIFYICONDATA tip field.
pub fn truncate_utf16(text: &str, max_units: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() <= max_units {
        return text.to_string();
    }
    let mut cut = max_units;
    if cut > 0 && (0xD800..=0xDBFF).contains(&units[cut - 1]) {
        cut -= 1;
    }
    String::from_utf16_lossy(&units[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_format_sensibly() {
        assert_eq!(format_rate(0.0), "0 KB/s");
        assert_eq!(format_rate(512.0), "0 KB/s");
        assert_eq!(format_rate(1536.0), "1.5 KB/s");
        assert_eq!(format_rate(200.0 * 1024.0), "200 KB/s");
        assert_eq!(format_rate(1.25 * 1024.0 * 1024.0), "1.2 MB/s");
        assert_eq!(format_rate(20.0 * 1024.0 * 1024.0), "20 MB/s");
        assert_eq!(format_rate(-5.0), "0 KB/s");
        assert_eq!(format_rate(f64::NAN), "0 KB/s");
    }

    #[test]
    fn capacities_use_binary_units() {
        assert_eq!(format_capacity(0), "0 MB");
        assert_eq!(format_capacity(512 * 1024 * 1024), "512 MB");
        assert_eq!(format_capacity(8 * 1024 * 1024 * 1024), "8.0 GB");
        assert_eq!(
            format_capacity_pair(3 * 1024 * 1024 * 1024, 8 * 1024 * 1024 * 1024),
            "3.0 GB / 8.0 GB"
        );
    }

    #[test]
    fn gpu_names_are_shortened() {
        assert_eq!(
            shorten_gpu_name("NVIDIA GeForce RTX 5060 Laptop GPU"),
            "RTX 5060 Laptop GPU"
        );
        assert_eq!(
            shorten_gpu_name("Intel(R) Arc(TM) A770 Graphics"),
            "A770 Graphics"
        );
        assert_eq!(shorten_gpu_name("Custom GPU"), "Custom GPU");
    }

    #[test]
    fn tooltip_is_always_within_limit() {
        let mut snapshot = Snapshot::empty();
        snapshot.cpu_pct = Some(23.4);
        snapshot.memory = Some(MemoryInfo {
            used: 12_400_000_000,
            total: 34_000_000_000,
            percent: 39.0,
        });
        snapshot.gpu = Some(GpuInfo {
            name: "NVIDIA GeForce RTX 5060 Laptop GPU".into(),
            util_pct: 45.0,
            temp_c: Some(53),
            vram_used: Some(3_100_000_000),
            vram_total: Some(8_500_000_000),
        });
        snapshot.disk = Some(DiskInfo {
            active_pct: Some(12.0),
            read_bps: Some(3_200_000.0),
            write_bps: Some(400_000.0),
            volume_letter: "C:".into(),
            volume_used: 460_000_000_000,
            volume_total: 512_000_000_000,
        });
        snapshot.net = Some(NetInfo {
            down_bps: 1_200_000.0,
            up_bps: 300_000.0,
            alias: "WLAN".into(),
            connected: true,
        });
        let tip = tray_tooltip(&snapshot);
        assert!(tip.contains("CPU 23%"));
        assert!(tip.contains("显存 36%"));
        assert!(tip.encode_utf16().count() <= 127);

        let giant = "测".repeat(200);
        assert!(truncate_utf16(&giant, 127).encode_utf16().count() <= 127);
        let emoji = "😀".repeat(80);
        assert!(truncate_utf16(&emoji, 127).encode_utf16().count() <= 127);
    }
}
