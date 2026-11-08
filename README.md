# DeepCool Digital Desktop

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
![Platform: Linux](https://img.shields.io/badge/platform-Linux-informational)
![Written in Rust](https://img.shields.io/badge/written%20in-Rust-orange)

Control the digital displays of DeepCool CPU coolers and cases on Linux, from a desktop app.
Pick your device, choose what it shows, and let it run in the background — including automatically
when you sign in. Everything is written in Rust.

![DeepCool Digital desktop app](docs/screenshot.png)

> This project is a fork of [**deepcool-digital-linux**](https://github.com/Nortank12/deepcool-digital-linux)
> by **Nortank12** and its contributors, who wrote the device controller that talks to the hardware.
> It adds a desktop app, background and sign-in support, Linux packages and some fixes.
> See [Credits](#credits).

## Table of contents

- [Features](#features)
- [Supported devices](#supported-devices)
- [Install](#install)
- [Set up the app](#set-up-the-app)
- [Run in the background and at sign-in](#run-in-the-background-and-at-sign-in)
- [Command-line controller](#command-line-controller)
- [Running as a system service instead](#running-as-a-system-service-instead)
- [CPU power readings](#cpu-power-readings)
- [Build from source](#build-from-source)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [Credits](#credits)
- [License](#license)

## Features

- **Desktop app** with device discovery, per-device settings and a live activity log. No terminal needed.
- **Only valid choices**: display modes, rotation, °F, alarm and leading zeros are shown only where the
  selected device supports them.
- **Runs in the background**: closing the window keeps your displays running. Opening the app again brings
  the window back; only one copy ever runs.
- **Starts automatically**: mark a display to start with its saved settings, and have the app start without
  a window when you sign in. Displays that stop unexpectedly (for example after being unplugged) are retried.
- **CPU power on the display**: one button (with your administrator password) lets the display show CPU
  watts on systems that restrict the CPU energy counter.
- **Several devices at once**, each with its own settings.
- **No root needed**: packages install udev rules that give your desktop session access to the device.
- **Packages** for Debian/Ubuntu (`.deb`), Fedora/openSUSE (`.rpm`), Arch (`.pkg.tar.zst`), plus a portable
  `.tar.gz` and AppImage.
- **The original command-line controller** is included for scripts and services, with a fix so it keeps
  running when the CPU power sensor can't be read.

## Supported devices

Device support comes from the controller of the original project.

| CPU air coolers | | CPU liquid coolers | | Cases | |
|---|:-:|---|:-:|---|:-:|
| AG300 DIGITAL | ✅ | LD240 | ✅ | CH170 DIGITAL | ✔️ |
| AG400 DIGITAL | ✅ | LD360 | ✅ | CH270 DIGITAL | ✔️ |
| AG500 DIGITAL | ✅ | LP240 | ✔️ | CH360 DIGITAL | ✅ |
| AG620 DIGITAL | ✅ | LP360 | ✔️ | CH510 MESH DIGITAL | ✅ |
| AK400 DIGITAL / DIGITAL SE | ✅ | LQ240 | ✅ | CH560 DIGITAL | ✅ |
| AK400 DIGITAL PRO | ✅ | LQ360 | ✅ | CH690 DIGITAL | ✔️ |
| AK400 G2 DIGITAL NYX | ✅ | LS520 SE DIGITAL | ✅ | MORPHEUS | ✅ |
| AK500 DIGITAL | ✅ | LS720 SE DIGITAL | ✅ | | |
| AK500 DIGITAL PRO | ✅ | | | | |
| AK500 G2 DIGITAL NYX | ✅ | | | | |
| AK500S DIGITAL / DIGITAL SE | ✅ | | | | |
| AK620 DIGITAL / DIGITAL SE | ✅ | | | | |
| AK620 DIGITAL PRO | ✅ | | | | |
| AK620 G2 DIGITAL NYX | ✅ | | | | |
| AK700 DIGITAL NYX | ✅ | | | | |
| ASSASSIN IV VC VISION | ✅ | | | | |

✅ Fully supported · ✔️ Partially supported (some display modes are unavailable on the device)

The MYSTIQUE series (LCD screens) is not supported. If your device is missing, run the app anyway and select
**Refresh devices**: it may still be detected. USB IDs and display mappings are documented in
[device-list](device-list/README.md).

## Install

Download the package for your distribution from the
[Releases page](https://github.com/tajbinkhan/deepcool-digital-desktop/releases), or
[build it yourself](#build-from-source).

**Debian, Ubuntu, Linux Mint, Pop!_OS**

```bash
sudo apt install ./deepcool-digital-gui_*_amd64.deb
```

**Fedora, openSUSE**

```bash
sudo dnf install ./deepcool-digital-gui-*.x86_64.rpm
```

**Arch Linux, Manjaro, EndeavourOS**

```bash
sudo pacman -U deepcool-digital-gui-*-x86_64.pkg.tar.zst
```

**Any distribution (portable)**: extract the `.tar.gz` and run `AppRun`, or, where a release includes one,
make the AppImage executable and run it. Portable builds cannot install the USB access rules for you; install them once
as described in [Troubleshooting](#the-app-says-usb-access-is-denied-or-no-device-is-found).

The packages install:

| Path | What it is |
|---|---|
| `/usr/bin/deepcool-digital-gui` | The desktop app (also in your app menu as **DeepCool Digital**) |
| `/usr/bin/deepcool-digital-linux` | The command-line controller |
| `/usr/lib/deepcool-digital/deepcool-digital-cpu-power` | Helper the app uses to [allow CPU power readings](#cpu-power-readings) |
| `/usr/lib/udev/rules.d/70-deepcool-digital.rules` | Gives your desktop session access to DeepCool USB devices |

Requirements: a graphical Linux desktop (Wayland or X11) with OpenGL. No Python, GTK or other runtime is
needed.

## Set up the app

1. Open **DeepCool Digital** from your app menu. Do not run it as root.
2. Your device appears in the list. If it doesn't, unplug and reconnect it once (so the new USB rule applies)
   and select **Refresh devices**.
3. Choose the **display mode** and other options. Controls the device doesn't support are disabled.
4. Select **Save settings**, then **Start display**.

To change settings while a display is running, select **Stop display**, change them, then **Start display**
again. If you have several devices, select each one in turn and start it.

Settings are saved per device in `~/.config/deepcool-digital/settings.json`. The activity log is kept in
`~/.local/state/deepcool-digital/activity.log`.

## Run in the background and at sign-in

The **Automatic start** section of the window controls this:

| Setting | What it does |
|---|---|
| **Start this display automatically** | Per device. Starts the display with its saved settings whenever the app is running, and retries every 15 seconds if it stops unexpectedly. **Stop display** pauses this for that device until you select **Start display** again. |
| **Keep displays running after I close this window** | On by default. Closing the window leaves the app running without a window. Open **DeepCool Digital** from the app menu to show it again. **Stop all and quit** stops everything and exits. |
| **Start DeepCool Digital in the background when I sign in** | Starts the app without a window when you sign in, which then starts the displays marked to start automatically. |

For displays that come on by themselves after every restart, turn on **Start this display automatically**
and **Start DeepCool Digital in the background when I sign in**.

The app starts when your desktop session starts. If your system signs you in automatically, that is at
boot. It runs as your user rather than as a system service, because the USB rules give access to the
signed-in desktop session. If you need the display before anyone signs in, see
[Running as a system service instead](#running-as-a-system-service-instead).

The sign-in entry is the file `~/.config/autostart/deepcool-digital.desktop`. It runs
`deepcool-digital-gui --background`.

## Command-line controller

The controller from the original project is installed as `deepcool-digital-linux`. The desktop app uses it
behind the scenes, and you can use it directly in scripts.

```
Usage: deepcool-digital-linux [OPTIONS]

Options:
  -m, --mode <MODE>       Change the display mode of your device
  -s, --secondary <MODE>  Change the secondary display mode of your device (if supported)
      --pid <ID>          Specify the Product ID if multiple devices are connected
      --gpuid <VENDOR:ID> Specify the nth GPU of a specific vendor to monitor (use ID 0 for integrated GPU)

  -u, --update <MILLISEC> Change the update interval of the display [default: 1000]
  -f, --fahrenheit        Change the temperature unit to °F
  -a, --alarm             Enable the alarm
  -r, --rotate <DEGREE>   Rotate the display (LP Series only)
  -z, --zeros             Display leading zeros (LD Series only)

Commands:
  -l, --list         Print Product ID of the connected devices
  -g, --gpulist      Print all available GPUs
  -h, --help         Print help
  -v, --version      Print version
```

With several devices connected, list them and run one controller per product ID:

```bash
deepcool-digital-linux --list
```

```
Device list [PID | Name]
-----
4 | AK500S-DIGITAL
7 | MORPHEUS
```

```bash
deepcool-digital-linux --pid 4
```

The desktop app's background and sign-in features replace the need for a hand-written service in most
cases.

## Running as a system service instead

A system service starts at boot, before anyone signs in, and runs as root. Use it only if you don't want the
desktop app's automatic start, and **never run both at once**: two programs writing to the same display
conflict.

<details>
<summary><b>systemd (Debian, Ubuntu, Fedora, Arch and most others)</b></summary>

Create `/etc/systemd/system/deepcool-digital.service`:

```ini
[Unit]
Description=DeepCool Digital

[Service]
ExecStart=/usr/bin/deepcool-digital-linux
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
# On headless servers, use this target instead if your GPU is not detected
# WantedBy=graphical.target
```

Add options such as `--pid 4 --mode cpu_temp` to `ExecStart` as needed, then enable it:

```bash
sudo systemctl enable --now deepcool-digital
```

</details>

<details>
<summary><b>OpenRC (Gentoo, Artix and others)</b></summary>

Create `/etc/init.d/deepcool-digital`:

```sh
#!/sbin/openrc-run

description="DeepCool Digital"
command="/usr/bin/deepcool-digital-linux"
command_args=""
command_background=1
pidfile="/run/deepcool-digital.pid"
```

```bash
sudo chmod +x /etc/init.d/deepcool-digital
```

```bash
sudo rc-update add deepcool-digital default
```

</details>

NixOS users can use the `services.hardware.deepcool-digital-linux` module from Nixpkgs, which packages the
original project; see its [README](https://github.com/Nortank12/deepcool-digital-linux#installation).

## CPU power readings

Displays that show CPU power (watts) need to read the CPU's energy counter, for example
`/sys/class/powercap/intel-rapl:0/energy_uj`. Most distributions let only root read it, so by default the
display shows 0 W. The **CPU power** section of the app tells you which case applies:

- **CPU power readings are available**: nothing to do.
- **Only administrators can read the CPU energy counter**: select **Allow CPU power readings…** and enter
  your administrator password. The app makes the counter readable now and after every restart, and restarts
  running displays so they show CPU power. **Turn off** in the same place restores the default.
- **No supported CPU energy sensor**: your kernel exposes none of the supported counters (`intel_rapl`,
  `amd_energy` or `zenergy`), so CPU power can't be shown.

Allowing it makes the energy counter readable by every user on the computer. Linux restricts it because
precise energy readings have been used in side-channel attacks
([CVE-2020-8694](https://nvd.nist.gov/vuln/detail/CVE-2020-8694)); on a personal computer the risk is low.

Behind the button, the helper `/usr/lib/deepcool-digital/deepcool-digital-cpu-power` runs as root through
polkit. It writes `/etc/udev/rules.d/71-deepcool-digital-cpu-power.rules` and changes the counters'
permissions. Uninstalling the `.deb` or `.rpm` removes that rule. Without polkit (or from the portable
build), run it yourself:

```bash
sudo /usr/lib/deepcool-digital/deepcool-digital-cpu-power enable
```

Use `disable` to undo it, or `status` to check.

## Build from source

The repository is a Cargo workspace:

| Folder | Contents |
|---|---|
| `src/` | The device controller (`deepcool-digital-linux`), from the original project |
| `gui/` | The desktop app (`deepcool-digital-gui`), built with [egui](https://github.com/emilk/egui) |
| `xtask/` | The packaging tool, run as `cargo xtask package` |
| `packaging/` | udev rules, desktop entry, icon and install script used by the packages |
| `device-list/` | USB IDs and display mappings for each device family |

### 1. Install the build dependencies

You need [Rust](https://rustup.rs/) (stable), a C toolchain, `pkg-config` and the libudev headers. The desktop
app needs no extra development libraries.

<details>
<summary><b>Debian / Ubuntu</b></summary>

```bash
sudo apt install build-essential pkg-config libudev-dev curl
```

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

</details>

<details>
<summary><b>Fedora</b></summary>

```bash
sudo dnf install gcc pkgconf-pkg-config systemd-devel rustup
```

```bash
rustup-init
```

</details>

<details>
<summary><b>Arch Linux</b></summary>

```bash
sudo pacman -S base-devel rustup
```

```bash
rustup default stable
```

</details>

### 2. Build and run

```bash
git clone https://github.com/tajbinkhan/deepcool-digital-desktop.git
```

```bash
cd deepcool-digital-desktop
```

```bash
cargo build --release
```

```bash
./target/release/deepcool-digital-gui
```

The app looks for the controller next to its own executable, so both binaries in `target/release/` work
together. To use the USB device without root when running from source, install the rules once:

```bash
sudo cp packaging/70-deepcool-digital.rules /usr/lib/udev/rules.d/ && sudo udevadm control --reload-rules
```

Then unplug and reconnect the device.

### 3. Test

```bash
cargo test --workspace
```

With a supported device connected, this starts and stops the real display through the app:

```bash
cargo test -p deepcool-digital-gui -- --ignored
```

### 4. Build packages

```bash
cargo xtask package --formats deb,arch,tar
```

Packages and a `SHA256SUMS` file are written to `dist/`. The available formats are `deb`, `rpm`, `arch`, `tar`
and `appimage`, and the default is `deb,rpm,arch,tar`. They need these tools:

| Format | Tools |
|---|---|
| `deb` | `dpkg-deb`, `dpkg-shlibdeps` |
| `rpm` | `rpmbuild` |
| `arch` | `tar`, `zstd` |
| `tar` | `tar` |
| `appimage` | `mksquashfs` and the official [type2 runtime](https://github.com/AppImage/type2-runtime/releases), passed with `--runtime <path>` |

Packages use the build machine's architecture and its glibc version as the minimum. Build on the oldest
distribution you want to support. The package version comes from `gui/Cargo.toml`.

More details about the app, packaging and test hooks are in [GUI.md](GUI.md).

## Troubleshooting

#### The app says USB access is denied, or no device is found

- Unplug and reconnect the device, then select **Refresh devices**.
- Check that the rule is installed: `/usr/lib/udev/rules.d/70-deepcool-digital.rules`. Portable builds and
  source builds need it installed manually (see [Build and run](#2-build-and-run)).
- The rule gives access to the signed-in desktop session. Over SSH or on a system without systemd-logind,
  give access another way, or run the command-line controller as root.

#### The display flickers or shows the wrong values

Another program may also be driving it. Stop any `deepcool-digital` system service
(`sudo systemctl disable --now deepcool-digital`) and any other copy of the controller.

#### CPU power shows 0 W

Use **Allow CPU power readings…** in the app; see [CPU power readings](#cpu-power-readings). If the
password prompt doesn't appear, check that polkit is installed (`pkexec` on Debian and Ubuntu).

#### Where are the logs?

`~/.local/state/deepcool-digital/activity.log`. The last two older logs are kept as `activity.log.1` and
`activity.log.2`.

## Contributing

Bug reports and pull requests are welcome. When reporting a device problem, include the output of
`deepcool-digital-linux --list` and the relevant part of the activity log. Support for new devices
belongs in the controller (`src/`); consider sending it to the
[original project](https://github.com/Nortank12/deepcool-digital-linux) as well, so everyone benefits.

Before opening a pull request:

```bash
cargo fmt -p deepcool-digital-gui -p xtask && cargo clippy --workspace --all-targets && cargo test --workspace
```

The controller in `src/` keeps the original project's formatting so upstream changes stay easy to merge.

Maintainers: see [RELEASING.md](RELEASING.md) for how to publish a new version.

## Credits

- **[Nortank12](https://github.com/Nortank12)** and the contributors of
  [deepcool-digital-linux](https://github.com/Nortank12/deepcool-digital-linux): the device controller,
  device support, protocol research and device documentation this project is built on.
- **[Tajbin Khan](https://github.com/tajbinkhan)**: the desktop app, background and sign-in support,
  single-instance handling, CPU power access, packaging tool and CPU energy sensor fix.

This project is not affiliated with or endorsed by DeepCool.

## License

DeepCool Digital Desktop is free and open-source software, licensed under the
**GNU General Public License v3.0**. The full text is in [LICENSE](LICENSE).

```
Copyright (C) Nortank12 and deepcool-digital-linux contributors
Copyright (C) 2026 Tajbin Khan

This program is free software: you can redistribute it and/or modify it under the terms of the
GNU General Public License as published by the Free Software Foundation, version 3.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without
even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.
```

The original project is GPL-3.0 licensed, and this project is a modified version of it, so it is
GPL-3.0 as well. You may use, study, share and change it. If you distribute it or a modified version,
you must provide the source code under the same license and keep these copyright notices.
