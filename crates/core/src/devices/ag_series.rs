//! Display module for:
//! - AG300 DIGITAL
//! - AG400 DIGITAL
//! - AG500 DIGITAL
//! - AG620 DIGITAL

use super::{check_mode, packet, Alarm, Cycle, Driver, Mode, Settings, Status};
use crate::{
    telemetry::{Needs, Readings},
    Error,
};
use std::time::Instant;

pub const DEFAULT_MODE: Mode = Mode::CpuTemperature;
pub const TEMP_LIMIT_C: u8 = 90;

pub struct Display {
    pub mode: Mode,
    cycle: Cycle,
    alarm: bool,
}

impl Display {
    pub fn new(settings: &Settings) -> Result<Self, Error> {
        let mode = check_mode(settings.mode, &[Mode::Auto, Mode::CpuTemperature, Mode::CpuUsage], DEFAULT_MODE)?;
        Ok(Display { mode, cycle: Cycle::new(Mode::CpuTemperature, Mode::CpuUsage), alarm: settings.alarm })
    }
}

/// Two digits, showing 99 for values of 100 or more.
fn two_digits(data: &mut [u8], value: u8) {
    data[3] = if value < 100 { value / 10 } else { 9 };
    data[4] = if value < 100 { value % 10 } else { 9 };
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs { cpu_temp: true, ..Needs::default() }
    }

    fn frame(&mut self, r: &Readings, now: Instant) -> Vec<u8> {
        let mode = if self.mode == Mode::Auto { self.cycle.current(now) } else { self.mode };
        let mut data = packet();
        // The AG Series only shows °C.
        let temp = r.cpu_temp(false);
        match mode {
            Mode::CpuUsage => {
                data[1] = 76;
                two_digits(&mut data, r.cpu_usage);
            }
            Mode::CpuTemperature => {
                data[1] = 19;
                two_digits(&mut data, temp);
            }
            _ => (),
        }
        data[5] = (self.alarm && temp >= TEMP_LIMIT_C) as u8;
        data.to_vec()
    }

    fn status(&self) -> Status {
        Status {
            mode: self.mode,
            secondary: None,
            alarm: if self.alarm { Alarm::On { limit: TEMP_LIMIT_C } } else { Alarm::Off },
            notes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_at_99() {
        let mut d = Display::new(&Settings { alarm: true, ..Settings::default() }).unwrap();
        let hot = Readings { cpu_temp_milli: Some(101_000), ..Readings::default() };
        assert_eq!(&d.frame(&hot, Instant::now())[..6], &[16, 19, 0, 9, 9, 1]);
        let mut d = Display::new(&Settings { mode: Mode::CpuUsage, ..Settings::default() }).unwrap();
        let busy = Readings { cpu_usage: 37, cpu_temp_milli: Some(50_000), ..Readings::default() };
        assert_eq!(&d.frame(&busy, Instant::now())[..6], &[16, 76, 0, 3, 7, 0]);
    }
}
