//! Reads live data from an AMD, Intel, or NVIDIA GPU.

mod amd;
mod intel;
mod nvidia;
pub mod pci;

use pci::{PciDevice, Vendor};

pub enum Gpu {
    Amd(amd::Gpu),
    Intel(intel::Gpu),
    Nvidia(nvidia::Gpu),
    /// No GPU is monitored. Holds the reason shown to the user.
    None(String),
}

impl Gpu {
    pub fn new(pci_device: Option<&PciDevice>) -> Self {
        let Some(gpu) = pci_device else {
            return Gpu::None("No supported GPU was found".into());
        };
        match gpu.vendor {
            Vendor::Amd => amd::Gpu::new(&gpu.address).map(Gpu::Amd).unwrap_or_else(|| {
                Gpu::None(format!("Failed to access GPU sensors (AMD) PCI_ADDR={}", gpu.address))
            }),
            Vendor::Intel => intel::Gpu::new(&gpu.address)
                .map(Gpu::Intel)
                .unwrap_or_else(|| Gpu::None("Intel GPU sensor not found".into())),
            Vendor::Nvidia => nvidia::Gpu::new(&gpu.address).map(Gpu::Nvidia).unwrap_or_else(|| {
                Gpu::None(format!(
                    "NVIDIA GPU library (libnvidia-ml) was not found or could not access PCI_ADDR={}",
                    gpu.address
                ))
            }),
        }
    }

    pub fn is_available(&self) -> bool {
        !matches!(self, Gpu::None(_))
    }

    /// Why the GPU cannot be monitored, or a note about limited support.
    pub fn problem(&self) -> Option<String> {
        match self {
            Gpu::None(reason) => Some(reason.clone()),
            Gpu::Intel(intel) if intel.shared_sensor => {
                Some("Intel Xe driver is not fully supported on iGPUs yet".into())
            }
            _ => None,
        }
    }

    /// GPU temperature in millidegrees Celsius.
    pub fn temp_milli(&self) -> Option<u32> {
        match self {
            Gpu::Amd(amd) => amd.temp_milli(),
            Gpu::Intel(intel) => intel.temp_milli(),
            Gpu::Nvidia(nvidia) => nvidia.temp_milli(),
            Gpu::None(_) => None,
        }
    }

    pub fn usage(&self) -> u8 {
        match self {
            Gpu::Amd(amd) => amd.usage(),
            Gpu::Intel(intel) => intel.usage(),
            Gpu::Nvidia(nvidia) => nvidia.usage(),
            Gpu::None(_) => 0,
        }
    }

    pub fn power_w(&self) -> u16 {
        match self {
            Gpu::Amd(amd) => amd.power_w(),
            Gpu::Intel(intel) => intel.power_w(),
            Gpu::Nvidia(nvidia) => nvidia.power_w(),
            Gpu::None(_) => 0,
        }
    }

    /// Used and total video memory in bytes, when the driver reports it.
    pub fn memory(&self) -> Option<(u64, u64)> {
        match self {
            Gpu::Amd(amd) => amd.memory(),
            Gpu::Nvidia(nvidia) => nvidia.memory(),
            Gpu::Intel(_) | Gpu::None(_) => None,
        }
    }

    pub fn frequency_mhz(&self) -> u16 {
        match self {
            Gpu::Amd(amd) => amd.frequency_mhz(),
            Gpu::Intel(intel) => intel.frequency_mhz(),
            Gpu::Nvidia(nvidia) => nvidia.frequency_mhz(),
            Gpu::None(_) => 0,
        }
    }
}

/// Picks the GPU to monitor.
///
/// With `id = Some((vendor, n))`, picks the nth dedicated GPU of that vendor
/// (n starts at 1), or the integrated GPU when n is 0. Returns `None` if no
/// GPU matches. Without an id, picks the first dedicated GPU, else the iGPU.
pub fn select(gpus: &[PciDevice], id: Option<(Vendor, u8)>) -> Option<PciDevice> {
    match id {
        Some((vendor, 0)) => gpus.first().filter(|gpu| gpu.vendor == vendor && gpu.bus == 0).cloned(),
        Some((vendor, n)) => gpus.iter().filter(|gpu| gpu.vendor == vendor && gpu.bus > 0).nth(n as usize - 1).cloned(),
        None => gpus.iter().find(|gpu| gpu.bus > 0).or_else(|| gpus.first()).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(vendor: Vendor, bus: u8, address: &str) -> PciDevice {
        PciDevice { vendor, bus, address: address.into(), name: address.into() }
    }

    #[test]
    fn selects_gpus_like_the_cli() {
        let gpus = [
            gpu(Vendor::Amd, 0, "igpu"),
            gpu(Vendor::Nvidia, 1, "nv1"),
            gpu(Vendor::Nvidia, 2, "nv2"),
        ];
        assert_eq!(select(&gpus, None).unwrap().address, "nv1");
        assert_eq!(select(&gpus, Some((Vendor::Nvidia, 2))).unwrap().address, "nv2");
        assert_eq!(select(&gpus, Some((Vendor::Amd, 0))).unwrap().address, "igpu");
        assert!(select(&gpus, Some((Vendor::Nvidia, 3))).is_none());
        assert!(select(&gpus, Some((Vendor::Intel, 0))).is_none());
        assert_eq!(select(&gpus[..1], None).unwrap().address, "igpu");
        assert!(select(&[], None).is_none());
    }
}
