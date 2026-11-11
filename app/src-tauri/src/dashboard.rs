//! Live readings for the dashboard, sampled once a second while the window is shown.

use deepcool_core::{
    monitor::{cpu, gpu},
    system::{self, Counters},
    telemetry::{Needs, Readings, Sample, Telemetry},
};
use serde::Serialize;
use std::{sync::Arc, time::Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuView {
    pub name: String,
    pub load: u8,
    pub mhz: u16,
    /// °C; `None` without a sensor.
    pub temp: Option<f32>,
    /// Watts; `None` without a readable energy counter.
    pub power: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuView {
    pub index: usize,
    pub name: String,
    pub load: u8,
    pub mhz: u16,
    pub temp: Option<f32>,
    pub power: u16,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskView {
    pub name: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub cpu: CpuView,
    pub gpus: Vec<String>,
    pub gpu: Option<GpuView>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_read: u64,
    pub disk_write: u64,
    pub disks: Vec<DiskView>,
    pub net_up: u64,
    pub net_down: u64,
}

/// Disk space only changes slowly; refresh it every few ticks.
const SPACE_EVERY: u32 = 5;

pub struct Dashboard {
    telemetry: Arc<Telemetry>,
    cpu_name: String,
    gpus: Vec<(String, gpu::Gpu)>,
    selected: usize,
    disk_names: Vec<String>,
    interfaces: Vec<String>,
    disk_space: Vec<DiskView>,
    tick: u32,
    previous: Option<(Instant, Sample, Counters, Counters)>,
}

impl Dashboard {
    pub fn new(telemetry: Arc<Telemetry>) -> Self {
        let list = gpu::pci::get_gpu_list();
        // Default to the GPU the displays would pick: the first dedicated one.
        let auto = gpu::select(&list, None);
        let selected = auto.and_then(|a| list.iter().position(|g| *g == a)).unwrap_or(0);
        let gpus = list.iter().map(|g| (g.name.clone(), gpu::Gpu::new(Some(g)))).collect();
        Dashboard {
            telemetry,
            cpu_name: cpu::get_name().unwrap_or_else(|| "Unknown CPU".into()),
            gpus,
            selected,
            disk_names: system::disks().into_iter().map(|d| d.name).collect(),
            interfaces: system::physical_interfaces(),
            disk_space: Vec::new(),
            tick: 0,
            previous: None,
        }
    }

    pub fn select_gpu(&mut self, index: usize) {
        if index < self.gpus.len() {
            self.selected = index;
        }
    }

    /// Forget the last sample, e.g. after the window was hidden for a while.
    pub fn reset(&mut self) {
        self.previous = None;
    }

    /// Takes a reading. The first call after a reset reports zero rates.
    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let sample = self.telemetry.sample(Needs { cpu_temp: true, cpu_power: true, cpu_freq: true, gpu: false });
        let disk = system::disk_counters(&self.disk_names);
        let net = system::network_counters(&self.interfaces);
        let (readings, (disk_read, disk_write), (net_down, net_up)) = match &self.previous {
            Some((at, before, disk_before, net_before)) => {
                let secs = now.duration_since(*at).as_secs_f64();
                (Readings::between(before, &sample), disk_before.rate(&disk, secs), net_before.rate(&net, secs))
            }
            None => (Readings::between(&sample, &sample), (0, 0), (0, 0)),
        };
        self.previous = Some((now, sample, disk, net));

        if self.tick % SPACE_EVERY == 0 || self.disk_space.is_empty() {
            self.disk_space = system::disks()
                .into_iter()
                .map(|d| {
                    let space = system::disk_space(&d.name);
                    DiskView { name: d.model.unwrap_or(d.name), used: space.used, total: space.total }
                })
                .filter(|d| d.total > 0)
                .collect();
        }
        self.tick = self.tick.wrapping_add(1);

        let status = self.telemetry.status();
        let memory = system::memory();
        let gpu = self.gpus.get(self.selected).map(|(name, g)| {
            let memory = g.memory();
            GpuView {
                index: self.selected,
                name: name.clone(),
                load: g.usage(),
                mhz: g.frequency_mhz(),
                temp: g.temp_milli().map(|t| t as f32 / 1000.0),
                power: g.power_w(),
                mem_used: memory.map(|m| m.0),
                mem_total: memory.map(|m| m.1),
            }
        });
        Snapshot {
            cpu: CpuView {
                name: self.cpu_name.clone(),
                load: readings.cpu_usage,
                mhz: readings.cpu_mhz,
                temp: readings.cpu_temp_milli.map(|t| t as f32 / 1000.0),
                power: status.cpu_power.then_some(readings.cpu_power_w),
            },
            gpus: self.gpus.iter().map(|(name, _)| name.clone()).collect(),
            gpu,
            mem_used: memory.used,
            mem_total: memory.total,
            disk_read,
            disk_write,
            disks: self.disk_space.clone(),
            net_up,
            net_down,
        }
    }
}
