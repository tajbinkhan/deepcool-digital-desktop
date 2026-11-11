//! What each product ID is called and which settings it supports.

use crate::devices::{Mode, CH510_PRODUCT_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    WaterCooler,
    AirCooler,
    Case,
}

impl Category {
    /// Stable key, used by the UI for translations and pictures.
    pub const fn key(&self) -> &'static str {
        match self {
            Category::WaterCooler => "water_cooler",
            Category::AirCooler => "air_cooler",
            Category::Case => "case",
        }
    }
}

/// Settings a product supports.
#[derive(Debug, Clone, PartialEq)]
pub struct Capabilities {
    pub modes: &'static [Mode],
    pub secondary: &'static [Mode],
    pub fahrenheit: bool,
    pub alarm: bool,
    pub rotate: bool,
    pub lead_zeros: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Product {
    pub pid: u16,
    pub series: &'static str,
    pub category: Category,
    /// Which display layout the device has, for previews.
    pub display: &'static str,
    pub capabilities: Capabilities,
}

const BASIC: &[Mode] = &[Mode::Auto, Mode::CpuTemperature, Mode::CpuUsage];
const LP: &[Mode] =
    &[Mode::CpuUsage, Mode::CpuTemperature, Mode::CpuPower, Mode::GpuUsage, Mode::GpuTemperature, Mode::GpuPower];

/// Looks up a supported product.
pub fn product(pid: u16) -> Option<Product> {
    use Category::*;
    let (series, category, display) = match pid {
        1..=4 => ("AK-Series", AirCooler, "ak"),
        5 | 21 => ("CH-Series", Case, "ch"),
        7 => ("MORPHEUS", Case, "ch"),
        6 => ("LS-Series", WaterCooler, "ls"),
        8 => ("AG-Series", AirCooler, "ag"),
        10 => ("LD-Series", WaterCooler, "ld"),
        12 => ("LP-Series", WaterCooler, "lp"),
        13 => ("LQ-Series", WaterCooler, "lq"),
        15 | 31 => ("ASSASSIN IV VC VISION", AirCooler, "lq"),
        16 => ("AK400 DIGITAL PRO", AirCooler, "pro"),
        17 => ("AK500 DIGITAL PRO", AirCooler, "pro"),
        18 => ("AK620 DIGITAL PRO", AirCooler, "pro"),
        19 => ("CH170 DIGITAL", Case, "ch2"),
        22 => ("CH270 DIGITAL", Case, "ch2"),
        27 => ("CH690 DIGITAL", Case, "ch2"),
        41 => ("AK620 G2 DIGITAL NYX", AirCooler, "lq"),
        42 => ("AK700 DIGITAL NYX", AirCooler, "lq"),
        43 => ("AK400 G2 DIGITAL NYX", AirCooler, "lq"),
        44 => ("AK500 G2 DIGITAL NYX", AirCooler, "lq"),
        CH510_PRODUCT_ID => ("CH510 MESH DIGITAL", Case, "ch510"),
        _ => return None,
    };
    let modes: &'static [Mode] = match pid {
        6 => &[Mode::Auto, Mode::CpuTemperature, Mode::CpuPower],
        12 => LP,
        19 | 22 | 27 => &[Mode::Auto, Mode::CpuFrequency, Mode::Gpu],
        CH510_PRODUCT_ID => &[Mode::Cpu, Mode::Gpu],
        10 | 13 | 15 | 16 | 17 | 18 | 31 | 41 | 42 | 43 | 44 => &[Mode::Auto],
        _ => BASIC,
    };
    let secondary: &'static [Mode] = match pid {
        12 => LP,
        5 | 7 | 21 => &[Mode::GpuTemperature, Mode::GpuUsage],
        _ => &[],
    };
    Some(Product {
        pid,
        series,
        category,
        display,
        capabilities: Capabilities {
            modes,
            secondary,
            fahrenheit: pid != 8,
            alarm: matches!(pid, 1..=4 | 6 | 8),
            rotate: pid == 12,
            lead_zeros: pid == 10,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{driver_for, Settings};

    #[test]
    fn every_listed_mode_is_accepted_by_its_driver() {
        for pid in (1..=50).chain([CH510_PRODUCT_ID]) {
            let Some(product) = product(pid) else {
                assert!(driver_for(pid, &Settings::default()).is_err(), "pid {pid} has a driver but no product");
                continue;
            };
            for &mode in product.capabilities.modes {
                let settings = Settings { mode, ..Settings::default() };
                assert!(driver_for(pid, &settings).is_ok(), "pid {pid} mode {mode:?}");
            }
            for &secondary in product.capabilities.secondary {
                let settings = Settings { secondary, ..Settings::default() };
                assert!(driver_for(pid, &settings).is_ok(), "pid {pid} secondary {secondary:?}");
            }
        }
    }
}
