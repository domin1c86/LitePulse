use std::mem::size_of;

use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
    PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery,
    PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::core::PCWSTR;

use crate::metrics::GpuInfo;
use crate::win32::{logging, to_wide};

struct AdapterInfo {
    name: String,
    /// Lower-case `luid_0x...._0x........` prefix used by GPU performance counters.
    luid_marker: String,
    vram_total: u64,
}

pub struct PdhGpuSampler {
    query: PDH_HQUERY,
    engines: Option<PDH_HCOUNTER>,
    adapter_memory: Option<PDH_HCOUNTER>,
    adapter: Option<AdapterInfo>,
    com_initialized: bool,
}

impl PdhGpuSampler {
    pub fn new() -> Self {
        let com_initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let adapter = pick_adapter();
        let mut sampler = Self {
            query: PDH_HQUERY::default(),
            engines: None,
            adapter_memory: None,
            adapter,
            com_initialized,
        };
        if sampler.adapter.is_some() {
            sampler.open_pdh();
        }
        sampler
    }

    pub fn is_available(&self) -> bool {
        self.adapter.is_some()
            && !self.query.is_invalid()
            && self.engines.is_some()
            && self.adapter_memory.is_some()
    }

    pub fn status(&self) -> String {
        match (&self.adapter, self.is_available()) {
            (Some(adapter), true) => format!("pdh gpu=\"{}\"", adapter.name),
            (Some(adapter), false) => {
                format!("pdh gpu=\"{}\" counters unavailable", adapter.name)
            }
            (None, _) => "pdh gpu unavailable".to_string(),
        }
    }

    fn open_pdh(&mut self) {
        let mut query = PDH_HQUERY::default();
        let result = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
        if result != 0 {
            logging::log(&format!("PdhOpenQueryW(GPU) failed: {result:#x}"));
            return;
        }

        let mut engines = PDH_HCOUNTER::default();
        let mut adapter_memory = PDH_HCOUNTER::default();
        let paths = [
            (r"\GPU Engine(*)\Utilization Percentage", &mut engines),
            (
                r"\GPU Adapter Memory(*)\Dedicated Usage",
                &mut adapter_memory,
            ),
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
        let _ = unsafe { PdhCollectQueryData(query) };
        self.query = query;
        self.engines = Some(engines);
        self.adapter_memory = Some(adapter_memory);
        if let Some(adapter) = &self.adapter {
            logging::log(&format!(
                "GPU counters ready: {} ({} MiB)",
                adapter.name,
                adapter.vram_total / (1024 * 1024)
            ));
        }
    }

    pub fn sample(&mut self) -> Option<GpuInfo> {
        if !self.is_available() {
            return None;
        }
        let adapter = self.adapter.as_ref()?;
        let name = adapter.name.clone();
        let luid = adapter.luid_marker.clone();
        let vram_total = adapter.vram_total;
        let query = self.query;
        let engines = self.engines?;
        let adapter_memory = self.adapter_memory?;

        if unsafe { PdhCollectQueryData(query) } != 0 {
            return None;
        }

        let engine_items = counter_array(engines).unwrap_or_default();
        let utilization = utilization_from_items(&engine_items, &luid);

        let memory_items = counter_array(adapter_memory).unwrap_or_default();
        let vram_used = memory_items
            .iter()
            .filter(|(instance, _)| instance.to_ascii_lowercase().contains(&luid))
            .map(|(_, value)| *value)
            .fold(0.0_f64, f64::max)
            .max(0.0) as u64;

        Some(GpuInfo {
            name,
            util_pct: utilization,
            temp_c: None,
            vram_used: (vram_total > 0).then_some(vram_used),
            vram_total: (vram_total > 0).then_some(vram_total),
        })
    }
}

impl Drop for PdhGpuSampler {
    fn drop(&mut self) {
        if !self.query.is_invalid() {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
        if self.com_initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn pick_adapter() -> Option<AdapterInfo> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    let mut best: Option<AdapterInfo> = None;
    for index in 0..16u32 {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            break;
        };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if desc.Flags & 2 != 0 {
            continue; // DXGI_ADAPTER_FLAG_SOFTWARE
        }
        let end = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..end])
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let vram_total = desc.DedicatedVideoMemory as u64;
        let luid_marker = format!(
            "luid_0x{:08x}_0x{:08x}",
            desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart
        );
        let candidate = AdapterInfo {
            name,
            luid_marker,
            vram_total,
        };
        let replace = best
            .as_ref()
            .map(|current| candidate.vram_total > current.vram_total)
            .unwrap_or(true);
        if replace {
            best = Some(candidate);
        }
    }
    best
}

fn counter_array(counter: PDH_HCOUNTER) -> Option<Vec<(String, f64)>> {
    if counter.is_invalid() {
        return None;
    }
    let mut size = 0u32;
    let mut count = 0u32;
    let result = unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
    };
    if result != PDH_MORE_DATA && result != 0 {
        return Some(Vec::new());
    }
    if count == 0 {
        return Some(Vec::new());
    }

    // PDH's reported size includes both the item array and the instance-name
    // strings packed after it, so it is much larger than count * struct size.
    let item_size = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    let capacity = (size as usize)
        .div_ceil(item_size)
        .max(count as usize)
        .saturating_add(8);
    let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); capacity];
    let mut buffer_size = (items.len() * item_size) as u32;
    let mut item_count = count;
    let result = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut buffer_size,
            &mut item_count,
            Some(items.as_mut_ptr()),
        )
    };
    if result != 0 {
        return None;
    }

    let mut values = Vec::with_capacity(item_count as usize);
    for item in items.iter().take(item_count as usize) {
        if item.FmtValue.CStatus != PDH_CSTATUS_VALID_DATA
            && item.FmtValue.CStatus != PDH_CSTATUS_NEW_DATA
        {
            continue;
        }
        if item.szName.0.is_null() {
            continue;
        }
        let name = match unsafe { PCWSTR(item.szName.0).to_string() } {
            Ok(name) => name,
            Err(_) => continue,
        };
        let value = unsafe { item.FmtValue.Anonymous.doubleValue };
        if value.is_finite() {
            values.push((name, value));
        }
    }
    Some(values)
}

fn utilization_from_items(items: &[(String, f64)], luid_marker: &str) -> f32 {
    // Per-process engine counters overlap: several processes can report
    // activity on the same engine, so summing them over-counts badly. The
    // busiest process/engine is the closest lightweight analogue of the
    // hardware's global busy percentage.
    let busiest = items
        .iter()
        .filter(|(instance, _)| instance.to_ascii_lowercase().contains(luid_marker))
        .map(|(_, value)| value.max(0.0))
        .fold(0.0_f64, f64::max);
    busiest.clamp(0.0, 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(instance: &str, value: f64) -> (String, f64) {
        (instance.to_string(), value)
    }

    #[test]
    fn utilization_uses_the_busiest_instance_for_the_selected_adapter() {
        let items = vec![
            item(
                "pid_1_luid_0x00000000_0x000149b1_phys_0_eng_0_engtype_3D",
                40.0,
            ),
            item(
                "pid_2_luid_0x00000000_0x000149b1_phys_0_eng_0_engtype_3D",
                30.0,
            ),
            item(
                "pid_1_luid_0x00000000_0x000149b1_phys_0_eng_0_engtype_Copy",
                80.0,
            ),
            item(
                "pid_3_luid_0x00000000_0x00015dc9_phys_0_eng_0_engtype_3D",
                99.0,
            ),
        ];
        let utilization = utilization_from_items(&items, "luid_0x00000000_0x000149b1");
        assert_eq!(utilization, 80.0);
    }

    #[test]
    fn no_engine_instances_means_zero() {
        assert_eq!(
            utilization_from_items(&[], "luid_0x00000000_0x000149b1"),
            0.0
        );
    }

    #[test]
    fn utilization_is_capped() {
        let items = vec![item(
            "pid_1_luid_0x00000000_0x000149b1_phys_0_eng_0_engtype_3D",
            140.0,
        )];
        assert_eq!(
            utilization_from_items(&items, "luid_0x00000000_0x000149b1"),
            100.0
        );
    }
}
