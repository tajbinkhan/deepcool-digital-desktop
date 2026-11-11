//! Display module for:
//! - AK400 G2 DIGITAL NYX
//! - AK500 G2 DIGITAL NYX
//! - AK620 G2 DIGITAL NYX
//! - AK700 DIGITAL NYX
//! - ASSASSIN IV VC VISION
//! - LQ240
//! - LQ360

use super::{checksum, packet, put_f32, put_u16, Alarm, Driver, Mode, Settings, Status};
use crate::telemetry::{Needs, Readings};
use std::time::Instant;

pub const DEFAULT_MODE: Mode = Mode::Auto;
// The temperature limits are hard-coded in the device
pub const TEMP_WARNING_C: u8 = 80;
pub const TEMP_WARNING_F: u8 = 176;
pub const TEMP_LIMIT_C: u8 = 90;
pub const TEMP_LIMIT_F: u8 = 194;

pub struct Display {
    fahrenheit: bool,
}

impl Display {
    pub fn new(settings: &Settings) -> Self {
        Display { fahrenheit: settings.fahrenheit }
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs { cpu_temp: true, cpu_power: true, cpu_freq: true, gpu: false }
    }

    fn frame(&mut self, r: &Readings, _now: Instant) -> Vec<u8> {
        let mut data = packet();
        data[1..7].copy_from_slice(&[104, 1, 8, 12, 1, 2]);
        put_u16(&mut data, 7, r.cpu_power_w);
        data[9] = self.fahrenheit as u8;
        put_f32(&mut data, 10, r.cpu_temp(self.fahrenheit));
        data[14] = r.cpu_usage;
        put_u16(&mut data, 15, r.cpu_mhz);
        data[17] = checksum(&data[1..=16]);
        data[18] = 22;
        data.to_vec()
    }

    fn status(&self) -> Status {
        Status {
            mode: DEFAULT_MODE,
            secondary: None,
            alarm: if self.fahrenheit {
                Alarm::Auto { limit: TEMP_LIMIT_F, warning: TEMP_WARNING_F }
            } else {
                Alarm::Auto { limit: TEMP_LIMIT_C, warning: TEMP_WARNING_C }
            },
            notes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout() {
        let mut d = Display::new(&Settings::default());
        let r = Readings { cpu_usage: 9, cpu_power_w: 300, cpu_temp_milli: Some(61_000), cpu_mhz: 4000, ..Readings::default() };
        let f = d.frame(&r, Instant::now());
        assert_eq!(&f[..10], &[16, 104, 1, 8, 12, 1, 2, 1, 44, 0]);
        assert_eq!(&f[10..14], &61f32.to_be_bytes());
        assert_eq!(&f[14..17], &[9, 15, 160]);
        assert_eq!(f[17], checksum(&f[1..=16]));
        assert_eq!(f[18], 22);
    }
}
