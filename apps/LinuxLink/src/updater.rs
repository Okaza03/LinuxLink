//! Update checks against GitHub releases, and installing them where the
//! install type allows it.
//!
//! - AppImage and tarball installs replace themselves in the background and
//!   only need a restart.
//! - .deb, .rpm and Flatpak are owned by the system, so the update is
//!   downloaded to ~/Downloads and opened in the system's installer.
//! - Arch packages (pacman/AUR) and source builds get the release page.
//!
//! Every download is verified against the release's SHA256SUMS file.

use crate::sys;
use crate::variant::{REPO, TEST_BUILD, VERSION};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_DOWNLOAD: u64 = 500 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum InstallKind {
    AppImage(PathBuf),
    /// The release tarball's binary, at this path.
    Tarball(PathBuf),
    Deb,
    Rpm,
    Flatpak,
    Arch,
    /// Source builds (including LinuxLink Test) and unknown package types.
    Other,
}

impl InstallKind {
    pub fn detect() -> Self {
        if TEST_BUILD {
            return Self::Other;
        }
        if let Some(path) = std::env::var_os("APPIMAGE") {
            return Self::AppImage(path.into());
        }
        if sys::in_flatpak() {
            return Self::Flatpak;
        }
        let Ok(exe) = std::env::current_exe() else { return Self::Other };
        if !exe.starts_with("/usr") {
            return Self::Tarball(exe);
        }
        let os = std::fs::read_to_string("/etc/os-release").unwrap_or_default().to_lowercase();
        let ids: Vec<&str> = os
            .lines()
            .filter_map(|l| l.strip_prefix("id=").or_else(|| l.strip_prefix("id_like=")))
            .flat_map(|v| v.trim_matches('"').split_whitespace())
            .collect();
        let any = |names: &[&str]| names.iter().any(|n| ids.contains(n));
        if any(&["arch"]) {
            Self::Arch
        } else if any(&["debian", "ubuntu"]) {
            Self::Deb
        } else if any(&["fedora", "rhel", "centos", "suse", "opensuse"]) {
            Self::Rpm
        } else {
            Self::Other
        }
    }

    /// Whether updates install themselves in the background.
    fn installs_itself(&self) -> bool {
        matches!(self, Self::AppImage(_) | Self::Tarball(_))
    }

    /// The release file for this install type and CPU (names match release.yml).
    fn asset_name(&self, version: &str) -> Option<String> {
        let arch = std::env::consts::ARCH;
        let deb_arch = match arch {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            other => other,
        };
        Some(match self {
            Self::AppImage(_) => format!("LinuxLink-{version}-{arch}.AppImage"),
            Self::Tarball(_) => format!("linuxlink-{version}-{arch}-linux.tar.gz"),
            Self::Deb => format!("linuxlink_{version}_{deb_arch}.deb"),
            Self::Rpm => format!("linuxlink-{version}-1.{arch}.rpm"),
            Self::Flatpak => format!("LinuxLink-{version}-{arch}.flatpak"),
            Self::Arch | Self::Other => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Checking,
    UpToDate,
    Available {
        version: String,
    },
    Downloading {
        version: String,
        progress: f32,
    },
    /// Installed in place; a restart runs the new version.
    ReadyToRestart {
        version: String,
    },
    /// Handed to the system installer.
    Downloaded {
        version: String,
        path: PathBuf,
    },
    Failed(String),
}

#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Default)]
struct State {
    status: Option<Status>,
    release: Option<Release>,
    busy: bool,
    restart: Option<PathBuf>,
}

pub struct Updater {
    pub kind: InstallKind,
    state: Mutex<State>,
}

impl Updater {
    /// Starts checking now and every few hours in the background.
    pub fn start() -> Arc<Self> {
        let me = Arc::new(Self { kind: InstallKind::detect(), state: Mutex::default() });
        let bg = me.clone();
        std::thread::Builder::new()
            .name("linuxlink-updates".into())
            .spawn(move || loop {
                bg.check();
                std::thread::sleep(CHECK_EVERY);
            })
            .expect("spawning update thread");
        me
    }

    pub fn status(&self) -> Status {
        self.state.lock().unwrap().status.clone().unwrap_or(Status::Checking)
    }

    fn set(&self, status: Status) {
        self.state.lock().unwrap().status = Some(status);
    }

    /// What the update button does in the current state.
    pub fn press(self: &Arc<Self>) {
        match self.status() {
            Status::UpToDate | Status::Failed(_) => self.in_background(Self::check),
            Status::Available { .. } if self.kind.asset_name("").is_some() => self.in_background(Self::install),
            Status::Available { .. } => {
                let _ = sys::open(&self.release_page());
            }
            Status::Downloaded { path, .. } => {
                let _ = sys::open(&path.to_string_lossy());
            }
            Status::ReadyToRestart { .. } => self.request_restart(),
            Status::Checking | Status::Downloading { .. } => {}
        }
    }

    pub fn release_page(&self) -> String {
        let release = self.state.lock().unwrap().release.as_ref().map(|r| r.html_url.clone());
        release.unwrap_or_else(|| format!("https://github.com/{REPO}/releases/latest"))
    }

    fn request_restart(&self) {
        let path = match &self.kind {
            InstallKind::AppImage(p) | InstallKind::Tarball(p) => Some(p.clone()),
            _ => None,
        };
        self.state.lock().unwrap().restart = path;
    }

    /// The program to launch after LinuxLink closes, if a restart was asked for.
    pub fn take_restart(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().restart.take()
    }

    fn in_background(self: &Arc<Self>, task: fn(&Self)) {
        let me = self.clone();
        std::thread::spawn(move || task(&me));
    }

    /// Looks for a newer release; installs it right away where possible.
    fn check(&self) {
        {
            let mut st = self.state.lock().unwrap();
            let settled = matches!(
                st.status,
                Some(Status::Downloading { .. } | Status::ReadyToRestart { .. } | Status::Downloaded { .. })
            );
            if st.busy || settled {
                return;
            }
            st.busy = true;
            st.status = Some(Status::Checking);
        }
        let status = match latest_release() {
            Ok(Some(release)) => {
                let version = release.tag_name.trim_start_matches('v').to_string();
                let newer = is_newer(&version, VERSION);
                self.state.lock().unwrap().release = Some(release);
                if newer {
                    Status::Available { version }
                } else {
                    Status::UpToDate
                }
            }
            Ok(None) => Status::UpToDate,
            Err(e) => Status::Failed(format!("Couldn't check for updates: {e:#}")),
        };
        let auto = matches!(status, Status::Available { .. }) && self.kind.installs_itself();
        self.set(status);
        self.state.lock().unwrap().busy = false;
        if auto {
            self.install();
        }
    }

    fn install(&self) {
        let Some(release) = self.state.lock().unwrap().release.clone() else { return };
        let version = release.tag_name.trim_start_matches('v').to_string();
        let result = self.try_install(&release, &version);
        self.set(result.unwrap_or_else(|e| Status::Failed(format!("Update failed: {e:#}"))));
    }

    fn try_install(&self, release: &Release, version: &str) -> Result<Status> {
        let name = self.kind.asset_name(version).ok_or_else(|| anyhow!("no download for this install type"))?;
        let asset = find_asset(release, &name)?;
        let sums = get_text(&find_asset(release, "SHA256SUMS")?.browser_download_url)?;
        let expected = expected_hash(&sums, &name)?;
        let progress = |p: f32| self.set(Status::Downloading { version: version.to_string(), progress: p });

        match &self.kind {
            InstallKind::AppImage(path) => {
                let part = path.with_file_name(format!(".{name}.part"));
                download(&asset.browser_download_url, &part, &expected, progress)?;
                make_executable(&part)?;
                std::fs::rename(&part, path).with_context(|| format!("replacing {}", path.display()))?;
                Ok(Status::ReadyToRestart { version: version.to_string() })
            }
            InstallKind::Tarball(exe) => {
                let dir = std::env::temp_dir().join(format!("linuxlink-update-{version}"));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir)?;
                let archive = dir.join(&name);
                download(&asset.browser_download_url, &archive, &expected, progress)?;
                let ok = Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(&dir).status()?.success();
                if !ok {
                    bail!("couldn't unpack {name}");
                }
                let unpacked = dir.join(name.trim_end_matches(".tar.gz")).join("linuxlink");
                let staged = exe.with_extension("new");
                std::fs::copy(&unpacked, &staged).with_context(|| format!("copying {}", unpacked.display()))?;
                make_executable(&staged)?;
                std::fs::rename(&staged, exe).with_context(|| format!("replacing {}", exe.display()))?;
                let _ = std::fs::remove_dir_all(&dir);
                Ok(Status::ReadyToRestart { version: version.to_string() })
            }
            InstallKind::Deb | InstallKind::Rpm | InstallKind::Flatpak => {
                let path = dirs::download_dir().unwrap_or_else(std::env::temp_dir).join(&name);
                download(&asset.browser_download_url, &path, &expected, progress)?;
                sys::open(&path.to_string_lossy()).context("opening the installer")?;
                Ok(Status::Downloaded { version: version.to_string(), path })
            }
            InstallKind::Arch | InstallKind::Other => unreachable!("no asset for this install type"),
        }
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

fn user_agent() -> String {
    format!("LinuxLink/{VERSION} (+https://github.com/{REPO})")
}

/// The newest published release, or `None` if there are no releases yet.
fn latest_release() -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = agent(Duration::from_secs(20))
        .get(&url)
        .header("User-Agent", &user_agent())
        .header("Accept", "application/vnd.github+json")
        .call();
    match response {
        Ok(mut r) => Ok(Some(r.body_mut().read_json()?)),
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_text(url: &str) -> Result<String> {
    let mut r = agent(Duration::from_secs(30)).get(url).header("User-Agent", &user_agent()).call()?;
    Ok(r.body_mut().read_to_string()?)
}

fn find_asset<'a>(release: &'a Release, name: &str) -> Result<&'a Asset> {
    release.assets.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("the release has no {name}"))
}

/// Downloads `url` to `dest`, reporting progress, and checks its SHA-256.
fn download(url: &str, dest: &Path, expected: &str, progress: impl Fn(f32)) -> Result<()> {
    let mut response = agent(Duration::from_secs(15 * 60)).get(url).header("User-Agent", &user_agent()).call()?;
    let total = response.body().content_length().filter(|&n| n > 0);
    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > MAX_DOWNLOAD {
            bail!("download is unexpectedly large");
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        if let Some(total) = total {
            progress(done as f32 / total as f32);
        }
    }
    file.sync_all()?;
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected {
        let _ = std::fs::remove_file(dest);
        bail!("the download's checksum doesn't match SHA256SUMS");
    }
    Ok(())
}

/// The hash listed for `name` in a `sha256sum`-style file.
fn expected_hash(sums: &str, name: &str) -> Result<String> {
    sums.lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            Some((parts.next()?, parts.next()?.trim_start_matches('*')))
        })
        .find(|(_, file)| *file == name)
        .map(|(hash, _)| hash.to_lowercase())
        .ok_or_else(|| anyhow!("SHA256SUMS has no entry for {name}"))
}

fn make_executable(path: &Path) -> Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Whether version `a` is newer than `b` ("1.2.10" > "1.2.9"; suffixes ignored).
pub fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0-beta", "0.1.0"));
    }

    #[test]
    fn finds_hash_in_sha256sums() {
        let sums = "abc123  linuxlink_0.2.0_amd64.deb\nDEF456 *LinuxLink-0.2.0-x86_64.AppImage\n";
        assert_eq!(expected_hash(sums, "LinuxLink-0.2.0-x86_64.AppImage").unwrap(), "def456");
        assert!(expected_hash(sums, "missing.rpm").is_err());
    }

    #[test]
    fn asset_names_match_release_files() {
        let arch = std::env::consts::ARCH;
        assert_eq!(
            InstallKind::AppImage("/x".into()).asset_name("0.2.0").unwrap(),
            format!("LinuxLink-0.2.0-{arch}.AppImage")
        );
        assert_eq!(
            InstallKind::Tarball("/x".into()).asset_name("0.2.0").unwrap(),
            format!("linuxlink-0.2.0-{arch}-linux.tar.gz")
        );
        assert!(InstallKind::Arch.asset_name("0.2.0").is_none());
    }
}
