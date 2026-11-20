//! The window: a device list on the left, and for the chosen device a "Keys" page (click a key,
//! pick what it does) and a "Lighting" page (effect, colour, speed, brightness).

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Pos2, Rect, RichText,
    Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::device::{DeviceKind, Driver, EffectSettings, KeySettings, LightingState, MouseState, MouseStatus};
use crate::keycodes::{
    KeyAction, MouseAction, KEY_GROUPS, MEDIA, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN,
};
use crate::{drivers, store};

const ACCENT: Color32 = Color32::from_rgb(46, 196, 214);
const BG: Color32 = Color32::from_rgb(20, 22, 27);
const PANEL: Color32 = Color32::from_rgb(28, 31, 38);
const CARD: Color32 = Color32::from_rgb(36, 40, 49);
const KEYCAP: Color32 = Color32::from_rgb(48, 53, 64);
const TEXT_DIM: Color32 = Color32::from_rgb(150, 158, 172);
const GOOD: Color32 = Color32::from_rgb(70, 200, 120);
const BAD: Color32 = Color32::from_rgb(230, 90, 90);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Keys,
    Lighting,
    Mouse,
}

/// The kinds of function offered in the key editor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Default,
    Key,
    Shortcut,
    Mouse,
    Media,
    Disabled,
}

impl Kind {
    const ALL: [(Kind, &'static str); 6] = [
        (Kind::Default, "Default"),
        (Kind::Key, "Another key"),
        (Kind::Shortcut, "Shortcut"),
        (Kind::Mouse, "Mouse"),
        (Kind::Media, "Media"),
        (Kind::Disabled, "Off"),
    ];
    fn of(a: KeyAction) -> Kind {
        match a {
            KeyAction::Default => Kind::Default,
            KeyAction::Key { mods: 0, .. } => Kind::Key,
            KeyAction::Key { .. } => Kind::Shortcut,
            KeyAction::Mouse(_) => Kind::Mouse,
            KeyAction::Media(_) => Kind::Media,
            KeyAction::Disabled => Kind::Disabled,
        }
    }
    fn starter(self) -> KeyAction {
        match self {
            Kind::Default => KeyAction::Default,
            Kind::Key => KeyAction::Key { mods: 0, usage: 0x04 },
            Kind::Shortcut => KeyAction::Key { mods: MOD_CTRL, usage: 0x06 },
            Kind::Mouse => KeyAction::Mouse(MouseAction::Left),
            Kind::Media => KeyAction::Media(0xCD),
            Kind::Disabled => KeyAction::Disabled,
        }
    }
}

enum Msg {
    Detected(usize, bool),
    Lighting(usize, Result<LightingState, String>),
    /// A mouse profile read; `true` when it replaces unsent edits (the person picked another profile).
    MouseRead(usize, Result<(MouseState, MouseStatus), String>, bool),
    Progress(String),
    Applied(usize, Result<(bool, bool, bool), String>),
}

struct DeviceEntry {
    driver: Arc<dyn Driver>,
    connected: Option<bool>,
    keys: KeySettings,
    lighting: LightingState,
    keys_dirty: bool,
    lighting_dirty: bool,
    mouse: Option<MouseState>,
    mouse_status: MouseStatus,
    mouse_dirty: bool,
    read_once: bool,
}

impl DeviceEntry {
    fn dirty(&self) -> bool {
        self.keys_dirty || self.lighting_dirty || self.mouse_dirty
    }
}

pub struct PeripheralsApp {
    devices: Vec<DeviceEntry>,
    selected: usize,
    tab: Tab,
    fn_layer: bool,
    selected_key: Option<u8>,
    /// DPI stage being edited on the mouse page.
    edit_stage: usize,
    busy: bool,
    status: String,
    status_ok: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    last_detect: Option<Instant>,
    detecting: bool,
    screenshot: Option<PathBuf>,
    frames: u32,
    screenshot_sent: bool,
}

impl PeripheralsApp {
    /// `page` (a hidden option for checks): "lighting", or "mouse" to open the first mouse.
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, page: Option<String>) -> Self {
        style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let devices: Vec<DeviceEntry> = drivers::all()
            .into_iter()
            .map(|driver| {
                let saved = store::load(driver.info().id);
                let lighting = saved.lighting.clone().unwrap_or_else(|| driver.default_lighting());
                let mouse = saved.mouse.clone().or_else(|| driver.default_mouse());
                DeviceEntry {
                    driver,
                    connected: None,
                    keys: saved.keys,
                    lighting,
                    keys_dirty: false,
                    lighting_dirty: false,
                    mouse,
                    mouse_status: MouseStatus::default(),
                    mouse_dirty: false,
                    read_once: false,
                }
            })
            .collect();
        let page = page.unwrap_or_default().to_ascii_lowercase();
        let (selected, tab) = match page.as_str() {
            "mouse" => (
                devices.iter().position(|d| d.driver.capabilities().mouse).unwrap_or(0),
                Tab::Mouse,
            ),
            "lighting" => (0, Tab::Lighting),
            _ => (0, Tab::Keys),
        };
        PeripheralsApp {
            devices,
            selected,
            tab,
            fn_layer: false,
            selected_key: Some(51),
            edit_stage: 1,
            busy: false,
            status: "Ready".into(),
            status_ok: true,
            tx,
            rx,
            last_detect: None,
            detecting: false,
            screenshot,
            frames: 0,
            screenshot_sent: false,
        }
    }

    fn screenshot_mode(&self) -> bool {
        self.screenshot.is_some()
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Detected(i, c) => {
                    self.detecting = false;
                    let d = &mut self.devices[i];
                    let was = d.connected;
                    d.connected = Some(c);
                    if c && !d.read_once && !self.screenshot.is_some() && !self.busy {
                        d.read_once = true;
                        let driver = d.driver.clone();
                        let caps = driver.capabilities();
                        let tx = self.tx.clone();
                        let ctx = ctx.clone();
                        std::thread::spawn(move || {
                            if caps.lighting {
                                let _ = tx.send(Msg::Lighting(i, driver.read_lighting()));
                            }
                            if caps.mouse {
                                let _ = tx.send(Msg::MouseRead(i, driver.read_mouse(None), false));
                            }
                            ctx.request_repaint();
                        });
                    }
                    if !c {
                        d.read_once = false;
                    }
                    let name = d.driver.info().name;
                    if was == Some(true) && !c {
                        self.set_status(format!("{name} was unplugged."), false);
                    }
                }
                Msg::Lighting(i, r) => match r {
                    Ok(l) => {
                        let d = &mut self.devices[i];
                        if !d.lighting_dirty {
                            d.lighting = l;
                        }
                        let name = d.driver.info().name;
                        self.set_status(format!("Read the current lighting from {name}."), true);
                    }
                    Err(e) => self.set_status(e, false),
                },
                Msg::MouseRead(i, r, replace) => match r {
                    Ok((m, s)) => {
                        let d = &mut self.devices[i];
                        d.mouse_status = s;
                        if replace || !d.mouse_dirty {
                            self.edit_stage = m.active_stage;
                            d.mouse = Some(m);
                        }
                        let name = d.driver.info().name;
                        self.set_status(format!("Read the current settings from {name}."), true);
                    }
                    Err(e) => self.set_status(e, false),
                },
                Msg::Progress(p) => self.status = p,
                Msg::Applied(i, r) => {
                    self.busy = false;
                    match r {
                        Ok((k, l, m)) => {
                            let d = &mut self.devices[i];
                            if k {
                                d.keys_dirty = false;
                            }
                            if l {
                                d.lighting_dirty = false;
                            }
                            if m {
                                d.mouse_dirty = false;
                            }
                            self.set_status("Settings saved to the device.".into(), true);
                        }
                        Err(e) => self.set_status(e, false),
                    }
                }
            }
        }

        let due = self.last_detect.map_or(true, |t| t.elapsed() > Duration::from_secs(3));
        if due && !self.detecting && !self.busy {
            self.detecting = true;
            self.last_detect = Some(Instant::now());
            for (i, d) in self.devices.iter().enumerate() {
                let driver = d.driver.clone();
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Detected(i, driver.detect()));
                    ctx.request_repaint();
                });
            }
        }
        ctx.request_repaint_after(Duration::from_secs(3));
    }

    fn set_status(&mut self, s: String, ok: bool) {
        self.status = s;
        self.status_ok = ok;
    }

    fn save(&self) {
        let d = &self.devices[self.selected];
        let saved = store::Saved {
            keys: d.keys.clone(),
            lighting: Some(d.lighting.clone()),
            mouse: d.mouse.clone(),
        };
        if !self.screenshot_mode() {
            let _ = store::save(d.driver.info().id, &saved);
        }
    }

    fn apply(&mut self, ctx: &egui::Context) {
        let i = self.selected;
        let d = &self.devices[i];
        let keys = d.keys_dirty.then(|| d.keys.clone());
        let lighting = d.lighting_dirty.then(|| d.lighting.clone());
        let mouse = if d.mouse_dirty { d.mouse.clone() } else { None };
        if keys.is_none() && lighting.is_none() && mouse.is_none() {
            return;
        }
        let driver = d.driver.clone();
        self.busy = true;
        self.set_status("Sending…".into(), true);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let report = |s: &str| {
                let _ = tx.send(Msg::Progress(s.to_string()));
                ctx.request_repaint();
            };
            let mut r = Ok(());
            if keys.is_some() || lighting.is_some() {
                r = driver.apply(keys.as_ref(), lighting.as_ref(), &report);
            }
            if let (Ok(()), Some(m)) = (&r, mouse.as_ref()) {
                r = driver.apply_mouse(m, &report);
            }
            let r = r.map(|_| (keys.is_some(), lighting.is_some(), mouse.is_some()));
            let _ = tx.send(Msg::Applied(i, r));
            ctx.request_repaint();
        });
    }

    // -----------------------------------------------------------------------------------------
    // Layout

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Peripherals").size(22.0).strong().color(Color32::WHITE));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Settings for your PC devices").color(TEXT_DIM));
        });
        ui.add_space(22.0);
        ui.label(RichText::new("DEVICES").size(11.0).color(TEXT_DIM));
        ui.add_space(6.0);
        for i in 0..self.devices.len() {
            let sel = i == self.selected;
            let d = &self.devices[i];
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), Sense::click());
            let fill = if sel {
                Color32::from_rgb(40, 52, 62)
            } else if resp.hovered() {
                CARD
            } else {
                Color32::TRANSPARENT
            };
            let p = ui.painter();
            p.rect_filled(rect, CornerRadius::same(10), fill);
            if sel {
                p.rect_filled(
                    Rect::from_min_size(rect.min + Vec2::new(0.0, 12.0), Vec2::new(3.0, rect.height() - 24.0)),
                    CornerRadius::same(2),
                    ACCENT,
                );
            }
            let icon_at = Pos2::new(rect.min.x + 30.0, rect.center().y);
            match d.driver.info().kind {
                DeviceKind::Mouse => draw_mouse_icon(p, Rect::from_center_size(icon_at, Vec2::new(18.0, 26.0))),
                _ => draw_keyboard_icon(p, Rect::from_center_size(icon_at, Vec2::new(30.0, 18.0))),
            }
            let info = d.driver.info();
            p.text(Pos2::new(rect.min.x + 56.0, rect.min.y + 19.0), Align2::LEFT_CENTER, info.name, FontId::proportional(15.0), Color32::WHITE);
            let (txt, col) = match d.connected {
                Some(true) => ("Connected", GOOD),
                Some(false) => ("Not connected", TEXT_DIM),
                None => ("Looking…", TEXT_DIM),
            };
            p.circle_filled(Pos2::new(rect.min.x + 60.0, rect.min.y + 39.0), 3.5, col);
            p.text(
                Pos2::new(rect.min.x + 69.0, rect.min.y + 39.0),
                Align2::LEFT_CENTER,
                format!("{} · {}", info.kind.name(), txt),
                FontId::proportional(12.0),
                TEXT_DIM,
            );
            if resp.clicked() {
                self.selected = i;
            }
            ui.add_space(4.0);
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(14.0);
            ui.label(
                RichText::new(format!("{} supported device{}", self.devices.len(), if self.devices.len() == 1 { "" } else { "s" }))
                    .size(11.0)
                    .color(TEXT_DIM),
            );
        });
    }

    fn header(&mut self, ui: &mut Ui) {
        let d = &self.devices[self.selected];
        let info = d.driver.info().clone();
        let caps = d.driver.capabilities();
        let connected = d.connected == Some(true);
        let dirty = d.dirty();
        let battery = if caps.mouse && connected {
            d.mouse_status.battery.map(|b| (b, d.mouse_status.charging, d.mouse_status.wired))
        } else {
            None
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(info.name).size(26.0).strong().color(Color32::WHITE));
                ui.label(RichText::new(format!("{} {} · USB {:04X}:{:04X}", info.maker, info.kind.name().to_lowercase(), info.vendor_id, info.product_id)).color(TEXT_DIM));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if self.busy { "Sending…" } else { "Apply to device" };
                let enabled = connected && dirty && !self.busy;
                let btn = egui::Button::new(RichText::new(label).size(15.0).strong().color(if enabled { Color32::BLACK } else { TEXT_DIM }))
                    .fill(if enabled { ACCENT } else { CARD })
                    .corner_radius(CornerRadius::same(10))
                    .min_size(Vec2::new(160.0, 40.0));
                let resp = ui.add_enabled(enabled, btn);
                if resp.clicked() {
                    self.apply(ui.ctx());
                }
                ui.add_space(10.0);
                if dirty {
                    pill(ui, "Unsent changes", Color32::from_rgb(240, 180, 60));
                } else if connected {
                    pill(ui, "Up to date", GOOD);
                }
                if !connected {
                    pill(ui, "Plug in the device to apply", TEXT_DIM);
                }
                if let Some((pct, charging, wired)) = battery {
                    let text = match (charging, wired) {
                        (true, _) => format!("Battery {pct}% · charging"),
                        (false, true) => format!("Battery {pct}% · cable"),
                        _ => format!("Battery {pct}%"),
                    };
                    pill(ui, &text, if pct <= 15 { BAD } else { ACCENT });
                }
            });
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if caps.key_remap && tab_button(ui, "Keys", self.tab == Tab::Keys) {
                self.tab = Tab::Keys;
            }
            if caps.lighting && tab_button(ui, "Lighting", self.tab == Tab::Lighting) {
                self.tab = Tab::Lighting;
            }
            if caps.mouse && tab_button(ui, "Mouse", self.tab == Tab::Mouse) {
                self.tab = Tab::Mouse;
            }
        });
        ui.add_space(12.0);
    }

    // -----------------------------------------------------------------------------------------
    // Keys page

    fn keys_page(&mut self, ui: &mut Ui) {
        let fn_cap = self.devices[self.selected].driver.capabilities().fn_layer;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Layer").color(TEXT_DIM));
            if segment(ui, "Normal", !self.fn_layer) {
                self.fn_layer = false;
            }
            if fn_cap && segment(ui, "Fn + key", self.fn_layer) {
                self.fn_layer = true;
            }
            ui.add_space(16.0);
            ui.label(
                RichText::new(if self.fn_layer {
                    "What a key does while Fn is held."
                } else {
                    "Click a key, then choose what it does."
                })
                .color(TEXT_DIM),
            );
        });
        ui.add_space(10.0);

        let avail = ui.available_width();
        let kb_h = (avail * 0.36).clamp(220.0, 330.0);
        Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(14)).show(ui, |ui| {
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, kb_h), Sense::hover());
            self.draw_keys(ui, rect);
        });
        ui.add_space(12.0);
        Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(16)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            self.key_editor(ui);
        });
    }

    fn draw_keys(&mut self, ui: &mut Ui, area: Rect) {
        let d = &self.devices[self.selected];
        let layout = d.driver.layout().clone();
        let map = if self.fn_layer { &d.keys.fn_layer } else { &d.keys.normal };
        let scale = (area.width() / layout.width).min(area.height() / layout.height);
        let size = Vec2::new(layout.width, layout.height) * scale;
        let origin = area.center() - size / 2.0;
        let mut clicked = None;
        for k in &layout.keys {
            let r = Rect::from_min_max(
                origin + Vec2::new(k.rect[0], k.rect[1]) * scale,
                origin + Vec2::new(k.rect[2], k.rect[3]) * scale,
            )
            .shrink(1.0);
            let id = ui.scope_id().with(("key", k.id));
            let resp = ui.interact(r, id, if k.locked { Sense::hover() } else { Sense::click() });
            let action = map.get(&k.id).copied().unwrap_or_default();
            let custom = action != KeyAction::Default;
            let selected = self.selected_key == Some(k.id);
            let mut fill = if k.locked { Color32::from_rgb(38, 42, 50) } else { KEYCAP };
            if custom {
                fill = Color32::from_rgb(28, 74, 84);
            }
            if resp.hovered() && !k.locked {
                fill = fill.gamma_multiply(1.25);
            }
            let p = ui.painter();
            p.rect_filled(r, CornerRadius::same(5), fill);
            if selected {
                p.rect_stroke(r, CornerRadius::same(5), Stroke::new(2.0, ACCENT), StrokeKind::Outside);
            } else if custom {
                p.rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, ACCENT.gamma_multiply(0.7)), StrokeKind::Inside);
            }
            let fs = (10.5 * scale).clamp(8.0, 13.0);
            if custom {
                p.text(r.center() - Vec2::new(0.0, fs * 0.55), Align2::CENTER_CENTER, k.label, FontId::proportional(fs * 0.85), TEXT_DIM);
                p.text(r.center() + Vec2::new(0.0, fs * 0.6), Align2::CENTER_CENTER, action.short(), FontId::proportional(fs * 0.8), ACCENT);
            } else {
                p.text(r.center(), Align2::CENTER_CENTER, k.label, FontId::proportional(fs), if k.locked { TEXT_DIM } else { Color32::WHITE });
            }
            if resp.clicked() {
                clicked = Some(k.id);
            }
        }
        if clicked.is_some() {
            self.selected_key = clicked;
        }
    }

    fn key_editor(&mut self, ui: &mut Ui) {
        let Some(key_id) = self.selected_key else {
            ui.label(RichText::new("Pick a key above to change it.").color(TEXT_DIM));
            return;
        };
        let fn_layer = self.fn_layer;
        let d = &mut self.devices[self.selected];
        let label = d.driver.layout().keys.iter().find(|k| k.id == key_id).map(|k| k.label).unwrap_or("?");
        let default_text = d.driver.default_description(key_id, fn_layer);
        let map = if fn_layer { &mut d.keys.fn_layer } else { &mut d.keys.normal };
        let mut action = map.get(&key_id).copied().unwrap_or_default();
        let before = action;
        let mut cleared = false;

        ui.horizontal(|ui| {
            let title = if fn_layer { format!("Fn + {label}") } else { format!("Key: {label}") };
            ui.label(RichText::new(title).size(18.0).strong().color(Color32::WHITE));
            ui.add_space(12.0);
            ui.label(RichText::new(format!("Default: {default_text}")).color(TEXT_DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Reset all keys").clicked() {
                    map.clear();
                    action = KeyAction::Default;
                    cleared = true;
                }
            });
        });
        ui.add_space(10.0);
        let kind = Kind::of(action);
        ui.horizontal_wrapped(|ui| {
            for (k, name) in Kind::ALL {
                if segment(ui, name, kind == k) && kind != k {
                    action = k.starter();
                }
            }
        });
        ui.add_space(10.0);
        match action {
            KeyAction::Default => {
                ui.label(RichText::new(format!("This key does what it normally does: {default_text}.")).color(TEXT_DIM));
            }
            KeyAction::Disabled => {
                ui.label(RichText::new("This key will do nothing.").color(TEXT_DIM));
            }
            KeyAction::Key { mods, usage } => {
                let mut mods = mods;
                let mut usage = usage;
                ui.horizontal(|ui| {
                    if kind == Kind::Shortcut {
                        for (bit, name) in [(MOD_CTRL, "Ctrl"), (MOD_SHIFT, "Shift"), (MOD_ALT, "Alt"), (MOD_WIN, "Win")] {
                            let mut on = mods & bit != 0;
                            if ui.checkbox(&mut on, name).changed() {
                                mods = if on { mods | bit } else { mods & !bit };
                            }
                        }
                        ui.label(RichText::new("+").strong());
                    }
                    key_picker(ui, &mut usage);
                });
                if kind == Kind::Shortcut && mods == 0 {
                    mods = MOD_CTRL;
                }
                action = KeyAction::Key { mods, usage };
            }
            KeyAction::Mouse(m) => {
                let mut m = m;
                ui.horizontal_wrapped(|ui| {
                    for opt in MouseAction::ALL {
                        if segment(ui, opt.name(), m == opt) {
                            m = opt;
                        }
                    }
                });
                action = KeyAction::Mouse(m);
            }
            KeyAction::Media(u) => {
                let mut u = u;
                ui.horizontal_wrapped(|ui| {
                    for (code, name) in MEDIA {
                        if segment(ui, name, u == *code) {
                            u = *code;
                        }
                    }
                });
                action = KeyAction::Media(u);
            }
        }

        let mut changed = cleared;
        if action != before && !cleared {
            if action == KeyAction::Default {
                map.remove(&key_id);
            } else {
                map.insert(key_id, action);
            }
            changed = true;
        }
        if changed {
            d.keys_dirty = true;
            self.save();
        }
    }

    // -----------------------------------------------------------------------------------------
    // Lighting page

    fn lighting_page(&mut self, ui: &mut Ui) {
        let d = &mut self.devices[self.selected];
        let driver = d.driver.clone();
        let effects = driver.effects();
        let mut changed = false;

        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.label(RichText::new("Effect").size(16.0).strong().color(Color32::WHITE));
            ui.add_space(8.0);
            let per_row = 3;
            let w = (ui.available_width() - 8.0 * (per_row as f32 - 1.0)) / per_row as f32;
            for row in effects.chunks(per_row).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (j, e) in row.1.iter().enumerate() {
                        let idx = row.0 * per_row + j;
                        let sel = d.lighting.effect == idx;
                        let btn = egui::Button::new(RichText::new(e.name).color(if sel { Color32::BLACK } else { Color32::WHITE }))
                            .fill(if sel { ACCENT } else { KEYCAP })
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(w, 36.0));
                        if ui.add(btn).clicked() && !sel {
                            d.lighting.effect = idx;
                            changed = true;
                        }
                    }
                });
                ui.add_space(6.0);
            }

            let ui = &mut cols[1];
            let idx = d.lighting.effect.min(effects.len() - 1);
            let info = effects[idx];
            ui.label(RichText::new(format!("{} settings", info.name)).size(16.0).strong().color(Color32::WHITE));
            ui.add_space(8.0);
            preview(ui, &d.lighting, &info, driver.as_ref());
            ui.add_space(12.0);
            if info.off {
                ui.label(RichText::new("The lights will be switched off.").color(TEXT_DIM));
            }
            if info.groups > 0 {
                ui.label(RichText::new("Uses the colour groups stored on the keyboard.").color(TEXT_DIM));
                ui.horizontal(|ui| {
                    for g in 0..info.groups {
                        if segment(ui, &format!("M{}", g + 1), d.lighting.group == g) && d.lighting.group != g {
                            d.lighting.group = g;
                            changed = true;
                        }
                    }
                });
            }
            if idx < d.lighting.per_effect.len() {
                let s: &mut EffectSettings = &mut d.lighting.per_effect[idx];
                if info.colour {
                    ui.label(RichText::new("Colour").color(TEXT_DIM));
                    ui.horizontal_wrapped(|ui| {
                        for (ci, c) in driver.colours().iter().enumerate() {
                            if swatch(ui, Color32::from_rgb(c.rgb[0], c.rgb[1], c.rgb[2]), s.colour == Some(ci as u8), c.name) {
                                s.colour = Some(ci as u8);
                                changed = true;
                            }
                        }
                        if info.colourful && rainbow_swatch(ui, s.colour.is_none()) {
                            s.colour = None;
                            changed = true;
                        }
                    });
                    ui.add_space(8.0);
                }
                if info.speed {
                    ui.label(RichText::new("Speed").color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        ui.label("Slow");
                        changed |= ui.add(egui::Slider::new(&mut s.speed, 1..=driver.max_speed()).show_value(false)).changed();
                        ui.label("Fast");
                    });
                    ui.add_space(8.0);
                }
                if info.brightness {
                    let labels = driver.brightness_labels();
                    ui.label(RichText::new("Brightness").color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::Slider::new(&mut s.brightness, 1..=driver.max_brightness()).show_value(false)).changed();
                        let l = labels.get(s.brightness as usize - 1).cloned().unwrap_or_default();
                        ui.label(l);
                    });
                    ui.add_space(8.0);
                }
                if info.direction {
                    ui.label(RichText::new("Direction").color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        if segment(ui, "→  Forward", s.direction == 0) && s.direction != 0 {
                            s.direction = 0;
                            changed = true;
                        }
                        if segment(ui, "←  Reverse", s.direction == 1) && s.direction != 1 {
                            s.direction = 1;
                            changed = true;
                        }
                    });
                }
            }
        });
        if changed {
            self.devices[self.selected].lighting_dirty = true;
            self.save();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Mouse page

impl PeripheralsApp {
    /// Keeps the open page one the selected device has.
    fn fix_tab(&mut self) {
        let caps = self.devices[self.selected].driver.capabilities();
        let ok = match self.tab {
            Tab::Keys => caps.key_remap,
            Tab::Lighting => caps.lighting,
            Tab::Mouse => caps.mouse,
        };
        if !ok {
            self.tab = if caps.key_remap {
                Tab::Keys
            } else if caps.lighting {
                Tab::Lighting
            } else {
                Tab::Mouse
            };
        }
    }

    fn mouse_page(&mut self, ui: &mut Ui) {
        let i = self.selected;
        let connected = self.devices[i].connected == Some(true);
        let driver = self.devices[i].driver.clone();
        let status = self.devices[i].mouse_status.clone();
        // Unplugged, show every rate the receiver allows; the cable only goes to 1 kHz.
        let Some(spec) = driver.mouse_spec(connected && status.wired) else { return };
        let Some(mut m) = self.devices[i].mouse.clone() else { return };
        let before = m.clone();
        let mut profile_pick = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new("Onboard profile").color(TEXT_DIM));
            for p in 1..=spec.profiles {
                if segment(ui, &format!("  {p}  "), m.profile == p) && m.profile != p {
                    profile_pick = Some(p);
                }
            }
            ui.add_space(16.0);
            let conn = if !connected {
                "Not connected: changes are kept until you plug it in"
            } else if status.wired {
                "Connected by cable"
            } else {
                "Connected through the receiver"
            };
            ui.label(RichText::new(conn).color(TEXT_DIM));
            if let Some(fw) = &status.firmware {
                ui.label(RichText::new(format!("· firmware {fw}")).color(TEXT_DIM));
            }
        });
        ui.add_space(10.0);

        // DPI stages
        let n = m.dpi_stages.len().max(1);
        self.edit_stage = self.edit_stage.min(n - 1);
        let colour_of = |s: usize| {
            let c = spec.stage_colours.get(s).copied().unwrap_or([255, 255, 255]);
            Color32::from_rgb(c[0], c[1], c[2])
        };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("DPI stages").size(16.0).strong().color(Color32::WHITE));
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Click a stage to edit it. The DPI button on the mouse steps through them.")
                        .color(TEXT_DIM),
                );
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for s in 0..m.dpi_stages.len() {
                    if stage_chip(ui, s, m.dpi_stages[s], colour_of(s), s == m.active_stage, s == self.edit_stage) {
                        self.edit_stage = s;
                    }
                }
                if m.dpi_stages.len() < spec.max_stages {
                    let add = egui::Button::new(RichText::new("+  Add stage").color(Color32::WHITE))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.0, TEXT_DIM.gamma_multiply(0.5)))
                        .corner_radius(CornerRadius::same(10))
                        .min_size(Vec2::new(110.0, 66.0));
                    if ui.add(add).clicked() {
                        let last = *m.dpi_stages.last().unwrap_or(&800);
                        let next = (last * 2).min(spec.dpi_max) / spec.dpi_step * spec.dpi_step;
                        m.dpi_stages.push(next.max(spec.dpi_min));
                        self.edit_stage = m.dpi_stages.len() - 1;
                    }
                }
            });
            ui.add_space(10.0);
            dpi_track(ui, &m.dpi_stages, spec.dpi_min, spec.dpi_max, self.edit_stage, &colour_of);
            ui.add_space(10.0);
            let e = self.edit_stage;
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 5.0, colour_of(e));
                ui.label(RichText::new(format!("Stage {}", e + 1)).strong().color(Color32::WHITE));
                let v = &mut m.dpi_stages[e];
                ui.add(
                    egui::Slider::new(v, spec.dpi_min..=spec.dpi_max)
                        .step_by(spec.dpi_step as f64)
                        .logarithmic(true)
                        .show_value(false),
                );
                ui.add(
                    egui::DragValue::new(v)
                        .range(spec.dpi_min..=spec.dpi_max)
                        .speed(spec.dpi_step as f64)
                        .suffix(" DPI"),
                );
                ui.add_space(10.0);
                if m.active_stage == e {
                    pill(ui, "In use now", ACCENT);
                } else if ui.button("Use this stage").clicked() {
                    m.active_stage = e;
                }
                if m.dpi_stages.len() > 1 && ui.button("Remove").clicked() {
                    m.dpi_stages.remove(e);
                    if m.active_stage >= m.dpi_stages.len() || m.active_stage > e {
                        m.active_stage = m.active_stage.saturating_sub(1).min(m.dpi_stages.len() - 1);
                    }
                    self.edit_stage = e.min(m.dpi_stages.len() - 1);
                }
            });
        });
        ui.add_space(12.0);

        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                ui.label(RichText::new("Performance").size(16.0).strong().color(Color32::WHITE));
                ui.add_space(6.0);
                ui.label(RichText::new("Polling rate").color(TEXT_DIM));
                ui.horizontal_wrapped(|ui| {
                    for &hz in &spec.poll_rates {
                        let label = if hz >= 1000 { format!("{} kHz", hz / 1000) } else { format!("{hz} Hz") };
                        if segment(ui, &label, m.poll_hz == hz) {
                            m.poll_hz = hz;
                        }
                    }
                });
                if connected && status.wired {
                    ui.label(RichText::new("Over the cable the mouse polls up to 1 kHz; the receiver allows up to 8 kHz.").size(12.0).color(TEXT_DIM));
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Lift-off distance").color(TEXT_DIM));
                ui.horizontal(|ui| {
                    for &(code, label) in &spec.lift_off {
                        if segment(ui, label, m.lift_off == code) {
                            m.lift_off = code;
                        }
                    }
                });
                ui.add_space(8.0);
                toggle(ui, &mut m.motion_sync, "Motion sync", "Lines sensor reads up with USB polls: smoother, a touch more delay.");
                toggle(ui, &mut m.ripple, "Ripple control", "Smooths sensor jitter at high DPI.");
                if spec.angle_snap {
                    toggle(ui, &mut m.angle_snap, "Angle snapping", "Straightens nearly-straight movements.");
                }
            });
            card(&mut cols[1], |ui| {
                ui.label(RichText::new("Buttons & power").size(16.0).strong().color(Color32::WHITE));
                ui.add_space(6.0);
                ui.label(RichText::new("Click debounce").color(TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut m.debounce_ms, 0..=spec.debounce_max).show_value(false));
                    ui.label(format!("{} ms", m.debounce_ms));
                });
                ui.label(RichText::new("Lower reacts faster; raise it if single clicks register twice.").size(12.0).color(TEXT_DIM));
                ui.add_space(8.0);
                ui.label(RichText::new("Sleep after").color(TEXT_DIM));
                ui.horizontal_wrapped(|ui| {
                    for &(secs, label) in &spec.sleep_options {
                        if segment(ui, label, m.sleep_secs == secs) {
                            m.sleep_secs = secs;
                        }
                    }
                });
                if !spec.sleep_options.iter().any(|(s, _)| *s == m.sleep_secs) {
                    ui.label(RichText::new(format!("Currently {} s (set elsewhere).", m.sleep_secs)).size(12.0).color(TEXT_DIM));
                }
            });
        });

        if let Some(p) = profile_pick {
            m.profile = p;
            if connected && !self.screenshot_mode() {
                let tx = self.tx.clone();
                let ctx = ui.ctx().clone();
                let drv = driver.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::MouseRead(i, drv.read_mouse(Some(p)), true));
                    ctx.request_repaint();
                });
            }
        }
        if m != before {
            let d = &mut self.devices[i];
            d.mouse = Some(m);
            d.mouse_dirty = true;
            self.save();
        }
    }
}

impl eframe::App for PeripheralsApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);

        egui::Panel::left("devices")
            .exact_size(240.0)
            .resizable(false)
            .frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(14, 0)))
            .show(ui, |ui| self.sidebar(ui));
        egui::Panel::bottom("status")
            .exact_size(30.0)
            .frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(16, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(if self.status_ok { GOOD } else { BAD }, "●");
                    ui.label(RichText::new(&self.status).color(TEXT_DIM));
                });
            });
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(26, 20)))
            .show(ui, |ui| {
                self.fix_tab();
                self.header(ui);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::Keys => self.keys_page(ui),
                    Tab::Lighting => self.lighting_page(ui),
                    Tab::Mouse => self.mouse_page(ui),
                });
            });

        if let Some(path) = self.screenshot.clone() {
            self.frames += 1;
            ctx.request_repaint();
            if self.frames >= 120 && !self.screenshot_sent {
                self.screenshot_sent = true;
                ctx.request_screenshot(move |image| {
                    let ok = save_png(&path, &image);
                    std::process::exit(if ok { 0 } else { 1 });
                });
            }
        }
    }
}

fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> bool {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let [w, h] = image.size;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for p in &image.pixels {
        bytes.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
    }
    image::save_buffer(path, &bytes, w as u32, h as u32, image::ColorType::Rgba8).is_ok()
}

// ---------------------------------------------------------------------------------------------
// Small drawing helpers

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(16, 18, 22);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::BLACK);
    v.widgets.inactive.weak_bg_fill = KEYCAP;
    v.widgets.inactive.bg_fill = KEYCAP;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(60, 66, 80);
    v.widgets.active.weak_bg_fill = ACCENT;
    v.slider_trailing_fill = true;
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.slider_width = 220.0;
    });
}

fn pill(ui: &mut Ui, text: &str, colour: Color32) {
    let font = FontId::proportional(12.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, colour);
    let size = galley.size() + Vec2::new(22.0, 10.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(12), colour.gamma_multiply(0.16));
    p.galley(rect.center() - galley.size() / 2.0, galley, colour);
}

fn tab_button(ui: &mut Ui, text: &str, on: bool) -> bool {
    let btn = egui::Button::new(RichText::new(text).size(15.0).color(if on { Color32::WHITE } else { TEXT_DIM }))
        .fill(if on { CARD } else { Color32::TRANSPARENT })
        .stroke(if on { Stroke::new(1.0, ACCENT.gamma_multiply(0.6)) } else { Stroke::NONE })
        .corner_radius(CornerRadius::same(9))
        .min_size(Vec2::new(110.0, 34.0));
    ui.add(btn).clicked()
}

fn segment(ui: &mut Ui, text: &str, on: bool) -> bool {
    let btn = egui::Button::new(RichText::new(text).color(if on { Color32::BLACK } else { Color32::WHITE }))
        .fill(if on { ACCENT } else { KEYCAP })
        .corner_radius(CornerRadius::same(8));
    ui.add(btn).clicked()
}

fn key_picker(ui: &mut Ui, usage: &mut u8) {
    egui::ComboBox::from_id_salt("key_picker")
        .selected_text(crate::keycodes::usage_name(*usage))
        .height(360.0)
        .width(200.0)
        .show_ui(ui, |ui| {
            for (group, keys) in KEY_GROUPS {
                ui.label(RichText::new(*group).size(11.0).color(TEXT_DIM));
                for (u, name) in *keys {
                    ui.selectable_value(usage, *u, *name);
                }
                ui.separator();
            }
        });
}

fn swatch(ui: &mut Ui, colour: Color32, on: bool, name: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let p = ui.painter();
    p.circle_filled(rect.center(), 12.0, colour);
    if on {
        p.circle_stroke(rect.center(), 14.5, Stroke::new(2.0, Color32::WHITE));
    } else if resp.hovered() {
        p.circle_stroke(rect.center(), 14.5, Stroke::new(1.0, TEXT_DIM));
    }
    resp.on_hover_text(name).clicked()
}

fn rainbow_swatch(ui: &mut Ui, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(96.0, 30.0), Sense::click());
    let p = ui.painter();
    let inner = Rect::from_center_size(rect.center(), Vec2::new(90.0, 24.0));
    let n = 18;
    for i in 0..n {
        let x0 = inner.min.x + inner.width() * i as f32 / n as f32;
        let x1 = inner.min.x + inner.width() * (i + 1) as f32 / n as f32 + 0.5;
        let c = hue(i as f32 / n as f32);
        p.rect_filled(Rect::from_min_max(Pos2::new(x0, inner.min.y), Pos2::new(x1, inner.max.y)), CornerRadius::ZERO, c);
    }
    p.text(inner.center(), Align2::CENTER_CENTER, "Colourful", FontId::proportional(12.0), Color32::BLACK);
    if on {
        p.rect_stroke(inner.expand(3.0), CornerRadius::same(6), Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
    }
    resp.on_hover_text("Rainbow colours").clicked()
}

fn hue(t: f32) -> Color32 {
    let h = (t.fract() * 6.0) as i32;
    let f = t.fract() * 6.0 - h as f32;
    let (r, g, b) = match h {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// A small keyboard drawing tinted the way the chosen lighting would look.
fn preview(ui: &mut Ui, l: &LightingState, info: &crate::device::EffectInfo, driver: &dyn Driver) {
    let w = ui.available_width().min(460.0);
    let layout = driver.layout();
    let scale = w / layout.width;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, layout.height * scale), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(14, 16, 20));
    let s = l.per_effect.get(l.effect).copied();
    let bright = s.map(|s| s.brightness as f32 / driver.max_brightness() as f32).unwrap_or(1.0);
    let t = ui.input(|i| i.time) as f32;
    for k in &layout.keys {
        let r = Rect::from_min_max(
            rect.min + Vec2::new(k.rect[0], k.rect[1]) * scale,
            rect.min + Vec2::new(k.rect[2], k.rect[3]) * scale,
        )
        .shrink(0.8);
        let x = k.rect[0] / layout.width;
        let colour = if info.off {
            Color32::from_rgb(40, 42, 48)
        } else if info.groups > 0 {
            hue(0.55 + 0.1 * l.group as f32 + x * 0.1)
        } else if info.colour && s.and_then(|s| s.colour).is_some() {
            let c = driver.colours()[s.unwrap().colour.unwrap() as usize].rgb;
            Color32::from_rgb(c[0], c[1], c[2])
        } else {
            let speed = s.map(|s| s.speed as f32).unwrap_or(3.0);
            hue(x + if info.speed { t * 0.04 * speed } else { 0.0 })
        };
        let c = if info.off { colour } else { colour.gamma_multiply(0.25 + 0.75 * bright) };
        p.rect_filled(r, CornerRadius::same(2), c);
    }
    if !info.off {
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }
}

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

/// One DPI stage card: colour strip, stage number, value, "in use" mark. Returns true when clicked.
fn stage_chip(ui: &mut Ui, index: usize, dpi: u32, colour: Color32, active: bool, editing: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(116.0, 66.0), Sense::click());
    let p = ui.painter();
    let fill = if editing { Color32::from_rgb(56, 62, 76) } else if resp.hovered() { Color32::from_rgb(52, 58, 70) } else { KEYCAP };
    p.rect_filled(rect, CornerRadius::same(10), fill);
    p.rect_filled(
        Rect::from_min_size(rect.min + Vec2::new(10.0, 8.0), Vec2::new(rect.width() - 20.0, 4.0)),
        CornerRadius::same(2),
        colour,
    );
    p.text(rect.min + Vec2::new(10.0, 24.0), Align2::LEFT_CENTER, format!("Stage {}", index + 1), FontId::proportional(11.5), TEXT_DIM);
    if active {
        p.text(rect.right_top() + Vec2::new(-10.0, 24.0), Align2::RIGHT_CENTER, "IN USE", FontId::proportional(10.0), ACCENT);
    }
    p.text(rect.min + Vec2::new(10.0, 46.0), Align2::LEFT_CENTER, format!("{dpi}"), FontId::proportional(21.0), Color32::WHITE);
    if active {
        p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(2.0, ACCENT), StrokeKind::Inside);
    } else if editing {
        p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.6)), StrokeKind::Inside);
    }
    resp.clicked()
}

/// A DPI scale (logarithmic) with a marker per stage.
fn dpi_track(ui: &mut Ui, stages: &[u32], min: u32, max: u32, editing: usize, colour_of: &dyn Fn(usize) -> Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::hover());
    let p = ui.painter();
    let (lo, hi) = ((min.max(1) as f32).ln(), (max as f32).ln());
    let x_of = |v: u32| rect.min.x + 8.0 + (rect.width() - 16.0) * (((v.max(1) as f32).ln() - lo) / (hi - lo)).clamp(0.0, 1.0);
    let y = rect.min.y + 16.0;
    p.rect_filled(Rect::from_min_max(Pos2::new(rect.min.x + 8.0, y - 2.0), Pos2::new(rect.max.x - 8.0, y + 2.0)), CornerRadius::same(2), KEYCAP);
    for tick in [100u32, 400, 800, 1600, 3200, 6400, 12800, 26000, 42000] {
        if tick < min || tick > max {
            continue;
        }
        let x = x_of(tick);
        p.line_segment([Pos2::new(x, y + 5.0), Pos2::new(x, y + 9.0)], Stroke::new(1.0, TEXT_DIM.gamma_multiply(0.6)));
        p.text(Pos2::new(x, y + 20.0), Align2::CENTER_CENTER, format!("{tick}"), FontId::proportional(10.0), TEXT_DIM.gamma_multiply(0.8));
    }
    for (s, &v) in stages.iter().enumerate() {
        let c = Pos2::new(x_of(v), y);
        p.circle_filled(c, if s == editing { 8.0 } else { 6.0 }, colour_of(s));
        if s == editing {
            p.circle_stroke(c, 10.0, Stroke::new(1.5, Color32::WHITE));
        }
    }
}

/// A small on/off switch with a label and an explanation underneath.
fn toggle(ui: &mut Ui, on: &mut bool, label: &str, hint: &str) {
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(38.0, 22.0), Sense::click());
        if resp.clicked() {
            *on = !*on;
        }
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(11), if *on { ACCENT } else { KEYCAP });
        let x = if *on { rect.max.x - 11.0 } else { rect.min.x + 11.0 };
        p.circle_filled(Pos2::new(x, rect.center().y), 8.0, Color32::WHITE);
        ui.vertical(|ui| {
            ui.label(RichText::new(label).color(Color32::WHITE));
            ui.label(RichText::new(hint).size(12.0).color(TEXT_DIM));
        });
    });
    ui.add_space(4.0);
}

fn draw_mouse_icon(p: &egui::Painter, r: Rect) {
    p.rect_stroke(r, CornerRadius::same(9), Stroke::new(1.5, ACCENT), StrokeKind::Inside);
    let top = r.min.y + 2.0;
    p.line_segment([Pos2::new(r.center().x, top), Pos2::new(r.center().x, r.min.y + r.height() * 0.42)], Stroke::new(1.5, ACCENT));
    p.line_segment(
        [Pos2::new(r.min.x + 2.0, r.min.y + r.height() * 0.42), Pos2::new(r.max.x - 2.0, r.min.y + r.height() * 0.42)],
        Stroke::new(1.0, ACCENT.gamma_multiply(0.6)),
    );
    p.circle_filled(Pos2::new(r.center().x, r.min.y + r.height() * 0.24), 1.8, ACCENT);
}

fn draw_keyboard_icon(p: &egui::Painter, r: Rect) {
    p.rect_stroke(r, CornerRadius::same(3), Stroke::new(1.5, ACCENT), StrokeKind::Inside);
    for row in 0..3 {
        let y = r.min.y + 4.5 + row as f32 * 4.5;
        let n = if row == 2 { 1 } else { 5 };
        for i in 0..n {
            let (x0, x1) = if row == 2 {
                (r.min.x + 9.0, r.max.x - 9.0)
            } else {
                let w = (r.width() - 8.0) / 5.0;
                (r.min.x + 4.0 + i as f32 * w, r.min.x + 4.0 + i as f32 * w + w - 1.5)
            };
            p.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(2.0, ACCENT.gamma_multiply(0.8)));
        }
    }
}
