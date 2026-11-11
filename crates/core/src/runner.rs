//! Drives one display: writes the init packets, then one frame per update
//! interval until asked to stop.

use crate::{
    devices::Driver,
    telemetry::{Readings, Telemetry},
    Error,
};
use hidapi::{HidApi, HidDevice};
use std::{
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Where packets go. Implemented for USB devices, and by fakes in tests.
pub trait Sink {
    fn write(&mut self, data: &[u8]) -> Result<(), String>;
}

impl Sink for HidDevice {
    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        HidDevice::write(self, data).map(|_| ()).map_err(|err| err.to_string())
    }
}

/// Opens a device and returns it with its USB product string.
pub fn open(api: &HidApi, vid: u16, pid: u16) -> Result<(HidDevice, String), Error> {
    let device = api.open(vid, pid).map_err(|_| Error::DeviceAccess { pid })?;
    let product = device.get_product_string().ok().flatten().unwrap_or_default();
    Ok((device, product))
}

/// Runs a display until `stop` receives a message or its sender is dropped.
///
/// `on_frame` is called after each successful write, for example to show the
/// readings in a preview.
pub fn run(
    sink: &mut dyn Sink,
    driver: &mut dyn Driver,
    telemetry: &Telemetry,
    update: Duration,
    stop: &mpsc::Receiver<()>,
    on_frame: &mut dyn FnMut(&Readings),
) -> Result<(), Error> {
    for packet in driver.init() {
        sink.write(&packet).map_err(Error::Write)?;
    }
    let needs = driver.needs();
    let mut before = telemetry.sample(needs);
    // Ticks on a fixed schedule, so slow writes do not make the display drift.
    let mut next = Instant::now() + update;
    loop {
        let wait = next.saturating_duration_since(Instant::now());
        match stop.recv_timeout(wait) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        let after = telemetry.sample(needs);
        let readings = Readings::between(&before, &after);
        let packet = driver.frame(&readings, after.at);
        sink.write(&packet).map_err(Error::Write)?;
        on_frame(&readings);
        before = after;
        next += update;
        // After a stall (e.g. suspend), restart the schedule instead of catching up.
        let now = Instant::now();
        if next < now {
            next = now + update;
        }
    }
}

/// A display running on its own thread.
pub struct Running {
    stop: mpsc::Sender<()>,
    handle: JoinHandle<Result<(), Error>>,
}

impl Running {
    /// True once the thread has ended (stopped, or failed).
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Asks the thread to stop and waits for it. Returns its error, if any.
    pub fn stop(self) -> Result<(), Error> {
        let _ = self.stop.send(());
        self.join()
    }

    /// Waits for the thread without asking it to stop.
    pub fn join(self) -> Result<(), Error> {
        self.handle.join().unwrap_or_else(|_| Err(Error::Write("display thread panicked".into())))
    }
}

/// Starts a display on a new thread.
pub fn spawn<S, F>(
    mut sink: S,
    mut driver: Box<dyn Driver>,
    telemetry: Arc<Telemetry>,
    update: Duration,
    mut on_frame: F,
) -> Running
where
    S: Sink + Send + 'static,
    F: FnMut(&Readings) + Send + 'static,
{
    let (stop, stop_rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("deepcool-display".into())
        .spawn(move || run(&mut sink, driver.as_mut(), &telemetry, update, &stop_rx, &mut on_frame))
        .expect("failed to start display thread");
    Running { stop, handle }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{driver_for, Settings};
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct Recorder {
        packets: Arc<Mutex<Vec<Vec<u8>>>>,
        fail_after: Option<usize>,
    }

    impl Sink for Recorder {
        fn write(&mut self, data: &[u8]) -> Result<(), String> {
            let mut packets = self.packets.lock().unwrap();
            if self.fail_after.is_some_and(|n| packets.len() >= n) {
                return Err("unplugged".into());
            }
            packets.push(data.to_vec());
            Ok(())
        }
    }

    #[test]
    fn writes_init_then_frames_and_stops_promptly() {
        let recorder = Recorder::default();
        let packets = recorder.packets.clone();
        let driver = driver_for(10, &Settings::default()).unwrap();
        let telemetry = Arc::new(Telemetry::new(None));
        let running = spawn(recorder, driver, telemetry, Duration::from_millis(100), |_| {});
        thread::sleep(Duration::from_millis(350));
        let started = Instant::now();
        running.stop().unwrap();
        assert!(started.elapsed() < Duration::from_millis(50), "stop must not wait for the next tick");
        let packets = packets.lock().unwrap();
        // 2 init packets + about 3 frames.
        assert!((4..=6).contains(&packets.len()), "{} packets", packets.len());
        assert_eq!(packets[0][4], 2);
        assert_eq!(packets[2][4], 11);
    }

    #[test]
    fn write_failure_ends_the_thread_with_an_error() {
        let recorder = Recorder { fail_after: Some(3), ..Recorder::default() };
        let driver = driver_for(10, &Settings::default()).unwrap();
        let running =
            spawn(recorder, driver, Arc::new(Telemetry::new(None)), Duration::from_millis(100), |_| {});
        thread::sleep(Duration::from_millis(400));
        assert!(running.is_finished());
        assert_eq!(running.join(), Err(Error::Write("unplugged".into())));
    }
}
