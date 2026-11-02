use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::GetSystemTimes;

#[derive(Debug, Default)]
pub struct CpuSampler {
    previous: Option<(u64, u64, u64)>,
}

impl CpuSampler {
    pub fn new() -> Self {
        let mut sampler = Self::default();
        sampler.reset();
        sampler
    }

    /// Re-baseline the counters (after resume/unlock or a counter glitch).
    pub fn reset(&mut self) {
        self.previous = read_times();
    }

    pub fn sample(&mut self) -> Option<f32> {
        let current = read_times()?;
        let previous = self.previous.replace(current);
        let previous = previous?;
        usage_from_times(previous, current)
    }
}

fn read_times() -> Option<(u64, u64, u64)> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetSystemTimes(
            Some(&mut idle as *mut FILETIME),
            Some(&mut kernel as *mut FILETIME),
            Some(&mut user as *mut FILETIME),
        )
        .ok()?;
    }
    Some((filetime(idle), filetime(kernel), filetime(user)))
}

fn filetime(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
}

/// `kernel` includes idle time, so total = kernel delta + user delta.
fn usage_from_times(previous: (u64, u64, u64), current: (u64, u64, u64)) -> Option<f32> {
    if current.0 < previous.0 || current.1 < previous.1 || current.2 < previous.2 {
        return None;
    }
    let idle_delta = current.0 - previous.0;
    let total_delta = (current.1 - previous.1) + (current.2 - previous.2);
    if total_delta == 0 {
        return None;
    }
    let busy = total_delta.saturating_sub(idle_delta);
    Some(((busy as f64 / total_delta as f64) * 100.0).clamp(0.0, 100.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_the_non_idle_share() {
        let previous = (100, 1_000, 500);
        let current = (200, 1_300, 700);
        // total delta = 300 + 200 = 500, idle delta = 100 -> 80%
        assert_eq!(usage_from_times(previous, current), Some(80.0));
    }

    #[test]
    fn zero_interval_is_rejected() {
        assert_eq!(usage_from_times((1, 2, 3), (1, 2, 3)), None);
    }

    #[test]
    fn backwards_counters_are_rejected() {
        assert_eq!(usage_from_times((10, 20, 30), (9, 20, 30)), None);
    }
}
