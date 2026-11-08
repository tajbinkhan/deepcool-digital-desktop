//! State and actions of the control panel, independent of drawing.

use crate::core::{
    self, capabilities, is_supported, ActivityLog, Controller, Preferences, Settings, DEVICE_DEFAULT, ROTATIONS,
    UPDATE_RANGE,
};
use serde_json::{Map, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_ACTIVITY_LINES: usize = 500;
const TRIMMED_ACTIVITY_LINES: usize = 400;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to wait before retrying an automatic display that stopped or is not connected.
const RETRY_INTERVAL: Duration = Duration::from_secs(15);

/// Result of asking the controller which devices and GPUs are present.
#[derive(Debug, Default)]
pub struct Discovery {
    pub devices: Vec<(u32, String)>,
    pub gpus: Vec<(String, String)>,
    pub error: String,
}

/// Runs `--list` and `--gpulist`. Blocking; call it off the UI thread.
pub fn discover() -> Discovery {
    let backend = match core::backend_path() {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(error) => {
            return Discovery {
                error,
                ..Default::default()
            }
        }
    };
    let devices = match core::run_with_timeout(&[backend.clone(), "--list".into()], DISCOVERY_TIMEOUT) {
        Ok((true, stdout, _)) => core::devices_from_output(&stdout),
        Ok((false, stdout, stderr)) => {
            let message = if stderr.trim().is_empty() { stdout } else { stderr };
            return Discovery {
                error: core::clean(message.trim()),
                ..Default::default()
            };
        }
        Err(error) => {
            return Discovery {
                error,
                ..Default::default()
            }
        }
    };
    let gpus = core::run_with_timeout(&[backend, "--gpulist".into()], DISCOVERY_TIMEOUT)
        .map(|(_, stdout, _)| core::gpus_from_output(&stdout))
        .unwrap_or_default();
    Discovery {
        devices,
        gpus,
        error: String::new(),
    }
}

pub fn label(value: &str) -> String {
    match value {
        "auto" => "Automatic",
        "cpu_temp" => "CPU temperature",
        "cpu_usage" => "CPU usage",
        "cpu_power" => "CPU power",
        "cpu_freq" => "CPU frequency",
        "gpu_temp" => "GPU temperature",
        "gpu_usage" => "GPU usage",
        "gpu_power" => "GPU power",
        "cpu" => "CPU",
        "gpu" => "GPU",
        other => other,
    }
    .into()
}

pub struct Panel {
    pub devices: Vec<(u32, String)>,
    pub selected: usize,
    /// (GPU ID, display name); the ID "" means automatic.
    pub gpus: Vec<(String, String)>,
    pub settings: Settings,
    pub status: String,
    pub scanning: bool,
    pub activity: VecDeque<String>,
    /// Whether the app starts in the background when the user signs in.
    pub sign_in: bool,
    pub preferences: Preferences,
    pub controller: Controller,
    profiles: Map<String, Value>,
    /// Automatic devices the user stopped by hand; not restarted until started again.
    paused: HashSet<u32>,
    last_attempt: HashMap<u32, Instant>,
    last_scan: Option<Instant>,
    quiet_scan: bool,
    /// How long to wait before retrying an automatic display (shortened in tests).
    pub retry_interval: Duration,
    log_file: ActivityLog,
    output: Receiver<(u32, String)>,
    discovery: Option<Receiver<Discovery>>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl Panel {
    /// `notify` is called from background threads when new data arrives.
    pub fn new(notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, output) = mpsc::channel();
        let log_file = ActivityLog::open();
        log_file.write(&format!("DeepCool Digital GUI {} opened", env!("CARGO_PKG_VERSION")));
        let mut panel = Panel {
            devices: Vec::new(),
            selected: 0,
            gpus: vec![(String::new(), "Automatic".into())],
            settings: Settings::default(),
            status: "Discovering devices…".into(),
            scanning: false,
            activity: VecDeque::new(),
            sign_in: core::autostart_path().exists(),
            preferences: core::load_preferences(),
            controller: Controller::new(sender, notify.clone()),
            profiles: core::load_profiles(),
            paused: HashSet::new(),
            last_attempt: HashMap::new(),
            last_scan: None,
            quiet_scan: false,
            retry_interval: RETRY_INTERVAL,
            log_file,
            output,
            discovery: None,
            notify,
        };
        // Refresh an existing sign-in entry so it points at this program and starts in the background.
        if panel.sign_in {
            panel.set_sign_in(true);
        }
        panel
    }

    pub fn pid(&self) -> Option<u32> {
        self.devices.get(self.selected).map(|(pid, _)| *pid)
    }

    pub fn is_running(&self) -> bool {
        self.pid().is_some_and(|pid| self.controller.is_running(pid))
    }

    pub fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        self.log_file.write(&line);
        self.activity.extend(line.lines().map(str::to_owned));
        if self.activity.len() > MAX_ACTIVITY_LINES {
            let excess = self.activity.len() - TRIMMED_ACTIVITY_LINES;
            self.activity.drain(..excess);
        }
    }

    /// Starts device discovery on a background thread.
    pub fn scan(&mut self) {
        self.start_scan(false);
    }

    /// A quiet scan looks for devices that should start automatically without logging.
    fn start_scan(&mut self, quiet: bool) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        self.quiet_scan = quiet;
        self.last_scan = Some(Instant::now());
        if !quiet {
            self.status = "Discovering devices…".into();
        }
        let (sender, receiver) = mpsc::channel();
        let notify = self.notify.clone();
        thread::spawn(move || {
            let _ = sender.send(discover());
            notify();
        });
        self.discovery = Some(receiver);
    }

    /// Applies finished background work and keeps automatic displays running.
    /// Call it regularly, whether or not the window is open.
    pub fn poll(&mut self) {
        while let Ok((pid, line)) = self.output.try_recv() {
            self.log(format!("[{pid}] {line}"));
        }
        if let Some(found) = self.discovery.as_ref().and_then(|receiver| receiver.try_recv().ok()) {
            self.discovery = None;
            self.discovered(found);
        }
        self.controller.enforce_stops();
        self.keep_alive();
        self.update_status();
    }

    pub fn discovered(&mut self, found: Discovery) {
        let quiet = std::mem::take(&mut self.quiet_scan);
        self.scanning = false;
        self.gpus = std::iter::once((String::new(), "Automatic".into()))
            .chain(
                found
                    .gpus
                    .into_iter()
                    .map(|(id, name)| (id.clone(), format!("{name} · {id}"))),
            )
            .collect();
        let previous = self.pid();
        // The controller addresses devices by product ID, so duplicates cannot be told apart.
        let mut devices: Vec<(u32, String)> = Vec::new();
        for (pid, name) in found.devices {
            match devices.iter_mut().find(|(known, _)| *known == pid) {
                Some(entry) => entry.1 = name,
                None => devices.push((pid, name)),
            }
        }
        self.devices = devices;
        self.selected = self
            .devices
            .iter()
            .position(|(pid, _)| Some(*pid) == previous)
            .unwrap_or(0);
        // Keep unsaved edits when a rescan finds the same device selected.
        if self.pid() != previous || !quiet {
            self.device_changed();
        }
        if quiet {
            return;
        }
        if !found.error.is_empty() {
            self.log(found.error);
        }
        if self.devices.is_empty() {
            self.status = "No device detected. Connect a supported DeepCool USB device and refresh.".into();
            self.log(
                "No device found. Installed packages include desktop USB access rules; reconnect the USB device \
                 after installation. Portable builds require those rules to be installed separately.",
            );
        }
    }

    pub fn mode_options(&self) -> Vec<&'static str> {
        let modes = self.pid().map(|pid| capabilities(pid).modes).unwrap_or_default();
        std::iter::once(DEVICE_DEFAULT).chain(modes.iter().copied()).collect()
    }

    pub fn secondary_options(&self) -> Vec<&'static str> {
        let modes = self.pid().map(|pid| capabilities(pid).secondary).unwrap_or_default();
        std::iter::once(DEVICE_DEFAULT).chain(modes.iter().copied()).collect()
    }

    fn saved(&self, pid: u32) -> Settings {
        Settings::from_json(self.profiles.get(&pid.to_string()).unwrap_or(&Value::Null))
    }

    /// Loads the saved profile for the selected device, keeping only supported values.
    pub fn device_changed(&mut self) {
        let Some(pid) = self.pid() else {
            self.settings = Settings::default();
            return;
        };
        let cap = capabilities(pid);
        let saved = self.saved(pid);
        let pick = |value: String, options: Vec<&str>| {
            if options.contains(&value.as_str()) {
                value
            } else {
                DEVICE_DEFAULT.to_owned()
            }
        };
        self.settings = Settings {
            mode: pick(saved.mode, self.mode_options()),
            secondary: pick(saved.secondary, self.secondary_options()),
            rotate: if cap.rotate && ROTATIONS.contains(&saved.rotate.as_str()) {
                saved.rotate
            } else {
                "0".into()
            },
            fahrenheit: saved.fahrenheit && cap.fahrenheit,
            alarm: saved.alarm && cap.alarm,
            zeros: saved.zeros && cap.zeros,
            update: saved.update.clamp(*UPDATE_RANGE.start(), *UPDATE_RANGE.end()),
            gpuid: if self.gpus.iter().any(|(id, _)| *id == saved.gpuid) {
                saved.gpuid
            } else {
                String::new()
            },
            autostart: saved.autostart,
        };
        self.update_status();
    }

    pub fn update_status(&mut self) {
        if self.scanning && !self.quiet_scan {
            return;
        }
        if let Some(pid) = self.pid() {
            self.status = if self.controller.is_running(pid) {
                "Display running"
            } else if !is_supported(pid) {
                "Device detected, but not supported by this controller"
            } else if self.saved(pid).autostart && self.paused.contains(&pid) {
                "Stopped. It will start automatically again after you select Start display."
            } else {
                "Ready to start"
            }
            .into();
        }
    }

    pub fn can_start(&self) -> bool {
        self.pid()
            .is_some_and(|pid| is_supported(pid) && !self.controller.is_running(pid))
    }

    /// Product IDs whose saved profile asks to start automatically.
    pub fn automatic_devices(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self
            .profiles
            .keys()
            .filter_map(|key| key.parse().ok())
            .filter(|pid| is_supported(*pid) && self.saved(*pid).autostart)
            .collect();
        pids.sort();
        pids
    }

    /// True while there is something for the app to do without its window.
    pub fn has_background_work(&self) -> bool {
        !self.controller.running().is_empty() || self.automatic_devices().iter().any(|pid| !self.paused.contains(pid))
    }

    /// Starts automatic displays that are connected but not running, retrying
    /// after a pause if a display stops or its device is not connected yet.
    fn keep_alive(&mut self) {
        let now = Instant::now();
        let mut missing = false;
        for pid in self.automatic_devices() {
            if self.paused.contains(&pid) || self.controller.is_running(pid) {
                continue;
            }
            if !self.devices.iter().any(|(known, _)| *known == pid) {
                missing = true;
                continue;
            }
            if self
                .last_attempt
                .get(&pid)
                .is_some_and(|last| now.duration_since(*last) < self.retry_interval)
            {
                continue;
            }
            self.last_attempt.insert(pid, now);
            let settings = self.saved(pid);
            self.log(format!("Starting display {pid} automatically with its saved settings."));
            if let Err(error) = self.launch(pid, &settings) {
                self.log(format!("Could not start display {pid} automatically: {error}"));
            }
        }
        let due = self
            .last_scan
            .is_none_or(|last| now.duration_since(last) >= self.retry_interval);
        if missing && due && !self.scanning {
            self.start_scan(true);
        }
    }

    fn launch(&mut self, pid: u32, settings: &Settings) -> Result<(), String> {
        let args = core::command(&core::backend_path()?, pid, settings)?;
        self.controller.start(pid, &args)?;
        self.log_file
            .write(&format!("Device {pid} requested settings: {}", settings.to_json()));
        Ok(())
    }

    pub fn start(&mut self) {
        let result = (|| {
            let pid = self.pid().ok_or("Connect a device before starting the display.")?;
            let settings = self.settings.clone();
            self.launch(pid, &settings)?;
            self.paused.remove(&pid);
            Ok::<_, String>(())
        })();
        match result {
            Ok(()) => self.log(
                "Starting display. If USB access is denied, reconnect the device after installing the package. \
                 Do not run the desktop app as root.",
            ),
            Err(error) => self.log(error),
        }
        self.update_status();
    }

    pub fn stop(&mut self) {
        if let Some(pid) = self.pid() {
            self.controller.stop(pid);
            self.paused.insert(pid);
        }
        self.update_status();
    }

    pub fn save(&mut self) {
        let result = (|| {
            let pid = self.pid().ok_or("Connect a device before saving settings.")?;
            core::command(&core::backend_path()?, pid, &self.settings)?;
            self.profiles.insert(pid.to_string(), self.settings.to_json());
            core::save_profiles(&self.profiles)
        })();
        match result {
            Ok(()) => self.log("Settings saved for this device. Select Start display to apply them."),
            Err(error) => self.log(error),
        }
    }

    /// Turns automatic start on or off for the selected device. Saved at once; a device
    /// without saved settings gets the ones currently shown.
    pub fn set_device_autostart(&mut self, enabled: bool) {
        let result = (|| {
            let pid = self.pid().ok_or("Connect a device first.")?;
            let mut profile = match self.profiles.get(&pid.to_string()) {
                Some(_) => self.saved(pid),
                None => {
                    core::command(&core::backend_path()?, pid, &self.settings)?;
                    self.settings.clone()
                }
            };
            profile.autostart = enabled;
            self.profiles.insert(pid.to_string(), profile.to_json());
            core::save_profiles(&self.profiles)?;
            self.paused.remove(&pid);
            self.last_attempt.remove(&pid);
            Ok::<_, String>(())
        })();
        match result {
            Ok(()) if enabled => self.log(
                "This display now starts automatically with its saved settings, and restarts if it stops unexpectedly.",
            ),
            Ok(()) => self.log("This display no longer starts automatically."),
            Err(error) => self.log(error),
        }
        self.settings.autostart = self.pid().is_some_and(|pid| self.saved(pid).autostart);
        self.update_status();
    }

    pub fn set_run_in_background(&mut self, enabled: bool) {
        let preferences = core::Preferences {
            run_in_background: enabled,
        };
        match core::save_preferences(&preferences) {
            Ok(()) => self.preferences = preferences,
            Err(error) => self.log(error),
        }
    }

    /// Adds or removes the sign-in entry that starts the app in the background.
    pub fn set_sign_in(&mut self, enabled: bool) {
        let path = core::autostart_path();
        let result = (|| {
            if enabled {
                let executable = match std::env::var("APPIMAGE") {
                    Ok(appimage) if !appimage.is_empty() => appimage,
                    _ => std::env::current_exe()
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .into_owned(),
                };
                let entry = core::autostart_entry(&executable)?;
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                std::fs::write(&path, entry).map_err(|e| e.to_string())
            } else {
                match std::fs::remove_file(&path) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
                    _ => Ok(()),
                }
            }
        })();
        if let Err(error) = result {
            self.log(format!("Could not change the sign-in setting: {error}"));
        }
        self.sign_in = path.exists();
    }

    pub fn close(&mut self) {
        self.controller.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tests::{temp_dir, ENV};
    use std::{fs, os::unix::fs::PermissionsExt, time::Instant};

    /// Mirrors the old GUI integration test: fake controller, two devices.
    #[test]
    fn discovery_capabilities_save_start_stop() {
        let _guard = ENV.lock().unwrap();
        let root = temp_dir("panel");
        let backend = root.join("backend");
        fs::write(
            &backend,
            "#!/bin/sh\ncase \"$*\" in\n *--gpulist*) printf 'GPU list\\n-----\\nnvidia:1 | RTX (0000:01:00.0)\\n';;\n \
             *--list*) printf 'Device list [PID | Name]\\n-----\\n10 | LD-Series\\n12 | LP-Series\\n';;\n \
             *) echo \"Display streaming $*\"; exec sleep 30;;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&backend, fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("DEEPCOOL_BACKEND", &backend);
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_STATE_HOME", root.join("state"));

        let mut panel = Panel::new(Arc::new(|| {}));
        panel.scan();
        let deadline = Instant::now() + Duration::from_secs(10);
        while panel.scanning && Instant::now() < deadline {
            panel.poll();
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(panel.devices.len(), 2);
        assert_eq!(panel.gpus[1].0, "nvidia:1");
        assert_eq!(panel.pid(), Some(10));
        assert_eq!(panel.mode_options(), [DEVICE_DEFAULT, "auto"]);
        assert!(!capabilities(10).rotate && capabilities(10).zeros);

        panel.selected = 1;
        panel.device_changed();
        assert_eq!(panel.pid(), Some(12));
        assert_eq!(panel.secondary_options().len(), 7);
        panel.settings.mode = "cpu_power".into();
        panel.settings.rotate = "90".into();
        panel.settings.gpuid = "nvidia:1".into();
        panel.save();
        assert!(root.join("config/deepcool-digital/settings.json").exists());

        // Switching away and back restores the saved profile.
        panel.selected = 0;
        panel.device_changed();
        assert_eq!(panel.settings.mode, DEVICE_DEFAULT);
        panel.selected = 1;
        panel.device_changed();
        assert_eq!(
            (panel.settings.mode.as_str(), panel.settings.rotate.as_str()),
            ("cpu_power", "90")
        );

        assert!(panel.can_start());
        panel.start();
        assert!(panel.is_running());
        assert!(!panel.can_start());
        assert_eq!(panel.status, "Display running");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !panel.activity.iter().any(|l| l.starts_with("[12] Display streaming")) && Instant::now() < deadline {
            panel.poll();
            thread::sleep(Duration::from_millis(10));
        }
        panel.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while panel.is_running() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!panel.is_running());
        thread::sleep(Duration::from_millis(100));
        panel.poll();
        assert_eq!(panel.status, "Ready to start");
        let activity: Vec<&String> = panel.activity.iter().collect();
        assert!(activity
            .iter()
            .any(|l| l.contains("[12] Display streaming --pid 12") && l.contains("--rotate 90")));
        assert!(activity.iter().any(|l| l.contains("[12] Controller exited")));

        panel.log((0..550).map(|i| i.to_string()).collect::<Vec<_>>().join("\n"));
        assert!(panel.activity.len() <= MAX_ACTIVITY_LINES);
        let log = fs::read_to_string(root.join("state/deepcool-digital/activity.log")).unwrap();
        assert!(log.contains("requested settings"));

        panel.set_sign_in(true);
        assert!(panel.sign_in);
        assert!(fs::read_to_string(root.join("config/autostart/deepcool-digital.desktop"))
            .unwrap()
            .contains("--background"));
        panel.set_sign_in(false);
        assert!(!panel.sign_in);

        panel.close();
        for variable in ["DEEPCOOL_BACKEND", "XDG_CONFIG_HOME", "XDG_STATE_HOME"] {
            std::env::remove_var(variable);
        }
        let _ = fs::remove_dir_all(root);
    }

    /// Needs a connected DeepCool device and a release build of the controller:
    /// `cargo build --release && cargo test -p deepcool-digital-gui -- --ignored`
    #[test]
    #[ignore]
    fn real_device_starts_and_stops() {
        let _guard = ENV.lock().unwrap();
        let backend = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/release/deepcool-digital-linux");
        std::env::set_var("DEEPCOOL_BACKEND", backend);
        let root = temp_dir("hardware");
        std::env::set_var("XDG_STATE_HOME", &root);
        let mut panel = Panel::new(Arc::new(|| {}));
        panel.discovered(discover());
        let pid = panel.pid().expect("a connected DeepCool device");
        panel.start();
        thread::sleep(Duration::from_secs(4));
        panel.poll();
        let text: Vec<String> = panel.activity.iter().cloned().collect();
        assert!(panel.is_running(), "controller exited early:\n{}", text.join("\n"));
        assert!(
            text.iter().any(|l| l.starts_with(&format!("[{pid}] Device found:"))),
            "{text:#?}"
        );
        assert!(!text.iter().any(|l| l.contains("Error!")), "{text:#?}");
        panel.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while panel.is_running() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!panel.is_running());
        for variable in ["DEEPCOOL_BACKEND", "XDG_STATE_HOME"] {
            std::env::remove_var(variable);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn automatic_display_starts_restarts_and_pauses() {
        let _guard = ENV.lock().unwrap();
        let root = temp_dir("automatic");
        let backend = root.join("backend");
        // Lists one device; each display run prints a line and then exits after a moment,
        // like a controller that loses its device.
        fs::write(
            &backend,
            "#!/bin/sh\ncase \"$*\" in\n *--gpulist*) ;;\n *--list*) echo '10 | LD-Series';;\n \
             *) echo running; exec sleep 0.3;;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&backend, fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("DEEPCOOL_BACKEND", &backend);
        std::env::set_var("XDG_CONFIG_HOME", root.join("config"));
        std::env::set_var("XDG_STATE_HOME", root.join("state"));

        let wait = |panel: &mut Panel, done: &dyn Fn(&Panel) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done(panel) && Instant::now() < deadline {
                panel.poll();
                thread::sleep(Duration::from_millis(10));
            }
            assert!(done(panel), "timed out; activity: {:#?}", panel.activity);
        };
        let starts = |panel: &Panel| {
            panel
                .activity
                .iter()
                .filter(|l| l.contains("automatically with its saved"))
                .count()
        };

        let mut panel = Panel::new(Arc::new(|| {}));
        panel.retry_interval = Duration::from_millis(200);
        panel.scan();
        wait(&mut panel, &|p| !p.scanning);
        assert!(!panel.has_background_work(), "nothing is automatic yet");
        panel.set_device_autostart(true);
        assert!(panel.settings.autostart);
        assert_eq!(panel.automatic_devices(), [10]);
        assert!(panel.has_background_work());

        // Starts on its own, and starts again after it stops unexpectedly.
        wait(&mut panel, &|p| starts(p) >= 2);
        // Stopping by hand pauses automatic restarts.
        wait(&mut panel, &|p| p.is_running());
        panel.stop();
        wait(&mut panel, &|p| !p.is_running());
        let paused_at = starts(&panel);
        let until = Instant::now() + Duration::from_millis(600);
        while Instant::now() < until {
            panel.poll();
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(starts(&panel), paused_at);
        assert!(panel.status.starts_with("Stopped."));
        assert!(!panel.has_background_work());

        // A new app session (for example at sign-in) starts it again from the saved profile.
        panel.close();
        let mut next = Panel::new(Arc::new(|| {}));
        next.scan();
        wait(&mut next, &|p| starts(p) >= 1);
        next.set_device_autostart(false);
        assert!(next.automatic_devices().is_empty());
        next.close();

        // The background preference is saved.
        next.set_run_in_background(false);
        assert!(!core::load_preferences().run_in_background);
        for variable in ["DEEPCOOL_BACKEND", "XDG_CONFIG_HOME", "XDG_STATE_HOME"] {
            std::env::remove_var(variable);
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_devices_are_reported() {
        let _guard = ENV.lock().unwrap();
        let root = temp_dir("empty");
        std::env::set_var("XDG_STATE_HOME", &root);
        let mut panel = Panel::new(Arc::new(|| {}));
        panel.discovered(Discovery {
            error: "USB access denied".into(),
            ..Default::default()
        });
        assert!(panel.status.starts_with("No device detected"));
        assert!(panel.activity.iter().any(|l| l == "USB access denied"));
        assert!(!panel.can_start());
        std::env::remove_var("XDG_STATE_HOME");
        let _ = fs::remove_dir_all(root);
    }
}
