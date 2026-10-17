mod apps;
mod config;
mod engine;
mod icons;
mod meter;
mod pactl;
mod ui;

use config::Config;
use eframe::egui;
use std::sync::OnceLock;
use std::time::Duration;

static EGUI_CTX: OnceLock<egui::Context> = OnceLock::new();

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--cleanup") {
        engine::cleanup_stale();
        println!("Removed leftover LinuxLink devices.");
        return Ok(());
    }

    let mut cfg = Config::load();
    let defaults = engine::prepare(&mut cfg)?;
    let engine = engine::spawn(cfg.clone(), defaults);

    // On Ctrl+C / SIGTERM close the window so the normal exit path tears the
    // virtual devices down; exiting mid-frame crashes the renderer.
    let handle = engine.clone();
    ctrlc::set_handler(move || {
        if let Some(ctx) = EGUI_CTX.get() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            std::thread::sleep(Duration::from_secs(3));
        }
        handle.shutdown();
        std::process::exit(0);
    })?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LinuxLink")
            .with_app_id("linuxlink")
            .with_inner_size([1560.0, 900.0])
            .with_min_inner_size([900.0, 520.0]),
        ..Default::default()
    };
    let app_engine = engine.clone();
    let result = eframe::run_native(
        "LinuxLink",
        options,
        Box::new(move |cc| {
            let _ = EGUI_CTX.set(cc.egui_ctx.clone());
            Ok(Box::new(ui::LinuxLinkApp::new(cc, cfg, app_engine)))
        }),
    );
    engine.shutdown();
    result.map_err(|e| anyhow::anyhow!("{e}"))
}
