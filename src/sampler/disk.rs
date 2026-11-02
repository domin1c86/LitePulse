use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
    PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
    PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows::core::PCWSTR;

use crate::metrics::DiskInfo;
use crate::win32::{logging, to_wide};

pub struct DiskSampler {
    query: PDH_HQUERY,
    idle: Option<PDH_HCOUNTER>,
    read: Option<PDH_HCOUNTER>,
    write: Option<PDH_HCOUNTER>,
    volume_root: Vec<u16>,
    volume_letter: String,
}

impl DiskSampler {
    pub fn new() -> Self {
        let (volume_root, volume_letter) = system_volume();
        let mut sampler = Self {
            query: PDH_HQUERY::default(),
            idle: None,
            read: None,
            write: None,
            volume_root,
            volume_letter,
        };
        sampler.open_pdh();
        sampler
    }

    fn open_pdh(&mut self) {
        let mut query = PDH_HQUERY::default();
        let result = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
        if result != 0 {
            logging::log(&format!("PdhOpenQueryW failed: {result:#x}"));
            return;
        }

        let mut idle = PDH_HCOUNTER::default();
        let mut read = PDH_HCOUNTER::default();
        let mut write = PDH_HCOUNTER::default();
        let paths = [
            (r"\PhysicalDisk(_Total)\% Idle Time", &mut idle),
            (r"\PhysicalDisk(_Total)\Disk Read Bytes/sec", &mut read),
            (r"\PhysicalDisk(_Total)\Disk Write Bytes/sec", &mut write),
        ];
        for (path, counter) in paths {
            let wide = to_wide(path);
            let result = unsafe { PdhAddEnglishCounterW(query, PCWSTR(wide.as_ptr()), 0, counter) };
            if result != 0 {
                logging::log(&format!(
                    "PdhAddEnglishCounterW failed for {path}: {result:#x}"
                ));
                unsafe {
                    PdhCloseQuery(query);
                }
                return;
            }
        }

        // First collect primes the rate counters; the first sample after this
        // will be valid.
        let _ = unsafe { PdhCollectQueryData(query) };
        self.query = query;
        self.idle = Some(idle);
        self.read = Some(read);
        self.write = Some(write);
    }

    pub fn sample(&mut self) -> Option<DiskInfo> {
        let (volume_total, volume_free) = self.volume_space()?;
        let volume_used = volume_total.saturating_sub(volume_free);

        let mut active = None;
        let mut read_bps = None;
        let mut write_bps = None;

        if !self.query.is_invalid() {
            let collected = unsafe { PdhCollectQueryData(self.query) };
            if collected == 0 {
                active = self
                    .idle
                    .and_then(counter_value)
                    .map(|idle| (100.0 - idle).clamp(0.0, 100.0) as f32);
                read_bps = self.read.and_then(counter_value);
                write_bps = self.write.and_then(counter_value);
            }
        }

        Some(DiskInfo {
            active_pct: active,
            read_bps,
            write_bps,
            volume_letter: self.volume_letter.clone(),
            volume_used,
            volume_total,
        })
    }

    fn volume_space(&self) -> Option<(u64, u64)> {
        let mut total = 0u64;
        let mut total_free = 0u64;
        let result = unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(self.volume_root.as_ptr()),
                None,
                Some(&mut total as *mut u64),
                Some(&mut total_free as *mut u64),
            )
        };
        match result {
            Ok(()) => Some((total, total_free)),
            Err(err) => {
                logging::log(&format!("GetDiskFreeSpaceExW failed: {err}"));
                None
            }
        }
    }
}

impl Drop for DiskSampler {
    fn drop(&mut self) {
        if !self.query.is_invalid() {
            unsafe {
                PdhCloseQuery(self.query);
            }
            self.query = PDH_HQUERY::default();
        }
    }
}

fn counter_value(counter: PDH_HCOUNTER) -> Option<f64> {
    let mut value = PDH_FMT_COUNTERVALUE::default();
    let result = unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut value) };
    if result != 0 {
        return None;
    }
    if value.CStatus != PDH_CSTATUS_VALID_DATA && value.CStatus != PDH_CSTATUS_NEW_DATA {
        return None;
    }
    let number = unsafe { value.Anonymous.doubleValue };
    if number.is_finite() && number >= 0.0 {
        Some(number)
    } else {
        None
    }
}

fn system_volume() -> (Vec<u16>, String) {
    let mut buffer = [0u16; 260];
    let length = unsafe { GetWindowsDirectoryW(Some(&mut buffer)) } as usize;
    let path = if length > 3 && length <= buffer.len() {
        &buffer[..length]
    } else {
        &buffer[..0]
    };
    if path.len() >= 3
        && path[1] == b':' as u16
        && (path[2] == b'\\' as u16 || path[2] == b'/' as u16)
    {
        let letter = String::from_utf16_lossy(&path[..2]);
        let root = vec![path[0], b':' as u16, b'\\' as u16, 0];
        (root, letter)
    } else {
        (to_wide(r"C:\"), "C:".to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn active_time_is_the_inverse_of_idle() {
        let idle: f64 = 90.0;
        assert_eq!((100.0 - idle).clamp(0.0, 100.0), 10.0);
    }
}
