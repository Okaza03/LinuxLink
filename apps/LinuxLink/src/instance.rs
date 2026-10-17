//! One LinuxLink per session. A lock file decides who runs; a Unix socket lets
//! a second launch (e.g. from the app menu) bring the running one's window back
//! instead of starting another copy.

use crate::variant::APP_ID;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

pub enum Start {
    /// We're the only one; keep the guard alive while running.
    First(Guard),
    /// Another copy runs and was asked to show its window.
    Shown,
    /// Another variant (LinuxLink vs. LinuxLink Test) is running.
    OtherRunning,
}

pub struct Guard {
    _lock: Option<File>,
}

fn runtime_dir() -> PathBuf {
    dirs::runtime_dir().unwrap_or_else(std::env::temp_dir)
}

fn socket_path() -> PathBuf {
    runtime_dir().join("linuxlink.sock")
}

pub fn acquire() -> Start {
    let Ok(file) = File::create(runtime_dir().join("linuxlink.lock")) else {
        // No usable runtime dir: run without the single-instance guarantee.
        return Start::First(Guard { _lock: None });
    };
    match file.try_lock() {
        Ok(()) => Start::First(Guard { _lock: Some(file) }),
        Err(std::fs::TryLockError::WouldBlock) => match ask_running_to_show() {
            Some(true) => Start::Shown,
            _ => Start::OtherRunning,
        },
        Err(_) => Start::First(Guard { _lock: None }),
    }
}

/// `Some(true)` if the running copy is this app and showed its window.
fn ask_running_to_show() -> Option<bool> {
    let mut stream = UnixStream::connect(socket_path()).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    writeln!(stream, "open {APP_ID}").ok()?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).ok()?;
    Some(reply.trim() == "ok")
}

/// Listens for later launches and calls `open` when one asks for the window.
pub fn serve(open: fn()) {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else { return };
    std::thread::Builder::new()
        .name("linuxlink-instance".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut line = String::new();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let same_app = line.trim() == format!("open {APP_ID}");
                let _ = writeln!(stream, "{}", if same_app { "ok" } else { "other" });
                if same_app {
                    open();
                }
            }
        })
        .expect("spawning instance thread");
}
