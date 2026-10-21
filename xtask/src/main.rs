//! Workspace tasks. Run with `cargo xtask <task>`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  install [--autostart]   build \"LinuxLink Test\" from this checkout and install it for your user
  uninstall               remove LinuxLink Test
  dist                    build the release tarball and AppImage folder into target/dist";

const APP_ID: &str = "io.github.Okaza03.LinuxLink";
const TEST_ID: &str = "io.github.Okaza03.LinuxLink.Test";
const TEST_BIN: &str = "linuxlink-test";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME is not set"))
}

fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map_or_else(|| home().join(".local/share"), PathBuf::from)
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home().join(".config"), PathBuf::from)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("running {cmd:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd:?} failed"))
    }
}

fn cargo() -> Command {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.current_dir(root());
    cmd
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(path, contents).map_err(|e| format!("writing {}: {e}", path.display()))
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::copy(from, to).map(drop).map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
}

fn linux_file(ext: &str) -> PathBuf {
    root().join("packaging/linux").join(format!("{APP_ID}.{ext}"))
}

// ── LinuxLink Test ─────────────────────────────────────────────────────────

struct TestPaths {
    bin: PathBuf,
    desktop: PathBuf,
    icon: PathBuf,
    autostart: PathBuf,
}

impl TestPaths {
    fn new() -> Self {
        Self {
            bin: home().join(".local/bin").join(TEST_BIN),
            desktop: data_dir().join("applications").join(format!("{TEST_ID}.desktop")),
            icon: data_dir().join("icons/hicolor/scalable/apps").join(format!("{TEST_ID}.svg")),
            autostart: config_dir().join("autostart").join(format!("{TEST_ID}.desktop")),
        }
    }
}

/// The app icon with an orange "TEST" badge.
fn test_icon() -> Result<String, String> {
    let badge = r##"  <rect x="22" y="44" width="40" height="17" rx="4" fill="#ff9f1c"/>
  <text x="42" y="57" text-anchor="middle" font-family="sans-serif" font-weight="bold" font-size="12" fill="#1b1f27">TEST</text>
</svg>"##;
    Ok(read(&linux_file("svg"))?.replace("</svg>", badge))
}

fn test_desktop_entry(exec: &Path, extra_args: &str) -> Result<String, String> {
    let entry = read(&linux_file("desktop"))?
        .replace("Name=LinuxLink", "Name=LinuxLink Test")
        .replace("Exec=linuxlink", &format!("Exec=\"{}\"{extra_args}", exec.display()))
        .replace(APP_ID, TEST_ID);
    Ok(entry)
}

/// Removes files from installs made before the Test variant existed.
fn remove_legacy_install() {
    let legacy_desktop = data_dir().join("applications/linuxlink.desktop");
    if legacy_desktop.exists() {
        for p in [
            legacy_desktop,
            data_dir().join("icons/hicolor/scalable/apps/linuxlink.svg"),
            home().join(".local/bin/linuxlink"),
            config_dir().join("autostart/linuxlink.desktop"),
        ] {
            if fs::remove_file(&p).is_ok() {
                println!("  removed old {}", p.display());
            }
        }
    }
}

fn install(autostart: bool) -> Result<(), String> {
    println!("Building LinuxLink Test...");
    run(cargo()
        .args(["build", "--release", "-p", "linuxlink"])
        // A separate target dir keeps release builds and test builds apart.
        .env("CARGO_TARGET_DIR", root().join("target/test-build"))
        .env("LINUXLINK_TEST_BUILD", "1"))?;

    remove_legacy_install();
    let paths = TestPaths::new();
    let built = root().join("target/test-build/release/linuxlink");
    // Copy then rename, so a running instance's binary isn't truncated.
    let tmp = paths.bin.with_extension("new");
    copy(&built, &tmp)?;
    fs::rename(&tmp, &paths.bin).map_err(|e| e.to_string())?;
    println!("  installed {}", paths.bin.display());

    write(&paths.icon, &test_icon()?)?;
    write(&paths.desktop, &test_desktop_entry(&paths.bin, "")?)?;
    println!("  installed {}", paths.desktop.display());
    if autostart {
        write(&paths.autostart, &test_desktop_entry(&paths.bin, " --minimized")?)?;
        println!("  enabled start on login");
    }

    // Start the test build with the real app's settings, once.
    let test_cfg = config_dir().join("linuxlink-test/config.toml");
    let real_cfg = config_dir().join("linuxlink/config.toml");
    if !test_cfg.exists() && real_cfg.exists() {
        copy(&real_cfg, &test_cfg)?;
        println!("  copied your settings to {}", test_cfg.display());
    }

    let _ = Command::new("update-desktop-database").arg(paths.desktop.parent().unwrap()).output();
    let _ = Command::new("kbuildsycoca6").arg("--noincremental").output();
    println!("\nDone. Start \"LinuxLink Test\" from your app menu or run `{TEST_BIN}`.");
    Ok(())
}

fn uninstall() -> Result<(), String> {
    let paths = TestPaths::new();
    if paths.bin.exists() {
        let _ = Command::new(&paths.bin).arg("--cleanup").output();
        let _ = Command::new(&paths.bin).arg("--restore-devices").status();
    }
    for p in [&paths.bin, &paths.desktop, &paths.icon, &paths.autostart] {
        match fs::remove_file(p) {
            Ok(()) => println!("  removed {}", p.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("removing {}: {e}", p.display())),
        }
    }
    remove_legacy_install();
    let _ = Command::new("kbuildsycoca6").arg("--noincremental").output();
    println!("\nLinuxLink Test was removed. Its settings are kept in ~/.config/linuxlink-test/.");
    Ok(())
}

// ── Release artifacts ──────────────────────────────────────────────────────

/// Installs the app's files under `prefix` (a `usr`-style tree).
fn stage_files(prefix: &Path) -> Result<(), String> {
    let bin = prefix.join("bin/linuxlink");
    copy(&root().join("target/release/linuxlink"), &bin)?;
    fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).map_err(|e| e.to_string())?;
    copy(&linux_file("desktop"), &prefix.join(format!("share/applications/{APP_ID}.desktop")))?;
    copy(&linux_file("svg"), &prefix.join(format!("share/icons/hicolor/scalable/apps/{APP_ID}.svg")))?;
    for size in [128, 256] {
        copy(
            &root().join(format!("packaging/linux/{APP_ID}-{size}.png")),
            &prefix.join(format!("share/icons/hicolor/{size}x{size}/apps/{APP_ID}.png")),
        )?;
    }
    copy(&linux_file("metainfo.xml"), &prefix.join(format!("share/metainfo/{APP_ID}.metainfo.xml")))
}

/// Builds the release binary, the tarball and the AppImage folder.
fn dist() -> Result<(), String> {
    run(cargo().args(["build", "--release", "--locked", "-p", "linuxlink"]))?;
    let arch = std::env::consts::ARCH;
    let dist = root().join("target/dist");
    let _ = fs::remove_dir_all(&dist);

    // Tarball: binary, desktop integration and install.sh, in one folder.
    let name = format!("linuxlink-{VERSION}-{arch}-linux");
    let dir = dist.join(&name);
    stage_files(&dir.join("staging"))?;
    fs::rename(dir.join("staging/bin/linuxlink"), dir.join("linuxlink")).map_err(|e| e.to_string())?;
    fs::rename(dir.join("staging/share"), dir.join("share")).map_err(|e| e.to_string())?;
    fs::remove_dir_all(dir.join("staging")).map_err(|e| e.to_string())?;
    for f in ["install.sh", "uninstall.sh"] {
        copy(&root().join("packaging/tarball").join(f), &dir.join(f))?;
        fs::set_permissions(dir.join(f), std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    for f in ["README.md", "CHANGELOG.md", "LICENSE-MIT", "LICENSE-APACHE"] {
        copy(&root().join(f), &dir.join(f))?;
    }
    run(Command::new("tar").arg("-czf").arg(dist.join(format!("{name}.tar.gz"))).arg("-C").arg(&dist).arg(&name))?;
    println!("  built {}", dist.join(format!("{name}.tar.gz")).display());

    // AppDir for appimagetool.
    let appdir = dist.join("AppDir");
    stage_files(&appdir.join("usr"))?;
    copy(&root().join("packaging/appimage/AppRun"), &appdir.join("AppRun"))?;
    fs::set_permissions(appdir.join("AppRun"), std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    copy(&linux_file("desktop"), &appdir.join(format!("{APP_ID}.desktop")))?;
    copy(&linux_file("svg"), &appdir.join(format!("{APP_ID}.svg")))?;
    std::os::unix::fs::symlink(format!("{APP_ID}.svg"), appdir.join(".DirIcon")).map_err(|e| e.to_string())?;
    println!("  built {}", appdir.display());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("install") => install(args.iter().any(|a| a == "--autostart")),
        Some("uninstall") => uninstall(),
        Some("dist") => dist(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
