//! Installed applications, read from freedesktop `.desktop` files, so apps
//! can be assigned to a channel before they ever play audio.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct InstalledApp {
    /// Rule key: the Flatpak id, or the executable's file name.
    pub key: String,
    pub name: String,
    pub icon: Option<String>,
}

pub fn installed() -> Vec<InstalledApp> {
    let home = dirs::home_dir().unwrap_or_default();
    let dirs = [
        home.join(".local/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        crate::sys::host_path("/usr/local/share/applications"),
        crate::sys::host_path("/usr/share/applications"),
    ];
    // Earlier directories override later ones, like the desktop does.
    let mut by_key: HashMap<String, InstalledApp> = HashMap::new();
    for dir in dirs.iter().rev() {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "desktop") {
                if let Some(app) = std::fs::read_to_string(&path).ok().and_then(|s| parse(&s)) {
                    by_key.insert(app.key.clone(), app);
                }
            }
        }
    }
    let mut apps: Vec<InstalledApp> = by_key.into_values().collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

fn parse(contents: &str) -> Option<InstalledApp> {
    let mut in_entry = false;
    let mut fields: HashMap<&str, &str> = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some((k, v)) = line.split_once('=') {
                fields.entry(k.trim()).or_insert(v.trim());
            }
        }
    }
    let hidden = |k: &str| fields.get(k).is_some_and(|v| v.eq_ignore_ascii_case("true"));
    if fields.get("Type").is_some_and(|t| *t != "Application") || hidden("NoDisplay") || hidden("Hidden") {
        return None;
    }
    let key = match fields.get("X-Flatpak") {
        Some(id) => id.to_string(),
        None => exec_binary(fields.get("Exec")?)?,
    };
    Some(InstalledApp { key, name: fields.get("Name")?.to_string(), icon: fields.get("Icon").map(|s| s.to_string()) })
}

/// Splits an `Exec=` line into words, keeping quoted paths with spaces whole.
fn exec_words(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    for c in exec.chars() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// The program an `Exec=` line runs, skipping `env VAR=x` wrappers.
fn exec_binary(exec: &str) -> Option<String> {
    let words = exec_words(exec);
    let mut words = words.iter();
    let mut prog = words.next()?;
    if prog.ends_with("/env") || prog == "env" {
        prog = words.find(|w| !w.contains('=') && !w.starts_with('-'))?;
    }
    let name = prog.rsplit('/').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_binary_skips_env_and_paths() {
        assert_eq!(exec_binary("spotify %U").as_deref(), Some("spotify"));
        assert_eq!(exec_binary("/usr/bin/vivaldi-stable %U").as_deref(), Some("vivaldi-stable"));
        assert_eq!(exec_binary("env GDK_BACKEND=x11 /opt/discord/Discord").as_deref(), Some("Discord"));
        assert_eq!(exec_binary("\"/opt/My App/run\" --x").as_deref(), Some("run"));
    }

    #[test]
    fn parse_uses_flatpak_id_and_skips_hidden() {
        let flatpak = "[Desktop Entry]\nType=Application\nName=Prism\nExec=/usr/bin/flatpak run org.prism.P\nIcon=org.prism.P\nX-Flatpak=org.prism.P\n";
        let app = parse(flatpak).unwrap();
        assert_eq!((app.key.as_str(), app.name.as_str()), ("org.prism.P", "Prism"));
        assert!(parse("[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n").is_none());
        // Keys from other sections (e.g. actions) must not override the entry.
        let actions = "[Desktop Entry]\nName=Real\nExec=real\n[Desktop Action new]\nName=Fake\nExec=fake\n";
        assert_eq!(parse(actions).unwrap().key, "real");
    }
}
