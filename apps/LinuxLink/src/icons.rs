//! Finds application icons in the freedesktop icon themes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SUBDIRS: &[&str] = &[
    "64x64/apps",
    "48x48/apps",
    "128x128/apps",
    "256x256/apps",
    "scalable/apps",
    "apps/64",
    "apps/48",
    "apps/scalable",
    "32x32/apps",
];

pub struct IconCache {
    themes: Vec<PathBuf>,
    found: HashMap<String, Option<String>>,
}

impl IconCache {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let bases = [
            home.join(".local/share/icons"),
            home.join(".icons"),
            home.join(".local/share/flatpak/exports/share/icons"),
            PathBuf::from("/var/lib/flatpak/exports/share/icons"),
            crate::sys::host_path("/usr/share/icons"),
        ];
        let mut names: Vec<String> = current_theme().into_iter().collect();
        names.extend(["hicolor", "breeze-dark", "breeze", "Papirus-Dark", "Papirus", "Adwaita"].map(String::from));
        let themes = names.iter().flat_map(|t| bases.iter().map(move |b| b.join(t))).filter(|p| p.is_dir()).collect();
        Self { themes, found: HashMap::new() }
    }

    /// An image URI for the icon named `name`, if one exists.
    pub fn uri(&mut self, name: &str) -> Option<String> {
        if let Some(hit) = self.found.get(name) {
            return hit.clone();
        }
        let hit = self.lookup(name).map(|p| format!("file://{}", p.display()));
        self.found.insert(name.to_string(), hit.clone());
        hit
    }

    fn lookup(&self, name: &str) -> Option<PathBuf> {
        if name.starts_with('/') {
            return Path::new(name).is_file().then(|| PathBuf::from(name));
        }
        let lower = name.to_lowercase();
        let candidates = [name, lower.as_str()];
        for theme in &self.themes {
            for sub in SUBDIRS {
                for n in candidates {
                    for ext in ["png", "svg"] {
                        let p = theme.join(sub).join(format!("{n}.{ext}"));
                        if p.is_file() {
                            return Some(p);
                        }
                    }
                }
            }
        }
        let pixmaps = crate::sys::host_path("/usr/share/pixmaps");
        candidates.iter().flat_map(|n| ["png", "svg"].map(|e| pixmaps.join(format!("{n}.{e}")))).find(|p| p.is_file())
    }
}

/// The KDE icon theme, if configured.
fn current_theme() -> Option<String> {
    let s = std::fs::read_to_string(crate::sys::host_config_dir()?.join("kdeglobals")).ok()?;
    let mut in_icons = false;
    for line in s.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_icons = line == "[Icons]";
        } else if let Some(theme) = line.strip_prefix("Theme=").filter(|_| in_icons) {
            return Some(theme.to_string());
        }
    }
    None
}
