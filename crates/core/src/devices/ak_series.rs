//! Display module for:
//! - AK400 DIGITAL & DIGITAL SE
//! - AK500 DIGITAL
//! - AK500S DIGITAL & DIGITAL SE
//! - AK620 DIGITAL & DIGITAL SE

use super::{bar, check_mode, packet, put_digits, Alarm, Cycle, Driver, Mode, Settings, Status};
use crate::{
    telemetry::{Needs, Readings},
    Error,
};
use std::time::Instant;

pub const DEFAULT_MODE: Mode = Mode::CpuTemperature;
pub const TEMP_LIMIT_C: u8 = 90;
pub const TEMP_LIMIT_F: u8 = 194;

pub struct Display {
    pub mode: Mode,
    cycle: Cycle,
    fahrenheit: bool,
    alarm: bool,
    se_mode: bool,
}

impl Display {
    pub fn new(settings: &Settings) -> Result<Self, Error> {
        let mode = check_mode(settings.mode, &[Mode::Auto, Mode::CpuTemperature, Mode::CpuUsage], DEFAULT_MODE)?;
        Ok(Display {
            mode,
            cycle: Cycle::new(Mode::CpuTemperature, Mode::CpuUsage),
            fahrenheit: settings.fahrenheit,
            alarm: settings.alarm,
            se_mode: false,
        })
    }

    /// On "DIGITAL SE" devices, the report ID is not sent, so every byte moves back by one.
    fn finish(&self, mut data: [u8; 64]) -> Vec<u8> {
        if self.se_mode {
            data.copy_within(1.., 0);
        }
        data.to_vec()
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs { cpu_temp: true, ..Needs::default() }
    }

    /// Identifies the device type from the product name: "AK400" is AK400 DIGITAL,
    /// "A400" is AK400 DIGITAL SE.
    fn on_open(&mut self, product: &str) {
        self.se_mode = !product.starts_with("AK");
    }

    fn init(&self) -> Vec<Vec<u8>> {
        let mut data = packet();
        data[1] = 170;
        vec![self.finish(data)]
    }

    fn frame(&mut self, r: &Readings, now: Instant) -> Vec<u8> {
        let mode = if self.mode == Mode::Auto { self.cycle.current(now) } else { self.mode };
        let mut data = packet();
        let temp = r.cpu_temp(self.fahrenheit);
        match mode {
            Mode::CpuTemperature => {
                data[1] = if self.fahrenheit { 35 } else { 19 };
                put_digits(&mut data, 3, temp as u16);
            }
            Mode::CpuUsage => {
                data[1] = 76;
                put_digits(&mut data, 3, r.cpu_usage as u16);
            }
            _ => (),
        }
        data[2] = bar(r.cpu_usage);
        data[6] = (self.alarm && temp >= if self.fahrenheit { TEMP_LIMIT_F } else { TEMP_LIMIT_C }) as u8;
        self.finish(data)
    }

    fn status(&self) -> Status {
        let limit = if self.fahrenheit { TEMP_LIMIT_F } else { TEMP_LIMIT_C };
        Status {
            mode: self.mode,
            secondary: None,
            alarm: if self.alarm { Alarm::On { limit } } else { Alarm::Off },
            notes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn readings(usage: u8, temp: u32) -> Readings {
        Readings { cpu_usage: usage, cpu_temp_milli: Some(temp * 1000), ..Readings::default() }
    }

    #[test]
    fn temperature_usage_and_alarm() {
        let mut d = Display::new(&Settings { alarm: true, ..Settings::default() }).unwrap();
        d.on_open("AK620-DIGITAL");
        assert_eq!(&d.init()[0][..2], &[16, 170]);
        let f = d.frame(&readings(42, 91), Instant::now());
        assert_eq!(&f[..7], &[16, 19, 4, 0, 9, 1, 1]);

        let mut d = Display::new(&Settings { mode: Mode::CpuUsage, fahrenheit: true, ..Settings::default() }).unwrap();
        let f = d.frame(&readings(100, 30), Instant::now());
        assert_eq!(&f[..7], &[16, 76, 10, 1, 0, 0, 0]);
    }

    #[test]
    fn se_devices_drop_the_report_id() {
        let mut d = Display::new(&Settings::default()).unwrap();
        d.on_open("A400");
        assert_eq!(&d.init()[0][..2], &[170, 0]);
        let f = d.frame(&readings(10, 45), Instant::now());
        assert_eq!(&f[..6], &[19, 1, 0, 4, 5, 0]);
    }

    #[test]
    fn auto_alternates() {
        let mut d = Display::new(&Settings { mode: Mode::Auto, ..Settings::default() }).unwrap();
        let start = Instant::now();
        assert_eq!(d.frame(&readings(10, 45), start)[1], 19);
        assert_eq!(d.frame(&readings(10, 45), start + Duration::from_secs(5))[1], 76);
    }
}
