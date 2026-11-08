# DeepCool Digital desktop app

A native desktop control panel written entirely in Rust (using [egui](https://github.com/emilk/egui)),
which drives the existing Rust controller. Launch **DeepCool Digital** from your desktop's application menu
after installing a native package. No terminal is needed for normal device control. Select a device, choose
its settings and click **Start display**. Stop before applying new settings. Multiple different product IDs
can run at once.

Only supported controls are enabled for each device. Settings are saved per product ID under
`$XDG_CONFIG_HOME/deepcool-digital/settings.json` (normally `~/.config`); settings saved by the earlier
Python version are read unchanged. The activity panel shows controller output and USB permission errors, and
rotating diagnostic logs are saved to `$XDG_STATE_HOME/deepcool-digital/activity.log`
(normally `~/.local/state/deepcool-digital/activity.log`).

## Running in the background and at sign-in

The **Automatic start** section of the window has three settings:

- **Start this display automatically** (per device, saved immediately): the display starts with its saved
  settings whenever the app is running, including right after sign-in. If it stops unexpectedly (for example
  the device was unplugged) the app tries again every 15 seconds. **Stop display** pauses this for the device
  until you select **Start display** again.
- **Keep displays running after I close this window** (on by default): closing the window leaves the app
  running without a window while any display is running or set to start automatically. Open
  **DeepCool Digital** from the app menu again to show the window; only one copy of the app runs per session.
  **Stop all and quit** stops every display and exits.
- **Start DeepCool Digital in the background when I sign in**: adds
  `~/.config/autostart/deepcool-digital.desktop`, which runs `deepcool-digital-gui --background` when you sign
  in. With no display set to start automatically, that background start simply exits.

"At sign-in" means when your desktop session starts. With automatic login enabled, that is at system startup.
The app runs as your user, not as a system service, because the USB access rules grant access to the active
desktop session.

## Installation

- Debian / Ubuntu: open the `.deb` in your software installer.
- Fedora / openSUSE: open the `.rpm` in your software installer.
- Arch: install the `.pkg.tar.zst` using a graphical package manager supporting local packages, or
  `sudo pacman -U <package>`.
- AppImage: mark executable in your file manager and open it.
- Portable `.tar.gz`: extract and open `AppRun`.

The app needs no Python or GTK. At runtime it loads the desktop's OpenGL/EGL, Wayland or X11, and
xkbcommon libraries, which every graphical Linux desktop already provides; native packages declare them.

Native packages install session-based USB access rules and reapply them to connected DeepCool devices
during installation. Reconnect the device if access remains denied. The rules grant access to the active
local desktop session using logind/udev; systems without that mechanism need administrator-provided device
access. Portable packages include the rules at `usr/lib/udev/rules.d/70-deepcool-digital.rules`; an
administrator must install them to the system udev rules directory and reload rules once. Do not run the GUI
as root.

CPU power readings need read access to the CPU energy counter (for example
`/sys/class/powercap/intel-rapl:0/energy_uj`), which is usually root-only; see the main README. Without it,
the display runs and simply leaves CPU power out. This package does not change those permissions. If an older
DeepCool system service is running, stop/disable it before starting this app to avoid two programs writing
to the same USB display.

The controller identifies devices by product ID, so two identical devices cannot be addressed separately.
Unplugged devices require stopping, reconnecting and refreshing.

## Building

The repository is a Cargo workspace: the controller is the root crate, the desktop app is `gui/`, and
`xtask/` holds the packaging tool. Everything is Rust.

Build dependencies: Rust toolchain, pkg-config and libudev development headers (for the controller).
The desktop app needs no system development packages.

Run from source: `cargo build --release --workspace`, then `./target/release/deepcool-digital-gui`.
The app finds the controller next to its own executable, in `../lib/deepcool-digital/`, on `PATH`, or at
the path in `DEEPCOOL_BACKEND`.

Build packages with `cargo xtask package` (the `xtask/` crate). This requires `dpkg-deb`, `dpkg-shlibdeps`,
`rpmbuild`, `tar`, and `zstd`. Select a subset using `--formats deb,rpm,arch,tar`. Add `appimage` (which also
needs `mksquashfs`) and `--runtime /path/to/runtime-x86_64` to create an AppImage using the official AppImage type2 runtime. Use the
runtime matching the build architecture. The package version comes from `gui/Cargo.toml`. Outputs and
SHA-256 checksums are in `dist/`. The builder uses the host CPU architecture; cross-compilation is not
implied.

Releases are built by GitHub Actions (`.github/workflows/release.yml`): pushing a tag such as `v0.12.0` that
matches the version in `gui/Cargo.toml` builds deb, rpm, Arch and tar.gz packages in Ubuntu 22.04
(glibc 2.35) and attaches them to a draft release to review and publish. Step-by-step instructions are in [RELEASING.md](RELEASING.md).

These formats cover major distribution families, not every Linux release. Native libraries impose minimum
versions. Build on the oldest intended distribution and validate on each supported target before publishing.
Flatpak and Snap are not supplied: sandboxed USB/raw HID access and system udev rule installation require
separate integration and distribution-store policies.

## Verification

`cargo test --workspace` tests capability restrictions, argument validation, discovery parsing, profile
persistence, the activity log, controller start/stop (including one that ignores the stop request), and a
full panel workflow against a fake controller.

With a supported device connected, `cargo build --release && cargo test -p deepcool-digital-gui -- --ignored`
starts and stops the real display through the panel.

`DEEPCOOL_SMOKE_TEST=1 ./target/release/deepcool-digital-gui` opens the real window, waits for device
discovery and exits. `DEEPCOOL_SCREENSHOT=/path/preview.png` saves a screenshot and then closes the window
(the app keeps running in the background if a display is set to start automatically). Run either in a
desktop session.
