//! Display module for:
//! - LP240
//! - LP360

use super::{checksum, packet, Alarm, Driver, Mode, Settings, Status};
use crate::{
    telemetry::{Needs, Readings},
    Error,
};
use std::time::Instant;

/// Helper module for the LP Series.
mod dot_matrix {
    pub enum Unit {
        Percent,
        Celsius,
        Fahrenheit,
        Watt,
        Empty,
    }

    impl Unit {
        /// Returns a 5x5 matrix array representing the unit.
        pub const fn get_pattern(self) -> [[bool; 5]; 5] {
            match self {
                Unit::Percent => [
                    [true, true, false, false, true],
                    [true, true, false, true, false],
                    [false, false, true, false, false],
                    [false, true, false, true, true],
                    [true, false, false, true, true],
                ],
                Unit::Celsius => [
                    [true, false, false, false, false],
                    [false, false, true, true, false],
                    [false, true, false, false, false],
                    [false, true, false, false, false],
                    [false, false, true, true, false],
                ],
                Unit::Fahrenheit => [
                    [true, false, true, true, false],
                    [false, false, true, false, false],
                    [false, false, true, true, false],
                    [false, false, true, false, false],
                    [false, false, true, false, false],
                ],
                Unit::Watt => [
                    [false, false, false, false, false],
                    [true, false, true, false, true],
                    [true, false, true, false, true],
                    [true, false, true, false, true],
                    [false, true, false, true, false],
                ],
                Unit::Empty => [[false; 5]; 5],
            }
        }
    }

    /// Returns a 3x5 matrix array representing the number.
    pub const fn get_number_pattern(num: u8) -> [[bool; 3]; 5] {
        match num {
            0 => [
                [true, true, true],
                [true, false, true],
                [true, false, true],
                [true, false, true],
                [true, true, true],
            ],
            1 => [
                [false, true, false],
                [true, true, false],
                [false, true, false],
                [false, true, false],
                [true, true, true],
            ],
            2 => [
                [true, true, true],
                [false, false, true],
                [false, true, false],
                [true, false, false],
                [true, true, true],
            ],
            3 => [
                [true, true, true],
                [false, false, true],
                [true, true, true],
                [false, false, true],
                [true, true, true],
            ],
            4 => [
                [true, false, true],
                [true, false, true],
                [true, true, true],
                [false, false, true],
                [false, false, true],
            ],
            5 => [
                [true, true, true],
                [true, false, false],
                [true, true, true],
                [false, false, true],
                [true, true, true],
            ],
            6 => [
                [true, true, true],
                [true, false, false],
                [true, true, true],
                [true, false, true],
                [true, true, true],
            ],
            7 => [
                [true, true, true],
                [false, false, true],
                [false, true, false],
                [false, true, false],
                [false, true, false],
            ],
            8 => [
                [true, true, true],
                [true, false, true],
                [true, true, true],
                [true, false, true],
                [true, true, true],
            ],
            9 => [
                [true, true, true],
                [true, false, true],
                [true, true, true],
                [false, false, true],
                [true, true, true],
            ],
            _ => [[false; 3]; 5],
        }
    }

    /// Inserts a pattern into the 14x14 matrix at the defined position.
    pub fn insert_pattern<const M: usize, const N: usize>(
        matrix: &mut [[bool; 14]; 14],
        pattern: [[bool; M]; N],
        row_pos: usize,
        col_pos: usize,
    ) {
        // Calculate the actual dimensions that will fit
        let max_rows = (14 - row_pos).min(N);
        let max_cols = (14 - col_pos).min(M);

        // Insert the pattern
        for i in 0..max_rows {
            for j in 0..max_cols {
                matrix[row_pos + i][col_pos + j] = pattern[i][j];
            }
        }
    }

    /// Rotates the matrix values by a given degree.
    pub fn rotate_matrix(matrix: &mut [[bool; 14]; 14], degrees: u16) {
        let mut rotated = [[false; 14]; 14];
        match degrees {
            90 => {
                for i in 0..14 {
                    for j in 0..14 {
                        rotated[j][13 - i] = matrix[i][j];
                    }
                }
            }
            180 => {
                for i in 0..14 {
                    for j in 0..14 {
                        rotated[13 - i][13 - j] = matrix[i][j];
                    }
                }
            }
            270 => {
                for i in 0..14 {
                    for j in 0..14 {
                        rotated[13 - j][i] = matrix[i][j];
                    }
                }
            }
            _ => {
                return;
            }
        }
        *matrix = rotated;
    }

    /// Converts the 14x14 matrix to be data bytes.
    pub fn matrix_to_bytes(matrix: [[bool; 14]; 14]) -> [u8; 28] {
        let mut bytes: [u8; 28] = [0; 28];

        // Values for each row position (HEX: 10, 20, 40, 80, 01, 02, 04)
        const ROW_VALUES: [u8; 7] = [16, 32, 64, 128, 1, 2, 4];

        // First 14 bytes (odd rows)
        for col in 0..14 {
            let mut byte = 0;
            for row_id in 0..7 {
                if matrix[row_id * 2][col] {
                    byte += ROW_VALUES[row_id];
                }
            }
            bytes[col] = byte;
        }

        // Last 14 bytes (even rows - reversed)
        for col in 0..14 {
            let mut byte = 0;
            for row_id in 0..7 {
                if matrix[row_id * 2 + 1][col] {
                    byte += ROW_VALUES[row_id];
                }
            }
            bytes[27 - col] = byte;
        }

        bytes
    }
}

pub const DEFAULT_MODE: Mode = Mode::CpuUsage;

const MODES: [Mode; 6] =
    [Mode::CpuUsage, Mode::CpuTemperature, Mode::CpuPower, Mode::GpuUsage, Mode::GpuTemperature, Mode::GpuPower];

pub struct Display {
    pub mode: Mode,
    pub secondary: Option<Mode>,
    fahrenheit: bool,
    rotate: u16,
}

impl Display {
    pub fn new(settings: &Settings) -> Result<Self, Error> {
        let mode = super::check_mode(settings.mode, &MODES, DEFAULT_MODE)?;
        let secondary = match settings.secondary {
            Mode::Default => None,
            mode if MODES.contains(&mode) => Some(mode),
            mode => return Err(Error::UnsupportedSecondary(mode)),
        };
        Ok(Display { mode, secondary, fahrenheit: settings.fahrenheit, rotate: settings.rotate })
    }

    fn uses(&self, modes: &[Mode]) -> bool {
        modes.contains(&self.mode) || self.secondary.is_some_and(|s| modes.contains(&s))
    }

    fn value(&self, mode: Mode, r: &Readings) -> (u16, dot_matrix::Unit) {
        let temp_unit = if self.fahrenheit { dot_matrix::Unit::Fahrenheit } else { dot_matrix::Unit::Celsius };
        match mode {
            Mode::CpuUsage => (r.cpu_usage as u16, dot_matrix::Unit::Percent),
            Mode::CpuTemperature => (r.cpu_temp(self.fahrenheit) as u16, temp_unit),
            Mode::CpuPower => (r.cpu_power_w, dot_matrix::Unit::Watt),
            Mode::GpuUsage => (r.gpu.usage as u16, dot_matrix::Unit::Percent),
            Mode::GpuTemperature => (r.gpu_temp(self.fahrenheit) as u16, temp_unit),
            Mode::GpuPower => (r.gpu.power_w, dot_matrix::Unit::Watt),
            _ => (0, dot_matrix::Unit::Empty),
        }
    }
}

fn insert_data_to_matrix(matrix: &mut [[bool; 14]; 14], row_id: usize, data: (u16, dot_matrix::Unit)) {
    let (value, unit) = data;
    if value / 100 < 1 {
        // 2-digit number
        dot_matrix::insert_pattern(matrix, dot_matrix::get_number_pattern((value / 10) as u8), row_id, 1);
        dot_matrix::insert_pattern(matrix, dot_matrix::get_number_pattern((value % 10) as u8), row_id, 5);
        dot_matrix::insert_pattern(matrix, unit.get_pattern(), 5, 9);
    } else {
        // 3-digit number
        dot_matrix::insert_pattern(matrix, dot_matrix::get_number_pattern((value / 100) as u8), row_id, 1);
        dot_matrix::insert_pattern(matrix, dot_matrix::get_number_pattern((value % 100 / 10) as u8), row_id, 5);
        dot_matrix::insert_pattern(matrix, dot_matrix::get_number_pattern((value % 10) as u8), row_id, 9);
        dot_matrix::insert_pattern(matrix, unit.get_pattern(), 5, 13);
    }
}

impl Driver for Display {
    fn needs(&self) -> Needs {
        Needs {
            cpu_temp: self.uses(&[Mode::CpuTemperature]),
            cpu_power: self.uses(&[Mode::CpuPower]),
            cpu_freq: false,
            gpu: self.uses(&[Mode::GpuUsage, Mode::GpuTemperature, Mode::GpuPower]),
        }
    }

    fn frame(&mut self, r: &Readings, _now: Instant) -> Vec<u8> {
        let mut data = packet();
        data[1..6].copy_from_slice(&[104, 1, 5, 29, 1]);
        let mut matrix = [[false; 14]; 14];
        match self.secondary {
            Some(secondary) => {
                insert_data_to_matrix(&mut matrix, 1, self.value(self.mode, r));
                insert_data_to_matrix(&mut matrix, 8, self.value(secondary, r));
            }
            None => insert_data_to_matrix(&mut matrix, 5, self.value(self.mode, r)),
        }
        if self.rotate > 0 {
            dot_matrix::rotate_matrix(&mut matrix, self.rotate);
        }
        data[6..=33].copy_from_slice(&dot_matrix::matrix_to_bytes(matrix));
        data[34] = checksum(&data[1..=33]);
        data[35] = 22;
        data.to_vec()
    }

    fn status(&self) -> Status {
        Status { mode: self.mode, secondary: self.secondary, alarm: Alarm::NotSupported, notes: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_header_and_checksum() {
        let mut d = Display::new(&Settings::default()).unwrap();
        let r = Readings { cpu_usage: 42, ..Readings::default() };
        let f = d.frame(&r, Instant::now());
        assert_eq!(&f[..6], &[16, 104, 1, 5, 29, 1]);
        assert!(f[6..34].iter().any(|&b| b != 0));
        assert_eq!(f[34], checksum(&f[1..=33]));
        assert_eq!(f[35], 22);
    }

    #[test]
    fn rotation_changes_pixels_but_not_count() {
        let r = Readings { cpu_usage: 88, ..Readings::default() };
        let mut plain = Display::new(&Settings::default()).unwrap();
        let mut turned = Display::new(&Settings { rotate: 180, ..Settings::default() }).unwrap();
        let (a, b) = (plain.frame(&r, Instant::now()), turned.frame(&r, Instant::now()));
        assert_ne!(a[6..34], b[6..34]);
        let ones = |f: &[u8]| f[6..34].iter().map(|b| b.count_ones()).sum::<u32>();
        assert_eq!(ones(&a), ones(&b));
    }

    #[test]
    fn secondary_validation() {
        let s = Settings { secondary: Mode::GpuPower, ..Settings::default() };
        let d = Display::new(&s).unwrap();
        assert_eq!(d.secondary, Some(Mode::GpuPower));
        assert!(d.needs().gpu);
    }
}
