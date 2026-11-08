//! Keeps one copy of the app per user session. A second launch asks the running
//! copy to show its window and then exits, so displays are never driven twice.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{mpsc::Sender, Arc},
    thread,
    time::Duration,
};

const SHOW: &str = "show";

/// A request from another launch of the app.
#[derive(Debug, PartialEq)]
pub enum Request {
    Show,
}

pub struct Instance {
    path: PathBuf,
}

pub enum Acquired {
    /// This process is the only copy; requests arrive on the given channel.
    Primary(Instance),
    /// Another copy is running and has been asked to show itself when `show` was set.
    AlreadyRunning,
}

fn socket_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("deepcool-digital.sock"),
        _ => std::env::temp_dir().join(format!("deepcool-digital-{}.sock", unsafe { libc::getuid() })),
    }
}

fn notify_running(path: &PathBuf, show: bool) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    if show {
        let _ = stream.write_all(format!("{SHOW}\n").as_bytes());
    }
    true
}

impl Instance {
    /// Becomes the primary copy, or tells the existing one to show its window.
    /// `notify` is called after each request is queued.
    pub fn acquire(show: bool, requests: Sender<Request>, notify: Arc<dyn Fn() + Send + Sync>) -> Acquired {
        let path = socket_path();
        for _ in 0..2 {
            if notify_running(&path, show) {
                return Acquired::AlreadyRunning;
            }
            // Nobody answered, so any socket file left is stale (for example after a crash).
            let _ = std::fs::remove_file(&path);
            match UnixListener::bind(&path) {
                Ok(listener) => {
                    thread::spawn(move || {
                        for stream in listener.incoming().flatten() {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let mut line = String::new();
                            if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == SHOW {
                                let _ = requests.send(Request::Show);
                                notify();
                            }
                        }
                    });
                    return Acquired::Primary(Instance { path });
                }
                // Another copy started at the same moment; ask it instead.
                Err(_) => thread::sleep(Duration::from_millis(100)),
            }
        }
        eprintln!("Could not check for another running copy of DeepCool Digital; continuing.");
        Acquired::Primary(Instance { path: PathBuf::new() })
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tests::{temp_dir, ENV};
    use std::sync::mpsc;

    #[test]
    fn second_launch_asks_first_to_show() {
        let _guard = ENV.lock().unwrap();
        let dir = temp_dir("instance");
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        let (sender, requests) = mpsc::channel();
        let Acquired::Primary(first) = Instance::acquire(false, sender.clone(), Arc::new(|| {})) else {
            panic!("first launch should be primary");
        };
        assert!(matches!(
            Instance::acquire(true, sender.clone(), Arc::new(|| {})),
            Acquired::AlreadyRunning
        ));
        assert_eq!(requests.recv_timeout(Duration::from_secs(5)), Ok(Request::Show));
        // A background launch while running does nothing visible.
        assert!(matches!(
            Instance::acquire(false, sender.clone(), Arc::new(|| {})),
            Acquired::AlreadyRunning
        ));
        assert!(requests.recv_timeout(Duration::from_millis(300)).is_err());
        drop(first);
        assert!(!dir.join("deepcool-digital.sock").exists());
        // A stale socket file from a crashed copy is replaced.
        std::fs::write(dir.join("deepcool-digital.sock"), "").unwrap();
        assert!(matches!(
            Instance::acquire(false, sender, Arc::new(|| {})),
            Acquired::Primary(_)
        ));
        std::env::remove_var("XDG_RUNTIME_DIR");
        let _ = std::fs::remove_dir_all(dir);
    }
}
