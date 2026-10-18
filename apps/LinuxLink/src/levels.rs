//! Wave Link 3's volume maths, and how it maps onto PipeWire.
//!
//! A fader position `p` (0–100) is linear in decibels over a 60 dB range,
//! like Wave Link's mixer: `dB = 0.6·p − 60`, so 100 is 0 dB, 50 is −30 dB
//! and 0 is silence. Gains in a chain (channel master, send, boost, ducking)
//! add up in dB.
//!
//! PipeWire's pulse volumes are cubic: a raw volume `r` (65536 = 100%) is an
//! amplitude of `(r / 65536)³`, i.e. `60·log10(r / 65536)` dB. LinuxLink
//! therefore sets raw volumes, which reproduce any dB value exactly.

use serde::{Deserialize, Serialize};

/// Raw pulse volume of 0 dB.
pub const RAW_NORM: u32 = 65536;
/// The fader range, in dB below full scale.
pub const RANGE_DB: f32 = 60.0;
/// Most a channel can be boosted, in dB.
pub const MAX_BOOST_DB: f32 = 12.0;

/// Gain in dB of a fader position, or `None` for silence (position 0).
pub fn fader_db(position: u32) -> Option<f32> {
    (position > 0).then(|| position.min(100) as f32 * RANGE_DB / 100.0 - RANGE_DB)
}

/// Raw pulse volume for a gain in dB (`None` is silence).
pub fn raw_volume(db: Option<f32>) -> u32 {
    db.map_or(0, |db| (RAW_NORM as f32 * 10f32.powf(db / 60.0)).round() as u32)
}

/// Linear amplitude factor for a gain in dB (`None` is silence).
pub fn amplitude(db: Option<f32>) -> f32 {
    db.map_or(0.0, |db| 10f32.powf(db / 20.0))
}

/// Sums gains in dB; silence anywhere in the chain silences the result.
pub fn sum_db(parts: impl IntoIterator<Item = Option<f32>>) -> Option<f32> {
    parts.into_iter().sum()
}

/// "−12.0 dB", "0.0 dB", or "−∞ dB".
pub fn format_db(db: Option<f32>) -> String {
    match db {
        None => "−∞ dB".to_string(),
        Some(db) if db.abs() < 0.05 => "0.0 dB".to_string(),
        Some(db) if db < 0.0 => format!("−{:.1} dB", -db),
        Some(db) => format!("+{db:.1} dB"),
    }
}

/// 0..1 meter position for a linear peak, on Wave Link's −60…0 dBFS scale.
pub fn meter_position(peak: f32) -> f32 {
    if peak <= 1e-6 {
        return 0.0;
    }
    ((20.0 * peak.log10() + RANGE_DB) / RANGE_DB).clamp(0.0, 1.0)
}

/// The fader position that sounds like an old LinuxLink volume (format 3
/// stored pulse percentages, whose loudness is `60·log10(v/100)` dB).
pub fn position_from_percent(percent: u32) -> u32 {
    if percent == 0 {
        return 0;
    }
    let db = 60.0 * (percent as f32 / 100.0).log10();
    ((db + RANGE_DB) * 100.0 / RANGE_DB).round().clamp(0.0, 100.0) as u32
}

/// Wave Link's three-step ducking settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    Slow,
    #[default]
    Medium,
    Fast,
}

pub const STEPS: [Step; 3] = [Step::Low, Step::Medium, Step::High];
pub const SPEEDS: [Speed; 3] = [Speed::Slow, Speed::Medium, Speed::Fast];

impl Step {
    pub fn label(self) -> &'static str {
        match self {
            Step::Low => "Low",
            Step::Medium => "Medium",
            Step::High => "High",
        }
    }

    /// Key-channel level (peak dBFS) above which ducking starts. Uses Wave
    /// Link's three thresholds; more sensitive reacts to quieter sound.
    pub fn threshold_db(self) -> f32 {
        match self {
            Step::Low => -3.0,
            Step::Medium => -12.0,
            Step::High => -24.0,
        }
    }

    /// How far ducked channels are lowered, in dB (Wave Link's values).
    pub fn reduction_db(self) -> f32 {
        match self {
            Step::Low => 6.0,
            Step::Medium => 12.0,
            Step::High => 24.0,
        }
    }
}

impl Speed {
    pub fn label(self) -> &'static str {
        match self {
            Speed::Slow => "Slow",
            Speed::Medium => "Medium",
            Speed::Fast => "Fast",
        }
    }

    /// (attack, release) in milliseconds, from Wave Link's three presets.
    pub fn times_ms(self) -> (f32, f32) {
        match self {
            Speed::Fast => (30.0, 200.0),
            Speed::Medium => (80.0, 1200.0),
            Speed::Slow => (100.0, 2000.0),
        }
    }
}

/// Smoothly lowers ducked channels while a key channel is loud: ramps the
/// reduction to the full amount over the attack time, and back to zero over
/// the release time, linearly in dB.
#[derive(Debug, Default)]
pub struct Ducker {
    reduction_db: f32,
}

impl Ducker {
    /// Advances by `dt` seconds; `key_peak` is the loudest key-channel peak
    /// (linear) since the last call. Returns the current reduction in dB.
    pub fn step(&mut self, key_peak: f32, dt: f32, sensitivity: Step, amount: Step, speed: Speed) -> f32 {
        let full = amount.reduction_db();
        let (attack, release) = speed.times_ms();
        let loud = key_peak > 0.0 && 20.0 * key_peak.log10() > sensitivity.threshold_db();
        self.reduction_db = if loud {
            (self.reduction_db + full * dt * 1000.0 / attack).min(full)
        } else {
            (self.reduction_db - full * dt * 1000.0 / release).max(0.0)
        };
        // A lower amount takes effect at once rather than over a release.
        self.reduction_db = self.reduction_db.min(full);
        self.reduction_db
    }

    pub fn reset(&mut self) {
        self.reduction_db = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faders_are_linear_in_db_over_sixty_db() {
        assert_eq!(fader_db(100), Some(0.0));
        assert_eq!(fader_db(50), Some(-30.0));
        assert_eq!(fader_db(1), Some(-59.4));
        assert_eq!(fader_db(0), None);
    }

    #[test]
    fn raw_volumes_reproduce_the_db_value() {
        assert_eq!(raw_volume(Some(0.0)), RAW_NORM);
        assert_eq!(raw_volume(None), 0);
        // −30 dB is a cubic factor of 10^(−0.5).
        assert_eq!(raw_volume(Some(-30.0)), 20724);
        // A full +12 dB boost stays well within PipeWire's range.
        assert_eq!(raw_volume(Some(12.0)), 103868);
    }

    #[test]
    fn silence_anywhere_silences_the_chain() {
        assert_eq!(sum_db([Some(-6.0), Some(3.0)]), Some(-3.0));
        assert_eq!(sum_db([Some(-6.0), None]), None);
    }

    #[test]
    fn old_percent_volumes_keep_their_loudness() {
        assert_eq!(position_from_percent(100), 100);
        assert_eq!(position_from_percent(0), 0);
        assert_eq!(position_from_percent(5), 0);
        // 50% was −18 dB, which is position 70 (−18 dB) now.
        assert_eq!(position_from_percent(50), 70);
        for p in [20, 37, 50, 64, 81] {
            let old_db = 60.0 * (p as f32 / 100.0).log10();
            let new_db = fader_db(position_from_percent(p)).unwrap();
            assert!((old_db - new_db).abs() <= 0.3, "{p}%: {old_db} vs {new_db}");
        }
    }

    #[test]
    fn formats_decibels() {
        assert_eq!(format_db(Some(-12.04)), "−12.0 dB");
        assert_eq!(format_db(Some(0.01)), "0.0 dB");
        assert_eq!(format_db(Some(6.0)), "+6.0 dB");
        assert_eq!(format_db(None), "−∞ dB");
    }

    #[test]
    fn meters_use_the_sixty_db_scale() {
        assert_eq!(meter_position(1.0), 1.0);
        assert_eq!(meter_position(0.0), 0.0);
        assert!((meter_position(0.001) - 0.0).abs() < 1e-6);
        assert!((meter_position(0.0316) - 0.5).abs() < 0.01);
    }

    #[test]
    fn ducker_attacks_and_releases_at_preset_speed() {
        let mut d = Ducker::default();
        let loud = 0.5; // −6 dBFS: above every threshold but Low.
                        // Medium speed: 80 ms to the full 12 dB, 1.2 s back.
        let r = d.step(loud, 0.04, Step::Medium, Step::Medium, Speed::Medium);
        assert!((r - 6.0).abs() < 1e-3);
        let r = d.step(loud, 0.04, Step::Medium, Step::Medium, Speed::Medium);
        assert!((r - 12.0).abs() < 1e-3);
        let r = d.step(0.0, 0.6, Step::Medium, Step::Medium, Speed::Medium);
        assert!((r - 6.0).abs() < 1e-3);
        let r = d.step(0.0, 1.0, Step::Medium, Step::Medium, Speed::Medium);
        assert_eq!(r, 0.0);
        // Low sensitivity ignores −6 dBFS.
        assert_eq!(d.step(loud, 0.1, Step::Low, Step::Medium, Speed::Fast), 0.0);
    }
}
