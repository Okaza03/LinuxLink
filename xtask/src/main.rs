//! Workspace tasks. Run with `cargo xtask <task>`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  install [--autostart]   build LinuxLink and install it for the current user
  uninstall               remove everything `install` put in place";

const APP_ID: &str = "linuxlink";

const ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect width="64" height="64" rx="14" fill="#1b1f27"/>
  <g stroke-linecap="round" stroke-width="5">
    <line x1="18" y1="14" x2="18" y2="50" stroke="#3a4250"/>
    <line x1="32" y1="14" x2="32" y2="50" stroke="#3a4250"/>
    <line x1="46" y1="14" x2="46" y2="50" stroke="#3a4250"/>
    <line x1="18" y1="34" x2="18" y2="50" stroke="#3d8bfd"/>
    <line x1="32" y1="22" x2="32" y2="50" stroke="#3d8bfd"/>
    <line x1="46" y1="40" x2="46" y2="50" stroke="#3d8bfd"/>
  </g>
  <g fill="#e8ecf2">
    <rect x="12" y="30" width="12" height="7" rx="3"/>
    <rect x="26" y="18" width="12" height="7" rx="3"/>
    <rect x="40" y="36" width="12" height="7" rx="3"/>
  </g>
</svg>
"##;

struct Paths {
    bin: PathBuf,
    desktop: PathBuf,
    icon: PathBuf,
    autostart: PathBuf,
}

impl Paths {
    fn new() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").expect("HOME is not set"));
        let data = std::env::var_os("XDG_DATA_HOME").map_or_else(|| home.join(".local/share"), PathBuf::from);
        let config = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from);
        Self {
            bin: home.join(".local/bin").join(APP_ID),
            desktop: data.join("applications").join(format!("{APP_ID}.desktop")),
            icon: data.join("icons/hicolor/scalable/apps").join(format!("{APP_ID}.svg")),
            autostart: config.join("autostart").join(format!("{APP_ID}.desktop")),
        }
    }
}

fn desktop_entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]
Type=Application
Name=LinuxLink
GenericName=Audio Mixer
Comment=Wave Link style virtual audio mixer for PipeWire
Exec={}
Icon={APP_ID}
Terminal=false
Categories=AudioVideo;Audio;Mixer;
Keywords=audio;mixer;stream;obs;pipewire;wave;
StartupWMClass={APP_ID}
",
        exec.display()
    )
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn write(path: &Path, contents: &str) -> std::io::Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, contents)?;
    println!("  installed {}", path.display());
    Ok(())
}

fn install(autostart: bool) -> Result<(), String> {
    let root = workspace_root();
    println!("Building LinuxLink (release)...");
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--release", "-p", "linuxlink"])
        .current_dir(&root)
        .status()
        .map_err(|e| format!("running cargo: {e}"))?;
    if !status.success() {
        return Err("build failed".into());
    }

    let paths = Paths::new();
    let built = root.join("target/release").join(APP_ID);
    fs::create_dir_all(paths.bin.parent().unwrap()).map_err(|e| e.to_string())?;
    // Copy to a temp name and rename, so a running instance's binary isn't
    // truncated underneath it.
    let tmp = paths.bin.with_extension("new");
    fs::copy(&built, &tmp).map_err(|e| format!("copying {}: {e}", built.display()))?;
    fs::rename(&tmp, &paths.bin).map_err(|e| e.to_string())?;
    println!("  installed {}", paths.bin.display());

    let entry = desktop_entry(&paths.bin);
    write(&paths.icon, ICON_SVG).map_err(|e| e.to_string())?;
    write(&paths.desktop, &entry).map_err(|e| e.to_string())?;
    if autostart {
        write(&paths.autostart, &entry).map_err(|e| e.to_string())?;
    }

    // Refresh menu caches; harmless if these tools aren't present.
    let _ = Command::new("update-desktop-database").arg(paths.desktop.parent().unwrap()).output();
    let _ = Command::new("kbuildsycoca6").arg("--noincremental").output();

    println!("\nDone. Launch \"LinuxLink\" from your app menu or run `{APP_ID}`.");
    if !autostart {
        println!("To start it on login: cargo xtask install --autostart");
    }
    Ok(())
}

fn uninstall() -> Result<(), String> {
    let paths = Paths::new();
    // Drop any virtual devices a running/crashed instance left behind.
    if paths.bin.exists() {
        let _ = Command::new(&paths.bin).arg("--cleanup").output();
    }
    for p in [&paths.bin, &paths.desktop, &paths.icon, &paths.autostart] {
        match fs::remove_file(p) {
            Ok(()) => println!("  removed {}", p.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("removing {}: {e}", p.display())),
        }
    }
    let _ = Command::new("kbuildsycoca6").arg("--noincremental").output();
    println!("\nUninstalled. Settings were kept in ~/.config/linuxlink/.");
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("install") => install(args.iter().any(|a| a == "--autostart")),
        Some("uninstall") => uninstall(),
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
