//! The icon in the desktop's system tray (top bar on GNOME, panel on KDE), shown
//! while the app runs, with or without its window.
//!
//! Uses the StatusNotifierItem protocol over D-Bus. KDE, Cinnamon, XFCE and Ubuntu's
//! GNOME support it; plain GNOME needs the AppIndicator extension. Without support
//! the app still runs in the background and can be opened from the app menu.

use crate::instance::Request;
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::StandardItem,
    MenuItem, ToolTip,
};
use std::sync::{mpsc::Sender, Arc};

struct Icon {
    requests: Sender<Request>,
    /// Wakes the window so it sees the request.
    notify: Arc<dyn Fn() + Send + Sync>,
    /// Shown in the menu and tooltip.
    status: String,
}

impl Icon {
    fn send(&self, request: Request) {
        let _ = self.requests.send(request);
        (self.notify)();
    }
}

impl ksni::Tray for Icon {
    fn id(&self) -> String {
        "deepcool-digital".into()
    }

    fn title(&self) -> String {
        "DeepCool Digital".into()
    }

    fn icon_name(&self) -> String {
        "deepcool-digital".into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "DeepCool Digital".into(),
            description: self.status.clone(),
            ..Default::default()
        }
    }

    /// A left click opens the window.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Request::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: self.status.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Open DeepCool Digital".into(),
                activate: Box::new(|icon: &mut Self| icon.send(Request::Show)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Stop displays and quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|icon: &mut Self| icon.send(Request::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// The text shown in the menu for a number of running displays.
pub fn status(running: usize) -> String {
    match running {
        0 => "No display running".into(),
        1 => "1 display running".into(),
        n => format!("{n} displays running"),
    }
}

pub struct Tray {
    handle: Handle<Icon>,
    running: usize,
}

impl Tray {
    /// Adds the icon. Menu choices arrive on `requests`, like requests from another launch.
    pub fn show(requests: Sender<Request>, notify: Arc<dyn Fn() + Send + Sync>) -> Result<Tray, String> {
        let icon = Icon {
            requests,
            notify,
            status: status(0),
        };
        let handle = icon.spawn().map_err(|error| error.to_string())?;
        Ok(Tray { handle, running: 0 })
    }

    /// Keeps the menu's status line current. Cheap when nothing changed.
    pub fn set_running(&mut self, running: usize) {
        if running != self.running {
            self.running = running;
            self.handle.update(|icon| icon.status = status(running));
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.handle.shutdown().wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_running_displays() {
        assert_eq!(status(0), "No display running");
        assert_eq!(status(1), "1 display running");
        assert_eq!(status(3), "3 displays running");
    }
}
