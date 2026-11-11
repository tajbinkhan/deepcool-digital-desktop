//! Saved settings. The file formats are the same as the earlier egui app, so
//! existing per-device profiles and preferences carry over. Unknown keys are kept.

use deepcool_core::{Mode, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const DEVICE_DEFAULT: &str = "Device default";

fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(variable) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback),
    }
}

fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

pub fn profiles_path() -> PathBuf {
    config_dir().join("deepcool-digital/settings.json")
}

pub fn preferences_path() -> PathBuf {
    config_dir().join("deepcool-digital/preferences.json")
}

pub fn autostart_path() -> PathBuf {
    config_dir().join("autostart/deepcool-digital.desktop")
}

fn read_object(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str::<Value>(&data).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// Writes JSON atomically so a crash never leaves a half-written file.
fn write_object(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_string_pretty(map).map_err(std::io::Error::other)?)?;
        fs::rename(&temporary, path)
    };
    write().map_err(|err| format!("Could not save settings: {err}"))
}

/// Settings for one device, as stored on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Mode symbol such as `cpu_temp`, or "Device default".
    pub mode: String,
    pub secondary: String,
    pub rotate: u16,
    pub fahrenheit: bool,
    pub alarm: bool,
    pub zeros: bool,
    /// Update interval in milliseconds (100–2000).
    pub update: u64,
    /// GPU to monitor, e.g. `nvidia:1`; empty for automatic.
    pub gpuid: String,
    /// Start this display whenever the app runs, and keep it running.
    pub autostart: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            mode: DEVICE_DEFAULT.into(),
            secondary: DEVICE_DEFAULT.into(),
            rotate: 0,
            fahrenheit: false,
            alarm: false,
            zeros: false,
            update: 1000,
            gpuid: String::new(),
            autostart: false,
        }
    }
}

fn mode(symbol: &str) -> Mode {
    Mode::get(symbol).unwrap_or(Mode::Default)
}

impl Profile {
    /// Reads a stored profile, accepting the older string/number variants.
    pub fn from_json(value: &Value) -> Profile {
        let mut profile = Profile::default();
        let Some(object) = value.as_object() else {
            return profile;
        };
        let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_owned);
        let flag = |key: &str| object.get(key).and_then(Value::as_bool).unwrap_or(false);
        let number = |key: &str| match object.get(key) {
            Some(Value::Number(n)) => n.as_u64(),
            Some(Value::String(s)) => s.trim().parse().ok(),
            _ => None,
        };
        if let Some(mode) = text("mode") {
            profile.mode = mode;
        }
        if let Some(secondary) = text("secondary") {
            profile.secondary = secondary;
        }
        profile.rotate = number("rotate").filter(|r| [0, 90, 180, 270].contains(r)).unwrap_or(0) as u16;
        profile.fahrenheit = flag("fahrenheit");
        profile.alarm = flag("alarm");
        profile.zeros = flag("zeros");
        profile.update = number("update").unwrap_or(1000).clamp(100, 2000);
        profile.gpuid = text("gpuid").unwrap_or_default();
        profile.autostart = flag("autostart");
        profile
    }

    fn to_json(&self) -> Value {
        // `rotate` stays a string, as the earlier app wrote it.
        serde_json::json!({
            "mode": self.mode,
            "secondary": self.secondary,
            "rotate": self.rotate.to_string(),
            "fahrenheit": self.fahrenheit,
            "alarm": self.alarm,
            "zeros": self.zeros,
            "update": self.update,
            "gpuid": self.gpuid,
            "autostart": self.autostart,
        })
    }

    /// Driver settings. `fahrenheit` comes from the app-wide unit.
    pub fn settings(&self, fahrenheit: bool) -> Settings {
        Settings {
            mode: mode(&self.mode),
            secondary: mode(&self.secondary),
            update: Duration::from_millis(self.update.clamp(100, 2000)),
            fahrenheit,
            alarm: self.alarm,
            rotate: self.rotate,
            lead_zeros: self.zeros,
        }
    }
}

/// A device's saved profile. A device seen for the first time shows readings
/// right away, like the official app; saved profiles keep their own choice.
pub fn load_profile(pid: u16) -> Profile {
    read_object(&profiles_path())
        .get(&pid.to_string())
        .map(Profile::from_json)
        .unwrap_or(Profile { autostart: true, ..Profile::default() })
}

pub fn load_profiles() -> Vec<(u16, Profile)> {
    read_object(&profiles_path())
        .iter()
        .filter_map(|(key, value)| Some((key.parse().ok()?, Profile::from_json(value))))
        .collect()
}

pub fn save_profile(pid: u16, profile: &Profile) -> Result<(), String> {
    let path = profiles_path();
    let mut all = read_object(&path);
    all.insert(pid.to_string(), profile.to_json());
    write_object(&path, &all)
}

/// App-wide preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    /// Keep displays running after the window is closed.
    pub run_in_background: bool,
    /// Look for new versions automatically.
    pub check_for_updates: bool,
    pub fahrenheit: bool,
    /// UI language code, e.g. `en`.
    pub language: String,
    /// Start in the background at sign-in (stored as the autostart entry, not in the file).
    pub start_at_login: bool,
}

pub fn load_preferences() -> Preferences {
    let saved = read_object(&preferences_path());
    let flag = |key: &str, default: bool| saved.get(key).and_then(Value::as_bool).unwrap_or(default);
    // Before the app-wide unit existed, each device had its own; use them if all agree.
    let legacy_fahrenheit = {
        let profiles = load_profiles();
        !profiles.is_empty() && profiles.iter().all(|(_, p)| p.fahrenheit)
    };
    Preferences {
        run_in_background: flag("run_in_background", true),
        check_for_updates: flag("check_for_updates", true),
        fahrenheit: flag("fahrenheit", legacy_fahrenheit),
        language: saved.get("language").and_then(Value::as_str).unwrap_or("").to_owned(),
        start_at_login: autostart_path().exists(),
    }
}

pub fn save_preferences(preferences: &Preferences) -> Result<(), String> {
    let path = preferences_path();
    let mut saved = read_object(&path);
    saved.insert("run_in_background".into(), preferences.run_in_background.into());
    saved.insert("check_for_updates".into(), preferences.check_for_updates.into());
    saved.insert("fahrenheit".into(), preferences.fahrenheit.into());
    saved.insert("language".into(), preferences.language.clone().into());
    write_object(&path, &saved)?;
    set_start_at_login(preferences.start_at_login)
}

/// Contents of the sign-in entry that starts the given executable in the background.
pub fn autostart_entry(executable: &str) -> Result<String, String> {
    if executable.contains(['\n', '\r']) {
        return Err("Unsupported launcher path.".into());
    }
    let escaped = executable
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=DeepCool Digital\n\
         Comment=Runs your DeepCool displays in the background\nExec=\"{escaped}\" --background\n\
         Icon=deepcool-digital\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    ))
}

fn set_start_at_login(enabled: bool) -> Result<(), String> {
    let path = autostart_path();
    if !enabled {
        return match fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(format!("Could not turn off auto-start: {err}")),
            _ => Ok(()),
        };
    }
    // An AppImage runs from a temporary mount; start the AppImage file itself.
    let executable = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or("Could not find this program's location.")?;
    let entry = autostart_entry(&executable.to_string_lossy())?;
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(path.parent().unwrap_or(Path::new("/")))?;
        fs::write(&path, entry)
    };
    write().map_err(|err| format!("Could not turn on auto-start: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_earlier_apps_profile() {
        let value = serde_json::json!({
            "alarm": false, "autostart": true, "fahrenheit": false, "gpuid": "",
            "mode": "Device default", "rotate": "0", "secondary": "Device default",
            "update": 1000, "zeros": true
        });
        let profile = Profile::from_json(&value);
        assert!(profile.autostart && profile.zeros);
        assert_eq!(profile.update, 1000);
        assert_eq!(Profile::from_json(&profile.to_json()), profile);
        let settings = profile.settings(true);
        assert_eq!(settings.mode, Mode::Default);
        assert!(settings.fahrenheit && settings.lead_zeros);
    }

    #[test]
    fn tolerates_bad_values() {
        let value = serde_json::json!({ "update": "99999", "rotate": 45, "mode": 3 });
        let profile = Profile::from_json(&value);
        assert_eq!(profile.update, 2000);
        assert_eq!(profile.rotate, 0);
        assert_eq!(profile.mode, DEVICE_DEFAULT);
        assert_eq!(Profile::from_json(&Value::Null), Profile::default());
    }

    #[test]
    fn autostart_entry_escapes_path() {
        let entry = autostart_entry("/opt/My \"App\"/$bin").unwrap();
        assert!(entry.contains("Exec=\"/opt/My \\\"App\\\"/\\$bin\" --background"));
        assert!(autostart_entry("/bad\npath").is_err());
    }
}
