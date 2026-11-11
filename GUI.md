# DeepCool Digital desktop app

A desktop control panel for DeepCool Digital displays, built with [Tauri](https://tauri.app) (Rust backend,
Svelte interface). Launch **DeepCool Digital** from your desktop's application menu after installing a
native package. No terminal is needed.

## What it does

- **Dashboard**: CPU and GPU load, temperature, clock and power (GPU memory too), memory use, disk read and
  write speed and space, and network speed. Choose which GPU to show when there are several.
- **Computer**: device name, operating system, CPU, GPUs, motherboard, memory and drives.
- **Device**: each connected DeepCool device, with its own settings and a live preview of the display.
  Only the settings the device supports are shown.
- **Settings**: start at sign-in, °C or °F, language (English, Deutsch, Español, Français, Italiano,
  Português, Русский), updates, running in the background, and CPU power readings.

## How it works

The repository is a Cargo workspace:

| Folder | Contents |
|---|---|
| `crates/core/` | Device drivers (packet encoders), sensor readers, the shared telemetry reader, system information, CPU power access and update logic. Nothing in it prints or ends the process. |
| `crates/helpers/` | `deepcool-digital-cpu-power` and `deepcool-digital-update`, the only parts that run as root (through polkit). |
| `src/` | The command-line controller `deepcool-digital-linux`, built on `crates/core`. |
| `app/src-tauri/` | The app's backend: commands, display threads, dashboard sampling, tray, settings files, updater. |
| `app/src/` | The interface (Svelte 5). In a plain browser it runs with sample data, for layout work. |
| `xtask/` | The packaging tool. |

Displays run as threads inside the app, not as separate processes. One shared reader samples the sensors;
displays that update at the same moment share one reading. The dashboard samples once a second only while
the window is visible. The window and its web view exist only while the window is open: closing it to the
tray frees them, leaving just the display threads running.

Settings are stored in `$XDG_CONFIG_HOME/deepcool-digital/` (normally `~/.config`): `settings.json` per
product ID, in the same format as earlier versions, and `preferences.json` for app-wide choices. A device
seen for the first time shows readings straight away; its **Show readings on this device** switch is saved
in its profile.

## Background, sign-in and tray

- **Keep displays running after closing the window** (on by default): closing the window keeps the app in
  the tray while any display runs. The tray menu has **Open DeepCool Digital** and **Stop displays and
  quit**. Only one copy of the app runs; opening it again shows the existing window.
- **Boot DeepCool Digital Auto-start** writes `~/.config/autostart/deepcool-digital.desktop`, which runs
  `deepcool-digital-gui --background` (no window). With nothing to show, that background start exits.
- Displays that stop unexpectedly (for example after unplugging) start again within about 15 seconds.

The app runs as your user, not as a system service, because the USB access rules grant access to the
signed-in desktop session.

## Installation

- Debian / Ubuntu: open the `.deb` in your software installer.
- Fedora / openSUSE: open the `.rpm` in your software installer.
- Arch: `sudo pacman -U <package>`.
- Portable `.tar.gz`: extract and open `AppRun`. It needs WebKitGTK 4.1 and `libayatana-appindicator3`
  installed.

Native packages install session-based USB access rules and reapply them to connected devices during
installation. Reconnect the device if access remains denied. Do not run the app as root.

The app is installed as `/usr/lib/deepcool-digital/deepcool-digital-gui` (linked from `/usr/bin`), next to
the controller and the two helpers. The polkit policy allows exactly those two helper paths.

## CPU power readings

The CPU energy counter is usually readable only by root. When it is, the dashboard shows **Allow** next to
CPU Power, and **Settings → CPU Power Readings** has a switch. Both run
`/usr/lib/deepcool-digital/deepcool-digital-cpu-power enable` (or `disable`) through `pkexec`. The helper
writes `/etc/udev/rules.d/71-deepcool-digital-cpu-power.rules` and makes the counters readable immediately;
the app finds the counter again without restarting. The `.deb` and `.rpm` remove the rule on uninstall.

## Updates

With automatic updates on, the app checks
`https://api.github.com/repos/tajbinkhan/deepcool-digital-desktop/releases/latest` at start and every six
hours; it never installs on its own. **Update now** downloads the package matching how the app was installed
(detected with `dpkg-query`, `rpm` or `pacman`), verifies it against the release's `SHA256SUMS`, and runs
`/usr/lib/deepcool-digital/deepcool-digital-update install <file>` through polkit. The helper only accepts a
regular file containing the `deepcool-digital-gui` package in the installed format. **Restart now** stops the
displays and replaces the process with the new version.

## Building

Build dependencies: Rust, a C toolchain, `pkg-config`, libudev, WebKitGTK 4.1, `libayatana-appindicator3`
and librsvg development packages, and Node.js 22.12+ with pnpm.

```bash
pnpm --dir app install && pnpm --dir app build
cargo build --release --features deepcool-digital-app/custom-protocol
./target/release/deepcool-digital-app
```

`pnpm --dir app tauri dev` runs the app with live reload. The `custom-protocol` feature embeds the built
interface; without it the app loads the dev server at `http://localhost:1420`.

Build packages with `cargo xtask package` (needs `dpkg-deb`, `dpkg-shlibdeps`, `rpmbuild`, `tar` and `zstd`;
select formats with `--formats deb,rpm,arch,tar`). The package version comes from `app/src-tauri/Cargo.toml`.
Releases are built by GitHub Actions on Ubuntu 22.04; see [RELEASING.md](RELEASING.md).

## Verification

- `cargo test --workspace`: drivers' packet layouts, auto-mode timing, the runner (stopping promptly, write
  failures), sensor maths, `/proc` parsers, settings files, CPU power access, and the updater (download,
  checksum, install through a stand-in helper, damaged downloads).
- `pnpm --dir app check`: type-checks the interface.
- `cargo run -p deepcool-core --example readings`: prints live sensor readings.
- `DEEPCOOL_PRETEND_VERSION=0.1.0` makes the app believe it is that version, to try the update flow.
