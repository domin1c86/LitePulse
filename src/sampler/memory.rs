use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use crate::metrics::MemoryInfo;

pub fn sample() -> Option<MemoryInfo> {
    let mut memory = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe {
        GlobalMemoryStatusEx(&mut memory).ok()?;
    }
    let total = memory.ullTotalPhys;
    let available = memory.ullAvailPhys;
    let used = total.saturating_sub(available);
    Some(MemoryInfo {
        used,
        total,
        percent: (memory.dwMemoryLoad as f32).clamp(0.0, 100.0),
    })
}
