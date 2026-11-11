mod utils;

use colored::*;
use deepcool_core::{
    devices::{self, ak_series, ch510, ch_series, ch_series_gen2, ls_series, lp_series, ag_series, Mode, Settings},
    monitor::{cpu, gpu},
    runner,
    telemetry::Telemetry,
    Error,
};
use hidapi::HidApi;
use std::{process::exit, sync::mpsc};
use utils::{args::Args, status::print_device_status};

/// Product IDs that run the same driver.
const AUTO_ONLY: &[u16] = &[10, 13, 15, 16, 17, 18, 31, 41, 42, 43, 44];
const HARDCODED_ALARM: &[u16] = AUTO_ONLY;

/// Prints the modes a device supports.
fn print_supported_modes(pid: u16) {
    match pid {
        1..=4 | 8 => {
            let default = if pid == 8 { ag_series::DEFAULT_MODE } else { ak_series::DEFAULT_MODE };
            println!("Supported modes: {} [default: {}]", "auto cpu_temp cpu_usage".bold(), default.symbol());
        }
        6 => println!("Supported modes: {} [default: {}]", "auto cpu_temp cpu_power".bold(), ls_series::DEFAULT_MODE.symbol()),
        12 => {
            let modes = "cpu_usage cpu_temp cpu_power gpu_usage gpu_temp gpu_power";
            println!("Supported modes: {} [default: {}]", modes.bold(), lp_series::DEFAULT_MODE.symbol());
            println!("Supported secondary: {}", modes.bold());
        }
        19 | 22 | 27 => println!(
            "Supported modes: {} {} {} {} [default: {}]",
            "auto cpu_freq".bold(),
            "cpu_fan".bright_black().strikethrough(),
            "gpu".bold(),
            "psu".bright_black().strikethrough(),
            ch_series_gen2::DEFAULT_MODE.symbol()
        ),
        5 | 7 | 21 => {
            println!("Supported modes: {} [default: {}]", "auto cpu_temp cpu_usage".bold(), ch_series::DEFAULT_MODE.symbol());
            println!("Supported secondary: {}", "gpu_temp gpu_usage".bold());
        }
        devices::CH510_PRODUCT_ID => {
            println!("Supported modes: {} [default: {}]", "cpu gpu".bold(), ch510::DEFAULT_MODE.symbol())
        }
        _ => println!("Supported modes: {}", "auto".bold()),
    }
}

/// Warns about options the device ignores.
fn warn_ignored(pid: u16, args: &Args) {
    let has_secondary = matches!(pid, 12 | 5 | 7 | 21);
    if AUTO_ONLY.contains(&pid) && args.mode != Mode::Default {
        warning!("Display mode cannot be changed, value will be ignored");
    }
    if !has_secondary && args.secondary != Mode::Default {
        warning!("Secondary display mode is not supported, value will be ignored");
    }
    if pid == 8 && args.fahrenheit {
        warning!("Displaying ˚F is not supported, value will be ignored");
    }
    if args.alarm {
        if HARDCODED_ALARM.contains(&pid) {
            warning!("The alarm is hard-coded in your device, value will be ignored");
        } else if !matches!(pid, 1..=4 | 6 | 8) {
            warning!("Alarm is not supported, value will be ignored");
        }
    }
    if pid != 12 && args.rotate > 0 {
        warning!("Display rotation is not supported, value will be ignored");
    }
    if pid != 10 && args.lead_zeros {
        warning!("Displaying leading zeros is not supported, value will be ignored");
    }
}

fn fail(err: impl std::fmt::Display) -> ! {
    error!(err);
    exit(1);
}

fn main() {
    let args = Args::read();
    println!("--- Deepcool Digital Linux ---");

    // Find dedicated or integrated GPU
    let gpus = gpu::pci::get_gpu_list();
    let pci_device = gpu::select(&gpus, args.gpuid);
    if pci_device.is_none() && args.gpuid.is_some() && !gpus.is_empty() {
        fail("No GPU was found with the specified GPUID");
    }

    // Display CPU and GPU name
    let cpu_name = cpu::get_name().unwrap_or_else(|| "Unknown CPU".into());
    println!("CPU MON.: {}", cpu_name.bright_green());
    match &pci_device {
        Some(gpu) => println!("GPU MON.: {}", gpu.name.bright_green()),
        None => println!("GPU MON.: {}", "none".bright_black()),
    };
    println!("-----");

    // Find DeepCool device
    let api = HidApi::new().unwrap_or_else(|err| fail(err));
    let Some(found) = devices::find_devices(&api).into_iter().find(|d| args.pid == 0 || d.pid == args.pid) else {
        fail(if args.pid > 0 {
            "No DeepCool device was found with the specified PID"
        } else {
            "No DeepCool device was found"
        });
    };
    println!("Device found: {}", found.name.bright_green());

    let settings = Settings {
        mode: args.mode,
        secondary: args.secondary,
        update: args.update,
        fahrenheit: args.fahrenheit,
        alarm: args.alarm,
        rotate: args.rotate,
        lead_zeros: args.lead_zeros,
    };
    let mut driver = match devices::driver_for(found.pid, &settings) {
        Ok(driver) => driver,
        Err(Error::UnsupportedDevice(_)) => {
            println!("Device not yet supported!");
            println!("\nPlease create an issue on GitHub providing your device name and the following information:");
            let device = api.open(found.vid, found.pid).unwrap_or_else(|_| access_error());
            let info = device.get_device_info().unwrap_or_else(|err| fail(err));
            println!("Vendor ID: {}", info.vendor_id().to_string().bright_cyan());
            println!("Device ID: {}", info.product_id().to_string().bright_cyan());
            println!("Vendor name: {}", info.manufacturer_string().unwrap_or("").bright_cyan());
            println!("Device name: {}", info.product_string().unwrap_or("").bright_cyan());
            return;
        }
        Err(err) => fail(err),
    };

    // Print current configuration & warnings
    print_supported_modes(found.pid);
    let status = driver.status();
    print_device_status(
        &status,
        (found.pid == 12).then_some(args.rotate),
        (found.pid == 10).then_some(args.lead_zeros),
        args.fahrenheit && found.pid != 8,
        args.update,
    );
    warn_ignored(found.pid, &args);

    // Connect to device
    let (mut device, product) = runner::open(&api, found.vid, found.pid).unwrap_or_else(|_| access_error());
    driver.on_open(&product);

    // Warn about missing sensors this display uses
    let telemetry = Telemetry::new(pci_device.as_ref());
    let sensors = telemetry.status();
    let needs = driver.needs();
    for note in &status.notes {
        warning!(note);
    }
    if needs.cpu_temp && !sensors.cpu_temp {
        warning!("No supported CPU temperature sensor was found");
        eprintln!("         CPU temperature will not be displayed, and alarm will be disabled.");
        eprintln!("         Supported kernel modules are: asusec, coretemp, k10temp, and zenpower.");
    }
    if needs.cpu_power && !sensors.cpu_power {
        warning!("No supported CPU energy sensor was found");
        eprintln!("         CPU power consumption will not be displayed.");
        eprintln!("         Supported kernel modules are: amd_energy, intel_rapl, and zenergy.");
        eprintln!("         If one is loaded, its energy file may not be readable by this user.");
    }
    if needs.gpu {
        if let Some(problem) = &sensors.gpu_problem {
            warning!(problem);
            if !sensors.gpu_available {
                eprintln!("         GPU information will not be displayed.");
            }
        }
    }

    // Display loop. The sender stays alive, so this runs until Ctrl+C or an error.
    let (_keep_running, stop) = mpsc::channel();
    if let Err(err) = runner::run(&mut device, driver.as_mut(), &telemetry, args.update, &stop, &mut |_| {}) {
        fail(err);
    }
}

fn access_error() -> ! {
    error!("Failed to access the USB device");
    eprintln!("       Try to run the program as root or give permission to the neccesary resources.");
    eprintln!("       You can find instructions about rootless mode on GitHub.");
    exit(1);
}
