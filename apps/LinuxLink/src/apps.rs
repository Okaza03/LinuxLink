//! Installed applications, read from freedesktop `.desktop` files, and
//! working out which of them an audio stream belongs to. Streams often name
//! a library rather than the app ("WEBRTC VoiceEngine" for Discord,
//! "deadlock.exe" for a Proton game), so LinuxLink matches them to the app's
//! menu entry for its real name and icon.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct InstalledApp {
    /// Rule key: `steam:<appid>` for Steam games, the Flatpak id, or the
    /// executable's file name.
    pub key: String,
    pub name: String,
    pub icon: Option<String>,
    /// Lowercase names a running app may report: menu entry id, executable
    /// and window class.
    aliases: Vec<String>,
}

/// How LinuxLink shows and remembers one app.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub key: String,
    pub name: String,
    pub icon: Option<String>,
    /// Matched to an installed app (or the desktop's system sounds).
    pub recognized: bool,
}

/// What an audio stream says about the app playing it.
#[derive(Debug, Default)]
pub struct StreamInfo<'a> {
    pub binary: Option<&'a str>,
    pub app_name: Option<&'a str>,
    pub icon: Option<&'a str>,
    pub flatpak_id: Option<&'a str>,
}

/// Stream names that are an audio library or engine, not the app.
const GENERIC_NAMES: &[&str] = &[
    "webrtc voiceengine",
    "chromium",
    "chromium input",
    "libcanberra",
    "playback",
    "playstream",
    "audio stream",
    "alsa plug-in",
    "pipewire alsa",
    "sdl application",
    "openal soft",
    "fmod",
];

/// Desktop helpers that play notification and feedback sounds.
const SYSTEM_SOUNDS: &[&str] = &[
    "kded6",
    "kded5",
    "plasmashell",
    "kwin_wayland",
    "kwin_x11",
    "org_kde_powerdevil",
    "gnome-shell",
    "gsd-media-keys",
    "canberra-gtk-play",
    "libcanberra",
];

/// Programs that run games for other apps; the stream's app name says which game.
const LAUNCHERS: &[&str] =
    &["steam", "wine", "wine64", "wine-preloader", "wine64-preloader", "pv-adverb", "proton", "lutris", "heroic"];

/// Helper processes that play audio on behalf of an app.
fn alias(binary: &str) -> &str {
    match binary {
        "steamwebhelper" => "steam",
        "vivaldi-bin" => "vivaldi-stable",
        "chrome" => "google-chrome",
        "firefox-bin" => "firefox",
        other => other,
    }
}

/// "deadlock.exe" -> "deadlock", "PipeWire ALSA [valo]" -> "valo".
fn clean(name: &str) -> String {
    let name = name.trim();
    let lower = name.to_lowercase();
    let bracketed = (lower.starts_with("pipewire alsa") || lower.starts_with("alsa plug-in"))
        .then(|| name.rsplit_once('[').and_then(|(_, r)| r.strip_suffix(']')))
        .flatten();
    let name = bracketed.unwrap_or(name).trim();
    let name = if name.to_lowercase().ends_with(".exe") { &name[..name.len() - 4] } else { name };
    name.trim().to_string()
}

fn is_generic(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    n.is_empty()
        || GENERIC_NAMES
            .iter()
            .any(|g| n == *g || n.strip_prefix(g).is_some_and(|rest| rest.starts_with([' ', '#', '['])))
}

/// "Spotify (Launcher)" -> "Spotify".
fn display_name(name: &str) -> String {
    name.strip_suffix(" (Launcher)").unwrap_or(name).to_string()
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
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
                let id = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
                if let Some(app) = std::fs::read_to_string(&path).ok().and_then(|s| parse(&s, &id)) {
                    by_key.insert(app.key.clone(), app);
                }
            }
        }
    }
    let mut apps: Vec<InstalledApp> = by_key.into_values().collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Installed apps, for recognizing what plays audio.
pub struct AppIndex {
    apps: Vec<InstalledApp>,
}

impl AppIndex {
    pub fn load() -> Self {
        Self { apps: installed() }
    }

    #[cfg(test)]
    fn from(apps: Vec<InstalledApp>) -> Self {
        Self { apps }
    }

    fn find(&self, candidate: &str) -> Option<&InstalledApp> {
        let c = candidate.trim().to_lowercase();
        if c.is_empty() {
            return None;
        }
        self.apps.iter().find(|a| a.key.to_lowercase() == c || a.aliases.contains(&c)).or_else(|| {
            self.apps.iter().find(|a| a.name.to_lowercase() == c || display_name(&a.name).to_lowercase() == c)
        })
    }

    /// The app behind an audio stream.
    pub fn identify(&self, s: &StreamInfo) -> Identity {
        let binary = s.binary.map(alias).filter(|b| !b.is_empty());
        if binary.is_some_and(|b| SYSTEM_SOUNDS.contains(&b.to_lowercase().as_str()))
            || s.app_name.is_some_and(|n| n.eq_ignore_ascii_case("libcanberra"))
        {
            return Identity {
                key: "system-sounds".into(),
                name: "System sounds".into(),
                icon: Some("preferences-desktop-sound".into()),
                recognized: true,
            };
        }
        let app_name = s.app_name.map(clean).filter(|n| !is_generic(n));
        let launcher = binary.is_some_and(|b| LAUNCHERS.contains(&clean(b).to_lowercase().as_str()));
        // Most specific first: the Flatpak id, the app's own name (a game run
        // by Steam or Wine), then the executable.
        let candidates = [
            s.flatpak_id.map(String::from),
            app_name.clone(),
            binary.map(clean).filter(|_| !(launcher && app_name.is_some())),
        ];
        for c in candidates.iter().flatten() {
            if let Some(app) = self.find(c) {
                return Identity {
                    key: app.key.clone(),
                    name: display_name(&app.name),
                    icon: app.icon.clone(),
                    recognized: true,
                };
            }
        }
        // Not installed (or not in the menu): make the best of the stream.
        let name =
            app_name.clone().or_else(|| binary.map(|b| capitalized(&clean(b)))).unwrap_or_else(|| "Unknown app".into());
        let key = match (launcher, &app_name) {
            (true, Some(game)) => game.clone(),
            _ => binary.map(String::from).or_else(|| app_name.clone()).unwrap_or_else(|| name.clone()),
        };
        let icon = s
            .icon
            .map(String::from)
            .filter(|i| !LAUNCHERS.contains(&i.as_str()))
            .or_else(|| binary.map(|b| b.to_lowercase()));
        Identity { key, name, icon, recognized: false }
    }
}

/// Fixes up remembered apps: real names and icons for entries saved by older
/// versions, one entry per app (no "game" and "game.exe"), and forgets
/// unrecognized apps that aren't in a channel (they come back when they play).
pub fn tidy_known_apps(cfg: &mut crate::config::Config, index: &AppIndex) {
    use crate::config::KnownApp;
    let old_known = std::mem::take(&mut cfg.known_apps);
    let old_rules = std::mem::take(&mut cfg.app_rules);
    for (key, app) in &old_known {
        let who = index.identify(&StreamInfo {
            binary: Some(key),
            app_name: Some(&app.name),
            icon: app.icon.as_deref(),
            flatpak_id: None,
        });
        let rule = old_rules.get(key);
        if !who.recognized && rule.is_none() {
            continue;
        }
        let (key, known) = if who.recognized {
            (who.key, KnownApp { name: who.name, icon: who.icon })
        } else {
            (key.clone(), app.clone())
        };
        cfg.known_apps.entry(key.clone()).or_insert(known);
        if let Some(channel) = rule {
            cfg.app_rules.entry(key).or_insert(channel.clone());
        }
    }
    // Rules for apps picked from the installed list without playing yet.
    for (key, channel) in old_rules {
        if !old_known.contains_key(&key) {
            cfg.app_rules.entry(key).or_insert(channel);
        }
    }
}

fn parse(contents: &str, id: &str) -> Option<InstalledApp> {
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
    let exec = fields.get("Exec").copied().unwrap_or("");
    let steam_game = exec
        .split("steam://rungameid/")
        .nth(1)
        .map(|rest| rest.chars().take_while(char::is_ascii_digit).collect::<String>())
        .filter(|id| !id.is_empty());
    let binary = exec_binary(exec);
    let (key, aliases) = match (steam_game, fields.get("X-Flatpak")) {
        // Steam games run through "steam": don't alias them to it.
        (Some(game), _) => (format!("steam:{game}"), vec![id.to_string()]),
        (None, Some(flatpak)) => (flatpak.to_string(), vec![id.to_string(), flatpak.to_lowercase()]),
        (None, None) => {
            let mut aliases = vec![id.to_string()];
            aliases.extend(binary.as_ref().map(|b| b.to_lowercase()));
            aliases.extend(fields.get("StartupWMClass").map(|w| w.to_lowercase()));
            (binary?, aliases)
        }
    };
    Some(InstalledApp {
        key,
        name: display_name(fields.get("Name")?),
        icon: fields.get("Icon").map(|s| s.to_string()),
        aliases,
    })
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
        let app = parse(flatpak, "org.prism.p").unwrap();
        assert_eq!((app.key.as_str(), app.name.as_str()), ("org.prism.P", "Prism"));
        assert!(parse("[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n", "x").is_none());
        // Keys from other sections (e.g. actions) must not override the entry.
        let actions = "[Desktop Entry]\nName=Real\nExec=real\n[Desktop Action new]\nName=Fake\nExec=fake\n";
        assert_eq!(parse(actions, "real").unwrap().key, "real");
    }

    fn app(file: &str, desktop: &str) -> InstalledApp {
        parse(desktop, file).unwrap()
    }

    fn index() -> AppIndex {
        AppIndex::from(vec![
            app("discord", "[Desktop Entry]\nName=Discord\nIcon=discord\nExec=/usr/bin/discord --url -- %u\nStartupWMClass=discord"),
            app("spotify-launcher", "[Desktop Entry]\nName=Spotify (Launcher)\nIcon=spotify-launcher\nExec=spotify-launcher %U\nStartupWMClass=spotify"),
            app("steam", "[Desktop Entry]\nName=Steam\nIcon=steam\nExec=/usr/bin/steam %U"),
            app("deadlock", "[Desktop Entry]\nName=Deadlock\nIcon=steam_icon_1422450\nExec=steam steam://rungameid/1422450"),
            app("vivaldi-stable", "[Desktop Entry]\nName=Vivaldi\nIcon=vivaldi\nExec=/usr/bin/vivaldi-stable %U"),
            app("valo-appimage", "[Desktop Entry]\nName=Valo\nIcon=valo\nExec=\"/home/u/valo.AppImage\" %F\nStartupWMClass=valo-appimage"),
        ])
    }

    fn who(binary: Option<&str>, app_name: Option<&str>) -> Identity {
        index().identify(&StreamInfo { binary, app_name, ..Default::default() })
    }

    #[test]
    fn recognizes_apps_behind_generic_stream_names() {
        let discord = who(Some("Discord"), Some("WEBRTC VoiceEngine"));
        assert_eq!((discord.name.as_str(), discord.icon.as_deref()), ("Discord", Some("discord")));
        let spotify = who(None, Some("Spotify"));
        assert_eq!((spotify.name.as_str(), spotify.icon.as_deref()), ("Spotify", Some("spotify-launcher")));
        assert_eq!(who(Some("steamwebhelper"), Some("Chromium")).name, "Steam");
        assert_eq!(who(Some("vivaldi-bin"), Some("Vivaldi")).key, "vivaldi-stable");
        assert_eq!(who(None, Some("PipeWire ALSA [valo]")).name, "Valo");
    }

    #[test]
    fn games_match_their_steam_entry_with_or_without_exe() {
        let proton = who(None, Some("deadlock.exe"));
        assert_eq!((proton.key.as_str(), proton.name.as_str()), ("steam:1422450", "Deadlock"));
        assert_eq!(proton.icon.as_deref(), Some("steam_icon_1422450"));
        // A game without a menu entry: Steam's and Wine's streams merge.
        let from_steam = who(Some("steam"), Some("One Million Coins"));
        let from_wine = who(Some("wine64-preloader"), Some("One Million Coins.exe"));
        assert_eq!(from_steam.key, from_wine.key);
        assert_eq!(from_wine.name, "One Million Coins");
        // Steam itself is still Steam.
        assert_eq!(who(Some("steam"), Some("Steam")).name, "Steam");
    }

    #[test]
    fn desktop_sound_helpers_are_system_sounds() {
        assert_eq!(who(Some("kded6"), Some("libcanberra")).name, "System sounds");
        assert_eq!(who(Some("plasmashell"), Some("")).key, "system-sounds");
    }

    #[test]
    fn unknown_apps_get_a_readable_name() {
        assert_eq!(who(Some("pacat"), Some("paplay")).name, "paplay");
        assert_eq!(who(Some("mytool"), Some("Playback")).name, "Mytool");
    }

    #[test]
    fn tidying_fixes_names_merges_and_keeps_assignments() {
        use crate::config::{Config, KnownApp};
        let mut cfg = Config::default();
        let known = |name: &str, icon: Option<&str>| KnownApp { name: name.into(), icon: icon.map(String::from) };
        cfg.known_apps.insert("Discord".into(), known("WEBRTC VoiceEngine", Some("Discord")));
        cfg.known_apps.insert("deadlock.exe".into(), known("deadlock.exe", None));
        cfg.known_apps.insert("kded6".into(), known("libcanberra", None));
        cfg.known_apps.insert("plasmashell".into(), known("", Some("")));
        cfg.known_apps.insert("Browser output".into(), known("Browser output", None));
        cfg.known_apps.insert("mytool".into(), known("mytool", None));
        cfg.app_rules.insert("Discord".into(), "chat".into());
        cfg.app_rules.insert("mytool".into(), "sfx".into());

        tidy_known_apps(&mut cfg, &index());

        let names: Vec<(&str, &str)> = cfg.known_apps.iter().map(|(k, a)| (k.as_str(), a.name.as_str())).collect();
        assert_eq!(
            names,
            [
                ("discord", "Discord"),
                ("mytool", "mytool"),
                ("steam:1422450", "Deadlock"),
                ("system-sounds", "System sounds")
            ]
        );
        assert_eq!(cfg.app_rules.get("discord").map(String::as_str), Some("chat"));
        assert_eq!(cfg.app_rules.get("mytool").map(String::as_str), Some("sfx"));
        assert!(!cfg.app_rules.contains_key("Discord"));
    }
}
