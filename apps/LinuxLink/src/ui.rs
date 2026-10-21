//! Wave Link 3 style UI: a sidebar plus a routing matrix with channels as
//! rows and mixes as columns.

use crate::apps::{self, InstalledApp};
use crate::config::{
    Boost, ChannelCfg, ChannelKind, Config, KnownApp, Level, MixCfg, Theme, WindowMode, CHANNEL_ICONS, LATENCY_CHOICES,
    MAX_MIXES, MIX_ICONS,
};
use crate::engine::{
    channel_capture, channel_sink, mix_capture, mix_source, send_db, send_muted, Command, EngineHandle, Snapshot,
};
use crate::icons::IconCache;
use crate::levels::{self, SPEEDS, STEPS};
use crate::meter::MeterBank;
use crate::updater::{InstallKind, Status, Updater};
use crate::variant::{APP_ID, APP_NAME, REPO, TEST_BUILD, VERSION};
use crate::whats_new::{self, Section};
use crate::{sys, tray};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, ComboBox, CornerRadius, FontFamily, FontId, Frame, Id, Layout, Margin,
    Rect, Response, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Wave Link 3's colours, sampled from screenshots of its dark and light themes.
#[derive(Clone, Copy)]
struct Palette {
    /// The content area.
    bg: Color32,
    /// The sidebar and window chrome.
    sidebar: Color32,
    card: Color32,
    card_hover: Color32,
    card_empty: Color32,
    /// Card outlines (light theme only).
    border: Color32,
    nav_selected: Color32,
    nav_hover: Color32,
    divider: Color32,
    muted_bg: Color32,
    mute_red: Color32,
    /// Selection bar, toggles, links.
    accent: Color32,
    /// Fader knobs.
    knob: Color32,
    track: Color32,
    /// The peak-hold marker in a fader.
    tick: Color32,
    text: Color32,
    text_dim: Color32,
    meter_low: Color32,
    meter_high: Color32,
    /// The square behind a mix's icon.
    tile: Color32,
    /// Small square buttons (listen, compact view).
    button: Color32,
    /// Dialogs.
    window: Color32,
    dark: bool,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const DARK: Palette = Palette {
    bg: rgb(0x272727),
    sidebar: rgb(0x202020),
    card: rgb(0x323232),
    card_hover: rgb(0x3a3a3a),
    card_empty: rgb(0x2d2d2d),
    border: rgb(0x323232),
    nav_selected: rgb(0x2d2d2d),
    nav_hover: rgb(0x292929),
    divider: rgb(0x333333),
    muted_bg: rgb(0x4a2428),
    mute_red: rgb(0xe0203a),
    accent: rgb(0x4cc2ff),
    knob: rgb(0x71d4db),
    track: rgb(0x3e3e3e),
    tick: rgb(0xaeaeae),
    text: rgb(0xffffff),
    text_dim: rgb(0xa8a8a8),
    meter_low: rgb(0x3bb455),
    meter_high: rgb(0xfddb01),
    tile: rgb(0xffffff),
    button: rgb(0x3e3e3e),
    window: rgb(0x2b2b2b),
    dark: true,
};

const LIGHT: Palette = Palette {
    bg: rgb(0xf9f9f9),
    sidebar: rgb(0xf3f3f3),
    card: rgb(0xfdfdfd),
    card_hover: rgb(0xf3f3f3),
    card_empty: rgb(0xfbfbfb),
    border: rgb(0xe5e5e5),
    nav_selected: rgb(0xeaeaea),
    nav_hover: rgb(0xededed),
    divider: rgb(0xe5e5e5),
    muted_bg: rgb(0xfde7e9),
    mute_red: rgb(0xc42b1c),
    accent: rgb(0x0067c0),
    knob: rgb(0x0065cc),
    track: rgb(0xe6e6e6),
    tick: rgb(0x8a8a8a),
    text: rgb(0x1b1b1b),
    text_dim: rgb(0x616161),
    meter_low: rgb(0x3bb455),
    meter_high: rgb(0xe3b500),
    tile: rgb(0xffffff),
    button: rgb(0xf3f3f3),
    window: rgb(0xffffff),
    dark: false,
};

static LIGHT_THEME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The palette of the theme in use.
fn p() -> &'static Palette {
    if LIGHT_THEME.load(std::sync::atomic::Ordering::Relaxed) {
        &LIGHT
    } else {
        &DARK
    }
}

/// Makes egui's own widgets (buttons, combo boxes, text fields) match.
fn apply_theme(ctx: &egui::Context, light: bool) {
    LIGHT_THEME.store(light, std::sync::atomic::Ordering::Relaxed);
    let pal = p();
    let mut v = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = pal.bg;
    v.window_fill = pal.window;
    v.window_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = if light { rgb(0xffffff) } else { rgb(0x1f1f1f) };
    v.selection.bg_fill = pal.accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, pal.accent);
    v.hyperlink_color = pal.accent;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = CornerRadius::same(4);
    }
    v.widgets.inactive.weak_bg_fill = pal.card;
    v.widgets.inactive.bg_fill = pal.card;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, if light { rgb(0xe0e0e0) } else { rgb(0x3a3a3a) });
    v.widgets.hovered.weak_bg_fill = pal.card_hover;
    v.widgets.hovered.bg_fill = pal.card_hover;
    v.widgets.noninteractive.fg_stroke.color = pal.text;
    v.widgets.inactive.fg_stroke.color = pal.text;
    ctx.set_visuals(v);
}

const WHATS_NEW_WIDTH: f32 = 600.0;
const UPDATE_BUTTON_SIZE: [f32; 2] = [300.0, 32.0];
const TEST_BADGE: Color32 = Color32::from_rgb(0xff, 0x9f, 0x1c);

const COMPACT_W: f32 = 430.0;
const FULL_SIZE: [f32; 2] = [1560.0, 900.0];
const FULL_MIN_SIZE: [f32; 2] = [900.0, 520.0];

// Wave Link 3's proportions.
const SIDEBAR_W: f32 = 230.0;
const CH_W: f32 = 310.0;
const MIX_W: f32 = 190.0;
const ROW_H: f32 = 54.0;
const HEADER_H: f32 = 52.0;
const GAP: f32 = 6.0;
const RADIUS: f32 = 6.0;

#[derive(Clone, PartialEq)]
enum Page {
    Mixes,
    Device(String),
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    General,
    Listening,
    Ducking,
    Devices,
    Api,
    Updates,
    About,
}

const SETTINGS_TABS: [(SettingsTab, &str, &str); 7] = [
    (SettingsTab::General, "⚙", "General"),
    (SettingsTab::Listening, "🎧", "Listening"),
    (SettingsTab::Ducking, "↓", "Voice ducking"),
    (SettingsTab::Devices, "🔊", "Devices"),
    (SettingsTab::Api, "🔌", "Control API"),
    (SettingsTab::Updates, "⟳", "Updates"),
    (SettingsTab::About, "ℹ", "About"),
];

enum Dialog {
    /// Wave Link's "Create channel" list: inputs and apps, or an empty channel.
    CreateChannel {
        query: String,
    },
    Channel {
        id: String,
        name: String,
        query: String,
    },
    Mix {
        id: String,
        name: String,
    },
    NewChannel {
        name: String,
        icon: String,
        source: Option<String>,
    },
    NewMix {
        name: String,
        icon: String,
    },
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
    /// Settings changes made through the control API that this window adopted.
    seen_api_revision: u64,
    /// The window mode the window was last sized for.
    shown_mode: Option<WindowMode>,
    /// Whether the light palette is applied (None until the first frame).
    shown_light: Option<bool>,
    settings_tab: SettingsTab,
    /// The device page's tab: input & monitoring (false) or settings (true).
    device_settings_tab: bool,
    /// Frames drawn, for the debug screenshot hook.
    #[cfg(debug_assertions)]
    frames: u32,
}

impl LinuxLinkApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shared: Arc<Mutex<Config>>,
        engine: Arc<EngineHandle>,
        updater: Arc<Updater>,
    ) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        install_fonts(&cc.egui_ctx);
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
            seen_api_revision: crate::api::revision(),
            shown_mode: None,
            shown_light: None,
            settings_tab: SettingsTab::General,
            device_settings_tab: false,
            #[cfg(debug_assertions)]
            frames: 0,
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
            ui.label(RichText::new("〰").size(16.0).color(p().accent));
            ui.label(RichText::new(APP_NAME).size(13.5).color(p().text));
            if TEST_BUILD {
                ui.label(RichText::new(" TEST ").small().strong().color(Color32::BLACK).background_color(TEST_BADGE));
            }
        });
        ui.add_space(14.0);
        section_label(ui, "Your devices");
        for d in &snap.inputs {
            let page = Page::Device(d.name.clone());
            if nav_item(ui, "🎤", &d.description, self.page == page) {
                self.page = page;
                self.device_settings_tab = false;
            }
        }
        ui.add_space(8.0);
        divider(ui);
        ui.add_space(8.0);
        section_label(ui, "Mixes & effects");
        if nav_item(ui, "🎚", "Mixes", self.page == Page::Mixes) {
            self.page = Page::Mixes;
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(4.0);
            if nav_item(ui, "⚙", "Settings", self.page == Page::Settings) {
                self.page = Page::Settings;
            }
        });
    }

    // ── Mixes page ─────────────────────────────────────────────────────────

    fn mixes_page(&mut self, ui: &mut Ui, snap: &Snapshot) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Mixes").font(semibold(28.0)).color(p().text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if square_button(ui, "🗗", "Shrink mixes view").clicked() {
                    self.cfg.window_mode = WindowMode::Compact;
                }
                ui.add_space(2.0);
                self.listen_picker(ui, snap, "listen", 280.0);
            });
        });
        ui.add_space(16.0);

        // Per-channel app icons, resolved up front to keep borrows simple.
        let channel_icons: Vec<Vec<String>> =
            self.cfg.channels.clone().iter().map(|ch| self.channel_app_icons(ch, snap)).collect();

        ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            ui.horizontal(|ui| {
                ui.allocate_exact_size(vec2(CH_W, HEADER_H), Sense::hover());
                let mut open = None;
                let mut toggle_listen = None;
                let mut shift = None;
                let last = self.cfg.mixes.len().saturating_sub(1);
                for (i, mix) in self.cfg.mixes.iter().enumerate() {
                    let level = self.meters.level(&mix_capture(&mix.id));
                    let listening = self.cfg.listening.contains(&mix.id);
                    let (clicked, toggled, moved) = mix_header(ui, mix, level, listening, i > 0, i < last);
                    if clicked {
                        open = Some(Dialog::Mix { id: mix.id.clone(), name: mix.name.clone() });
                    }
                    if toggled {
                        toggle_listen = Some(mix.id.clone());
                    }
                    if let Some(by) = moved {
                        shift = Some((i, by));
                    }
                }
                if let Some(id) = toggle_listen {
                    if !self.cfg.listening.remove(&id) {
                        self.cfg.listening.insert(id);
                    }
                }
                if let Some((i, by)) = shift {
                    move_item(&mut self.cfg.mixes, i, by);
                }
                if self.cfg.mixes.len() < MAX_MIXES && add_card(ui, vec2(44.0, HEADER_H), "Add mix").clicked() {
                    open = Some(Dialog::NewMix { name: String::new(), icon: MIX_ICONS[1].into() });
                }
                if open.is_some() {
                    self.dialog = open;
                }
            });

            // The mixes' own volumes, one fader per column.
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(CH_W, 40.0), Sense::hover());
                ui.painter().text(
                    rect.left_center() + vec2(14.0, 0.0),
                    Align2::LEFT_CENTER,
                    "Mix volume",
                    FontId::proportional(14.0),
                    p().text_dim,
                );
                for mix in &mut self.cfg.mixes {
                    let level = self.meters.level(&mix_capture(&mix.id));
                    mix_volume_cell(ui, mix, level);
                }
            });

            let mix_ids: Vec<String> = self.cfg.mixes.iter().map(|m| m.id.clone()).collect();
            self.source_edits.retain(|_, (_, t)| t.elapsed() < Duration::from_millis(1500));
            let ducking = self.cfg.ducking.clone();
            let listening = self.cfg.listening.clone();
            let ducking_now = snap.ducking_db > 0.0;
            let visible: Vec<usize> = (0..self.cfg.channels.len()).filter(|&i| !self.cfg.channels[i].hidden).collect();
            let mut action = None;
            for (pos, &idx) in visible.iter().enumerate() {
                let icons = &channel_icons[idx];
                let ch = &mut self.cfg.channels[idx];
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
                    let movable = (pos > 0, pos + 1 < visible.len());
                    match channel_cell(ui, ch, &mut master, level, icons, movable) {
                        Some(RowAction::Open) => {
                            self.dialog = Some(Dialog::Channel {
                                id: ch.id.clone(),
                                name: ch.name.clone(),
                                query: String::new(),
                            });
                        }
                        Some(a) => action = Some((pos, a)),
                        None => {}
                    }
                    for mix_id in &mix_ids {
                        let cell = CellState {
                            ducked: ducking.ducks(&ch.id, mix_id),
                            ducking_now,
                            hear_yourself: matches!(ch.kind, ChannelKind::Input { .. }) && listening.contains(mix_id),
                        };
                        send_cell(ui, ch, mix_id, level, cell);
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
            match action {
                Some((pos, RowAction::Hide)) => self.cfg.channels[visible[pos]].hidden = true,
                // Swap with the neighbouring visible row (hidden ones stay put).
                Some((pos, RowAction::MoveUp)) => self.cfg.channels.swap(visible[pos], visible[pos - 1]),
                Some((pos, RowAction::MoveDown)) => self.cfg.channels.swap(visible[pos], visible[pos + 1]),
                _ => {}
            }
            let hidden: Vec<(usize, String)> = self
                .cfg
                .channels
                .iter()
                .enumerate()
                .filter(|(_, c)| c.hidden)
                .map(|(i, c)| (i, c.name.clone()))
                .collect();
            if !hidden.is_empty() {
                let label = match hidden.len() {
                    1 => "👁  1 hidden channel".to_string(),
                    n => format!("👁  {n} hidden channels"),
                };
                ui.menu_button(RichText::new(label).color(p().text_dim), |ui| {
                    ui.label(
                        RichText::new("Hidden channels keep playing. Click one to show it.")
                            .small()
                            .color(p().text_dim),
                    );
                    for (i, name) in hidden {
                        if ui.button(name).clicked() {
                            self.cfg.channels[i].hidden = false;
                        }
                    }
                });
            }

            if add_card(ui, vec2(CH_W, 50.0), "Create channel").clicked() {
                self.dialog = Some(Dialog::CreateChannel { query: String::new() });
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
            .height(400.0)
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

    /// Wave Link's device page: the device and its mute on the left, its
    /// input and settings in tabs on the right.
    fn device_page(&mut self, ui: &mut Ui, snap: &Snapshot, name: &str) {
        let Some(dev) = snap.inputs.iter().find(|d| d.name == name).cloned() else {
            ui.label(RichText::new("This device is no longer connected.").color(p().text_dim));
            return;
        };
        self.source_edits.retain(|_, (_, t)| t.elapsed() < Duration::from_millis(1500));
        let mut level = hardware_level(snap, &self.source_edits, name).unwrap_or_else(Level::full);
        let before = level.clone();
        let meter = self.meters.level(name);
        let height = (ui.available_height() - 8.0).clamp(320.0, 520.0);

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            panel(ui, vec2(360.0, height), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(height * 0.16);
                    ui.label(RichText::new("🎤").size(96.0).color(p().text));
                    ui.add_space(height * 0.08);
                    ui.label(RichText::new(&dev.description).font(semibold(20.0)).color(p().text));
                    ui.add_space(18.0);
                    let label = if level.muted { "🎤  Unmute" } else { "🎤  Mute" };
                    let fill = if level.muted { p().mute_red } else { p().card };
                    let text = if level.muted { Color32::WHITE } else { p().text };
                    let button =
                        egui::Button::new(RichText::new(label).color(text)).fill(fill).min_size(vec2(270.0, 32.0));
                    if ui.add(button).clicked() {
                        level.muted = !level.muted;
                    }
                    ui.add_space(10.0);
                    let (rect, _) = ui.allocate_exact_size(vec2(270.0, 8.0), Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, p().track);
                    paint_meter(ui.painter(), rect.shrink(1.5), if level.muted { 0.0 } else { meter_frac(meter) });
                });
            });
            let width = ui.available_width().max(360.0);
            panel(ui, vec2(width, height), |ui| {
                ui.horizontal(|ui| {
                    for (settings, label) in [(false, "Input & monitoring"), (true, "Settings")] {
                        if tab_button(ui, label, self.device_settings_tab == settings) {
                            self.device_settings_tab = settings;
                        }
                    }
                });
                divider(ui);
                ui.add_space(12.0);
                if !self.device_settings_tab {
                    group_heading(ui, "Input");
                    settings_row(ui, "Input gain", "The device's own volume, as other apps see it too.", |ui| {
                        ui.label(RichText::new(format!("{}%", level.volume)).color(p().text_dim));
                        meter_slider(ui, &mut level.volume, if level.muted { 0.0 } else { meter }, 220.0)
                            .on_hover_text(format!("{}%", level.volume));
                    });
                } else {
                    group_heading(ui, "General");
                    settings_row(ui, "Name", "", |ui| {
                        ui.label(RichText::new(&dev.description).color(p().text));
                    });
                    row_divider(ui);
                    settings_row(ui, "Device", "", |ui| {
                        ui.label(RichText::new(name).small().color(p().text_dim));
                    });
                    row_divider(ui);
                    let owner =
                        self.cfg.channels.iter().find(|c| c.kind == ChannelKind::Input { source: name.to_string() });
                    match owner {
                        Some(ch) => settings_row(ui, "Channel", "", |ui| {
                            ui.label(RichText::new(format!("Used by “{}”", ch.name)).color(p().text_dim));
                        }),
                        None => settings_row(ui, "Channel", "Not in any mix yet.", |ui| {
                            if ui.button("Add as channel").clicked() {
                                self.add_channel(
                                    &dev.description,
                                    "🎤",
                                    ChannelKind::Input { source: name.to_string() },
                                );
                                self.page = Page::Mixes;
                            }
                        }),
                    }
                }
            });
        });
        if level != before {
            self.source_edits.insert(name.to_string(), (level.clone(), Instant::now()));
            self.engine.send(Command::SetSource { name: name.to_string(), level });
        }
    }

    // ── Settings page ──────────────────────────────────────────────────────

    /// Wave Link's settings: tabs on the left, the chosen page on the right.
    fn settings_page(&mut self, ui: &mut Ui, snap: &Snapshot) {
        ui.label(RichText::new("Settings").font(semibold(28.0)).color(p().text));
        ui.add_space(16.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                for (tab, icon, label) in SETTINGS_TABS {
                    if nav_item(ui, icon, label, self.settings_tab == tab) {
                        self.settings_tab = tab;
                    }
                }
            });
            ui.add_space(28.0);
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(720.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                // The tabs sit in a row; the page itself runs top to bottom.
                ui.vertical(|ui| match self.settings_tab {
                    SettingsTab::General => self.general_settings(ui),
                    SettingsTab::Listening => self.listening_settings(ui, snap),
                    SettingsTab::Ducking => {
                        group_heading(ui, "Voice ducking");
                        self.ducking_settings(ui);
                    }
                    SettingsTab::Devices => self.device_settings(ui, snap),
                    SettingsTab::Api => self.api_settings(ui),
                    SettingsTab::Updates => self.update_settings(ui),
                    SettingsTab::About => self.about_settings(ui),
                });
            });
        });
    }

    fn general_settings(&mut self, ui: &mut Ui) {
        group_heading(ui, "Personalization");
        settings_row(ui, "Color theme", "", |ui| {
            let label = |t: Theme| match t {
                Theme::System => "Same as system",
                Theme::Light => "Light",
                Theme::Dark => "Dark",
            };
            ComboBox::from_id_salt("theme").width(220.0).selected_text(label(self.cfg.theme)).show_ui(ui, |ui| {
                for t in [Theme::System, Theme::Light, Theme::Dark] {
                    ui.selectable_value(&mut self.cfg.theme, t, label(t));
                }
            });
        });
        ui.add_space(14.0);
        group_heading(ui, "Startup");
        settings_row(ui, "Start app at login", "Launch LinuxLink in the tray when you log in.", |ui| {
            let mut on = autostart_path().is_some_and(|p| p.exists());
            if toggle(ui, &mut on).changed() {
                if let Err(e) = set_autostart(on) {
                    eprintln!("linuxlink: autostart: {e}");
                }
            }
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
        ui.add_space(14.0);
        group_heading(ui, "Audio");
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
        ui.add_space(20.0);
        if ui.button(format!("Quit {APP_NAME}")).on_hover_text("Removes LinuxLink's devices and closes it").clicked() {
            tray::quit();
        }
    }

    fn listening_settings(&mut self, ui: &mut Ui, snap: &Snapshot) {
        group_heading(ui, "Listening");
        settings_row(ui, "Listening device", "Your headphones or speakers.", |ui| {
            self.listen_picker(ui, snap, "settings_listen", 280.0);
        });
        row_divider(ui);
        settings_row(
            ui,
            "Mixes you hear",
            "Same as the 🎧 buttons on the mix headers. Turn on as many as you like.",
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
    }

    fn device_settings(&mut self, ui: &mut Ui, snap: &Snapshot) {
        group_heading(ui, "Turned-off devices");
        settings_row(
            ui,
            "Turn them off on this computer too",
            "Turned-off sound-card devices also disappear from your system's sound settings and other \
             apps. Applying a change restarts the system's audio service, so sound drops out for a \
             moment. Devices LinuxLink uses and Bluetooth devices are only hidden from LinuxLink.",
            |ui| {
                toggle(ui, &mut self.cfg.disable_hidden_on_system);
            },
        );
        let note = if self.cfg.disable_hidden_on_system {
            "Turned-off devices are hidden from LinuxLink, and sound cards from the whole computer."
        } else {
            "Turned-off devices are hidden from LinuxLink. Other apps can still use them."
        };
        let system_off = self.cfg.system_disabled();
        ui.add_space(14.0);
        group_heading(ui, "Input devices");
        ui.label(RichText::new(note).small().color(p().text_dim));
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
            device_toggle_row(ui, d, note, &mut self.cfg.hidden_inputs, &mut self.cfg.hidden_labels);
        }
        let missing = missing_devices(&self.cfg.hidden_inputs, &snap.inputs);
        for (i, name) in missing.iter().enumerate() {
            if i > 0 || !snap.inputs.is_empty() {
                row_divider(ui);
            }
            missing_device_row(ui, name, &system_off, &mut self.cfg.hidden_inputs, &mut self.cfg.hidden_labels);
        }
        ui.add_space(14.0);
        group_heading(ui, "Output devices");
        ui.label(RichText::new(note).small().color(p().text_dim));
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
            device_toggle_row(ui, d, note, &mut self.cfg.hidden_outputs, &mut self.cfg.hidden_labels);
        }
        let missing = missing_devices(&self.cfg.hidden_outputs, &snap.outputs);
        for (i, name) in missing.iter().enumerate() {
            if i > 0 || !snap.outputs.is_empty() {
                row_divider(ui);
            }
            missing_device_row(ui, name, &system_off, &mut self.cfg.hidden_outputs, &mut self.cfg.hidden_labels);
        }
    }

    fn api_settings(&mut self, ui: &mut Ui) {
        group_heading(ui, "Control API");
        settings_row(
            ui,
            "Allow control by other apps",
            "A local WebSocket API in the style of Wave Link's, for Stream Deck style controllers and \
             scripts. Only programs on this computer can use it, never websites.",
            |ui| {
                toggle(ui, &mut self.cfg.api_enabled);
            },
        );
        if let (true, Some(port)) = (self.cfg.api_enabled, crate::api::port()) {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "JSON-RPC 2.0 at ws://127.0.0.1:{port}. The port is also written to {}.",
                    Config::path().with_file_name("ws-info.json").display()
                ))
                .small()
                .color(p().text_dim),
            );
        }
    }

    fn update_settings(&mut self, ui: &mut Ui) {
        group_heading(ui, "Updates");
        let status = self.updater.status();
        let (text, action) = match &status {
            Status::Checking => ("Checking for updates…".to_string(), None),
            Status::UpToDate => (format!("{APP_NAME} {VERSION} is the newest version."), Some("Check again")),
            Status::Available { version } => (format!("Version {version} is available."), Some("Download update")),
            Status::Downloading { version, progress } => {
                (format!("Downloading version {version}… {:.0}%", progress * 100.0), None)
            }
            Status::ReadyToRestart { version } => (format!("Version {version} is installed."), Some("Restart now")),
            Status::Downloaded { version, .. } => (format!("Version {version} is downloaded."), Some("Open installer")),
            Status::Failed(err) => (format!("Couldn't check for updates: {err}"), Some("Try again")),
        };
        settings_row(ui, &format!("{APP_NAME} {VERSION}"), &text, |ui| {
            if let Some(label) = action {
                if ui.button(label).clicked() {
                    self.updater.press();
                    if matches!(status, Status::ReadyToRestart { .. }) {
                        tray::quit();
                    }
                }
            }
        });
        row_divider(ui);
        settings_row(ui, "Release notes", "What changed in this version.", |ui| {
            if ui.button("What's new").clicked() {
                self.whats_new = whats_new::sections().into_iter().filter(|s| s.version == VERSION).collect();
            }
        });
    }

    fn about_settings(&mut self, ui: &mut Ui) {
        group_heading(ui, "Help");
        ui.horizontal(|ui| {
            ui.hyperlink_to("Project page", format!("https://github.com/{REPO}"));
            ui.add_space(12.0);
            ui.hyperlink_to("Report a problem", format!("https://github.com/{REPO}/issues/new/choose"));
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Every mix is also a virtual microphone named “LinuxLink <mix name>”, \
                 so you can pick any mix as the audio source in OBS, Discord or a recorder.",
            )
            .color(p().text_dim),
        );
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Settings file: {}", Config::path().display())).small().color(p().text_dim));
        ui.add_space(20.0);
        ui.label(
            RichText::new(format!(
                "{APP_NAME} {VERSION}. Not affiliated with Elgato or Corsair; Wave Link is their trademark."
            ))
            .small()
            .color(p().text_dim),
        );
    }

    /// Wave Link's ducking settings: what triggers it, how it reacts, and
    /// a table of which channels are lowered in which mixes.
    fn ducking_settings(&mut self, ui: &mut Ui) {
        ui.label(
            RichText::new(
                "While a trigger channel (your microphone, say) is loud, the channels ticked below are \
                 lowered in those mixes, so people hear you over the music or game.",
            )
            .small()
            .color(p().text_dim),
        );
        ui.add_space(6.0);
        let channels: Vec<(String, String)> =
            self.cfg.channels.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
        ui.label(RichText::new("Triggered by").color(p().text));
        ui.label(RichText::new("Channels whose sound ducks the others.").small().color(p().text_dim));
        ui.horizontal_wrapped(|ui| {
            for (id, name) in &channels {
                let mut on = self.cfg.ducking.keys.contains(id);
                if ui.checkbox(&mut on, name).changed() {
                    if on {
                        self.cfg.ducking.keys.insert(id.clone());
                    } else {
                        self.cfg.ducking.keys.remove(id);
                    }
                }
            }
        });
        row_divider(ui);
        let d = &mut self.cfg.ducking;
        settings_row(ui, "Sensitivity", "Higher reacts to quieter sound.", |ui| {
            for step in STEPS.iter().rev() {
                ui.selectable_value(&mut d.sensitivity, *step, step.label());
            }
        });
        row_divider(ui);
        settings_row(ui, "Amount", "How far ducked channels go down.", |ui| {
            for step in STEPS.iter().rev() {
                let label = format!("{} ({:.0} dB)", step.label(), step.reduction_db());
                ui.selectable_value(&mut d.amount, *step, label);
            }
        });
        row_divider(ui);
        settings_row(ui, "Speed", "How quickly ducking starts and stops.", |ui| {
            for speed in SPEEDS.iter().rev() {
                ui.selectable_value(&mut d.speed, *speed, speed.label());
            }
        });
        row_divider(ui);
        ui.add_space(4.0);
        ui.label(RichText::new("Duck these channels").color(p().text));
        ui.add_space(4.0);
        let mixes: Vec<(String, String)> = self.cfg.mixes.iter().map(|m| (m.id.clone(), m.name.clone())).collect();
        egui::Grid::new("ducking_table").striped(true).spacing(vec2(24.0, 6.0)).show(ui, |ui| {
            ui.label("");
            for (_, name) in &mixes {
                ui.label(RichText::new(name).color(p().text_dim));
            }
            ui.end_row();
            for (id, name) in &channels {
                ui.label(name);
                let is_key = self.cfg.ducking.keys.contains(id);
                for (mix, _) in &mixes {
                    let mut on = self.cfg.ducking.ducks(id, mix);
                    let resp = ui.add_enabled(!is_key, egui::Checkbox::without_text(&mut on));
                    let resp = if is_key { resp.on_disabled_hover_text("Triggers ducking") } else { resp };
                    if resp.changed() {
                        self.cfg.ducking.set(id, mix, on);
                    }
                }
                ui.end_row();
            }
        });
    }

    // ── Updates ────────────────────────────────────────────────────────────

    /// The pill in the lower-right corner: update status and action.
    fn update_button(&mut self, ctx: &egui::Context) {
        let status = self.updater.status();
        // Like Wave Link, stay out of sight unless there's something to do
        // (Settings → Updates always shows the status).
        if matches!(status, Status::Checking | Status::UpToDate) {
            return;
        }
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
                let label = RichText::new(text).color(if accent { p().bg } else { p().text });
                let button = egui::Button::new(label)
                    .fill(if accent { p().accent } else { p().card })
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
            ui.label(RichText::new(title).size(22.0).strong().color(p().text));
            ui.add_space(10.0);
            ScrollArea::vertical().id_salt("whats_new").max_height(480.0).auto_shrink([false, true]).show(ui, |ui| {
                let several = self.whats_new.len() > 1;
                for section in &self.whats_new {
                    if several {
                        ui.label(
                            RichText::new(format!("Version {}", section.version)).size(18.0).strong().color(p().text),
                        );
                    }
                    if !section.date.is_empty() {
                        ui.label(RichText::new(format!("Released {}", section.date)).small().color(p().text_dim));
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

    // ── Compact view ───────────────────────────────────────────────────────

    /// Resizes the window when the mode changes (also through the API).
    fn size_for_mode(&mut self, ctx: &egui::Context) {
        let mode = self.cfg.window_mode;
        if self.shown_mode == Some(mode) {
            return;
        }
        let first = self.shown_mode.is_none();
        self.shown_mode = Some(mode);
        match mode {
            WindowMode::Compact => {
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(360.0, 240.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(COMPACT_W, 560.0)));
            }
            WindowMode::Full => {
                if !first {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(FULL_SIZE.into()));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(FULL_MIN_SIZE.into()));
            }
        }
    }

    /// Wave Link's compact window: one mix, with a fader per channel.
    fn compact_page(&mut self, ui: &mut Ui) {
        let mix_id = self
            .cfg
            .compact_mix
            .clone()
            .filter(|m| self.cfg.mix(m).is_some())
            .or_else(|| self.cfg.mixes.first().map(|m| m.id.clone()));
        let Some(mix_id) = mix_id else { return };
        ui.horizontal(|ui| {
            let current = self.cfg.mix(&mix_id).map(|m| format!("{}  {}", m.icon, m.name)).unwrap_or_default();
            ComboBox::from_id_salt("compact_mix").width(220.0).selected_text(current).show_ui(ui, |ui| {
                for m in &self.cfg.mixes {
                    let label = format!("{}  {}", m.icon, m.name);
                    if ui.selectable_label(m.id == mix_id, label).clicked() {
                        self.cfg.compact_mix = Some(m.id.clone());
                    }
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("⛶  Full view").clicked() {
                    self.cfg.window_mode = WindowMode::Full;
                }
            });
        });
        ui.add_space(10.0);

        let meter = self.meters.level(&mix_capture(&mix_id));
        if let Some(mix) = self.cfg.mixes.iter_mut().find(|m| m.id == mix_id) {
            let (icon, name) = (mix.icon.clone(), "Mix volume".to_string());
            let gain = levels::amplitude(levels::fader_db(mix.level.volume));
            compact_row(ui, &icon, &name, Some(&mut mix.level), meter * gain);
        }
        ui.add_space(4.0);
        divider(ui);
        ui.add_space(4.0);

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = GAP;
            for ch in self.cfg.channels.iter_mut().filter(|c| !c.hidden) {
                let level = self.meters.level(&channel_capture(ch));
                let ch_copy = ch.clone();
                let (icon, name) = (ch.icon.clone(), ch.name.clone());
                match ch.sends.get_mut(&mix_id) {
                    Some(send) => {
                        let db = send_db(&ch_copy, send, 0.0);
                        let shown = if send_muted(&ch_copy, send) { 0.0 } else { level * levels::amplitude(db) };
                        compact_row(ui, &icon, &name, Some(send), shown);
                    }
                    None => {
                        if compact_row(ui, &icon, &name, None, 0.0) {
                            ch.sends.insert(mix_id.clone(), Level::full());
                        }
                    }
                }
            }
        });
    }

    // ── Dialogs ────────────────────────────────────────────────────────────

    fn dialogs(&mut self, ctx: &egui::Context, snap: &Snapshot) {
        let Some(mut dialog) = self.dialog.take() else { return };
        let mut keep = true;
        let modal = egui::Modal::new(Id::unique("linuxlink_dialog")).show(ctx, |ui| {
            ui.set_width(440.0);
            keep = match &mut dialog {
                Dialog::CreateChannel { query } => self.create_channel_dialog(ui, snap, query),
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
        ui.label(RichText::new("Name").color(p().text_dim));
        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Icon").color(p().text_dim));
        icon_picker(ui, CHANNEL_ICONS, &mut self.cfg.channels[idx].icon);
        ui.add_space(8.0);

        let ch = &mut self.cfg.channels[idx];
        ui.horizontal(|ui| {
            ui.checkbox(&mut ch.boost.enabled, "Boost")
                .on_hover_text("Raises this channel in every mix, on top of its faders. For quiet apps or mics.");
            ui.add_enabled(
                ch.boost.enabled,
                egui::Slider::new(&mut ch.boost.db, 0.0..=levels::MAX_BOOST_DB).step_by(0.5).suffix(" dB"),
            );
        });
        ui.checkbox(&mut ch.hidden, "Hide from the Mixes page").on_hover_text("Its audio keeps playing.");
        ui.add_space(8.0);

        if let ChannelKind::Input { source } = &mut self.cfg.channels[idx].kind {
            ui.label(RichText::new("Input device").color(p().text_dim));
            device_combo(ui, "ch_source", source, &snap.inputs);
        } else {
            ui.label(RichText::new("Apps in this channel").color(p().text_dim));
            self.app_assignments(ui, snap, id, query);
        }

        ui.add_space(12.0);
        let mut keep = true;
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Delete channel").color(p().mute_red)).clicked() {
                self.cfg.channels.remove(idx);
                self.cfg.app_rules.retain(|_, c| c != id);
                self.cfg.ducking.keys.remove(id);
                self.cfg.ducking.ducked.remove(id);
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

        // Apps seen playing audio, plus installed apps matching the search
        // (or, with no search yet, the usual suspects for a channel).
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
        {
            let installed = self.installed.get_or_insert_with(apps::installed);
            let wanted = |a: &InstalledApp| if q.is_empty() { suggested(&a.name) } else { matches(&a.name, &a.key) };
            for app in installed.iter().filter(|a| wanted(a)) {
                if !rows.iter().any(|(k, _)| k.eq_ignore_ascii_case(&app.key)) {
                    rows.push((app.key.clone(), KnownApp { name: app.name.clone(), icon: app.icon.clone() }));
                }
            }
        }
        // This channel's apps first, then Wave Link's ranking: playing now,
        // played before, suggested, the rest; alphabetical within each.
        let playing = |key: &str| {
            let lower = key.to_lowercase();
            snap.apps.iter().any(|a| a.key == key || a.ids.contains(&lower))
        };
        let rank = |key: &str, app: &KnownApp| -> u32 {
            if playing(key) {
                100
            } else if self.cfg.known_apps.contains_key(key) {
                75
            } else if suggested(&app.name) {
                50
            } else {
                0
            }
        };
        rows.sort_by_key(|(k, a)| {
            (self.cfg.app_rules.get(k).is_none_or(|c| c != id), std::cmp::Reverse(rank(k, a)), a.name.to_lowercase())
        });
        rows.truncate(60);

        if rows.is_empty() {
            let msg = if q.is_empty() {
                "Search for an application, or start playing audio in it."
            } else {
                "No applications match your search."
            };
            ui.label(RichText::new(msg).italics().color(p().text_dim));
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
                            ui.painter().rect_filled(rect, 4.0, p().card_hover);
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
                        ui.label(RichText::new("● playing").small().color(p().meter_low));
                    }
                    let elsewhere = self.cfg.app_rules.get(&key).filter(|c| *c != id).and_then(|c| self.cfg.channel(c));
                    if let Some(other) = elsewhere {
                        ui.label(RichText::new(format!("in {}", other.name)).small().color(p().text_dim));
                    }
                });
            }
        });
    }

    fn mix_dialog(&mut self, ui: &mut Ui, snap: &Snapshot, id: &str, name: &mut String) -> bool {
        let Some(idx) = self.cfg.mixes.iter().position(|m| m.id == id) else { return false };
        dialog_title(ui, "Mix");
        ui.label(RichText::new("Name").color(p().text_dim));
        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Icon").color(p().text_dim));
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

        ui.label(RichText::new("Extra outputs").color(p().text_dim));
        let listen_out = self.cfg.listen_output.clone();
        for d in snap.outputs.iter().filter(|d| Some(&d.name) != listen_out.as_ref()) {
            let mut on = self.cfg.mixes[idx].outputs.contains(&d.name);
            ui.horizontal(|ui| {
                if ui.checkbox(&mut on, &d.description).changed() {
                    self.assign_output(id, &d.name, on);
                }
                let other = self.cfg.mixes.iter().find(|m| m.id != id && m.outputs.contains(&d.name));
                if let Some(other) = other {
                    ui.label(RichText::new(format!("used by {}", other.name)).small().color(p().text_dim));
                }
            });
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Capture this mix in OBS or Discord as “LinuxLink {}”.", self.cfg.mixes[idx].name))
                .small()
                .color(p().text_dim),
        )
        .on_hover_text(mix_source(id));

        ui.add_space(12.0);
        let mut keep = true;
        ui.horizontal(|ui| {
            let can_delete = self.cfg.mixes.len() > 1;
            if ui.add_enabled(can_delete, egui::Button::new(RichText::new("Delete mix").color(p().mute_red))).clicked()
            {
                self.cfg.mixes.remove(idx);
                for ch in &mut self.cfg.channels {
                    ch.sends.remove(id);
                }
                self.cfg.listening.remove(id);
                for mixes in self.cfg.ducking.ducked.values_mut() {
                    mixes.remove(id);
                }
                self.cfg.ducking.ducked.retain(|_, m| !m.is_empty());
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

    /// Adds a channel to every mix; returns its id. Inputs start muted in
    /// the first mix (usually your own) so you don't hear yourself.
    fn add_channel(&mut self, name: &str, icon: &str, kind: ChannelKind) -> String {
        let is_input = matches!(kind, ChannelKind::Input { .. });
        let sends = self
            .cfg
            .mixes
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.clone(), Level::new(100, is_input && i == 0)))
            .collect();
        let id = self.cfg.unique_id(name);
        self.cfg.channels.push(ChannelCfg {
            id: id.clone(),
            name: name.to_string(),
            icon: icon.to_string(),
            kind,
            level: Level::full(),
            sends,
            boost: Boost::default(),
            hidden: false,
        });
        id
    }

    /// Wave Link's "Create channel": search your inputs and apps; picking
    /// one makes a channel for it. Or start from an empty channel.
    fn create_channel_dialog(&mut self, ui: &mut Ui, snap: &Snapshot, query: &mut String) -> bool {
        dialog_title(ui, "Create channel");
        let search = ui.add(egui::TextEdit::singleline(query).hint_text("🔍  Search").desired_width(f32::INFINITY));
        if ui.memory(|m| m.focused().is_none()) {
            search.request_focus();
        }
        ui.add_space(6.0);
        let q = query.trim().to_lowercase();
        let matches = |name: &str| q.is_empty() || name.to_lowercase().contains(&q);

        let used: Vec<&str> = self
            .cfg
            .channels
            .iter()
            .filter_map(|c| match &c.kind {
                ChannelKind::Input { source } => Some(source.as_str()),
                ChannelKind::Virtual => None,
            })
            .collect();
        let inputs: Vec<(String, String)> = snap
            .inputs
            .iter()
            .filter(|d| !used.contains(&d.name.as_str()) && matches(&d.description))
            .map(|d| (d.name.clone(), d.description.clone()))
            .collect();

        // Apps: playing now, then played before, then suggested (Wave Link's order).
        let mut apps: Vec<(String, KnownApp, u32)> = Vec::new();
        let playing = |key: &str| {
            let lower = key.to_lowercase();
            snap.apps.iter().any(|a| a.key == key || a.ids.contains(&lower))
        };
        for (key, app) in &self.cfg.known_apps {
            if matches(&app.name) {
                apps.push((key.clone(), app.clone(), if playing(key) { 100 } else { 75 }));
            }
        }
        let installed = self.installed.get_or_insert_with(apps::installed);
        for app in installed.iter() {
            let wanted = if q.is_empty() { suggested(&app.name) } else { matches(&app.name) };
            if wanted && !apps.iter().any(|(k, _, _)| k.eq_ignore_ascii_case(&app.key)) {
                let rank = if suggested(&app.name) { 50 } else { 0 };
                apps.push((app.key.clone(), KnownApp { name: app.name.clone(), icon: app.icon.clone() }, rank));
            }
        }
        apps.sort_by_key(|(_, a, rank)| (std::cmp::Reverse(*rank), a.name.to_lowercase()));
        apps.truncate(60);

        let mut keep = true;
        ScrollArea::vertical().max_height(330.0).auto_shrink([false, true]).show(ui, |ui| {
            for (name, description) in &inputs {
                if picker_row(ui, None, "🎤", description, "").clicked() {
                    self.add_channel(description, "🎤", ChannelKind::Input { source: name.clone() });
                    keep = false;
                }
            }
            for (key, app, _) in &apps {
                let uri = app.icon.as_deref().or(Some(key)).and_then(|n| self.icons.uri(n));
                let note =
                    self.cfg.app_rules.get(key).and_then(|c| self.cfg.channel(c)).map(|c| format!("in {}", c.name));
                if picker_row(ui, uri.as_deref(), "▣", &app.name, note.as_deref().unwrap_or("")).clicked() {
                    let id = self.add_channel(&app.name, CHANNEL_ICONS[5], ChannelKind::Virtual);
                    self.cfg.app_rules.insert(key.clone(), id);
                    self.cfg.known_apps.entry(key.clone()).or_insert_with(|| app.clone());
                    keep = false;
                }
            }
            if inputs.is_empty() && apps.is_empty() {
                ui.label(RichText::new("Nothing matches your search.").italics().color(p().text_dim));
            }
        });
        ui.add_space(4.0);
        divider(ui);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new("Add empty channel  ›").frame(false)).clicked() {
                self.dialog = Some(Dialog::NewChannel {
                    name: query.trim().to_string(),
                    icon: CHANNEL_ICONS[0].into(),
                    source: None,
                });
                keep = false;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
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
        ui.label(RichText::new("Name").color(p().text_dim));
        ui.add(egui::TextEdit::singleline(name).hint_text("e.g. Stream Alerts").desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Type").color(p().text_dim));
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
        ui.label(RichText::new("Icon").color(p().text_dim));
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
                    self.add_channel(name.trim(), icon, kind);
                    self.page = Page::Mixes;
                    keep = false;
                }
            });
        });
        keep
    }

    fn new_mix_dialog(&mut self, ui: &mut Ui, name: &mut String, icon: &mut String) -> bool {
        dialog_title(ui, "Add mix");
        ui.label(RichText::new("Name").color(p().text_dim));
        ui.add(egui::TextEdit::singleline(name).hint_text("e.g. Recording").desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Icon").color(p().text_dim));
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
        let light = match self.cfg.theme {
            Theme::Light => true,
            Theme::Dark => false,
            Theme::System => ctx.system_theme() == Some(egui::Theme::Light),
        };
        if self.shown_light != Some(light) {
            self.shown_light = Some(light);
            apply_theme(&ctx, light);
        }
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
        // Settings changed through the control API.
        let revision = crate::api::revision();
        if revision != self.seen_api_revision {
            self.seen_api_revision = revision;
            self.cfg = self.shared.lock().unwrap().clone();
        }
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
        self.size_for_mode(&ctx);
        if self.cfg.window_mode == WindowMode::Compact {
            egui::CentralPanel::default()
                .frame(Frame::new().fill(p().bg).inner_margin(Margin::same(14)))
                .show(ui, |ui| self.compact_page(ui));
            self.dialogs(&ctx, &view);
            if self.cfg != before {
                self.engine.send(Command::Apply(Box::new(self.cfg.clone()), self.seen_listen_switches));
                *self.shared.lock().unwrap() = self.cfg.clone();
            }
            ctx.request_repaint_after(Duration::from_millis(33));
            return;
        }
        egui::Panel::left("sidebar")
            .exact_size(SIDEBAR_W)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(p().sidebar).inner_margin(Margin::same(12)))
            .show(ui, |ui| self.sidebar(ui, &view));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p().bg).inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 12 }))
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
            self.engine.send(Command::Apply(Box::new(self.cfg.clone()), self.seen_listen_switches));
            *self.shared.lock().unwrap() = self.cfg.clone();
        }
        #[cfg(debug_assertions)]
        self.debug_screenshot(&ctx);
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

#[cfg(debug_assertions)]
impl LinuxLinkApp {
    /// Debug builds only: `LINUXLINK_SCREENSHOT=out.png` saves the window
    /// after a couple of seconds and exits, for checking the UI in a
    /// headless session. `LINUXLINK_SCREENSHOT_PAGE` picks the page
    /// (`settings`, `ducking`, `device`, `create`).
    fn debug_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("LINUXLINK_SCREENSHOT") else { return };
        self.frames += 1;
        if self.frames == 2 {
            match std::env::var("LINUXLINK_SCREENSHOT_PAGE").as_deref() {
                Ok("settings") => self.page = Page::Settings,
                Ok("ducking") => {
                    self.page = Page::Settings;
                    self.settings_tab = SettingsTab::Ducking;
                }
                Ok("device") => {
                    if let Some(d) = self.engine.snapshot.lock().unwrap().inputs.first() {
                        self.page = Page::Device(d.name.clone());
                    }
                }
                Ok("create") => self.dialog = Some(Dialog::CreateChannel { query: String::new() }),
                _ => {}
            }
        }
        if self.frames == 60 {
            let ctx2 = ctx.clone();
            let path = std::path::PathBuf::from(path);
            let save = egui::ScreenshotCallback::new(move |img| {
                let [w, h] = img.size;
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
                    let _ = buf.save(&path);
                }
                tray::quit();
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx2.request_repaint();
            });
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(save));
        }
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
    ui.label(RichText::new(text).size(13.5).color(p().text_dim));
    ui.add_space(4.0);
}

fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p().divider);
}

/// A semibold font, for names and headings (Wave Link's weight).
fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

/// Uses Noto Sans when installed (close to Wave Link's Segoe UI), with a
/// semibold face for names, and egui's own fonts for everything else.
fn install_fonts(ctx: &egui::Context) {
    const DIRS: &[&str] = &[
        "/usr/share/fonts/noto",
        "/usr/share/fonts/truetype/noto",
        "/usr/share/fonts/google-noto",
        "/usr/share/fonts/google-noto-vf",
        "/usr/share/fonts/TTF",
        "/run/host/fonts/noto",
    ];
    let read = |names: &[&str]| {
        DIRS.iter().flat_map(|d| names.iter().map(move |n| format!("{d}/{n}"))).find_map(|p| std::fs::read(p).ok())
    };
    let mut fonts = egui::FontDefinitions::default();
    if let Some(regular) = read(&["NotoSans-Regular.ttf"]) {
        fonts.font_data.insert("ui".into(), Arc::new(egui::FontData::from_owned(regular)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    let mut stack = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some(bold) = read(&["NotoSans-SemiBold.ttf", "NotoSans-Medium.ttf", "NotoSans-Bold.ttf"]) {
        fonts.font_data.insert("ui-semibold".into(), Arc::new(egui::FontData::from_owned(bold)));
        stack.insert(0, "ui-semibold".into());
    }
    fonts.families.insert(FontFamily::Name("semibold".into()), stack);
    ctx.set_fonts(fonts);
}

/// A Wave Link group heading in settings pages.
fn group_heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(semibold(19.0)).color(p().text));
    ui.add_space(6.0);
}

/// A rounded panel of a fixed size (the device page's two halves).
fn panel(ui: &mut Ui, size: egui::Vec2, add: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    paint_card(ui.painter(), rect, p().card);
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink(18.0)).layout(Layout::top_down(Align::Min)));
    add(&mut child);
}

/// A tab with Wave Link's short underline when selected.
fn tab_button(ui: &mut Ui, label: &str, selected: bool) -> bool {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(14.5), p().text);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(24.0, 18.0), Sense::click());
    if selected || resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, if selected { p().nav_selected } else { p().nav_hover });
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p().text);
    if selected {
        let bar = Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 2.0), vec2(16.0, 3.0));
        ui.painter().rect_filled(bar, 1.5, p().accent);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// A 32 px square icon button with an outline (Wave Link's top bar).
fn square_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let fill = if resp.hovered() { p().card_hover } else { p().card };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, p().button), StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(15.0), p().text);
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One line in a pick list: an icon (image or glyph), a name and a note.
fn picker_row(ui: &mut Ui, image: Option<&str>, glyph: &str, name: &str, note: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, p().nav_hover);
    }
    let icon = Rect::from_min_size(rect.left_center() + vec2(8.0, -10.0), vec2(20.0, 20.0));
    match image {
        Some(uri) => {
            egui::Image::new(uri).paint_at(ui, icon);
        }
        None => {
            ui.painter().text(icon.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(15.0), p().text);
        }
    }
    let painter = ui.painter();
    painter.text(rect.left_center() + vec2(38.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(14.5), p().text);
    if !note.is_empty() {
        painter.text(
            rect.right_center() - vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            note,
            FontId::proportional(12.0),
            p().text_dim,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn nav_item(ui: &mut Ui, icon: &str, label: &str, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let painter = ui.painter_at(rect);
    let fill = if selected {
        p().nav_selected
    } else if resp.hovered() {
        p().nav_hover
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 4.0, fill);
    if selected {
        let bar = Rect::from_min_size(rect.left_top() + vec2(0.0, 11.0), vec2(3.0, 16.0));
        painter.rect_filled(bar, 1.5, p().accent);
    }
    painter.text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), p().text);
    let galley = painter.layout(label.to_string(), FontId::proportional(14.5), p().text, rect.width() - 50.0);
    painter.galley(pos2(rect.left() + 42.0, rect.center().y - galley.size().y / 2.0), galley, p().text);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// A card's fill, with the thin outline the light theme gives cards.
fn paint_card(painter: &egui::Painter, rect: Rect, fill: Color32) {
    painter.rect_filled(rect, RADIUS, fill);
    if !p().dark {
        painter.rect_stroke(rect, RADIUS, Stroke::new(1.0, p().border), StrokeKind::Inside);
    }
}

fn card(ui: &mut Ui, size: egui::Vec2, fill: Color32, sense: Sense) -> (Rect, Response) {
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    paint_card(ui.painter(), rect, fill);
    (rect, resp)
}

fn add_card(ui: &mut Ui, size: egui::Vec2, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_card(ui.painter(), rect, if resp.hovered() { p().card_hover } else { p().card_empty });
    let text = if size.x < 80.0 { "+".to_string() } else { format!("+   {label}") };
    let color = if size.x < 80.0 { p().text } else { p().text_dim };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(15.0), color);
    resp.on_hover_text(label).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// dBFS-scaled 0..1 position for a linear peak, Wave Link's −60…0 dB.
fn meter_frac(level: f32) -> f32 {
    levels::meter_position(level)
}

/// A fader's tooltip: its gain in dB.
fn db_tip(volume: u32) -> String {
    levels::format_db(levels::fader_db(volume))
}

/// Moves `items[i]` one place towards `by` (−1 or +1), if there's room.
fn move_item<T>(items: &mut [T], i: usize, by: isize) {
    if let Some(j) = i.checked_add_signed(by).filter(|&j| j < items.len()) {
        items.swap(i, j);
    }
}

/// Apps most people put in a channel, offered before anyone searches.
fn suggested(name: &str) -> bool {
    const SUGGESTED: &[&str] = &[
        "spotify",
        "firefox",
        "chrome",
        "chromium",
        "brave",
        "vivaldi",
        "opera",
        "discord",
        "vesktop",
        "slack",
        "zoom",
        "teamspeak",
        "mumble",
        "element",
        "telegram",
        "obs",
        "steam",
        "vlc",
        "mpv",
        "rhythmbox",
        "elisa",
        "strawberry",
        "lollypop",
        "tidal",
        "deezer",
        "youtube music",
    ];
    let name = name.to_lowercase();
    SUGGESTED.iter().any(|s| name.contains(s))
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Where a meter turns yellow: above −6 dBFS, like Wave Link's hot zone.
const METER_HOT: f32 = 0.9;

/// Wave Link's meter: flat green, with a yellow tip in the hot zone.
fn paint_meter(painter: &egui::Painter, rect: Rect, frac: f32) {
    if frac <= 0.0 {
        return;
    }
    let x = |f: f32| rect.left() + rect.width() * f.clamp(0.0, 1.0);
    let r = (rect.height() / 2.0).min(2.5);
    let green = Rect::from_min_max(rect.min, pos2(x(frac.min(METER_HOT)), rect.bottom()));
    painter.rect_filled(green, r, p().meter_low);
    if frac > METER_HOT {
        let hot = Rect::from_min_max(pos2(x(METER_HOT), rect.top()), pos2(x(frac), rect.bottom()));
        painter.rect_filled(hot, r, p().meter_high);
    }
}

/// Volumes a dragged fader sticks to while it is within `SNAP_RANGE` of them.
const SNAP_POINTS: [u32; 3] = [25, 50, 75];
const SNAP_RANGE: u32 = 2;

fn snap_volume(v: u32) -> u32 {
    SNAP_POINTS.into_iter().find(|p| p.abs_diff(v) <= SNAP_RANGE).unwrap_or(v)
}

/// Wave Link's fader: a pill-shaped track with the level meter inside it, a
/// peak-hold tick, and a round knob for the volume.
fn meter_slider(ui: &mut Ui, value: &mut u32, level: f32, width: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click_and_drag());
    let r = 7.0;
    let (x0, x1) = (rect.left() + r, rect.right() - r);
    if resp.is_pointer_button_down_on() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let v = snap_volume((((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0) * 100.0).round() as u32);
            if v != *value {
                *value = v;
                resp.mark_changed();
            }
        }
    }
    let pal = p();
    let cy = rect.center().y;
    let track = Rect::from_min_max(pos2(rect.left(), cy - 4.5), pos2(rect.right(), cy + 4.5));
    let painter = ui.painter();
    painter.rect_filled(track, 4.5, pal.track);
    if !pal.dark {
        painter.rect_stroke(track, 4.5, Stroke::new(1.0, pal.tick), StrokeKind::Inside);
    }
    let inner = track.shrink(2.0);
    let frac = meter_frac(level);
    paint_meter(painter, inner, frac);

    // Peak hold: the highest level of the last 1.5 s.
    let id = resp.id.with("peak");
    let now = ui.input(|i| i.time);
    let (mut peak, mut since) = ui.data(|d| d.get_temp::<(f32, f64)>(id)).unwrap_or((0.0, 0.0));
    if frac >= peak || now - since > 1.5 {
        (peak, since) = (frac, now);
    }
    ui.data_mut(|d| d.insert_temp(id, (peak, since)));
    if peak > 0.02 {
        let x = inner.left() + inner.width() * peak;
        ui.painter().line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], Stroke::new(1.5, pal.tick));
    }

    let knob = pos2(x0 + (x1 - x0) * *value as f32 / 100.0, cy);
    let active = resp.hovered() || resp.dragged();
    ui.painter().circle_filled(knob, if active { 8.0 } else { 7.0 }, pal.knob);
    resp
}

fn speaker_button(ui: &mut Ui, muted: bool, volume: u32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    let painter = ui.painter();
    if muted {
        painter.rect_filled(rect, 5.0, p().mute_red);
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
        painter.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(15.0), p().text);
    }
    resp.on_hover_text(if muted { "Unmute" } else { "Mute" })
}

/// Mix column header, like Wave Link's: an icon tile, the name and how many
/// outputs play it, the listen button, and the mix's meter along the bottom.
/// Returns (header clicked, listen button clicked, moved left (−1) or right
/// (+1) from its menu).
fn mix_header(
    ui: &mut Ui,
    mix: &MixCfg,
    level: f32,
    listening: bool,
    can_left: bool,
    can_right: bool,
) -> (bool, bool, Option<isize>) {
    let (rect, resp) = ui.allocate_exact_size(vec2(MIX_W, HEADER_H), Sense::click());
    let btn = Rect::from_center_size(pos2(rect.right() - 22.0, rect.top() + 23.0), vec2(30.0, 30.0));
    let listen = ui.interact(btn, ui.scope_id().with(("listen", &mix.id)), Sense::click());
    let pal = p();
    let painter = ui.painter_at(rect);
    let hovered = resp.hovered() && !listen.hovered();
    paint_card(&painter, rect, if hovered { pal.card_hover } else { pal.card });

    let tile = Rect::from_min_size(rect.left_top() + vec2(9.0, 8.0), vec2(30.0, 30.0));
    painter.rect_filled(tile, 5.0, pal.tile);
    if !pal.dark {
        painter.rect_stroke(tile, 5.0, Stroke::new(1.0, pal.border), StrokeKind::Inside);
    }
    painter.text(tile.center(), Align2::CENTER_CENTER, &mix.icon, FontId::proportional(17.0), rgb(0x1b1b1b));

    let outputs = mix.outputs.len() + usize::from(listening);
    let sub = match outputs {
        0 => "Virtual mic".to_string(),
        1 => "1 output".to_string(),
        n => format!("{n} outputs"),
    };
    let name_width = btn.left() - tile.right() - 14.0;
    let name = painter.layout(mix.name.clone(), semibold(14.0), pal.text, name_width);
    painter.galley(pos2(tile.right() + 9.0, rect.top() + 7.0), name, pal.text);
    painter.text(
        pos2(tile.right() + 9.0, rect.top() + 26.0),
        Align2::LEFT_TOP,
        sub,
        FontId::proportional(12.5),
        pal.text_dim,
    );
    if mix.level.muted {
        painter.text(
            btn.left_center() - vec2(6.0, 0.0),
            Align2::RIGHT_CENTER,
            "🔇",
            FontId::proportional(13.0),
            pal.mute_red,
        );
    }

    if listening {
        painter.rect_filled(btn, 5.0, if listen.hovered() { pal.card_hover } else { pal.button });
        painter.text(btn.center(), Align2::CENTER_CENTER, "🎧", FontId::proportional(15.0), pal.knob);
    } else {
        if listen.hovered() {
            painter.rect_filled(btn, 5.0, pal.card_hover);
        }
        painter.text(btn.center(), Align2::CENTER_CENTER, "🎧", FontId::proportional(15.0), pal.text_dim);
        painter.line_segment(
            [btn.center() + vec2(-8.0, 8.0), btn.center() + vec2(8.0, -8.0)],
            Stroke::new(1.5, pal.text_dim),
        );
    }

    let bar =
        Rect::from_min_max(pos2(rect.left() + 1.0, rect.bottom() - 4.0), pos2(rect.right() - 1.0, rect.bottom() - 1.0));
    let gain = if mix.level.muted { 0.0 } else { levels::amplitude(levels::fader_db(mix.level.volume)) };
    paint_meter(&painter, bar, meter_frac(level * gain));

    let tip = if listening {
        "Playing to you. Click to stop hearing this mix."
    } else {
        "Not playing to you. Click to hear this mix."
    };
    let toggled = listen.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    let mut moved = None;
    resp.context_menu(|ui| {
        if ui.add_enabled(can_left, egui::Button::new("⏴  Move left")).clicked() {
            moved = Some(-1);
        }
        if ui.add_enabled(can_right, egui::Button::new("⏵  Move right")).clicked() {
            moved = Some(1);
        }
    });
    let clicked = resp.on_hover_text("Mix settings").on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    (clicked && !toggled, toggled, moved)
}

/// A mix's own volume, in the row under the mix headers.
fn mix_volume_cell(ui: &mut Ui, mix: &mut MixCfg, level: f32) {
    let (rect, _) = card(ui, vec2(MIX_W, 40.0), if mix.level.muted { p().muted_bg } else { p().card }, Sense::hover());
    let inner = Rect::from_center_size(rect.center(), vec2(150.0, 40.0));
    place(ui, inner, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if speaker_button(ui, mix.level.muted, mix.level.volume).clicked() {
            mix.level.muted = !mix.level.muted;
        }
        let gain = if mix.level.muted { 0.0 } else { levels::amplitude(levels::fader_db(mix.level.volume)) };
        let tip = db_tip(mix.level.volume);
        meter_slider(ui, &mut mix.level.volume, level * gain, 112.0)
            .on_hover_text(format!("{} volume: {tip}", mix.name));
    });
}

/// One line of the compact view: icon, name, mute and fader, or (with no
/// level) a button to add the channel to the mix. Returns that click.
fn compact_row(ui: &mut Ui, icon: &str, name: &str, level: Option<&mut Level>, meter: f32) -> bool {
    let fill = match &level {
        Some(l) if l.muted => p().muted_bg,
        Some(_) => p().card,
        None => p().card_empty,
    };
    let (rect, _) = card(ui, vec2(ui.available_width(), 48.0), fill, Sense::hover());
    let painter = ui.painter();
    painter.text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(18.0), p().text);
    let galley = painter.layout(name.to_string(), FontId::proportional(14.0), p().text, 120.0);
    painter.galley(pos2(rect.left() + 44.0, rect.center().y - galley.size().y / 2.0), galley, p().text);
    let controls = Rect::from_min_max(pos2(rect.left() + 170.0, rect.top()), pos2(rect.right() - 10.0, rect.bottom()));
    place(ui, controls, Layout::left_to_right(Align::Center), |ui| match level {
        Some(level) => {
            ui.spacing_mut().item_spacing.x = 6.0;
            if speaker_button(ui, level.muted, level.volume).clicked() {
                level.muted = !level.muted;
            }
            let shown = if level.muted { 0.0 } else { meter };
            let width = (ui.available_width() - 4.0).max(80.0);
            let tip = db_tip(level.volume);
            meter_slider(ui, &mut level.volume, shown, width).on_hover_text(tip);
            false
        }
        None => ui.button("+  Add to this mix").clicked(),
    })
}

fn paint_channel_icon(ui: &Ui, rect: Rect, ch: &ChannelCfg, icons: &[String]) {
    let painter = ui.painter();
    match icons.len() {
        0 => {
            painter.rect_filled(rect, 6.0, p().button);
            painter.text(rect.center(), Align2::CENTER_CENTER, &ch.icon, FontId::proportional(20.0), p().text);
        }
        1 => {
            egui::Image::new(icons[0].as_str()).corner_radius(6).paint_at(ui, rect);
        }
        _ => {
            let s = (rect.width() - 2.0) / 2.0;
            for (i, uri) in icons.iter().enumerate() {
                let off = vec2((i % 2) as f32 * (s + 2.0), (i / 2) as f32 * (s + 2.0));
                egui::Image::new(uri.as_str())
                    .corner_radius(3)
                    .paint_at(ui, Rect::from_min_size(rect.min + off, vec2(s, s)));
            }
        }
    }
}

/// What the row header asks for: its settings, or a row menu action.
#[derive(Clone, Copy, PartialEq)]
enum RowAction {
    Open,
    MoveUp,
    MoveDown,
    Hide,
}

/// Row header, like Wave Link's: icon, name, master mute and fader, and on
/// the right the channel's processing (boost) indicator. `movable` is (up, down).
fn channel_cell(
    ui: &mut Ui,
    ch: &ChannelCfg,
    master: &mut Level,
    level: f32,
    icons: &[String],
    movable: (bool, bool),
) -> Option<RowAction> {
    let (rect, _) = ui.allocate_exact_size(vec2(CH_W, ROW_H), Sense::hover());
    let info = Rect::from_min_size(rect.min, vec2(150.0, ROW_H));
    let open = ui.interact(info, ui.scope_id().with(("ch_open", &ch.id)), Sense::click());
    let fx = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(24.0, 24.0));
    let fx_resp = ui.interact(fx, ui.scope_id().with(("ch_fx", &ch.id)), Sense::click());
    paint_card(ui.painter(), rect, p().card);
    if open.hovered() {
        ui.painter().rect_filled(info, RADIUS, p().card_hover);
    }
    paint_channel_icon(ui, Rect::from_min_size(rect.left_top() + vec2(9.0, 7.0), vec2(40.0, 40.0)), ch, icons);
    let galley = ui.painter().layout(ch.name.clone(), semibold(14.0), p().text, 92.0);
    let pos = pos2(rect.left() + 57.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, p().text);

    // Wave Link shows its effects here; LinuxLink's processing is the boost.
    let fx_color = if ch.boost.enabled { p().meter_low } else { p().text_dim };
    ui.painter().text(fx.center(), Align2::CENTER_CENTER, "〰", FontId::proportional(15.0), fx_color);
    let fx_tip = if ch.boost.enabled {
        format!("Boost: +{:.1} dB. Click to change.", ch.boost.gain_db())
    } else {
        "No boost. Click to set one up.".to_string()
    };

    let controls = Rect::from_min_max(pos2(rect.left() + 152.0, rect.top()), pos2(fx.left() - 4.0, rect.bottom()));
    place(ui, controls, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if speaker_button(ui, master.muted, master.volume).clicked() {
            master.muted = !master.muted;
        }
        let shown = if master.muted { 0.0 } else { level };
        let tip = match ch.kind {
            ChannelKind::Virtual => db_tip(master.volume),
            ChannelKind::Input { .. } => format!("Device volume: {}%", master.volume),
        };
        let width = ui.available_width().max(60.0);
        meter_slider(ui, &mut master.volume, shown, width).on_hover_text(tip);
    });
    let mut action = None;
    open.context_menu(|ui| {
        if ui.add_enabled(movable.0, egui::Button::new("⏶  Move up")).clicked() {
            action = Some(RowAction::MoveUp);
        }
        if ui.add_enabled(movable.1, egui::Button::new("⏷  Move down")).clicked() {
            action = Some(RowAction::MoveDown);
        }
        if ui.button("👁  Hide channel").clicked() {
            action = Some(RowAction::Hide);
        }
    });
    if fx_resp.on_hover_text(fx_tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        action = Some(RowAction::Open);
    }
    if open.on_hover_text("Channel settings").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        action = Some(RowAction::Open);
    }
    action
}

/// What a matrix cell shows besides its fader.
#[derive(Clone, Copy)]
struct CellState {
    /// Voice ducking lowers this channel in this mix...
    ducked: bool,
    /// ...and is doing so right now.
    ducking_now: bool,
    /// An input in a mix you listen to: you'd hear yourself, slightly late.
    hear_yourself: bool,
}

/// One cell of the routing matrix: this channel's send into one mix.
fn send_cell(ui: &mut Ui, ch: &mut ChannelCfg, mix_id: &str, level: f32, cell: CellState) {
    let Some(send) = ch.sends.get(mix_id).cloned() else {
        let (rect, resp) = ui.allocate_exact_size(vec2(MIX_W, ROW_H), Sense::click());
        paint_card(ui.painter(), rect, p().card_empty);
        if resp.hovered() {
            ui.painter().circle_filled(rect.center(), 13.0, p().card_hover);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), p().text);
        }
        if resp.on_hover_text("Add to this mix").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            ch.sends.insert(mix_id.to_string(), Level::full());
        }
        return;
    };

    let (rect, resp) = ui.allocate_exact_size(vec2(MIX_W, ROW_H), Sense::click());
    paint_card(ui.painter(), rect, if send.muted { p().muted_bg } else { p().card });
    let mut new = send.clone();
    let inner = Rect::from_center_size(rect.center(), vec2(150.0, ROW_H));
    place(ui, inner, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if speaker_button(ui, new.muted, new.volume).clicked() {
            new.muted = !new.muted;
        }
        let shown = if send_muted(ch, &new) { 0.0 } else { level * levels::amplitude(send_db(ch, &new, 0.0)) };
        let tip = db_tip(new.volume);
        meter_slider(ui, &mut new.volume, shown, 112.0).on_hover_text(tip);
    });
    let badge = |ui: &mut Ui, at: egui::Pos2, text: &str, color: Color32, salt: &str, tip: &str| {
        let r = Rect::from_center_size(at, vec2(16.0, 16.0));
        ui.painter().text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(12.0), color);
        ui.interact(r, ui.scope_id().with((salt, &ch.id, mix_id)), Sense::hover()).on_hover_text(tip);
    };
    if cell.ducked {
        let color = if cell.ducking_now { p().accent } else { p().text_dim };
        badge(ui, rect.right_top() + vec2(-12.0, 11.0), "↓", color, "duck", "Lowered by voice ducking");
    }
    if cell.hear_yourself && !send.muted {
        badge(
            ui,
            rect.left_top() + vec2(12.0, 11.0),
            "⚠",
            p().meter_high,
            "monitor",
            "You listen to this mix, so you'll hear this input in your headphones, slightly delayed. \
             Mute it here if that echoes.",
        );
    }
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
    Frame::new().fill(p().muted_bg).corner_radius(6.0).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("⚠  {err}")).color(p().text));
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
    ui.label(RichText::new(title).size(20.0).strong().color(p().text));
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

fn settings_row(ui: &mut Ui, title: &str, help: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 320.0).max(200.0));
            ui.label(RichText::new(title).color(p().text));
            if !help.is_empty() {
                ui.label(RichText::new(help).small().color(p().text_dim));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
}

fn row_divider(ui: &mut Ui) {
    ui.add_space(6.0);
}

/// One device with an on/off switch; off hides it from LinuxLink.
fn device_toggle_row(
    ui: &mut Ui,
    d: &crate::pactl::Device,
    note: &str,
    hidden: &mut BTreeSet<String>,
    labels: &mut BTreeMap<String, String>,
) {
    // Remember the name of a turned-off device, to list it once it's gone.
    if hidden.contains(&d.name) && labels.get(&d.name) != Some(&d.description) {
        labels.insert(d.name.clone(), d.description.clone());
    }
    settings_row(ui, &d.description, note, |ui| {
        let mut on = !hidden.contains(&d.name);
        if toggle(ui, &mut on).changed() {
            if on {
                hidden.remove(&d.name);
                labels.remove(&d.name);
            } else {
                hidden.insert(d.name.clone());
                labels.insert(d.name.clone(), d.description.clone());
            }
        }
    });
}

/// Turned-off devices PipeWire doesn't list right now.
fn missing_devices(hidden: &BTreeSet<String>, present: &[crate::pactl::Device]) -> Vec<String> {
    hidden.iter().filter(|n| !present.iter().any(|d| &d.name == *n)).cloned().collect()
}

/// A turned-off device that's gone from PipeWire (turned off on the
/// computer, or not connected), so it can still be turned back on.
fn missing_device_row(
    ui: &mut Ui,
    name: &str,
    system_off: &BTreeSet<String>,
    hidden: &mut BTreeSet<String>,
    labels: &mut BTreeMap<String, String>,
) {
    let title = labels.get(name).cloned().unwrap_or_else(|| name.to_string());
    let note = if system_off.contains(name) { "Turned off on this computer" } else { "Not connected" };
    settings_row(ui, &title, note, |ui| {
        let mut on = false;
        if toggle(ui, &mut on).changed() {
            hidden.remove(name);
            labels.remove(name);
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
    painter.rect_filled(rect, 10.0, lerp_color(p().track, p().accent, t));
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
            ui.label(RichText::new(heading).size(16.0).strong().color(p().accent));
        } else if let Some(item) = block.strip_prefix("- ") {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new("•").color(p().accent));
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
        Device {
            index: 0,
            name: name.into(),
            description: name.into(),
            volume: None,
            volume_raw: None,
            muted: false,
            node_id: None,
        }
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
