//! Turning devices off for the whole computer, not just in LinuxLink's
//! lists: a WirePlumber rule marks their nodes disabled, so PipeWire never
//! creates them and no app or sound settings page offers them. WirePlumber
//! reads its rules when it starts, so a change restarts it (sound drops out
//! for a moment).
//!
//! Only sound-card (ALSA) devices can be turned off this way; WirePlumber
//! has no rule for a single Bluetooth output.
//!
//! A restart drops Bluetooth headsets' stereo (A2DP) connection, and they
//! come back on the mono headset (HFP) profile: quiet, 16 kHz, and on some
//! headsets with weaker noise cancelling. So the restart puts each headset
//! back on the profile it had (see `restore_bluetooth`).

use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The A2DP sink service, to reconnect a headset's stereo connection.
const A2DP_SINK_UUID: &str = "0000110b-0000-1000-8000-00805f9b34fb";
/// How long to wait for headsets to return after WirePlumber restarts.
const BLUETOOTH_RETURN: Duration = Duration::from_secs(10);

/// Whether `name` is a device a WirePlumber rule can turn off.
pub fn can_disable(name: &str) -> bool {
    name.starts_with("alsa_output.") || name.starts_with("alsa_input.")
}

/// WirePlumber 0.5's drop-in folder, one file per LinuxLink build.
fn rule_file() -> Option<PathBuf> {
    let name = format!("90-{}-disabled-devices.conf", crate::variant::CONFIG_DIR);
    Some(crate::sys::host_config_dir()?.join("wireplumber/wireplumber.conf.d").join(name))
}

fn render(names: &BTreeSet<String>) -> String {
    let matches: String = names
        .iter()
        .map(|n| format!("      {{ node.name = \"{}\" }}\n", n.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    format!(
        "# Written by {app}: devices turned off in its Settings. Turn them back on\n\
         # there, or delete this file and run `systemctl --user restart wireplumber`.\n\
         monitor.alsa.rules = [\n  {{\n    matches = [\n{matches}    ]\n    actions = {{\n      \
         update-props = {{\n        node.disabled = true\n      }}\n    }}\n  }}\n]\n",
        app = crate::variant::APP_NAME,
    )
}

/// Makes the rule turn off exactly `names` (none removes it), restarting
/// WirePlumber if that changed anything. Returns whether it did.
pub fn apply(names: &BTreeSet<String>) -> Result<bool> {
    let path = rule_file().context("no config folder to put the WirePlumber rule in")?;
    let current = std::fs::read_to_string(&path).ok();
    let wanted = (!names.is_empty()).then(|| render(names));
    if current == wanted {
        return Ok(false);
    }
    match &wanted {
        Some(text) => {
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        }
        None => std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?,
    }
    eprintln!("linuxlink: devices turned off on this computer: {names:?}; restarting WirePlumber");
    let headsets = bluetooth_profiles();
    let status = crate::sys::host_command("systemctl")
        .args(["--user", "restart", "wireplumber"])
        .status()
        .context("restarting WirePlumber")?;
    if !status.success() {
        bail!("couldn't restart WirePlumber; device changes apply after you log in again");
    }
    restore_bluetooth(&headsets);
    Ok(true)
}

/// Connected Bluetooth headsets and the profile each is on.
fn bluetooth_profiles() -> Vec<(String, String)> {
    crate::pactl::cards()
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.bluez_path.is_some() && c.active_profile != "off")
        .map(|c| (c.name, c.active_profile))
        .collect()
}

/// Puts headsets back on the profiles they had before WirePlumber restarted,
/// reconnecting their stereo (A2DP) connection if it dropped.
fn restore_bluetooth(saved: &[(String, String)]) {
    let deadline = Instant::now() + BLUETOOTH_RETURN;
    let mut pending: Vec<&(String, String)> = saved.iter().collect();
    let mut reconnected = BTreeSet::new();
    while !pending.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        let cards = crate::pactl::cards().unwrap_or_default();
        pending.retain(|(name, profile)| {
            let Some(card) = cards.iter().find(|c| &c.name == name) else { return true };
            if &card.active_profile == profile {
                return false;
            }
            if card.profiles.contains(profile) {
                if let Err(e) = crate::pactl::set_card_profile(name, profile) {
                    eprintln!("linuxlink: switching {name} back to {profile}: {e:#}");
                }
                return true;
            }
            // The stereo connection is gone: ask the headset for it again, once.
            if profile.starts_with("a2dp") && reconnected.insert(name.clone()) {
                if let Some(path) = &card.bluez_path {
                    let _ = crate::sys::host_command("busctl")
                        .args(["call", "org.bluez", path, "org.bluez.Device1", "ConnectProfile", "s", A2DP_SINK_UUID])
                        .status();
                }
            }
            true
        });
    }
    for (name, profile) in pending {
        eprintln!("linuxlink: {name} didn't come back on {profile}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_wireplumber_rule_per_device() {
        let names = BTreeSet::from(["alsa_output.hdmi".to_string(), "alsa_input.\"odd\"".to_string()]);
        let text = render(&names);
        assert!(text.contains("monitor.alsa.rules = ["));
        assert!(text.contains("{ node.name = \"alsa_output.hdmi\" }"));
        assert!(text.contains("{ node.name = \"alsa_input.\\\"odd\\\"\" }"));
        assert!(text.contains("node.disabled = true"));
    }

    #[test]
    fn only_sound_card_devices_can_be_turned_off() {
        assert!(can_disable("alsa_output.usb-Generic_USB_Audio-00.HiFi__SPDIF__sink"));
        assert!(can_disable("alsa_input.pci-0000_00_1f.3.analog-stereo"));
        assert!(!can_disable("bluez_output.EC_E9_D2_25_E2_CA.1"));
        assert!(!can_disable("linuxlink_ch_system"));
    }
}
