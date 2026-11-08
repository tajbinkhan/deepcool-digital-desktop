//! DeepCool Digital desktop control panel.
//!
//! Drives the `deepcool-digital-linux` controller as a child process, one per
//! product ID, so a controller crash never takes the window down with it.
//! The app can keep running without its window, and `--background` starts it
//! that way (used at sign-in).

mod core;
#[allow(dead_code)] // `enable` and `disable` are used by the helper binary.
mod cpu_power;
mod instance;
mod panel;
mod png;
mod tray;
#[allow(dead_code)] // Parts are used only by the update helper binary.
mod update;
mod updater;

use eframe::egui::{self, Color32, RichText};
use instance::{Acquired, Instance, Request};
use panel::{label, Panel};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tray::Tray;
use updater::State as UpdateState;

const SPACING: f32 = 18.0;
const USAGE: &str = "Usage: deepcool-digital-gui [--background]

  --background   Start without a window: run displays set to start automatically.
                 Opening the app again shows the window.";

/// Set when the session asks the app to end (for example at sign-out).
static TERMINATE: AtomicBool = AtomicBool::new(false);

extern "C" fn request_exit(_signal: libc::c_int) {
    TERMINATE.store(true, Ordering::SeqCst);
}

/// Exit cleanly on SIGTERM, SIGINT and SIGHUP: stop the displays and remove the instance socket.
fn handle_exit_signals() {
    let handler = request_exit as extern "C" fn(libc::c_int) as libc::sighandler_t;
    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        unsafe { libc::signal(signal, handler) };
    }
}

/// Wakes whichever loop is running: the window when it is open, otherwise nothing
/// (the background loop polls on its own).
#[derive(Clone, Default)]
struct Waker(Arc<Mutex<Option<egui::Context>>>);

impl Waker {
    fn notify(&self) -> Arc<dyn Fn() + Send + Sync> {
        let target = self.0.clone();
        Arc::new(move || {
            if let Some(ctx) = &*target.lock().unwrap() {
                ctx.request_repaint();
            }
        })
    }

    fn set(&self, ctx: Option<egui::Context>) {
        *self.0.lock().unwrap() = ctx;
    }
}

/// Test hooks from the environment: close the window soon, optionally saving a screenshot.
#[derive(Default)]
struct Automation {
    smoke_test: bool,
    screenshot: Option<String>,
}

impl Automation {
    fn from_env() -> Self {
        Automation {
            smoke_test: std::env::var_os("DEEPCOOL_SMOKE_TEST").is_some(),
            screenshot: std::env::var("DEEPCOOL_SCREENSHOT").ok().filter(|p| !p.is_empty()),
        }
    }

    fn active(&self) -> bool {
        self.smoke_test || self.screenshot.is_some()
    }
}

struct App<'a> {
    panel: &'a mut Panel,
    requests: &'a Receiver<Request>,
    quit: &'a mut bool,
    /// Relaunch after quitting, to run a newly installed version.
    restart: &'a mut bool,
    automation: &'a mut Automation,
    tray: &'a mut Option<Tray>,
    automated: bool,
    opened: Instant,
}

impl App<'_> {
    /// Test hooks: close after a moment, optionally saving a screenshot first.
    fn automation(&mut self, ctx: &egui::Context) {
        let ready = !self.panel.scanning && self.opened.elapsed() > Duration::from_millis(1500);
        if let Some(path) = ready.then(|| self.automation.screenshot.take()).flatten() {
            let closer = ctx.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                match png::write(&path, image.size[0], image.size[1], &image.pixels) {
                    Ok(()) => println!("Saved screenshot to {path}"),
                    Err(error) => eprintln!("Could not save screenshot: {error}"),
                }
                closer.send_viewport_cmd(egui::ViewportCommand::Close);
                closer.request_repaint();
            })));
        } else if self.automation.smoke_test && self.automation.screenshot.is_none() && ready {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.automated {
            // The smoke test exits fully; a screenshot only closes the window.
            *self.quit |= self.automation.smoke_test;
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// Shown at the top while an update is available or being installed.
    fn update_banner(&mut self, ui: &mut egui::Ui) {
        let state = self.panel.updater.state.clone();
        let current = self.panel.updater.current.clone();
        let fill = ui.visuals().selection.bg_fill.gamma_multiply(0.25);
        let show = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
            egui::Frame::group(ui.style()).fill(fill).show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui);
            });
            ui.add_space(SPACING / 2.0);
        };
        match state {
            UpdateState::Available(release) => show(ui, &mut |ui| {
                ui.label(
                    RichText::new(format!("Version {} is available. You have version {current}.", release.version))
                        .strong(),
                );
                ui.horizontal(|ui| {
                    let can_install = self.panel.updater.installer.is_some() && release.package.is_some();
                    let label = if can_install { "Update now" } else { "Download…" };
                    let accent = ui.visuals().selection.bg_fill;
                    let button = egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(accent);
                    if ui.add(button).clicked() {
                        if let Err(error) = self.panel.updater.install() {
                            self.panel.log(error);
                        }
                    }
                    if ui.button("What's new").clicked() {
                        if let Err(error) = updater::open_url(&release.page) {
                            self.panel.log(error);
                        }
                    }
                });
                if self.panel.updater.installer.is_none() {
                    hint(
                        ui,
                        "This copy wasn't installed from a package, so it can't update itself. \
                         Download opens the release page.",
                    );
                } else if release.package.is_none() {
                    hint(
                        ui,
                        "This release has no package for your system. Download opens the release page.",
                    );
                } else {
                    hint(
                        ui,
                        "The update is downloaded, checked, and installed after you enter your password.",
                    );
                }
            }),
            UpdateState::Downloading { release, done, total } => show(ui, &mut |ui| {
                ui.label(RichText::new(format!("Downloading version {}…", release.version)).strong());
                let fraction = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                ui.add(egui::ProgressBar::new(fraction).show_percentage());
            }),
            UpdateState::Installing(release) => show(ui, &mut |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("Installing version {}…", release.version)).strong());
                });
                hint(ui, "Enter your administrator password if asked.");
            }),
            UpdateState::Installed(version) => show(ui, &mut |ui| {
                ui.label(RichText::new(format!("Version {version} is installed.")).strong());
                ui.horizontal(|ui| {
                    let accent = ui.visuals().selection.bg_fill;
                    let button = egui::Button::new(RichText::new("Restart now").color(Color32::WHITE)).fill(accent);
                    if ui.add(button).clicked() {
                        *self.restart = true;
                        *self.quit = true;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.label(RichText::new("Running displays start again after the restart.").weak());
                });
            }),
            _ => {}
        }
    }

    fn updates_section(&mut self, ui: &mut egui::Ui) {
        let panel = &mut *self.panel;
        ui.label(RichText::new("Updates").strong());
        ui.horizontal(|ui| {
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            let status = match &panel.updater.state {
                UpdateState::Checking => "Checking for updates…".to_owned(),
                UpdateState::UpToDate => "Up to date.".to_owned(),
                UpdateState::Available(release) => format!("Version {} is available.", release.version),
                UpdateState::Downloading { .. } | UpdateState::Installing(_) => "Updating…".to_owned(),
                UpdateState::Installed(version) => format!("Version {version} installed; restart to use it."),
                UpdateState::Failed(error) => error.clone(),
                UpdateState::Idle => String::new(),
            };
            if matches!(panel.updater.state, UpdateState::Checking) {
                ui.spinner();
            }
            ui.label(RichText::new(status).weak());
        });
        if ui
            .add_enabled(!panel.updater.busy(), egui::Button::new("Check for updates"))
            .clicked()
        {
            panel.updater.check();
        }
        let mut automatic = panel.preferences.check_for_updates;
        if ui.checkbox(&mut automatic, "Check for updates automatically").changed() {
            panel.set_check_for_updates(automatic);
        }
        hint(
            ui,
            "Asks GitHub for the latest release when the window opens and every few hours while it is open.",
        );
    }

    fn cpu_power_section(&mut self, ui: &mut egui::Ui) {
        let panel = &mut *self.panel;
        ui.label(RichText::new("CPU power").strong());
        match panel.cpu_power {
            cpu_power::Access::Readable => {
                ui.horizontal(|ui| {
                    ui.label("✔ CPU power readings are available, so the display can show CPU power.");
                    if panel.cpu_power_rule {
                        let button = egui::Button::new(if panel.cpu_power_busy { "Waiting…" } else { "Turn off" });
                        if ui
                            .add_enabled(!panel.cpu_power_busy, button)
                            .on_hover_text("Make the CPU energy counter readable only by administrators again")
                            .clicked()
                        {
                            panel.set_cpu_power(false);
                        }
                    }
                });
            }
            cpu_power::Access::Restricted => {
                ui.label(
                    "Your system lets only administrators read the CPU energy counter, \
                     so the display shows 0 W for CPU power.",
                );
                let label = if panel.cpu_power_busy {
                    "Waiting for permission…"
                } else {
                    "Allow CPU power readings…"
                };
                if ui
                    .add_enabled(!panel.cpu_power_busy, egui::Button::new(label))
                    .clicked()
                {
                    panel.set_cpu_power(true);
                }
                hint(
                    ui,
                    "You'll be asked for an administrator password. This makes the CPU energy counter readable by \
                     every user on this computer, now and after restarts. Linux restricts it by default because \
                     precise energy readings have been used in side-channel attacks; on a personal computer the risk \
                     is low. Running displays restart to pick up the change, and you can turn it off again here.",
                );
            }
            cpu_power::Access::NoSensor => {
                hint(
                    ui,
                    "No supported CPU energy sensor was found, so CPU power can't be shown. Supported kernel \
                     modules are intel_rapl, amd_energy and zenergy.",
                );
            }
        }
    }

    fn background_section(&mut self, ui: &mut egui::Ui) {
        let panel = &mut *self.panel;
        ui.label(RichText::new("Automatic start").strong());
        let supported = panel.pid().is_some_and(core::is_supported);
        let mut device_auto = panel.settings.autostart;
        let response = ui.add_enabled(
            supported,
            egui::Checkbox::new(&mut device_auto, "Start this display automatically"),
        );
        if response.changed() {
            panel.set_device_autostart(device_auto);
        }
        hint(
            ui,
            "Uses this device's saved settings, and restarts the display if it stops unexpectedly.",
        );

        let mut background = panel.preferences.run_in_background;
        if ui
            .checkbox(&mut background, "Keep displays running after I close this window")
            .changed()
        {
            panel.set_run_in_background(background);
        }
        let mut sign_in = panel.sign_in;
        if ui
            .checkbox(&mut sign_in, "Start DeepCool Digital in the background when I sign in")
            .changed()
        {
            panel.set_sign_in(sign_in);
        }
        hint(
            ui,
            "At sign-in the app starts without a window and runs the displays set to start automatically. \
             Click the DeepCool Digital icon in the top bar, or open the app from the app menu, to show this \
             window again. The icon's menu can also stop the displays and quit.",
        );
    }

    fn settings_grid(&mut self, ui: &mut egui::Ui) {
        let panel = &mut self.panel;
        let cap = panel.pid().map(core::capabilities);
        let enabled = |check: fn(&core::Capabilities) -> bool| cap.as_ref().is_some_and(check);
        let modes = panel.mode_options();
        let secondary = panel.secondary_options();
        let gpus = panel.gpus.clone();
        let combo_width = (ui.available_width() - 220.0).max(200.0);

        egui::Grid::new("settings")
            .num_columns(2)
            .spacing([24.0, 14.0])
            .show(ui, |ui| {
                ui.label("Display mode");
                choice(ui, "mode", combo_width, cap.is_some(), &mut panel.settings.mode, &modes, label);
                ui.end_row();

                ui.label("Secondary display");
                choice(
                    ui,
                    "secondary",
                    combo_width,
                    secondary.len() > 1,
                    &mut panel.settings.secondary,
                    &secondary,
                    label,
                );
                ui.end_row();

                ui.label("Rotation");
                choice(
                    ui,
                    "rotate",
                    combo_width,
                    enabled(|c| c.rotate),
                    &mut panel.settings.rotate,
                    &core::ROTATIONS,
                    |r| format!("{r}°"),
                );
                ui.end_row();

                ui.label("Refresh interval");
                ui.add_enabled(
                    cap.is_some(),
                    egui::Slider::new(&mut panel.settings.update, core::UPDATE_RANGE)
                        .step_by(100.0)
                        .suffix(" ms"),
                );
                ui.end_row();

                ui.label("GPU to monitor");
                let ids: Vec<&str> = gpus.iter().map(|(id, _)| id.as_str()).collect();
                choice(ui, "gpu", combo_width, cap.is_some(), &mut panel.settings.gpuid, &ids, |id| {
                    gpus.iter()
                        .find(|(known, _)| known == id)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_default()
                });
                ui.end_row();

                for (text, value, allowed) in [
                    ("Use Fahrenheit", &mut panel.settings.fahrenheit, enabled(|c| c.fahrenheit)),
                    ("Temperature alarm", &mut panel.settings.alarm, enabled(|c| c.alarm)),
                    ("Show leading zeros", &mut panel.settings.zeros, enabled(|c| c.zeros)),
                ] {
                    ui.label("");
                    ui.add_enabled(allowed, egui::Checkbox::new(value, text));
                    ui.end_row();
                }
            });
    }
}

/// A drop-down that edits `value`, choosing among `options`.
fn choice(
    ui: &mut egui::Ui,
    id: &str,
    width: f32,
    enabled: bool,
    value: &mut String,
    options: &[&str],
    name: impl Fn(&str) -> String,
) {
    ui.add_enabled_ui(enabled && options.len() > 1, |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(width)
            .selected_text(name(value))
            .show_ui(ui, |ui| {
                for option in options {
                    if ui.selectable_label(value == option, name(option)).clicked() {
                        *value = option.to_string();
                    }
                }
            });
    });
}

impl eframe::App for App<'_> {
    /// Runs before every frame, and also while the window is minimized or covered
    /// (when `ui` is not called), so tray and quit requests are handled right away.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.panel.poll();
        let check = self.panel.preferences.check_for_updates;
        self.panel.updater.maybe_check(check);
        if TERMINATE.load(Ordering::SeqCst) {
            *self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Another launch of the app, or the tray icon, asked to see the window or to quit.
        while let Ok(request) = self.requests.try_recv() {
            match request {
                Request::Show => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Request::Quit => {
                    *self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if let Some(tray) = self.tray.as_mut() {
            tray.set_running(self.panel.controller.running().len());
        }
        // Controller state can change without output (for example when it is killed).
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.automation(&ctx);

        // The whole window scrolls when it is shorter than its contents.
        egui::Frame::central_panel(ui.style()).inner_margin(0.0).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("window")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Frame::new().inner_margin(24.0).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("DeepCool Digital").size(28.0).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).weak());
                            });
                        });
                        ui.label("Your hardware. Your display.");
                        ui.add_space(SPACING / 2.0);
                        self.update_banner(ui);
                        ui.add(egui::Label::new(RichText::new(&self.panel.status).strong()).wrap());
                        ui.add_space(SPACING / 2.0);

                        ui.horizontal(|ui| {
                            let names: Vec<String> = self
                                .panel
                                .devices
                                .iter()
                                .map(|(pid, name)| format!("{name} · {pid}"))
                                .collect();
                            let current = names.get(self.panel.selected).cloned().unwrap_or_else(|| {
                                if self.panel.scanning {
                                    "Scanning…".into()
                                } else {
                                    "No device detected".into()
                                }
                            });
                            let mut selected = self.panel.selected;
                            ui.add_enabled_ui(!names.is_empty(), |ui| {
                                egui::ComboBox::from_id_salt("device")
                                    .width((ui.available_width() - 150.0).max(200.0))
                                    .selected_text(current)
                                    .show_ui(ui, |ui| {
                                        for (index, name) in names.iter().enumerate() {
                                            ui.selectable_value(&mut selected, index, name);
                                        }
                                    });
                            });
                            if selected != self.panel.selected {
                                self.panel.selected = selected;
                                self.panel.device_changed();
                            }
                            if ui
                                .add_enabled(!self.panel.scanning, egui::Button::new("Refresh devices"))
                                .clicked()
                            {
                                self.panel.scan();
                            }
                        });
                        ui.add_space(SPACING / 2.0);

                        self.settings_grid(ui);
                        ui.add_space(SPACING / 2.0);

                        ui.horizontal(|ui| {
                            let accent = ui.visuals().selection.bg_fill;
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let quit = ui
                                    .button("Stop all and quit")
                                    .on_hover_text("Stops every display and exits the app");
                                if quit.clicked() {
                                    *self.quit = true;
                                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let can_start = self.panel.can_start();
                                    // Highlight Start only while it can be pressed.
                                    let start = if can_start {
                                        egui::Button::new(RichText::new("Start display").color(Color32::WHITE))
                                            .fill(accent)
                                    } else {
                                        egui::Button::new("Start display")
                                    };
                                    if ui.add_enabled(can_start, start).clicked() {
                                        self.panel.start();
                                    }
                                    if ui
                                        .add_enabled(self.panel.is_running(), egui::Button::new("Stop display"))
                                        .clicked()
                                    {
                                        self.panel.stop();
                                    }
                                    if ui
                                        .add_enabled(self.panel.pid().is_some(), egui::Button::new("Save settings"))
                                        .clicked()
                                    {
                                        self.panel.save();
                                    }
                                });
                            });
                        });
                        hint(
                            ui,
                            "Start applies the selected settings. Stop before changing a running display. \
                 An existing system service must be stopped separately.",
                        );
                        ui.add_space(SPACING / 2.0);

                        self.cpu_power_section(ui);
                        ui.add_space(SPACING / 2.0);

                        self.background_section(ui);
                        ui.add_space(SPACING / 2.0);

                        self.updates_section(ui);
                        ui.add_space(SPACING / 2.0);

                        ui.label(RichText::new("Activity").strong());
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("activity")
                                .auto_shrink([false, false])
                                .max_height(180.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    let text: String =
                                        self.panel.activity.iter().map(|line| format!("{line}\n")).collect();
                                    ui.add(
                                        egui::Label::new(RichText::new(text).monospace())
                                            .wrap()
                                            .selectable(true),
                                    );
                                });
                        });
                    });
                });
        });
    }
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).weak()).wrap());
}

fn open_window(
    panel: &mut Panel,
    requests: &Receiver<Request>,
    waker: &Waker,
    quit: &mut bool,
    restart: &mut bool,
    automation: &mut Automation,
    tray: &mut Option<Tray>,
) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DeepCool Digital")
            .with_app_id("deepcool-digital")
            .with_inner_size([860.0, 900.0])
            .with_min_inner_size([560.0, 700.0]),
        ..Default::default()
    };
    let automated = automation.active();
    let result = eframe::run_native(
        "DeepCool Digital",
        options,
        Box::new(|cc| {
            waker.set(Some(cc.egui_ctx.clone()));
            Ok(Box::new(App {
                panel,
                requests,
                quit,
                restart,
                automation,
                tray,
                automated,
                opened: Instant::now(),
            }))
        }),
    );
    waker.set(None);
    result
}

fn main() {
    let mut background = false;
    let mut resume = Vec::new();
    for arg in std::env::args().skip(1) {
        if let Some(list) = arg.strip_prefix("--resume=") {
            // Used when the app restarts itself after an update.
            resume = list.split(',').filter_map(|pid| pid.parse::<u32>().ok()).collect();
            continue;
        }
        match arg.as_str() {
            "--background" => background = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            other => {
                eprintln!("Unknown option '{other}'.\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("Do not run DeepCool Digital as root. Installed packages grant your desktop session USB access.");
    }

    let waker = Waker::default();
    let (sender, requests) = mpsc::channel();
    let instance = match Instance::acquire(!background, sender.clone(), waker.notify()) {
        Acquired::Primary(instance) => instance,
        Acquired::AlreadyRunning => {
            println!("DeepCool Digital is already running.");
            return;
        }
    };

    handle_exit_signals();
    let mut panel = Panel::new(waker.notify());
    panel.resume = resume;
    let mut tray = match Tray::show(sender, waker.notify()) {
        Ok(tray) => Some(tray),
        Err(error) => {
            panel.log(format!(
                "No system tray icon: {error}. On GNOME, the AppIndicator extension adds tray support."
            ));
            None
        }
    };
    panel.scan();
    let mut automation = Automation::from_env();
    let mut quit = false;
    let mut restart = false;
    let mut show = !background;
    if background {
        panel.log("Started in the background.");
    }
    loop {
        if show {
            show = false;
            if let Err(error) = open_window(
                &mut panel,
                &requests,
                &waker,
                &mut quit,
                &mut restart,
                &mut automation,
                &mut tray,
            ) {
                eprintln!(
                    "No graphical display is available. Open DeepCool Digital from your desktop session. ({error})"
                );
                if !background {
                    panel.close();
                    std::process::exit(1);
                }
            }
            // A request that arrived as the window closed still counts.
            while let Ok(request) = requests.try_recv() {
                quit |= request == Request::Quit;
            }
            if quit || !panel.preferences.run_in_background {
                break;
            }
            if panel.has_background_work() {
                panel.log("Window closed. Displays keep running in the background.");
            }
        }
        // Without a window: keep automatic displays running until asked to show the window.
        panel.poll();
        if let Some(tray) = tray.as_mut() {
            tray.set_running(panel.controller.running().len());
        }
        if TERMINATE.load(Ordering::SeqCst) || !panel.has_background_work() {
            break;
        }
        match requests.recv_timeout(Duration::from_millis(500)) {
            Ok(Request::Show) => show = true,
            Ok(Request::Quit) => {
                panel.log("Quit from the tray icon.");
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    drop(tray);
    if restart {
        restart_app(panel, instance);
    }
    panel.log("DeepCool Digital closed; stopping its displays.");
    panel.close();
}

/// Replaces this process with the newly installed version, which starts the
/// displays that were running again.
fn restart_app(mut panel: Panel, instance: Instance) -> ! {
    use std::os::unix::process::CommandExt;
    let running: Vec<String> = panel.controller.running().iter().map(u32::to_string).collect();
    panel.log("Restarting DeepCool Digital to finish the update.");
    panel.close();
    drop(panel);
    drop(instance); // Let the new process become the running copy.
    let app = updater::app_path().unwrap_or_else(|| update::INSTALLED_APP.into());
    let mut command = std::process::Command::new(&app);
    if !running.is_empty() {
        command.arg(format!("--resume={}", running.join(",")));
    }
    let error = command.exec();
    eprintln!("Could not restart DeepCool Digital ({error}). Open it again from the app menu.");
    std::process::exit(1);
}
