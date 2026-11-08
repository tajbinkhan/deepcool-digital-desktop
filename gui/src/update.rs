//! Version numbers, GitHub release data and package managers, shared by the app and
//! the `deepcool-digital-update` helper. No network access happens here.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The GitHub repository that publishes releases.
pub const REPOSITORY: &str = "tajbinkhan/deepcool-digital-desktop";
/// The package every release format installs.
pub const PACKAGE: &str = "deepcool-digital-gui";
/// Where native packages install the app.
pub const INSTALLED_APP: &str = "/usr/lib/deepcool-digital/deepcool-digital-gui";
/// The helper that installs a downloaded update as root (through polkit).
pub const HELPER_NAME: &str = "deepcool-digital-update";

/// `MAJOR.MINOR.PATCH`, from `0.14.0` or `v0.14.0`. Pre-releases (`1.0.0-beta`) are ignored.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// True when `latest` is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

/// How this copy of the app was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer {
    Deb,
    Rpm,
    Arch,
}

fn output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Finds `program` in the directories of `PATH`.
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/sbin:/usr/bin:/sbin:/bin".into());
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

impl Installer {
    /// Asks each package manager whether it installed `app`. `None` for portable and source builds.
    pub fn detect(app: &Path) -> Option<Self> {
        let app = app.to_str()?;
        if output("dpkg-query", &["-S", app]).is_some_and(|owner| owner.starts_with(&format!("{PACKAGE}:"))) {
            return Some(Installer::Deb);
        }
        if output("rpm", &["-qf", "--queryformat", "%{NAME}", app]).as_deref() == Some(PACKAGE) {
            return Some(Installer::Rpm);
        }
        if output("pacman", &["-Qqo", app]).as_deref() == Some(PACKAGE) {
            return Some(Installer::Arch);
        }
        None
    }

    pub fn name(self) -> &'static str {
        match self {
            Installer::Deb => ".deb",
            Installer::Rpm => ".rpm",
            Installer::Arch => "Arch",
        }
    }

    /// The end of the release file name for this kind of package on this machine.
    pub fn asset_suffix(self) -> String {
        let machine = std::env::consts::ARCH;
        match self {
            Installer::Deb => {
                let arch = match machine {
                    "x86_64" => "amd64",
                    "aarch64" => "arm64",
                    other => other,
                };
                format!("_{arch}.deb")
            }
            Installer::Rpm => format!(".{machine}.rpm"),
            Installer::Arch => format!("-{machine}.pkg.tar.zst"),
        }
    }

    /// The package name recorded inside a package file.
    pub fn package_name(self, file: &Path) -> Result<String, String> {
        let file = file.to_str().ok_or("Unsupported file name.")?;
        let name = match self {
            Installer::Deb => output("dpkg-deb", &["--field", file, "Package"]),
            Installer::Rpm => output("rpm", &["-qp", "--queryformat", "%{NAME}", file]),
            Installer::Arch => output("tar", &["--zstd", "-xOf", file, ".PKGINFO"]).and_then(|info| {
                info.lines()
                    .find_map(|line| line.strip_prefix("pkgname = "))
                    .map(str::to_owned)
            }),
        };
        name.filter(|name| !name.is_empty())
            .ok_or_else(|| "Could not read the downloaded package.".into())
    }

    /// The command that installs `file` with this system's package manager.
    pub fn install_command(self, file: &Path) -> Vec<String> {
        let file = file.to_string_lossy().into_owned();
        let command: Vec<&str> = match self {
            Installer::Deb if which("apt-get").is_some() => vec!["apt-get", "install", "-y", &file],
            Installer::Deb => vec!["dpkg", "-i", &file],
            Installer::Rpm if which("dnf").is_some() => vec!["dnf", "install", "-y", &file],
            Installer::Rpm if which("zypper").is_some() => {
                vec!["zypper", "--non-interactive", "install", "--allow-unsigned-rpm", &file]
            }
            Installer::Rpm => vec!["rpm", "-U", &file],
            Installer::Arch => vec!["pacman", "-U", "--noconfirm", &file],
        };
        command.into_iter().map(str::to_owned).collect()
    }
}

/// A downloadable file of a release.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// The newest published release.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// Without the leading `v`, for example `0.14.0`.
    pub version: String,
    /// The release page on GitHub.
    pub page: String,
    /// The package for this installation, when one exists.
    pub package: Option<Asset>,
    pub checksums: Option<Asset>,
}

/// Reads the GitHub "latest release" API response.
pub fn parse_release(json: &str, installer: Option<Installer>) -> Result<Release, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "The update server sent an unexpected reply.".to_owned())?;
    let tag = value["tag_name"].as_str().ok_or("The latest release has no version.")?;
    let version = tag.trim_start_matches('v').to_owned();
    if parse_version(&version).is_none() {
        return Err(format!("The latest release has an unexpected version: {tag}"));
    }
    let assets: Vec<Asset> = value["assets"]
        .as_array()
        .map(|assets| {
            assets
                .iter()
                .filter_map(|asset| {
                    Some(Asset {
                        name: asset["name"].as_str()?.to_owned(),
                        url: asset["browser_download_url"].as_str()?.to_owned(),
                        size: asset["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let package = installer.and_then(|installer| {
        let suffix = installer.asset_suffix();
        assets
            .iter()
            .find(|asset| asset.name.starts_with(PACKAGE) && asset.name.ends_with(&suffix))
            .cloned()
    });
    Ok(Release {
        version,
        page: value["html_url"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("https://github.com/{REPOSITORY}/releases/latest")),
        package,
        checksums: assets.iter().find(|asset| asset.name == "SHA256SUMS").cloned(),
    })
}

/// The SHA-256 listed for `name` in a `SHA256SUMS` file.
pub fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert_eq!(parse_version("v0.14.0"), Some((0, 14, 0)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.0.0-beta"), None);
        assert!(is_newer("v0.14.0", "0.13.0"));
        assert!(is_newer("0.13.10", "0.13.9"), "compares numbers, not text");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.13.0", "0.13.0"));
        assert!(!is_newer("0.12.0", "0.13.0"));
        assert!(!is_newer("garbage", "0.13.0"));
    }

    const RELEASE: &str = r#"{
        "tag_name": "v0.14.0",
        "html_url": "https://github.com/tajbinkhan/deepcool-digital-desktop/releases/tag/v0.14.0",
        "assets": [
            {"name": "deepcool-digital-gui-0.14.0-1-x86_64.pkg.tar.zst", "size": 5, "browser_download_url": "https://x/arch"},
            {"name": "deepcool-digital-gui-0.14.0-1.x86_64.rpm", "size": 6, "browser_download_url": "https://x/rpm"},
            {"name": "deepcool-digital-gui-0.14.0-x86_64.tar.gz", "size": 7, "browser_download_url": "https://x/tar"},
            {"name": "deepcool-digital-gui_0.14.0_amd64.deb", "size": 8, "browser_download_url": "https://x/deb"},
            {"name": "SHA256SUMS", "size": 9, "browser_download_url": "https://x/sums"}
        ]
    }"#;

    #[test]
    fn picks_the_package_for_this_installation() {
        if std::env::consts::ARCH != "x86_64" {
            return;
        }
        let deb = parse_release(RELEASE, Some(Installer::Deb)).unwrap();
        assert_eq!(deb.version, "0.14.0");
        assert_eq!(deb.package.unwrap().url, "https://x/deb");
        assert_eq!(deb.checksums.unwrap().url, "https://x/sums");
        assert_eq!(
            parse_release(RELEASE, Some(Installer::Rpm))
                .unwrap()
                .package
                .unwrap()
                .url,
            "https://x/rpm"
        );
        assert_eq!(
            parse_release(RELEASE, Some(Installer::Arch))
                .unwrap()
                .package
                .unwrap()
                .url,
            "https://x/arch"
        );
        let portable = parse_release(RELEASE, None).unwrap();
        assert!(portable.package.is_none());
        assert!(portable.page.ends_with("/v0.14.0"));
        assert!(parse_release("{}", None).is_err());
        assert!(parse_release("not json", None).is_err());
    }

    #[test]
    fn reads_checksums() {
        let sums =
            "0396dae17103bcd36ef7a72c5e359f16083644c612117fcbb8350769e57d6117  deepcool-digital-gui_0.12.0_amd64.deb\n\
                    1140ae8a15bbf06a77f087e4aeef2ec5f37a0e8d81521aeb088d31f0d689f1e4  other.rpm\n";
        assert_eq!(
            checksum_for(sums, "deepcool-digital-gui_0.12.0_amd64.deb").as_deref(),
            Some("0396dae17103bcd36ef7a72c5e359f16083644c612117fcbb8350769e57d6117")
        );
        assert_eq!(checksum_for(sums, "missing.deb"), None);
    }

    #[test]
    fn install_commands_use_the_package_manager() {
        let file = Path::new("/tmp/update.pkg");
        assert_eq!(
            Installer::Arch.install_command(file),
            ["pacman", "-U", "--noconfirm", "/tmp/update.pkg"]
        );
        assert_eq!(Installer::Deb.install_command(file).last().unwrap(), "/tmp/update.pkg");
    }
}
