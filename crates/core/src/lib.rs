//! Device drivers and sensor readers for DeepCool digital displays.
//!
//! Used by both the command-line controller and the desktop app. Nothing here
//! prints or ends the process; problems are returned as [`Error`] values.

pub mod catalog;
pub mod cpu_power;
pub mod devices;
pub mod monitor;
pub mod runner;
pub mod system;
pub mod telemetry;

pub use devices::{driver_for, find_devices, vendor_for, Alarm, Driver, Found, Mode, Settings, Status};
pub use runner::{run, spawn, Running};
pub use telemetry::{Needs, Readings, Telemetry};

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The USB HID library could not start.
    Hid(String),
    /// The device exists but cannot be opened (usually permissions).
    DeviceAccess { pid: u16 },
    /// No driver exists for this product ID.
    UnsupportedDevice(u16),
    UnsupportedMode(Mode),
    UnsupportedSecondary(Mode),
    /// Writing to the device failed (for example it was unplugged).
    Write(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Hid(err) => write!(f, "{err}"),
            Error::DeviceAccess { .. } => write!(f, "Failed to access the USB device"),
            Error::UnsupportedDevice(pid) => write!(f, "Device with product ID {pid} is not yet supported"),
            Error::UnsupportedMode(mode) => {
                write!(f, "Display mode \"{}\" is not supported on your device", mode.symbol())
            }
            Error::UnsupportedSecondary(mode) => {
                write!(f, "Secondary display mode \"{}\" is not supported on your device", mode.symbol())
            }
            Error::Write(err) => write!(f, "Failed to write to the USB device: {err}"),
        }
    }
}

impl std::error::Error for Error {}
