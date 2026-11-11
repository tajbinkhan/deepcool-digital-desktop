//! Display module for:
//! - LD240
//! - LD360

use super::{checksum, packet, put_f32, put_u16, Alarm, Driver, Mode, Settings, Status};
use crate::telemetry::{Needs, Readings};
use std::time::Instant;

pub const DEFAULT_MODE: Mode = Mode::Auto;
// The temperature limits are hard-coded in the device
pub const TEMP_LIMIT_C: u8 = 85;
pub const TEMP_LIMIT_F: u8 = 185;

pub struct Display {
    fahrenheit: bool,
    lead_zeros: bool,
}

fn header() -> [u8; 64] {
    let mut data = packet();
    data[1] = 104;
    data[2] = 1;
    data[3] = 1;
    data
}

impl Display {
    pub fn new(settings: &Settings) -> Self {
        Display { fahrenheit: settings.fahrenheit, lead_zeros: settings.lead_zeros }
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs { cpu_temp: true, cpu_power: true, ..Needs::default() }
    }

    fn init(&self) -> Vec<Vec<u8>> {
        let mut init = header();
        init[4] = 2;
        init[5] = 3;
        init[6] = 1;
        init[7] = 112;
        init[8] = 22;
        let first = init.to_vec();
        // Display format (show/hide leading zeros)
        init[5] = 2;
        if self.lead_zeros {
            init[7] = 111;
        } else {
            init[6] = 0;
            init[7] = 110;
        }
        vec![first, init.to_vec()]
    }

    fn frame(&mut self, r: &Readings, _now: Instant) -> Vec<u8> {
        let mut data = header();
        data[4] = 11;
        data[5] = 1;
        data[6] = 2;
        data[7] = 5;
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
            alarm: Alarm::Auto {
                limit: if self.fahrenheit { TEMP_LIMIT_F } else { TEMP_LIMIT_C },
                warning: 0,
            },
            notes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_match_protocol() {
        let mut ld = Display::new(&Settings { lead_zeros: true, ..Settings::default() });
        let init = ld.init();
        assert_eq!(&init[0][..9], &[16, 104, 1, 1, 2, 3, 1, 112, 22]);
        assert_eq!(&init[1][..9], &[16, 104, 1, 1, 2, 2, 1, 111, 22]);
        let off = Display::new(&Settings::default()).init();
        assert_eq!(&off[1][..9], &[16, 104, 1, 1, 2, 2, 0, 110, 22]);

        let r = Readings { cpu_usage: 4, cpu_power_w: 25, cpu_temp_milli: Some(42_000), ..Readings::default() };
        let frame = ld.frame(&r, Instant::now());
        let temp = 42f32.to_be_bytes();
        let mut expected = vec![16, 104, 1, 1, 11, 1, 2, 5, 0, 25, 0, temp[0], temp[1], temp[2], temp[3], 4];
        let sum: u16 = expected[1..].iter().map(|&x| x as u16).sum();
        expected.push((sum % 256) as u8);
        expected.push(22);
        assert_eq!(&frame[..18], &expected[..]);
        assert!(frame[18..].iter().all(|&b| b == 0));
        assert_eq!(frame.len(), 64);
    }
}
