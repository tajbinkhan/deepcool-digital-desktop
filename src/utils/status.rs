use colored::*;
use deepcool_core::{Alarm, Status};
use std::time::Duration;

pub fn print_device_status(
    status: &Status,
    rotation: Option<u16>,
    lead_zeros: Option<bool>,
    fahrenheit: bool,
    update: Duration,
) {
    let unit = if fahrenheit { "°F" } else { "°C" };
    println!("-----");
    match status.secondary {
        Some(s) => println!("DISP. MODE: {} | {}", status.mode.symbol().bright_cyan(), s.symbol().bright_cyan()),
        None => println!("DISP. MODE: {}", status.mode.symbol().bright_cyan()),
    }
    if let Some(r) = rotation {
        if r > 0 {
            println!("ROTATION:   {}", format!("{r}°").bright_cyan());
        } else {
            println!("ROTATION:   {}", "none".bright_black());
        }
    }
    if let Some(z) = lead_zeros {
        match z {
            true => println!("LEAD. ZERO: {}", "on".bright_green()),
            false => println!("LEAD. ZERO: {}", "off".bright_red()),
        }
    }
    println!("TEMP. UNIT: {}", unit.bright_cyan());
    match status.alarm {
        Alarm::Auto { limit, warning } if warning > 0 => println!(
            "ALARM:      {} | {} [warning: {}]",
            "auto".bright_green(),
            format!("{limit}{unit}").bright_cyan(),
            format!("{warning}{unit}").bright_cyan()
        ),
        Alarm::Auto { limit, .. } => {
            println!("ALARM:      {} | {}", "auto".bright_green(), format!("{limit}{unit}").bright_cyan())
        }
        Alarm::On { limit } => {
            println!("ALARM:      {} | {}", "on".bright_green(), format!("{limit}{unit}").bright_cyan())
        }
        Alarm::Off => println!("ALARM:      {}", "off".bright_red()),
        Alarm::NotSupported => println!("ALARM:      {}", "not supported".bright_black().italic()),
    }
    println!("-----");
    println!("Update interval: {}", format!("{:?}", update).bright_cyan());
    println!("\nPress {} to terminate", "Ctrl+C".bold());
}
