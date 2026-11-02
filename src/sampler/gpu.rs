use std::ffi::{CString, c_char, c_void};
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{PCSTR, PCWSTR};

use crate::metrics::GpuInfo;
use crate::win32::{logging, to_wide};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NvmlMemory {
    total: u64,
    free: u64,
    used: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NvmlUtilization {
    gpu: u32,
    memory: u32,
}

type FnInit = unsafe extern "system" fn() -> i32;
type FnShutdown = unsafe extern "system" fn() -> i32;
type FnCount = unsafe extern "system" fn(*mut u32) -> i32;
type FnHandle = unsafe extern "system" fn(u32, *mut *mut c_void) -> i32;
type FnName = unsafe extern "system" fn(*mut c_void, *mut c_char, u32) -> i32;
type FnMemory = unsafe extern "system" fn(*mut c_void, *mut NvmlMemory) -> i32;
type FnUtilization = unsafe extern "system" fn(*mut c_void, *mut NvmlUtilization) -> i32;
type FnTemperature = unsafe extern "system" fn(*mut c_void, u32, *mut u32) -> i32;

struct NvmlApi {
    module: HMODULE,
    init: FnInit,
    shutdown: FnShutdown,
    count: FnCount,
    handle: FnHandle,
    name: FnName,
    memory: FnMemory,
    utilization: FnUtilization,
    temperature: Option<FnTemperature>,
    initialized: bool,
}

impl NvmlApi {
    fn load() -> Option<Self> {
        let wide = to_wide("nvml.dll");
        let module =
            unsafe { LoadLibraryExW(PCWSTR(wide.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .ok()?;

        let init: Option<FnInit> =
            symbol(module, "nvmlInit_v2").or_else(|| symbol(module, "nvmlInit"));
        let shutdown: Option<FnShutdown> = symbol(module, "nvmlShutdown");
        let count: Option<FnCount> = symbol(module, "nvmlDeviceGetCount_v2")
            .or_else(|| symbol(module, "nvmlDeviceGetCount"));
        let handle: Option<FnHandle> = symbol(module, "nvmlDeviceGetHandleByIndex_v2")
            .or_else(|| symbol(module, "nvmlDeviceGetHandleByIndex"));
        let name: Option<FnName> = symbol(module, "nvmlDeviceGetName");
        let memory: Option<FnMemory> = symbol(module, "nvmlDeviceGetMemoryInfo");
        let utilization: Option<FnUtilization> = symbol(module, "nvmlDeviceGetUtilizationRates");
        let temperature: Option<FnTemperature> = symbol(module, "nvmlDeviceGetTemperature");

        let (
            Some(init),
            Some(shutdown),
            Some(count),
            Some(handle),
            Some(name),
            Some(memory),
            Some(utilization),
        ) = (init, shutdown, count, handle, name, memory, utilization)
        else {
            unsafe {
                let _ = FreeLibrary(module);
            }
            return None;
        };

        let mut api = Self {
            module,
            init,
            shutdown,
            count,
            handle,
            name,
            memory,
            utilization,
            temperature,
            initialized: false,
        };

        if unsafe { (api.init)() } != 0 {
            unsafe {
                let _ = FreeLibrary(module);
            }
            return None;
        }
        api.initialized = true;
        Some(api)
    }

    fn select_device(&self) -> Option<(*mut c_void, String)> {
        let mut count = 0u32;
        if unsafe { (self.count)(&mut count) } != 0 || count == 0 {
            return None;
        }

        let mut best: Option<(*mut c_void, String)> = None;
        let mut best_memory = 0u64;
        for index in 0..count {
            let mut device: *mut c_void = null_mut();
            if unsafe { (self.handle)(index, &mut device) } != 0 || device.is_null() {
                continue;
            }
            let mut memory = NvmlMemory::default();
            let total = if unsafe { (self.memory)(device, &mut memory) } == 0 {
                memory.total
            } else {
                0
            };
            let name = self.device_name(device);
            if best.is_none() || total > best_memory {
                best = Some((device, name));
                best_memory = total;
            }
        }
        best
    }

    fn device_name(&self, device: *mut c_void) -> String {
        let mut buffer = [0 as c_char; 96];
        let result = unsafe { (self.name)(device, buffer.as_mut_ptr(), buffer.len() as u32) };
        if result != 0 {
            return "NVIDIA GPU".to_string();
        }
        let bytes: Vec<u8> = buffer
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        let name = String::from_utf8_lossy(&bytes).trim().to_string();
        if name.is_empty() {
            "NVIDIA GPU".to_string()
        } else {
            name
        }
    }
}

impl Drop for NvmlApi {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                let _ = (self.shutdown)();
            }
        }
        unsafe {
            let _ = FreeLibrary(self.module);
        }
    }
}

fn symbol<T: Copy>(module: HMODULE, name: &str) -> Option<T> {
    let name = CString::new(name).ok()?;
    let function = unsafe { GetProcAddress(module, PCSTR(name.as_ptr() as *const u8)) }?;
    Some(unsafe { std::mem::transmute_copy(&function) })
}

pub struct GpuSampler {
    api: Option<NvmlApi>,
    device: Option<*mut c_void>,
    name: String,
    last_attempt: Option<Instant>,
    logged_failure: bool,
}

impl GpuSampler {
    pub fn new() -> Self {
        let mut sampler = Self {
            api: None,
            device: None,
            name: String::new(),
            last_attempt: None,
            logged_failure: false,
        };
        sampler.ensure_loaded();
        sampler
    }

    pub fn reset(&mut self) {
        // NVML has no rate counters that need a baseline.
    }

    pub fn status(&self) -> String {
        if self.api.is_some() {
            format!("nvml device=\"{}\"", self.name)
        } else {
            "nvml unavailable".to_string()
        }
    }

    fn ensure_loaded(&mut self) {
        if self.api.is_some() {
            return;
        }
        if let Some(last) = self.last_attempt {
            if last.elapsed() < Duration::from_secs(60) {
                return;
            }
        }
        self.last_attempt = Some(Instant::now());

        let Some(api) = NvmlApi::load() else {
            if !self.logged_failure {
                logging::log("NVML unavailable; GPU metrics disabled");
                self.logged_failure = true;
            }
            return;
        };
        match api.select_device() {
            Some((device, name)) => {
                logging::log(&format!("NVML ready: {name}"));
                self.name = name;
                self.device = Some(device);
                self.api = Some(api);
                self.logged_failure = false;
            }
            None => {
                logging::log("NVML initialized but no usable device was found");
                self.logged_failure = true;
            }
        }
    }

    pub fn sample(&mut self) -> Option<GpuInfo> {
        self.ensure_loaded();
        let device = self.device?;
        let api = self.api.as_ref()?;

        let mut utilization = NvmlUtilization::default();
        if unsafe { (api.utilization)(device, &mut utilization) } != 0 {
            return None;
        }

        let mut memory = NvmlMemory::default();
        let (vram_used, vram_total) =
            if unsafe { (api.memory)(device, &mut memory) } == 0 && memory.total > 0 {
                (Some(memory.used), Some(memory.total))
            } else {
                (None, None)
            };

        let temp_c = api.temperature.and_then(|read_temperature| {
            let mut temperature = 0u32;
            if unsafe { read_temperature(device, 0, &mut temperature) } == 0 && temperature > 0 {
                Some(temperature)
            } else {
                None
            }
        });

        Some(GpuInfo {
            name: self.name.clone(),
            util_pct: (utilization.gpu.min(100)) as f32,
            temp_c,
            vram_used,
            vram_total,
        })
    }
}
