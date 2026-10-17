//! Wave Link 3 style UI: a sidebar plus a routing matrix with channels as
//! rows and mixes as columns.

use crate::apps::{self, InstalledApp};
use crate::config::{
    ChannelCfg, ChannelKind, Config, KnownApp, Level, MixCfg, CHANNEL_ICONS, LATENCY_CHOICES, MAX_MIXES, MIX_ICONS,
};
use crate::engine::{
    channel_capture, channel_sink, effective, mix_capture, mix_source, Command, EngineHandle, Snapshot,
};
use crate::icons::IconCache;
use crate::meter::MeterBank;
use crate::updater::{InstallKind, Status, Updater};
use crate::variant::{APP_ID, APP_NAME, REPO, TEST_BUILD, VERSION};
use crate::whats_new::{self, Section};
use crate::{sys, tray};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, ComboBox, CornerRadius, FontId, Frame, Id, Layout, Margin, Mesh, Rect,
    Response, RichText, ScrollArea, Sense, Shape, Stroke, Ui, UiBuilder,
};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BG: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
const SIDEBAR: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
const CARD: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x2b);
const CARD_HOVER: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
const CARD_EMPTY: Color32 = Color32::from_rgb(0x25, 0x25, 0x25);
const NAV_SELECTED: Color32 = Color32::from_rgb(0x2d, 0x2d, 0x2d);
const MUTED_BG: Color32 = Color32::from_rgb(0x3b, 0x1d, 0x1f);
const MUTE_RED: Color32 = Color32::from_rgb(0xe0, 0x20, 0x34);
const ACCENT: Color32 = Color32::from_rgb(0x4c, 0xc2, 0xff);
const TRACK: Color32 = Color32::from_rgb(0x48, 0x48, 0x48);
const TICK: Color32 = Color32::from_rgb(0x5c, 0x5c, 0x5c);
const TEXT: Color32 = Color32::from_rgb(0xec, 0xec, 0xec);
const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9a);
const METER_LOW: Color32 = Color32::from_rgb(0x16, 0xc6, 0x0c);
const METER_HIGH: Color32 = Color32::from_rgb(0xe3, 0xe0, 0x2a);
const WHATS_NEW_WIDTH: f32 = 600.0;
const UPDATE_BUTTON_SIZE: [f32; 2] = [300.0, 32.0];
const TEST_BADGE: Color32 = Color32::from_rgb(0xff, 0x9f, 0x1c);

const SIDEBAR_W: f32 = 220.0;
const CH_W: f32 = 330.0;
const MIX_W: f32 = 250.0;
const ROW_H: f32 = 58.0;
const HEADER_H: f32 = 60.0;
const GAP: f32 = 6.0;

#[derive(Clone, PartialEq)]
enum Page {
    Mixes,
    Device(String),
    Settings,
}

enum Dialog {
    Channel { id: String, name: String, query: String },
    Mix { id: String, name: String },
    NewChannel { name: String, icon: String, source: Option<String> },
    NewMix { name: String, icon: String },
}

pub struct LinuxLinkApp {
    cfg: Config,
    engine: Arc<EngineHandle>,
    meters: MeterBank,
    icons: IconCache,
    last_frame: Instant,
    page: Page,
    dialog: Option<Dialog>,
    /// Hardware source levels changed here but not yet seen in a refresh.
    source_edits: HashMap<String, (Level, Instant)>,
    /// Installed applications, loaded the first time a channel is opened.
    installed: Option<Vec<InstalledApp>>,
    updater: Arc<Updater>,
    /// Release notes to show in the "What's new" window; empty when closed.
    whats_new: Vec<Section>,
    /// Settings shared with `main`, so a window reopened from the tray
    /// starts exactly where the last one left off.
    shared: Arc<Mutex<Config>>,
    /// Listening-device switches made by the engine that this window adopted.
    seen_listen_switches: u64,
}

impl LinuxLinkApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Arc<Mutex<Config>>,
        engine: Arc<EngineHandle>,
        updater: Arc<Updater>,
    ) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = Color32::from_rgb(0x26, 0x26, 0x26);
        visuals.window_corner_radius = CornerRadius::same(10);
        visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
        visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        visuals.hyperlink_color = ACCENT;
        cc.egui_ctx.set_visuals(visuals);
        tray::set_window(Some(cc.egui_ctx.clone()));
        let cfg = shared.lock().unwrap().clone();
        let notes = whats_new::since(cfg.last_seen_version.as_deref());
        Self {
            cfg,
            engine,
            meters: MeterBank::default(),
            icons: IconCache::new(),
            last_frame: Instant::now(),
            page: Page::Mixes,
            dialog: None,
            source_edits: HashMap::new(),
            installed: None,
            updater,
            whats_new: notes,
            shared,
            seen_listen_switches: 0,
        }
    }

    /// Remembers every app seen so channels can list them while they're idle.
    fn learn_apps(&mut self, snap: &Snapshot) {
        for app in &snap.apps {
            let known = KnownApp { name: app.name.clone(), icon: app.icon.clone() };
            if self.cfg.known_apps.get(&app.key) != Some(&known) {
                self.cfg.known_apps.insert(app.key.clone(), known);
            }
        }
    }

    // ── Sidebar ────────────────────────────────────────────────────────────

    fn sidebar(&mut self, ui: &mut Ui, snap: &Snapshot) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("〰").size(18.0).color(ACCENT));
            ui.label(RichText::new("LinuxLink").size(15.0).color(TEXT));
            if TEST_BUILD {
                ui.label(RichText::new(" TEST ").small().strong().color(Color32::BLACK).background_color(TEST_BADGE));
            }
        });
        ui.add_space(16.0);
        section_label(ui, "Your devices");
        for d in &snap.inputs {
            let page = Page::Device(d.name.clone());
            if nav_item(ui, "🎤", &d.description, self.page == page) {
                self.page = page;
            }
        }
        ui.add_space(10.0);
        divider(ui);
        ui.add_space(10.0);
        section_label(ui, "Sound & Mixes");
        if nav_item(ui, "☰", "Mixes", self.page == Page::Mixes) {
            self.page = Page::Mixes;
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(4.0);
            if nav_item(ui, "⏻", &format!("Quit {APP_NAME}"), false) {
                tray::quit();
            }
            if nav_item(ui, "⚙", "Settings", self.page == Page::Settings) {
                self.page = Page::Settings;
            }
            divider(ui);
        });
    }

    // ── Mixes page ─────────────────────────────────────────────────────────

    fn mixes_page(&mut self, ui: &mut Ui, snap: &Snapshot) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mixes").size(30.0).strong().color(TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.listen_picker(ui, snap, "listen", 260.0));
        });
        ui.add_space(14.0);

        // Per-channel app icons, resolved up front to keep borrows simple.
        let channel_icons: Vec<Vec<String>> =
            self.cfg.channels.clone().iter().map(|ch| self.channel_app_icons(ch, snap)).collect();

        ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            ui.horizontal(|ui| {
                ui.allocate_exact_size(vec2(CH_W, HEADER_H), Sense::hover());
                let mut open = None;
                let mut toggle_listen = None;
                for mix in &self.cfg.mixes {
                    let level = self.meters.level(&mix_capture(&mix.id));
                    let listening = self.cfg.listening.contains(&mix.id);
                    let (clicked, toggled) = mix_header(ui, mix, level, listening);
                    if clicked {
                        open = Some(Dialog::Mix { id: mix.id.clone(), name: mix.name.clone() });
                    }
                    if toggled {
                        toggle_listen = Some(mix.id.clone());
                    }
                }
                if let Some(id) = toggle_listen {
                    if !self.cfg.listening.remove(&id) {
                        self.cfg.listening.insert(id);
                    }
                }
                if self.cfg.mixes.len() < MAX_MIXES && add_card(ui, vec2(44.0, HEADER_H), "Add mix").clicked() {
                    open = Some(Dialog::NewMix { name: String::new(), icon: MIX_ICONS[1].into() });
                }
                if open.is_some() {
                    self.dialog = open;
                }
            });

            let mix_ids: Vec<String> = self.cfg.mixes.iter().map(|m| m.id.clone()).collect();
            self.source_edits.retain(|_, (_, t)| t.elapsed() < Duration::from_millis(1500));
            for (ch, icons) in self.cfg.channels.iter_mut().zip(&channel_icons) {
                let level = self.meters.level(&channel_capture(ch));
                // An input channel's master fader is the device's real volume.
                let source = match &ch.kind {
                    ChannelKind::Input { source } => Some(source.clone()),
                    ChannelKind::Virtual => None,
                };
                let mut master = match &source {
                    Some(src) => hardware_level(snap, &self.source_edits, src).unwrap_or_else(Level::full),
                    None => ch.level.clone(),
                };
                let before = master.clone();
                ui.horizontal(|ui| {
                    if channel_cell(ui, ch, &mut master, level, icons) {
                        self.dialog =
                            Some(Dialog::Channel { id: ch.id.clone(), name: ch.name.clone(), query: String::new() });
                    }
                    for mix_id in &mix_ids {
                        send_cell(ui, ch, mix_id, level);
                    }
                });
                if master != before {
                    match source {
                        Some(name) => {
                            self.source_edits.insert(name.clone(), (master.clone(), Instant::now()));
                            self.engine.send(Command::SetSource { name, level: master });
                        }
                        None => ch.level = master,
                    }
                }
            }

            if add_card(ui, vec2(CH_W, 48.0), "Create channel").clicked() {
                self.dialog =
                    Some(Dialog::NewChannel { name: String::new(), icon: CHANNEL_ICONS[0].into(), source: None });
            }
        });
    }

    /// Your listening device: every mix with its 🎧 toggle on plays here.
    fn listen_picker(&mut self, ui: &mut Ui, snap: &Snapshot, salt: &str, width: f32) {
        let current = self
            .cfg
            .listen_output
            .as_ref()
            .map(|o| {
                snap.outputs
                    .iter()
                    .find(|d| &d.name == o)
                    .map_or_else(|| format!("(disconnected) {o}"), |d| d.description.clone())
            })
            .unwrap_or_else(|| "No listening device".into());
        ComboBox::from_id_salt(salt)
            .width(width)
            .selected_text(format!("🎧  {current}"))
            .show_ui(ui, |ui| {
                for d in &snap.outputs {
                    ui.selectable_value(&mut self.cfg.listen_output, Some(d.name.clone()), &d.description);
                }
            })
            .response
            .on_hover_text("Your listening device. Mixes with 🎧 turned on play here.");
        // A device is either your listening device or a mix's extra output.
        if let Some(out) = self.cfg.listen_output.clone() {
            for mix in &mut self.cfg.mixes {
                mix.outputs.retain(|o| *o != out);
            }
        }
    }

    /// Adds/removes `output` on a mix; an output only ever plays one mix.
    fn assign_output(&mut self, mix_id: &str, output: &str, on: bool) {
        for mix in &mut self.cfg.mixes {
            if mix.id == mix_id {
                if on && !mix.outputs.iter().any(|o| o == output) {
                    mix.outputs.push(output.to_string());
                } else if !on {
                    mix.outputs.retain(|o| o != output);
                }
            } else if on {
                mix.outputs.retain(|o| o != output);
            }
        }
    }

    fn channel_app_icons(&mut self, ch: &ChannelCfg, snap: &Snapshot) -> Vec<String> {
        let sink = channel_sink(&ch.id);
        let mut names: Vec<String> = Vec::new();
        let assigned = self.cfg.app_rules.iter().filter(|(_, c)| **c == ch.id).map(|(k, _)| k.as_str());
        let live = snap.apps.iter().filter(|a| a.sink == sink).map(|a| a.key.as_str());
        for key in assigned.chain(live) {
            let icon = self.cfg.known_apps.get(key).and_then(|a| a.icon.clone()).unwrap_or_else(|| key.to_string());
            if !names.contains(&icon) {
                names.push(icon);
            }
        }
        names.iter().filter_map(|n| self.icons.uri(n)).take(4).collect()
    }

    // ── Device page ────────────────────────────────────────────────────────

    fn device_page(&mut self, ui: &mut Ui, snap: &Snapshot, name: &str) {
        let Some(dev) = snap.inputs.iter().find(|d| d.name == name) else {
            ui.label(RichText::new("This device is no longer connected.").color(TEXT_DIM));
            return;
        };
        ui.label(RichText::new(&dev.description).size(30.0).strong().color(TEXT));
        ui.label(RichText::new("Input device").color(TEXT_DIM));
        ui.add_space(18.0);

        self.source_edits.retain(|_, (_, t)| t.elapsed() < Duration::from_millis(1500));
        let mut level = hardware_level(snap, &self.source_edits, name).unwrap_or_else(Level::full);
        let before = level.clone();
        let meter = self.meters.level(name);

        let (rect, _) = card(ui, vec2(CH_W + MIX_W, ROW_H), CARD, Sense::hover());
        place(ui, rect.shrink2(vec2(14.0, 0.0)), Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new("Input gain").color(TEXT));
            ui.add_space(12.0);
            if speaker_button(ui, level.muted, level.volume).clicked() {
                level.muted = !level.muted;
            }
            meter_slider(ui, &mut level.volume, if level.muted { 0.0 } else { meter }, 300.0);
            ui.label(RichText::new(format!("{}%", level.volume)).color(TEXT_DIM));
        });
        if level != before {
            self.source_edits.insert(name.to_string(), (level.clone(), Instant::now()));
            self.engine.send(Command::SetSource { name: name.to_string(), level });
        }

        ui.add_space(18.0);
        let owner = self.cfg.channels.iter().find(|c| c.kind == ChannelKind::Input { source: name.to_string() });
        match owner {
            Some(ch) => {
                ui.label(RichText::new(format!("Used by the “{}” channel.", ch.name)).color(TEXT_DIM));
            }
            None => {
                if ui.button("➕  Add as channel").clicked() {
                    self.dialog = Some(Dialog::NewChannel {
                        name: dev.description.clone(),
                        icon: "🎤".into(),
                        source: Some(name.to_string()),
                    });
                }
            }
        }
    }

    // ── Settings page ──────────────────────────────────────────────────────

    fn settings_page(&mut self, ui: &mut Ui, snap: &Snapshot) {
        ui.label(RichText::new("Settings").size(30.0).strong().color(TEXT));
        ui.add_space(18.0);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(780.0);
            ui.spacing_mut().item_spacing.y = 6.0;

            section(ui, "listening", "🎧", "Listening", "What you hear", true, |ui| {
                settings_row(ui, "Listening device", "Your headphones or speakers.", |ui| {
                    self.listen_picker(ui, snap, "settings_listen", 280.0);
                });
                row_divider(ui);
                settings_row(
                    ui,
                    "Mixes you hear",
                    "Same as the 🎧 toggles on the Mixes page. Turn on as many as you like.",
                    |ui| {
                        for mix in self.cfg.mixes.iter().rev() {
                            let mut on = self.cfg.listening.contains(&mix.id);
                            if ui.checkbox(&mut on, &mix.name).changed() {
                                if on {
                                    self.cfg.listening.insert(mix.id.clone());
                                } else {
                                    self.cfg.listening.remove(&mix.id);
                                }
                            }
                        }
                    },
                );
            });

            section(ui, "general", "⚙", "General", "Startup and default output", true, |ui| {
                settings_row(ui, "System default output", "Apps without a channel play here.", |ui| {
                    let current = self
                        .cfg
                        .default_channel
                        .as_ref()
                        .and_then(|id| self.cfg.channel(id))
                        .map_or("Don't change".to_string(), |c| c.name.clone());
                    ComboBox::from_id_salt("default_channel").width(220.0).selected_text(current).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.cfg.default_channel, None, "Don't change");
                        for ch in self.cfg.channels.iter().filter(|c| c.kind == ChannelKind::Virtual) {
                            ui.selectable_value(&mut self.cfg.default_channel, Some(ch.id.clone()), &ch.name);
                        }
                    });
                });
                row_divider(ui);
                settings_row(
                    ui,
                    "Keep running when closed",
                    "Closing the window keeps LinuxLink running in the system tray, so your audio keeps working.",
                    |ui| {
                        toggle(ui, &mut self.cfg.close_to_tray);
                    },
                );
                if let (Some(_), Some(data_dir)) = (crate::integrate::appimage_path(), dirs::data_dir()) {
                    row_divider(ui);
                    settings_row(
                        ui,
                        "Show in app menu",
                        "Adds this AppImage to your app menu, which also gives its window the right icon.",
                        |ui| {
                            if toggle(ui, &mut self.cfg.appimage_menu_entry).changed() {
                                if self.cfg.appimage_menu_entry {
                                    crate::integrate::on_start(true);
                                } else {
                                    crate::integrate::remove(&data_dir);
                                }
                            }
                        },
                    );
                }
                row_divider(ui);
                settings_row(ui, "Start with your computer", "Launch LinuxLink in the tray when you log in.", |ui| {
                    let mut on = autostart_path().is_some_and(|p| p.exists());
                    if toggle(ui, &mut on).changed() {
                        if let Err(e) = set_autostart(on) {
                            eprintln!("linuxlink: autostart: {e}");
                        }
                    }
                });
            });

            let hidden_in = snap.inputs.iter().filter(|d| self.cfg.hidden_inputs.contains(&d.name)).count();
            let summary = format!("{} on, {} off", snap.inputs.len() - hidden_in, hidden_in);
            section(ui, "inputs", "🎤", "Input devices", &summary, false, |ui| {
                ui.label(
                    RichText::new("Turned-off devices are hidden from LinuxLink. Other apps can still use them.")
                        .small()
                        .color(TEXT_DIM),
                );
                ui.add_space(4.0);
                let used: Vec<String> = self
                    .cfg
                    .channels
                    .iter()
                    .filter_map(|c| match &c.kind {
                        ChannelKind::Input { source } => Some(source.clone()),
                        ChannelKind::Virtual => None,
                    })
                    .collect();
                for (i, d) in snap.inputs.iter().enumerate() {
                    if i > 0 {
                        row_divider(ui);
                    }
                    let note = if used.contains(&d.name) { "In use by a channel" } else { "" };
                    device_toggle_row(ui, d, note, &mut self.cfg.hidden_inputs);
                }
            });

            let hidden_out = snap.outputs.iter().filter(|d| self.cfg.hidden_outputs.contains(&d.name)).count();
            let summary = format!("{} on, {} off", snap.outputs.len() - hidden_out, hidden_out);
            section(ui, "outputs", "🔊", "Output devices", &summary, false, |ui| {
                ui.label(
                    RichText::new("Turned-off devices are hidden from LinuxLink. Other apps can still use them.")
                        .small()
                        .color(TEXT_DIM),
                );
                ui.add_space(4.0);
                for (i, d) in snap.outputs.iter().enumerate() {
                    if i > 0 {
                        row_divider(ui);
                    }
                    let note = if self.cfg.listen_output.as_ref() == Some(&d.name) {
                        "Your listening device"
                    } else if self.cfg.mixes.iter().any(|m| m.outputs.contains(&d.name)) {
                        "In use by a mix"
                    } else {
                        ""
                    };
                    device_toggle_row(ui, d, note, &mut self.cfg.hidden_outputs);
                }
            });

            section(ui, "audio", "〰", "Audio", "Latency", false, |ui| {
                settings_row(
                    ui,
                    "Latency",
                    "Lower is snappier; raise it if you hear crackling. Changing it briefly restarts the mixes.",
                    |ui| {
                        let current = LATENCY_CHOICES
                            .iter()
                            .find(|(ms, _)| *ms == self.cfg.latency_ms)
                            .map_or_else(|| format!("{} ms", self.cfg.latency_ms), |(_, l)| l.to_string());
                        ComboBox::from_id_salt("latency").width(220.0).selected_text(current).show_ui(ui, |ui| {
                            for (ms, label) in LATENCY_CHOICES {
                                ui.selectable_value(&mut self.cfg.latency_ms, *ms, *label);
                            }
                        });
                    },
                );
            });

            let about = format!("{APP_NAME} {VERSION}");
            section(ui, "about", "ℹ", "About", &about, false, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("What's new").clicked() {
                        self.whats_new = whats_new::sections().into_iter().filter(|s| s.version == VERSION).collect();
                    }
                    ui.hyperlink_to("Project page", format!("https://github.com/{REPO}"));
                    ui.hyperlink_to("Report a problem", format!("https://github.com/{REPO}/issues/new/choose"));
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Every mix is also a virtual microphone named “LinuxLink <mix name>”, \
                         so you can pick any mix as the audio source in OBS, Discord or a recorder.",
                    )
                    .color(TEXT_DIM),
                );
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Settings file: {}", Config::path().display())).small().color(TEXT_DIM));
            });
        });
    }

    // ── Updates ────────────────────────────────────────────────────────────

    /// The pill in the lower-right corner: update status and action.
    fn update_button(&mut self, ctx: &egui::Context) {
        let status = self.updater.status();
        let manual = matches!(self.updater.kind, InstallKind::Arch | InstallKind::Other);
        let (text, accent, enabled, tip) = match &status {
            Status::Checking => ("⟳  Checking for updates…".to_string(), false, false, String::new()),
            Status::UpToDate => (
                "✔  Up to date".to_string(),
                false,
                true,
                format!("{APP_NAME} {VERSION} is the newest version. Click to check again."),
            ),
            Status::Available { version } => (
                format!("⬇  Download new update (v{version})"),
                true,
                true,
                if TEST_BUILD {
                    "This is a test build from source. Click to open the release page.".to_string()
                } else if manual {
                    "Opens the release page. Update with your package manager (pacman or your AUR helper).".to_string()
                } else {
                    "Downloads the update and opens it in your system's installer.".to_string()
                },
            ),
            Status::Downloading { version, progress } => {
                (format!("⬇  Downloading v{version}…  {:.0}%", progress * 100.0), false, false, String::new())
            }
            Status::ReadyToRestart { version } => (
                format!("⟳  Restart to update to v{version}"),
                true,
                true,
                "The update is installed. Click to restart LinuxLink now.".to_string(),
            ),
            Status::Downloaded { version, path } => (
                format!("📦  Open the v{version} installer"),
                false,
                true,
                format!("Downloaded to {}. Click to open it again.", path.display()),
            ),
            Status::Failed(err) => {
                ("⚠  Update check failed".to_string(), false, true, format!("{err}\n\nClick to try again."))
            }
        };
        egui::Area::new(Id::unique("linuxlink_update_button"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -14.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let label = RichText::new(text).color(if accent { BG } else { TEXT });
                let button = egui::Button::new(label)
                    .fill(if accent { ACCENT } else { CARD })
                    .corner_radius(16.0)
                    .min_size(vec2(0.0, 32.0));
                // Fixed size, so the pill doesn't jump around as its text changes.
                let mut resp = ui.add_enabled_ui(enabled, |ui| ui.add_sized(UPDATE_BUTTON_SIZE, button)).inner;
                if !tip.is_empty() {
                    resp = resp.on_hover_text(tip);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.updater.press();
                    if matches!(status, Status::ReadyToRestart { .. }) {
                        tray::quit();
                    }
                }
            });
    }

    fn whats_new_window(&mut self, ctx: &egui::Context) {
        if self.whats_new.is_empty() {
            return;
        }
        let mut close = false;
        let modal = egui::Modal::new(Id::unique("linuxlink_whats_new")).show(ctx, |ui| {
            ui.set_width(WHATS_NEW_WIDTH);
            let title = match self.whats_new.as_slice() {
                [only] => format!("What's new in {APP_NAME} {}", only.version),
                _ => "What's new".to_string(),
            };
            ui.label(RichText::new(title).size(22.0).strong().color(TEXT));
            ui.add_space(10.0);
            ScrollArea::vertical().id_salt("whats_new").max_height(480.0).auto_shrink([false, true]).show(ui, |ui| {
                let several = self.whats_new.len() > 1;
                for section in &self.whats_new {
                    if several {
                        ui.label(RichText::new(format!("Version {}", section.version)).size(18.0).strong().color(TEXT));
                    }
                    if !section.date.is_empty() {
                        ui.label(RichText::new(format!("Released {}", section.date)).small().color(TEXT_DIM));
                    }
                    markdown(ui, &section.body, WHATS_NEW_WIDTH - 24.0);
                    ui.add_space(14.0);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.hyperlink_to("Full release notes", format!("https://github.com/{REPO}/releases/tag/v{VERSION}"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("  Got it  ").clicked() {
                        close = true;
                    }
                });
            });
        });
        if close || modal.should_close() {
            self.whats_new.clear();
            self.cfg.last_seen_version = Some(VERSION.to_string());
        }
    }

    // ── Dialogs ────────────────────────────────────────────────────────────

    fn dialogs(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        let Some(mut dialog) = self.dialog.take() else { return };
        let mut keep = true;
        let modal = egui::Modal::new(Id::unique("linuxlink_dialog")).show(ctx, |ui| {
            ui.set_width(440.0);
            keep = match &mut dialog {
                Dialog::Channel { id, name, query } => self.channel_dialog(ui, snap, id, name, query),
                Dialog::Mix { id, name } => self.mix_dialog(ui, snap, id, name),
                Dialog::NewChannel { name, icon, source } => self.new_channel_dialog(ui, snap, name, icon, source),
                Dialog::NewMix { name, icon } => self.new_mix_dialog(ui, name, icon),
            };
        });
        if keep && !modal.should_close() {
            self.dialog = Some(dialog);
        }
    }

    fn channel_dialog(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        id: &str,
        name: &mut String,
        query: &mut String,
    ) -> bool {
        let Some(idx) = self.cfg.channels.iter().position(|c| c.id == id) else { return false };
        dialog_title(ui, "Channel");
        ui.label(RichText::new("Name").color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Icon").color(TEXT_DIM));
        icon_picker(ui, CHANNEL_ICONS, &mut self.cfg.channels[idx].icon);
        ui.add_space(8.0);

        if let ChannelKind::Input { source } = &mut self.cfg.channels[idx].kind {
            ui.label(RichText::new("Input device").color(TEXT_DIM));
            device_combo(ui, "ch_source", source, &snap.inputs);
        } else {
            ui.label(RichText::new("Apps in this channel").color(TEXT_DIM));
            self.app_assignments(ui, snap, id, query);
        }

        ui.add_space(12.0);
        let mut keep = true;
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Delete channel").color(MUTE_RED)).clicked() {
                self.cfg.channels.remove(idx);
                self.cfg.app_rules.retain(|_, c| c != id);
                if self.cfg.default_channel.as_deref() == Some(id) {
                    self.cfg.default_channel = None;
                }
                keep = false;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Done").clicked() {
                    keep = false;
                }
            });
        });
        if !keep {
            if let Some(ch) = self.cfg.channels.iter_mut().find(|c| c.id == id) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    ch.name = trimmed.to_string();
                }
            }
        }
        keep
    }

    fn app_assignments(&mut self, ui: &mut Ui, snap: &Snapshot, id: &str, query: &mut String) {
        let search =
            ui.add(egui::TextEdit::singleline(query).hint_text("🔍  Search applications").desired_width(f32::INFINITY));
        if query.is_empty() && !search.has_focus() && ui.memory(|m| m.focused().is_none()) {
            search.request_focus();
        }
        ui.add_space(6.0);

        // Apps seen playing audio, plus installed apps matching the search.
        let q = query.trim().to_lowercase();
        let matches =
            |name: &str, key: &str| q.is_empty() || name.to_lowercase().contains(&q) || key.to_lowercase().contains(&q);
        let mut rows: Vec<(String, KnownApp)> = self
            .cfg
            .known_apps
            .iter()
            .filter(|(k, a)| matches(&a.name, k))
            .map(|(k, a)| (k.clone(), a.clone()))
            .collect();
        if !q.is_empty() {
            let installed = self.installed.get_or_insert_with(apps::installed);
            for app in installed.iter().filter(|a| matches(&a.name, &a.key)) {
                if !rows.iter().any(|(k, _)| k.eq_ignore_ascii_case(&app.key)) {
                    rows.push((app.key.clone(), KnownApp { name: app.name.clone(), icon: app.icon.clone() }));
                }
            }
        }
        // This channel's apps first, then alphabetical.
        rows.sort_by_key(|(k, a)| (self.cfg.app_rules.get(k).is_none_or(|c| c != id), a.name.to_lowercase()));
        rows.truncate(60);

        if rows.is_empty() {
            let msg = if q.is_empty() {
                "Search for an application, or start playing audio in it."
            } else {
                "No applications match your search."
            };
            ui.label(RichText::new(msg).italics().color(TEXT_DIM));
            return;
        }
        ScrollArea::vertical().max_height(280.0).auto_shrink([false, true]).show(ui, |ui| {
            for (key, app) in rows {
                let mut on = self.cfg.app_rules.get(&key).is_some_and(|c| c == id);
                ui.horizontal(|ui| {
                    let uri = app.icon.as_deref().or(Some(&key)).and_then(|n| self.icons.uri(n));
                    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    match uri {
                        Some(uri) => {
                            egui::Image::new(uri).paint_at(ui, rect);
                        }
                        None => {
                            ui.painter().rect_filled(rect, 4.0, CARD_HOVER);
                        }
                    }
                    if ui.checkbox(&mut on, &app.name).changed() {
                        if on {
                            self.cfg.app_rules.insert(key.clone(), id.to_string());
                            self.cfg.known_apps.entry(key.clone()).or_insert_with(|| app.clone());
                        } else {
                            self.cfg.app_rules.remove(&key);
                        }
                    }
                    let lower = key.to_lowercase();
                    if snap.apps.iter().any(|a| a.key == key || a.ids.contains(&lower)) {
                        ui.label(RichText::new("● playing").small().color(METER_LOW));
                    }
                    let elsewhere = self.cfg.app_rules.get(&key).filter(|c| *c != id).and_then(|c| self.cfg.channel(c));
                    if let Some(other) = elsewhere {
                        ui.label(RichText::new(format!("in {}", other.name)).small().color(TEXT_DIM));
                    }
                });
            }
        });
    }

    fn mix_dialog(&mut self, ui: &mut Ui, snap: &Snapshot, id: &str, name: &mut String) -> bool {
        let Some(idx) = self.cfg.mixes.iter().position(|m| m.id == id) else { return false };
        dialog_title(ui, "Mix");
        ui.label(RichText::new("Name").color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Icon").color(TEXT_DIM));
        icon_picker(ui, MIX_ICONS, &mut self.cfg.mixes[idx].icon);
        ui.add_space(8.0);

        let mut listening = self.cfg.listening.contains(id);
        if ui.checkbox(&mut listening, "🎧  Play this mix to me").changed() {
            if listening {
                self.cfg.listening.insert(id.to_string());
            } else {
                self.cfg.listening.remove(id);
            }
        }
        ui.add_space(8.0);

        ui.label(RichText::new("Extra outputs").color(TEXT_DIM));
        let listen_out = self.cfg.listen_output.clone();
        for d in snap.outputs.iter().filter(|d| Some(&d.name) != listen_out.as_ref()) {
            let mut on = self.cfg.mixes[idx].outputs.contains(&d.name);
            ui.horizontal(|ui| {
                if ui.checkbox(&mut on, &d.description).changed() {
                    self.assign_output(id, &d.name, on);
                }
                let other = self.cfg.mixes.iter().find(|m| m.id != id && m.outputs.contains(&d.name));
                if let Some(other) = other {
                    ui.label(RichText::new(format!("used by {}", other.name)).small().color(TEXT_DIM));
                }
            });
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Capture this mix in OBS or Discord as “LinuxLink {}”.", self.cfg.mixes[idx].name))
                .small()
                .color(TEXT_DIM),
        )
        .on_hover_text(mix_source(id));

        ui.add_space(12.0);
        let mut keep = true;
        ui.horizontal(|ui| {
            let can_delete = self.cfg.mixes.len() > 1;
            if ui.add_enabled(can_delete, egui::Button::new(RichText::new("Delete mix").color(MUTE_RED))).clicked() {
                self.cfg.mixes.remove(idx);
                for ch in &mut self.cfg.channels {
                    ch.sends.remove(id);
                }
                self.cfg.listening.remove(id);
                keep = false;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Done").clicked() {
                    keep = false;
                }
            });
        });
        if !keep {
            if let Some(mix) = self.cfg.mixes.iter_mut().find(|m| m.id == id) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    mix.name = trimmed.to_string();
                }
            }
        }
        keep
    }

    fn new_channel_dialog(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        name: &mut String,
        icon: &mut String,
        source: &mut Option<String>,
    ) -> bool {
        dialog_title(ui, "Create channel");
        ui.label(RichText::new("Name").color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(name).hint_text("e.g. Stream Alerts").desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Type").color(TEXT_DIM));
        ui.horizontal(|ui| {
            if ui.radio(source.is_none(), "Apps").clicked() {
                *source = None;
            }
            if ui.radio(source.is_some(), "Input device").clicked() && source.is_none() {
                *source = Some(snap.inputs.first().map(|d| d.name.clone()).unwrap_or_default());
            }
        });
        if let Some(src) = source {
            device_combo(ui, "new_source", src, &snap.inputs);
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Icon").color(TEXT_DIM));
        icon_picker(ui, CHANNEL_ICONS, icon);

        ui.add_space(12.0);
        let mut keep = true;
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                keep = false;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let valid = !name.trim().is_empty();
                if ui.add_enabled(valid, egui::Button::new("Create")).clicked() {
                    let kind = match source {
                        Some(s) => ChannelKind::Input { source: s.clone() },
                        None => ChannelKind::Virtual,
                    };
                    let is_input = matches!(kind, ChannelKind::Input { .. });
                    let sends = self
                        .cfg
                        .mixes
                        .iter()
                        .enumerate()
                        // Inputs start muted in your own mix so you don't hear yourself.
                        .map(|(i, m)| (m.id.clone(), Level::new(100, is_input && i == 0)))
                        .collect();
                    self.cfg.channels.push(ChannelCfg {
                        id: self.cfg.unique_id(name.trim()),
                        name: name.trim().to_string(),
                        icon: icon.clone(),
                        kind,
                        level: Level::full(),
                        sends,
                    });
                    self.page = Page::Mixes;
                    keep = false;
                }
            });
        });
        keep
    }

    fn new_mix_dialog(&mut self, ui: &mut Ui, name: &mut String, icon: &mut String) -> bool {
        dialog_title(ui, "Add mix");
        ui.label(RichText::new("Name").color(TEXT_DIM));
        ui.add(egui::TextEdit::singleline(name).hint_text("e.g. Recording").desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Icon").color(TEXT_DIM));
        icon_picker(ui, MIX_ICONS, icon);
        ui.add_space(12.0);
        let mut keep = true;
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                keep = false;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(!name.trim().is_empty(), egui::Button::new("Add")).clicked() {
                    let id = self.cfg.unique_id(name.trim());
                    for ch in &mut self.cfg.channels {
                        ch.sends.insert(id.clone(), Level::full());
                    }
                    self.cfg.mixes.push(MixCfg {
                        id,
                        name: name.trim().to_string(),
                        icon: icon.clone(),
                        outputs: Vec::new(),
                        level: Level::full(),
                    });
                    keep = false;
                }
            });
        });
        keep
    }
}

impl eframe::App for LinuxLinkApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Closing the window keeps LinuxLink running in the tray; main opens
        // a new window when asked. Without a tray, minimize so it can't get lost.
        if ctx.input(|i| i.viewport().close_requested()) && !tray::quitting() {
            if !self.cfg.close_to_tray {
                tray::set_quitting();
            } else if !tray::available() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
        let snap = self.engine.snapshot.lock().unwrap().clone();
        let before = self.cfg.clone();
        self.learn_apps(&snap);
        // You switched outputs in the system's volume controls.
        if snap.listen_switches > self.seen_listen_switches {
            self.seen_listen_switches = snap.listen_switches;
            self.cfg.listen_output = snap.listen_output.clone();
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        let mut sources: Vec<String> = self.cfg.channels.iter().map(channel_capture).collect();
        sources.extend(self.cfg.mixes.iter().map(|m| mix_capture(&m.id)));
        if let Page::Device(name) = &self.page {
            sources.push(name.clone());
        }
        self.meters.update(sources.iter().map(String::as_str), dt);

        let view = visible_devices(&snap, &self.cfg);
        egui::Panel::left("sidebar")
            .exact_size(SIDEBAR_W)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(SIDEBAR).inner_margin(Margin::same(12)))
            .show(ui, |ui| self.sidebar(ui, &view));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 12 }))
            .show(ui, |ui| {
                if let Some(err) = snap.errors.last() {
                    error_banner(ui, err, &self.engine);
                }
                match self.page.clone() {
                    Page::Mixes => self.mixes_page(ui, &view),
                    Page::Device(name) => self.device_page(ui, &snap, &name),
                    Page::Settings => self.settings_page(ui, &snap),
                }
            });
        self.dialogs(&ctx, &view);
        self.update_button(&ctx);
        if self.dialog.is_none() {
            self.whats_new_window(&ctx);
        }

        if self.cfg != before {
            self.engine.send(Command::Apply(Box::new(self.cfg.clone())));
            *self.shared.lock().unwrap() = self.cfg.clone();
        }
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

impl Drop for LinuxLinkApp {
    fn drop(&mut self) {
        tray::set_window(None);
    }
}

// ── Widgets ────────────────────────────────────────────────────────────────

/// Lays out `add` inside `rect` without reserving space in the parent, for
/// controls drawn on top of an already allocated card.
fn place<R>(ui: &mut Ui, rect: Rect, layout: Layout, add: impl FnOnce(&mut Ui) -> R) -> R {
    add(&mut ui.new_child(UiBuilder::new().max_rect(rect).layout(layout)))
}

/// A hardware source's volume: a just-made local edit, else what PipeWire
/// reports (so changes made in other mixers show up here too).
fn hardware_level(snap: &Snapshot, edits: &HashMap<String, (Level, Instant)>, name: &str) -> Option<Level> {
    if let Some((level, _)) = edits.get(name) {
        return Some(level.clone());
    }
    let dev = snap.inputs.iter().find(|d| d.name == name)?;
    Some(Level::new(dev.volume.unwrap_or(100).min(100), dev.muted))
}

fn section_label(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(14.0).color(TEXT_DIM));
    ui.add_space(4.0);
}

fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::from_gray(0x33));
}

fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let painter = ui.painter_at(rect);
    let fill = if selected {
        NAV_SELECTED
    } else if resp.hovered() {
        Color32::from_gray(0x28)
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 5.0, fill);
    if selected {
        let bar = Rect::from_min_size(rect.left_top() + vec2(0.0, 11.0), vec2(3.0, 16.0));
        painter.rect_filled(bar, 2.0, ACCENT);
    }
    painter.text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), TEXT);
    painter.text(rect.left_center() + vec2(40.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(14.5), TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn card(ui: &mut Ui, size: egui::Vec2, fill: Color32, sense: Sense) -> (Rect, Response) {
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    ui.painter().rect_filled(rect, 6.0, fill);
    (rect, resp)
}

fn add_card(ui: &mut Ui, size: egui::Vec2, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let fill = if resp.hovered() { CARD_HOVER } else { CARD_EMPTY };
    ui.painter().rect_filled(rect, 6.0, fill);
    let text = if size.x < 80.0 { "+".to_string() } else { format!("+   {label}") };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(15.0), TEXT);
    resp.on_hover_text(label).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// dBFS-scaled 0..1 position for a linear peak (-60 dB .. 0 dB).
fn meter_frac(level: f32) -> f32 {
    let db = 20.0 * level.max(1e-5).log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Green→yellow meter fill covering `frac` of `rect`.
fn paint_meter(painter: &egui::Painter, rect: Rect, frac: f32) {
    if frac <= 0.0 {
        return;
    }
    let fill = Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * frac, rect.bottom()));
    let right = lerp_color(METER_LOW, METER_HIGH, frac);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(fill.left_top(), METER_LOW);
    mesh.colored_vertex(fill.right_top(), right);
    mesh.colored_vertex(fill.right_bottom(), right);
    mesh.colored_vertex(fill.left_bottom(), METER_LOW);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// Volumes a dragged fader sticks to while it is within `SNAP_RANGE` of them.
const SNAP_POINTS: [u32; 3] = [25, 50, 75];
const SNAP_RANGE: u32 = 2;

fn snap_volume(v: u32) -> u32 {
    SNAP_POINTS.into_iter().find(|p| p.abs_diff(v) <= SNAP_RANGE).unwrap_or(v)
}

/// Wave Link style fader: a thin track with the level meter inside it and a
/// round knob for the volume.
fn meter_slider(ui: &mut Ui, value: &mut u32, level: f32, width: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
    let r = 8.0;
    let (x0, x1) = (rect.left() + r, rect.right() - r);
    if resp.is_pointer_button_down_on() {
        if let Some(p) = resp.interact_pointer_pos() {
            let v = snap_volume((((p.x - x0) / (x1 - x0)).clamp(0.0, 1.0) * 100.0).round() as u32);
            if v != *value {
                *value = v;
                resp.mark_changed();
            }
        }
    }
    let painter = ui.painter();
    let cy = rect.center().y;
    let track = Rect::from_min_max(pos2(x0, cy - 2.5), pos2(x1, cy + 2.5));
    for i in 0..=10 {
        let x = x0 + (x1 - x0) * i as f32 / 10.0;
        let h = if i % 5 == 0 { 4.0 } else { 2.5 };
        painter.line_segment([pos2(x, cy + 5.0), pos2(x, cy + 5.0 + h)], Stroke::new(1.0, TICK));
    }
    painter.rect_filled(track, 3.0, TRACK);
    paint_meter(painter, track, meter_frac(level));
    let knob = pos2(x0 + (x1 - x0) * *value as f32 / 100.0, cy);
    let active = resp.hovered() || resp.dragged();
    painter.circle_filled(knob, r, Color32::from_rgb(0x3a, 0x3a, 0x3a));
    painter.circle_filled(knob, if active { 6.0 } else { 5.0 }, ACCENT);
    resp.on_hover_text(format!("{value}%"))
}

fn speaker_button(ui: &mut Ui, muted: bool, volume: u32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    let painter = ui.painter();
    if muted {
        painter.rect_filled(rect, 5.0, MUTE_RED);
        painter.text(rect.center(), Align2::CENTER_CENTER, "🔇", FontId::proportional(15.0), Color32::WHITE);
    } else {
        if resp.hovered() {
            painter.rect_filled(rect, 5.0, Color32::from_white_alpha(12));
        }
        let glyph = match volume {
            0 => "🔈",
            1..=49 => "🔉",
            _ => "🔊",
        };
        painter.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(15.0), TEXT);
    }
    resp.on_hover_text(if muted { "Unmute" } else { "Mute" })
}

/// Mix column header with a 🎧 toggle on the right that decides whether the
/// mix plays to you. Returns (header clicked, listen toggle clicked).
fn mix_header(ui: &mut Ui, mix: &MixCfg, level: f32, listening: bool) -> (bool, bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(MIX_W, HEADER_H), Sense::click());
    let btn = Rect::from_center_size(pos2(rect.right() - 26.0, rect.center().y - 1.0), vec2(32.0, 32.0));
    let listen = ui.interact(btn, ui.scope_id().with(("listen", &mix.id)), Sense::click());
    let painter = ui.painter_at(rect);
    let hovered = resp.hovered() && !listen.hovered();
    painter.rect_filled(rect, 6.0, if hovered { CARD_HOVER } else { CARD });
    painter.text(
        rect.left_center() + vec2(16.0, -2.0),
        Align2::LEFT_CENTER,
        &mix.icon,
        FontId::proportional(22.0),
        TEXT,
    );
    painter.text(rect.left_top() + vec2(54.0, 13.0), Align2::LEFT_TOP, &mix.name, FontId::proportional(15.0), TEXT);
    let n = mix.outputs.len();
    let sub = match n {
        0 => "Virtual mic".to_string(),
        1 => "1 Extra output".to_string(),
        _ => format!("{n} Extra outputs"),
    };
    painter.text(rect.left_top() + vec2(54.0, 33.0), Align2::LEFT_TOP, sub, FontId::proportional(13.0), TEXT_DIM);
    if mix.level.muted {
        painter.text(
            btn.left_center() - vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            "🔇",
            FontId::proportional(14.0),
            MUTE_RED,
        );
    }

    if listening {
        painter.circle_filled(btn.center(), 15.0, ACCENT.gamma_multiply(if listen.hovered() { 0.4 } else { 0.25 }));
        painter.text(btn.center(), Align2::CENTER_CENTER, "🎧", FontId::proportional(15.0), ACCENT);
    } else {
        if listen.hovered() {
            painter.circle_filled(btn.center(), 15.0, Color32::from_white_alpha(14));
        }
        let off = Color32::from_gray(0x6a);
        painter.text(btn.center(), Align2::CENTER_CENTER, "🎧", FontId::proportional(15.0), off);
        painter.line_segment([btn.center() + vec2(-9.0, 9.0), btn.center() + vec2(9.0, -9.0)], Stroke::new(1.5, off));
    }

    let bar = Rect::from_min_max(
        pos2(rect.left(), rect.bottom() - 3.0),
        pos2(rect.left() + rect.width() * 0.66, rect.bottom()),
    );
    let gain = if mix.level.muted { 0.0 } else { (mix.level.volume as f32 / 100.0).powi(3) };
    paint_meter(&painter, bar, meter_frac(level * gain));

    let tip = if listening {
        "Playing to you. Click to stop hearing this mix."
    } else {
        "Not playing to you. Click to hear this mix."
    };
    let toggled = listen.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    let clicked = resp.on_hover_text("Mix settings").on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    (clicked && !toggled, toggled)
}

fn paint_channel_icon(ui: &Ui, rect: Rect, ch: &ChannelCfg, icons: &[String]) {
    let painter = ui.painter();
    match icons.len() {
        0 => {
            painter.rect_filled(rect, 7.0, Color32::from_rgb(0x3a, 0x3a, 0x3a));
            painter.text(rect.center(), Align2::CENTER_CENTER, &ch.icon, FontId::proportional(20.0), TEXT);
        }
        1 => {
            egui::Image::new(icons[0].as_str()).paint_at(ui, rect);
        }
        _ => {
            let s = (rect.width() - 2.0) / 2.0;
            for (i, uri) in icons.iter().enumerate() {
                let off = vec2((i % 2) as f32 * (s + 2.0), (i / 2) as f32 * (s + 2.0));
                egui::Image::new(uri.as_str()).paint_at(ui, Rect::from_min_size(rect.min + off, vec2(s, s)));
            }
        }
    }
}

/// Row header: icon, name, channel master. Returns true to open settings.
fn channel_cell(ui: &mut Ui, ch: &ChannelCfg, master: &mut Level, level: f32, icons: &[String]) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(CH_W, ROW_H), Sense::hover());
    let info = Rect::from_min_size(rect.min, vec2(168.0, ROW_H));
    let open = ui.interact(info, ui.scope_id().with(("ch_open", &ch.id)), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, CARD);
    if open.hovered() {
        painter.rect_filled(info, 6.0, CARD_HOVER);
    }
    paint_channel_icon(ui, Rect::from_min_size(rect.left_top() + vec2(10.0, 9.0), vec2(40.0, 40.0)), ch, icons);
    let galley = ui.painter().layout(ch.name.clone(), FontId::proportional(15.0), TEXT, 105.0);
    let pos = pos2(rect.left() + 60.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, TEXT);

    let controls = Rect::from_min_max(pos2(rect.left() + 170.0, rect.top()), pos2(rect.right() - 8.0, rect.bottom()));
    place(ui, controls, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if speaker_button(ui, master.muted, master.volume).clicked() {
            master.muted = !master.muted;
        }
        let shown = if master.muted { 0.0 } else { level };
        meter_slider(ui, &mut master.volume, shown, 106.0);
    });
    open.on_hover_text("Channel settings").on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// One cell of the routing matrix: this channel's send into one mix.
fn send_cell(ui: &mut Ui, ch: &mut ChannelCfg, mix_id: &str, level: f32) {
    let Some(send) = ch.sends.get(mix_id).cloned() else {
        let (rect, resp) = ui.allocate_exact_size(vec2(MIX_W, ROW_H), Sense::click());
        ui.painter().rect_filled(rect, 6.0, CARD_EMPTY);
        if resp.hovered() {
            ui.painter().circle_filled(rect.center(), 13.0, CARD_HOVER);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), TEXT);
        }
        if resp.on_hover_text("Add to this mix").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            ch.sends.insert(mix_id.to_string(), Level::full());
        }
        return;
    };

    let (rect, resp) = ui.allocate_exact_size(vec2(MIX_W, ROW_H), Sense::click());
    ui.painter().rect_filled(rect, 6.0, if send.muted { MUTED_BG } else { CARD });
    let mut new = send.clone();
    let inner = Rect::from_center_size(rect.center(), vec2(150.0, ROW_H));
    place(ui, inner, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if speaker_button(ui, new.muted, new.volume).clicked() {
            new.muted = !new.muted;
        }
        let eff = effective(ch, &new);
        let shown = if eff.muted { 0.0 } else { level * (eff.volume as f32 / 100.0).powi(3) };
        meter_slider(ui, &mut new.volume, shown, 112.0);
    });
    let mut remove = false;
    resp.context_menu(|ui| {
        if ui.button("Remove from this mix").clicked() {
            remove = true;
        }
    });
    if remove {
        ch.sends.remove(mix_id);
    } else if new != send {
        ch.sends.insert(mix_id.to_string(), new);
    }
}

fn error_banner(ui: &mut Ui, err: &str, engine: &EngineHandle) {
    Frame::new().fill(MUTED_BG).corner_radius(6.0).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("⚠  {err}")).color(TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Dismiss").clicked() {
                    engine.snapshot.lock().unwrap().errors.clear();
                }
            });
        });
    });
    ui.add_space(10.0);
}

fn dialog_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(20.0).strong().color(TEXT));
    ui.add_space(10.0);
}

fn icon_picker(ui: &mut Ui, choices: &[&str], icon: &mut String) {
    ui.horizontal_wrapped(|ui| {
        for c in choices {
            let selected = icon == c;
            if ui.selectable_label(selected, RichText::new(*c).size(18.0)).clicked() {
                *icon = c.to_string();
            }
        }
    });
}

fn device_combo(ui: &mut Ui, salt: &str, selected: &mut String, devices: &[crate::pactl::Device]) {
    let current = devices
        .iter()
        .find(|d| &d.name == selected)
        .map_or_else(|| format!("(disconnected) {selected}"), |d| d.description.clone());
    ComboBox::from_id_salt(salt).width(300.0).selected_text(current).show_ui(ui, |ui| {
        for d in devices {
            ui.selectable_value(selected, d.name.clone(), &d.description);
        }
    });
}

/// A collapsible settings card: click the header to open or close it.
fn section(
    ui: &mut Ui,
    key: &str,
    icon: &str,
    title: &str,
    subtitle: &str,
    default_open: bool,
    body: impl FnOnce(&mut Ui),
) {
    let id = Id::unique(("linuxlink_settings", key));
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    Frame::new().fill(CARD).corner_radius(8.0).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let header = ui
            .horizontal(|ui| {
                ui.label(RichText::new(icon).size(17.0).color(TEXT));
                ui.add_space(4.0);
                ui.label(RichText::new(title).size(16.0).color(TEXT));
                ui.label(RichText::new(subtitle).color(TEXT_DIM));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let arrow = if state.is_open() { "⏶" } else { "⏷" };
                    ui.label(RichText::new(arrow).color(TEXT_DIM));
                });
            })
            .response;
        let click = ui.interact(header.rect, id.with("header"), Sense::click());
        if click.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            state.toggle(ui);
        }
        state.show_body_unindented(ui, |ui| {
            ui.add_space(10.0);
            body(ui);
        });
    });
}

fn settings_row(ui: &mut Ui, title: &str, help: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 320.0).max(200.0));
            ui.label(RichText::new(title).color(TEXT));
            if !help.is_empty() {
                ui.label(RichText::new(help).small().color(TEXT_DIM));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
}

fn row_divider(ui: &mut Ui) {
    ui.add_space(2.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::from_gray(0x36));
    ui.add_space(2.0);
}

/// One device with an on/off switch; off hides it from LinuxLink.
fn device_toggle_row(ui: &mut Ui, d: &crate::pactl::Device, note: &str, hidden: &mut BTreeSet<String>) {
    settings_row(ui, &d.description, note, |ui| {
        let mut on = !hidden.contains(&d.name);
        if toggle(ui, &mut on).changed() {
            if on {
                hidden.remove(&d.name);
            } else {
                hidden.insert(d.name.clone());
            }
        }
    });
}

/// The device lists with turned-off devices removed, except ones LinuxLink
/// is actively using (so they stay visible where they're selected).
fn visible_devices(snap: &Snapshot, cfg: &Config) -> Snapshot {
    let used_inputs: Vec<&str> = cfg
        .channels
        .iter()
        .filter_map(|c| match &c.kind {
            ChannelKind::Input { source } => Some(source.as_str()),
            ChannelKind::Virtual => None,
        })
        .collect();
    let in_use_output = |name: &str| {
        cfg.listen_output.as_deref() == Some(name) || cfg.mixes.iter().any(|m| m.outputs.iter().any(|o| o == name))
    };
    let mut view = snap.clone();
    view.inputs.retain(|d| !cfg.hidden_inputs.contains(&d.name) || used_inputs.contains(&d.name.as_str()));
    view.outputs.retain(|d| !cfg.hidden_outputs.contains(&d.name) || in_use_output(&d.name));
    view
}

/// iOS/Windows style on/off switch.
fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let painter = ui.painter();
    painter.rect_filled(rect, 10.0, lerp_color(TRACK, ACCENT, t));
    let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
    painter.circle_filled(pos2(x, rect.center().y), 7.0, Color32::WHITE);
    resp
}

fn autostart_path() -> Option<PathBuf> {
    Some(sys::host_config_dir()?.join("autostart").join(format!("{APP_ID}.desktop")))
}

fn set_autostart(on: bool) -> std::io::Result<()> {
    let path = autostart_path().ok_or(std::io::ErrorKind::NotFound)?;
    if !on {
        return std::fs::remove_file(path);
    }
    let exec = if sys::in_flatpak() {
        format!("flatpak run {APP_ID}")
    } else if let Some(appimage) = std::env::var_os("APPIMAGE") {
        format!("\"{}\"", appimage.to_string_lossy())
    } else {
        format!("\"{}\"", std::env::current_exe()?.display())
    };
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(
        path,
        format!(
            "[Desktop Entry]\nType=Application\nName={APP_NAME}\nExec={exec} --minimized\nIcon={APP_ID}\n\
             Terminal=false\nX-GNOME-Autostart-enabled=true\n"
        ),
    )
}

/// Renders the small Markdown subset used in CHANGELOG.md: `###` headings,
/// `-` bullets (with wrapped continuation lines), **bold**, `code`, links.
fn markdown(ui: &mut Ui, text: &str, width: f32) {
    let mut blocks: Vec<String> = Vec::new();
    for line in text.lines() {
        // Hard-wrapped lines continue the paragraph or bullet above them.
        let starts_block = line.starts_with("- ") || line.starts_with('#');
        let continues = !line.trim().is_empty() && !starts_block;
        match blocks.last_mut() {
            Some(last) if continues && !last.is_empty() => {
                last.push(' ');
                last.push_str(line.trim());
            }
            _ => blocks.push(line.trim().to_string()),
        }
    }
    for block in blocks {
        if block.is_empty() {
            ui.add_space(4.0);
        } else if let Some(heading) = block.strip_prefix("### ") {
            ui.add_space(6.0);
            ui.label(RichText::new(heading).size(16.0).strong().color(ACCENT));
        } else if let Some(item) = block.strip_prefix("- ") {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new("•").color(ACCENT));
                wrapped(ui, inline_markdown(item), width - 16.0);
            });
        } else {
            wrapped(ui, inline_markdown(&block), width);
        }
    }
}

/// A label wrapped at exactly `width` (a Label alone wraps at whatever space
/// is available, which is unbounded inside a modal that sizes to its content).
fn wrapped(ui: &mut Ui, job: egui::text::LayoutJob, width: f32) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_max_width(width);
        ui.label(job);
    });
}

fn inline_markdown(text: &str) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let (mut bold, mut code) = (false, false);
    let mut rest = text;
    while !rest.is_empty() {
        let (piece, skip) = if rest.starts_with("**") {
            bold = !bold;
            ("", 2)
        } else if rest.starts_with('*') {
            // *emphasis*: drop the markers.
            ("", 1)
        } else if rest.starts_with('`') {
            code = !code;
            ("", 1)
        } else if let Some((label, after)) = rest.strip_prefix('[').and_then(|r| r.split_once("](")) {
            // [label](url): keep the label.
            let close = after.find(')').map_or(after.len(), |i| i + 1);
            let consumed = 1 + label.len() + 2 + close;
            (label, consumed)
        } else {
            let end = rest
                .find(['*', '`', '['])
                .filter(|&i| i > 0)
                .unwrap_or_else(|| rest.chars().next().unwrap().len_utf8());
            let end = if rest.find(['*', '`', '[']).is_none() { rest.len() } else { end };
            (&rest[..end], end)
        };
        if !piece.is_empty() {
            let font = if code { FontId::monospace(13.5) } else { FontId::proportional(14.5) };
            let color = if bold { Color32::WHITE } else { Color32::from_gray(0xc8) };
            job.append(piece, 0.0, TextFormat { font_id: font, color, ..Default::default() });
        }
        rest = &rest[skip.min(rest.len())..];
    }
    job
}

/// A small standalone window with a message, for problems before startup.
pub fn show_notice(title: &str, message: &str) -> anyhow::Result<()> {
    struct Notice(String);
    impl eframe::App for Notice {
        fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.add_space(6.0);
                ui.label(RichText::new(&self.0).size(15.0));
                ui.add_space(14.0);
                if ui.button("  OK  ").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("{APP_NAME}: {title}"))
            .with_app_id(APP_ID)
            .with_inner_size([440.0, 170.0])
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Notice(message.to_string())))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::{snap_volume, visible_devices};
    use crate::config::{ChannelKind, Config};
    use crate::engine::Snapshot;
    use crate::pactl::Device;

    fn dev(name: &str) -> Device {
        Device { index: 0, name: name.into(), description: name.into(), volume: None, muted: false }
    }

    #[test]
    fn hidden_devices_disappear_unless_in_use() {
        let mut cfg = Config::default();
        cfg.channels[0].kind = ChannelKind::Input { source: "mic".into() };
        cfg.listen_output = Some("phones".into());
        cfg.hidden_inputs = ["mic", "line_in"].map(String::from).into();
        cfg.hidden_outputs = ["phones", "hdmi"].map(String::from).into();
        let snap = Snapshot {
            inputs: vec![dev("mic"), dev("line_in"), dev("usb_mic")],
            outputs: vec![dev("phones"), dev("hdmi"), dev("speakers")],
            ..Default::default()
        };
        let view = visible_devices(&snap, &cfg);
        let names = |d: &[Device]| d.iter().map(|d| d.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&view.inputs), ["mic", "usb_mic"]);
        assert_eq!(names(&view.outputs), ["phones", "speakers"]);
    }

    #[test]
    fn snaps_within_two_percent_of_quarter_marks() {
        assert_eq!(snap_volume(23), 25);
        assert_eq!(snap_volume(27), 25);
        assert_eq!(snap_volume(28), 28);
        assert_eq!(snap_volume(49), 50);
        assert_eq!(snap_volume(77), 75);
        assert_eq!(snap_volume(78), 78);
        assert_eq!(snap_volume(0), 0);
        assert_eq!(snap_volume(100), 100);
    }
}
