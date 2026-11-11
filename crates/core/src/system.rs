//! System-wide readings for the dashboard and the computer page: memory, disks,
//! network and static hardware information. Everything is read from `/proc`
//! and `/sys` without root.

use std::{
    ffi::CString,
    fs::{read_dir, read_to_string},
    mem::MaybeUninit,
    path::Path,
};

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    let text = read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Memory use in bytes. "Used" is total minus what the kernel says is available.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub used: u64,
}

pub fn parse_meminfo(text: &str) -> Memory {
    let field = |name: &str| -> Option<u64> {
        let line = text.lines().find(|line| line.starts_with(name))?;
        let kb = line[name.len()..].trim().trim_end_matches("kB").trim().parse::<u64>().ok()?;
        Some(kb * 1024)
    };
    let total = field("MemTotal:").unwrap_or(0);
    let available = field("MemAvailable:").unwrap_or(total);
    Memory { total, used: total.saturating_sub(available) }
}

pub fn memory() -> Memory {
    read_to_string("/proc/meminfo").map(|text| parse_meminfo(&text)).unwrap_or_default()
}

/// Cumulative byte counters. Rates come from the difference between two readings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    pub read: u64,
    pub written: u64,
}

impl Counters {
    /// Bytes per second between two readings `secs` apart. Counter resets give 0.
    pub fn rate(&self, later: &Counters, secs: f64) -> (u64, u64) {
        if secs <= 0.0 {
            return (0, 0);
        }
        let per_sec = |a: u64, b: u64| (b.saturating_sub(a) as f64 / secs).round() as u64;
        (per_sec(self.read, later.read), per_sec(self.written, later.written))
    }
}

/// True for whole physical disks (not partitions, loop, RAM or device-mapper devices).
fn is_physical_disk(name: &str) -> bool {
    Path::new("/sys/block").join(name).join("device").exists()
}

/// Sums sectors read/written over the given disk names. Sectors are always 512 bytes in diskstats.
pub fn parse_diskstats(text: &str, disks: &[String]) -> Counters {
    let mut total = Counters::default();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 || !disks.iter().any(|d| d == fields[2]) {
            continue;
        }
        total.read += fields[5].parse::<u64>().unwrap_or(0) * 512;
        total.written += fields[9].parse::<u64>().unwrap_or(0) * 512;
    }
    total
}

/// Sums received (`read`) and transmitted (`written`) bytes over the given interfaces.
pub fn parse_net_dev(text: &str, interfaces: &[String]) -> Counters {
    let mut total = Counters::default();
    for line in text.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if !interfaces.iter().any(|i| i == name.trim()) {
            continue;
        }
        let fields: Vec<u64> = rest.split_whitespace().filter_map(|f| f.parse().ok()).collect();
        if fields.len() >= 9 {
            total.read += fields[0];
            total.written += fields[8];
        }
    }
    total
}

/// Physical network interfaces (those backed by a device; skips lo, VPNs, bridges and containers).
pub fn physical_interfaces() -> Vec<String> {
    let Ok(entries) = read_dir("/sys/class/net") else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().join("device").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub fn network_counters(interfaces: &[String]) -> Counters {
    read_to_string("/proc/net/dev").map(|text| parse_net_dev(&text, interfaces)).unwrap_or_default()
}

pub fn disk_counters(disks: &[String]) -> Counters {
    read_to_string("/proc/diskstats").map(|text| parse_diskstats(&text, disks)).unwrap_or_default()
}

/// A physical disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    /// Kernel name, e.g. `nvme0n1`.
    pub name: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    /// Size of the whole disk in bytes.
    pub size: u64,
}

/// Lists physical disks, sorted by name.
pub fn disks() -> Vec<Disk> {
    let Ok(entries) = read_dir("/sys/block") else {
        return Vec::new();
    };
    let mut disks: Vec<Disk> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| is_physical_disk(name) && !name.starts_with("sr"))
        .map(|name| {
            let base = Path::new("/sys/block").join(&name);
            let size = read_trim(base.join("size")).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) * 512;
            let device = base.join("device");
            // NVMe namespaces keep model/serial on the controller (`device`); SATA keeps them on the device.
            let model = read_trim(device.join("model"));
            let serial = read_trim(device.join("serial")).or_else(|| {
                read_trim(device.join("vpd_pg80")).map(|s| s.trim_matches(|c: char| !c.is_ascii_graphic()).to_owned())
            });
            Disk { name, model, serial, size }
        })
        .filter(|disk| disk.size > 0)
        .collect();
    disks.sort_by(|a, b| a.name.cmp(&b.name));
    disks
}

/// Filesystem space in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Space {
    pub total: u64,
    pub used: u64,
}

fn statvfs(path: &str) -> Option<Space> {
    let c_path = CString::new(path).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    let block = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * block;
    let free = stat.f_bfree as u64 * block;
    Some(Space { total, used: total.saturating_sub(free) })
}

/// Decodes the octal escapes (`\040` for space) used in `/proc/self/mounts`.
fn unescape_mount(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            if let Ok(code) = u8::from_str_radix(&path[i + 1..i + 4], 8) {
                out.push(code);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Space used on a disk: the sum over its mounted partitions (each mounted once).
/// Partitions that are not mounted (for example another OS) are not counted.
pub fn disk_space(disk: &str) -> Space {
    let Ok(mounts) = read_to_string("/proc/self/mounts") else {
        return Space::default();
    };
    let mut seen: Vec<String> = Vec::new();
    let mut space = Space::default();
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let (Some(source), Some(target)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Some(device) = source.strip_prefix("/dev/") else {
            continue;
        };
        let on_disk = device == disk
            || device.strip_prefix(disk).is_some_and(|rest| {
                let rest = rest.strip_prefix('p').unwrap_or(rest);
                !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
            });
        if !on_disk || seen.iter().any(|s| s == device) {
            continue;
        }
        seen.push(device.to_owned());
        if let Some(part) = statvfs(&unescape_mount(target)) {
            space.total += part.total;
            space.used += part.used;
        }
    }
    space
}

/// Static hardware and OS information for the computer page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemInfo {
    pub hostname: String,
    pub os: String,
    pub cpu: String,
    pub gpus: Vec<String>,
    pub motherboard: Option<String>,
    pub memory_total: u64,
    pub disks: Vec<Disk>,
}

/// Turns "Gigabyte Technology Co., Ltd." into "GIGABYTE"-style short vendor names.
fn short_vendor(vendor: &str) -> String {
    let lower = vendor.to_ascii_lowercase();
    for (needle, short) in [
        ("gigabyte", "GIGABYTE"),
        ("asustek", "ASUS"),
        ("micro-star", "MSI"),
        ("asrock", "ASRock"),
        ("dell", "Dell"),
        ("lenovo", "Lenovo"),
        ("hewlett", "HP"),
    ] {
        if lower.contains(needle) {
            return short.to_owned();
        }
    }
    vendor.trim_end_matches(['.', ',']).to_owned()
}

pub fn system_info() -> SystemInfo {
    let hostname = read_trim("/proc/sys/kernel/hostname").unwrap_or_default();
    let pretty = read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "Linux".into());
    let kernel = read_trim("/proc/sys/kernel/osrelease").unwrap_or_default();
    let arch = std::env::consts::ARCH;
    let os = format!("{pretty} ({arch}) Kernel {kernel}");
    let motherboard = read_trim("/sys/class/dmi/id/board_name").map(|board| {
        match read_trim("/sys/class/dmi/id/board_vendor") {
            Some(vendor) => format!("{} {board}", short_vendor(&vendor)),
            None => board,
        }
    });
    SystemInfo {
        hostname: hostname.to_uppercase(),
        os,
        cpu: crate::monitor::cpu::get_name().unwrap_or_else(|| "Unknown CPU".into()),
        gpus: crate::monitor::gpu::pci::get_gpu_list().into_iter().map(|g| g.name).collect(),
        motherboard,
        memory_total: memory().total,
        disks: disks(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo() {
        let text = "MemTotal:       31413872 kB\nMemFree:        17120472 kB\nMemAvailable:   25170960 kB\n";
        let m = parse_meminfo(text);
        assert_eq!(m.total, 31413872 * 1024);
        assert_eq!(m.used, (31413872 - 25170960) * 1024);
    }

    #[test]
    fn diskstats_sums_whole_disks_only() {
        let text = " 259       0 nvme0n1 5372744 1997385 296216920 1148392 3712972 4668556 91025098 22526962 0 81833 23690079\n 259       1 nvme0n1p1 10 0 100 0 0 0 0 0 0 0 0\n   7       0 loop0 1 0 8 0 0 0 0 0 0 0 0\n";
        let c = parse_diskstats(text, &["nvme0n1".into()]);
        assert_eq!(c, Counters { read: 296216920 * 512, written: 91025098 * 512 });
    }

    #[test]
    fn net_dev_skips_unlisted() {
        let text = "Inter-|   Receive\n face |bytes\n    lo: 2247215    9265    0    0    0     0          0         0  2247215    9265    0    0    0     0       0          0\n  enp7s0: 1000 5 0 0 0 0 0 0 2000 6 0 0 0 0 0 0\n";
        assert_eq!(parse_net_dev(text, &["enp7s0".into()]), Counters { read: 1000, written: 2000 });
    }

    #[test]
    fn rates() {
        let a = Counters { read: 1000, written: 0 };
        let b = Counters { read: 3000, written: 500 };
        assert_eq!(a.rate(&b, 2.0), (1000, 250));
        assert_eq!(b.rate(&a, 1.0), (0, 0));
    }

    #[test]
    fn mount_escapes() {
        assert_eq!(unescape_mount("/run/media/my\\040disk"), "/run/media/my disk");
        assert_eq!(unescape_mount("/plain"), "/plain");
    }

    #[test]
    fn vendors() {
        assert_eq!(short_vendor("Gigabyte Technology Co., Ltd."), "GIGABYTE");
        assert_eq!(short_vendor("Framework"), "Framework");
    }

    #[test]
    fn reads_this_machine() {
        let info = system_info();
        assert!(info.memory_total > 0);
        assert!(!info.os.is_empty());
        let _ = disk_space("nvme0n1");
    }
}
