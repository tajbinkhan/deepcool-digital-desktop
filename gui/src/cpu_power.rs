//! CPU energy counters, which the display needs to show CPU power.
//!
//! Linux makes these counters readable only by root by default, because precise energy
//! readings have been used in side-channel attacks. The app can ask an administrator to
//! make them readable, using the `deepcool-digital-cpu-power` helper. Shared by the app
//! and that helper; every path is relative to `root` ("/" outside tests).

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// The helper program, installed next to the app.
pub const HELPER_NAME: &str = "deepcool-digital-cpu-power";
/// The udev rule that keeps the counters readable after a reboot.
pub const RULE_PATH: &str = "etc/udev/rules.d/71-deepcool-digital-cpu-power.rules";
/// Applied at boot by udev. It only uses the system's own tools, so it keeps working
/// (and stays harmless) even if the app is removed.
pub const RULE: &str = "\
# Written by DeepCool Digital: lets users read the CPU energy counter so the display can show CPU power.
# Delete this file (or use \"Turn off\" in the app) to restore the default; it applies at the next boot.
ACTION==\"add\", SUBSYSTEM==\"powercap\", KERNEL==\"intel-rapl:0\", RUN+=\"/bin/chmod 0444 /sys%p/energy_uj\"
ACTION==\"add\", SUBSYSTEM==\"hwmon\", ATTR{name}==\"amd_energy|zenergy\", RUN+=\"/bin/sh -c '/bin/chmod 0444 /sys%p/energy*_input'\"
";

/// Whether the display can show CPU power.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// At least one energy counter can be read by this user.
    Readable,
    /// Counters exist but only an administrator can read them.
    Restricted,
    /// This system has no supported energy counter.
    NoSensor,
}

/// The energy counters the controller can use: the RAPL package counter, and the
/// counters of the `amd_energy` and `zenergy` kernel modules.
pub fn counters(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let rapl = root.join("sys/class/powercap/intel-rapl:0/energy_uj");
    if rapl.is_file() {
        found.push(rapl);
    }
    if let Ok(entries) = fs::read_dir(root.join("sys/class/hwmon")) {
        let mut hwmons: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        hwmons.sort();
        for hwmon in hwmons {
            let name = fs::read_to_string(hwmon.join("name")).unwrap_or_default();
            if !matches!(name.trim(), "amd_energy" | "zenergy") {
                continue;
            }
            let Ok(files) = fs::read_dir(&hwmon) else { continue };
            let mut inputs: Vec<PathBuf> = files
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("energy") && name.ends_with("_input"))
                })
                .collect();
            inputs.sort();
            found.extend(inputs);
        }
    }
    found
}

fn readable(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|data| data.trim().parse::<u64>().is_ok())
}

pub fn access(root: &Path) -> Access {
    let counters = counters(root);
    if counters.is_empty() {
        Access::NoSensor
    } else if counters.iter().any(|path| readable(path)) {
        Access::Readable
    } else {
        Access::Restricted
    }
}

/// True when the app's rule is installed, so readings can be turned off again.
pub fn rule_installed(root: &Path) -> bool {
    root.join(RULE_PATH).is_file()
}

/// Makes the counters readable now and after every boot. Needs root.
pub fn enable(root: &Path) -> Result<usize, String> {
    let rule = root.join(RULE_PATH);
    fs::create_dir_all(rule.parent().unwrap()).map_err(|e| format!("Could not create the rules folder: {e}"))?;
    fs::write(&rule, RULE).map_err(|e| format!("Could not write {}: {e}", rule.display()))?;
    fs::set_permissions(&rule, fs::Permissions::from_mode(0o644)).map_err(|e| e.to_string())?;
    set_mode(root, 0o444)
}

/// Removes the rule and makes the counters root-only again. Needs root.
pub fn disable(root: &Path) -> Result<usize, String> {
    match fs::remove_file(root.join(RULE_PATH)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("Could not remove the rule: {error}"));
        }
        _ => {}
    }
    set_mode(root, 0o400)
}

fn set_mode(root: &Path, mode: u32) -> Result<usize, String> {
    let counters = counters(root);
    for path in &counters {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| format!("Could not change {}: {e}", path.display()))?;
    }
    Ok(counters.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_system(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("deepcool-cpu-power-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn counter(path: &Path, mode: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "123456\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn finds_rapl_and_amd_counters() {
        let root = fake_system("find");
        assert_eq!(access(&root), Access::NoSensor);
        counter(&root.join("sys/class/powercap/intel-rapl:0/energy_uj"), 0o444);
        let hwmon = root.join("sys/class/hwmon/hwmon3");
        counter(&hwmon.join("energy1_input"), 0o444);
        counter(&hwmon.join("energy2_input"), 0o444);
        fs::write(hwmon.join("name"), "zenergy\n").unwrap();
        let other = root.join("sys/class/hwmon/hwmon1");
        counter(&other.join("energy1_input"), 0o444);
        fs::write(other.join("name"), "nvme\n").unwrap();
        assert_eq!(counters(&root).len(), 3, "{:?}", counters(&root));
        assert_eq!(access(&root), Access::Readable);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn enable_and_disable() {
        let root = fake_system("toggle");
        let rapl = root.join("sys/class/powercap/intel-rapl:0/energy_uj");
        // Real counters are root-owned with mode 0400; this file is ours, so use 000 to make it unreadable.
        counter(&rapl, 0o000);
        // Root can read anything, so only check the restricted state as a normal user.
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(access(&root), Access::Restricted);
        }
        assert!(!rule_installed(&root));

        assert_eq!(enable(&root), Ok(1));
        assert!(rule_installed(&root));
        assert_eq!(fs::read_to_string(root.join(RULE_PATH)).unwrap(), RULE);
        assert_eq!(fs::metadata(&rapl).unwrap().permissions().mode() & 0o777, 0o444);
        assert_eq!(access(&root), Access::Readable);

        assert_eq!(disable(&root), Ok(1));
        assert!(!rule_installed(&root));
        assert_eq!(fs::metadata(&rapl).unwrap().permissions().mode() & 0o777, 0o400);
        // Turning off twice is fine.
        assert_eq!(disable(&root), Ok(1));
        let _ = fs::remove_dir_all(root);
    }
}
