//! Permission to read the CPU energy counter, which CPU power readings need.
//!
//! Linux lets only root read the counter by default. Allowing it runs the
//! packaged helper as root through polkit (`pkexec`), which installs a udev rule
//! so the change lasts after restarts.

use deepcool_core::cpu_power::{self, Access, HELPER_NAME};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Where packages install the helper; the polkit policy names this exact path.
const INSTALLED_HELPER_DIR: &str = "/usr/lib/deepcool-digital";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerStatus {
    /// `readable`, `restricted` or `no_sensor`.
    pub access: &'static str,
    /// True when this app's rule is installed, so readings can be turned off again.
    pub rule_installed: bool,
}

pub fn status() -> PowerStatus {
    let root = Path::new("/");
    PowerStatus {
        access: match cpu_power::access(root) {
            Access::Readable => "readable",
            Access::Restricted => "restricted",
            Access::NoSensor => "no_sensor",
        },
        rule_installed: cpu_power::rule_installed(root),
    }
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The helper next to this program (development builds), else the packaged one.
fn helper() -> Result<PathBuf, String> {
    let beside = std::env::current_exe().ok().map(|exe| exe.with_file_name(HELPER_NAME));
    let installed = Path::new(INSTALLED_HELPER_DIR).join(HELPER_NAME);
    beside
        .into_iter()
        .chain([installed])
        .find(|path| is_executable(path))
        .ok_or_else(|| format!("The {HELPER_NAME} helper is missing. Reinstall DeepCool Digital."))
}

/// Allows (or restricts again) CPU power readings. Blocks while the password prompt is open.
pub fn set(allow: bool) -> Result<(), String> {
    let helper = helper()?;
    let action = if allow { "enable" } else { "disable" };
    match Command::new("pkexec").arg(&helper).arg(action).output() {
        Ok(output) if output.status.success() => Ok(()),
        // pkexec exits with 126 when the prompt is dismissed and 127 when not authorised.
        Ok(output) if output.status.code() == Some(126) => Err("Cancelled: the password prompt was closed.".into()),
        Ok(output) if output.status.code() == Some(127) => {
            Err("Not allowed: an administrator password is needed to change this.".into())
        }
        Ok(output) => Err(format!(
            "Could not change CPU power access: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "pkexec is not installed. Install polkit, or run: sudo {} {action}",
            helper.display()
        )),
        Err(err) => Err(format!("Could not ask for permission: {err}")),
    }
}
