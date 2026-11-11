//! Runs device displays as threads inside the app (no controller processes).

use crate::config::{self, Profile};
use deepcool_core::{
    catalog, devices, monitor::gpu, runner, telemetry::Telemetry, vendor_for, Mode, Running,
};
use hidapi::HidApi;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// How long to wait before restarting an automatic display that stopped.
const RETRY_AFTER: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesView {
    pub modes: Vec<&'static str>,
    pub secondary: Vec<&'static str>,
    pub fahrenheit: bool,
    pub alarm: bool,
    pub rotate: bool,
    pub lead_zeros: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub pid: u16,
    /// USB product string.
    pub name: String,
    pub series: String,
    /// `water_cooler`, `air_cooler`, `case` or `unknown`.
    pub category: &'static str,
    /// Display layout key for previews, e.g. `ld`.
    pub display: &'static str,
    pub supported: bool,
    pub capabilities: Option<CapabilitiesView>,
    pub running: bool,
    pub error: Option<String>,
    pub profile: Profile,
}

fn symbols(modes: &[Mode]) -> Vec<&'static str> {
    modes.iter().map(Mode::symbol).collect()
}

struct State {
    running: HashMap<u16, Running>,
    /// Displays the user stopped; automatic start leaves them alone until started again.
    paused: HashSet<u16>,
    errors: HashMap<u16, String>,
    last_attempt: HashMap<u16, Instant>,
    gpu_id: Option<String>,
}

pub struct Displays {
    api: Mutex<Option<HidApi>>,
    telemetry: Arc<Telemetry>,
    state: Mutex<State>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Parses a GPU id like `nvidia:1` and picks that GPU (or the automatic choice).
fn pick_gpu(gpu_id: &str) -> Option<gpu::pci::PciDevice> {
    let gpus = gpu::pci::get_gpu_list();
    let id = gpu_id.split_once(':').and_then(|(vendor, n)| Some((gpu::pci::Vendor::get(vendor)?, n.parse().ok()?)));
    gpu::select(&gpus, id).or_else(|| gpu::select(&gpus, None))
}

impl Displays {
    pub fn new(telemetry: Arc<Telemetry>) -> Self {
        Displays {
            api: Mutex::new(HidApi::new().ok()),
            telemetry,
            state: Mutex::new(State {
                running: HashMap::new(),
                paused: HashSet::new(),
                errors: HashMap::new(),
                last_attempt: HashMap::new(),
                gpu_id: None,
            }),
        }
    }

    fn connected(&self) -> Vec<devices::Found> {
        let mut api = lock(&self.api);
        if api.is_none() {
            *api = HidApi::new().ok();
        }
        match api.as_mut() {
            Some(api) => {
                let _ = api.refresh_devices();
                devices::find_devices(api)
            }
            None => Vec::new(),
        }
    }

    /// Notes displays whose thread has ended (stopped by an error such as unplugging).
    fn reap(&self, state: &mut State) {
        let ended: Vec<u16> = state.running.iter().filter(|(_, r)| r.is_finished()).map(|(&pid, _)| pid).collect();
        for pid in ended {
            if let Some(running) = state.running.remove(&pid) {
                if let Err(err) = running.join() {
                    state.errors.insert(pid, err.to_string());
                }
            }
        }
    }

    pub fn list(&self) -> Vec<DeviceView> {
        let found = self.connected();
        let mut state = lock(&self.state);
        self.reap(&mut state);
        found
            .into_iter()
            .map(|device| {
                let product = catalog::product(device.pid);
                DeviceView {
                    pid: device.pid,
                    series: product.as_ref().map_or_else(|| device.name.clone(), |p| p.series.to_owned()),
                    name: device.name,
                    category: product.as_ref().map_or("unknown", |p| p.category.key()),
                    display: product.as_ref().map_or("generic", |p| p.display),
                    supported: product.is_some(),
                    capabilities: product.map(|p| CapabilitiesView {
                        modes: symbols(p.capabilities.modes),
                        secondary: symbols(p.capabilities.secondary),
                        fahrenheit: p.capabilities.fahrenheit,
                        alarm: p.capabilities.alarm,
                        rotate: p.capabilities.rotate,
                        lead_zeros: p.capabilities.lead_zeros,
                    }),
                    running: state.running.contains_key(&device.pid),
                    error: state.errors.get(&device.pid).cloned(),
                    profile: config::load_profile(device.pid),
                }
            })
            .collect()
    }

    /// Starts (or restarts) a display with its saved profile.
    pub fn start(&self, pid: u16, fahrenheit: bool) -> Result<(), String> {
        let profile = config::load_profile(pid);
        let mut state = lock(&self.state);
        state.paused.remove(&pid);
        state.last_attempt.insert(pid, Instant::now());
        if let Some(running) = state.running.remove(&pid) {
            let _ = running.stop();
        }
        let result = self.spawn(&mut state, pid, &profile, fahrenheit);
        match &result {
            Ok(()) => state.errors.remove(&pid),
            Err(err) => state.errors.insert(pid, err.clone()),
        };
        result
    }

    fn spawn(&self, state: &mut State, pid: u16, profile: &Profile, fahrenheit: bool) -> Result<(), String> {
        let settings = profile.settings(fahrenheit);
        let mut driver = devices::driver_for(pid, &settings).map_err(|e| e.to_string())?;
        // All displays share one GPU reader; switch it if this profile asks for another GPU.
        if state.gpu_id.as_deref() != Some(profile.gpuid.as_str()) {
            self.telemetry.set_gpu(pick_gpu(&profile.gpuid).as_ref());
            state.gpu_id = Some(profile.gpuid.clone());
        }
        let api = lock(&self.api);
        let api = api.as_ref().ok_or("The USB device library could not start.")?;
        let (device, product) = runner::open(api, vendor_for(pid), pid).map_err(|_| {
            "Failed to access the USB device. Reconnect it, or check the USB access rules.".to_owned()
        })?;
        driver.on_open(&product);
        let running = runner::spawn(device, driver, self.telemetry.clone(), settings.update, |_| {});
        state.running.insert(pid, running);
        Ok(())
    }

    pub fn stop(&self, pid: u16) {
        let mut state = lock(&self.state);
        state.paused.insert(pid);
        state.errors.remove(&pid);
        if let Some(running) = state.running.remove(&pid) {
            let _ = running.stop();
        }
    }

    pub fn stop_all(&self) {
        let mut state = lock(&self.state);
        for (_, running) in state.running.drain() {
            let _ = running.stop();
        }
    }

    pub fn is_running(&self, pid: u16) -> bool {
        lock(&self.state).running.contains_key(&pid)
    }

    /// Connected, supported devices whose profile asks for automatic start.
    fn wanted(&self) -> Vec<u16> {
        self.connected()
            .into_iter()
            .map(|d| d.pid)
            .filter(|&pid| catalog::product(pid).is_some() && config::load_profile(pid).autostart)
            .collect()
    }

    /// True if any display runs or should start automatically.
    pub fn has_work(&self) -> bool {
        !lock(&self.state).running.is_empty() || !self.wanted().is_empty()
    }

    /// Restarts running displays, e.g. after the temperature unit changed.
    pub fn restart_running(&self, fahrenheit: bool) {
        let pids: Vec<u16> = lock(&self.state).running.keys().copied().collect();
        for pid in pids {
            let _ = self.start(pid, fahrenheit);
        }
    }

    /// Starts automatic displays that are connected but not running. Returns true if anything changed.
    pub fn keep_alive(&self, fahrenheit: bool) -> bool {
        let wanted = self.wanted();
        let mut changed = {
            let mut state = lock(&self.state);
            let before = state.running.len();
            self.reap(&mut state);
            before != state.running.len()
        };
        for pid in wanted {
            let due = {
                let state = lock(&self.state);
                !state.running.contains_key(&pid)
                    && !state.paused.contains(&pid)
                    && state.last_attempt.get(&pid).is_none_or(|t| t.elapsed() >= RETRY_AFTER)
            };
            if due {
                let _ = self.start(pid, fahrenheit);
                changed = true;
            }
        }
        changed
    }
}
