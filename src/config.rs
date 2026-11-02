use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::win32::{data_dir, font, logging};

pub const MAX_INTERVAL_MS: u32 = 60_000;
pub const INTERVAL_OPTIONS_MS: [u32; 5] = [1000, 2000, 5000, 30_000, MAX_INTERVAL_MS];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSetting {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayMetric {
    #[default]
    Cpu,
    Memory,
    Gpu,
    Disk,
}

impl TrayMetric {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU 利用率",
            Self::Memory => "内存占用",
            Self::Gpu => "显存占用",
            Self::Disk => "磁盘活动时间",
        }
    }

    pub fn load(self, snapshot: &crate::metrics::Snapshot) -> Option<f32> {
        match self {
            Self::Cpu => snapshot.cpu_pct,
            Self::Memory => snapshot.memory.as_ref().map(|m| m.percent),
            Self::Gpu => snapshot.gpu.as_ref().and_then(|g| g.vram_percent()),
            Self::Disk => snapshot.disk.as_ref().and_then(|d| d.active_pct),
        }
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 100.0))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub theme: ThemeSetting,
    pub interval_ms: u32,
    pub always_on_top: bool,
    pub tray_metric: TrayMetric,
    pub font_family: String,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            theme: ThemeSetting::System,
            interval_ms: 1000,
            always_on_top: true,
            tray_metric: TrayMetric::Cpu,
            font_family: font::DEFAULT_FAMILY.to_string(),
            window_x: None,
            window_y: None,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        data_dir().join("settings.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => return Self::default(),
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(err) => {
                logging::log(&format!("settings parse failed, using defaults: {err}"));
                Self::default()
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn sanitize(&mut self) {
        if self.version == 0 {
            self.version = 1;
        }
        if !INTERVAL_OPTIONS_MS.contains(&self.interval_ms) {
            self.interval_ms = 1000;
        }
        let family = self.font_family.trim();
        if family.is_empty() {
            self.font_family = font::DEFAULT_FAMILY.to_string();
        } else if family != self.font_family {
            self.font_family = family.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let settings = Settings::default();
        assert_eq!(settings.interval_ms, 1000);
        assert!(settings.always_on_top);
        assert_eq!(settings.theme, ThemeSetting::System);
        assert_eq!(settings.font_family, font::DEFAULT_FAMILY);
    }

    #[test]
    fn roundtrip_preserves_values() {
        let settings = Settings {
            version: 1,
            theme: ThemeSetting::Dark,
            interval_ms: 2000,
            always_on_top: false,
            tray_metric: TrayMetric::Memory,
            font_family: "Microsoft YaHei UI".to_string(),
            window_x: Some(100),
            window_y: Some(-20),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.theme, ThemeSetting::Dark);
        assert_eq!(parsed.interval_ms, 2000);
        assert_eq!(parsed.window_x, Some(100));
        assert!(!parsed.always_on_top);
        assert_eq!(parsed.font_family, "Microsoft YaHei UI");
        assert_eq!(parsed.tray_metric, TrayMetric::Memory);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let parsed: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.interval_ms, 1000);
        assert_eq!(parsed.theme, ThemeSetting::System);
        assert_eq!(parsed.font_family, font::DEFAULT_FAMILY);
        assert_eq!(parsed.tray_metric, TrayMetric::Cpu);
    }

    #[test]
    fn invalid_interval_is_repaired() {
        let mut parsed: Settings = serde_json::from_str(r#"{"interval_ms": 333}"#).unwrap();
        parsed.sanitize();
        assert_eq!(parsed.interval_ms, 1000);
    }

    #[test]
    fn long_intervals_survive_save_load_and_sanitization() {
        for interval_ms in [30_000, 60_000] {
            let settings = Settings {
                interval_ms,
                ..Settings::default()
            };
            let json = serde_json::to_string(&settings).unwrap();
            let mut parsed: Settings = serde_json::from_str(&json).unwrap();
            parsed.sanitize();
            assert_eq!(parsed.interval_ms, interval_ms);
        }
    }

    #[test]
    fn blank_font_family_is_repaired() {
        let mut parsed: Settings = serde_json::from_str(r#"{"font_family": "  "}"#).unwrap();
        parsed.sanitize();
        assert_eq!(parsed.font_family, font::DEFAULT_FAMILY);
    }

    #[test]
    fn tray_metric_uses_only_the_selected_source() {
        use crate::metrics::{DiskInfo, GpuInfo, MemoryInfo, Snapshot};
        let mut sample = Snapshot::empty();
        for metric in [
            TrayMetric::Cpu,
            TrayMetric::Memory,
            TrayMetric::Gpu,
            TrayMetric::Disk,
        ] {
            assert_eq!(metric.load(&sample), None);
        }
        sample.cpu_pct = Some(12.0);
        sample.memory = Some(MemoryInfo {
            used: 90,
            total: 100,
            percent: 90.0,
        });
        sample.gpu = Some(GpuInfo {
            name: "GPU".into(),
            util_pct: 99.0,
            temp_c: None,
            vram_used: Some(25),
            vram_total: Some(100),
        });
        sample.disk = Some(DiskInfo {
            active_pct: Some(3.0),
            read_bps: None,
            write_bps: None,
            volume_letter: "C:".into(),
            volume_used: 95,
            volume_total: 100,
        });
        assert_eq!(TrayMetric::Cpu.load(&sample), Some(12.0));
        assert_eq!(TrayMetric::Memory.load(&sample), Some(90.0));
        assert_eq!(TrayMetric::Gpu.load(&sample), Some(25.0));
        assert_eq!(TrayMetric::Disk.load(&sample), Some(3.0));
        sample.cpu_pct = Some(f32::NAN);
        assert_eq!(TrayMetric::Cpu.load(&sample), None);
        sample.cpu_pct = Some(110.0);
        assert_eq!(TrayMetric::Cpu.load(&sample), Some(100.0));
    }
}
