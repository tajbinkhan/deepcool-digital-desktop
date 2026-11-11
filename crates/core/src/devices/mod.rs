//! Packet encoders for each DeepCool display family.
//!
//! A [`Driver`] only turns [`Readings`] into packets. Opening the device, timing
//! and stopping are handled once for all families in [`crate::runner`].

pub mod ag_series;
pub mod ak400_pro;
pub mod ak620_pro;
pub mod ak_series;
pub mod ch510;
pub mod ch_series;
pub mod ch_series_gen2;
pub mod ld_series;
pub mod lp_series;
pub mod lq_series;
pub mod ls_series;

use crate::{
    telemetry::{Needs, Readings},
    Error,
};
use std::time::{Duration, Instant};

pub const DEFAULT_VENDOR_ID: u16 = 13875;
pub const CH510_VENDOR_ID: u16 = 13523;
pub const CH510_PRODUCT_ID: u16 = 4352;

pub const AUTO_MODE_INTERVAL: Duration = Duration::from_millis(5000);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Default,
    Auto,
    CpuTemperature,
    CpuUsage,
    CpuPower,
    CpuFrequency,
    CpuFan,
    GpuTemperature,
    GpuUsage,
    GpuPower,
    Cpu,
    Gpu,
    Psu,
}

impl Mode {
    pub const fn symbol(&self) -> &'static str {
        match self {
            Mode::Default => "",
            Mode::Auto => "auto",
            Mode::CpuTemperature => "cpu_temp",
            Mode::CpuUsage => "cpu_usage",
            Mode::CpuPower => "cpu_power",
            Mode::CpuFrequency => "cpu_freq",
            Mode::CpuFan => "cpu_fan",
            Mode::GpuTemperature => "gpu_temp",
            Mode::GpuUsage => "gpu_usage",
            Mode::GpuPower => "gpu_power",
            Mode::Cpu => "cpu",
            Mode::Gpu => "gpu",
            Mode::Psu => "psu",
        }
    }

    pub fn get(symbol: &str) -> Option<Mode> {
        match symbol {
            "auto" => Some(Self::Auto),
            "cpu_temp" => Some(Self::CpuTemperature),
            "cpu_usage" => Some(Self::CpuUsage),
            "cpu_power" => Some(Self::CpuPower),
            "cpu_freq" => Some(Self::CpuFrequency),
            "cpu_fan" => Some(Self::CpuFan),
            "gpu_temp" => Some(Self::GpuTemperature),
            "gpu_usage" => Some(Self::GpuUsage),
            "gpu_power" => Some(Self::GpuPower),
            "cpu" => Some(Self::Cpu),
            "gpu" => Some(Self::Gpu),
            "psu" => Some(Self::Psu),
            _ => None,
        }
    }
}

/// User settings for one display.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub mode: Mode,
    pub secondary: Mode,
    pub update: Duration,
    pub fahrenheit: bool,
    pub alarm: bool,
    pub rotate: u16,
    pub lead_zeros: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: Mode::Default,
            secondary: Mode::Default,
            update: Duration::from_millis(1000),
            fahrenheit: false,
            alarm: false,
            rotate: 0,
            lead_zeros: false,
        }
    }
}

/// How the device's temperature alarm behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alarm {
    /// Built into the device. `warning` is 0 when the device has no warning level.
    Auto { limit: u8, warning: u8 },
    On { limit: u8 },
    Off,
    NotSupported,
}

/// What a driver will actually show, after defaults are applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub mode: Mode,
    pub secondary: Option<Mode>,
    pub alarm: Alarm,
    /// Notes about limited support, shown as warnings.
    pub notes: Vec<&'static str>,
}

pub trait Driver: Send {
    /// Sensors the frames use.
    fn needs(&self) -> Needs;

    /// Called once with the USB product string after the device is opened.
    fn on_open(&mut self, _product: &str) {}

    /// Packets written once after opening.
    fn init(&self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    /// Encodes one update.
    fn frame(&mut self, readings: &Readings, now: Instant) -> Vec<u8>;

    fn status(&self) -> Status;
}

/// Alternates between two modes every [`AUTO_MODE_INTERVAL`].
#[derive(Debug, Clone)]
pub(crate) struct Cycle {
    modes: [Mode; 2],
    index: usize,
    switch_at: Option<Instant>,
}

impl Cycle {
    pub(crate) fn new(first: Mode, second: Mode) -> Self {
        Cycle { modes: [first, second], index: 0, switch_at: None }
    }

    /// The mode to show at `now`.
    pub(crate) fn current(&mut self, now: Instant) -> Mode {
        let switch_at = *self.switch_at.get_or_insert(now + AUTO_MODE_INTERVAL);
        if now >= switch_at {
            self.index ^= 1;
            // After a long pause (e.g. suspend), restart the interval from now.
            let next = switch_at + AUTO_MODE_INTERVAL;
            self.switch_at = Some(if next > now { next } else { now + AUTO_MODE_INTERVAL });
        }
        self.modes[self.index]
    }
}

/// An empty 64-byte report starting with report ID 16.
pub(crate) fn packet() -> [u8; 64] {
    let mut data = [0; 64];
    data[0] = 16;
    data
}

/// Sum of `data[range]` modulo 256.
pub(crate) fn checksum(data: &[u8]) -> u8 {
    (data.iter().map(|&x| x as u16).sum::<u16>() % 256) as u8
}

/// Writes a whole number as a big-endian f32 (used for temperatures).
pub(crate) fn put_f32(data: &mut [u8], at: usize, value: u8) {
    data[at..at + 4].copy_from_slice(&(value as f32).to_be_bytes());
}

pub(crate) fn put_u16(data: &mut [u8], at: usize, value: u16) {
    data[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

/// Writes a 3-digit number as one digit per byte.
pub(crate) fn put_digits(data: &mut [u8], at: usize, value: u16) {
    data[at] = (value / 100) as u8;
    data[at + 1] = (value % 100 / 10) as u8;
    data[at + 2] = (value % 10) as u8;
}

/// Status bar value (1–10) for a 0–100 usage.
pub(crate) fn bar(usage: u8) -> u8 {
    if usage < 15 { 1 } else { (usage as f32 / 10.0).round() as u8 }
}

pub(crate) fn check_mode(mode: Mode, allowed: &[Mode], default: Mode) -> Result<Mode, Error> {
    match mode {
        Mode::Default => Ok(default),
        mode if allowed.contains(&mode) => Ok(mode),
        mode => Err(Error::UnsupportedMode(mode)),
    }
}

/// Creates the driver for a product ID, validating the settings.
pub fn driver_for(pid: u16, settings: &Settings) -> Result<Box<dyn Driver>, Error> {
    Ok(match pid {
        1..=4 => Box::new(ak_series::Display::new(settings)?),
        5 | 7 | 21 => Box::new(ch_series::Display::new(settings)?),
        6 => Box::new(ls_series::Display::new(settings)?),
        8 => Box::new(ag_series::Display::new(settings)?),
        10 => Box::new(ld_series::Display::new(settings)),
        12 => Box::new(lp_series::Display::new(settings)?),
        13 | 15 | 31 | 41 | 42 | 43 | 44 => Box::new(lq_series::Display::new(settings)),
        16 => Box::new(ak400_pro::Display::new(settings)),
        17 | 18 => Box::new(ak620_pro::Display::new(settings)),
        19 | 22 | 27 => Box::new(ch_series_gen2::Display::new(settings)?),
        CH510_PRODUCT_ID => Box::new(ch510::Display::new(settings)?),
        pid => return Err(Error::UnsupportedDevice(pid)),
    })
}

/// The USB vendor ID of a product ID.
pub fn vendor_for(pid: u16) -> u16 {
    if pid == CH510_PRODUCT_ID { CH510_VENDOR_ID } else { DEFAULT_VENDOR_ID }
}

/// A connected DeepCool device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub vid: u16,
    pub pid: u16,
    pub name: String,
}

/// Lists connected DeepCool devices, one entry per product ID.
pub fn find_devices(api: &hidapi::HidApi) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    for device in api.device_list() {
        let (vid, pid) = (device.vendor_id(), device.product_id());
        let name = if vid == DEFAULT_VENDOR_ID {
            device.product_string().unwrap_or("DeepCool device").to_owned()
        } else if vid == CH510_VENDOR_ID && pid == CH510_PRODUCT_ID {
            "CH510-MESH-DIGITAL".to_owned()
        } else {
            continue;
        };
        if !found.iter().any(|f| f.pid == pid) {
            found.push(Found { vid, pid, name });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_switches_every_interval() {
        let start = Instant::now();
        let mut cycle = Cycle::new(Mode::CpuTemperature, Mode::CpuUsage);
        assert_eq!(cycle.current(start), Mode::CpuTemperature);
        assert_eq!(cycle.current(start + Duration::from_millis(4999)), Mode::CpuTemperature);
        assert_eq!(cycle.current(start + AUTO_MODE_INTERVAL), Mode::CpuUsage);
        assert_eq!(cycle.current(start + Duration::from_millis(9999)), Mode::CpuUsage);
        assert_eq!(cycle.current(start + AUTO_MODE_INTERVAL * 2), Mode::CpuTemperature);
        // After a long pause the next switch is a full interval later.
        let late = start + Duration::from_secs(3600);
        assert_eq!(cycle.current(late), Mode::CpuUsage);
        assert_eq!(cycle.current(late + Duration::from_millis(4999)), Mode::CpuUsage);
    }

    #[test]
    fn every_supported_pid_has_a_driver() {
        for pid in [1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 13, 15, 16, 17, 18, 19, 21, 22, 27, 31, 41, 42, 43, 44, 4352] {
            assert!(driver_for(pid, &Settings::default()).is_ok(), "pid {pid}");
        }
        assert!(matches!(driver_for(9, &Settings::default()), Err(Error::UnsupportedDevice(9))));
    }

    #[test]
    fn rejects_unsupported_modes() {
        let settings = Settings { mode: Mode::GpuPower, ..Settings::default() };
        assert!(matches!(driver_for(1, &settings), Err(Error::UnsupportedMode(Mode::GpuPower))));
        let settings = Settings { secondary: Mode::CpuFan, ..Settings::default() };
        assert!(matches!(driver_for(12, &settings), Err(Error::UnsupportedSecondary(Mode::CpuFan))));
    }

    #[test]
    fn helpers() {
        assert_eq!(bar(0), 1);
        assert_eq!(bar(14), 1);
        assert_eq!(bar(15), 2);
        assert_eq!(bar(100), 10);
        let mut data = [0u8; 8];
        put_digits(&mut data, 0, 123);
        assert_eq!(&data[..3], &[1, 2, 3]);
        assert_eq!(checksum(&[200, 100]), 44);
    }
}
