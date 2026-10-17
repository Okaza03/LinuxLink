//! The audio graph, built from PipeWire null sinks and loopbacks:
//!
//! ```text
//!  app ──▶ [channel sink] ─┬─send──▶ [mix sink "My Mix"] ──▶ headphones (any number of outputs)
//!                          └─send──▶ [mix sink "Stream"] ──▶ "LinuxLink Stream" virtual mic
//!  mic ────────────────────send──▶ (any mix)
//! ```
//!
//! Every send is one loopback stream whose volume is `channel master × send`,
//! so mixes are fully independent, like Wave Link's routing matrix.

use crate::config::{ChannelCfg, ChannelKind, Config, Level, MixCfg, DEFAULT_OUTPUT};
use crate::pactl::{self, proplist, Device};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const PREFIX: &str = "linuxlink_";
/// How long input volumes are protected after LinuxLink changes devices.
const GUARD_FOR: Duration = Duration::from_secs(3);
/// `application.id` of LinuxLink's own streams, to tell them from apps.
pub const STREAM_TAG: &str = "linuxlink";

pub fn channel_sink(id: &str) -> String {
    format!("{PREFIX}ch_{id}")
}

pub fn mix_sink(id: &str) -> String {
    format!("{PREFIX}mix_{id}")
}

/// Virtual microphone carrying a mix, for OBS / Discord.
pub fn mix_source(id: &str) -> String {
    format!("{PREFIX}mixsrc_{id}")
}

/// The source carrying a channel's audio (what sends and meters read).
pub fn channel_capture(ch: &ChannelCfg) -> String {
    match &ch.kind {
        ChannelKind::Virtual => format!("{}.monitor", channel_sink(&ch.id)),
        ChannelKind::Input { source } => source.clone(),
    }
}

pub fn mix_capture(id: &str) -> String {
    format!("{}.monitor", mix_sink(id))
}

/// Volume and mute actually applied to a send. For app channels that is
/// channel master × send fader; an input channel's master is the hardware
/// device's own volume, which already shapes what the send captures.
pub fn effective(ch: &ChannelCfg, send: &Level) -> Level {
    match ch.kind {
        ChannelKind::Virtual => Level::new(ch.level.volume * send.volume / 100, ch.level.muted || send.muted),
        ChannelKind::Input { .. } => send.clone(),
    }
}

pub enum Command {
    /// New settings from the UI, with how many of the engine's own listening
    /// switches the UI had seen (so a stale UI can't undo a newer switch).
    Apply(Box<Config>, u64),
    SetSource {
        name: String,
        level: Level,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct AppStream {
    pub index: u32,
    pub key: String,
    /// Lowercase identifiers an app rule may use: binary, app name, Flatpak id.
    pub ids: Vec<String>,
    pub name: String,
    pub icon: Option<String>,
    pub sink: String,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub apps: Vec<AppStream>,
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    pub errors: Vec<String>,
    /// The engine's listening device, and how often it switched by itself
    /// (when you picked another output in the system's volume controls).
    pub listen_output: Option<String>,
    pub listen_switches: u64,
}

/// A loopback module. `module` is `None` when loading failed (device
/// missing); it is retried once the device shows up.
struct Loop {
    module: Option<u32>,
    sink_input: Option<u32>,
    applied: Option<Level>,
}

impl Loop {
    fn unload(self) {
        if let Some(m) = self.module {
            let _ = pactl::unload_module(m);
        }
    }
}

struct ChannelRt {
    name: String,
    kind: ChannelKind,
    sink_module: Option<u32>,
}

struct MixRt {
    name: String,
    modules: Vec<u32>,
    outputs: HashMap<String, Loop>,
    applied: Option<Level>,
}

struct Engine {
    cfg: Config,
    prev_default: String,
    channels: HashMap<String, ChannelRt>,
    mixes: HashMap<String, MixRt>,
    /// (channel id, mix id) -> send loopback.
    sends: HashMap<(String, String), Loop>,
    applied_default: Option<Option<String>>,
    applied_latency: u32,
    seen_streams: HashSet<u32>,
    live_apps: Vec<AppStream>,
    snapshot: Arc<Mutex<Snapshot>>,
    dirty_since: Option<Instant>,
    needs_sync: bool,
    /// Hardware outputs seen in the last refresh.
    known_sinks: HashSet<String>,
    /// Every device (inputs and outputs) seen in the last refresh.
    device_names: HashSet<String>,
    /// Their PipeWire node ids.
    node_ids: HashMap<String, u32>,
    /// Connected Bluetooth outputs, oldest connection first.
    bluetooth: Vec<String>,
    /// Counts refreshes, to check Bluetooth outputs every few of them.
    refreshes: u32,
    /// Installed apps, to name the apps playing audio.
    app_index: crate::apps::AppIndex,
    /// Volume and mute of each input device, as of the last refresh.
    input_levels: HashMap<String, (u32, bool)>,
    /// Until when input volumes are protected after LinuxLink added or
    /// removed devices (see `guard_inputs`).
    guard_until: Option<Instant>,
    listen_switches: u64,
}

/// Whether `pw-cli info` output describes a node in the error state.
fn node_failed(info: &str) -> bool {
    info.lines().any(|l| l.trim_start_matches(['*', ' ', '\t']).starts_with("state:") && l.contains("\"error\""))
}

/// Removes modules left behind by a previous run that didn't exit cleanly.
pub fn cleanup_stale() {
    if let Ok(modules) = pactl::modules() {
        for (id, args) in modules.into_iter().rev() {
            if args.contains(PREFIX) {
                let _ = pactl::unload_module(id);
            }
        }
    }
}

fn is_ours(name: &str) -> bool {
    name.starts_with(PREFIX)
}

fn is_bluetooth(sink: &str) -> bool {
    sink.starts_with("bluez_output.")
}

fn is_hardware_source(name: &str) -> bool {
    !is_ours(name) && !name.ends_with(".monitor")
}

/// The system default output found at startup, restored on exit.
pub struct Defaults {
    pub sink: String,
}

/// Cleans up stale state and fills in devices the config leaves unset.
pub fn prepare(cfg: &mut Config) -> Result<Defaults> {
    cleanup_stale();
    let sinks = pactl::sinks()?;
    let mut prev = pactl::default_sink().unwrap_or_default();
    if is_ours(&prev) || !sinks.iter().any(|s| s.name == prev) {
        prev = sinks.iter().find(|s| !is_ours(&s.name)).map(|s| s.name.clone()).unwrap_or_default();
    }
    if cfg.listen_output.as_deref().is_none_or(|o| o == DEFAULT_OUTPUT) {
        // Older configs had no listening device; the mix you heard played on
        // its first output, so keep using that one.
        let heard = cfg
            .mixes
            .iter()
            .find(|m| cfg.listening.contains(&m.id))
            .and_then(|m| m.outputs.iter().find(|o| *o != DEFAULT_OUTPUT && sinks.iter().any(|s| &s.name == *o)))
            .cloned();
        cfg.listen_output = heard.or_else(|| (!prev.is_empty()).then(|| prev.clone()));
    }
    // Your listening device isn't connected: use a connected Bluetooth
    // headset if there is one, else the system's output.
    if cfg.listen_output.as_ref().is_some_and(|o| !sinks.iter().any(|s| &s.name == o)) {
        let bluetooth = sinks.iter().find(|s| is_bluetooth(&s.name)).map(|s| s.name.clone());
        cfg.listen_output = bluetooth.or_else(|| (!prev.is_empty()).then(|| prev.clone()));
    }
    for mix in &mut cfg.mixes {
        for out in &mut mix.outputs {
            if out == DEFAULT_OUTPUT {
                *out = prev.clone();
            }
        }
        // The listening device is driven by the listen toggles instead
        // (older configs listed it as a plain output of My Mix).
        mix.outputs.retain(|o| !o.is_empty() && Some(o) != cfg.listen_output.as_ref());
    }

    crate::apps::tidy_known_apps(cfg, &crate::apps::AppIndex::load());

    let sources = pactl::sources()?;
    let def_src = pactl::default_source()
        .ok()
        .filter(|s| is_hardware_source(s) && sources.iter().any(|x| &x.name == s))
        .or_else(|| sources.iter().find(|s| is_hardware_source(&s.name)).map(|s| s.name.clone()))
        .unwrap_or_default();
    for ch in &mut cfg.channels {
        if let ChannelKind::Input { source } = &mut ch.kind {
            if source.is_empty() {
                *source = def_src.clone();
            }
        }
    }
    Ok(Defaults { sink: prev })
}

fn loopback(source: &str, sink: &str, label: &str, latency_ms: u32) -> Result<u32> {
    let props = proplist(&[
        ("media.name", label),
        ("application.id", STREAM_TAG),
        ("application.name", "LinuxLink"),
        // Keep WirePlumber from restoring old volumes over ours.
        ("state.restore-props", "false"),
    ]);
    pactl::load_module(
        "module-loopback",
        &[
            format!("source={source}"),
            format!("sink={sink}"),
            format!("latency_msec={latency_ms}"),
            "source_dont_move=true".into(),
            "sink_dont_move=true".into(),
            format!("sink_input_properties={props}"),
            format!("source_output_properties={props}"),
        ],
    )
}

fn null_sink(name: &str, description: &str) -> Result<u32> {
    pactl::load_module(
        "module-null-sink",
        &[format!("sink_name={name}"), format!("sink_properties={}", proplist(&[("device.description", description)]))],
    )
}

impl Engine {
    fn report(&self, e: anyhow::Error) {
        eprintln!("linuxlink: {e:#}");
        let mut snap = self.snapshot.lock().unwrap();
        snap.errors.push(format!("{e:#}"));
        let excess = snap.errors.len().saturating_sub(5);
        snap.errors.drain(..excess);
    }

    fn try_loop(&self, source: &str, sink: &str, label: &str) -> Loop {
        let module = loopback(source, sink, label, self.cfg.latency_ms).map_err(|e| self.report(e)).ok();
        Loop { module, sink_input: None, applied: None }
    }

    fn create_mix(&self, mix: &MixCfg) -> Option<MixRt> {
        let desc = format!("LinuxLink {}", mix.name);
        let sink = null_sink(&mix_sink(&mix.id), &desc).map_err(|e| self.report(e)).ok()?;
        let mut modules = vec![sink];
        let source = pactl::load_module(
            "module-remap-source",
            &[
                format!("master={}", mix_capture(&mix.id)),
                format!("source_name={}", mix_source(&mix.id)),
                format!("source_properties={}", proplist(&[("device.description", &desc)])),
            ],
        );
        match source {
            Ok(m) => modules.push(m),
            Err(e) => self.report(e),
        }
        Some(MixRt { name: mix.name.clone(), modules, outputs: HashMap::new(), applied: None })
    }

    fn destroy_mix(rt: MixRt) {
        for (_, l) in rt.outputs {
            l.unload();
        }
        for m in rt.modules.into_iter().rev() {
            let _ = pactl::unload_module(m);
        }
    }

    fn create_channel(&self, ch: &ChannelCfg) -> ChannelRt {
        let sink_module = match ch.kind {
            ChannelKind::Virtual => {
                null_sink(&channel_sink(&ch.id), &format!("LinuxLink {}", ch.name)).map_err(|e| self.report(e)).ok()
            }
            ChannelKind::Input { .. } => None,
        };
        ChannelRt { name: ch.name.clone(), kind: ch.kind.clone(), sink_module }
    }

    fn drop_sends(&mut self, pred: impl Fn(&(String, String)) -> bool) {
        let keys: Vec<_> = self.sends.keys().filter(|k| pred(k)).cloned().collect();
        for k in keys {
            self.sends.remove(&k).unwrap().unload();
        }
    }

    /// Brings loaded modules in line with the config's structure.
    /// Returns whether any streams were created or destroyed.
    fn sync_graph(&mut self) -> bool {
        self.needs_sync = false;
        let mut changed = false;

        // A latency change rebuilds every loopback with the new buffer size.
        if self.cfg.latency_ms != self.applied_latency {
            self.applied_latency = self.cfg.latency_ms;
            self.drop_sends(|_| true);
            for rt in self.mixes.values_mut() {
                for (_, l) in rt.outputs.drain() {
                    l.unload();
                }
            }
        }

        // Channels and mixes that were removed or renamed (descriptions are
        // fixed at creation, so a rename means a rebuild).
        let stale_ch: Vec<String> = self
            .channels
            .iter()
            .filter(|(id, rt)| self.cfg.channel(id).is_none_or(|c| c.kind != rt.kind || c.name != rt.name))
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale_ch {
            self.drop_sends(|(c, _)| *c == id);
            if let Some(m) = self.channels.remove(&id).unwrap().sink_module {
                let _ = pactl::unload_module(m);
            }
            changed = true;
        }
        let stale_mix: Vec<String> = self
            .mixes
            .iter()
            .filter(|(id, rt)| self.cfg.mix(id).is_none_or(|m| m.name != rt.name))
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale_mix {
            self.drop_sends(|(_, m)| *m == id);
            Self::destroy_mix(self.mixes.remove(&id).unwrap());
            changed = true;
        }

        let listen_target = self.listen_target();
        for mix in self.cfg.mixes.clone() {
            if !self.mixes.contains_key(&mix.id) {
                if let Some(rt) = self.create_mix(&mix) {
                    self.mixes.insert(mix.id.clone(), rt);
                    changed = true;
                }
            }
            let mut outputs = mix.outputs.clone();
            if let Some(listen) = listen_target.as_ref().filter(|_| self.cfg.listening.contains(&mix.id)) {
                if !outputs.contains(listen) {
                    outputs.push(listen.clone());
                }
            }
            let Some(rt) = self.mixes.get_mut(&mix.id) else { continue };
            let gone: Vec<String> = rt.outputs.keys().filter(|o| !outputs.contains(o)).cloned().collect();
            for o in gone {
                rt.outputs.remove(&o).unwrap().unload();
                changed = true;
            }
            let missing: Vec<&String> = outputs.iter().filter(|o| !rt.outputs.contains_key(*o)).collect();
            for out in missing {
                let l = self.try_loop(&mix_capture(&mix.id), out, &format!("LinuxLink {} output", mix.name));
                self.mixes.get_mut(&mix.id).unwrap().outputs.insert(out.clone(), l);
                changed = true;
            }
        }

        for ch in self.cfg.channels.clone() {
            if !self.channels.contains_key(&ch.id) {
                let rt = self.create_channel(&ch);
                self.channels.insert(ch.id.clone(), rt);
                changed = true;
            }
        }

        // Sends: one loopback per routed (channel, mix) cell.
        let wanted: HashSet<(String, String)> = self
            .cfg
            .channels
            .iter()
            .flat_map(|c| c.sends.keys().map(|m| (c.id.clone(), m.clone())))
            .filter(|(c, m)| self.channels.contains_key(c) && self.mixes.contains_key(m))
            .collect();
        let before = self.sends.len();
        self.drop_sends(|k| !wanted.contains(k));
        changed |= self.sends.len() != before;
        for key in wanted {
            if self.sends.contains_key(&key) {
                continue;
            }
            let ch = self.cfg.channel(&key.0).unwrap();
            let mix = self.cfg.mix(&key.1).unwrap();
            let l = self.try_loop(
                &channel_capture(ch),
                &mix_sink(&mix.id),
                &format!("LinuxLink {} to {}", ch.name, mix.name),
            );
            self.sends.insert(key, l);
            changed = true;
        }

        if changed {
            self.guard_until = Some(Instant::now() + GUARD_FOR);
            // Recreated sinks drop their streams and the default; re-route them.
            self.seen_streams.clear();
            self.applied_default = None;
        }
        let want = self.default_channel_sink();
        if self.applied_default.as_ref() != Some(&want) {
            let sink = want.clone().unwrap_or_else(|| self.prev_default.clone());
            if !sink.is_empty() {
                if let Err(e) = pactl::set_default_sink(&sink) {
                    self.report(e);
                }
            }
            self.applied_default = Some(want);
        }
        changed
    }

    fn default_channel_sink(&self) -> Option<String> {
        self.cfg
            .default_channel
            .as_ref()
            .filter(|id| self.cfg.channel(id).is_some_and(|c| c.kind == ChannelKind::Virtual))
            .map(|id| channel_sink(id))
    }

    /// Where an application's streams go: its channel, if a rule matches.
    /// A rule on the stream's own key wins over one on another identifier.
    fn app_target(&self, key: &str, ids: &[String]) -> Option<String> {
        self.cfg
            .app_rules
            .get(key)
            .or_else(|| self.cfg.app_rules.iter().find(|(k, _)| ids.contains(&k.to_lowercase())).map(|(_, c)| c))
            .and_then(|id| self.cfg.channel(id))
            .filter(|c| c.kind == ChannelKind::Virtual)
            .map(|c| channel_sink(&c.id))
    }

    /// Some apps (Discord's voice engine, for one) reset the system microphone
    /// volume whenever devices appear or disappear, which LinuxLink's own
    /// changes cause. For a moment after each such change, put the input
    /// channels' volumes back to what they were; otherwise just track them.
    fn guard_inputs(&mut self, sources: &[Device]) {
        let guarding = self.guard_until.is_some_and(|t| Instant::now() < t);
        if !guarding {
            self.guard_until = None;
        }
        for s in sources {
            let Some(volume) = s.volume else { continue };
            let input_channel =
                self.cfg.channels.iter().any(|c| matches!(&c.kind, ChannelKind::Input { source } if *source == s.name));
            match self.input_levels.get(&s.name) {
                Some(&(was, was_muted))
                    if guarding && input_channel && (was.abs_diff(volume) > 1 || was_muted != s.muted) =>
                {
                    eprintln!("linuxlink: another app changed {} to {volume}%; restoring {was}%", s.name);
                    let _ = pactl::set_source_volume(&s.name, was);
                    let _ = pactl::set_source_mute(&s.name, was_muted);
                }
                _ => {
                    self.input_levels.insert(s.name.clone(), (volume, s.muted));
                }
            }
        }
    }

    /// Where the mixes you listen to play: your listening device, or the
    /// system's previous output while that device is disconnected.
    fn listen_target(&self) -> Option<String> {
        let wanted = self.cfg.listen_output.clone()?;
        if self.known_sinks.is_empty() || self.known_sinks.contains(&wanted) {
            return Some(wanted);
        }
        self.fallback_output().or(Some(wanted))
    }

    /// Where to play when your listening device is gone: the most recently
    /// connected Bluetooth headset, else the system's previous output, else
    /// any other output (preferring anything over HDMI/S/PDIF).
    fn fallback_output(&self) -> Option<String> {
        let wanted = self.cfg.listen_output.as_ref();
        let usable = |n: &&String| self.known_sinks.contains(*n) && Some(*n) != wanted;
        let bluetooth = self.bluetooth.iter().rev().find(usable).cloned();
        let previous = Some(&self.prev_default).filter(usable).cloned();
        let any = || {
            let mut others: Vec<&String> = self.known_sinks.iter().filter(usable).collect();
            others.sort_by_key(|n| (n.contains("hdmi") || n.contains("SPDIF"), n.to_string()));
            others.first().map(|n| n.to_string())
        };
        bluetooth.or(previous).or_else(any)
    }

    fn switch_listening(&mut self, output: String) {
        if self.cfg.listen_output.as_ref() != Some(&output) {
            self.cfg.listen_output = Some(output);
            self.listen_switches += 1;
            self.dirty_since.get_or_insert_with(Instant::now);
            self.needs_sync = true;
        }
    }

    /// Bluetooth outputs sometimes fail when streams start or stop ("Failure
    /// in Bluetooth audio transport") and then stay silent in an error state
    /// until something reconnects to them. Reconnect the ones LinuxLink feeds.
    fn heal_bluetooth(&mut self) {
        let fed: HashSet<String> = self
            .mixes
            .values()
            .flat_map(|rt| rt.outputs.iter().filter(|(_, l)| l.module.is_some()).map(|(n, _)| n.clone()))
            .filter(|n| is_bluetooth(n))
            .collect();
        for name in fed {
            let Some(id) = self.node_ids.get(&name) else { continue };
            let Ok(out) = crate::sys::host_command("pw-cli").args(["info", &id.to_string()]).output() else { continue };
            if !node_failed(&String::from_utf8_lossy(&out.stdout)) {
                continue;
            }
            eprintln!("linuxlink: {name} stopped working; reconnecting");
            for rt in self.mixes.values_mut() {
                if let Some(l) = rt.outputs.remove(&name) {
                    l.unload();
                }
            }
            // Suspending the device clears its error state.
            let _ = pactl::suspend_sink(&name, true);
            std::thread::sleep(Duration::from_millis(300));
            let _ = pactl::suspend_sink(&name, false);
            self.needs_sync = true;
        }
    }

    fn refresh(&mut self) -> Result<()> {
        let sinks = pactl::sinks()?;
        let sources = pactl::sources()?;
        let inputs = pactl::sink_inputs()?;

        // Follow the listening device as outputs come and go.
        // Devices coming or going (a headset connecting, LinuxLink's own
        // changes) make some apps reset the microphone volume; guard it.
        let devices: HashSet<String> = sinks.iter().chain(&sources).map(|d| d.name.clone()).collect();
        if devices != self.device_names {
            if !self.device_names.is_empty() {
                self.guard_until = Some(Instant::now() + GUARD_FOR);
            }
            self.device_names = devices;
        }
        self.guard_inputs(&sources);
        let hardware: HashSet<String> = sinks.iter().filter(|s| !is_ours(&s.name)).map(|s| s.name.clone()).collect();
        self.node_ids = sinks.iter().filter_map(|s| Some((s.name.clone(), s.node_id?))).collect();
        if hardware != self.known_sinks {
            let before = self.listen_target();
            let first_look = self.known_sinks.is_empty();
            let connected: Vec<String> =
                hardware.iter().filter(|n| is_bluetooth(n) && !self.known_sinks.contains(*n)).cloned().collect();
            self.bluetooth.retain(|n| hardware.contains(n));
            self.bluetooth.extend(connected.iter().cloned());
            self.known_sinks = hardware;
            if let Some(headset) = connected.last().filter(|_| !first_look) {
                // A Bluetooth headset that just connected is what you want to hear.
                self.switch_listening(headset.clone());
            } else if self.cfg.listen_output.as_ref().is_some_and(|o| !self.known_sinks.contains(o)) {
                // Your listening device went away: carry on with the next best.
                if let Some(next) = self.fallback_output() {
                    self.switch_listening(next);
                }
            }
            self.needs_sync |= self.listen_target() != before;
        }
        // PipeWire removes a loopback when its device disappears; rebuild
        // any that are gone.
        if let Ok(modules) = pactl::modules() {
            let loaded: HashSet<u32> = modules.iter().map(|(id, _)| *id).collect();
            let dead = |l: &Loop| l.module.is_some_and(|m| !loaded.contains(&m));
            let before = self.sends.len() + self.mixes.values().map(|rt| rt.outputs.len()).sum::<usize>();
            self.sends.retain(|_, l| !dead(l));
            for rt in self.mixes.values_mut() {
                rt.outputs.retain(|_, l| !dead(l));
            }
            let after = self.sends.len() + self.mixes.values().map(|rt| rt.outputs.len()).sum::<usize>();
            self.needs_sync |= after != before;
        }
        self.refreshes = self.refreshes.wrapping_add(1);
        if self.refreshes.is_multiple_of(4) {
            self.heal_bluetooth();
        }
        // Pick up apps installed while LinuxLink runs, about once a minute.
        if self.refreshes.is_multiple_of(120) {
            self.app_index = crate::apps::AppIndex::load();
        }
        // You picked another output (e.g. in the volume applet) while
        // LinuxLink owns the default: listen there, and route apps through
        // LinuxLink again.
        if let Some(Some(ours)) = &self.applied_default {
            if let Ok(current) = pactl::default_sink() {
                if current != *ours && !is_ours(&current) && self.known_sinks.contains(&current) {
                    self.cfg.listen_output = Some(current);
                    self.listen_switches += 1;
                    self.dirty_since.get_or_insert_with(Instant::now);
                    self.applied_default = None;
                    self.needs_sync = true;
                }
            }
        }

        // Re-apply anything that drifted, e.g. WirePlumber restoring a saved
        // stream volume or another mixer app touching our streams.
        let drifted = |applied: &Option<Level>, volume: Option<u32>, muted: bool| {
            applied.as_ref().is_some_and(|a| a.muted != muted || volume.is_some_and(|v| v.abs_diff(a.volume) > 1))
        };
        let by_module: HashMap<u32, &pactl::SinkInput> =
            inputs.iter().filter_map(|i| Some((i.owner_module?, i))).collect();
        for send in self.sends.values_mut() {
            let si = send.module.and_then(|m| by_module.get(&m));
            if si.map(|i| i.index) != send.sink_input || si.is_some_and(|i| drifted(&send.applied, i.volume, i.muted)) {
                send.sink_input = si.map(|i| i.index);
                send.applied = None;
            }
        }
        for (id, rt) in &mut self.mixes {
            if let Some(s) = sinks.iter().find(|s| s.name == mix_sink(id)) {
                if drifted(&rt.applied, s.volume, s.muted) {
                    rt.applied = None;
                }
            }
        }

        // Retry loopbacks whose device has (re)appeared.
        let source_names: HashSet<&str> = sources.iter().map(|s| s.name.as_str()).collect();
        let sink_set: HashSet<&str> = sinks.iter().map(|s| s.name.as_str()).collect();
        let retry: Vec<(String, String)> = self
            .sends
            .iter()
            .filter(|(k, l)| {
                l.module.is_none()
                    && self.cfg.channel(&k.0).is_some_and(|c| source_names.contains(channel_capture(c).as_str()))
            })
            .map(|(k, _)| k.clone())
            .collect();
        for k in retry {
            self.sends.remove(&k);
            self.needs_sync = true;
        }
        for rt in self.mixes.values_mut() {
            let before = rt.outputs.len();
            rt.outputs.retain(|out, l| l.module.is_some() || !sink_set.contains(out.as_str()));
            self.needs_sync |= rt.outputs.len() != before;
        }

        let sink_names: HashMap<u32, &str> = sinks.iter().map(|s| (s.index, s.name.as_str())).collect();
        let live: HashSet<u32> = inputs.iter().map(|i| i.index).collect();
        self.seen_streams.retain(|i| live.contains(i));

        let mut apps = Vec::new();
        // Streams created by modules (loopbacks from other tools) aren't apps.
        for si in inputs.iter().filter(|i| i.prop("application.id") != Some(STREAM_TAG) && i.owner_module.is_none()) {
            let who = self.app_index.identify(&crate::apps::StreamInfo {
                binary: si.prop("application.process.binary"),
                app_name: si.prop("application.name"),
                icon: si.prop("application.icon_name"),
                flatpak_id: si.prop("pipewire.access.portal.app_id").or(si.prop("application.flatpak.id")),
            });
            let (key, name, icon) = (who.key, who.name, who.icon);
            let mut ids: Vec<String> = [
                "application.process.binary",
                "application.name",
                "pipewire.access.portal.app_id",
                "application.flatpak.id",
            ]
            .iter()
            .filter_map(|p| si.prop(p).map(str::to_lowercase))
            .collect();
            ids.push(key.to_lowercase());
            ids.dedup();
            let mut sink = sink_names.get(&si.sink).unwrap_or(&"").to_string();

            if self.seen_streams.insert(si.index) {
                if let Some(target) = self.app_target(&key, &ids) {
                    if sink != target {
                        match pactl::move_sink_input(si.index, &target) {
                            Ok(()) => sink = target,
                            Err(e) => self.report(e),
                        }
                    }
                }
            }
            apps.push(AppStream { index: si.index, key, ids, name, icon, sink });
        }
        apps.sort_by_key(|a| a.name.to_lowercase());
        self.live_apps = apps.clone();

        let mut snap = self.snapshot.lock().unwrap();
        snap.apps = apps;
        snap.outputs = sinks.into_iter().filter(|s| !is_ours(&s.name)).collect();
        snap.inputs = sources.into_iter().filter(|s| is_hardware_source(&s.name)).collect();
        snap.listen_output = self.cfg.listen_output.clone();
        snap.listen_switches = self.listen_switches;
        Ok(())
    }

    /// Pushes fader and mute state that differs from what was last applied.
    fn reconcile_levels(&mut self) {
        let mut errors = Vec::new();
        for ch in &self.cfg.channels {
            for (mix_id, send_level) in &ch.sends {
                let Some(send) = self.sends.get_mut(&(ch.id.clone(), mix_id.clone())) else { continue };
                let Some(si) = send.sink_input else { continue };
                let want = effective(ch, send_level);
                if send.applied.as_ref() != Some(&want) {
                    // Record even on failure; a new sink-input index resets it.
                    errors.extend(pactl::set_sink_input_volume(si, want.volume).err());
                    errors.extend(pactl::set_sink_input_mute(si, want.muted).err());
                    send.applied = Some(want);
                }
            }
        }
        for mix in &self.cfg.mixes {
            let Some(rt) = self.mixes.get_mut(&mix.id) else { continue };
            if rt.applied.as_ref() != Some(&mix.level) {
                rt.applied = Some(mix.level.clone());
                errors.extend(pactl::set_sink_volume(&mix_sink(&mix.id), mix.level.volume).err());
                errors.extend(pactl::set_sink_mute(&mix_sink(&mix.id), mix.level.muted).err());
            }
        }
        for e in errors {
            self.report(e);
        }
    }

    /// Returns whether the graph changed shape and needs a refresh.
    fn apply(&mut self, mut cfg: Config, listen_switches_seen: u64) -> bool {
        // The UI sent this before it saw our latest automatic switch (a
        // headset connecting, say): keep the switch.
        if listen_switches_seen < self.listen_switches {
            cfg.listen_output = self.cfg.listen_output.clone();
        }
        if cfg == self.cfg {
            return false;
        }
        let old_rules = std::mem::replace(&mut self.cfg, cfg).app_rules;
        self.dirty_since.get_or_insert_with(Instant::now);
        let changed = self.sync_graph();

        // Move live streams of apps whose channel assignment changed.
        let fallback = self.default_channel_sink().unwrap_or_else(|| self.prev_default.clone());
        let changed_keys: Vec<String> = old_rules
            .keys()
            .chain(self.cfg.app_rules.keys())
            .filter(|k| old_rules.get(*k) != self.cfg.app_rules.get(*k))
            .map(|k| k.to_lowercase())
            .collect();
        let mut moved = false;
        for app in &self.live_apps {
            let affected = changed_keys.iter().any(|k| *k == app.key.to_lowercase() || app.ids.contains(k));
            if !affected {
                continue;
            }
            let target = self.app_target(&app.key, &app.ids).unwrap_or_else(|| fallback.clone());
            if app.sink != target {
                if let Err(e) = pactl::move_sink_input(app.index, &target) {
                    self.report(e);
                }
                moved = true;
            }
        }
        changed || moved
    }

    fn save_if_due(&mut self) {
        if self.dirty_since.is_some_and(|t| t.elapsed() > Duration::from_millis(300)) {
            self.dirty_since = None;
            if let Err(e) = self.cfg.save() {
                self.report(e);
            }
        }
    }

    fn shutdown(&mut self) {
        // Leave the system output on the device you were listening on.
        let target = self.listen_target().filter(|t| !t.is_empty()).unwrap_or_else(|| self.prev_default.clone());
        if !target.is_empty() {
            let _ = pactl::set_default_sink(&target);
        }

        // Taking LinuxLink's devices away makes some apps (Discord's voice
        // engine, for one) reset the system microphone volume. Put the input
        // channels back to the levels they had while LinuxLink was running.
        let guarded: Vec<(String, u32, bool)> = self
            .cfg
            .channels
            .iter()
            .filter_map(|c| match &c.kind {
                ChannelKind::Input { source } => {
                    let (volume, muted) = *self.input_levels.get(source)?;
                    Some((source.clone(), volume, muted))
                }
                ChannelKind::Virtual => None,
            })
            .collect();

        for (_, l) in self.sends.drain() {
            l.unload();
        }
        for (_, rt) in self.channels.drain() {
            if let Some(m) = rt.sink_module {
                let _ = pactl::unload_module(m);
            }
        }
        for (_, rt) in self.mixes.drain() {
            Self::destroy_mix(rt);
        }
        if let Err(e) = self.cfg.save() {
            eprintln!("linuxlink: saving config: {e:#}");
        }

        for _ in 0..6 {
            std::thread::sleep(Duration::from_millis(500));
            let Ok(now) = pactl::sources() else { break };
            for (name, volume, muted) in &guarded {
                let Some(s) = now.iter().find(|s| &s.name == name) else { continue };
                if s.volume.is_some_and(|v| v.abs_diff(*volume) > 1) || s.muted != *muted {
                    eprintln!("linuxlink: {name} volume was changed on exit; restoring {volume}%");
                    let _ = pactl::set_source_volume(name, *volume);
                    let _ = pactl::set_source_mute(name, *muted);
                }
            }
        }
    }
}

pub struct EngineHandle {
    tx: Sender<Command>,
    pub snapshot: Arc<Mutex<Snapshot>>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl EngineHandle {
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    /// Tears the audio graph down and waits for it. Safe to call repeatedly.
    pub fn shutdown(&self) {
        if let Some(join) = self.join.lock().unwrap().take() {
            let _ = self.tx.send(Command::Shutdown);
            let _ = join.join();
        }
    }
}

pub fn spawn(cfg: Config, defaults: Defaults) -> Arc<EngineHandle> {
    let (tx, rx) = mpsc::channel();
    let snapshot = Arc::new(Mutex::new(Snapshot::default()));
    let mut engine = Engine {
        applied_latency: cfg.latency_ms,
        cfg,
        prev_default: defaults.sink,
        channels: HashMap::new(),
        mixes: HashMap::new(),
        sends: HashMap::new(),
        applied_default: None,
        known_sinks: HashSet::new(),
        device_names: HashSet::new(),
        node_ids: HashMap::new(),
        bluetooth: Vec::new(),
        refreshes: 0,
        app_index: crate::apps::AppIndex::load(),
        input_levels: HashMap::new(),
        guard_until: None,
        listen_switches: 0,
        seen_streams: HashSet::new(),
        live_apps: Vec::new(),
        snapshot: snapshot.clone(),
        dirty_since: None,
        needs_sync: false,
    };

    let join = std::thread::Builder::new()
        .name("linuxlink-engine".into())
        .spawn(move || {
            // Baseline input volumes before LinuxLink adds its devices.
            if let Ok(sources) = pactl::sources() {
                engine.guard_inputs(&sources);
            }
            engine.sync_graph();
            let mut last_refresh: Option<Instant> = None;
            loop {
                let mut cmds = Vec::new();
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(c) => cmds.push(c),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => cmds.push(Command::Shutdown),
                }
                cmds.extend(rx.try_iter());

                // Only the newest config matters; fader drags send one per frame.
                let mut latest = None;
                let mut sources = HashMap::new();
                let mut shutdown = false;
                for cmd in cmds {
                    match cmd {
                        Command::Apply(c, seen) => latest = Some((*c, seen)),
                        Command::SetSource { name, level } => {
                            sources.insert(name, level);
                        }
                        Command::Shutdown => shutdown = true,
                    }
                }
                for (name, level) in sources {
                    // Your own change: the new baseline for the guard.
                    engine.input_levels.insert(name.clone(), (level.volume, level.muted));
                    let r = pactl::set_source_volume(&name, level.volume)
                        .and_then(|()| pactl::set_source_mute(&name, level.muted));
                    if let Err(e) = r {
                        engine.report(e);
                    }
                    last_refresh = None;
                }
                if latest.is_some_and(|(c, seen)| engine.apply(c, seen)) {
                    last_refresh = None;
                }
                if shutdown {
                    engine.shutdown();
                    break;
                }
                if engine.needs_sync && engine.sync_graph() {
                    last_refresh = None;
                }
                if last_refresh.is_none_or(|t| t.elapsed() >= Duration::from_millis(500)) {
                    if let Err(e) = engine.refresh() {
                        engine.report(e);
                    }
                    last_refresh = Some(Instant::now());
                }
                engine.reconcile_levels();
                engine.save_if_due();
            }
        })
        .expect("spawning engine thread");

    Arc::new(EngineHandle { tx, snapshot, join: Mutex::new(Some(join)) })
}

#[cfg(test)]
mod tests {
    use super::node_failed;

    #[test]
    fn spots_failed_nodes_in_pw_cli_output() {
        let failed = "\tid: 566\n*\tstate: \"error\"\n*\terror: \"Received error event\"\n";
        let fine = "\tid: 566\n*\tstate: \"running\"\n*\tmax-input-ports: 0\n";
        assert!(node_failed(failed));
        assert!(!node_failed(fine));
        assert!(!node_failed("\tmedia.name = \"state: error\"\n"));
    }
}
