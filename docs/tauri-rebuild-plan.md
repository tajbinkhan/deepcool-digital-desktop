# Plan: Rebuild DeepCool Digital with Tauri

Goal: replace the egui desktop app (`gui/`) with a Tauri 2 app whose UI matches the reference
screenshots in `screenshots/`. It should also use less CPU and memory than today's design.

## 1. What the screenshots require

Window: 960×640, light theme, white background with light-grey content area (`#f5f6f7`-ish), teal accent
(`#2a9d96`-ish, sampled exactly from the PNGs during build). Custom title bar with the DeepCool wordmark and
minimise/maximise/close buttons. Left icon rail with Dashboard, Computer, Device and (at the bottom) Settings.
The active item has a teal icon and a teal bar on its left.

| Screen | Content |
|---|---|
| **Dashboard** (`104613`, `104649`) | **CPU card:** model name, a 270° arc gauge for load with 0 °C / 120 °C labels and a red tick, plus clock speed, temperature and power. **GPU card:** the same layout with a dropdown to switch GPU (NVIDIA / AMD) and a GPU memory reading of used/total GB. **Memory card:** hatched teal bar showing used/total and %, frequency and usage. **Storage card:** read/write MB/s and a hatched bar per disk. **Network card:** up/down KB/s. |
| **Computer Configuration** (`104617`) | Device name, OS and build, CPU, every GPU, motherboard, memory (vendor, type, size) and storage drives (model and serial) |
| **Device** (`104626`) | Card per connected cooler: big index "01", series name, category ("Digital Water Cooler"), product image, three live stats and a chevron |
| **Device detail** (`104629`) | Breadcrumb "Device / LD-Series". Settings on the left (e.g. "Leading Zero Display" toggle), on the right a live preview of the cooler's 7-segment display with the readings below it |
| **Settings** (`104635`, `104638`) | Auto-start at login toggle, °C/°F radio, language dropdown (English, Deutsch, Español, Français, Italiano, Português, Русский), version, "Check for Updates", "Official Website", automatic update toggle |

Asset rule: the DeepCool logo and product renders in the screenshots belong to DeepCool. We will not cut them
out of the PNGs. We will draw our own SVG wordmark/icon (the repo already has `packaging/deepcool-digital.svg`)
and our own SVG illustration of each device family. The LD display preview is drawn live as SVG 7-segment
digits anyway. "DeepCreative" text in Settings becomes "DeepCool Digital".

## 2. Architecture

```
deepcool-digital/            (Cargo workspace)
├── crates/core/             lib: devices/*, monitor/* (moved from src/), no CLI, no process exit()
├── src/main.rs              thin CLI binary over core (kept for scripts and service users)
├── app/                     Tauri 2 app
│   ├── src/                 frontend: Svelte 5 + TypeScript + Vite, plain CSS tokens
│   └── src-tauri/           Rust backend: commands, events, tray, autostart, updater
├── helpers/                 deepcool-digital-update, deepcool-digital-cpu-power (moved from gui/src/bin)
└── xtask/                   packaging for Arch and tar.gz (deb/rpm/AppImage come from tauri bundler)
```

### Optimisation over the current design

1. **No controller subprocesses.** Today the GUI spawns one `deepcool-digital-linux` process per device,
   and each one polls sensors on its own. The new backend links `core` directly and runs each display as a
   thread with a stop flag, which also gets rid of the stdout parsing, ANSI stripping and the stop-grace/kill logic.
2. **One sensor sampler.** A single `Telemetry` thread reads `/proc/stat`, `/proc/meminfo`, `/proc/diskstats`,
   `/proc/net/dev`, hwmon, RAPL and GPU once per tick into an `Arc<RwLock<Snapshot>>`. Display drivers and the UI
   both read that snapshot. Device drivers need refactoring to take a snapshot instead of owning a `Cpu`.
3. **Pay for the UI only while it is visible.** The sampler emits one `telemetry` event per second only while
   the window is shown. When the window closes to the tray, the webview is **destroyed**, not hidden. Only the
   Rust process stays (~10–15 MB). The tray recreates the window on demand.
4. **Small frontend.** Svelte compiles away its framework; there is no UI library and no chart library (gauges
   and bars are hand-written SVG/CSS). Target bundle under 100 KB gzipped.
5. GPU sources: NVML loaded with `libloading` for NVIDIA (already done this way), sysfs for AMD
   (`gpu_busy_percent`, `mem_info_vram_*`, `pp_dpm_sclk`), and existing Intel code.

Honest trade-off: on Linux, Tauri uses WebKitGTK. With the window open it will use more RAM than egui
(expect ~100–150 MB versus ~40 MB). It costs less than today only when running in the background (point 3).
The gain is a UI that matches the screenshots exactly, which egui cannot easily do.

### Backend API (Tauri commands and events)

- `get_system_info() -> SystemInfo` (Computer page; cached once)
- `list_gpus()`, `select_gpu(id)`
- event `telemetry` → `{cpu:{load,mhz,temp,watts}, gpu:{id,load,mhz,temp,mem_used,mem_total}, mem:{used,total,mhz},
  storage:{read_bps,write_bps,disks:[{name,used,total}]}, net:{up_bps,down_bps}}`
- `list_devices() -> [{pid, series, category, capabilities, running, settings}]` (hidapi enumerate + hot-plug rescan every 5 s)
- `get_device_settings(pid)`, `set_device_settings(pid, settings)` (applies live; restarts that display's thread)
- `get_preferences()`, `set_preferences({autostart, fahrenheit, language, auto_update})`
- `check_update()`, `install_update()` (reuses the existing polkit + `SHA256SUMS` flow from `gui/src/updater.rs`)
- `cpu_power_status()`, `enable_cpu_power()` (existing polkit helper)
- `open_website()` (GitHub repo URL)

Settings files stay at `~/.config/deepcool-digital/settings.json` and use the same format, so existing users keep
their per-device profiles. The language and temperature unit are added to the preferences file.

### Data sources for the Computer and Memory screens

- Hostname: `/proc/sys/kernel/hostname`. OS: `/etc/os-release` and `uname -r`.
- Board: `/sys/class/dmi/id/board_{vendor,name}`.
- Drives: `/sys/block/*/device/{model,serial}` and NVMe `/sys/class/nvme/*/{model,serial}`.
- DIMM vendor/type/speed need SMBIOS type 17, which can be read only by root (`/sys/firmware/dmi/tables/DMI`).
  We extend the existing root helper with a `dmi` command that runs once and caches the result to
  `~/.cache`. Until then, the app shows the total size and "—" for frequency. **Needs your decision (see §5).**

## 3. Phases

| # | Phase | Output | Done when |
|---|---|---|---|
| 0 | Prep | Install `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev`; `cargo install tauri-cli@^2`; sample exact colours, sizes and fonts from screenshots into `app/src/tokens.css` | `cargo tauri info` is clean |
| 1 | Extract `core` lib | `crates/core`; CLI rebuilt on it; drivers return `Result` instead of calling `exit()`; stop flag per driver | `cargo test --workspace` passes, the CLI still drives the LD cooler |
| 2 | Telemetry sampler | `Snapshot` and sampler thread, plus new readers for disk, network, memory and GPU memory | unit tests against fixture `/proc` files; values within ±5 % of `htop`/`nvtop` |
| 3 | Tauri shell | Frameless 960×640 window, custom title bar (`data-tauri-drag-region`), icon rail, router, i18n (7 JSON locales) | All four pages navigate; window drag/min/max/close work on GNOME Wayland and KDE X11 |
| 4 | Dashboard | CPU/GPU gauge component, hatched progress bar, five cards, GPU dropdown | Pixel overlay against `104613.png` at 100 % zoom shows no visible drift |
| 5 | Computer page | `SystemInfo` + page | Matches `104617.png` layout |
| 6 | Devices | Device list and detail; per-family settings driven by `Capabilities` (mode, secondary, rotate, alarm, °F, leading zeros, interval); SVG previews: LD 7-seg (exact), others generic | Toggling Leading Zero changes both the preview and the physical display within one tick |
| 7 | Settings, tray, background | Autostart (XDG `.desktop`, `--background`), unit, language, updater, tray (open / stop all and quit), single instance (`tauri-plugin-single-instance`), destroying the webview on close | Matches `104635/104638.png`; RSS after closing to tray is ≤ 20 MB |
| 8 | Packaging and CI | `tauri.conf.json` bundles deb/rpm/AppImage with the udev rules, polkit policy and helpers; xtask keeps Arch and tar.gz; release workflow on Ubuntu 22.04 | All formats install on Ubuntu 24.04, Fedora and Arch VMs; upgrading from 0.15.x keeps settings |
| 9 | Remove egui | Delete `gui/`; update README, GUI.md, RELEASING.md; bump to 1.0.0 | No reference to eframe/ksni remains |

Order: 1 → 2 → 3, then 4–7 can overlap, then 8 → 9. The old egui app keeps shipping until phase 8 passes.

## 4. Verification

- `cargo test --workspace`, plus `vitest` for frontend formatting and i18n key coverage.
- Screenshot comparison: a `DEEPCOOL_SCREENSHOT` mode (as today) captures each page with mocked
  telemetry, and the result is diffed against the reference PNGs.
- Real hardware: the LD-series cooler (PID 10) on this machine; the other families are checked against the fake-HID tests.
- Resource budget (measured with `ps -o rss,%cpu`): window open ≤ 160 MB RSS / ≤ 2 % CPU; tray-only
  ≤ 20 MB / ≤ 0.5 % CPU. Builds run one at a time (15 GB RAM rule).

## 5. Decisions needed from you

1. **Frontend framework:** Svelte 5 (recommended: smallest output) or React/Vue if you prefer.
2. **egui app:** fully replace it (recommended) or keep it as a fallback package.
3. **Memory vendor/frequency:** add a one-time root (polkit) DMI read, or show only total size.
4. **Device families without a reference screenshot** (AK, AG, CH, LS, LP, LQ…): use a generic preview card
   in the same style (recommended), or should LD-only be the first release?

## Status (2026-11-11)

| Phase | State |
|---|---|
| 0 Prep | Done (system packages installed; Tauri CLI comes from npm) |
| 1 Core library | Done: `crates/core`, CLI rebuilt on it, tested on the LD-Series |
| 2 Telemetry | Done: memory, disks, network, GPU memory, system info (`crates/core/src/system.rs`) |
| 3 Tauri shell | Done: `app/` (Svelte 5), frameless window, rail, 7 languages |
| 4–6 Pages | Done and checked against the reference screenshots in a browser preview |
| 7 Settings/tray/background | Partly: preferences, auto-start, tray, background mode done; **update check not wired yet** |
| 8 Packaging | Not started |
| 9 Remove egui | Not started |

Known differences from the reference, on purpose:
- Own wordmark and device illustrations instead of DeepCool's logo and product photos.
- The detail page labels the power reading "CPU Power" (the reference says "CPU Frequency" for a value in W).
- Memory frequency shows "--": reading it needs root (decided: total size only for now).
- Extra controls the reference does not have: "Show readings on this device", "Update Interval", and a "Background" section in Settings.

### Measured on this machine (release build, LD-Series running, 1 s updates)

| | Memory (PSS) | CPU |
|---|---|---|
| New app, window open | 193 MB (app 89 + web view 89 + network 15) | ~1 % of one core |
| New app, background (tray only) | 45 MB, no web view | ~0.1 % |
| Installed egui app, background | 24 MB (GUI 6 + controller 18) | ~0.1 % |

The background target (≤ 20 MB) is not met: the tray keeps GTK loaded. If that matters, the
display threads could move into a small separate service that the window talks to.
