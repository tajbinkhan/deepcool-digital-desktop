//! DeepCool Digital desktop app (Tauri backend).
//!
//! Displays run as threads in this process. The window (and its web view) only
//! exists while it is open: closing it to the tray frees the web view, leaving
//! just the display threads running.

mod config;
mod dashboard;
mod displays;
mod power;
mod updater;

use config::{Preferences, Profile};
use dashboard::{Dashboard, Snapshot};
use deepcool_core::{system, telemetry::Telemetry};
use displays::{DeviceView, Displays};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    thread,
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent, State, WebviewWindowBuilder,
};

const WEBSITE: &str = "https://github.com/tajbinkhan/deepcool-digital-desktop";

struct AppState {
    telemetry: Arc<Telemetry>,
    displays: Displays,
    dashboard: Mutex<Dashboard>,
    preferences: Mutex<Preferences>,
    system_info: OnceLock<SystemInfoView>,
    updater: Mutex<updater::Updater>,
}

/// Set once the app has started, so background work can send events to the window.
static APP: OnceLock<AppHandle> = OnceLock::new();

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DriveView {
    model: String,
    serial: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfoView {
    hostname: String,
    os: String,
    cpu: String,
    gpus: Vec<String>,
    motherboard: Option<String>,
    memory_total: u64,
    drives: Vec<DriveView>,
}

#[tauri::command]
fn get_snapshot(state: State<AppState>) -> Snapshot {
    lock(&state.dashboard).sample()
}

#[tauri::command]
fn select_gpu(state: State<AppState>, index: usize) -> Snapshot {
    let mut dashboard = lock(&state.dashboard);
    dashboard.select_gpu(index);
    dashboard.sample()
}

#[tauri::command]
fn get_system_info(state: State<AppState>) -> SystemInfoView {
    state
        .system_info
        .get_or_init(|| {
            let info = system::system_info();
            SystemInfoView {
                hostname: info.hostname,
                os: info.os,
                cpu: info.cpu,
                gpus: info.gpus,
                motherboard: info.motherboard,
                memory_total: info.memory_total,
                drives: info
                    .disks
                    .into_iter()
                    .map(|d| DriveView { model: d.model.unwrap_or(d.name), serial: d.serial })
                    .collect(),
            }
        })
        .clone()
}

#[tauri::command]
fn list_devices(state: State<AppState>) -> Vec<DeviceView> {
    state.displays.list()
}

#[tauri::command]
fn start_display(state: State<AppState>, pid: u16) -> Result<(), String> {
    let fahrenheit = lock(&state.preferences).fahrenheit;
    state.displays.start(pid, fahrenheit)
}

#[tauri::command]
fn stop_display(state: State<AppState>, pid: u16) {
    state.displays.stop(pid);
}

/// Saves a device profile and applies it right away if the display is running.
#[tauri::command]
fn save_profile(state: State<AppState>, pid: u16, profile: Profile) -> Result<(), String> {
    let fahrenheit = lock(&state.preferences).fahrenheit;
    let profile = Profile { fahrenheit, ..profile };
    config::save_profile(pid, &profile)?;
    if state.displays.is_running(pid) {
        state.displays.start(pid, fahrenheit)?;
    }
    Ok(())
}

#[tauri::command]
fn get_preferences(state: State<AppState>) -> Preferences {
    lock(&state.preferences).clone()
}

#[tauri::command]
fn set_preferences(state: State<AppState>, preferences: Preferences) -> Result<(), String> {
    let unit_changed = {
        let mut current = lock(&state.preferences);
        let changed = current.fahrenheit != preferences.fahrenheit;
        config::save_preferences(&preferences)?;
        *current = preferences.clone();
        changed
    };
    if unit_changed {
        // Keep the earlier per-device field in step for older versions.
        for (pid, profile) in config::load_profiles() {
            let _ = config::save_profile(pid, &Profile { fahrenheit: preferences.fahrenheit, ..profile });
        }
        state.displays.restart_running(preferences.fahrenheit);
    }
    Ok(())
}

/// Whether CPU power can be read. Picks up a change made outside the app too.
#[tauri::command]
fn cpu_power_status(state: State<AppState>) -> power::PowerStatus {
    let status = power::status();
    if (status.access == "readable") != state.telemetry.status().cpu_power {
        state.telemetry.reload_cpu_sensors();
    }
    status
}

/// Allows or restricts CPU power readings (asks for an administrator password).
#[tauri::command]
async fn set_cpu_power(app: AppHandle, allow: bool) -> Result<power::PowerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || power::set(allow))
        .await
        .map_err(|err| err.to_string())??;
    // Running displays read the counter every tick, so finding it again is enough.
    let state = app.state::<AppState>();
    state.telemetry.reload_cpu_sensors();
    Ok(power::status())
}

/// The update state as the Settings page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateView {
    /// idle, checking, upToDate, available, downloading, installing, installed or failed.
    state: &'static str,
    version: Option<String>,
    /// Download progress, 0–100.
    progress: Option<u8>,
    message: Option<String>,
    /// False for portable copies: "update" then opens the release page.
    can_install: bool,
}

fn update_view(updater: &mut updater::Updater) -> UpdateView {
    use updater::State;
    updater.poll();
    let can_install = updater.installer.is_some();
    let view = |state, version: Option<&str>| UpdateView {
        state,
        version: version.map(str::to_owned),
        progress: None,
        message: None,
        can_install,
    };
    match &updater.state {
        State::Idle => view("idle", None),
        State::Checking => view("checking", None),
        State::UpToDate => view("upToDate", None),
        State::Available(release) => view("available", Some(&release.version)),
        State::Downloading { release, done, total } => UpdateView {
            progress: Some(if *total > 0 { (done * 100 / total).min(100) as u8 } else { 0 }),
            ..view("downloading", Some(&release.version))
        },
        State::Installing(release) => view("installing", Some(&release.version)),
        State::Installed(version) => view("installed", Some(version)),
        State::Failed(message) => UpdateView { message: Some(message.clone()), ..view("failed", None) },
    }
}

#[tauri::command]
fn update_state(state: State<AppState>) -> UpdateView {
    update_view(&mut lock(&state.updater))
}

#[tauri::command]
fn check_update(state: State<AppState>) -> UpdateView {
    let mut updater = lock(&state.updater);
    updater.check();
    update_view(&mut updater)
}

/// Downloads, verifies and installs the available update (asks for an administrator password).
#[tauri::command]
fn install_update(state: State<AppState>) -> Result<UpdateView, String> {
    let mut updater = lock(&state.updater);
    updater.install()?;
    Ok(update_view(&mut updater))
}

/// Replaces this process with the newly installed version.
#[tauri::command]
fn restart_app(state: State<AppState>) -> String {
    use std::os::unix::process::CommandExt;
    state.displays.stop_all();
    let Some(path) = updater::app_path() else {
        return "Could not find this program. Start DeepCool Digital again yourself.".into();
    };
    let error = std::process::Command::new(path).exec();
    format!("Could not restart: {error}")
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
fn open_website() -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(WEBSITE)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("Could not open the website: {err}"))
}

/// Opens the window, or brings it to the front if it is already open.
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") else {
        return;
    };
    match WebviewWindowBuilder::from_config(app, config).and_then(|builder| builder.build()) {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(err) => eprintln!("Could not open the window: {err}"),
    }
}

fn quit(app: &AppHandle) {
    app.state::<AppState>().displays.stop_all();
    app.exit(0);
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open DeepCool Digital", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Stop displays and quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit_item])?;
    let mut tray = TrayIconBuilder::with_id("main").tooltip("DeepCool Digital").menu(&menu);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| match event.id.as_ref() {
        "open" => show_window(app),
        "quit" => quit(app),
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_window(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

/// Sends dashboard readings once a second while the window is visible.
fn start_sampler(app: AppHandle) {
    thread::Builder::new()
        .name("dashboard".into())
        .spawn(move || {
            let mut was_visible = false;
            loop {
                thread::sleep(Duration::from_secs(1));
                let visible = app
                    .get_webview_window("main")
                    .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
                let state = app.state::<AppState>();
                if !visible {
                    if was_visible {
                        lock(&state.dashboard).reset();
                    }
                    was_visible = false;
                    continue;
                }
                was_visible = true;
                let snapshot = lock(&state.dashboard).sample();
                let _ = app.emit("telemetry", snapshot);
            }
        })
        .expect("failed to start the dashboard thread");
}

/// Looks for updates every few hours while automatic updates are on. Only checks:
/// installing always waits for the user.
fn start_update_checks(app: AppHandle) {
    thread::Builder::new()
        .name("updates".into())
        .spawn(move || loop {
            let state = app.state::<AppState>();
            let enabled = lock(&state.preferences).check_for_updates;
            {
                let mut updater = lock(&state.updater);
                updater.poll();
                updater.maybe_check(enabled);
            }
            thread::sleep(Duration::from_secs(60));
        })
        .expect("failed to start the update thread");
}

/// Starts automatic displays and restarts them after they stop (e.g. replugging).
fn start_keep_alive(app: AppHandle) {
    thread::Builder::new()
        .name("keep-alive".into())
        .spawn(move || loop {
            let state = app.state::<AppState>();
            let fahrenheit = lock(&state.preferences).fahrenheit;
            if state.displays.keep_alive(fahrenheit) {
                let _ = app.emit("devices-changed", ());
            }
            thread::sleep(Duration::from_secs(5));
        })
        .expect("failed to start the keep-alive thread");
}

pub fn run() {
    // Window class / Wayland app id, matching StartupWMClass in the desktop file so
    // the dock shows the app's own icon.
    glib::set_prgname(Some("deepcool-digital"));
    let background = std::env::args().any(|arg| arg == "--background");
    let telemetry = Arc::new(Telemetry::new(None));
    let state = AppState {
        displays: Displays::new(telemetry.clone()),
        dashboard: Mutex::new(Dashboard::new(telemetry.clone())),
        telemetry,
        preferences: Mutex::new(config::load_preferences()),
        system_info: OnceLock::new(),
        updater: Mutex::new(updater::Updater::new(Arc::new(|| {
            if let Some(app) = APP.get() {
                let _ = app.emit("update-changed", ());
            }
        }))),
    };
    if background && !state.displays.has_work() {
        // Started at sign-in with nothing to run.
        return;
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            select_gpu,
            get_system_info,
            list_devices,
            start_display,
            stop_display,
            save_profile,
            get_preferences,
            set_preferences,
            app_version,
            open_website,
            cpu_power_status,
            set_cpu_power,
            update_state,
            check_update,
            install_update,
            restart_app,
        ])
        .setup(move |app| {
            let _ = APP.set(app.handle().clone());
            if let Err(err) = build_tray(app.handle()) {
                eprintln!("Tray icon unavailable: {err}");
            }
            start_sampler(app.handle().clone());
            start_keep_alive(app.handle().clone());
            start_update_checks(app.handle().clone());
            if !background {
                show_window(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start DeepCool Digital");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            // The last window closed. Keep running for the displays, if wanted.
            let state = app.state::<AppState>();
            let keep = lock(&state.preferences).run_in_background && state.displays.has_work();
            if keep {
                api.prevent_exit();
            } else {
                state.displays.stop_all();
            }
        }
    });
}
