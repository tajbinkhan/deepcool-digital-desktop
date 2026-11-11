//! Reads live GPU data from the Linux kernel. Supports both GPUs and iGPUs (APU).

use std::fs::{read_dir, read_to_string};

pub struct Gpu {
    device_dir: String,
    usage_file: String,
    hwmon_dir: String,
}

fn read_u64(path: &str) -> Option<u64> {
    read_to_string(path).ok()?.trim_end().parse::<u64>().ok()
}

impl Gpu {
    /// Returns `None` if the device is not an amdgpu card or has no hwmon sensor.
    pub fn new(pci_address: &str) -> Option<Self> {
        let path = format!("/sys/bus/pci/devices/{pci_address}");
        Some(Gpu {
            usage_file: find_card(&path)?,
            hwmon_dir: find_hwmon_dir(&path)?,
            device_dir: path,
        })
    }

    /// GPU temperature in millidegrees Celsius.
    pub fn temp_milli(&self) -> Option<u32> {
        read_u64(&format!("{}/temp1_input", self.hwmon_dir)).map(|t| t as u32)
    }

    /// GPU usage in percent.
    pub fn usage(&self) -> u8 {
        read_u64(&self.usage_file).unwrap_or(0).min(100) as u8
    }

    /// GPU power consumption in watts.
    pub fn power_w(&self) -> u16 {
        (read_u64(&format!("{}/power1_average", self.hwmon_dir)).unwrap_or(0) / 1_000_000) as u16
    }

    /// Used and total video memory in bytes.
    pub fn memory(&self) -> Option<(u64, u64)> {
        let used = read_u64(&format!("{}/mem_info_vram_used", self.device_dir))?;
        let total = read_u64(&format!("{}/mem_info_vram_total", self.device_dir))?;
        Some((used, total))
    }

    /// GPU core frequency in MHz.
    pub fn frequency_mhz(&self) -> u16 {
        (read_u64(&format!("{}/freq1_input", self.hwmon_dir)).unwrap_or(0) / 1_000_000) as u16
    }
}

/// Confirms that the specified path belongs to an AMD GPU and returns the path of the "GPU Usage" file.
fn find_card(path: &str) -> Option<String> {
    let data = read_to_string(format!("{path}/uevent")).ok()?;
    if data.lines().next()?.ends_with("amdgpu") {
        return Some(format!("{path}/gpu_busy_percent"));
    }
    None
}

/// Looks for the hwmon directory of the specified AMD GPU.
fn find_hwmon_dir(path: &str) -> Option<String> {
    let hwmon_path = read_dir(format!("{path}/hwmon")).ok()?.next()?.ok()?.path();
    let name = read_to_string(hwmon_path.join("name")).ok()?;
    if name.starts_with("amdgpu") {
        return Some(hwmon_path.to_str()?.to_owned());
    }
    None
}
