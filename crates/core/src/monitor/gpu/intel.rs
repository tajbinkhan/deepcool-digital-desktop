//! Reads live GPU data from the Linux kernel.

use std::fs::{read_dir, read_to_string};

pub struct Gpu {
    drm_dir: Option<String>,
    hwmon_dir: String,
    /// True when only the CPU's `coretemp` sensor was found (Xe iGPU).
    pub shared_sensor: bool,
}

fn read_u32(path: &str) -> Option<u32> {
    read_to_string(path).ok()?.trim().parse::<u32>().ok()
}

impl Gpu {
    /// Returns `None` if no usable sensor was found.
    pub fn new(pci_address: &str) -> Option<Self> {
        let pci_path = format!("/sys/bus/pci/devices/{pci_address}");
        let mut drm_dir = find_drm_dir(&pci_path);
        let (hwmon_dir, shared_sensor) = find_hwmon_dir()?;
        // With only the shared "coretemp" sensor, usage stats are not meaningful.
        if shared_sensor {
            drm_dir = None;
        }
        Some(Gpu { drm_dir, hwmon_dir, shared_sensor })
    }

    /// GPU temperature in millidegrees Celsius.
    pub fn temp_milli(&self) -> Option<u32> {
        // Standard temp1_input (common for xe, i915, and coretemp)
        if let Some(temp) = read_u32(&format!("{}/temp1_input", self.hwmon_dir)) {
            return Some(temp);
        }
        // Fallback: package temperature (B-series/other drivers)
        for idx in 2..=5 {
            let label = read_to_string(format!("{}/temp{}_label", self.hwmon_dir, idx));
            if let Ok(label) = label {
                let label = label.trim();
                if label.eq_ignore_ascii_case("pkg") || label.eq_ignore_ascii_case("package id 0") {
                    return read_u32(&format!("{}/temp{}_input", self.hwmon_dir, idx));
                }
            }
        }
        None
    }

    /// Estimates GPU usage from the current/maximum frequency ratio.
    pub fn usage(&self) -> u8 {
        let Some(drm_dir) = &self.drm_dir else {
            return 0;
        };
        let ratio = |cur: String, max: String| -> Option<u8> {
            let (cur, max) = (read_u32(&cur)?, read_u32(&max)?);
            (max > 0).then(|| ((cur as f32 / max as f32) * 100.0).round().min(100.0) as u8)
        };
        // A-series / standard i915
        if let Some(usage) = ratio(
            format!("{drm_dir}/device/gt_cur_freq_mhz"),
            format!("{drm_dir}/device/gt_max_freq_mhz"),
        ) {
            return usage;
        }
        // B-series (xe)
        let base = format!("{drm_dir}/device/tile0/gt0/freq0");
        ratio(format!("{base}/cur_freq"), format!("{base}/max_freq")).unwrap_or(0)
    }

    /// GPU power in watts.
    pub fn power_w(&self) -> u16 {
        read_to_string(format!("{}/power1_average", self.hwmon_dir))
            .or_else(|_| read_to_string(format!("{}/power/average", self.hwmon_dir)))
            .ok()
            .and_then(|d| d.trim().parse::<u64>().ok())
            .map_or(0, |p| (p / 1_000_000) as u16)
    }

    /// GPU frequency in MHz.
    pub fn frequency_mhz(&self) -> u16 {
        read_to_string(format!("{}/freq1_input", self.hwmon_dir))
            .ok()
            .and_then(|d| d.trim().parse::<u64>().ok())
            .map_or(0, |f| (f / 1_000_000) as u16)
    }
}

/// Finds DRM directory (Standard PCI-based)
fn find_drm_dir(path: &str) -> Option<String> {
    let data = read_to_string(format!("{path}/uevent")).ok()?;
    if data.lines().any(|l| l.contains("DRIVER=i915") || l.contains("DRIVER=xe")) {
        for dir in read_dir(format!("{path}/drm")).ok()? {
            let name = dir.ok()?.file_name().into_string().ok()?;
            if name.starts_with("card") {
                return Some(format!("{path}/drm/{name}"));
            }
        }
    }
    None
}

/// Finds the hwmon directory: a dedicated GPU driver first, then `coretemp`.
/// Returns the directory and whether it is the shared CPU sensor.
fn find_hwmon_dir() -> Option<(String, bool)> {
    let mut fallback_path = None;
    for entry in read_dir("/sys/class/hwmon").ok()?.flatten() {
        let path = entry.path();
        if let Ok(name) = read_to_string(path.join("name")) {
            let name = name.trim();
            if name.contains("xe") || name.contains("i915") || name.contains("intel_arc") || name.contains("drm") {
                return Some((path.to_string_lossy().to_string(), false));
            }
            if name == "coretemp" {
                fallback_path = Some(path.to_string_lossy().to_string());
            }
        }
    }
    fallback_path.map(|path| (path, true))
}
