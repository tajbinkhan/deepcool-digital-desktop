//! Lets users read the CPU energy counter so DeepCool displays can show CPU power.
//!
//! The desktop app runs this as root through polkit (`pkexec`) when you select
//! "Allow CPU power readings" or "Turn off". It accepts no paths or other input,
//! only one of the fixed commands below.

use deepcool_core::cpu_power::{self, Access};
use std::{path::Path, process::exit};

const USAGE: &str = "Usage: deepcool-digital-cpu-power <enable|disable|status>

  enable    Make the CPU energy counter readable by users, now and after every boot (needs root)
  disable   Restore the default: only root can read it (needs root)
  status    Show whether CPU power can be read";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command] = args.as_slice() else {
        eprintln!("{USAGE}");
        exit(2);
    };
    let root = Path::new("/");
    let needs_root = matches!(command.as_str(), "enable" | "disable");
    if needs_root && unsafe { libc::geteuid() } != 0 {
        eprintln!("This command must be run as root (the app uses pkexec).");
        exit(1);
    }
    let result = match command.as_str() {
        "enable" => cpu_power::enable(root).map(|count| format!("CPU power readings allowed ({count} counter(s)).")),
        "disable" => {
            cpu_power::disable(root).map(|count| format!("CPU power readings restricted ({count} counter(s))."))
        }
        "status" => Ok(match cpu_power::access(root) {
            Access::Readable => "readable",
            Access::Restricted => "restricted",
            Access::NoSensor => "no-sensor",
        }
        .to_owned()),
        "-h" | "--help" => Ok(USAGE.to_owned()),
        other => {
            eprintln!("Unknown command '{other}'.\n\n{USAGE}");
            exit(2);
        }
    };
    match result {
        Ok(message) => println!("{message}"),
        Err(error) => {
            eprintln!("{error}");
            exit(1);
        }
    }
}
