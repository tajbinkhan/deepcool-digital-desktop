//! Device capabilities, argument validation, settings persistence and controller
//! process management. Nothing in this module depends on the UI toolkit.

use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    ffi::CString,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub const DEVICE_DEFAULT: &str = "Device default";
pub const ROTATIONS: [&str; 4] = ["0", "90", "180", "270"];
pub const UPDATE_RANGE: std::ops::RangeInclusive<i64> = 100..=2000;
const BASIC: &[&str] = &["auto", "cpu_temp", "cpu_usage"];
const LP: &[&str] = &[
    "cpu_usage",
    "cpu_temp",
    "cpu_power",
    "gpu_usage",
    "gpu_temp",
    "gpu_power",
];
const SUPPORTED: &[u32] = &[
    1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 13, 15, 16, 17, 18, 19, 21, 22, 27, 31, 41, 42, 43, 44, 4352,
];
const STOP_GRACE: Duration = Duration::from_secs(3);

/// Controls the controller supports for one product ID.
#[derive(Debug, Clone, PartialEq)]
pub struct Capabilities {
    pub modes: &'static [&'static str],
    pub secondary: &'static [&'static str],
    pub fahrenheit: bool,
    pub alarm: bool,
    pub rotate: bool,
    pub zeros: bool,
}

pub fn is_supported(pid: u32) -> bool {
    SUPPORTED.contains(&pid)
}

pub fn capabilities(pid: u32) -> Capabilities {
    let modes: &'static [&'static str] = match pid {
        6 => &["auto", "cpu_temp", "cpu_power"],
        12 => LP,
        19 | 22 | 27 => &["auto", "cpu_freq", "gpu"],
        4352 => &["cpu", "gpu"],
        10 | 13 | 15 | 16 | 17 | 18 | 31 | 41 | 42 | 43 | 44 => &["auto"],
        _ => BASIC,
    };
    let secondary: &'static [&'static str] = match pid {
        12 => LP,
        5 | 7 | 21 => &["gpu_temp", "gpu_usage"],
        _ => &[],
    };
    Capabilities {
        modes,
        secondary,
        fahrenheit: pid != 8,
        alarm: matches!(pid, 1 | 2 | 3 | 4 | 6 | 8),
        rotate: pid == 12,
        zeros: pid == 10,
    }
}

/// The settings for one device, stored per product ID.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub mode: String,
    pub secondary: String,
    pub rotate: String,
    pub fahrenheit: bool,
    pub alarm: bool,
    pub zeros: bool,
    pub update: i64,
    pub gpuid: String,
    /// Start this display whenever the app runs, and keep it running.
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: DEVICE_DEFAULT.into(),
            secondary: DEVICE_DEFAULT.into(),
            rotate: "0".into(),
            fahrenheit: false,
            alarm: false,
            zeros: false,
            update: 1000,
            gpuid: String::new(),
            autostart: false,
        }
    }
}

impl Settings {
    /// Reads a saved profile, falling back to defaults for missing or malformed fields.
    pub fn from_json(value: &Value) -> Self {
        let mut settings = Settings::default();
        let Some(object) = value.as_object() else {
            return settings;
        };
        let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_owned);
        let flag = |key: &str| object.get(key).and_then(Value::as_bool).unwrap_or(false);
        if let Some(mode) = text("mode") {
            settings.mode = mode;
        }
        if let Some(secondary) = text("secondary") {
            settings.secondary = secondary;
        }
        settings.rotate = match object.get("rotate") {
            Some(Value::String(rotate)) => rotate.clone(),
            Some(Value::Number(rotate)) => rotate.to_string(),
            _ => settings.rotate,
        };
        settings.fahrenheit = flag("fahrenheit");
        settings.alarm = flag("alarm");
        settings.zeros = flag("zeros");
        settings.update = match object.get("update") {
            Some(Value::Number(update)) => update.as_i64().unwrap_or(1000),
            Some(Value::String(update)) => update.trim().parse().unwrap_or(1000),
            _ => 1000,
        };
        if let Some(gpuid) = text("gpuid") {
            settings.gpuid = gpuid;
        }
        settings.autostart = flag("autostart");
        settings
    }

    pub fn to_json(&self) -> Value {
        json!({
            "mode": self.mode,
            "secondary": self.secondary,
            "rotate": self.rotate,
            "fahrenheit": self.fahrenheit,
            "alarm": self.alarm,
            "zeros": self.zeros,
            "update": self.update,
            "gpuid": self.gpuid,
            "autostart": self.autostart,
        })
    }
}

/// Builds the controller command line, rejecting anything the device does not support.
pub fn command(binary: &Path, pid: u32, settings: &Settings) -> Result<Vec<String>, String> {
    if !is_supported(pid) {
        return Err("This device is not supported by the controller yet.".into());
    }
    if !UPDATE_RANGE.contains(&settings.update) {
        return Err("Update interval must be between 100 and 2000 ms.".into());
    }
    let cap = capabilities(pid);
    let mut args = vec![
        binary.to_string_lossy().into_owned(),
        "--pid".into(),
        pid.to_string(),
        "--update".into(),
        settings.update.to_string(),
    ];
    for (name, value, allowed, flag) in [
        ("mode", &settings.mode, cap.modes, "--mode"),
        ("secondary", &settings.secondary, cap.secondary, "--secondary"),
    ] {
        if value != DEVICE_DEFAULT {
            if !allowed.contains(&value.as_str()) {
                return Err(format!("Unsupported {name}: {value}"));
            }
            args.extend([flag.to_string(), value.clone()]);
        }
    }
    for (supported, enabled, flag) in [
        (cap.fahrenheit, settings.fahrenheit, "--fahrenheit"),
        (cap.alarm, settings.alarm, "--alarm"),
        (cap.zeros, settings.zeros, "--zeros"),
    ] {
        if supported && enabled {
            args.push(flag.into());
        }
    }
    if cap.rotate && settings.rotate != "0" {
        if !ROTATIONS[1..].contains(&settings.rotate.as_str()) {
            return Err("Invalid rotation.".into());
        }
        args.extend(["--rotate".into(), settings.rotate.clone()]);
    }
    let gpu = settings.gpuid.trim();
    if !gpu.is_empty() {
        if !is_valid_gpu_id(gpu) {
            return Err("GPU must be amd:N, intel:N, or nvidia:N (0–255).".into());
        }
        args.extend(["--gpuid".into(), gpu.into()]);
    }
    Ok(args)
}

fn is_valid_gpu_id(gpu: &str) -> bool {
    let Some((vendor, id)) = gpu.split_once(':') else {
        return false;
    };
    matches!(vendor, "amd" | "intel" | "nvidia")
        && (1..=3).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_digit())
        && id.parse::<u16>().is_ok_and(|n| n <= 255)
}

/// Removes ANSI escape sequences (CSI) from controller output.
pub fn clean(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // Parameter and intermediate bytes, then one final byte in @..~
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            output.push(c);
        }
    }
    output
}

/// Splits a `left | right` row; returns trimmed parts when both are present.
fn split_row(line: &str) -> Option<(&str, &str)> {
    let (left, right) = line.split_once('|')?;
    let (left, right) = (left.trim(), right.trim());
    (!left.is_empty() && !right.is_empty()).then_some((left, right))
}

/// Parses `--list` output into (product ID, name) pairs.
pub fn devices_from_output(text: &str) -> Vec<(u32, String)> {
    clean(text)
        .lines()
        .filter_map(split_row)
        .filter(|(pid, _)| pid.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|(pid, name)| Some((pid.parse().ok()?, name.to_owned())))
        .collect()
}

/// Parses `--gpulist` output into (GPU ID, name) pairs.
pub fn gpus_from_output(text: &str) -> Vec<(String, String)> {
    clean(text)
        .lines()
        .filter_map(split_row)
        .filter(|(id, _)| {
            id.split_once(':').is_some_and(|(vendor, n)| {
                matches!(vendor, "amd" | "intel" | "nvidia") && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
            })
        })
        .map(|(id, name)| (id.to_owned(), name.to_owned()))
        .collect()
}

fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(variable) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback),
    }
}

pub fn config_path() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("deepcool-digital/settings.json")
}

pub fn autostart_path() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("autostart/deepcool-digital.desktop")
}

pub fn activity_log_path() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state").join("deepcool-digital/activity.log")
}

pub fn load_profiles() -> Map<String, Value> {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|data| serde_json::from_str::<Value>(&data).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// Writes the profiles atomically so a crash never leaves a half-written file.
pub fn save_profiles(profiles: &Map<String, Value>) -> Result<(), String> {
    write_json(&config_path(), &Value::Object(profiles.clone()))
        .map_err(|error| format!("Could not save settings: {error}"))
}

/// Writes JSON atomically so a crash never leaves a half-written file.
fn write_json(path: &Path, value: &Value) -> std::io::Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension("tmp");
    let data = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    fs::write(&temporary, data)?;
    fs::rename(&temporary, path)
}

pub fn preferences_path() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("deepcool-digital/preferences.json")
}

/// App-wide behaviour, separate from the per-device profiles.
#[derive(Debug, Clone, PartialEq)]
pub struct Preferences {
    /// Keep controllers running after the window is closed.
    pub run_in_background: bool,
    /// Look for new versions on GitHub while the window is open.
    pub check_for_updates: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            run_in_background: true,
            check_for_updates: true,
        }
    }
}

pub fn load_preferences() -> Preferences {
    let saved = fs::read_to_string(preferences_path())
        .ok()
        .and_then(|data| serde_json::from_str::<Value>(&data).ok());
    let defaults = Preferences::default();
    let flag = |key: &str, default: bool| {
        saved
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    };
    Preferences {
        run_in_background: flag("run_in_background", defaults.run_in_background),
        check_for_updates: flag("check_for_updates", defaults.check_for_updates),
    }
}

pub fn save_preferences(preferences: &Preferences) -> Result<(), String> {
    write_json(
        &preferences_path(),
        &json!({
            "run_in_background": preferences.run_in_background,
            "check_for_updates": preferences.check_for_updates,
        }),
    )
    .map_err(|error| format!("Could not save preferences: {error}"))
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

pub fn is_executable(path: &Path) -> bool {
    path.is_file()
        && CString::new(path.as_os_str().as_bytes())
            .is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0)
}

/// Finds the device controller: an explicit override, next to this program,
/// in the packaged library folder, or on `PATH`.
pub fn backend_path() -> Result<PathBuf, String> {
    const NAME: &str = "deepcool-digital-linux";
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("DEEPCOOL_BACKEND").filter(|p| !p.is_empty()) {
        candidates.push(PathBuf::from(path));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(NAME));
        candidates.push(dir.join("../lib/deepcool-digital").join(NAME));
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(NAME)));
    }
    candidates
        .into_iter()
        .find(|path| is_executable(path))
        .ok_or_else(|| "Device controller is missing. Install the complete DeepCool Digital package.".into())
}

fn describe(status: ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => code.to_string(),
        (None, Some(signal)) => format!("-{signal}"),
        _ => "unknown".into(),
    }
}

/// Runs a short command and returns its combined stdout, or an error after `timeout`.
pub fn run_with_timeout(args: &[String], timeout: Duration) -> Result<(bool, String, String), String> {
    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not run the device controller: {error}"))?;
    let read = |stream: Option<Box<dyn std::io::Read + Send>>| {
        thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut stream) = stream {
                let mut bytes = Vec::new();
                let _ = stream.read_to_end(&mut bytes);
                text = String::from_utf8_lossy(&bytes).into_owned();
            }
            text
        })
    };
    let stdout = read(child.stdout.take().map(|s| Box::new(s) as _));
    let stderr = read(child.stderr.take().map(|s| Box::new(s) as _));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "The device controller did not respond within {} seconds.",
                    timeout.as_secs()
                ));
            }
        }
    };
    Ok((
        status.success(),
        stdout.join().unwrap_or_default(),
        stderr.join().unwrap_or_default(),
    ))
}

struct Process {
    child: Arc<Mutex<Child>>,
    stop_deadline: Option<Instant>,
    args: Vec<String>,
}

/// One controller child per product ID. Never invokes a shell.
pub struct Controller {
    processes: HashMap<u32, Process>,
    output: Sender<(u32, String)>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl Controller {
    /// `output` receives every controller line; `notify` is called after each one.
    pub fn new(output: Sender<(u32, String)>, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        Controller {
            processes: HashMap::new(),
            output,
            notify,
        }
    }

    pub fn is_running(&self, pid: u32) -> bool {
        self.processes
            .get(&pid)
            .is_some_and(|process| matches!(process.child.lock().unwrap().try_wait(), Ok(None)))
    }

    /// The command line a controller was last started with.
    pub fn args(&self, pid: u32) -> Option<Vec<String>> {
        self.processes.get(&pid).map(|process| process.args.clone())
    }

    /// Product IDs whose controller is currently running.
    pub fn running(&self) -> Vec<u32> {
        self.processes
            .keys()
            .copied()
            .filter(|pid| self.is_running(*pid))
            .collect()
    }

    pub fn start(&mut self, pid: u32, args: &[String]) -> Result<(), String> {
        if self.is_running(pid) {
            return Err("Stop this device before applying a new configuration.".into());
        }
        let (reader, writer) = std::io::pipe().map_err(|error| error.to_string())?;
        let mut command = Command::new(&args[0]);
        command
            .args(&args[1..])
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(writer.try_clone().map_err(|error| error.to_string())?)
            .stderr(writer);
        // Stop the controller if this app dies without a chance to clean up.
        unsafe {
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let child = command
            .spawn()
            .map_err(|error| format!("Could not start the device controller: {error}"))?;
        drop(command); // Close our copies of the pipe's write end so EOF arrives.
        let child = Arc::new(Mutex::new(child));
        let (shared, output, notify) = (child.clone(), self.output.clone(), self.notify.clone());
        thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut line = Vec::new();
            while reader.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
                let text = clean(String::from_utf8_lossy(&line).trim_end());
                let _ = output.send((pid, text));
                notify();
                line.clear();
            }
            let status = loop {
                match shared.lock().unwrap().try_wait() {
                    Ok(Some(status)) => break describe(status),
                    Ok(None) => {}
                    Err(error) => break error.to_string(),
                }
                thread::sleep(Duration::from_millis(20));
            };
            let _ = output.send((pid, format!("Controller exited with status {status}.")));
            notify();
        });
        self.processes.insert(
            pid,
            Process {
                child,
                stop_deadline: None,
                args: args.to_vec(),
            },
        );
        Ok(())
    }

    /// Asks the controller to exit; it is killed if still running after a grace period.
    pub fn stop(&mut self, pid: u32) {
        if !self.is_running(pid) {
            return;
        }
        if let Some(process) = self.processes.get_mut(&pid) {
            let id = process.child.lock().unwrap().id() as libc::pid_t;
            unsafe { libc::kill(id, libc::SIGTERM) };
            process.stop_deadline.get_or_insert(Instant::now() + STOP_GRACE);
        }
    }

    /// Kills controllers that ignored a stop request. Call this periodically.
    pub fn enforce_stops(&mut self) {
        let now = Instant::now();
        for process in self.processes.values() {
            if process.stop_deadline.is_some_and(|deadline| now >= deadline) {
                let mut child = process.child.lock().unwrap();
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                }
            }
        }
    }

    /// Stops every controller and waits for them to exit.
    pub fn close(&mut self) {
        let pids: Vec<u32> = self.processes.keys().copied().collect();
        for pid in &pids {
            self.stop(*pid);
        }
        while pids.iter().any(|pid| self.is_running(*pid)) {
            self.enforce_stops();
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.close();
    }
}

/// Rotating diagnostic log that outlives the window. Never fails app startup.
pub struct ActivityLog {
    path: Option<PathBuf>,
}

const LOG_MAX_BYTES: u64 = 262_144;
const LOG_BACKUPS: u32 = 2;

impl ActivityLog {
    pub fn open() -> Self {
        let path = activity_log_path();
        let usable = path.parent().is_some_and(|dir| fs::create_dir_all(dir).is_ok());
        ActivityLog {
            path: usable.then_some(path),
        }
    }

    pub fn write(&self, message: &str) {
        let line = format!("{} {message}\n", timestamp());
        let Some(path) = &self.path else {
            eprint!("{line}");
            return;
        };
        if fs::metadata(path).is_ok_and(|meta| meta.len() + line.len() as u64 > LOG_MAX_BYTES) {
            for index in (1..LOG_BACKUPS).rev() {
                let _ = fs::rename(backup(path, index), backup(path, index + 1));
            }
            let _ = fs::rename(path, backup(path, 1));
        }
        let written = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if written.is_err() {
            eprint!("{line}");
        }
    }
}

fn backup(path: &Path, index: u32) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

/// Local time as `YYYY-MM-DD HH:MM:SS,mmm`.
fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&seconds, &mut tm) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02},{:03}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        now.subsec_millis()
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Serialises tests that change process-wide environment variables.
    pub static ENV: Mutex<()> = Mutex::new(());

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deepcool-gui-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn discovery_ignores_headers_and_ansi() {
        assert_eq!(
            devices_from_output("Device list [PID | Name]\n-----\n\x1b[32m4 | AK500S\x1b[0m\n4352 | CH510\n"),
            vec![(4, "AK500S".into()), (4352, "CH510".into())]
        );
        assert_eq!(
            gpus_from_output("GPU list [ID | Name (PCI Address)]\n-----\nnvidia:1 | RTX (0000:01:00.0)\nfoo:1 | x\n"),
            vec![("nvidia:1".into(), "RTX (0000:01:00.0)".into())]
        );
    }

    #[test]
    fn capability_mapping() {
        assert!(!capabilities(8).fahrenheit);
        assert!(capabilities(12).rotate);
        assert_eq!(capabilities(4352).modes, ["cpu", "gpu"]);
        assert_eq!(capabilities(19).modes, ["auto", "cpu_freq", "gpu"]);
        assert_eq!(capabilities(17).modes, ["auto"]);
        assert!(capabilities(10).zeros && !capabilities(10).rotate);
    }

    #[test]
    fn supported_arguments() {
        let s = Settings {
            mode: "cpu_temp".into(),
            secondary: "gpu_temp".into(),
            rotate: "90".into(),
            gpuid: "nvidia:1".into(),
            fahrenheit: true,
            alarm: true,
            ..settings()
        };
        let args = command(Path::new("/backend"), 12, &s).unwrap();
        assert!(args.contains(&"--rotate".into()));
        assert!(args.contains(&"--fahrenheit".into()));
        assert!(!args.contains(&"--alarm".into()));
        assert_eq!(args[args.len() - 2..], ["--gpuid", "nvidia:1"]);
    }

    #[test]
    fn rejects_unsupported_mode() {
        let s = Settings {
            mode: "cpu_usage".into(),
            ..settings()
        };
        assert!(command(Path::new("/backend"), 10, &s).is_err());
        assert!(command(Path::new("/backend"), 999, &settings()).is_err());
    }

    #[test]
    fn rejects_invalid_values() {
        for s in [
            Settings {
                update: 99,
                ..settings()
            },
            Settings {
                update: 2001,
                ..settings()
            },
            Settings {
                gpuid: "nvidia:256".into(),
                ..settings()
            },
            Settings {
                gpuid: "nvidia:1; id".into(),
                ..settings()
            },
            Settings {
                gpuid: "nvidia:1000".into(),
                ..settings()
            },
            Settings {
                rotate: "45".into(),
                ..settings()
            },
        ] {
            assert!(command(Path::new("/backend"), 12, &s).is_err(), "{s:?}");
        }
    }

    #[test]
    fn profile_roundtrip_and_corruption() {
        let _guard = ENV.lock().unwrap();
        let dir = temp_dir("profiles");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        assert!(load_profiles().is_empty());
        let mut profiles = Map::new();
        profiles.insert(
            "12".into(),
            Settings {
                mode: "gpu_temp".into(),
                ..settings()
            }
            .to_json(),
        );
        save_profiles(&profiles).unwrap();
        assert_eq!(Settings::from_json(&load_profiles()["12"]).mode, "gpu_temp");
        fs::write(dir.join("deepcool-digital/settings.json"), "invalid").unwrap();
        assert!(load_profiles().is_empty());
        // Profiles written by the earlier Python app use the same format.
        let legacy = json!({"mode": "cpu_temp", "rotate": "180", "update": "500", "alarm": true, "gpuid": "amd:1"});
        let s = Settings::from_json(&legacy);
        assert_eq!(
            (s.mode.as_str(), s.rotate.as_str(), s.update, s.alarm),
            ("cpu_temp", "180", 500, true)
        );
        assert!(!s.autostart);
        assert!(load_preferences().run_in_background, "runs in the background by default");
        assert!(load_preferences().check_for_updates, "checks for updates by default");
        save_preferences(&Preferences {
            run_in_background: false,
            check_for_updates: false,
        })
        .unwrap();
        assert!(!load_preferences().run_in_background);
        assert!(!load_preferences().check_for_updates);
        std::env::remove_var("XDG_CONFIG_HOME");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn persistent_activity_log() {
        let _guard = ENV.lock().unwrap();
        let dir = temp_dir("log");
        std::env::set_var("XDG_STATE_HOME", &dir);
        let log = ActivityLog::open();
        log.write("USB access denied");
        let path = dir.join("deepcool-digital/activity.log");
        assert!(fs::read_to_string(&path).unwrap().contains("USB access denied"));
        let big = "x".repeat(100_000);
        for _ in 0..8 {
            log.write(&big);
        }
        assert!(fs::metadata(&path).unwrap().len() <= LOG_MAX_BYTES);
        assert!(backup(&path, 1).exists() && backup(&path, 2).exists() && !backup(&path, 3).exists());
        std::env::remove_var("XDG_STATE_HOME");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn autostart_entry_escapes_path() {
        let entry = autostart_entry("/opt/My $App/run%\"").unwrap();
        assert!(entry.contains("Exec=\"/opt/My \\$App/run%%\\\"\" --background\n"));
        assert!(autostart_entry("/a\nb").is_err());
    }

    fn script(dir: &Path, body: &str) -> String {
        let path = dir.join("backend");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn process_lifecycle() {
        let dir = temp_dir("lifecycle");
        let backend = script(&dir, "echo ready; echo oops >&2; exec sleep 30");
        let (tx, rx) = mpsc::channel();
        let mut controller = Controller::new(tx, Arc::new(|| {}));
        controller.start(12, std::slice::from_ref(&backend)).unwrap();
        assert!(controller.is_running(12));
        assert!(controller.start(12, &[backend]).is_err());
        let next = || rx.recv_timeout(Duration::from_secs(5)).expect("controller output").1;
        assert_eq!([next(), next()], ["ready", "oops"]);
        let started = Instant::now();
        controller.close();
        assert!(!controller.is_running(12));
        assert!(started.elapsed() < STOP_GRACE, "SIGTERM should stop it promptly");
        assert_eq!(next(), "Controller exited with status -15.");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stubborn_process_is_killed() {
        let dir = temp_dir("stubborn");
        let backend = script(&dir, "trap '' TERM; echo ready; while :; do sleep 0.05; done");
        let (tx, rx) = mpsc::channel();
        let mut controller = Controller::new(tx, Arc::new(|| {}));
        controller.start(5, &[backend]).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().1, "ready");
        controller.close();
        assert!(!controller.is_running(5));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn timeout_runner() {
        let dir = temp_dir("timeout");
        let backend = script(&dir, "echo '10 | LD-Series'; exit 0");
        let (ok, out, _) = run_with_timeout(&[backend], Duration::from_secs(5)).unwrap();
        assert!(ok && out.contains("LD-Series"));
        let slow = script(&dir, "sleep 30");
        assert!(run_with_timeout(&[slow], Duration::from_millis(200)).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
