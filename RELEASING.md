# Releasing DeepCool Digital Desktop

This guide is for maintainers. It explains how to publish a new version on the
[Releases page](https://github.com/tajbinkhan/deepcool-digital-desktop/releases).

Releases are built by GitHub Actions, not on your own computer. When you push a version tag such as `v0.13.0`,
the [release workflow](.github/workflows/release.yml):

1. checks that the tag matches the version in `gui/Cargo.toml`,
2. runs all tests,
3. builds the `.deb`, `.rpm`, Arch `.pkg.tar.zst` and portable `.tar.gz` packages plus `SHA256SUMS`, and
4. attaches them to a **draft** release.

Nothing becomes public until you review the draft and publish it.

The packages are built inside an Ubuntu 22.04 container, so they need glibc 2.35 or newer and run on
Ubuntu 22.04+, Debian 12+, Fedora 36+ and Arch. Packages built on a newer machine with `cargo xtask package`
need that machine's glibc version, so don't upload local builds to a release.

## Contents

- [Before you start](#before-you-start)
- [1. Choose the version number](#1-choose-the-version-number)
- [2. Update the version](#2-update-the-version)
- [3. Check everything locally](#3-check-everything-locally)
- [4. Commit and push](#4-commit-and-push)
- [5. Tag the release](#5-tag-the-release)
- [6. Wait for the build](#6-wait-for-the-build)
- [7. Review the draft](#7-review-the-draft)
- [8. Write the release notes and publish](#8-write-the-release-notes-and-publish)
- [9. After publishing](#9-after-publishing)
- [If something goes wrong](#if-something-goes-wrong)
- [Test builds without releasing](#test-builds-without-releasing)
- [Release checklist](#release-checklist)

## Before you start

You need:

- push access to the repository,
- Git, and the Rust toolchain to run the checks locally,
- optionally the [GitHub CLI](https://cli.github.com/) (`gh`), signed in with `gh auth login`. Every `gh` step
  below can also be done on the GitHub website.

Start from an up-to-date `main` branch with no uncommitted changes:

```bash
git switch main && git pull
```

```bash
git status
```

## 1. Choose the version number

Versions follow [Semantic Versioning](https://semver.org/): `MAJOR.MINOR.PATCH`.

| Change | Example | Bump |
|---|---|---|
| Bug fixes only | Fix a crash, correct a device mapping | Patch: `0.12.0` → `0.12.1` |
| New features or devices, still compatible | Support a new cooler, add a setting | Minor: `0.12.1` → `0.13.0` |
| Breaking changes | Settings file format changes, removed options | Major (after 1.0), or minor while below 1.0 |

The app and the controller share one version number.

## 2. Update the version

Change `version` in the `[package]` section of both manifests:

| File | Line |
|---|---|
| `Cargo.toml` | `version = "0.13.0"` (the controller; shown by `deepcool-digital-linux --version`) |
| `gui/Cargo.toml` | `version = "0.13.0"` (the app; used for package names and checked against the tag) |

Leave `xtask/Cargo.toml` alone: the packaging tool is not released.

Then update `Cargo.lock` so it records the new versions:

```bash
cargo update --workspace
```

`git diff` should now show only the two version lines and the two matching entries in `Cargo.lock`.

## 3. Check everything locally

Run the same checks as the release workflow, plus formatting and lint:

```bash
cargo fmt -p deepcool-digital-gui -p xtask --check
```

```bash
cargo clippy --workspace --all-targets
```

Formatting is checked for the app and the packaging tool only. The controller in `src/` keeps the original
project's formatting so upstream changes stay easy to merge. Clippy should report no errors; the controller
has some existing style warnings from upstream.

```bash
cargo test --workspace
```

If a DeepCool device is connected, also test the real hardware path:

```bash
cargo build --release && cargo test -p deepcool-digital-gui -- --ignored
```

Optionally, build and install the `.deb` locally and try the app. This checks the app, not the release
packages (see the glibc note above):

```bash
cargo xtask package --formats deb
```

```bash
sudo apt install ./dist/deepcool-digital-gui_0.13.0_amd64.deb
```

Things worth trying by hand:

- the device appears, and **Start display** / **Stop display** work,
- **Save settings** persists after restarting the app,
- with **Keep displays running after I close this window** on, closing the window keeps the display on, and
  opening the app from the menu brings the window back,
- **Start this display automatically** starts the display after **Stop all and quit** and reopening,
- **Start DeepCool Digital in the background when I sign in** starts the display after signing out and in.

## 4. Commit and push

```bash
git add Cargo.toml gui/Cargo.toml Cargo.lock
```

```bash
git commit -m "Release 0.13.0"
```

```bash
git push origin main
```

## 5. Tag the release

The tag is `v` followed by the exact version from `gui/Cargo.toml`. Use an annotated tag:

```bash
git tag -a v0.13.0 -m "DeepCool Digital Desktop 0.13.0"
```

```bash
git push origin v0.13.0
```

Pushing the tag starts the release workflow.

## 6. Wait for the build

The build takes about four minutes. Follow it on the
[Actions page](https://github.com/tajbinkhan/deepcool-digital-desktop/actions/workflows/release.yml), or in
the terminal:

```bash
gh run watch --repo tajbinkhan/deepcool-digital-desktop
```

If it fails, see [If something goes wrong](#if-something-goes-wrong).

## 7. Review the draft

Open the [Releases page](https://github.com/tajbinkhan/deepcool-digital-desktop/releases). The new release is
marked **Draft** and only people with write access can see it. It should have five files:

| File | For |
|---|---|
| `deepcool-digital-gui_0.13.0_amd64.deb` | Debian, Ubuntu, Linux Mint, Pop!_OS |
| `deepcool-digital-gui-0.13.0-1.x86_64.rpm` | Fedora, openSUSE |
| `deepcool-digital-gui-0.13.0-1-x86_64.pkg.tar.zst` | Arch Linux and derivatives |
| `deepcool-digital-gui-0.13.0-x86_64.tar.gz` | Any distribution (portable) |
| `SHA256SUMS` | Checksums of the files above |

It's worth downloading and checking them before publishing:

```bash
mkdir -p /tmp/release-check && cd /tmp/release-check
```

```bash
gh release download v0.13.0 --repo tajbinkhan/deepcool-digital-desktop
```

```bash
sha256sum -c SHA256SUMS
```

```bash
dpkg-deb -I deepcool-digital-gui_0.13.0_amd64.deb | grep -E "Version|Depends"
```

The `Depends` line should start with `libc6 (>= 2.35)`. A higher number means the build picked up a newer
glibc; don't publish it.

The build log's **Show glibc requirement** step also prints the newest glibc version each binary needs.

## 8. Write the release notes and publish

On the draft, select **Edit**, set the title to `DeepCool Digital Desktop 0.13.0`, and write the notes. Use
this template:

```markdown
One-sentence summary of this release.

## What's new

- New: …
- Improved: …
- Fixed: …

## Install

Download the file for your distribution below, then:

| Distribution | Command |
|---|---|
| Debian, Ubuntu, Linux Mint, Pop!_OS | `sudo apt install ./deepcool-digital-gui_0.13.0_amd64.deb` |
| Fedora, openSUSE | `sudo dnf install ./deepcool-digital-gui-0.13.0-1.x86_64.rpm` |
| Arch Linux, Manjaro, EndeavourOS | `sudo pacman -U deepcool-digital-gui-0.13.0-1-x86_64.pkg.tar.zst` |
| Other (portable) | Extract `deepcool-digital-gui-0.13.0-x86_64.tar.gz` and run `DeepCoolDigital/AppRun` |

Installing over an older version keeps your saved settings. If the app is running in the background,
select **Stop all and quit** first, or sign out and in after installing.

**Requirements:** x86_64, a graphical desktop (Wayland or X11) with OpenGL, and glibc 2.35 or newer.

Verify downloads with `sha256sum -c SHA256SUMS`.

## Credits

Built on [deepcool-digital-linux](https://github.com/Nortank12/deepcool-digital-linux) by Nortank12 and
contributors. Licensed under the GNU GPL v3.0.
```

To list what changed since the last release:

```bash
git log --oneline v0.12.0..v0.13.0
```

Mention anything users must do themselves, such as reconnecting the device after a udev rule change.

Then select **Publish release**. With the GitHub CLI, save the notes as `notes.md` and run:

```bash
gh release edit v0.13.0 --repo tajbinkhan/deepcool-digital-desktop --title "DeepCool Digital Desktop 0.13.0" --notes-file notes.md --draft=false --latest
```

## 9. After publishing

- Open the release page while signed out (or in a private window) and check that the downloads work.
- If the README mentions version numbers or features that changed, update it.
- Device support changes in `src/` may help the
  [original project](https://github.com/Nortank12/deepcool-digital-linux) too; consider opening a pull request
  there.

## If something goes wrong

#### "Tag … does not match gui/Cargo.toml version"

The tag and the version in `gui/Cargo.toml` differ. Delete the tag (below), fix the version or the tag name,
and tag again.

#### Tests or packaging fail

Open the failed run on the Actions page and read the failing step's log. Fix the problem on `main`, then move
the tag to the fixed commit:

```bash
git push --delete origin v0.13.0 && git tag -d v0.13.0
```

If a draft release was already created, delete it too:

```bash
gh release delete v0.13.0 --repo tajbinkhan/deepcool-digital-desktop --yes
```

Then commit and push the fix, and repeat [step 5](#5-tag-the-release).

Only move or delete a tag whose release was **never published**. Once a release is public, people may have
downloaded it, so never replace its files or reuse its tag. Publish a new patch version instead.

#### A published release has a serious bug

Release a fixed patch version as soon as possible. Meanwhile, you can edit the broken release to add a
warning at the top of its notes, or mark it as a pre-release so it is no longer shown as the latest.

#### The workflow shows deprecation warnings

GitHub periodically retires runner images and Node.js versions used by actions. If the workflow warns about
them, update the action versions in [.github/workflows/release.yml](.github/workflows/release.yml) (for
example `actions/checkout`, `actions/upload-artifact` and `softprops/action-gh-release`) and run a
[test build](#test-builds-without-releasing). The build itself runs in the `ubuntu:22.04` container, so the
host runner's Ubuntu version doesn't affect the packages. Raising the container version raises the minimum
glibc the packages need; update the requirements in the README and release notes if you do.

## Test builds without releasing

To check that the workflow still works, for example after changing it or the packaging, run it by hand. This
builds and tests everything and stores the packages as a downloadable artifact, without creating a release.

On the website: **Actions** → **Release** → **Run workflow** → choose the branch → **Run workflow**. When it
finishes, download **packages** from the run's summary page.

With the GitHub CLI:

```bash
gh workflow run release.yml --repo tajbinkhan/deepcool-digital-desktop --ref main
```

## Release checklist

- [ ] On an up-to-date `main` with no uncommitted changes
- [ ] Version updated in `Cargo.toml` and `gui/Cargo.toml`, and `cargo update --workspace` run
- [ ] `cargo fmt -p deepcool-digital-gui -p xtask --check`, `cargo clippy --workspace --all-targets` (no errors) and
      `cargo test --workspace` pass
- [ ] App tried by hand (device, start/stop, settings, background, automatic start)
- [ ] Version commit pushed to `main`
- [ ] Tag `vX.Y.Z` created and pushed
- [ ] Release workflow succeeded
- [ ] Draft has the five files, checksums verify, `.deb` needs `libc6 (>= 2.35)`
- [ ] Release notes written and release published
- [ ] Downloads checked from the public release page
