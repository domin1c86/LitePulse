use crate::metrics::Snapshot;
use std::collections::VecDeque;

pub const HISTORY_CAP: usize = 60;
pub type Series = VecDeque<Option<f32>>;

#[derive(Debug, Default)]
pub struct History {
    pub cpu: Series,
    pub memory: Series,
    pub gpu: Series,
    pub vram: Series,
    pub disk: Series,
    pub disk_read: Series,
    pub disk_write: Series,
    pub net_down: Series,
    pub net_up: Series,
    network_alias: Option<String>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, snapshot: &Snapshot) {
        push_capped(&mut self.cpu, snapshot.cpu_pct);
        push_capped(
            &mut self.memory,
            snapshot.memory.as_ref().map(|m| m.percent),
        );
        push_capped(&mut self.gpu, snapshot.gpu.as_ref().map(|g| g.util_pct));
        push_capped(
            &mut self.vram,
            snapshot.gpu.as_ref().and_then(|g| g.vram_percent()),
        );
        let disk = snapshot.disk.as_ref();
        push_capped(&mut self.disk, disk.and_then(|d| d.active_pct));
        push_capped(
            &mut self.disk_read,
            disk.and_then(|d| d.read_bps).map(|v| v as f32),
        );
        push_capped(
            &mut self.disk_write,
            disk.and_then(|d| d.write_bps).map(|v| v as f32),
        );
        let net = snapshot.net.as_ref();
        if let Some(net) = net {
            if self.network_alias.as_deref() != Some(&net.alias) {
                self.net_down.clear();
                self.net_up.clear();
                self.network_alias = Some(net.alias.clone());
            }
        }
        let connected = net.filter(|n| n.connected);
        push_capped(&mut self.net_down, connected.map(|n| n.down_bps as f32));
        push_capped(&mut self.net_up, connected.map(|n| n.up_bps as f32));
    }
}
fn push_capped(queue: &mut Series, value: Option<f32>) {
    if queue.len() == HISTORY_CAP {
        queue.pop_front();
    }
    queue.push_back(value.filter(|v| v.is_finite()));
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{DiskInfo, GpuInfo, MemoryInfo, NetInfo};
    #[test]
    fn history_keeps_only_the_latest_points() {
        let mut history = History::new();
        for i in 0..(HISTORY_CAP + 15) {
            let mut sample = Snapshot::empty();
            sample.cpu_pct = Some(i as f32);
            history.push(&sample);
        }
        assert_eq!(history.cpu.len(), HISTORY_CAP);
        assert_eq!(history.cpu.front().copied(), Some(Some(15.0)));
        assert_eq!(
            history.cpu.back().copied(),
            Some(Some((HISTORY_CAP + 14) as f32))
        );
        assert_eq!(history.memory.len(), HISTORY_CAP);
    }
    #[test]
    fn missing_values_leave_gaps_without_shifting_time() {
        let mut history = History::new();
        let mut sample = Snapshot::empty();
        sample.cpu_pct = Some(42.0);
        history.push(&sample);
        history.push(&Snapshot::empty());
        sample.cpu_pct = Some(f32::NAN);
        history.push(&sample);
        assert_eq!(history.cpu, [Some(42.0), None, None]);
        assert_eq!(history.gpu, [None, None, None]);
    }
    #[test]
    fn all_graphs_record_their_own_metric_and_disconnection() {
        let mut history = History::new();
        let mut sample = Snapshot::empty();
        sample.memory = Some(MemoryInfo {
            used: 40,
            total: 100,
            percent: 40.0,
        });
        sample.gpu = Some(GpuInfo {
            name: "GPU".into(),
            util_pct: 20.0,
            temp_c: None,
            vram_used: Some(30),
            vram_total: Some(100),
        });
        sample.disk = Some(DiskInfo {
            active_pct: Some(5.0),
            read_bps: Some(1024.0),
            write_bps: Some(2048.0),
            volume_letter: "C:".into(),
            volume_used: 10,
            volume_total: 100,
        });
        sample.net = Some(NetInfo {
            down_bps: 4096.0,
            up_bps: 512.0,
            alias: "WLAN".into(),
            connected: true,
        });
        history.push(&sample);
        assert_eq!(history.memory[0], Some(40.0));
        assert_eq!(history.gpu[0], Some(20.0));
        assert!((history.vram[0].unwrap() - 30.0).abs() < 0.001);
        assert_eq!(history.disk[0], Some(5.0));
        assert_eq!(history.disk_read[0], Some(1024.0));
        assert_eq!(history.disk_write[0], Some(2048.0));
        assert_eq!(history.net_down[0], Some(4096.0));
        assert_eq!(history.net_up[0], Some(512.0));
        sample.net.as_mut().unwrap().connected = false;
        history.push(&sample);
        assert_eq!(history.net_down[1], None);
        sample.net.as_mut().unwrap().alias = "Ethernet".into();
        history.push(&sample);
        assert_eq!(history.net_up.len(), 1);
    }

    #[test]
    fn unavailable_network_preserves_existing_samples_and_adds_a_gap() {
        let mut history = History::new();
        let mut sample = Snapshot::empty();
        sample.net = Some(NetInfo {
            down_bps: 100.0,
            up_bps: 50.0,
            alias: "WLAN".into(),
            connected: true,
        });
        history.push(&sample);
        history.push(&Snapshot::empty());
        history.push(&sample);
        assert_eq!(history.net_down, [Some(100.0), None, Some(100.0)]);
        assert_eq!(history.net_up, [Some(50.0), None, Some(50.0)]);
    }
}
