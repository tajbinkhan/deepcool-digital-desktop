//! DeepCool Digital desktop control panel.
//!
//! Drives the `deepcool-digital-linux` controller as a child process, one per
//! product ID, so a controller crash never takes the window down with it.
//! The app can keep running without its window, and `--background` starts it
//! that way (used at sign-in).

mod core;
mod instance;
mod panel;
mod png;

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
    automation: &'a mut Automation,
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
             Open DeepCool Digital from the app menu to show this window again.",
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.panel.poll();
        self.automation(&ctx);
        if TERMINATE.load(Ordering::SeqCst) {
            *self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Another launch of the app asked to see the window.
        while let Ok(Request::Show) = self.requests.try_recv() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        // Controller state can change without output (for example when it is killed).
        ctx.request_repaint_after(Duration::from_millis(500));

        egui::Frame::central_panel(ui.style())
            .inner_margin(24.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(RichText::new("DeepCool Digital").size(28.0).strong());
                ui.label("Your hardware. Your display.");
                ui.add_space(SPACING / 2.0);
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
                                egui::Button::new(RichText::new("Start display").color(Color32::WHITE)).fill(accent)
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

                self.background_section(ui);
                ui.add_space(SPACING / 2.0);

                ui.label(RichText::new("Activity").strong());
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .min_scrolled_height(110.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            let text: String = self.panel.activity.iter().map(|line| format!("{line}\n")).collect();
                            ui.add(
                                egui::Label::new(RichText::new(text).monospace())
                                    .wrap()
                                    .selectable(true),
                            );
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
    automation: &mut Automation,
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
                automation,
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
    for arg in std::env::args().skip(1) {
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
    let _instance = match Instance::acquire(!background, sender, waker.notify()) {
        Acquired::Primary(instance) => instance,
        Acquired::AlreadyRunning => {
            println!("DeepCool Digital is already running.");
            return;
        }
    };

    handle_exit_signals();
    let mut panel = Panel::new(waker.notify());
    panel.scan();
    let mut automation = Automation::from_env();
    let mut quit = false;
    let mut show = !background;
    if background {
        panel.log("Started in the background.");
    }
    loop {
        if show {
            show = false;
            if let Err(error) = open_window(&mut panel, &requests, &waker, &mut quit, &mut automation) {
                eprintln!(
                    "No graphical display is available. Open DeepCool Digital from your desktop session. ({error})"
                );
                if !background {
                    panel.close();
                    std::process::exit(1);
                }
            }
            while requests.try_recv().is_ok() {}
            if quit || !panel.preferences.run_in_background {
                break;
            }
            if panel.has_background_work() {
                panel.log("Window closed. Displays keep running in the background.");
            }
        }
        // Without a window: keep automatic displays running until asked to show the window.
        panel.poll();
        if TERMINATE.load(Ordering::SeqCst) || !panel.has_background_work() {
            break;
        }
        match requests.recv_timeout(Duration::from_millis(500)) {
            Ok(Request::Show) => show = true,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    panel.log("DeepCool Digital closed; stopping its displays.");
    panel.close();
}
