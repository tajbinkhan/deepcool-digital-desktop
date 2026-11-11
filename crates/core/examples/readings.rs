//! Prints live readings once a second: `cargo run -p deepcool-core --example readings`.

use deepcool_core::{
    monitor::gpu,
    telemetry::{Needs, Readings, Telemetry},
};
use std::{thread::sleep, time::Duration};

fn main() {
    let gpus = gpu::pci::get_gpu_list();
    let selected = gpu::select(&gpus, None);
    println!("GPU: {}", selected.as_ref().map_or("none", |g| g.name.as_str()));
    let telemetry = Telemetry::new(selected.as_ref());
    println!("{:?}", telemetry.status());
    let mut before = telemetry.sample(Needs::ALL);
    for _ in 0..3 {
        sleep(Duration::from_secs(1));
        let after = telemetry.sample(Needs::ALL);
        let r = Readings::between(&before, &after);
        println!(
            "CPU {:>3}% {:>3}°C {:>4} W {:>5} MHz | GPU {:>3}% {:>3}°C {:>4} W {:>5} MHz",
            r.cpu_usage, r.cpu_temp(false), r.cpu_power_w, r.cpu_mhz,
            r.gpu.usage, r.gpu_temp(false), r.gpu.power_w, r.gpu.mhz
        );
        before = after;
    }
}
