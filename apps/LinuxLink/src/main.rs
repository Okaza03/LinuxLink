mod apps;
mod config;
mod engine;
mod icons;
mod instance;
mod integrate;
mod meter;
mod pactl;
mod sys;
mod tray;
mod ui;
mod updater;
mod variant;
mod whats_new;

use config::Config;
use eframe::egui;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use variant::{APP_ID, APP_NAME, VERSION};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{APP_NAME} {VERSION}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--cleanup") {
        engine::cleanup_stale();
        println!("Removed leftover LinuxLink devices.");
        return Ok(());
    }

    // LinuxLink and LinuxLink Test drive the same virtual devices, so only one
    // may run. Starting this one again just shows the running window.
    let guard = match instance::acquire() {
        instance::Start::First(guard) => guard,
        instance::Start::Shown => return Ok(()),
        instance::Start::OtherRunning => {
            return ui::show_notice(
                "Already running",
                "Another LinuxLink (or LinuxLink Test) is already running.\n\n\
                 Quit it from its tray icon first, then start this one again.",
            );
        }
    };

    let mut cfg = Config::load();
    integrate::on_start(cfg.appimage_menu_entry);
    let defaults = engine::prepare(&mut cfg)?;
    let engine = engine::spawn(cfg.clone(), defaults);
    let updater = updater::Updater::start();
    let shared = Arc::new(Mutex::new(cfg));

    let (requests_tx, requests) = mpsc::channel();
    let _tray = tray::spawn(requests_tx);
    instance::serve(tray::open);

    // Ctrl+C, and how logout and shutdown end LinuxLink: quit for real (not to
    // the tray) so the devices are removed and the settings saved.
    let handle = engine.clone();
    ctrlc::set_handler(move || {
        tray::quit();
        std::thread::sleep(Duration::from_secs(3));
        handle.shutdown();
        std::process::exit(0);
    })?;

    // Started at login: begin in the tray, without a window.
    let mut show_window = !(args.iter().any(|a| a == "--minimized") && tray::available());
    let mut result = Ok(());
    loop {
        if show_window {
            result = run_window(&shared, &engine, &updater);
        }
        if tray::quitting() || !tray::available() {
            break;
        }
        // The window was closed into the tray. Drop clicks made while it was
        // open, then wait to be asked for a window again, or to quit.
        while requests.try_recv().is_ok() {}
        match requests.recv() {
            Ok(tray::Request::Open) if !tray::quitting() => show_window = true,
            _ => break,
        }
    }
    engine.shutdown();

    // "Restart to update": launch the freshly installed version.
    if let Some(program) = updater.take_restart() {
        drop(guard);
        std::process::Command::new(program).env_remove("APPIMAGE").spawn()?;
    }
    result
}

fn run_window(
    shared: &Arc<Mutex<Config>>,
    engine: &Arc<engine::EngineHandle>,
    updater: &Arc<updater::Updater>,
) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_app_id(APP_ID)
            .with_icon(eframe::icon_data::from_png_bytes(integrate::ICON_256).expect("bundled icon is a valid PNG"))
            .with_inner_size([1560.0, 900.0])
            .with_min_inner_size([900.0, 520.0]),
        ..Default::default()
    };
    let (shared, engine, updater) = (shared.clone(), engine.clone(), updater.clone());
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(ui::LinuxLinkApp::new(cc, shared, engine, updater)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
