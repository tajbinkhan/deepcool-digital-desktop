//! Display module for:
//! - LS520 SE DIGITAL
//! - LS720 SE DIGITAL

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
}

impl Display {
    pub fn new(settings: &Settings) -> Result<Self, Error> {
        let mode = check_mode(settings.mode, &[Mode::Auto, Mode::CpuTemperature, Mode::CpuPower], DEFAULT_MODE)?;
        Ok(Display {
            mode,
            cycle: Cycle::new(Mode::CpuTemperature, Mode::CpuPower),
            fahrenheit: settings.fahrenheit,
            alarm: settings.alarm,
        })
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs { cpu_temp: true, cpu_power: self.mode != Mode::CpuTemperature, ..Needs::default() }
    }

    fn init(&self) -> Vec<Vec<u8>> {
        let mut data = packet();
        data[1] = 170;
        vec![data.to_vec()]
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
            Mode::CpuPower => {
                data[1] = 76;
                put_digits(&mut data, 3, r.cpu_power_w);
            }
            _ => (),
        }
        data[2] = bar(r.cpu_usage);
        data[6] = (self.alarm && temp >= if self.fahrenheit { TEMP_LIMIT_F } else { TEMP_LIMIT_C }) as u8;
        data.to_vec()
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

    #[test]
    fn power_mode() {
        let mut d = Display::new(&Settings { mode: Mode::CpuPower, ..Settings::default() }).unwrap();
        let r = Readings { cpu_usage: 55, cpu_power_w: 142, cpu_temp_milli: Some(60_000), ..Readings::default() };
        assert_eq!(&d.frame(&r, Instant::now())[..7], &[16, 76, 6, 1, 4, 2, 0]);
        assert_eq!(&d.init()[0][..2], &[16, 170]);
    }
}
