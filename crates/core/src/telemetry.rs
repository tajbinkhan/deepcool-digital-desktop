//! One shared sensor reader for every running display (and later the UI).
//!
//! Readers call [`Telemetry::sample`] at their own pace. A sample taken in the
//! last few milliseconds is reused, so displays that tick together share one read.
//! Usage and power come from cumulative counters, so each display computes them
//! over its own interval with [`Readings::between`].

use crate::monitor::{
    cpu::{self, Cpu},
    gpu::{pci::PciDevice, Gpu},
};
use cpu_monitor::CpuInstant;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long a sample is reused by other readers.
const SHARE_WINDOW: Duration = Duration::from_millis(25);

/// Which sensors a reader needs. Unneeded sensors are not read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Needs {
    pub cpu_temp: bool,
    pub cpu_power: bool,
    pub cpu_freq: bool,
    pub gpu: bool,
}

impl Needs {
    pub const ALL: Needs = Needs { cpu_temp: true, cpu_power: true, cpu_freq: true, gpu: true };

    fn covered_by(&self, other: &Needs) -> bool {
        (!self.cpu_temp || other.cpu_temp)
            && (!self.cpu_power || other.cpu_power)
            && (!self.cpu_freq || other.cpu_freq)
            && (!self.gpu || other.gpu)
    }

    fn union(&self, other: &Needs) -> Needs {
        Needs {
            cpu_temp: self.cpu_temp || other.cpu_temp,
            cpu_power: self.cpu_power || other.cpu_power,
            cpu_freq: self.cpu_freq || other.cpu_freq,
            gpu: self.gpu || other.gpu,
        }
    }
}

/// Instantaneous GPU values.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuSample {
    pub temp_milli: Option<u32>,
    pub usage: u8,
    pub power_w: u16,
    pub mhz: u16,
}

/// Raw sensor values at one moment.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub at: Instant,
    needs: Needs,
    pub cpu_times: Option<CpuInstant>,
    pub energy_uj: Option<u64>,
    pub energy_max_uj: Option<u64>,
    pub cpu_temp_milli: Option<u32>,
    pub cpu_mhz: u16,
    pub gpu: GpuSample,
}

/// Values a display shows, computed between two samples.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Readings {
    pub cpu_usage: u8,
    pub cpu_power_w: u16,
    pub cpu_temp_milli: Option<u32>,
    pub cpu_mhz: u16,
    pub gpu: GpuSample,
}

/// Converts millidegrees Celsius to whole °C or °F, the way the original
/// controller did (0 when there is no sensor).
pub fn convert_temp(milli: Option<u32>, fahrenheit: bool) -> u8 {
    match milli {
        None => 0,
        Some(t) => {
            let t = if fahrenheit { t * 9 / 5 + 32000 } else { t };
            (t as f32 / 1000.0).round().min(255.0) as u8
        }
    }
}

/// CPU power in watts from two energy counter readings: `P[W] = ΔE[μJ] / (Δt[ms] * 1000)`.
pub fn power_w(before_uj: u64, after_uj: u64, max_uj: Option<u64>, elapsed: Duration) -> u16 {
    let delta = if after_uj >= before_uj {
        after_uj - before_uj
    } else if let Some(max) = max_uj {
        // A bounded counter wrapped around.
        max + after_uj - before_uj
    } else {
        // Unbounded hwmon counters only decrease when the sensor resets.
        return 0;
    };
    let millis = elapsed.as_millis().max(1) as f64;
    (delta as f64 / (millis * 1000.0)).round().min(u16::MAX as f64) as u16
}

impl Readings {
    pub fn between(before: &Sample, after: &Sample) -> Readings {
        let cpu_usage = match (before.cpu_times, after.cpu_times) {
            (Some(b), Some(a)) => {
                let usage = (a - b).non_idle() * 100.0;
                if usage.is_finite() { usage.round().clamp(0.0, 100.0) as u8 } else { 0 }
            }
            _ => 0,
        };
        let cpu_power_w = match (before.energy_uj, after.energy_uj) {
            (Some(b), Some(a)) => power_w(b, a, after.energy_max_uj, after.at.saturating_duration_since(before.at)),
            _ => 0,
        };
        Readings {
            cpu_usage,
            cpu_power_w,
            cpu_temp_milli: after.cpu_temp_milli,
            cpu_mhz: after.cpu_mhz,
            gpu: after.gpu,
        }
    }

    pub fn cpu_temp(&self, fahrenheit: bool) -> u8 {
        convert_temp(self.cpu_temp_milli, fahrenheit)
    }

    pub fn gpu_temp(&self, fahrenheit: bool) -> u8 {
        convert_temp(self.gpu.temp_milli, fahrenheit)
    }
}

/// Problems with the sensors, shown to the user as warnings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SensorStatus {
    pub cpu_temp: bool,
    pub cpu_power: bool,
    /// `None` when the GPU works normally.
    pub gpu_problem: Option<String>,
    pub gpu_available: bool,
}

struct Inner {
    cpu: Cpu,
    gpu: Gpu,
    last: Option<Sample>,
}

pub struct Telemetry {
    inner: Mutex<Inner>,
}

impl Telemetry {
    /// Finds the CPU sensors and opens the given GPU.
    pub fn new(gpu: Option<&PciDevice>) -> Self {
        Telemetry {
            inner: Mutex::new(Inner { cpu: Cpu::new(), gpu: Gpu::new(gpu), last: None }),
        }
    }

    /// Switches the monitored GPU.
    pub fn set_gpu(&self, gpu: Option<&PciDevice>) {
        let gpu = Gpu::new(gpu);
        let mut inner = self.lock();
        inner.gpu = gpu;
        inner.last = None;
    }

    /// Looks for the CPU temperature and energy sensors again, e.g. after
    /// permission to read the energy counter was granted.
    pub fn reload_cpu_sensors(&self) {
        let cpu = Cpu::new();
        let mut inner = self.lock();
        inner.cpu = cpu;
        inner.last = None;
    }

    pub fn status(&self) -> SensorStatus {
        let inner = self.lock();
        SensorStatus {
            cpu_temp: inner.cpu.has_temp(),
            cpu_power: inner.cpu.has_energy(),
            gpu_problem: inner.gpu.problem(),
            gpu_available: inner.gpu.is_available(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A panic while sampling leaves the data valid; keep going.
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Reads the sensors in `needs`, or reuses a sample taken moments ago.
    pub fn sample(&self, needs: Needs) -> Sample {
        let mut inner = self.lock();
        let now = Instant::now();
        if let Some(last) = inner.last {
            if now.saturating_duration_since(last.at) < SHARE_WINDOW && needs.covered_by(&last.needs) {
                return last;
            }
        }
        // Read what this caller and the previous caller needed, so readers
        // with different needs that tick together still share one read.
        let needs = match inner.last {
            Some(last) if now.saturating_duration_since(last.at) < SHARE_WINDOW => needs.union(&last.needs),
            _ => needs,
        };
        let sample = Sample {
            at: now,
            needs,
            cpu_times: cpu::instant(),
            energy_uj: if needs.cpu_power { inner.cpu.energy_uj() } else { None },
            energy_max_uj: inner.cpu.energy_max_uj(),
            cpu_temp_milli: if needs.cpu_temp { inner.cpu.temp_milli() } else { None },
            cpu_mhz: if needs.cpu_freq { cpu::frequency_mhz() } else { 0 },
            gpu: if needs.gpu {
                GpuSample {
                    temp_milli: inner.gpu.temp_milli(),
                    usage: inner.gpu.usage(),
                    power_w: inner.gpu.power_w(),
                    mhz: inner.gpu.frequency_mhz(),
                }
            } else {
                GpuSample::default()
            },
        };
        inner.last = Some(sample);
        sample
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_conversion_matches_original() {
        assert_eq!(convert_temp(None, false), 0);
        assert_eq!(convert_temp(None, true), 0);
        assert_eq!(convert_temp(Some(42_499), false), 42);
        assert_eq!(convert_temp(Some(42_500), false), 43);
        // 42.0 °C = 107.6 °F
        assert_eq!(convert_temp(Some(42_000), true), 108);
        // NVIDIA reports whole degrees; the original rounded 1.8c + 32.
        for c in 0..=120u32 {
            assert_eq!(convert_temp(Some(c * 1000), true), (c as f32 * 1.8 + 32.0).round() as u8);
        }
    }

    #[test]
    fn power_handles_wrap_and_reset() {
        let second = Duration::from_secs(1);
        assert_eq!(power_w(1_000_000, 26_000_000, None, second), 25);
        assert_eq!(power_w(5, 5, None, second), 0);
        // Bounded counter wraps.
        assert_eq!(power_w(90_000_000, 10_000_000, Some(100_000_000), second), 20);
        // Unbounded counter reset.
        assert_eq!(power_w(90_000_000, 10_000_000, None, second), 0);
        // Half-second interval.
        assert_eq!(power_w(0, 10_000_000, None, Duration::from_millis(500)), 20);
    }

    #[test]
    fn needs_cover_and_union() {
        let temp = Needs { cpu_temp: true, ..Needs::default() };
        let gpu = Needs { gpu: true, ..Needs::default() };
        assert!(temp.covered_by(&Needs::ALL));
        assert!(!gpu.covered_by(&temp));
        assert!(gpu.covered_by(&temp.union(&gpu)));
    }

    #[test]
    fn real_sampling_does_not_panic() {
        let telemetry = Telemetry::new(None);
        let a = telemetry.sample(Needs::ALL);
        std::thread::sleep(Duration::from_millis(60));
        let b = telemetry.sample(Needs::ALL);
        let readings = Readings::between(&a, &b);
        assert!(readings.cpu_usage <= 100);
        // Reused within the share window.
        let c = telemetry.sample(Needs::default());
        assert_eq!(c.at, b.at);
    }
}
