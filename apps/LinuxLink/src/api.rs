//! Local control API, modelled on Wave Link 3's (what its Stream Deck plugin
//! talks to): JSON-RPC 2.0 over a WebSocket on 127.0.0.1, on a port the
//! system picks. The port is written to `ws-info.json` next to the config
//! file, as `{"port": N}`.
//!
//! Methods (Wave Link's names): `getApplicationInfo`, `getChannels`,
//! `getMixes`, `getInputDevices`, `getOutputDevices`, `setChannel`, `setMix`,
//! `setInputDevice`, `setOutputDevice`, `addToChannel`, `setSubscription`,
//! `getWindowMode`, `setWindowMode`. Notifications: `channelsChanged`,
//! `mixesChanged`, `inputDevicesChanged`, `outputDevicesChanged`,
//! `windowModeChanged`, and `levelMeterChanged` for subscribed meters.
//!
//! Levels are fader positions from 0 to 1. Only local, non-web clients may
//! connect: a request from a web page (an `http(s)://` origin) is refused.

use crate::config::{ChannelKind, Config, Level, WindowMode};
use crate::engine::{channel_capture, channel_sink, mix_capture, Command, EngineHandle, Snapshot};
use crate::meter::MeterBank;
use crate::variant::{APP_ID, APP_NAME, VERSION};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::{Message, WebSocket};

/// Bumped whenever the API changes the settings, so the window adopts them.
static REVISION: AtomicU64 = AtomicU64::new(0);
static PORT: AtomicU16 = AtomicU16::new(0);

pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

/// The port the API listens on, once it runs.
pub fn port() -> Option<u16> {
    Some(PORT.load(Ordering::Relaxed)).filter(|&p| p != 0)
}

/// Bumped by `INTERFACE_REVISION` whenever the protocol changes shape.
const INTERFACE_REVISION: u32 = 1;
const METER_INTERVAL: Duration = Duration::from_millis(50);

fn port_file() -> PathBuf {
    Config::path().with_file_name("ws-info.json")
}

pub fn remove_port_file() {
    let _ = std::fs::remove_file(port_file());
}

struct Server {
    shared: Arc<Mutex<Config>>,
    engine: Arc<EngineHandle>,
    port: u16,
    clients: Mutex<Vec<Sender<String>>>,
    /// The engine's automatic listening-device switches adopted so far.
    adopted_switches: AtomicU64,
}

/// Starts the API; it serves only while `api_enabled` is on.
pub fn spawn(shared: Arc<Mutex<Config>>, engine: Arc<EngineHandle>) {
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(l) => l,
        Err(e) => return eprintln!("linuxlink: control API unavailable: {e}"),
    };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else { return };
    PORT.store(port, Ordering::Relaxed);
    let server =
        Arc::new(Server { shared, engine, port, clients: Mutex::new(Vec::new()), adopted_switches: AtomicU64::new(0) });

    let s = server.clone();
    std::thread::Builder::new()
        .name("linuxlink-api".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if !s.enabled() {
                    continue;
                }
                let s = s.clone();
                std::thread::spawn(move || s.serve(stream));
            }
        })
        .expect("spawning API thread");

    std::thread::Builder::new()
        .name("linuxlink-api-notify".into())
        .spawn(move || server.watch())
        .expect("spawning API notifier");
}

/// Wave Link's origin rule: local tools and Stream Deck, never web pages.
fn origin_allowed(origin: Option<&str>) -> bool {
    match origin.map(str::trim) {
        None | Some("") | Some("null") => true,
        Some(o) => o.starts_with("file://") || o.starts_with("streamdeck://"),
    }
}

/// Guards against DNS rebinding: the Host must be the loopback address.
fn host_allowed(host: Option<&str>) -> bool {
    let Some(host) = host else { return true };
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

impl Server {
    fn enabled(&self) -> bool {
        self.shared.lock().unwrap().api_enabled
    }

    fn snapshot(&self) -> Snapshot {
        self.engine.snapshot.lock().unwrap().clone()
    }

    // tungstenite's handshake callback signature returns its own large error.
    #[allow(clippy::result_large_err)]
    fn serve(&self, stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
            let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
            if origin_allowed(header("origin")) && host_allowed(header("host")) {
                return Ok(resp);
            }
            let mut refusal = ErrorResponse::new(Some("LinuxLink only accepts local clients".into()));
            *refusal.status_mut() = StatusCode::FORBIDDEN;
            Err(refusal)
        };
        let Ok(mut ws) = tungstenite::accept_hdr(stream, check) else { return };
        let _ = ws.get_ref().set_read_timeout(Some(METER_INTERVAL));

        let (tx, rx) = mpsc::channel();
        self.clients.lock().unwrap().push(tx);
        let mut client =
            Client { meters: MeterBank::default(), subscriptions: BTreeSet::new(), last_meter: Instant::now() };
        let _ = self.client_loop(&mut ws, &rx, &mut client);
        let _ = ws.close(None);
    }

    fn client_loop(
        &self,
        ws: &mut WebSocket<TcpStream>,
        notifications: &Receiver<String>,
        client: &mut Client,
    ) -> tungstenite::Result<()> {
        loop {
            match ws.read() {
                Ok(Message::Text(text)) => {
                    if let Some(reply) = self.handle(text.as_str(), client) {
                        ws.send(Message::text(reply))?;
                    }
                }
                Ok(Message::Close(_)) => return Ok(()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => return Err(e),
            }
            loop {
                match notifications.try_recv() {
                    Ok(n) => ws.send(Message::text(n))?,
                    Err(mpsc::TryRecvError::Empty) => break,
                    // The API was turned off.
                    Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                }
            }
            if let Some(n) = self.meter_notification(client) {
                ws.send(Message::text(n))?;
            }
        }
    }

    /// Handles one JSON-RPC message; `None` for notifications (no `id`).
    fn handle(&self, text: &str, client: &mut Client) -> Option<String> {
        let Ok(req) = serde_json::from_str::<Value>(text) else {
            return Some(error_reply(Value::Null, -32700, "Parse error"));
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);
        let result = self.call(method, &params, client);
        let id = id?;
        Some(match result {
            Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }).to_string(),
            Err((code, message)) => error_reply(id, code, &message),
        })
    }

    fn call(&self, method: &str, params: &Value, client: &mut Client) -> Result<Value, (i32, String)> {
        let snap = self.snapshot();
        match method {
            "getApplicationInfo" => Ok(json!({
                "appID": APP_ID,
                "name": APP_NAME,
                "version": VERSION,
                "build": 0,
                "interfaceRevision": INTERFACE_REVISION,
                "operatingSystem": "linux",
            })),
            "getChannels" => Ok(channels_dto(&self.shared.lock().unwrap(), &snap)),
            "getMixes" => Ok(mixes_dto(&self.shared.lock().unwrap())),
            "getInputDevices" => Ok(input_devices_dto(&snap)),
            "getOutputDevices" => Ok(output_devices_dto(&self.shared.lock().unwrap(), &snap)),
            "getWindowMode" => Ok(window_mode_dto(&self.shared.lock().unwrap())),
            "setChannel" => self.edit(&snap, |cfg, engine| set_channel(cfg, &snap, engine, params)),
            "setMix" => self.edit(&snap, |cfg, _| set_mix(cfg, params)),
            "setOutputDevice" => self.edit(&snap, |cfg, _| set_output_device(cfg, params)),
            "setInputDevice" => set_input_device(&self.engine, params).map(|()| json!({})),
            "addToChannel" => self.edit(&snap, |cfg, _| add_to_channel(cfg, params)),
            "setWindowMode" => self.edit(&snap, |cfg, _| set_window_mode(cfg, params)),
            "setSubscription" => client.subscribe(params).map(|()| json!({})),
            // Wave Link's plugin introduces itself; nothing to keep.
            "setPluginInfo" => Ok(json!({})),
            _ => Err((-32601, format!("Method not found: {method}"))),
        }
    }

    /// Applies an edit to the settings the window and engine share.
    fn edit(
        &self,
        snap: &Snapshot,
        change: impl FnOnce(&mut Config, &EngineHandle) -> Result<(), (i32, String)>,
    ) -> Result<Value, (i32, String)> {
        let mut cfg = self.shared.lock().unwrap();
        // The engine switched the listening device by itself (a headset
        // connecting while the window is closed, say): start from that.
        if snap.listen_switches > self.adopted_switches.swap(snap.listen_switches, Ordering::Relaxed) {
            cfg.listen_output = snap.listen_output.clone();
        }
        let before = cfg.clone();
        change(&mut cfg, &self.engine)?;
        if *cfg != before {
            self.engine.send(Command::Apply(Box::new(cfg.clone()), snap.listen_switches));
            REVISION.fetch_add(1, Ordering::Relaxed);
        }
        Ok(json!({}))
    }

    /// Sends `*Changed` notifications whenever what clients see changes,
    /// and keeps `ws-info.json` in line with the on/off setting.
    fn watch(&self) {
        let mut last: Vec<(&str, Value)> = Vec::new();
        let mut was_enabled = None;
        loop {
            let enabled = self.enabled();
            if was_enabled != Some(enabled) {
                was_enabled = Some(enabled);
                if enabled {
                    let file = port_file();
                    if let Some(dir) = file.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(file, json!({ "port": self.port }).to_string());
                } else {
                    remove_port_file();
                    // Dropping their senders disconnects every client.
                    self.clients.lock().unwrap().clear();
                }
            }
            if enabled && !self.clients.lock().unwrap().is_empty() {
                let snap = self.snapshot();
                let cfg = self.shared.lock().unwrap().clone();
                let now = vec![
                    ("channelsChanged", channels_dto(&cfg, &snap)),
                    ("mixesChanged", mixes_dto(&cfg)),
                    ("inputDevicesChanged", input_devices_dto(&snap)),
                    ("outputDevicesChanged", output_devices_dto(&cfg, &snap)),
                    ("windowModeChanged", window_mode_dto(&cfg)),
                ];
                if !last.is_empty() {
                    for ((method, params), (_, old)) in now.iter().zip(&last) {
                        if params != old {
                            self.broadcast(method, params);
                        }
                    }
                }
                last = now;
            } else {
                last.clear();
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    fn broadcast(&self, method: &str, params: &Value) {
        let text = json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string();
        self.clients.lock().unwrap().retain(|c| c.send(text.clone()).is_ok());
    }

    fn meter_notification(&self, client: &mut Client) -> Option<String> {
        if client.subscriptions.is_empty() || client.last_meter.elapsed() < METER_INTERVAL {
            return None;
        }
        let dt = client.last_meter.elapsed().as_secs_f32();
        client.last_meter = Instant::now();
        let cfg = self.shared.lock().unwrap().clone();
        let sources: Vec<(String, String, String)> = client
            .subscriptions
            .iter()
            .filter_map(|(kind, id)| Some((kind.clone(), id.clone(), meter_source(&cfg, kind, id)?)))
            .collect();
        client.meters.update(sources.iter().map(|(_, _, s)| s.as_str()), dt);
        let mut params = json!({ "inputDevices": [], "outputDevices": [], "mixes": [], "channels": [] });
        for (kind, id, source) in &sources {
            let pct = crate::levels::meter_position(client.meters.level(source)) * 100.0;
            let list = match kind.as_str() {
                "input" => "inputDevices",
                "output" => "outputDevices",
                "mix" => "mixes",
                _ => "channels",
            };
            params[list].as_array_mut().unwrap().push(json!({
                "id": id, "levelLeftPercentage": pct, "levelRightPercentage": pct,
            }));
        }
        Some(json!({ "jsonrpc": "2.0", "method": "levelMeterChanged", "params": params }).to_string())
    }
}

struct Client {
    meters: MeterBank,
    /// (type, id) of the meters this client asked for.
    subscriptions: BTreeSet<(String, String)>,
    last_meter: Instant,
}

impl Client {
    /// `{"levelMeterChanged": {"type": "channel", "id": "music", "isEnabled": true}}`
    fn subscribe(&mut self, params: &Value) -> Result<(), (i32, String)> {
        let Some(sub) = params.get("levelMeterChanged") else { return Ok(()) };
        let kind = sub.get("type").and_then(Value::as_str).ok_or(invalid("levelMeterChanged.type"))?;
        if !matches!(kind, "input" | "output" | "mix" | "channel") {
            return Err(invalid("levelMeterChanged.type"));
        }
        let id = sub.get("id").and_then(Value::as_str).ok_or(invalid("levelMeterChanged.id"))?;
        let key = (kind.to_string(), id.to_string());
        if sub.get("isEnabled").and_then(Value::as_bool).unwrap_or(true) {
            self.subscriptions.insert(key);
        } else {
            self.subscriptions.remove(&key);
        }
        Ok(())
    }
}

fn meter_source(cfg: &Config, kind: &str, id: &str) -> Option<String> {
    match kind {
        "channel" => cfg.channel(id).map(channel_capture),
        "mix" => cfg.mix(id).map(|m| mix_capture(&m.id)),
        "input" => Some(id.to_string()),
        "output" => Some(format!("{id}.monitor")),
        _ => None,
    }
}

fn error_reply(id: Value, code: i32, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

fn invalid(what: &str) -> (i32, String) {
    (-32602, format!("Invalid params: {what}"))
}

fn position(level: f64) -> u32 {
    (level.clamp(0.0, 1.0) * 100.0).round() as u32
}

fn level_dto(level: &Level) -> Value {
    json!({ "level": level.volume as f64 / 100.0, "isMuted": level.muted })
}

/// A device's own volume as a level (inputs' masters are device volumes).
fn device_level(snap: &Snapshot, name: &str) -> Option<Level> {
    let d = snap.inputs.iter().find(|d| d.name == name)?;
    Some(Level::new(d.volume.unwrap_or(100).min(100), d.muted))
}

fn channels_dto(cfg: &Config, snap: &Snapshot) -> Value {
    let channels: Vec<Value> = cfg
        .channels
        .iter()
        .map(|ch| {
            let (kind, master) = match &ch.kind {
                ChannelKind::Virtual => ("software", ch.level.clone()),
                ChannelKind::Input { source } => ("hardware", device_level(snap, source).unwrap_or_else(Level::full)),
            };
            let mixes: Vec<Value> = cfg
                .mixes
                .iter()
                .filter_map(|m| {
                    let send = ch.sends.get(&m.id)?;
                    let mut v = level_dto(send);
                    v["id"] = json!(m.id);
                    Some(v)
                })
                .collect();
            let sink = channel_sink(&ch.id);
            let mut apps: Vec<Value> = Vec::new();
            let assigned = cfg.app_rules.iter().filter(|(_, c)| **c == ch.id).map(|(k, _)| k.clone());
            let live = snap.apps.iter().filter(|a| a.sink == sink).map(|a| a.key.clone());
            for key in assigned.chain(live) {
                if !apps.iter().any(|a| a["id"] == key) {
                    let name = cfg.known_apps.get(&key).map_or(key.clone(), |a| a.name.clone());
                    apps.push(json!({ "id": key, "name": name }));
                }
            }
            let mut v = level_dto(&master);
            v["id"] = json!(ch.id);
            v["name"] = json!(ch.name);
            v["type"] = json!(kind);
            v["mixes"] = json!(mixes);
            v["apps"] = json!(apps);
            v
        })
        .collect();
    json!({ "channels": channels })
}

fn mixes_dto(cfg: &Config) -> Value {
    let mixes: Vec<Value> = cfg
        .mixes
        .iter()
        .map(|m| {
            let mut v = level_dto(&m.level);
            v["id"] = json!(m.id);
            v["name"] = json!(m.name);
            v
        })
        .collect();
    json!({ "mixes": mixes })
}

fn input_devices_dto(snap: &Snapshot) -> Value {
    let devices: Vec<Value> = snap
        .inputs
        .iter()
        .map(|d| {
            json!({
                "id": d.name,
                "name": d.description,
                "deviceType": "thirdParty",
                "inputs": [{
                    "id": d.name,
                    "name": d.description,
                    "isMuted": d.muted,
                    "gain": { "value": d.volume.unwrap_or(100), "min": 0, "max": 100, "unit": "percent" },
                }],
            })
        })
        .collect();
    json!({ "inputDevices": devices })
}

fn output_devices_dto(cfg: &Config, snap: &Snapshot) -> Value {
    let devices: Vec<Value> = snap
        .outputs
        .iter()
        .map(|d| {
            // Which mix plays there: a mix's extra output, or the first mix
            // you listen to on your listening device.
            let mix = cfg.mixes.iter().find(|m| m.outputs.contains(&d.name)).map(|m| m.id.clone()).or_else(|| {
                (cfg.listen_output.as_ref() == Some(&d.name))
                    .then(|| cfg.mixes.iter().find(|m| cfg.listening.contains(&m.id)).map(|m| m.id.clone()))
                    .flatten()
            });
            json!({
                "id": d.name,
                "name": d.description,
                "deviceType": "thirdParty",
                "outputs": [{
                    "id": d.name,
                    "name": d.description,
                    "mixId": mix,
                    "level": d.volume.unwrap_or(100) as f64 / 100.0,
                    "isMuted": d.muted,
                    "supportsMute": false,
                }],
            })
        })
        .collect();
    let main = cfg.listen_output.clone();
    json!({ "mainOutput": { "outputDeviceId": main, "outputId": main }, "outputDevices": devices })
}

fn window_mode_dto(cfg: &Config) -> Value {
    let mode = match cfg.window_mode {
        WindowMode::Full => "fullSized",
        WindowMode::Compact => "compacted",
    };
    json!({ "windowMode": mode, "settings": { "mixId": cfg.compact_mix } })
}

fn apply_level(level: &mut Level, params: &Value) {
    if let Some(v) = params.get("level").and_then(Value::as_f64) {
        level.volume = position(v);
    }
    if let Some(m) = params.get("isMuted").and_then(Value::as_bool) {
        level.muted = m;
    }
}

/// `{"id", "level"?, "isMuted"?, "mixes"?: [{"id", "level"?, "isMuted"?}]}`
fn set_channel(cfg: &mut Config, snap: &Snapshot, engine: &EngineHandle, params: &Value) -> Result<(), (i32, String)> {
    let id = params.get("id").and_then(Value::as_str).ok_or(invalid("id"))?;
    let mix_ids: Vec<String> = cfg.mixes.iter().map(|m| m.id.clone()).collect();
    let ch = cfg.channels.iter_mut().find(|c| c.id == id).ok_or(invalid("unknown channel id"))?;
    match &ch.kind {
        ChannelKind::Virtual => apply_level(&mut ch.level, params),
        ChannelKind::Input { source } => {
            // An input channel's master is the device's own volume.
            if params.get("level").is_some() || params.get("isMuted").is_some() {
                let mut level = device_level(snap, source).unwrap_or_else(Level::full);
                apply_level(&mut level, params);
                engine.send(Command::SetSource { name: source.clone(), level });
            }
        }
    }
    for m in params.get("mixes").and_then(Value::as_array).into_iter().flatten() {
        let mix = m.get("id").and_then(Value::as_str).ok_or(invalid("mixes[].id"))?;
        if !mix_ids.iter().any(|x| x == mix) {
            return Err(invalid("unknown mix id"));
        }
        // Setting a level on a mix the channel isn't in adds it, like
        // clicking + in the matrix.
        apply_level(ch.sends.entry(mix.to_string()).or_insert_with(Level::full), m);
    }
    Ok(())
}

/// `{"id", "level"?, "isMuted"?}`
fn set_mix(cfg: &mut Config, params: &Value) -> Result<(), (i32, String)> {
    let id = params.get("id").and_then(Value::as_str).ok_or(invalid("id"))?;
    let mix = cfg.mixes.iter_mut().find(|m| m.id == id).ok_or(invalid("unknown mix id"))?;
    apply_level(&mut mix.level, params);
    Ok(())
}

/// `{"mainOutput": {"outputDeviceId"}}` picks your listening device;
/// `{"outputDevice": {"id", "outputs": [{"mixId"}]}}` plays a mix there.
fn set_output_device(cfg: &mut Config, params: &Value) -> Result<(), (i32, String)> {
    if let Some(main) = params.pointer("/mainOutput/outputDeviceId").and_then(Value::as_str) {
        cfg.listen_output = Some(main.to_string());
        for mix in &mut cfg.mixes {
            mix.outputs.retain(|o| o != main);
        }
    }
    if let Some(dev) = params.get("outputDevice") {
        let id = dev.get("id").and_then(Value::as_str).ok_or(invalid("outputDevice.id"))?;
        let mix = dev.pointer("/outputs/0/mixId").ok_or(invalid("outputDevice.outputs[0].mixId"))?;
        if cfg.listen_output.as_deref() == Some(id) {
            return Err(invalid("that is your listening device; use mainOutput"));
        }
        if let Some(m) = mix.as_str().filter(|m| cfg.mix(m).is_none()) {
            return Err(invalid(&format!("unknown mix id {m}")));
        }
        // An output plays one mix at most.
        for m in &mut cfg.mixes {
            m.outputs.retain(|o| o != id);
            if mix.as_str() == Some(m.id.as_str()) {
                m.outputs.push(id.to_string());
            }
        }
    }
    Ok(())
}

/// `{"id", "isMuted"?, "gain"?: {"value"}}` (percent of the device volume).
fn set_input_device(engine: &EngineHandle, params: &Value) -> Result<(), (i32, String)> {
    let id = params.get("id").and_then(Value::as_str).ok_or(invalid("id"))?;
    let snap = engine.snapshot.lock().unwrap().clone();
    let mut level = device_level(&snap, id).ok_or(invalid("unknown input device"))?;
    if let Some(v) = params.pointer("/gain/value").and_then(Value::as_f64) {
        level.volume = v.clamp(0.0, 100.0).round() as u32;
    }
    if let Some(m) = params.get("isMuted").and_then(Value::as_bool) {
        level.muted = m;
    }
    engine.send(Command::SetSource { name: id.to_string(), level });
    Ok(())
}

/// `{"appId", "channelId"}`: always play that app in that channel.
fn add_to_channel(cfg: &mut Config, params: &Value) -> Result<(), (i32, String)> {
    let app = params.get("appId").and_then(Value::as_str).ok_or(invalid("appId"))?;
    let channel = params.get("channelId").and_then(Value::as_str).ok_or(invalid("channelId"))?;
    if cfg.channel(channel).is_none_or(|c| c.kind != ChannelKind::Virtual) {
        return Err(invalid("channelId must be an app channel"));
    }
    cfg.app_rules.insert(app.to_string(), channel.to_string());
    Ok(())
}

/// `{"windowMode": "fullSized" | "compacted" | "horizontalCompact", "settings"?: {"mixId"}}`
fn set_window_mode(cfg: &mut Config, params: &Value) -> Result<(), (i32, String)> {
    cfg.window_mode = match params.get("windowMode").and_then(Value::as_str) {
        Some("fullSized") => WindowMode::Full,
        // LinuxLink has one compact view.
        Some("compacted" | "horizontalCompact") => WindowMode::Compact,
        _ => return Err(invalid("windowMode")),
    };
    if let Some(mix) = params.pointer("/settings/mixId").and_then(Value::as_str) {
        if cfg.mix(mix).is_none() {
            return Err(invalid("unknown mix id"));
        }
        cfg.compact_mix = Some(mix.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pactl::Device;

    fn dev(name: &str, volume: u32) -> Device {
        Device {
            index: 0,
            name: name.into(),
            description: name.into(),
            volume: Some(volume),
            volume_raw: None,
            muted: false,
            node_id: None,
        }
    }

    #[test]
    fn refuses_web_pages() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some("")));
        assert!(origin_allowed(Some("streamdeck://")));
        assert!(origin_allowed(Some("file://")));
        assert!(!origin_allowed(Some("https://example.com")));
        assert!(!origin_allowed(Some("http://127.0.0.1:8080")));
        assert!(host_allowed(Some("127.0.0.1:41234")));
        assert!(host_allowed(Some("localhost:41234")));
        assert!(!host_allowed(Some("evil.example:41234")));
    }

    #[test]
    fn set_channel_moves_faders_and_adds_routes() {
        let mut cfg = Config::default();
        cfg.channels[1].sends.remove("stream");
        let (engine, _) = EngineHandle::detached();
        let params = json!({
            "id": "system", "level": 0.5, "isMuted": true,
            "mixes": [{ "id": "stream", "level": 0.25 }, { "id": "my_mix", "isMuted": true }],
        });
        set_channel(&mut cfg, &Snapshot::default(), &engine, &params).unwrap();
        let ch = cfg.channel("system").unwrap();
        assert_eq!(ch.level, Level::new(50, true));
        assert_eq!(ch.sends["stream"], Level::new(25, false));
        assert_eq!(ch.sends["my_mix"], Level::new(100, true));
        let bad = json!({ "id": "system", "mixes": [{ "id": "nope" }] });
        assert!(set_channel(&mut cfg, &Snapshot::default(), &engine, &bad).is_err());
    }

    #[test]
    fn input_channels_report_and_set_the_device_volume() {
        let mut cfg = Config::default();
        cfg.channels[0].kind = ChannelKind::Input { source: "mic".into() };
        let snap = Snapshot { inputs: vec![dev("mic", 40)], ..Default::default() };
        let mic = &channels_dto(&cfg, &snap)["channels"][0];
        assert_eq!(mic["type"], "hardware");
        assert_eq!(mic["level"], 0.4);

        let (engine, rx) = EngineHandle::detached();
        set_channel(&mut cfg, &snap, &engine, &json!({ "id": "mic", "level": 0.8 })).unwrap();
        match rx.try_recv() {
            Ok(Command::SetSource { name, level }) => {
                assert_eq!(name, "mic");
                assert_eq!(level, Level::new(80, false));
            }
            _ => panic!("expected a device volume change"),
        }
    }

    #[test]
    fn outputs_play_one_mix_and_the_listening_device_is_separate() {
        let mut cfg = Config { listen_output: Some("phones".into()), ..Default::default() };
        set_output_device(&mut cfg, &json!({ "outputDevice": { "id": "card", "outputs": [{ "mixId": "stream" }] } }))
            .unwrap();
        assert_eq!(cfg.mix("stream").unwrap().outputs, ["card"]);
        set_output_device(&mut cfg, &json!({ "outputDevice": { "id": "card", "outputs": [{ "mixId": "my_mix" }] } }))
            .unwrap();
        assert!(cfg.mix("stream").unwrap().outputs.is_empty());
        assert_eq!(cfg.mix("my_mix").unwrap().outputs, ["card"]);
        set_output_device(&mut cfg, &json!({ "mainOutput": { "outputDeviceId": "card" } })).unwrap();
        assert_eq!(cfg.listen_output.as_deref(), Some("card"));
        assert!(cfg.mix("my_mix").unwrap().outputs.is_empty());
    }

    /// A server for one connection on a test port (no port file).
    fn serve_once() -> (u16, Receiver<Command>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (engine, rx) = EngineHandle::detached();
        let server = Server {
            shared: Arc::new(Mutex::new(Config::default())),
            engine: Arc::new(engine),
            port,
            clients: Mutex::new(Vec::new()),
            adopted_switches: AtomicU64::new(0),
        };
        std::thread::spawn(move || server.serve(listener.accept().unwrap().0));
        (port, rx)
    }

    fn connect(port: u16, origin: &str) -> tungstenite::Result<WebSocket<TcpStream>> {
        use tungstenite::client::IntoClientRequest;
        let mut req = format!("ws://127.0.0.1:{port}").into_client_request().unwrap();
        if !origin.is_empty() {
            req.headers_mut().insert("origin", origin.parse().unwrap());
        }
        tungstenite::client(req, TcpStream::connect(("127.0.0.1", port)).unwrap()).map(|(ws, _)| ws).map_err(
            |e| match e {
                tungstenite::HandshakeError::Failure(e) => e,
                tungstenite::HandshakeError::Interrupted(_) => unreachable!("blocking stream"),
            },
        )
    }

    fn rpc(ws: &mut WebSocket<TcpStream>, request: Value) -> Value {
        ws.send(Message::text(request.to_string())).unwrap();
        loop {
            if let Message::Text(t) = ws.read().unwrap() {
                return serde_json::from_str(t.as_str()).unwrap();
            }
        }
    }

    #[test]
    fn speaks_json_rpc_over_websocket() {
        let (port, commands) = serve_once();
        let mut ws = connect(port, "streamdeck://").unwrap();
        let reply = rpc(&mut ws, json!({ "jsonrpc": "2.0", "id": 1, "method": "getMixes" }));
        assert_eq!(reply["id"], 1);
        assert_eq!(
            reply["result"]["mixes"][0],
            json!({ "id": "my_mix", "name": "My Mix", "level": 1.0, "isMuted": false })
        );

        let reply = rpc(
            &mut ws,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "setMix", "params": { "id": "stream", "isMuted": true } }),
        );
        assert_eq!(reply["result"], json!({}));
        match commands.try_recv() {
            Ok(Command::Apply(cfg, _)) => assert!(cfg.mix("stream").unwrap().level.muted),
            _ => panic!("expected the change to reach the engine"),
        }

        let reply = rpc(&mut ws, json!({ "jsonrpc": "2.0", "id": 3, "method": "explode" }));
        assert_eq!(reply["error"]["code"], -32601);
        let reply = rpc(&mut ws, json!({ "jsonrpc": "2.0", "id": 4, "method": "setMix", "params": { "id": "nope" } }));
        assert_eq!(reply["error"]["code"], -32602);
    }

    #[test]
    fn web_pages_cannot_connect() {
        let (port, _) = serve_once();
        match connect(port, "https://example.com") {
            Err(tungstenite::Error::Http(resp)) => assert_eq!(resp.status(), StatusCode::FORBIDDEN),
            other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn window_mode_round_trips() {
        let mut cfg = Config::default();
        set_window_mode(&mut cfg, &json!({ "windowMode": "compacted", "settings": { "mixId": "stream" } })).unwrap();
        assert_eq!(window_mode_dto(&cfg), json!({ "windowMode": "compacted", "settings": { "mixId": "stream" } }));
        assert!(set_window_mode(&mut cfg, &json!({ "windowMode": "tiny" })).is_err());
    }
}
