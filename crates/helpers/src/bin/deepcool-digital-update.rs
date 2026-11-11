//! Installs a downloaded DeepCool Digital update with the system's package manager.
//!
//! The desktop app runs this as root through polkit (`pkexec`) after downloading a
//! release and verifying its checksum. It only installs a package named
//! `deepcool-digital-gui`, in the format this system installed the app with.

use std::{
    path::Path,
    process::{exit, Command},
};
use deepcool_core::update::{self, Installer};

const USAGE: &str = "Usage: deepcool-digital-update <install|check> <package file>

  install   Install the package (needs root; the app uses pkexec)
  check     Show what install would do, without installing";

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, file] = args.as_slice() else {
        eprintln!("{USAGE}");
        exit(2);
    };
    let install = match command.as_str() {
        "install" => true,
        "check" => false,
        _ => {
            eprintln!("{USAGE}");
            exit(2);
        }
    };
    if install && unsafe { libc::geteuid() } != 0 {
        fail("This command must be run as root (the app uses pkexec).");
    }

    let file = Path::new(file);
    if !file.is_absolute() {
        fail("The package path must be absolute.");
    }
    match std::fs::symlink_metadata(file) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => fail("The package must be a regular file."),
        Err(error) => fail(&format!("Could not open the package: {error}")),
    }
    let installer = Installer::detect(Path::new(update::INSTALLED_APP))
        .unwrap_or_else(|| fail("DeepCool Digital was not installed from a package, so it can't be updated this way."));
    let extension = match installer {
        Installer::Deb => ".deb",
        Installer::Rpm => ".rpm",
        Installer::Arch => ".pkg.tar.zst",
    };
    if !file.to_string_lossy().ends_with(extension) {
        fail(&format!("Expected a {} package.", installer.name()));
    }
    match installer.package_name(file) {
        Ok(name) if name == update::PACKAGE => {}
        Ok(name) => fail(&format!(
            "Refusing to install '{name}': this is not a DeepCool Digital package."
        )),
        Err(error) => fail(&error),
    }

    let command = installer.install_command(file);
    if !install {
        println!("{}", command.join(" "));
        return;
    }
    let status = Command::new(&command[0])
        .args(&command[1..])
        .env("DEBIAN_FRONTEND", "noninteractive")
        .status()
        .unwrap_or_else(|error| fail(&format!("Could not run {}: {error}", command[0])));
    if !status.success() {
        fail(&format!(
            "{} failed ({status}). Another installer may be running; try again later.",
            command[0]
        ));
    }
    println!("Update installed.");
}
