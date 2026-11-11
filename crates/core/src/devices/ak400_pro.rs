//! Display module for:
//! - AK400 DIGITAL PRO

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
        Needs { cpu_temp: true, cpu_power: true, ..Needs::default() }
    }

    fn frame(&mut self, r: &Readings, _now: Instant) -> Vec<u8> {
        let mut data = packet();
        data[1..8].copy_from_slice(&[104, 1, 2, 11, 1, 2, 5]);
        put_u16(&mut data, 8, r.cpu_power_w);
        data[10] = self.fahrenheit as u8;
        put_f32(&mut data, 11, r.cpu_temp(self.fahrenheit));
        data[15] = r.cpu_usage;
        data[16] = checksum(&data[1..=15]);
        data[17] = 22;
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
        let mut d = Display::new(&Settings { fahrenheit: true, ..Settings::default() });
        let r = Readings { cpu_usage: 50, cpu_power_w: 300, cpu_temp_milli: Some(50_000), ..Readings::default() };
        let f = d.frame(&r, Instant::now());
        assert_eq!(&f[..10], &[16, 104, 1, 2, 11, 1, 2, 5, 1, 44]);
        assert_eq!(f[10], 1);
        assert_eq!(&f[11..15], &122f32.to_be_bytes());
        assert_eq!(f[15], 50);
        assert_eq!(f[16], checksum(&f[1..=15]));
        assert_eq!(f[17], 22);
    }
}
