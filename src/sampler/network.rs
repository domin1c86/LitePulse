use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::NetworkManagement::IpHelper::{GetBestInterfaceEx, GetIfEntry2, MIB_IF_ROW2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, IN_ADDR, IN_ADDR_0, SOCKADDR, SOCKADDR_IN};

use crate::metrics::NetInfo;
use crate::win32::logging;

#[derive(Debug)]
pub struct NetSampler {
    previous: Option<Previous>,
    alias: String,
    max_gap_seconds: f64,
}

#[derive(Debug, Clone, Copy)]
struct Previous {
    index: u32,
    in_octets: u64,
    out_octets: u64,
    at: std::time::Instant,
}

impl NetSampler {
    pub fn new() -> Self {
        let mut sampler = Self {
            previous: None,
            alias: String::new(),
            max_gap_seconds: 10.0,
        };
        sampler.prime();
        sampler
    }

    pub fn reset(&mut self) {
        self.previous = None;
        self.prime();
    }

    fn prime(&mut self) {
        let Some(index) = best_interface_index() else {
            return;
        };
        if let Some(entry) = read_interface(index) {
            self.previous = Some(Previous {
                index,
                in_octets: entry.in_octets,
                out_octets: entry.out_octets,
                at: std::time::Instant::now(),
            });
            self.alias = entry.alias;
        }
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn set_interval_ms(&mut self, interval_ms: u32) {
        self.max_gap_seconds = (interval_ms as f64 / 1000.0 * 2.0).max(10.0);
    }

    pub fn sample(&mut self) -> Option<NetInfo> {
        let Some(index) = best_interface_index() else {
            self.previous = None;
            self.alias = "无连接".to_string();
            return Some(NetInfo {
                down_bps: 0.0,
                up_bps: 0.0,
                alias: self.alias.clone(),
                connected: false,
            });
        };

        let entry = read_interface(index)?;

        let now = std::time::Instant::now();
        let previous = self.previous.replace(Previous {
            index,
            in_octets: entry.in_octets,
            out_octets: entry.out_octets,
            at: now,
        });
        self.alias = entry.alias.clone();

        let connected = entry.up;
        let (down_bps, up_bps) = if connected {
            rates(
                previous,
                index,
                entry.in_octets,
                entry.out_octets,
                now,
                self.max_gap_seconds,
            )
            .unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };

        Some(NetInfo {
            down_bps,
            up_bps,
            alias: self.alias.clone(),
            connected,
        })
    }
}

struct InterfaceEntry {
    in_octets: u64,
    out_octets: u64,
    alias: String,
    up: bool,
}

fn best_interface_index() -> Option<u32> {
    let destination = SOCKADDR_IN {
        sin_family: AF_INET,
        sin_addr: IN_ADDR {
            S_un: IN_ADDR_0 {
                S_addr: u32::from_ne_bytes([1, 1, 1, 1]),
            },
        },
        ..Default::default()
    };
    let mut index = 0u32;
    let result = unsafe {
        GetBestInterfaceEx(
            &destination as *const SOCKADDR_IN as *const SOCKADDR,
            &mut index,
        )
    };
    if result == 0 && index != 0 {
        Some(index)
    } else {
        None
    }
}

fn read_interface(index: u32) -> Option<InterfaceEntry> {
    let mut row = MIB_IF_ROW2 {
        InterfaceIndex: index,
        ..Default::default()
    };
    let result: WIN32_ERROR = unsafe { GetIfEntry2(&mut row) };
    if result.0 != 0 {
        logging::log(&format!(
            "GetIfEntry2 failed for interface {index}: win32 error {}",
            result.0
        ));
        return None;
    }

    let end = row
        .Alias
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(row.Alias.len());
    let alias = String::from_utf16_lossy(&row.Alias[..end]);
    Some(InterfaceEntry {
        in_octets: row.InOctets,
        out_octets: row.OutOctets,
        alias,
        up: row.OperStatus == IfOperStatusUp,
    })
}

fn rates(
    previous: Option<Previous>,
    index: u32,
    in_octets: u64,
    out_octets: u64,
    now: std::time::Instant,
    max_gap_seconds: f64,
) -> Option<(f64, f64)> {
    let previous = previous?;
    if previous.index != index || now <= previous.at {
        return None;
    }
    let seconds = now.duration_since(previous.at).as_secs_f64();
    if seconds <= 0.01 || seconds > max_gap_seconds {
        // Counter reset or a long pause; avoid reporting a bogus spike.
        return None;
    }
    let down = in_octets.saturating_sub(previous.in_octets) as f64 / seconds;
    let up = out_octets.saturating_sub(previous.out_octets) as f64 / seconds;
    Some((down, up))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn previous() -> Previous {
        Previous {
            index: 15,
            in_octets: 1_000,
            out_octets: 500,
            at: std::time::Instant::now(),
        }
    }

    #[test]
    fn rates_use_actual_elapsed_time() {
        let previous = previous();
        let now = previous.at + std::time::Duration::from_secs(2);
        let (down, up) = rates(Some(previous), 15, 3_000, 1_500, now, 10.0).unwrap();
        assert!((down - 1_000.0).abs() < 0.01);
        assert!((up - 500.0).abs() < 0.01);
    }

    #[test]
    fn interface_change_resets_the_baseline() {
        let previous = previous();
        let now = previous.at + std::time::Duration::from_secs(1);
        assert_eq!(rates(Some(previous), 16, 3_000, 1_500, now, 10.0), None);
    }

    #[test]
    fn long_gap_is_ignored() {
        let previous = previous();
        let now = previous.at + std::time::Duration::from_secs(30);
        assert_eq!(rates(Some(previous), 15, 3_000, 1_500, now, 10.0), None);
    }

    #[test]
    fn long_sampling_intervals_report_rates_but_still_reject_large_pauses() {
        for seconds in [30, 60] {
            let previous = previous();
            let mut sampler = NetSampler {
                previous: None,
                alias: String::new(),
                max_gap_seconds: 10.0,
            };
            sampler.set_interval_ms(seconds * 1000);
            let now = previous.at + std::time::Duration::from_secs(seconds as u64);
            assert_eq!(
                rates(
                    Some(previous),
                    15,
                    1_000 + seconds as u64 * 1_000,
                    500 + seconds as u64 * 500,
                    now,
                    sampler.max_gap_seconds
                ),
                Some((1_000.0, 500.0))
            );
            let late = previous.at + std::time::Duration::from_secs(seconds as u64 * 3);
            assert_eq!(
                rates(
                    Some(previous),
                    15,
                    10_000,
                    5_000,
                    late,
                    sampler.max_gap_seconds
                ),
                None
            );
        }
    }
}
