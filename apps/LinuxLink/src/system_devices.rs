//! Turning devices off for the whole computer, not just in LinuxLink's
//! lists: a WirePlumber rule marks their nodes disabled, so PipeWire never
//! creates them and no app or sound settings page offers them. WirePlumber
//! reads its rules when it starts, so a change restarts it (sound drops out
//! for a moment).
//!
//! Only sound-card (ALSA) devices can be turned off this way; WirePlumber
//! has no rule for a single Bluetooth output.

use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;

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
    let status = crate::sys::host_command("systemctl")
        .args(["--user", "restart", "wireplumber"])
        .status()
        .context("restarting WirePlumber")?;
    if !status.success() {
        bail!("couldn't restart WirePlumber; device changes apply after you log in again");
    }
    Ok(true)
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
