//! Display module for:
//! - CH360 DIGITAL
//! - CH560 DIGITAL
//! - MORPHEUS

use super::{bar, check_mode, packet, put_digits, Alarm, Cycle, Driver, Mode, Settings, Status};
use crate::{
    telemetry::{Needs, Readings},
    Error,
};
use std::time::Instant;

pub const DEFAULT_MODE: Mode = Mode::CpuTemperature;

pub struct Display {
    pub mode: Mode,
    /// `Auto` means the GPU half follows the CPU half.
    pub secondary: Mode,
    cycle: Cycle,
    fahrenheit: bool,
}

impl Display {
    pub fn new(settings: &Settings) -> Result<Self, Error> {
        let mode = check_mode(settings.mode, &[Mode::Auto, Mode::CpuTemperature, Mode::CpuUsage], DEFAULT_MODE)?;
        let secondary = match settings.secondary {
            Mode::Default => match mode {
                Mode::CpuTemperature => Mode::GpuTemperature,
                Mode::CpuUsage => Mode::GpuUsage,
                _ => Mode::Auto,
            },
            Mode::GpuTemperature => Mode::GpuTemperature,
            Mode::GpuUsage => Mode::GpuUsage,
            other => return Err(Error::UnsupportedSecondary(other)),
        };
        Ok(Display {
            mode,
            secondary,
            cycle: Cycle::new(Mode::CpuTemperature, Mode::CpuUsage),
            fahrenheit: settings.fahrenheit,
        })
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs { cpu_temp: true, gpu: true, ..Needs::default() }
    }

    fn init(&self) -> Vec<Vec<u8>> {
        let mut data = packet();
        data[1] = 170;
        vec![data.to_vec()]
    }

    fn frame(&mut self, r: &Readings, now: Instant) -> Vec<u8> {
        let mode = if self.mode == Mode::Auto { self.cycle.current(now) } else { self.mode };
        let mut data = packet();
        let temp_unit = if self.fahrenheit { 35 } else { 19 };
        let gpu_usage = r.gpu.usage;

        match mode {
            Mode::CpuTemperature => {
                data[1] = temp_unit;
                put_digits(&mut data, 3, r.cpu_temp(self.fahrenheit) as u16);
                if self.secondary == Mode::Auto {
                    data[6] = temp_unit;
                    put_digits(&mut data, 8, r.gpu_temp(self.fahrenheit) as u16);
                }
            }
            Mode::CpuUsage => {
                data[1] = 76;
                put_digits(&mut data, 3, r.cpu_usage as u16);
                if self.secondary == Mode::Auto {
                    data[6] = 76;
                    put_digits(&mut data, 8, gpu_usage as u16);
                }
            }
            _ => (),
        }
        if data[6] == 0 {
            match self.secondary {
                Mode::GpuTemperature => {
                    data[6] = temp_unit;
                    put_digits(&mut data, 8, r.gpu_temp(self.fahrenheit) as u16);
                }
                Mode::GpuUsage => {
                    data[6] = 76;
                    put_digits(&mut data, 8, gpu_usage as u16);
                }
                _ => (),
            }
        }
        data[2] = bar(r.cpu_usage);
        data[7] = bar(gpu_usage);
        data.to_vec()
    }

    fn status(&self) -> Status {
        Status { mode: self.mode, secondary: Some(self.secondary), alarm: Alarm::NotSupported, notes: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::GpuSample;

    fn readings() -> Readings {
        Readings {
            cpu_usage: 30,
            cpu_temp_milli: Some(55_000),
            gpu: GpuSample { usage: 80, temp_milli: Some(66_000), ..GpuSample::default() },
            ..Readings::default()
        }
    }

    #[test]
    fn default_secondary_follows_mode() {
        let mut d = Display::new(&Settings::default()).unwrap();
        assert_eq!(d.secondary, Mode::GpuTemperature);
        assert_eq!(&d.frame(&readings(), Instant::now())[..11], &[16, 19, 3, 0, 5, 5, 19, 8, 0, 6, 6]);
    }

    #[test]
    fn explicit_secondary() {
        let settings = Settings { mode: Mode::CpuTemperature, secondary: Mode::GpuUsage, ..Settings::default() };
        let mut d = Display::new(&settings).unwrap();
        assert_eq!(&d.frame(&readings(), Instant::now())[..11], &[16, 19, 3, 0, 5, 5, 76, 8, 0, 8, 0]);
        let settings = Settings { secondary: Mode::CpuPower, ..Settings::default() };
        assert!(matches!(Display::new(&settings), Err(Error::UnsupportedSecondary(Mode::CpuPower))));
    }

    #[test]
    fn auto_mirrors_on_gpu() {
        let mut d = Display::new(&Settings { mode: Mode::Auto, ..Settings::default() }).unwrap();
        assert_eq!(d.secondary, Mode::Auto);
        let start = Instant::now();
        d.frame(&readings(), start);
        let f = d.frame(&readings(), start + std::time::Duration::from_secs(5));
        assert_eq!(&f[..11], &[16, 76, 3, 0, 3, 0, 76, 8, 0, 8, 0]);
    }
}
