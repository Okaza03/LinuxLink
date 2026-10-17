//! System tray icon (StatusNotifierItem). Closing the window leaves LinuxLink
//! running here: click the icon to open the window again, or quit from its menu.

use crate::variant::{APP_ID, APP_NAME};
use eframe::egui;
use ksni::blocking::TrayMethods;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

/// What the main thread should do while no window is open.
pub enum Request {
    Open,
    Quit,
}

/// Set when LinuxLink should exit instead of staying in the tray.
static QUIT: AtomicBool = AtomicBool::new(false);
static AVAILABLE: AtomicBool = AtomicBool::new(false);
/// The open window, if any.
static WINDOW: Mutex<Option<egui::Context>> = Mutex::new(None);
static REQUESTS: OnceLock<Sender<Request>> = OnceLock::new();

pub fn quitting() -> bool {
    QUIT.load(Ordering::Relaxed)
}

/// Whether a tray icon is showing (so closing the window is safe).
pub fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

pub fn set_window(ctx: Option<egui::Context>) {
    *WINDOW.lock().unwrap() = ctx;
}

fn window() -> Option<egui::Context> {
    WINDOW.lock().unwrap().clone()
}

/// Marks that the next window close should end LinuxLink.
pub fn set_quitting() {
    QUIT.store(true, Ordering::Relaxed);
}

/// Quits LinuxLink, tearing the audio graph down.
pub fn quit() {
    set_quitting();
    match window() {
        Some(ctx) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            ctx.request_repaint();
        }
        None => send(Request::Quit),
    }
}

/// Shows the window: brings it forward, or asks for a new one if it's closed.
pub fn open() {
    match window() {
        Some(ctx) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
        None => send(Request::Open),
    }
}

fn send(request: Request) {
    if let Some(tx) = REQUESTS.get() {
        let _ = tx.send(request);
    }
}

struct Tray;

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        APP_ID.into()
    }

    fn title(&self) -> String {
        APP_NAME.into()
    }

    fn icon_name(&self) -> String {
        APP_ID.into()
    }

    /// Drawn fallback for installs without the icon in a theme (AppImage, tarball).
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![fader_icon(32), fader_icon(64)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: APP_NAME.into(),
            description: "Your mixes keep playing while the window is closed.".into(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        open();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: format!("Open {APP_NAME}"),
                activate: Box::new(|_: &mut Self| open()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_: &mut Self| quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Starts the tray icon. Requests for the main thread arrive on `requests`.
/// Returns `None` if the desktop has no tray (e.g. GNOME without AppIndicator).
pub fn spawn(requests: Sender<Request>) -> Option<Box<dyn std::any::Any>> {
    let _ = REQUESTS.set(requests);
    // The Flatpak sandbox may not own extra D-Bus names; a unique name works too.
    let handle = Tray
        .disable_dbus_name(crate::sys::in_flatpak())
        .spawn()
        .map_err(|e| eprintln!("linuxlink: no system tray: {e}"))
        .ok()?;
    AVAILABLE.store(true, Ordering::Relaxed);
    Some(Box::new(handle))
}

/// The app icon's three faders, as ARGB32 (network byte order) pixels.
fn fader_icon(size: i32) -> ksni::Icon {
    let s = size as f32 / 64.0;
    let bg = [0xff, 0x1b, 0x1f, 0x27];
    let track = [0xff, 0x3a, 0x42, 0x50];
    let fill = [0xff, 0x3d, 0x8b, 0xfd];
    let knob = [0xff, 0xe8, 0xec, 0xf2];
    // (x centre, knob top) of each fader in the 64px design.
    let faders = [(18.0, 30.0), (32.0, 18.0), (46.0, 36.0)];
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (x, y) = ((px as f32 + 0.5) / s, (py as f32 + 0.5) / s);
            let mut c = [0u8; 4];
            if rounded_rect(x, y, 0.0, 0.0, 64.0, 64.0, 14.0) {
                c = bg;
            }
            for (cx, top) in faders {
                if (x - cx).abs() <= 2.5 && (14.0..=50.0).contains(&y) {
                    c = if y >= top + 3.5 { fill } else { track };
                }
                if rounded_rect(x, y, cx - 6.0, top, 12.0, 7.0, 3.0) {
                    c = knob;
                }
            }
            data.extend_from_slice(&c);
        }
    }
    ksni::Icon { width: size, height: size, data }
}

fn rounded_rect(x: f32, y: f32, left: f32, top: f32, w: f32, h: f32, r: f32) -> bool {
    let cx = x.clamp(left + r, left + w - r);
    let cy = y.clamp(top + r, top + h - r);
    x >= left && x <= left + w && y >= top && y <= top + h && (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}
