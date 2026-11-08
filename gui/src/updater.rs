//! Checks GitHub for a newer version, downloads and verifies it, and installs it with
//! the system's package manager through the `deepcool-digital-update` helper.

use crate::update::{self, Asset, Installer, Release};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

/// How often to look for updates while the window is open.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading {
        release: Release,
        done: u64,
        total: u64,
    },
    Installing(Release),
    /// Installed; the new version runs after a restart.
    Installed(String),
    Failed(String),
}

enum Event {
    Checked(Result<Release, String>),
    Progress(u64, u64),
    Installing,
    Finished(Result<String, String>),
}

pub struct Updater {
    pub state: State,
    /// The running version (overridable with `DEEPCOOL_PRETEND_VERSION` for testing).
    pub current: String,
    /// How this copy was installed; `None` means it can't update itself.
    pub installer: Option<Installer>,
    /// GitHub API address (a local server in tests).
    pub api: String,
    /// Runs the helper as root; the helper's command is appended.
    pub privileged: Vec<String>,
    /// The install helper; `None` finds it next to this program.
    pub helper: Option<PathBuf>,
    pub download_dir: PathBuf,
    manual: bool,
    last_check: Option<Instant>,
    job: Option<Receiver<Event>>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

/// This program's path, even after a package update replaced the file.
pub fn app_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let text = exe.to_string_lossy();
    Some(PathBuf::from(text.strip_suffix(" (deleted)").unwrap_or(&text)))
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(timeout))
        .user_agent(format!("deepcool-digital-desktop/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(404) => "No release has been published yet.".into(),
        ureq::Error::StatusCode(403 | 429) => "GitHub is limiting requests right now. Try again later.".into(),
        ureq::Error::StatusCode(code) => format!("The update server replied with error {code}."),
        _ => "Could not reach GitHub. Check your internet connection.".into(),
    }
}

/// Starts downloading `asset`: through GitHub's API first, then the public link.
fn fetch(agent: &ureq::Agent, asset: &Asset) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut error = None;
    if let Some(api_url) = &asset.api_url {
        match agent.get(api_url).header("Accept", "application/octet-stream").call() {
            Ok(response) => return Ok(response),
            Err(e) => error = Some(e),
        }
    }
    match agent.get(&asset.url).call() {
        Ok(response) => Ok(response),
        // Report the API's error when both fail: it is the more telling one.
        Err(e) => Err(describe(error.unwrap_or(e))),
    }
}

/// Asks GitHub for the latest release.
pub fn fetch_latest(api: &str, installer: Option<Installer>) -> Result<Release, String> {
    let url = format!("{api}/repos/{}/releases/latest", update::REPOSITORY);
    let body = agent(Duration::from_secs(20))
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(describe)?
        .body_mut()
        .read_to_string()
        .map_err(|_| "The update server sent an unexpected reply.".to_owned())?;
    update::parse_release(&body, installer)
}

/// Downloads `asset` into `dir` and checks it against the release's `SHA256SUMS`.
pub fn download(release: &Release, asset: &Asset, dir: &Path, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, String> {
    let checksums = release
        .checksums
        .as_ref()
        .ok_or("The release has no checksum file, so it can't be verified.")?;
    let agent = agent(Duration::from_secs(15 * 60));
    let sums = fetch(&agent, checksums)?
        .body_mut()
        .read_to_string()
        .map_err(|_| "Could not read the checksum file.".to_owned())?;
    let expected = update::checksum_for(&sums, &asset.name)
        .ok_or_else(|| format!("{} is not listed in the release's checksums.", asset.name))?;

    if asset.name.contains('/') || asset.name.starts_with('.') {
        return Err("The release has an unexpected file name.".into());
    }
    // Keep only this download in the folder.
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let target = dir.join(&asset.name);
    let partial = dir.join(format!("{}.part", asset.name));

    let response = fetch(&agent, asset)?;
    let total = response.body().content_length().unwrap_or(asset.size);
    if total > MAX_DOWNLOAD {
        return Err("The update is unexpectedly large; not downloading it.".into());
    }
    let mut reader = response.into_body().into_reader().take(MAX_DOWNLOAD + 1);
    let mut file = fs::File::create(&partial).map_err(|e| format!("Could not save the update: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut done = 0u64;
    let mut last_report = Instant::now();
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| "The download was interrupted. Try again.".to_owned())?;
        if read == 0 {
            break;
        }
        done += read as u64;
        if done > MAX_DOWNLOAD {
            return Err("The update is unexpectedly large; not downloading it.".into());
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|e| format!("Could not save the update: {e}"))?;
        if last_report.elapsed() > Duration::from_millis(100) {
            progress(done, total.max(done));
            last_report = Instant::now();
        }
    }
    progress(done, total.max(done));
    file.sync_all().map_err(|e| format!("Could not save the update: {e}"))?;
    let actual: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    if actual != expected {
        let _ = fs::remove_file(&partial);
        return Err("The downloaded update is damaged (checksum mismatch). Try again.".into());
    }
    fs::rename(&partial, &target).map_err(|e| format!("Could not save the update: {e}"))?;
    Ok(target)
}

fn cache_dir() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"),
    }
    .join("deepcool-digital/updates")
}

/// Opens a web page in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| format!("Could not open a browser. Visit {url}"))
}

impl Updater {
    pub fn new(notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        Updater {
            state: State::Idle,
            current: std::env::var("DEEPCOOL_PRETEND_VERSION")
                .ok()
                .filter(|version| update::parse_version(version).is_some())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
            installer: app_path().and_then(|path| Installer::detect(&path)),
            api: "https://api.github.com".into(),
            privileged: vec!["pkexec".into()],
            helper: None,
            download_dir: cache_dir(),
            manual: false,
            last_check: None,
            job: None,
            notify,
        }
    }

    pub fn busy(&self) -> bool {
        matches!(self.state, State::Checking | State::Downloading { .. } | State::Installing(_))
    }

    /// Checks if `enabled` and the last check is old enough. Quiet when it fails.
    pub fn maybe_check(&mut self, enabled: bool) {
        let due = self.last_check.is_none_or(|last| last.elapsed() >= CHECK_INTERVAL);
        let idle = matches!(self.state, State::Idle | State::UpToDate);
        if enabled && due && idle {
            self.start_check(false);
        }
    }

    /// A check the user asked for: failures are shown.
    pub fn check(&mut self) {
        if !self.busy() {
            self.start_check(true);
        }
    }

    fn start_check(&mut self, manual: bool) {
        self.manual = manual;
        self.last_check = Some(Instant::now());
        self.state = State::Checking;
        let (api, installer) = (self.api.clone(), self.installer);
        self.spawn(move |emit| emit(Event::Checked(fetch_latest(&api, installer))));
    }

    /// Runs `work` on a background thread; each event it emits wakes the window.
    fn spawn(&mut self, work: impl FnOnce(&dyn Fn(Event)) + Send + 'static) {
        let (sender, receiver) = mpsc::channel();
        let notify = self.notify.clone();
        thread::spawn(move || {
            let emit = |event| {
                let _ = sender.send(event);
                notify();
            };
            work(&emit);
        });
        self.job = Some(receiver);
    }

    fn install_helper(&self) -> Result<PathBuf, String> {
        if let Some(helper) = &self.helper {
            return Ok(helper.clone());
        }
        let helper = app_path()
            .ok_or("Could not find this program.")?
            .with_file_name(update::HELPER_NAME);
        if crate::core::is_executable(&helper) {
            Ok(helper)
        } else {
            Err(format!(
                "The {} helper is missing. Reinstall DeepCool Digital.",
                update::HELPER_NAME
            ))
        }
    }

    /// Downloads, verifies and installs the available update; without a matching
    /// package (portable or source builds) opens the release page instead.
    pub fn install(&mut self) -> Result<(), String> {
        let State::Available(release) = self.state.clone() else {
            return Ok(());
        };
        let Some(asset) = release.package.clone().filter(|_| self.installer.is_some()) else {
            return open_url(&release.page);
        };
        let helper = self.install_helper()?;
        let mut command = self.privileged.clone();
        command.push(helper.to_string_lossy().into_owned());
        command.push("install".into());
        let dir = self.download_dir.clone();
        self.state = State::Downloading {
            release: release.clone(),
            done: 0,
            total: asset.size,
        };
        self.spawn(move |emit| {
            let progress = |done, total| emit(Event::Progress(done, total));
            let file = match download(&release, &asset, &dir, &progress) {
                Ok(file) => file,
                Err(error) => return emit(Event::Finished(Err(error))),
            };
            emit(Event::Installing);
            command.push(file.to_string_lossy().into_owned());
            let result = match Command::new(&command[0]).args(&command[1..]).output() {
                Ok(output) if output.status.success() => Ok(release.version.clone()),
                Ok(output) if output.status.code() == Some(126) => {
                    Err("Update cancelled: the password prompt was closed.".into())
                }
                Ok(output) if output.status.code() == Some(127) => {
                    Err("Update not allowed: an administrator password is needed.".into())
                }
                Ok(output) => {
                    let text = String::from_utf8_lossy(&output.stderr);
                    let last = text
                        .lines()
                        .rev()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("unknown error");
                    Err(format!("The update could not be installed: {}", last.trim()))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(format!(
                    "{} is not installed. Install polkit, or install the update yourself.",
                    command[0]
                )),
                Err(error) => Err(format!("Could not start the installer: {error}")),
            };
            if result.is_ok() {
                let _ = fs::remove_file(&file);
            }
            emit(Event::Finished(result));
        });
        Ok(())
    }

    /// Applies finished background work; returns lines for the activity log.
    pub fn poll(&mut self) -> Vec<String> {
        let mut messages = Vec::new();
        let Some(job) = &self.job else { return messages };
        let events: Vec<Event> = job.try_iter().collect();
        for event in events {
            match event {
                Event::Checked(Ok(release)) if update::is_newer(&release.version, &self.current) => {
                    messages.push(format!("Version {} is available.", release.version));
                    self.state = State::Available(release);
                }
                Event::Checked(Ok(_)) => {
                    if self.manual {
                        messages.push(format!("DeepCool Digital {} is up to date.", self.current));
                    }
                    self.state = State::UpToDate;
                }
                Event::Checked(Err(error)) if self.manual => {
                    messages.push(format!("Could not check for updates: {error}"));
                    self.state = State::Failed(error);
                }
                Event::Checked(Err(_)) => self.state = State::Idle,
                Event::Progress(done, total) => {
                    if let State::Downloading { release, .. } = &self.state {
                        self.state = State::Downloading {
                            release: release.clone(),
                            done,
                            total,
                        };
                    }
                }
                Event::Installing => {
                    if let State::Downloading { release, .. } = &self.state {
                        messages.push("Download verified. Installing the update…".into());
                        self.state = State::Installing(release.clone());
                    }
                }
                Event::Finished(Ok(version)) => {
                    messages.push(format!("Version {version} is installed. Restart DeepCool Digital to use it."));
                    self.state = State::Installed(version);
                }
                Event::Finished(Err(error)) => {
                    messages.push(error.clone());
                    // Offer the update again unless it was the download that failed for good.
                    self.state = match &self.state {
                        State::Downloading { release, .. } | State::Installing(release) => {
                            State::Available(release.clone())
                        }
                        _ => State::Failed(error),
                    };
                }
            }
        }
        if !self.busy() {
            self.job = None;
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tests::temp_dir;
    use std::{io::BufRead, net::TcpListener};

    fn sha256(data: &[u8]) -> String {
        Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn release_json(base: &str, version: &str) -> String {
        let suffix = Installer::Deb.asset_suffix();
        format!(
            r#"{{"tag_name": "v{version}", "html_url": "{base}/page", "assets": [
                {{"name": "deepcool-digital-gui_{version}{suffix}", "size": 11, "browser_download_url": "{base}/package",
                  "url": "{base}/api/package"}},
                {{"name": "SHA256SUMS", "size": 80, "browser_download_url": "{base}/sums", "url": "{base}/api/sums"}}]}}"#
        )
    }

    fn wait(updater: &mut Updater, done: impl Fn(&State) -> bool) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut log = Vec::new();
        while !done(&updater.state) && Instant::now() < deadline {
            log.extend(updater.poll());
            thread::sleep(Duration::from_millis(10));
        }
        assert!(done(&updater.state), "timed out in {:?}; log {log:?}", updater.state);
        log
    }

    #[test]
    fn finds_downloads_verifies_and_installs_an_update() {
        let package = b"new version".to_vec();
        let name = format!("deepcool-digital-gui_9.0.0{}", Installer::Deb.asset_suffix());
        let sums = format!("{}  {name}\n", sha256(&package));
        let server = serve_with(|base| {
            vec![
                (
                    format!("/repos/{}/releases/latest", update::REPOSITORY),
                    release_json(base, "9.0.0").into_bytes(),
                ),
                ("/package".to_owned(), package.clone()),
                ("/sums".to_owned(), sums.into_bytes()),
            ]
        });

        let root = temp_dir("updater");
        let helper = root.join("helper");
        // Stands in for `pkexec helper install <file>`: records the file it was given.
        fs::write(
            &helper,
            format!("#!/bin/sh\ncp \"$2\" '{}'\n", root.join("installed").display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();

        let mut updater = Updater::new(Arc::new(|| {}));
        updater.api = server.clone();
        updater.current = "0.13.0".into();
        updater.installer = Some(Installer::Deb);
        updater.privileged = vec!["/bin/sh".into()];
        updater.helper = Some(helper);
        updater.download_dir = root.join("downloads");

        updater.check();
        let log = wait(&mut updater, |s| matches!(s, State::Available(_)));
        assert_eq!(log, ["Version 9.0.0 is available."]);
        updater.install().unwrap();
        let log = wait(&mut updater, |s| matches!(s, State::Installed(_)));
        assert!(log.iter().any(|l| l.starts_with("Download verified")), "{log:?}");
        assert_eq!(fs::read(root.join("installed")).unwrap(), package);

        // Same version: up to date, and an automatic check stays quiet about it.
        updater.current = "9.0.0".into();
        updater.state = State::Idle;
        updater.last_check = None;
        updater.maybe_check(true);
        let log = wait(&mut updater, |s| *s == State::UpToDate);
        assert!(log.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn downloads_when_one_route_fails() {
        let package = b"new version".to_vec();
        let name = "deepcool-digital-gui_9.0.0_amd64.deb".to_owned();
        let sums = format!("{}  {name}\n", sha256(&package)).into_bytes();
        let asset = |server: &str, kind: &str| Asset {
            name: if kind == "sums" {
                "SHA256SUMS".into()
            } else {
                name.clone()
            },
            url: format!("{server}/{kind}"),
            api_url: Some(format!("{server}/api/{kind}")),
            size: 1,
        };
        // Only the API works (the public links fail, as they did on GitHub), then only the public links.
        for routes in [["/api/package", "/api/sums"], ["/package", "/sums"]] {
            let server = serve_with(|_| vec![(routes[0].into(), package.clone()), (routes[1].into(), sums.clone())]);
            let release = Release {
                version: "9.0.0".into(),
                page: String::new(),
                package: Some(asset(&server, "package")),
                checksums: Some(asset(&server, "sums")),
            };
            let root = temp_dir("fallback");
            let file = download(&release, release.package.as_ref().unwrap(), &root, &|_, _| {}).unwrap();
            assert_eq!(fs::read(&file).unwrap(), package, "routes {routes:?}");
            let _ = fs::remove_dir_all(root);
        }
    }

    /// A tiny stand-in for GitHub: serves fixed responses by path. `routes` gets the
    /// server's address so responses can link back to it.
    fn serve_with(routes: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let routes = routes(&address);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) <= 2 {
                        break;
                    }
                }
                let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
                let mut stream = stream;
                match routes.iter().find(|(route, _)| *route == path) {
                    Some((_, body)) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(body);
                    }
                    None => {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
        });
        address
    }

    #[test]
    fn rejects_a_damaged_download() {
        let name = "deepcool-digital-gui_9.0.0_amd64.deb".to_owned();
        let server = serve_with(|_| {
            vec![
                ("/package".to_owned(), b"tampered".to_vec()),
                ("/sums".to_owned(), format!("{}  {name}\n", sha256(b"original")).into_bytes()),
            ]
        });
        let release = Release {
            version: "9.0.0".into(),
            page: String::new(),
            package: Some(Asset {
                name: name.clone(),
                url: format!("{server}/package"),
                api_url: None,
                size: 8,
            }),
            checksums: Some(Asset {
                name: "SHA256SUMS".into(),
                url: format!("{server}/sums"),
                api_url: None,
                size: 1,
            }),
        };
        let root = temp_dir("damaged");
        let error = download(&release, release.package.as_ref().unwrap(), &root, &|_, _| {}).unwrap_err();
        assert!(error.contains("checksum mismatch"), "{error}");
        assert!(!root.join(&name).exists());
        // A release without published packages for this system reports it clearly.
        let missing = serve_with(|_| vec![]);
        assert_eq!(fetch_latest(&missing, None).unwrap_err(), "No release has been published yet.");
        let _ = fs::remove_dir_all(root);
    }

    /// Talks to the real GitHub: `cargo test -p deepcool-digital-gui -- --ignored github`
    #[test]
    #[ignore]
    fn github_latest_release_downloads_and_verifies() {
        let installer = Some(Installer::Deb);
        let release = fetch_latest("https://api.github.com", installer).expect("latest release");
        assert!(update::parse_version(&release.version).is_some());
        let asset = release.package.clone().expect("a .deb in the latest release");
        let root = temp_dir("github");
        let file = download(&release, &asset, &root, &|_, _| {}).expect("download");
        assert_eq!(fs::metadata(&file).unwrap().len(), asset.size);
        let _ = fs::remove_dir_all(root);
    }
}
