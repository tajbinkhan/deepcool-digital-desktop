//! Project automation. Run with `cargo xtask <command>`.
//!
//! `package` builds native and portable Linux packages without installing anything
//! on the host. Outputs and SHA-256 checksums are written to `dist/`.

use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::{exit, Command},
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

const NAME: &str = "deepcool-digital-gui";
const BINARIES: [&str; 4] = [
    "deepcool-digital-gui",
    "deepcool-digital-linux",
    "deepcool-digital-cpu-power",
    "deepcool-digital-update",
];
/// Binaries also linked into /usr/bin; the helpers are only run by the app, through polkit.
const COMMANDS: [&str; 2] = ["deepcool-digital-gui", "deepcool-digital-linux"];
/// Written by the app when the user allows CPU power readings; removed with the package.
const CPU_POWER_RULE: &str = "/etc/udev/rules.d/71-deepcool-digital-cpu-power.rules";
const FORMATS: [&str; 5] = ["deb", "rpm", "arch", "tar", "appimage"];
const USAGE: &str = "\
Usage: cargo xtask package [OPTIONS]

Build native and portable Linux packages into dist/.

Options:
  --formats <LIST>   Comma separated: deb,rpm,arch,tar,appimage [default: deb,rpm,arch,tar]
  --runtime <PATH>   Official AppImage type2 runtime for this architecture (needed for appimage)
  --skip-build       Package the existing release binaries without running cargo build
  -h, --help         Print help";

// The window loads these with dlopen, so package tools cannot detect them.
// OpenGL/EGL draws the window; the rest covers Wayland and X11 sessions.
const DEB_RUNTIME: &str = "libegl1, libgl1, libwayland-client0, libwayland-egl1, libx11-6, libx11-xcb1, \
                           libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, libxrender1";
const RPM_RUNTIME: [&str; 11] = [
    "libEGL.so.1",
    "libGL.so.1",
    "libwayland-client.so.0",
    "libwayland-egl.so.1",
    "libX11.so.6",
    "libX11-xcb.so.1",
    "libXcursor.so.1",
    "libXi.so.6",
    "libxkbcommon.so.0",
    "libxkbcommon-x11.so.0",
    "libXrender.so.1",
];
const ARCH_RUNTIME: [&str; 8] = [
    "libglvnd",
    "wayland",
    "libx11",
    "libxcursor",
    "libxi",
    "libxkbcommon",
    "libxkbcommon-x11",
    "libxrender",
];

struct Options {
    formats: Vec<String>,
    runtime: Option<PathBuf>,
    skip_build: bool,
}

fn parse(mut args: impl Iterator<Item = String>) -> std::result::Result<Options, String> {
    let mut options = Options {
        formats: ["deb", "rpm", "arch", "tar"].map(String::from).to_vec(),
        runtime: None,
        skip_build: false,
    };
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
            None => (arg, None),
        };
        let mut value = |name: &str| {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or(format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--formats" => options.formats = value("--formats")?.split(',').map(str::to_owned).collect(),
            "--runtime" => options.runtime = Some(value("--runtime")?.into()),
            "--skip-build" => options.skip_build = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unexpected argument '{other}'")),
        }
    }
    if let Some(unknown) = options.formats.iter().find(|f| !FORMATS.contains(&f.as_str())) {
        return Err(format!("unknown package format '{unknown}'"));
    }
    Ok(options)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        Some("package") => match parse(args) {
            Ok(options) => Packager::new().and_then(|packager| packager.run(&options)),
            Err(message) if message.is_empty() => {
                println!("{USAGE}");
                return;
            }
            Err(message) => {
                eprintln!("error: {message}\n\n{USAGE}");
                exit(2);
            }
        },
        Some("-h" | "--help") | None => {
            println!("{USAGE}");
            return;
        }
        Some(other) => {
            eprintln!("error: unknown command '{other}'\n\n{USAGE}");
            exit(2);
        }
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        exit(1);
    }
}

/// Runs a program and fails if it does not succeed.
fn run(program: &str, args: &[&dyn AsRef<std::ffi::OsStr>], dir: Option<&Path>) -> Result {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let status = command
        .status()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    if !status.success() {
        return Err(format!("{program} failed with {status}").into());
    }
    Ok(())
}

fn copy(source: &Path, target: &Path) -> Result {
    fs::create_dir_all(target.parent().unwrap())?;
    fs::copy(source, target).map_err(|error| format!("could not copy {}: {error}", source.display()))?;
    Ok(())
}

/// Every path below `dir` (not following symbolic links), in a stable order.
fn walk(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        found.push(path.clone());
        if fs::symlink_metadata(&path)?.is_dir() {
            found.extend(walk(&path)?);
        }
    }
    Ok(found)
}

fn copy_tree(source: &Path, target: &Path) -> Result {
    fs::create_dir_all(target)?;
    for path in walk(source)? {
        let destination = target.join(path.strip_prefix(source)?);
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_symlink() {
            symlink(fs::read_link(&path)?, &destination)?;
        } else if meta.is_dir() {
            fs::create_dir_all(&destination)?;
            fs::set_permissions(&destination, meta.permissions())?;
        } else {
            copy(&path, &destination)?;
        }
    }
    Ok(())
}

fn write_executable(path: &Path, contents: &str) -> Result {
    fs::write(path, contents)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Reads `version = "..."` from the `[package]` table of a Cargo manifest.
fn package_version(manifest: &Path) -> Result<String> {
    let text = fs::read_to_string(manifest)?;
    let mut in_package = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if let Some(value) = line.strip_prefix("version").map(str::trim_start) {
            if let (true, Some(value)) = (in_package, value.strip_prefix('=')) {
                return Ok(value.trim().trim_matches('"').to_owned());
            }
        }
    }
    Err(format!("no package version in {}", manifest.display()).into())
}

struct Packager {
    root: PathBuf,
    out: PathBuf,
    build: PathBuf,
    version: String,
    machine: &'static str,
    arch: &'static str,
}

impl Packager {
    fn new() -> Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let machine = std::env::consts::ARCH;
        Ok(Packager {
            version: package_version(&root.join("gui/Cargo.toml"))?,
            out: root.join("dist"),
            build: root.join("build/packages"),
            arch: match machine {
                "x86_64" => "amd64",
                "aarch64" => "arm64",
                other => other,
            },
            machine,
            root,
        })
    }

    fn run(&self, options: &Options) -> Result {
        let wants = |format: &str| options.formats.iter().any(|f| f == format);
        if wants("appimage") && !options.runtime.as_ref().is_some_and(|runtime| runtime.is_file()) {
            return Err(
                "AppImage requires --runtime pointing to an official type2-runtime for this architecture.".into(),
            );
        }
        if !options.skip_build {
            let packages = ["-p", "deepcool-digital-linux", "-p", "deepcool-digital-gui"];
            let mut args: Vec<&dyn AsRef<std::ffi::OsStr>> = vec![&"build", &"--release", &"--locked"];
            args.extend(packages.iter().map(|arg| arg as &dyn AsRef<std::ffi::OsStr>));
            run(env!("CARGO"), &args, Some(&self.root))?;
        }
        fs::create_dir_all(&self.out)?;
        let stage = self.stage()?;
        if wants("deb") {
            self.deb(&stage)?;
        }
        if wants("rpm") {
            self.rpm(&stage)?;
        }
        if wants("arch") {
            self.arch_package(&stage)?;
        }
        if wants("tar") || wants("appimage") {
            let app = self.portable(&stage, wants("tar"))?;
            if let (true, Some(runtime)) = (wants("appimage"), &options.runtime) {
                self.appimage(&app, runtime)?;
            }
        }
        self.checksums()
    }

    /// Lays out the installed file system tree shared by every format.
    fn stage(&self) -> Result<PathBuf> {
        if self.build.exists() {
            fs::remove_dir_all(&self.build)?;
        }
        let stage = self.build.join("root");
        let app = stage.join("usr/lib/deepcool-digital");
        // The GUI finds the controller next to its own resolved path.
        for name in BINARIES {
            copy(&self.root.join("target/release").join(name), &app.join(name))?;
        }
        for name in COMMANDS {
            let link = stage.join("usr/bin").join(name);
            fs::create_dir_all(link.parent().unwrap())?;
            symlink(format!("../lib/deepcool-digital/{name}"), link)?;
        }
        let packaging = self.root.join("packaging");
        for (source, target) in [
            (
                packaging.join("deepcool-digital.desktop"),
                "usr/share/applications/deepcool-digital.desktop",
            ),
            (
                packaging.join("deepcool-digital.svg"),
                "usr/share/icons/hicolor/scalable/apps/deepcool-digital.svg",
            ),
            (
                packaging.join("70-deepcool-digital.rules"),
                "usr/lib/udev/rules.d/70-deepcool-digital.rules",
            ),
            (
                packaging.join("io.github.tajbinkhan.deepcool-digital.policy"),
                "usr/share/polkit-1/actions/io.github.tajbinkhan.deepcool-digital.policy",
            ),
            (self.root.join("LICENSE"), "usr/share/licenses/deepcool-digital-gui/LICENSE"),
            (self.root.join("GUI.md"), "usr/share/doc/deepcool-digital-gui/GUI.md"),
        ] {
            copy(&source, &stage.join(target))?;
        }
        for path in walk(&stage)? {
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_symlink() {
                let executable = meta.is_dir() || meta.permissions().mode() & 0o111 != 0;
                fs::set_permissions(&path, fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }))?;
            }
        }
        Ok(stage)
    }

    /// The USB access refresh script without its interpreter line.
    fn refresh_script(&self) -> Result<String> {
        let script = fs::read_to_string(self.root.join("packaging/refresh-usb-access.sh"))?;
        Ok(script.strip_prefix("#!/bin/sh\n").unwrap_or(&script).to_owned())
    }

    fn deb(&self, stage: &Path) -> Result {
        let control = stage.join("DEBIAN");
        fs::create_dir(&control)?;
        // Derive ELF dependencies from this build; do not claim older glibc compatibility.
        let debian = self.build.join("debian");
        fs::create_dir_all(&debian)?;
        fs::write(
            debian.join("control"),
            "Source: deepcool-digital-gui\nSection: utils\nPriority: optional\n\
             Maintainer: DeepCool contributors <noreply@example.org>\n\n\
             Package: deepcool-digital-gui\nArchitecture: any\nDescription: DeepCool desktop controller\n",
        )?;
        let binaries: Vec<PathBuf> = BINARIES
            .iter()
            .map(|name| stage.join("usr/lib/deepcool-digital").join(name))
            .collect();
        let output = Command::new("dpkg-shlibdeps")
            .args(["--ignore-missing-info", "-O"])
            .args(&binaries)
            .current_dir(&self.build)
            .output()
            .map_err(|error| format!("could not run dpkg-shlibdeps: {error}"))?;
        if !output.status.success() {
            return Err(format!("dpkg-shlibdeps failed: {}", String::from_utf8_lossy(&output.stderr)).into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let dependencies = stdout
            .lines()
            .find_map(|line| line.strip_prefix("shlibs:Depends="))
            .ok_or("dpkg-shlibdeps reported no dependencies")?;
        fs::write(
            control.join("control"),
            format!(
                "Package: {NAME}\nVersion: {}\nArchitecture: {}\n\
                 Maintainer: DeepCool Digital contributors <noreply@example.org>\n\
                 Depends: {dependencies}, {DEB_RUNTIME}\nRecommends: udev, pkexec | policykit-1\nSection: utils\nPriority: optional\n\
                 Description: Native desktop controller for DeepCool Digital USB displays\n",
                self.version, self.arch
            ),
        )?;
        write_executable(
            &control.join("postinst"),
            &format!("#!/bin/sh\nset -e\n{}", self.refresh_script()?),
        )?;
        write_executable(
            &control.join("postrm"),
            &format!(
                "#!/bin/sh\nset -e\nif [ \"$1\" = remove ] || [ \"$1\" = purge ]; then rm -f {CPU_POWER_RULE}; fi\n\
                 if command -v udevadm >/dev/null 2>&1; then udevadm control --reload-rules || true; fi\n"
            ),
        )?;
        let package = self.out.join(format!("{NAME}_{}_{}.deb", self.version, self.arch));
        run("dpkg-deb", &[&"--root-owner-group", &"--build", &stage, &package], None)?;
        fs::remove_dir_all(control)?;
        Ok(())
    }

    fn rpm(&self, stage: &Path) -> Result {
        let top = self.build.join("rpm");
        for folder in ["BUILD", "BUILDROOT", "RPMS", "SOURCES", "SPECS", "SRPMS", "tmp", "db"] {
            fs::create_dir_all(top.join(folder))?;
        }
        let suffix = if matches!(self.machine, "x86_64" | "aarch64") {
            "()(64bit)"
        } else {
            ""
        };
        let requires: Vec<String> = RPM_RUNTIME.iter().map(|lib| format!("{lib}{suffix}")).collect();
        let spec = top.join("SPECS/gui.spec");
        fs::write(
            &spec,
            format!(
                "Name: {NAME}\nVersion: {version}\nRelease: 1\n\
                 Summary: Desktop control panel for DeepCool Digital USB displays\n\
                 License: GPL-3.0-only\nRequires: {requires}\nRecommends: polkit\n\
                 %description\nNative desktop control panel with device discovery and saved settings.\n\
                 %prep\n%build\n%install\nmkdir -p %{{buildroot}}\ncp -a {stage}/usr %{{buildroot}}/\n\
                 %files\n/usr/bin/deepcool-digital-gui\n/usr/bin/deepcool-digital-linux\n/usr/lib/deepcool-digital\n\
                 /usr/lib/udev/rules.d/70-deepcool-digital.rules\n/usr/share/applications/deepcool-digital.desktop\n\
                 /usr/share/icons/hicolor/scalable/apps/deepcool-digital.svg\n/usr/share/licenses/deepcool-digital-gui\n\
                 /usr/share/doc/deepcool-digital-gui\n\
                 /usr/share/polkit-1/actions/io.github.tajbinkhan.deepcool-digital.policy\n\
                 %post\n{refresh}\
                 %postun\nif [ \"$1\" -eq 0 ]; then rm -f {CPU_POWER_RULE}; fi\n\
                 if command -v udevadm >/dev/null 2>&1; then udevadm control --reload-rules || :; fi\n",
                version = self.version,
                requires = requires.join(", "),
                stage = stage.display(),
                refresh = self.refresh_script()?,
            ),
        )?;
        let define = |name: &str, value: &Path| format!("{name} {}", value.display());
        let (topdir, tmppath, dbpath) = (
            define("_topdir", &top),
            define("_tmppath", &top.join("tmp")),
            define("_dbpath", &top.join("db")),
        );
        run(
            "rpmbuild",
            &[
                &"--define",
                &topdir,
                &"--define",
                &tmppath,
                &"--define",
                &dbpath,
                &"--define",
                &"_build_id_links none",
                &"--define",
                &"__os_install_post %{nil}",
                &"-bb",
                &spec,
            ],
            None,
        )?;
        for package in walk(&top.join("RPMS"))? {
            if package.extension().is_some_and(|ext| ext == "rpm") {
                copy(&package, &self.out.join(package.file_name().unwrap()))?;
            }
        }
        Ok(())
    }

    fn arch_package(&self, stage: &Path) -> Result {
        let mut size = 0;
        for path in walk(stage)? {
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_file() {
                size += meta.len();
            }
        }
        let mut info = format!(
            "pkgname = {NAME}\npkgbase = {NAME}\npkgver = {}-1\n\
             pkgdesc = Desktop controller for DeepCool Digital USB displays\nsize = {size}\narch = {}\n\
             license = GPL-3.0-only\ndepend = systemd-libs\ndepend = glibc\ndepend = gcc-libs\n\
             optdepend = polkit: allow CPU power readings from the app\n",
            self.version, self.machine
        );
        for name in ARCH_RUNTIME {
            info.push_str(&format!("depend = {name}\n"));
        }
        let metadata = stage.join(".PKGINFO");
        fs::write(&metadata, info)?;
        let package = self
            .out
            .join(format!("{NAME}-{}-1-{}.pkg.tar.zst", self.version, self.machine));
        run(
            "tar",
            &[
                &"--owner=0",
                &"--group=0",
                &"--numeric-owner",
                &"--zstd",
                &"-cf",
                &package,
                &"-C",
                &stage,
                &".PKGINFO",
                &"usr",
            ],
            None,
        )?;
        fs::remove_file(metadata)?;
        Ok(())
    }

    /// Builds the AppDir, and the portable `.tar.gz` of it when `archive` is set.
    fn portable(&self, stage: &Path, archive: bool) -> Result<PathBuf> {
        let parent = self.build.join("portable");
        let app = parent.join("DeepCoolDigital");
        copy_tree(stage, &app)?;
        write_executable(
            &app.join("AppRun"),
            "#!/bin/sh\nAPPDIR=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\"\n\
             exec \"$APPDIR/usr/lib/deepcool-digital/deepcool-digital-gui\" \"$@\"\n",
        )?;
        for name in ["deepcool-digital.desktop", "deepcool-digital.svg"] {
            copy(&self.root.join("packaging").join(name), &app.join(name))?;
        }
        copy(&self.root.join("GUI.md"), &app.join("README.md"))?;
        if archive {
            let package = self
                .out
                .join(format!("{NAME}-{}-{}.tar.gz", self.version, self.machine));
            run(
                "tar",
                &[
                    &"--owner=0",
                    &"--group=0",
                    &"--numeric-owner",
                    &"-czf",
                    &package,
                    &"-C",
                    &parent,
                    &"DeepCoolDigital",
                ],
                None,
            )?;
        }
        Ok(app)
    }

    fn appimage(&self, app: &Path, runtime: &Path) -> Result {
        let squash = self.build.join("app.squashfs");
        run(
            "mksquashfs",
            &[
                &app,
                &squash,
                &"-root-owned",
                &"-no-xattrs",
                &"-noappend",
                &"-comp",
                &"gzip",
                &"-quiet",
            ],
            None,
        )?;
        let target = self
            .out
            .join(format!("DeepCoolDigital-{}-{}.AppImage", self.version, self.machine));
        let mut output = fs::File::create(&target)?;
        for source in [runtime, squash.as_path()] {
            std::io::copy(&mut fs::File::open(source)?, &mut output)?;
        }
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn checksums(&self) -> Result {
        let mut lines = String::new();
        for path in walk(&self.out)? {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if name == "SHA256SUMS" || !fs::symlink_metadata(&path)?.is_file() {
                continue;
            }
            let mut hasher = Sha256::new();
            let mut file = fs::File::open(&path)?;
            let mut buffer = vec![0; 1 << 16];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            let digest: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
            lines.push_str(&format!("{digest}  {name}\n"));
        }
        fs::File::create(self.out.join("SHA256SUMS"))?.write_all(lines.as_bytes())?;
        println!("Packages and SHA256SUMS written to {}", self.out.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_package_version_only() {
        let dir = std::env::temp_dir().join(format!("xtask-version-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join("Cargo.toml");
        fs::write(
            &manifest,
            "[dependencies]\nversion = \"9\"\n\n[package]\nname = \"x\"\nversion = \"1.2.3\"\n",
        )
        .unwrap();
        assert_eq!(package_version(&manifest).unwrap(), "1.2.3");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parses_options() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        let options = parse(args(&["--formats=deb,tar", "--skip-build"])).unwrap();
        assert_eq!(options.formats, ["deb", "tar"]);
        assert!(options.skip_build);
        assert!(parse(args(&["--formats", "zip"])).is_err());
        assert!(parse(args(&["--runtime"])).is_err());
        assert_eq!(parse(args(&["--runtime", "/r"])).unwrap().runtime, Some(PathBuf::from("/r")));
    }

    #[test]
    fn checksum_matches_known_value() {
        let digest: String = Sha256::digest(b"abc").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(digest, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
