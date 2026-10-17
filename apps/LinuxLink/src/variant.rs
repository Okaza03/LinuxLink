//! Which build this is: a release package, or the local "LinuxLink Test"
//! build that `cargo xtask install` makes from a source checkout.

/// Set by `cargo xtask install` (`LINUXLINK_TEST_BUILD=1`).
pub const TEST_BUILD: bool = option_env!("LINUXLINK_TEST_BUILD").is_some();

pub const APP_NAME: &str = if TEST_BUILD { "LinuxLink Test" } else { "LinuxLink" };
pub const APP_ID: &str = if TEST_BUILD { "io.github.Okaza03.LinuxLink.Test" } else { "io.github.Okaza03.LinuxLink" };
/// Settings folder name under `~/.config`, separate so tests can't touch real settings.
pub const CONFIG_DIR: &str = if TEST_BUILD { "linuxlink-test" } else { "linuxlink" };
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO: &str = "Okaza03/LinuxLink";
