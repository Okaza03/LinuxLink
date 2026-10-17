//! App menu entry and icons for the AppImage, which (unlike the packages)
//! has nothing to install them. Without a menu entry the desktop also can't
//! match the window to an icon, so the taskbar shows a generic one.

use crate::variant::{APP_ID, APP_NAME};
use std::path::{Path, PathBuf};

const DESKTOP: &str = include_str!("../../../packaging/linux/io.github.Okaza03.LinuxLink.desktop");
const ICON_SVG: &str = include_str!("../../../packaging/linux/io.github.Okaza03.LinuxLink.svg");
const ICON_128: &[u8] = include_bytes!("../../../packaging/linux/io.github.Okaza03.LinuxLink-128.png");
pub const ICON_256: &[u8] = include_bytes!("../../../packaging/linux/io.github.Okaza03.LinuxLink-256.png");

/// The running AppImage file, if this is one.
pub fn appimage_path() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file())
}

fn files(data_dir: &Path) -> [(PathBuf, &'static [u8]); 3] {
    let icons = data_dir.join("icons/hicolor");
    [
        (icons.join(format!("scalable/apps/{APP_ID}.svg")), ICON_SVG.as_bytes()),
        (icons.join(format!("128x128/apps/{APP_ID}.png")), ICON_128),
        (icons.join(format!("256x256/apps/{APP_ID}.png")), ICON_256),
    ]
}

fn desktop_path(data_dir: &Path) -> PathBuf {
    data_dir.join("applications").join(format!("{APP_ID}.desktop"))
}

/// The menu entry for an AppImage at `appimage`. TryExec hides it if the
/// file is deleted.
fn desktop_entry(appimage: &Path) -> String {
    let quoted = format!("\"{}\"", appimage.display());
    DESKTOP
        .replace("Name=LinuxLink", &format!("Name={APP_NAME}"))
        .replace("Exec=linuxlink", &format!("TryExec={}\nExec={quoted}", appimage.display()))
}

/// Adds or refreshes the menu entry and icons. Does nothing if they're
/// already current, so it's cheap to call on every start.
pub fn install(data_dir: &Path, appimage: &Path) -> std::io::Result<bool> {
    let entry = desktop_entry(appimage);
    let path = desktop_path(data_dir);
    if std::fs::read_to_string(&path).is_ok_and(|current| current == entry) {
        return Ok(false);
    }
    for (file, bytes) in files(data_dir) {
        std::fs::create_dir_all(file.parent().unwrap())?;
        std::fs::write(file, bytes)?;
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, entry)?;
    Ok(true)
}

pub fn remove(data_dir: &Path) {
    let _ = std::fs::remove_file(desktop_path(data_dir));
    for (file, _) in files(data_dir) {
        let _ = std::fs::remove_file(file);
    }
    refresh_menus(data_dir);
}

/// Whether the AppImage's menu entry exists.
pub fn installed(data_dir: &Path) -> bool {
    desktop_path(data_dir).exists()
}

fn refresh_menus(data_dir: &Path) {
    let _ = std::process::Command::new("update-desktop-database").arg(data_dir.join("applications")).output();
    // KDE notices new entries on its own; this just makes it immediate.
    let _ = std::process::Command::new("kbuildsycoca6").output();
}

/// Called at startup: keeps an AppImage's menu entry in place and pointing
/// at the file's current location.
pub fn on_start(enabled: bool) {
    let (Some(appimage), Some(data_dir)) = (appimage_path(), dirs::data_dir()) else { return };
    if !enabled {
        return;
    }
    match install(&data_dir, &appimage) {
        Ok(true) => refresh_menus(&data_dir),
        Ok(false) => {}
        Err(e) => eprintln!("linuxlink: adding the app menu entry: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_entry_pointing_at_the_appimage() {
        let dir = std::env::temp_dir().join(format!("linuxlink-integrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let appimage = Path::new("/home/user/Apps/My Apps/LinuxLink-0.1.1-x86_64.AppImage");

        assert!(install(&dir, appimage).unwrap(), "first install writes files");
        let entry = std::fs::read_to_string(desktop_path(&dir)).unwrap();
        assert!(entry.contains("Exec=\"/home/user/Apps/My Apps/LinuxLink-0.1.1-x86_64.AppImage\""));
        assert!(entry.contains("TryExec=/home/user/Apps/My Apps/LinuxLink-0.1.1-x86_64.AppImage"));
        assert!(entry.contains(&format!("Icon={APP_ID}")));
        assert!(dir.join(format!("icons/hicolor/256x256/apps/{APP_ID}.png")).is_file());
        assert!(!install(&dir, appimage).unwrap(), "unchanged entry isn't rewritten");

        // Moving the AppImage updates the entry.
        assert!(install(&dir, Path::new("/opt/LinuxLink.AppImage")).unwrap());
        assert!(std::fs::read_to_string(desktop_path(&dir)).unwrap().contains("Exec=\"/opt/LinuxLink.AppImage\""));

        remove(&dir);
        assert!(!installed(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
