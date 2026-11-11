//! Display module for:
//! - AK500 DIGITAL PRO
//! - AK620 DIGITAL PRO

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
        data[1..8].copy_from_slice(&[104, 1, 4, 13, 1, 2, 8]);
        put_u16(&mut data, 8, r.cpu_power_w);
        data[10] = self.fahrenheit as u8;
        put_f32(&mut data, 11, r.cpu_temp(self.fahrenheit));
        data[15] = r.cpu_usage;
        put_u16(&mut data, 16, r.cpu_mhz);
        data[18] = checksum(&data[1..=17]);
        data[19] = 22;
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
        let r = Readings { cpu_usage: 7, cpu_power_w: 20, cpu_temp_milli: Some(40_000), cpu_mhz: 5016, ..Readings::default() };
        let f = d.frame(&r, Instant::now());
        assert_eq!(&f[..11], &[16, 104, 1, 4, 13, 1, 2, 8, 0, 20, 0]);
        assert_eq!(&f[11..15], &40f32.to_be_bytes());
        assert_eq!(&f[15..18], &[7, (5016u16 >> 8) as u8, (5016u16 & 255) as u8]);
        assert_eq!(f[18], checksum(&f[1..=17]));
        assert_eq!(f[19], 22);
    }
}
