//! Host integration that differs between a normal install and the Flatpak.
//!
//! Inside the Flatpak sandbox the audio tools (`pactl`, `parec`), `bluetoothctl`
//! and `xdg-open` run on the host through `flatpak-spawn`, and the host's
//! `/usr` is visible under `/run/host/usr`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn in_flatpak() -> bool {
    static FLATPAK: OnceLock<bool> = OnceLock::new();
    *FLATPAK.get_or_init(|| Path::new("/.flatpak-info").exists())
}

/// A command that runs `program` on the host system.
pub fn host_command(program: &str) -> Command {
    if in_flatpak() {
        let mut cmd = Command::new("flatpak-spawn");
        // --watch-bus ends the host process when we exit or are killed.
        cmd.args(["--host", "--watch-bus", program]);
        cmd
    } else {
        Command::new(program)
    }
}

/// A host system path (e.g. `/usr/share/icons`) as seen from this process.
pub fn host_path(path: &str) -> PathBuf {
    match path.strip_prefix("/usr/") {
        Some(rest) if in_flatpak() => Path::new("/run/host/usr").join(rest),
        _ => PathBuf::from(path),
    }
}

/// The host's `~/.config` (the Flatpak gets a private one by default).
pub fn host_config_dir() -> Option<PathBuf> {
    if in_flatpak() {
        dirs::home_dir().map(|h| h.join(".config"))
    } else {
        dirs::config_dir()
    }
}

/// Opens a file or URL with the user's default application.
pub fn open(target: &str) -> std::io::Result<()> {
    host_command("xdg-open").arg(target).spawn().map(drop)
}
