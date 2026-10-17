//! Thin wrapper around `pactl`, which talks to PipeWire through pipewire-pulse.
//!
//! Shelling out keeps the build free of native audio dependencies and every
//! call is a few milliseconds, which is plenty for a mixer UI.

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub volume: Option<u32>,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub struct SinkInput {
    pub index: u32,
    pub sink: u32,
    pub owner_module: Option<u32>,
    pub volume: Option<u32>,
    pub muted: bool,
    pub props: Map<String, Value>,
}

impl SinkInput {
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.get(key).and_then(Value::as_str)
    }
}

fn run(args: &[&str]) -> Result<String> {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .context("failed to run pactl (is pipewire-pulse / libpulse installed?)")?;
    if !out.status.success() {
        bail!(
            "pactl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn list_json(kind: &str) -> Result<Vec<Value>> {
    let s = run(&["-f", "json", "list", kind])?;
    serde_json::from_str(&s).with_context(|| format!("parsing `pactl list {kind}` output"))
}

/// pactl's JSON is inconsistent: some ids (e.g. `owner_module`) are strings.
fn as_u32(v: Option<&Value>) -> Option<u32> {
    v.and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|&n| n < u32::MAX as u64)
        .map(|n| n as u32)
}

/// Volume of the first channel, in percent.
fn volume_percent(v: &Value) -> Option<u32> {
    let raw = v.get("volume")?.as_object()?.values().next()?.get("value")?.as_u64()?;
    Some(((raw * 100 + 32768) / 65536) as u32)
}

fn muted(v: &Value) -> bool {
    v.get("mute").and_then(Value::as_bool).unwrap_or(false)
}

fn named(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty() && *s != "(null)")
}

/// Readable label for devices that report no description. Some BlueZ nodes
/// lose their name on the way through pipewire-pulse (e.g. names with a
/// curly apostrophe), so ask BlueZ itself and fall back to the address.
fn fallback_description(name: &str) -> String {
    match name.strip_prefix("bluez_output.").or_else(|| name.strip_prefix("bluez_input.")) {
        Some(rest) => {
            let addr = rest.split('.').next().unwrap_or(rest).replace('_', ":");
            bluetooth_alias(&addr).unwrap_or_else(|| format!("Bluetooth {addr}"))
        }
        None => name.to_string(),
    }
}

/// The name BlueZ shows for a device, cached since devices are listed often.
fn bluetooth_alias(addr: &str) -> Option<String> {
    static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(hit) = cache.get(addr) {
        return hit.clone();
    }
    let alias = Command::new("bluetoothctl")
        .args(["info", addr])
        .output()
        .ok()
        .and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let field = |key: &str| {
                text.lines().find_map(|l| l.trim().strip_prefix(key)).map(|v| v.trim().to_string())
            };
            field("Alias:").or_else(|| field("Name:")).filter(|v| !v.is_empty())
        });
    cache.insert(addr.to_string(), alias.clone());
    alias
}

fn devices(kind: &str) -> Result<Vec<Device>> {
    Ok(list_json(kind)?
        .iter()
        .filter_map(|d| {
            let name = d.get("name")?.as_str()?.to_string();
            let description = named(d.get("description"))
                .or_else(|| named(d.pointer("/properties/device.description")))
                .or_else(|| named(d.pointer("/properties/device.alias")))
                .map(str::to_string)
                .unwrap_or_else(|| fallback_description(&name));
            Some(Device {
                index: as_u32(d.get("index"))?,
                name,
                description,
                volume: volume_percent(d),
                muted: muted(d),
            })
        })
        .collect())
}

pub fn sinks() -> Result<Vec<Device>> {
    devices("sinks")
}

pub fn sources() -> Result<Vec<Device>> {
    devices("sources")
}

pub fn sink_inputs() -> Result<Vec<SinkInput>> {
    Ok(list_json("sink-inputs")?
        .into_iter()
        .filter_map(|v| {
            Some(SinkInput {
                index: as_u32(v.get("index"))?,
                sink: as_u32(v.get("sink"))?,
                owner_module: as_u32(v.get("owner_module")),
                volume: volume_percent(&v),
                muted: muted(&v),
                props: v.get("properties")?.as_object()?.clone(),
            })
        })
        .collect())
}

/// Formats a pulse proplist argument: `'k="v" k2="v2"'`.
pub fn proplist(pairs: &[(&str, &str)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", v.replace(['"', '\''], "")))
        .collect();
    format!("'{}'", body.join(" "))
}

pub fn load_module(name: &str, args: &[String]) -> Result<u32> {
    let mut argv = vec!["load-module", name];
    argv.extend(args.iter().map(String::as_str));
    let out = run(&argv)?;
    out.trim()
        .parse()
        .with_context(|| format!("unexpected load-module output: {out:?}"))
}

pub fn unload_module(id: u32) -> Result<()> {
    run(&["unload-module", &id.to_string()]).map(drop)
}

/// `(module id, arguments)` for every loaded module.
pub fn modules() -> Result<Vec<(u32, String)>> {
    Ok(run(&["list", "short", "modules"])?
        .lines()
        .filter_map(|l| {
            let mut cols = l.splitn(3, '\t');
            let id = cols.next()?.parse().ok()?;
            let _name = cols.next()?;
            Some((id, cols.next().unwrap_or("").to_string()))
        })
        .collect())
}

fn bool_arg(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

pub fn set_sink_input_volume(index: u32, percent: u32) -> Result<()> {
    run(&["set-sink-input-volume", &index.to_string(), &format!("{percent}%")]).map(drop)
}

pub fn set_sink_input_mute(index: u32, muted: bool) -> Result<()> {
    run(&["set-sink-input-mute", &index.to_string(), bool_arg(muted)]).map(drop)
}

pub fn set_sink_volume(sink: &str, percent: u32) -> Result<()> {
    run(&["set-sink-volume", sink, &format!("{percent}%")]).map(drop)
}

pub fn set_sink_mute(sink: &str, muted: bool) -> Result<()> {
    run(&["set-sink-mute", sink, bool_arg(muted)]).map(drop)
}

pub fn move_sink_input(index: u32, sink: &str) -> Result<()> {
    run(&["move-sink-input", &index.to_string(), sink]).map(drop)
}

pub fn default_sink() -> Result<String> {
    Ok(run(&["get-default-sink"])?.trim().to_string())
}

pub fn default_source() -> Result<String> {
    Ok(run(&["get-default-source"])?.trim().to_string())
}

pub fn set_default_sink(sink: &str) -> Result<()> {
    run(&["set-default-sink", sink]).map(drop)
}

pub fn set_source_volume(source: &str, percent: u32) -> Result<()> {
    run(&["set-source-volume", source, &format!("{percent}%")]).map(drop)
}

pub fn set_source_mute(source: &str, muted: bool) -> Result<()> {
    run(&["set-source-mute", source, bool_arg(muted)]).map(drop)
}
