//! Display module for:
//! - CH510 MESH DIGITAL

use super::{check_mode, Alarm, Driver, Mode, Settings, Status};
use crate::{
    telemetry::{Needs, Readings},
    Error,
};
use std::time::Instant;

pub const DEFAULT_MODE: Mode = Mode::Cpu;

pub struct Display {
    pub mode: Mode,
    fahrenheit: bool,
}

impl Display {
    pub fn new(settings: &Settings) -> Result<Self, Error> {
        let mode = check_mode(settings.mode, &[Mode::Cpu, Mode::Gpu], DEFAULT_MODE)?;
        Ok(Display { mode, fahrenheit: settings.fahrenheit })
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs { cpu_temp: self.mode == Mode::Cpu, gpu: self.mode == Mode::Gpu, ..Needs::default() }
    }

    fn frame(&mut self, r: &Readings, _now: Instant) -> Vec<u8> {
        let unit = if self.fahrenheit { "F" } else { "C" };
        let (usage, temp) = match self.mode {
            Mode::Gpu => (r.gpu.usage, r.gpu_temp(self.fahrenheit)),
            _ => (r.cpu_usage, r.cpu_temp(self.fahrenheit)),
        };
        format!("HLXDATA({usage},{temp},0,0,{unit})\r\n").into_bytes()
    }

    fn status(&self) -> Status {
        Status { mode: self.mode, secondary: None, alarm: Alarm::NotSupported, notes: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_protocol() {
        let mut d = Display::new(&Settings { fahrenheit: true, ..Settings::default() }).unwrap();
        let r = Readings { cpu_usage: 5, cpu_temp_milli: Some(40_000), ..Readings::default() };
        assert_eq!(d.frame(&r, Instant::now()), b"HLXDATA(5,104,0,0,F)\r\n");
    }
}
