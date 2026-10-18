use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub use crate::levels::{Speed, Step};

pub const CONFIG_VERSION: u32 = 4;
/// Wave Link 3's limit on output mixes.
pub const MAX_MIXES: usize = 5;
/// Placeholder output replaced by the system's default sink on startup.
pub const DEFAULT_OUTPUT: &str = "@default";
pub const LATENCY_CHOICES: &[(u32, &str)] =
    &[(10, "10 ms (lowest)"), (20, "20 ms (default)"), (40, "40 ms"), (80, "80 ms (most stable)")];

pub const MIX_ICONS: &[&str] = &["🎧", "👥", "📡", "⏺", "💬", "🎮", "🔊", "🎬"];
pub const CHANNEL_ICONS: &[&str] = &["🖥", "🎵", "🌐", "🎮", "💬", "✨", "🎤", "🎬", "🔔", "📺", "🎹", "📞"];

/// A fader and its mute. `volume` is the fader position, 0–100, which is
/// linear in dB over 60 dB (see `levels`).
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
    /// Extra gain on every send of this channel.
    #[serde(default)]
    pub boost: Boost,
    /// Hidden from the Mixes page; its audio keeps flowing.
    #[serde(default)]
    pub hidden: bool,
}

/// Wave Link's channel boost: up to `MAX_BOOST_DB` on top of the faders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Boost {
    pub enabled: bool,
    pub db: f32,
}

impl Default for Boost {
    fn default() -> Self {
        Self { enabled: false, db: 6.0 }
    }
}

impl Boost {
    /// The gain it adds, in dB.
    pub fn gain_db(&self) -> f32 {
        if self.enabled {
            self.db.clamp(0.0, crate::levels::MAX_BOOST_DB)
        } else {
            0.0
        }
    }
}

/// Voice ducking: while a key channel (your mic, say) is loud, the ducked
/// channels are lowered in the mixes chosen for them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DuckingCfg {
    /// Channels whose sound triggers ducking.
    pub keys: BTreeSet<String>,
    /// Channel id -> the mixes it is ducked in.
    pub ducked: BTreeMap<String, BTreeSet<String>>,
    pub sensitivity: Step,
    pub amount: Step,
    pub speed: Speed,
}

impl Default for DuckingCfg {
    fn default() -> Self {
        Self {
            keys: BTreeSet::from(["mic".to_string()]),
            ducked: BTreeMap::new(),
            sensitivity: Step::Medium,
            amount: Step::Medium,
            speed: Speed::Medium,
        }
    }
}

impl DuckingCfg {
    /// Whether `channel` is ducked in `mix` (key channels never duck themselves).
    pub fn ducks(&self, channel: &str, mix: &str) -> bool {
        !self.keys.contains(channel) && self.ducked.get(channel).is_some_and(|m| m.contains(mix))
    }

    pub fn set(&mut self, channel: &str, mix: &str, on: bool) {
        let mixes = self.ducked.entry(channel.to_string()).or_default();
        if on {
            mixes.insert(mix.to_string());
        } else {
            mixes.remove(mix);
        }
        if mixes.is_empty() {
            self.ducked.remove(channel);
        }
    }

    /// Whether anything can be ducked at all.
    pub fn active(&self) -> bool {
        !self.keys.is_empty() && self.ducked.keys().any(|c| !self.keys.contains(c))
    }
}

/// Wave Link's colour themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Same as the desktop.
    #[default]
    System,
    Light,
    Dark,
}

/// Wave Link's window modes: the full mixer, or a small one-mix view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    #[default]
    Full,
    Compact,
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
    pub ducking: DuckingCfg,
    pub window_mode: WindowMode,
    /// The mix shown in the compact view.
    pub compact_mix: Option<String>,
    /// Serve the local control API (for Stream Deck style controllers).
    pub api_enabled: bool,
    pub theme: Theme,
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
            boost: Boost::default(),
            hidden: false,
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
            ducking: DuckingCfg::default(),
            window_mode: WindowMode::Full,
            compact_mix: None,
            api_enabled: true,
            theme: Theme::System,
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
            Ok(mut cfg) if cfg.version == 3 => {
                let _ = std::fs::copy(&path, path.with_extension("toml.v3.bak"));
                cfg.migrate_from_v3();
                cfg
            }
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

    /// Format 3 stored pulse volume percentages; format 4 stores fader
    /// positions on Wave Link's dB scale. Keep every level sounding the same.
    fn migrate_from_v3(&mut self) {
        use crate::levels::position_from_percent as pos;
        let convert = |l: &mut Level| l.volume = pos(l.volume);
        for ch in &mut self.channels {
            // An input channel's master is the device's own volume, unchanged.
            if ch.kind == ChannelKind::Virtual {
                convert(&mut ch.level);
            }
            ch.sends.values_mut().for_each(convert);
        }
        for mix in &mut self.mixes {
            convert(&mut mix.level);
        }
        self.version = CONFIG_VERSION;
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
        cfg.channels[4].boost = Boost { enabled: true, db: 4.5 };
        cfg.channels[5].hidden = true;
        cfg.ducking.keys.insert("chat".into());
        cfg.ducking.set("music", "stream", true);
        cfg.ducking.sensitivity = Step::High;
        cfg.ducking.amount = Step::Low;
        cfg.ducking.speed = Speed::Fast;
        cfg.window_mode = WindowMode::Compact;
        cfg.compact_mix = Some("stream".into());
        cfg.api_enabled = false;
        cfg.theme = Theme::Light;

        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn version_3_volumes_become_fader_positions() {
        let mut cfg = Config { version: 3, ..Default::default() };
        cfg.channels[1].level = Level::new(50, false);
        cfg.channels[1].sends.insert("stream".into(), Level::new(100, true));
        cfg.channels[0].level = Level::new(50, false); // the mic: device volume
        cfg.mixes[0].level = Level::new(0, false);
        let mut back: Config = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        back.migrate_from_v3();
        assert_eq!(back.version, CONFIG_VERSION);
        assert_eq!(back.channels[1].level, Level::new(70, false));
        assert_eq!(back.channels[1].sends["stream"], Level::new(100, true));
        assert_eq!(back.channels[0].level, Level::new(50, false));
        assert_eq!(back.mixes[0].level.volume, 0);
        assert_eq!(back.ducking, DuckingCfg::default());
    }

    #[test]
    fn ducking_never_ducks_a_key_channel() {
        let mut d = DuckingCfg::default();
        assert!(!d.active());
        d.set("mic", "my_mix", true);
        assert!(!d.ducks("mic", "my_mix"));
        assert!(!d.active());
        d.set("music", "my_mix", true);
        assert!(d.ducks("music", "my_mix"));
        assert!(!d.ducks("music", "stream"));
        assert!(d.active());
        d.set("music", "my_mix", false);
        assert!(!d.ducked.contains_key("music"));
    }
}
