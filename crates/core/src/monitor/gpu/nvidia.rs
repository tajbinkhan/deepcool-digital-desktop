//! Reads live GPU data from the `libnvidia-ml` shared library.

use libloading::Library;
use std::{ffi::CString, path::Path, ptr::null_mut};

type NvmlInit = unsafe extern "C" fn() -> u32;
type NvmlDeviceGetHandleByPciBusId = unsafe extern "C" fn(pci_bus_id: *const libc_char, device: *mut Device) -> u32;
type NvmlDeviceGetUtilizationRates = unsafe extern "C" fn(device: Device, utilization: *mut Utilization) -> u32;
type NvmlDeviceGetTemperature = unsafe extern "C" fn(device: Device, sensor: u32, temp: *mut u32) -> u32;
type NvmlDeviceGetPowerUsage = unsafe extern "C" fn(device: Device, power: *mut u32) -> u32;
type NvmlDeviceGetMemoryInfo = unsafe extern "C" fn(device: Device, memory: *mut MemoryInfo) -> u32;
type NvmlDeviceGetClockInfo = unsafe extern "C" fn(device: Device, clock_type: u32, clock: *mut u32) -> u32;

#[allow(non_camel_case_types)]
type libc_char = std::os::raw::c_char;
type Device = *mut u8;

#[repr(C)]
struct MemoryInfo {
    total: u64,
    free: u64,
    used: u64,
}

#[repr(C)]
struct Utilization {
    gpu: u32,
    memory: u32,
}

const LIB_PATHS: [&str; 12] = [
    "/usr/lib/x86_64-linux-gnu/nvidia/current/libnvidia-ml.so",
    "/usr/lib/x86_64-linux-gnu/nvidia/current/libnvidia-ml.so.1",
    "/usr/lib/x86_64-linux-gnu/libnvidia-ml.so",
    "/usr/lib/x86_64-linux-gnu/libnvidia-ml.so.1",
    "/usr/lib/libnvidia-ml.so",
    "/usr/lib/libnvidia-ml.so.1",
    "/usr/lib64/libnvidia-ml.so",
    "/usr/lib64/libnvidia-ml.so.1",
    "/usr/lib32/libnvidia-ml.so",
    "/usr/lib32/libnvidia-ml.so.1",
    "/run/opengl-driver/lib/libnvidia-ml.so",
    "/run/opengl-driver/lib/libnvidia-ml.so.1",
];

/// NVML handle for one GPU. Function pointers are resolved once at start-up.
pub struct Gpu {
    // Keeps the library loaded for as long as the function pointers are used.
    _lib: Library,
    device: Device,
    utilization: NvmlDeviceGetUtilizationRates,
    temperature: NvmlDeviceGetTemperature,
    power: NvmlDeviceGetPowerUsage,
    clock: NvmlDeviceGetClockInfo,
    memory: Option<NvmlDeviceGetMemoryInfo>,
}

// NVML is documented as thread-safe; the handle is only an opaque pointer.
unsafe impl Send for Gpu {}

fn load_library() -> Option<Library> {
    unsafe {
        Library::new("libnvidia-ml.so.1").or_else(|_| Library::new("libnvidia-ml.so")).ok().or_else(|| {
            LIB_PATHS
                .iter()
                .filter(|path| Path::new(path).exists())
                .find_map(|path| Library::new(*path).ok())
        })
    }
}

impl Gpu {
    /// Initializes NVML with the GPU at the given PCI address. Returns `None` if
    /// the library or the device is unavailable.
    pub fn new(pci_address: &str) -> Option<Self> {
        let lib = load_library()?;
        let address = CString::new(pci_address).ok()?;
        unsafe {
            let init = *lib.get::<NvmlInit>(b"nvmlInit_v2").ok()?;
            if init() != 0 {
                return None;
            }
            let get_handle = *lib.get::<NvmlDeviceGetHandleByPciBusId>(b"nvmlDeviceGetHandleByPciBusId_v2").ok()?;
            let mut device: Device = null_mut();
            if get_handle(address.as_ptr(), &mut device) != 0 {
                return None;
            }
            Some(Gpu {
                device,
                utilization: *lib.get(b"nvmlDeviceGetUtilizationRates").ok()?,
                temperature: *lib.get(b"nvmlDeviceGetTemperature").ok()?,
                power: *lib.get(b"nvmlDeviceGetPowerUsage").ok()?,
                clock: *lib.get(b"nvmlDeviceGetClockInfo").ok()?,
                memory: lib.get(b"nvmlDeviceGetMemoryInfo").ok().map(|f| *f),
                _lib: lib,
            })
        }
    }

    /// GPU temperature in millidegrees Celsius.
    pub fn temp_milli(&self) -> Option<u32> {
        let mut temp: u32 = 0;
        (unsafe { (self.temperature)(self.device, 0, &mut temp) } == 0).then_some(temp * 1000)
    }

    /// GPU utilization in percent.
    pub fn usage(&self) -> u8 {
        let mut utilization = Utilization { gpu: 0, memory: 0 };
        if unsafe { (self.utilization)(self.device, &mut utilization) } != 0 {
            return 0;
        }
        utilization.gpu.min(100) as u8
    }

    /// GPU power consumption in watts.
    pub fn power_w(&self) -> u16 {
        let mut power: u32 = 0;
        if unsafe { (self.power)(self.device, &mut power) } != 0 {
            return 0;
        }
        (power as f32 / 1000.0).round() as u16
    }

    /// Used and total video memory in bytes.
    pub fn memory(&self) -> Option<(u64, u64)> {
        let get = self.memory?;
        let mut info = MemoryInfo { total: 0, free: 0, used: 0 };
        (unsafe { get(self.device, &mut info) } == 0).then_some((info.used, info.total))
    }

    /// GPU core frequency in MHz.
    pub fn frequency_mhz(&self) -> u16 {
        let mut clock: u32 = 0;
        if unsafe { (self.clock)(self.device, 0, &mut clock) } != 0 {
            return 0;
        }
        clock as u16
    }
}
