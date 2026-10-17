//! "What's new" notes, read from CHANGELOG.md compiled into the app.

use crate::updater::is_newer;
use crate::variant::VERSION;

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub version: String,
    pub date: String,
    pub body: String,
}

/// Every released version in the changelog, newest first.
pub fn sections() -> Vec<Section> {
    parse(CHANGELOG)
}

fn parse(changelog: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut current: Option<Section> = None;
    for line in changelog.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            out.extend(current.take());
            // "## [0.2.0] - 2026-11-01"; skips "## [Unreleased]".
            let version = head.split(']').next().unwrap_or("").trim_start_matches('[').trim();
            if version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let date = head.rsplit(" - ").next().filter(|d| *d != head).unwrap_or("").trim();
                current = Some(Section { version: version.into(), date: date.into(), body: String::new() });
            }
        } else if line.starts_with('[') && line.contains("]: http") {
            // Link reference definitions at the end of the file.
            continue;
        } else if let Some(section) = &mut current {
            section.body.push_str(line);
            section.body.push('\n');
        }
    }
    out.extend(current);
    for s in &mut out {
        s.body = s.body.trim().to_string();
    }
    out
}

/// Notes to show after starting a new version: every version newer than the
/// last one seen, up to this one. A first run just shows this version.
pub fn since(last_seen: Option<&str>) -> Vec<Section> {
    sections()
        .into_iter()
        .filter(|s| !is_newer(&s.version, VERSION))
        .filter(|s| match last_seen {
            Some(seen) => is_newer(&s.version, seen),
            None => s.version == VERSION,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Changelog\n\n## [Unreleased]\n\n- wip\n\n## [0.2.0] - 2026-11-01\n\n### Added\n\n- Thing\n\n## [0.1.0] - 2026-10-17\n\n- First\n\n[0.2.0]: https://example.com\n";

    #[test]
    fn parses_released_sections() {
        let s = parse(SAMPLE);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].version.as_str(), s[0].date.as_str()), ("0.2.0", "2026-11-01"));
        assert_eq!(s[0].body, "### Added\n\n- Thing");
        assert_eq!(s[1].body, "- First");
    }

    #[test]
    fn current_version_has_notes() {
        assert!(sections().iter().any(|s| s.version == VERSION), "CHANGELOG.md needs a section for {VERSION}");
    }
}
