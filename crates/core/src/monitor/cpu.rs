//! Reads live CPU data from the Linux kernel.
//!
//! Every reader returns `None` (or 0) when a value is unavailable instead of
//! ending the process, so a long-running app can keep going.

use cpu_monitor::CpuInstant;
use std::{
    fs::{read_dir, read_to_string},
    path::{Path, PathBuf},
};

struct EnergySensor {
    path: PathBuf,
    max_uj: Option<u64>,
}

pub struct Cpu {
    temp_sensor: Option<PathBuf>,
    energy_sensor: Option<EnergySensor>,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            temp_sensor: find_temp_sensor(),
            energy_sensor: find_energy_sensor(),
        }
    }

    pub fn has_temp(&self) -> bool {
        self.temp_sensor.is_some()
    }

    pub fn has_energy(&self) -> bool {
        self.energy_sensor.is_some()
    }

    /// Reads the CPU temperature in millidegrees Celsius.
    pub fn temp_milli(&self) -> Option<u32> {
        let sensor = self.temp_sensor.as_ref()?;
        read_to_string(sensor).ok()?.trim_end().parse::<u32>().ok()
    }

    /// Reads the package energy counter in microjoules.
    pub fn energy_uj(&self) -> Option<u64> {
        let sensor = self.energy_sensor.as_ref()?;
        read_to_string(&sensor.path).ok()?.trim_end().parse::<u64>().ok()
    }

    /// The value at which the energy counter wraps, if it is bounded.
    pub fn energy_max_uj(&self) -> Option<u64> {
        self.energy_sensor.as_ref()?.max_uj
    }
}

/// Reads the cumulative CPU time counters.
pub fn instant() -> Option<CpuInstant> {
    CpuInstant::now().ok()
}

/// Reads the frequency of all CPU cores and returns the highest one in MHz.
pub fn frequency_mhz() -> u16 {
    let Ok(cpuinfo) = read_to_string("/proc/cpuinfo") else {
        return 0;
    };
    let highest = cpuinfo
        .lines()
        .filter(|line| line.starts_with("cpu MHz"))
        .filter_map(|line| line.split(':').nth(1)?.trim().parse::<f32>().ok())
        .fold(0.0_f32, f32::max);
    highest.round() as u16
}

/// Looks for the appropriate CPU temperature sensor in the hwmon directory.
fn find_temp_sensor() -> Option<PathBuf> {
    for entry in read_dir("/sys/class/hwmon").ok()?.flatten() {
        let path = entry.path();
        if let Ok(name) = read_to_string(path.join("name")) {
            if ["asusec", "coretemp", "k10temp", "zenpower"].contains(&name.trim_end()) {
                return Some(path.join("temp1_input"));
            }
        }
    }
    None
}

/// Finds an Intel RAPL or AMD zenergy/amd_energy package energy counter.
fn find_energy_sensor() -> Option<EnergySensor> {
    let intel_rapl = Path::new("/sys/class/powercap/intel-rapl/intel-rapl:0");
    let intel_path = intel_rapl.join("energy_uj");
    if is_readable_counter(&intel_path) {
        let max_uj = read_to_string(intel_rapl.join("max_energy_range_uj"))
            .ok()
            .and_then(|data| data.trim_end().parse::<u64>().ok());
        return Some(EnergySensor { path: intel_path, max_uj });
    }

    for entry in read_dir("/sys/class/hwmon").ok()?.flatten() {
        let path = entry.path();
        let Ok(name) = read_to_string(path.join("name")) else {
            continue;
        };
        if !["zenergy", "amd_energy"].contains(&name.trim_end()) {
            continue;
        }
        if let Some(path) = find_socket_energy(&path) {
            return Some(EnergySensor { path, max_uj: None });
        }
    }
    None
}

/// Checks that an energy counter exists and can be read by the current user.
/// Counters such as `intel-rapl:0/energy_uj` are often readable only by root.
fn is_readable_counter(path: &Path) -> bool {
    read_to_string(path).is_ok_and(|data| data.trim_end().parse::<u64>().is_ok())
}

/// Finds the package/socket energy channel exposed by an AMD hwmon device.
fn find_socket_energy(hwmon: &Path) -> Option<PathBuf> {
    for entry in read_dir(hwmon).ok()?.flatten() {
        let label_path = entry.path();
        let Some(file_name) = label_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(channel) = file_name.strip_prefix("energy").and_then(|name| name.strip_suffix("_label")) else {
            continue;
        };
        let Ok(label) = read_to_string(&label_path) else {
            continue;
        };
        if label.trim_end().starts_with("Esocket") {
            let input_path = hwmon.join(format!("energy{channel}_input"));
            if is_readable_counter(&input_path) {
                return Some(input_path);
            }
        }
    }
    None
}

/// Gets the CPU model name.
pub fn get_name() -> Option<String> {
    let cpuinfo = read_to_string("/proc/cpuinfo").ok()?;
    cpuinfo
        .lines()
        .find(|line| line.starts_with("model name"))
        .and_then(|line| line.split_once(':'))
        .map(|(_, name)| name.trim().to_string())
}
