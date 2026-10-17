use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const CONFIG_VERSION: u32 = 3;
pub const MAX_MIXES: usize = 5;
/// Placeholder output replaced by the system's default sink on startup.
pub const DEFAULT_OUTPUT: &str = "@default";
pub const LATENCY_CHOICES: &[(u32, &str)] =
    &[(10, "10 ms (lowest)"), (20, "20 ms (default)"), (40, "40 ms"), (80, "80 ms (most stable)")];

pub const MIX_ICONS: &[&str] = &["🎧", "👥", "📡", "⏺", "💬", "🎮", "🔊", "🎬"];
pub const CHANNEL_ICONS: &[&str] = &["🖥", "🎵", "🌐", "🎮", "💬", "✨", "🎤", "🎬", "🔔", "📺", "🎹", "📞"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub volume: u32,
    pub muted: bool,
}

impl Level {
    pub fn new(volume: u32, muted: bool) -> Self {
        Self { volume, muted }
    }

    pub fn full() -> Self {
        Self::new(100, false)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ChannelKind {
    /// A virtual output device that applications play into.
    Virtual,
    /// A hardware capture device (e.g. a microphone).
    Input { source: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelCfg {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub kind: ChannelKind,
    /// Channel master, applied on top of every send.
    pub level: Level,
    /// Mix id -> send level. A missing entry means "not routed to that mix".
    pub sends: BTreeMap<String, Level>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixCfg {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// Extra hardware sinks this mix plays on (besides your listening device,
    /// which the mix's listen toggle controls). Each belongs to one mix.
    pub outputs: Vec<String>,
    pub level: Level,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownApp {
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub channels: Vec<ChannelCfg>,
    pub mixes: Vec<MixCfg>,
    /// Channel made the system default output while LinuxLink runs.
    pub default_channel: Option<String>,
    /// Application key (binary or app name) -> channel id.
    pub app_rules: BTreeMap<String, String>,
    /// Every application seen so far, so channels can list them while idle.
    pub known_apps: BTreeMap<String, KnownApp>,
    /// Your listening device (headphones/speakers), from the top-right picker.
    pub listen_output: Option<String>,
    /// Mixes played on `listen_output`; any number can be on at once.
    pub listening: BTreeSet<String>,
    /// Devices hidden from LinuxLink's lists (they keep working elsewhere).
    pub hidden_inputs: BTreeSet<String>,
    pub hidden_outputs: BTreeSet<String>,
    /// Buffer size of every loopback, in milliseconds.
    pub latency_ms: u32,
    /// Version whose "What's new" notes were last shown.
    pub last_seen_version: Option<String>,
    /// Closing the window minimizes instead of quitting, so audio keeps working.
    pub close_to_tray: bool,
    /// AppImage only: keep a LinuxLink entry (and icon) in the app menu.
    pub appimage_menu_entry: bool,
}

impl Default for Config {
    fn default() -> Self {
        let mixes = vec![
            MixCfg {
                id: "my_mix".into(),
                name: "My Mix".into(),
                icon: "🎧".into(),
                outputs: vec![],
                level: Level::full(),
            },
            MixCfg {
                id: "stream".into(),
                name: "Stream".into(),
                icon: "📡".into(),
                outputs: vec![],
                level: Level::full(),
            },
        ];
        let channel = |id: &str, name: &str, icon: &str, kind: ChannelKind| ChannelCfg {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            kind,
            level: Level::full(),
            sends: mixes.iter().map(|m| (m.id.clone(), Level::full())).collect(),
        };
        let mut mic = channel("mic", "Microphone", "🎤", ChannelKind::Input { source: String::new() });
        // Don't hear yourself by default.
        mic.sends.insert("my_mix".into(), Level::new(100, true));
        let channels = vec![
            mic,
            channel("system", "System", "🖥", ChannelKind::Virtual),
            channel("browser", "Browser", "🌐", ChannelKind::Virtual),
            channel("chat", "Voice Chat", "💬", ChannelKind::Virtual),
            channel("music", "Music", "🎵", ChannelKind::Virtual),
            channel("game", "Games", "🎮", ChannelKind::Virtual),
            channel("sfx", "Sound Effects", "✨", ChannelKind::Virtual),
        ];
        Self {
            version: CONFIG_VERSION,
            channels,
            mixes,
            default_channel: Some("system".into()),
            app_rules: BTreeMap::new(),
            known_apps: BTreeMap::new(),
            listen_output: Some(DEFAULT_OUTPUT.into()),
            listening: BTreeSet::from(["my_mix".to_string()]),
            hidden_inputs: BTreeSet::new(),
            hidden_outputs: BTreeSet::new(),
            latency_ms: 20,
            last_seen_version: None,
            close_to_tray: true,
            appimage_menu_entry: true,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(crate::variant::CONFIG_DIR).join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let Ok(s) = std::fs::read_to_string(&path) else { return Self::default() };
        match toml::from_str::<Self>(&s) {
            Ok(cfg) if cfg.version == CONFIG_VERSION => cfg,
            result => {
                let backup = path.with_extension("toml.bak");
                let _ = std::fs::copy(&path, &backup);
                let why = result.err().map_or("older format".to_string(), |e| e.to_string());
                eprintln!(
                    "linuxlink: starting with default settings ({why}); old config saved to {}",
                    backup.display()
                );
                Self::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write a temporary file and rename it over the real one, so a crash
        // or power cut mid-save can never leave a half-written config behind.
        let tmp = path.with_extension("toml.tmp");
        {
            let mut file = std::fs::File::create(&tmp)?;
            std::io::Write::write_all(&mut file, toml::to_string_pretty(self)?.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn channel(&self, id: &str) -> Option<&ChannelCfg> {
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn mix(&self, id: &str) -> Option<&MixCfg> {
        self.mixes.iter().find(|m| m.id == id)
    }

    /// Returns an id derived from `name` that no channel or mix uses yet.
    pub fn unique_id(&self, name: &str) -> String {
        let base: String =
            name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let base = if base.trim_matches('_').is_empty() { "item".to_string() } else { base };
        let taken = |id: &str| self.channel(id).is_some() || self.mix(id).is_some();
        let mut id = base.clone();
        let mut n = 2;
        while taken(&id) {
            id = format!("{base}_{n}");
            n += 1;
        }
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything the user can change must survive a save and reload.
    #[test]
    fn every_setting_survives_save_and_load() {
        let mut cfg = Config::default();
        cfg.channels[1].level = Level::new(42, true);
        cfg.channels[1].sends.insert("stream".into(), Level::new(13, true));
        cfg.channels[2].sends.remove("my_mix");
        cfg.channels[3].name = "Discord & Co".into();
        cfg.channels[3].icon = "📞".into();
        cfg.channels[0].kind = ChannelKind::Input { source: "alsa_input.usb-Mic".into() };
        cfg.mixes[1].outputs = vec!["alsa_output.capture-card".into()];
        cfg.mixes[1].level = Level::new(77, false);
        cfg.mixes.push(MixCfg {
            id: "rec".into(),
            name: "Recording".into(),
            icon: "⏺".into(),
            outputs: vec![],
            level: Level::new(60, true),
        });
        cfg.app_rules.insert("spotify".into(), "music".into());
        cfg.app_rules.insert("com.discordapp.Discord".into(), "chat".into());
        cfg.known_apps
            .insert("spotify".into(), KnownApp { name: "Spotify".into(), icon: Some("spotify-client".into()) });
        cfg.listen_output = Some("bluez_output.AA_BB.1".into());
        cfg.listening = BTreeSet::from(["my_mix".to_string(), "rec".to_string()]);
        cfg.hidden_inputs.insert("alsa_input.line-in".into());
        cfg.hidden_outputs.insert("alsa_output.hdmi".into());
        cfg.default_channel = Some("music".into());
        cfg.latency_ms = 40;
        cfg.last_seen_version = Some("0.1.0".into());
        cfg.close_to_tray = false;
        cfg.appimage_menu_entry = false;

        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, cfg);
    }
}
