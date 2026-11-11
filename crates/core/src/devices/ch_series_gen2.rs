//! Display module for:
//! - CH170 DIGITAL
//! - CH270 DIGITAL
//! - CH690 DIGITAL

use super::{checksum, check_mode, packet, put_f32, put_u16, Alarm, Cycle, Driver, Mode, Settings, Status};
use crate::{
    telemetry::{Needs, Readings},
    Error,
};
use std::time::Instant;

pub const DEFAULT_MODE: Mode = Mode::CpuFrequency;

pub struct Display {
    pub mode: Mode,
    cycle: Cycle,
    fahrenheit: bool,
}

impl Display {
    pub fn new(settings: &Settings) -> Result<Self, Error> {
        let mode = check_mode(
            settings.mode,
            &[Mode::Auto, Mode::CpuFrequency, Mode::CpuFan, Mode::Gpu, Mode::Psu],
            DEFAULT_MODE,
        )?;
        Ok(Display { mode, cycle: Cycle::new(Mode::CpuFrequency, Mode::Gpu), fahrenheit: settings.fahrenheit })
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        let cpu = matches!(self.mode, Mode::CpuFrequency | Mode::CpuFan | Mode::Auto);
        Needs {
            cpu_temp: cpu,
            cpu_power: cpu,
            cpu_freq: matches!(self.mode, Mode::CpuFrequency | Mode::Auto),
            gpu: matches!(self.mode, Mode::Gpu | Mode::Auto),
        }
    }

    fn frame(&mut self, r: &Readings, now: Instant) -> Vec<u8> {
        let mode = if self.mode == Mode::Auto { self.cycle.current(now) } else { self.mode };
        let mut data = packet();
        data[1..6].copy_from_slice(&[104, 1, 6, 35, 1]);
        data[9] = self.fahrenheit as u8;
        data[6] = match mode {
            Mode::CpuFrequency => 2,
            Mode::CpuFan => 3,
            Mode::Gpu => 4,
            Mode::Psu => 5,
            _ => 0,
        };
        match mode {
            Mode::CpuFrequency | Mode::CpuFan => {
                put_u16(&mut data, 7, r.cpu_power_w);
                put_f32(&mut data, 10, r.cpu_temp(self.fahrenheit));
                data[14] = r.cpu_usage;
                if mode == Mode::CpuFrequency {
                    put_u16(&mut data, 15, r.cpu_mhz);
                }
            }
            Mode::Gpu => {
                put_u16(&mut data, 19, r.gpu.power_w);
                put_f32(&mut data, 21, r.gpu_temp(self.fahrenheit));
                data[25] = r.gpu.usage;
                put_u16(&mut data, 26, r.gpu.mhz);
            }
            _ => (),
        }
        data[40] = checksum(&data[1..=39]);
        data[41] = 22;
        data.to_vec()
    }

    fn status(&self) -> Status {
        let notes = match self.mode {
            Mode::CpuFan => vec!["CPU fan speed monitoring is not yet supported"],
            Mode::Psu => vec!["PSU monitoring is not yet supported"],
            Mode::Auto => vec!["Display mode \"auto\" only cycles between fully supported modes"],
            _ => Vec::new(),
        };
        Status { mode: self.mode, secondary: None, alarm: Alarm::NotSupported, notes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::GpuSample;

    #[test]
    fn cpu_and_gpu_frames() {
        let r = Readings {
            cpu_usage: 12,
            cpu_power_w: 65,
            cpu_temp_milli: Some(48_000),
            cpu_mhz: 4800,
            gpu: GpuSample { usage: 99, power_w: 250, temp_milli: Some(70_000), mhz: 2500 },
        };
        let mut d = Display::new(&Settings::default()).unwrap();
        let f = d.frame(&r, Instant::now());
        assert_eq!(&f[..10], &[16, 104, 1, 6, 35, 1, 2, 0, 65, 0]);
        assert_eq!(&f[10..14], &48f32.to_be_bytes());
        assert_eq!(&f[14..17], &[12, (4800u16 >> 8) as u8, (4800u16 & 255) as u8]);
        assert!(f[19..28].iter().all(|&b| b == 0));
        assert_eq!(f[40], checksum(&f[1..=39]));
        assert_eq!(f[41], 22);

        let mut d = Display::new(&Settings { mode: Mode::Gpu, fahrenheit: true, ..Settings::default() }).unwrap();
        let f = d.frame(&r, Instant::now());
        assert_eq!(&f[6..10], &[4, 0, 0, 1]);
        assert_eq!(&f[19..21], &[0, 250]);
        assert_eq!(&f[21..25], &158f32.to_be_bytes());
        assert_eq!(&f[25..28], &[99, (2500u16 >> 8) as u8, (2500u16 & 255) as u8]);
    }
}
