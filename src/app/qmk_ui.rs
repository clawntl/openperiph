//! Pages for QMK keyboards with VIA: keymap (all layers), lighting (the definition's menus),
//! macros, and settings (layout options, backups, resets). Every change is sent to the keyboard
//! straight away by the device's worker thread (`crate::qmk`), the way VIA works.

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{card, pill, segment, tab_button, toggle, DeviceEntry, Msg, OpenPeriphApp, Tab, ACCENT, BAD, CARD, KEYCAP, TEXT_DIM};
use crate::device::Driver;
use crate::drivers::qmk::QmkDriver;
use crate::hid;
use crate::qmk::definition::{self, Definition};
use crate::qmk::keycodes::{self, CustomKeycode, KC_NO, KC_TRNS, MOD_ALT, MOD_CTL, MOD_GUI, MOD_RIGHT, MOD_SFT, QK_KB, QK_MACRO};
use crate::qmk::menus::{self, Control, ControlKind};
use crate::qmk::{self, Event, Job, Snapshot};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Picker {
    #[default]
    Basic,
    Media,
    Mouse,
    Layers,
    Modifiers,
    Lighting,
    Macros,
    Qmk,
    Custom,
    Any,
}

impl Picker {
    const ALL: [(Picker, &'static str); 10] = [
        (Picker::Basic, "Basic"),
        (Picker::Media, "Media"),
        (Picker::Mouse, "Mouse"),
        (Picker::Layers, "Layers"),
        (Picker::Modifiers, "Modifiers"),
        (Picker::Lighting, "Lighting"),
        (Picker::Macros, "Macros"),
        (Picker::Qmk, "QMK"),
        (Picker::Custom, "Custom"),
        (Picker::Any, "Any"),
    ];
}

/// What the window keeps for one QMK keyboard.
#[derive(Default)]
pub struct QmkState {
    jobs: Option<Sender<Job>>,
    pub snap: Option<Snapshot>,
    loading: bool,
    error: Option<String>,
    layer: usize,
    key: Option<(u8, u8)>,
    picker: Picker,
    any_text: String,
    builder_layer: u8,
    builder_mods: u8,
    builder_key: u16,
    macro_sel: usize,
    macro_edit: Vec<String>,
    macro_dirty: bool,
    macro_error: Option<String>,
    macro_insert: u16,
    /// A destructive button waiting for its second click.
    confirm: Option<&'static str>,
}

impl QmkState {
    fn send(&self, job: Job) {
        if let Some(tx) = &self.jobs {
            let _ = tx.send(job);
        }
    }
}

/// The simulated keyboard for `--demo`.
pub fn demo_driver() -> Arc<dyn Driver> {
    let def = Definition::parse(definition::BUNDLED[0]).ok();
    let mut d = QmkDriver::new(0xFEED, 0x0620, "demo".into(), "SK620".into(), "QMK".into(), def);
    d.demo = true;
    Arc::new(d)
}

fn qmk_driver(d: &DeviceEntry) -> Option<&QmkDriver> {
    d.driver.as_qmk()
}

impl OpenPeriphApp {
    // -----------------------------------------------------------------------------------------
    // Connection

    /// Called after each detection pass for a QMK keyboard: starts or stops its worker.
    pub(super) fn qmk_connection_changed(&mut self, i: usize, ctx: &egui::Context) {
        let connected = self.devices[i].connected == Some(true);
        let access = self.devices[i].needs_access;
        let screenshot = self.screenshot_mode();
        let driver = self.devices[i].driver.clone();
        let Some(q) = self.devices[i].qmk.as_mut() else { return };
        if !connected || access {
            q.jobs = None;
            q.loading = false;
            return;
        }
        if q.jobs.is_some() {
            return;
        }
        let Some(qd) = driver.as_qmk() else { return };
        let connect: qmk::Connect = if qd.demo {
            let kb = std::sync::Arc::new(std::sync::Mutex::new(qmk::demo::fake_sk620()));
            Box::new(move || Ok(Box::new(SharedFake(kb.clone())) as Box<dyn qmk::via::Transport>))
        } else {
            let Some(c) = qd.find() else { return };
            qmk::connect_hid(c.path)
        };
        if screenshot && !qd.demo {
            return;
        }
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let jobs = qmk::spawn_worker(connect, move |e| {
            let _ = tx.send(Msg::Qmk(i, e));
            ctx.request_repaint();
        });
        let _ = jobs.send(Job::Load(qd.definition()));
        q.jobs = Some(jobs);
        q.loading = true;
        q.error = None;
    }

    pub(super) fn qmk_event(&mut self, i: usize, e: Event) {
        let name = self.devices[i].driver.info().name.clone();
        let Some(q) = self.devices[i].qmk.as_mut() else { return };
        match e {
            Event::Loaded(Ok(s)) => {
                q.loading = false;
                q.error = None;
                q.macro_edit = s.macros.clone();
                q.macro_dirty = false;
                q.layer = q.layer.min(s.layers.saturating_sub(1));
                q.snap = Some(s);
                if self.screenshot.is_some() && q.key.is_none() {
                    q.key = Some((2, 1));
                }
                self.set_status(format!("Read the keymap and settings from {name}."), true);
            }
            Event::Loaded(Err(e)) => {
                q.loading = false;
                q.error = Some(e.clone());
                q.jobs = None;
                self.set_status(e, false);
            }
            Event::Done(Ok(m)) => self.set_status(m, true),
            Event::Done(Err(e)) => {
                self.set_status(e, false);
                // read everything again so the window shows what the keyboard really has
                let def = qmk_driver(&self.devices[i]).and_then(|d| d.definition());
                if let Some(q) = self.devices[i].qmk.as_mut() {
                    q.send(Job::Load(def));
                }
            }
        }
    }

    /// Adds QMK keyboards that aren't listed yet.
    pub(super) fn add_discovered(&mut self, found: Vec<hid::Collection>) {
        let defs = definition::all_definitions();
        for c in found {
            let known = self.devices.iter().any(|d| d.driver.as_qmk().is_some_and(|q| !q.demo && q.matches(&c)));
            if known {
                continue;
            }
            let def = defs.iter().find(|d| (d.vendor_id, d.product_id) == (c.vendor_id, c.product_id)).cloned();
            let name = if c.product.is_empty() { format!("QMK keyboard {:04X}:{:04X}", c.vendor_id, c.product_id) } else { c.product.clone() };
            let maker = if c.manufacturer.is_empty() { "QMK".to_string() } else { c.manufacturer.clone() };
            let driver = QmkDriver::new(c.vendor_id, c.product_id, c.serial.clone(), name, maker, def);
            let mut entry = DeviceEntry::new(Arc::new(driver));
            entry.connected = None;
            self.devices.push(entry);
            self.last_detect = None; // check it straight away
        }
    }

    // -----------------------------------------------------------------------------------------
    // Tabs

    fn qmk_tabs_available(&self) -> Vec<(Tab, &'static str)> {
        let d = &self.devices[self.selected];
        let q = d.qmk.as_ref();
        let snap = q.and_then(|q| q.snap.as_ref());
        let mut v = vec![(Tab::Keys, "Keymap")];
        if snap.is_some_and(|s| !s.menus.is_empty()) {
            v.push((Tab::Lighting, "Lighting"));
        }
        if snap.is_some_and(|s| s.macro_count > 0) {
            v.push((Tab::Macros, "Macros"));
        }
        v.push((Tab::Settings, "Settings"));
        v
    }

    pub(super) fn qmk_tabs(&mut self, ui: &mut Ui) {
        let tabs = self.qmk_tabs_available();
        ui.horizontal(|ui| {
            for (t, name) in tabs {
                if tab_button(ui, name, self.tab == t) {
                    self.tab = t;
                }
            }
        });
    }

    pub(super) fn qmk_fix_tab(&mut self) {
        let tabs = self.qmk_tabs_available();
        if !tabs.iter().any(|(t, _)| *t == self.tab) {
            // in a screenshot, wait for the simulated keyboard to be read before falling back
            if self.screenshot_mode() && self.devices[self.selected].qmk.as_ref().is_some_and(|q| q.snap.is_none()) {
                return;
            }
            self.tab = Tab::Keys;
        }
    }

    pub(super) fn qmk_page(&mut self, ui: &mut Ui, tab: Tab) {
        let i = self.selected;
        let connected = self.devices[i].connected == Some(true);
        let def = qmk_driver(&self.devices[i]).and_then(|d| d.definition());
        let (loading, error, have) = {
            let q = self.devices[i].qmk.as_ref().unwrap();
            (q.loading, q.error.clone(), q.snap.is_some())
        };
        if tab == Tab::Settings {
            self.qmk_settings_page(ui, def.as_deref());
            return;
        }
        if !connected && !have {
            card(ui, |ui| {
                ui.label(RichText::new("Plug in the keyboard").size(16.0).strong().color(Color32::WHITE));
                ui.label(RichText::new("Its keymap, macros and lighting are read from the keyboard itself, so they show up once it is connected.").color(TEXT_DIM));
            });
            return;
        }
        if let Some(e) = error {
            card(ui, |ui| {
                ui.label(RichText::new("Couldn't read the keyboard").size(16.0).strong().color(BAD));
                ui.label(RichText::new(e).color(TEXT_DIM));
                if ui.button("Try again").clicked() {
                    if let Some(q) = self.devices[i].qmk.as_mut() {
                        q.error = None;
                    }
                    let ctx = ui.ctx().clone();
                    self.qmk_connection_changed(i, &ctx);
                }
            });
            return;
        }
        if loading && !have {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Reading the keyboard…").color(TEXT_DIM));
            });
            return;
        }
        match tab {
            Tab::Keys => self.qmk_keymap_page(ui, def),
            Tab::Lighting => self.qmk_lighting_page(ui),
            Tab::Macros => self.qmk_macros_page(ui, def.as_deref()),
            _ => {}
        }
    }

    // -----------------------------------------------------------------------------------------
    // Keymap

    fn qmk_keymap_page(&mut self, ui: &mut Ui, def: Option<Arc<Definition>>) {
        let Some(def) = def else {
            self.no_definition_card(ui);
            return;
        };
        let i = self.selected;
        let Some(q) = self.devices[i].qmk.as_mut() else { return };
        let Some(snap) = q.snap.as_ref() else { return };
        if snap.keymap.is_empty() {
            // read before the definition was known
            q.send(Job::Load(Some(def.clone())));
            q.snap = None;
            q.loading = true;
            return;
        }
        let layers = snap.layers;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Layer").color(TEXT_DIM));
            for l in 0..layers {
                let name = if l == 0 { "0 · base".to_string() } else { format!("  {l}  ") };
                if segment(ui, &name, q.layer == l) {
                    q.layer = l;
                }
            }
            ui.add_space(16.0);
            ui.label(RichText::new("Click a key, then pick what it does. ▽ means it does whatever the layer below does.").color(TEXT_DIM));
        });
        ui.add_space(10.0);

        let choices = def.unpack_options(snap.layout_options);
        let keys = def.visible_keys(&choices);
        let w = keys.iter().map(|k| k.x + k.w.max(k.second.map_or(0.0, |s| s[0] + s[2]))).fold(0.0f32, f32::max);
        let h = keys.iter().map(|k| k.y + k.h.max(k.second.map_or(0.0, |s| s[1] + s[3]))).fold(0.0f32, f32::max);
        let avail = ui.available_width();
        let kb_h = (avail * 0.36).clamp(200.0, 340.0);
        let mut clicked = None;
        let layer = q.layer;
        let keymap = &snap.keymap;
        let custom = def.custom_keycodes.clone();
        egui::Frame::new().fill(CARD).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), kb_h), Sense::hover());
            let scale = (area.width() / w).min(area.height() / h);
            let origin = area.center() - Vec2::new(w, h) * scale / 2.0;
            for k in &keys {
                let r = Rect::from_min_size(origin + Vec2::new(k.x, k.y) * scale, Vec2::new(k.w, k.h) * scale).shrink(1.5);
                let mut rects = vec![r];
                if let Some(s) = k.second {
                    rects.push(Rect::from_min_size(origin + Vec2::new(k.x + s[0], k.y + s[1]) * scale, Vec2::new(s[2], s[3]) * scale).shrink(1.5));
                }
                let id = ui.make_persistent_id(("qmkkey", k.row, k.col));
                let hit = rects.iter().skip(1).fold(r, |a, b| a.union(*b));
                let resp = ui.interact(hit, id, Sense::click());
                let code = keymap.get(layer).and_then(|l| l.get(k.row as usize)).and_then(|r| r.get(k.col as usize)).copied().unwrap_or(0);
                let selected = q.key == Some((k.row, k.col));
                let mut fill = match code {
                    KC_TRNS => Color32::from_rgb(36, 40, 48),
                    KC_NO => Color32::from_rgb(30, 32, 38),
                    _ => KEYCAP,
                };
                if layer > 0 && code != KC_TRNS {
                    fill = Color32::from_rgb(28, 74, 84);
                }
                if resp.hovered() {
                    fill = fill.gamma_multiply(1.25);
                }
                let p = ui.painter();
                for rr in &rects {
                    p.rect_filled(*rr, CornerRadius::same(5), fill);
                }
                if selected {
                    for rr in &rects {
                        p.rect_stroke(*rr, CornerRadius::same(5), Stroke::new(2.0, ACCENT), StrokeKind::Outside);
                    }
                }
                let (main, sub) = keycodes::cap(code, &custom);
                let fs = (11.0 * scale / 40.0 * 3.2).clamp(8.0, 13.0);
                let colour = if code == KC_TRNS { TEXT_DIM } else { Color32::WHITE };
                let centre = if k.second.is_some() && k.h > 1.0 { r.center() + Vec2::new(0.0, r.height() * 0.15) } else { r.center() };
                let main = fit(&main, r.width(), fs);
                match sub {
                    Some(s) => {
                        p.text(centre - Vec2::new(0.0, fs * 0.55), Align2::CENTER_CENTER, main, FontId::proportional(fs), colour);
                        p.text(centre + Vec2::new(0.0, fs * 0.65), Align2::CENTER_CENTER, fit(&s, r.width(), fs * 0.8), FontId::proportional(fs * 0.8), ACCENT);
                    }
                    None => {
                        p.text(centre, Align2::CENTER_CENTER, main, FontId::proportional(fs), colour);
                    }
                }
                if resp.clicked() {
                    clicked = Some((k.row, k.col));
                }
                resp.on_hover_text(format!("{}  ·  matrix {},{}", keycodes::name(code), k.row, k.col));
            }
        });
        if clicked.is_some() {
            q.key = clicked;
        }
        ui.add_space(12.0);
        card(ui, |ui| self.qmk_key_editor(ui, &def, &keys));
    }

    fn qmk_key_editor(&mut self, ui: &mut Ui, def: &Definition, keys: &[definition::Key]) {
        let i = self.selected;
        let Some(q) = self.devices[i].qmk.as_mut() else { return };
        let Some((row, col)) = q.key else {
            ui.label(RichText::new("Pick a key above to change it.").color(TEXT_DIM));
            return;
        };
        let Some(snap) = q.snap.as_ref() else { return };
        let layer = q.layer;
        let current = snap.keymap[layer][row as usize][col as usize];
        let layers = snap.layers;
        let macro_count = snap.macro_count;
        let macro_texts = snap.macros.clone();
        let custom = &def.custom_keycodes;
        let mut assign: Option<u16> = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Layer {layer}, key {row},{col}")).size(18.0).strong().color(Color32::WHITE));
            ui.add_space(12.0);
            ui.label(RichText::new(format!("{}  ({})", keycodes::describe(current, custom), keycodes::name(current))).color(TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Off (KC_NO)").clicked() {
                    assign = Some(KC_NO);
                }
                if layer > 0 && ui.button("▽ Transparent").clicked() {
                    assign = Some(KC_TRNS);
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (p, name) in Picker::ALL {
                if p == Picker::Custom && custom.is_empty() {
                    continue;
                }
                if p == Picker::Macros && macro_count == 0 {
                    continue;
                }
                if segment(ui, name, q.picker == p) {
                    q.picker = p;
                }
            }
        });
        ui.add_space(8.0);

        let kc = |code: u16| (code, keycodes::cap(code, custom).0, keycodes::name(code));
        match q.picker {
            Picker::Basic => {
                let groups: [(&str, Vec<u16>); 7] = [
                    ("Letters", (0x04..=0x1D).collect()),
                    ("Numbers and symbols", (0x1E..=0x27).chain(0x2D..=0x38).chain([0x64]).collect()),
                    ("Editing and navigation", vec![0x29, 0x28, 0x2A, 0x2B, 0x2C, 0x39, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50, 0x51, 0x52, 0x65]),
                    ("Function keys", (0x3A..=0x45).chain(0x68..=0x73).collect()),
                    ("Modifiers", (0xE0..=0xE7).collect()),
                    ("Number pad", (0x53..=0x63).chain([0x67, 0x85, 0x86]).collect()),
                    ("International and language", (0x87..=0x98).collect()),
                ];
                for (title, codes) in groups {
                    ui.label(RichText::new(title).size(11.5).color(TEXT_DIM));
                    let items: Vec<_> = codes.into_iter().filter(|c| keycodes::lookup(*c).is_some()).map(kc).collect();
                    if let Some(c) = key_grid(ui, &items, current) {
                        assign = Some(c);
                    }
                }
            }
            Picker::Media => {
                for (title, groups) in [("Media", &["media"][..]), ("System", &["system"][..])] {
                    ui.label(RichText::new(title).size(11.5).color(TEXT_DIM));
                    if let Some(c) = key_grid(ui, &group_items(groups, custom), current) {
                        assign = Some(c);
                    }
                }
            }
            Picker::Mouse => {
                if let Some(c) = key_grid(ui, &group_items(&["mouse"], custom), current) {
                    assign = Some(c);
                }
            }
            Picker::Layers => {
                for (title, base, help) in [
                    ("MO: while held", 0x5220u16, "Layer while the key is held"),
                    ("TG: toggle", 0x5260, "Turns the layer on or off"),
                    ("TO: switch to", 0x5200, "Switches to the layer"),
                    ("TT: hold, or tap to toggle", 0x52C0, "Layer while held; tapping it a few times toggles it"),
                    ("OSL: next key only", 0x5280, "Layer for the next key press"),
                    ("DF: base layer", 0x5240, "Makes the layer the base layer until unplugged"),
                ] {
                    ui.label(RichText::new(title).size(11.5).color(TEXT_DIM)).on_hover_text(help);
                    let items: Vec<_> = (0..layers as u16).map(|l| kc(base | l)).collect();
                    if let Some(c) = key_grid(ui, &items, current) {
                        assign = Some(c);
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("LT: a key when tapped, a layer when held").size(11.5).color(TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.label("Layer");
                    for l in 0..(layers.min(16) as u8) {
                        if segment(ui, &l.to_string(), q.builder_layer == l) {
                            q.builder_layer = l;
                        }
                    }
                    ui.label("tap");
                    basic_combo(ui, "lt_key", &mut q.builder_key);
                    if ui.button("Set").clicked() {
                        assign = Some(0x4000 | ((q.builder_layer as u16 & 0xF) << 8) | (q.builder_key & 0xFF));
                    }
                });
            }
            Picker::Modifiers => {
                ui.label(RichText::new("Modifier keys").size(11.5).color(TEXT_DIM));
                if let Some(c) = key_grid(ui, &group_items(&["modifiers"], custom), current) {
                    assign = Some(c);
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Shortcuts and tap-hold keys").size(11.5).color(TEXT_DIM));
                ui.horizontal_wrapped(|ui| {
                    for (bit, name) in [(MOD_CTL, "Ctrl"), (MOD_SFT, "Shift"), (MOD_ALT, "Alt"), (MOD_GUI, "Win"), (MOD_RIGHT, "Right-hand")] {
                        let mut on = q.builder_mods & bit != 0;
                        if ui.checkbox(&mut on, name).changed() {
                            q.builder_mods = if on { q.builder_mods | bit } else { q.builder_mods & !bit };
                        }
                    }
                    ui.label("+");
                    basic_combo(ui, "mod_key", &mut q.builder_key);
                });
                let m = q.builder_mods & 0x1F;
                let k = q.builder_key & 0xFF;
                ui.horizontal_wrapped(|ui| {
                    let ok = m & 0x0F != 0;
                    if ui.add_enabled(ok, egui::Button::new("Shortcut (mods + key)")).clicked() {
                        assign = Some(0x0100 | ((m as u16) << 8) | k);
                    }
                    if ui.add_enabled(ok, egui::Button::new("Key when tapped, mods when held")).clicked() {
                        assign = Some(0x2000 | ((m as u16) << 8) | k);
                    }
                    if ui.add_enabled(ok, egui::Button::new("One-shot mods")).on_hover_text("The modifiers apply to the next key press").clicked() {
                        assign = Some(0x52A0 | m as u16);
                    }
                });
            }
            Picker::Lighting => {
                for (title, g) in [("RGB Matrix", "rgb_matrix"), ("Underglow", "underglow"), ("Backlight", "backlight"), ("LED Matrix", "led_matrix")] {
                    let items = group_items(&[g], custom);
                    if items.is_empty() {
                        continue;
                    }
                    ui.label(RichText::new(title).size(11.5).color(TEXT_DIM));
                    if let Some(c) = key_grid(ui, &items, current) {
                        assign = Some(c);
                    }
                }
            }
            Picker::Macros => {
                let items: Vec<_> = (0..macro_count as u16).map(|n| kc(QK_MACRO + n)).collect();
                ui.label(RichText::new("Plays a macro from the Macros page").size(11.5).color(TEXT_DIM));
                if let Some(c) = key_grid(ui, &items, current) {
                    assign = Some(c);
                }
                if let Some(n) = current.checked_sub(QK_MACRO).filter(|n| (*n as usize) < macro_texts.len()) {
                    ui.label(RichText::new(format!("M{n}: {}", macro_texts[n as usize])).color(TEXT_DIM));
                }
            }
            Picker::Qmk => {
                for (title, g) in [("QMK", "quantum"), ("Magic (swap and lock keys)", "magic")] {
                    ui.label(RichText::new(title).size(11.5).color(TEXT_DIM));
                    if let Some(c) = key_grid(ui, &group_items(&[g], custom), current) {
                        assign = Some(c);
                    }
                }
            }
            Picker::Custom => {
                let items: Vec<_> = (0..custom.len() as u16).map(|n| kc(QK_KB + n)).collect();
                if let Some(c) = key_grid(ui, &items, current) {
                    assign = Some(c);
                }
            }
            Picker::Any => {
                ui.label(RichText::new("Type any QMK keycode: KC_A, MO(1), LT(2, KC_SPC), LCTL(KC_C), MT(MOD_LSFT, KC_Z), QK_BOOT or a number like 0x7C00.").color(TEXT_DIM));
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut q.any_text).hint_text("keycode").desired_width(260.0));
                    let parsed = keycodes::parse(&q.any_text, custom);
                    let go = ui.add_enabled(parsed.is_some(), egui::Button::new("Set")).clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && parsed.is_some());
                    match parsed {
                        Some(c) => {
                            ui.label(RichText::new(format!("= {}  ·  {}", keycodes::name(c), keycodes::describe(c, custom))).color(ACCENT));
                        }
                        None if !q.any_text.trim().is_empty() => {
                            ui.label(RichText::new("not a keycode").color(BAD));
                        }
                        None => {}
                    }
                    if go {
                        assign = parsed;
                    }
                });
            }
        }

        if let Some(code) = assign {
            if code != current {
                if let Some(s) = q.snap.as_mut() {
                    s.keymap[layer][row as usize][col as usize] = code;
                }
                q.send(Job::SetKey { layer: layer as u8, row, col, code });
            }
            // move on to the next key, as VIA does
            let mut order: Vec<&definition::Key> = keys.iter().collect();
            order.sort_by(|a, b| (a.y.round() as i32, a.x).partial_cmp(&(b.y.round() as i32, b.x)).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(pos) = order.iter().position(|k| (k.row, k.col) == (row, col)) {
                if let Some(next) = order.get(pos + 1) {
                    q.key = Some((next.row, next.col));
                }
            }
        }
    }

    fn no_definition_card(&mut self, ui: &mut Ui) {
        let info = self.devices[self.selected].driver.info().clone();
        card(ui, |ui| {
            ui.label(RichText::new("This keyboard needs its VIA definition").size(16.0).strong().color(Color32::WHITE));
            ui.label(
                RichText::new(format!(
                    "QMK keyboards don't describe their own key positions, so the keymap needs the keyboard's VIA definition file (JSON). \
                     Look for {:04X}:{:04X} in github.com/the-via/keyboards (folder v3), or ask the keyboard's maker, then import it here. \
                     Lighting and macros work without it.",
                    info.vendor_id, info.product_id
                ))
                .color(TEXT_DIM),
            );
            ui.add_space(6.0);
            if ui.button("Import VIA definition…").clicked() {
                self.import_definition();
            }
        });
    }

    fn import_definition(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("VIA definition", &["json"]).set_title("Import a VIA definition").pick_file() else {
            return;
        };
        let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Definition::parse(&t).map(|d| (t, d)));
        let i = self.selected;
        match result {
            Ok((text, def)) => {
                let info = self.devices[i].driver.info().clone();
                if (def.vendor_id, def.product_id) != (info.vendor_id, info.product_id) {
                    self.set_status(
                        format!("That definition is for {:04X}:{:04X}, not this keyboard ({:04X}:{:04X}).", def.vendor_id, def.product_id, info.vendor_id, info.product_id),
                        false,
                    );
                    return;
                }
                if let Err(e) = crate::store::import_definition(&text, def.vendor_id, def.product_id) {
                    self.set_status(format!("Couldn't keep a copy of the definition: {e}"), false);
                }
                let name = def.name.clone();
                if let Some(qd) = self.devices[i].driver.as_qmk() {
                    qd.set_definition(def);
                }
                let def = qmk_driver(&self.devices[i]).and_then(|d| d.definition());
                if let Some(q) = self.devices[i].qmk.as_mut() {
                    q.send(Job::Load(def));
                    q.loading = true;
                }
                self.set_status(format!("Imported the definition for {name}."), true);
            }
            Err(e) => self.set_status(e, false),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Lighting

    fn qmk_lighting_page(&mut self, ui: &mut Ui) {
        let i = self.selected;
        let Some(q) = self.devices[i].qmk.as_mut() else { return };
        let Some(snap) = q.snap.as_mut() else { return };
        let menus = snap.menus.clone();
        let mut changes: Vec<(u8, u8, Vec<u8>)> = Vec::new();
        for m in &menus {
            for sec in &m.sections {
                // values by name, for showIf
                let named: HashMap<String, i64> = menus
                    .iter()
                    .flat_map(|m| m.sections.iter())
                    .flat_map(|s| s.controls.iter())
                    .map(|c| (c.key.clone(), c.number(snap.values.get(&(c.channel, c.id)).map(Vec::as_slice).unwrap_or(&[]))))
                    .collect();
                card(ui, |ui| {
                    let title = if sec.label == m.label { m.label.clone() } else { format!("{} · {}", m.label, sec.label) };
                    ui.label(RichText::new(title).size(16.0).strong().color(Color32::WHITE));
                    ui.add_space(6.0);
                    for c in &sec.controls {
                        if let Some(expr) = &c.show_if {
                            if !menus::show_if(expr, &named) {
                                continue;
                            }
                        }
                        let cur = snap.values.get(&(c.channel, c.id)).cloned().unwrap_or_else(|| vec![0; c.len()]);
                        if let Some(new) = control(ui, c, &cur) {
                            snap.values.insert((c.channel, c.id), new.clone());
                            changes.push((c.channel, c.id, new));
                        }
                        ui.add_space(4.0);
                    }
                });
                ui.add_space(10.0);
            }
        }
        for (channel, id, data) in changes {
            q.send(Job::SetValue { channel, id, data });
        }
    }

    // -----------------------------------------------------------------------------------------
    // Macros

    fn qmk_macros_page(&mut self, ui: &mut Ui, def: Option<&Definition>) {
        let custom: Vec<CustomKeycode> = def.map(|d| d.custom_keycodes.clone()).unwrap_or_default();
        let i = self.selected;
        let Some(q) = self.devices[i].qmk.as_mut() else { return };
        let Some(snap) = q.snap.as_ref() else { return };
        let count = snap.macro_count;
        let size = snap.macro_size;
        if q.macro_edit.len() < count {
            q.macro_edit.resize(count, String::new());
        }
        let used = qmk::macros::encode(&q.macro_edit, &custom).map(|b| b.len());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Pick a macro, write what it types, then save. A key set to M0, M1… on the Keymap page plays it.").color(TEXT_DIM));
        });
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            card(ui, |ui| {
                for n in 0..count {
                    let text = q.macro_edit[n].replace('\n', "⏎");
                    let preview: String = if text.is_empty() { "(empty)".into() } else { text.chars().take(34).collect() };
                    let sel = q.macro_sel == n;
                    let btn = egui::Button::new(RichText::new(format!("M{n}   {preview}")).color(if sel { Color32::BLACK } else { Color32::WHITE }))
                        .fill(if sel { ACCENT } else { KEYCAP })
                        .corner_radius(CornerRadius::same(8))
                        .min_size(Vec2::new(ui.available_width(), 28.0));
                    if ui.add(btn).clicked() {
                        q.macro_sel = n;
                    }
                }
            });
            let ui = &mut cols[1];
            card(ui, |ui| {
                let n = q.macro_sel.min(count.saturating_sub(1));
                ui.label(RichText::new(format!("Macro M{n}")).size(16.0).strong().color(Color32::WHITE));
                let edit = egui::TextEdit::multiline(&mut q.macro_edit[n]).font(egui::TextStyle::Monospace).desired_rows(6).desired_width(f32::INFINITY);
                if ui.add(edit).changed() {
                    q.macro_dirty = true;
                    q.macro_error = None;
                }
                ui.label(
                    RichText::new("Text is typed as written. {KC_ENT} taps a key, {KC_LCTL,KC_C} is a shortcut, {+KC_LSFT} / {-KC_LSFT} hold and release, {200} waits 200 ms.")
                        .size(12.0)
                        .color(TEXT_DIM),
                );
                ui.horizontal(|ui| {
                    ui.label("Insert key");
                    basic_combo(ui, "macro_key", &mut q.macro_insert);
                    if ui.button("Tap").clicked() {
                        q.macro_edit[n].push_str(&format!("{{{}}}", keycodes::name(q.macro_insert)));
                        q.macro_dirty = true;
                    }
                    if ui.button("Wait 100 ms").clicked() {
                        q.macro_edit[n].push_str("{100}");
                        q.macro_dirty = true;
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let save = egui::Button::new(RichText::new("Save macros").strong().color(if q.macro_dirty { Color32::BLACK } else { TEXT_DIM }))
                        .fill(if q.macro_dirty { ACCENT } else { KEYCAP });
                    if ui.add_enabled(q.macro_dirty, save).clicked() {
                        match qmk::macros::encode(&q.macro_edit, &custom) {
                            Ok(buf) if buf.len() <= size => {
                                q.send(Job::Macros(buf));
                                q.macro_dirty = false;
                                if let Some(s) = q.snap.as_mut() {
                                    s.macros = q.macro_edit.clone();
                                }
                            }
                            Ok(buf) => q.macro_error = Some(format!("The macros need {} bytes; the keyboard has {size}.", buf.len())),
                            Err(e) => q.macro_error = Some(e),
                        }
                    }
                    if q.macro_dirty && ui.button("Undo changes").clicked() {
                        if let Some(s) = q.snap.as_ref() {
                            q.macro_edit = s.macros.clone();
                        }
                        q.macro_dirty = false;
                        q.macro_error = None;
                    }
                    match &used {
                        Ok(u) => {
                            ui.label(RichText::new(format!("{u} of {size} bytes used")).color(if *u > size { BAD } else { TEXT_DIM }));
                        }
                        Err(e) => {
                            ui.label(RichText::new(e).color(BAD));
                        }
                    }
                });
                if let Some(e) = &q.macro_error {
                    ui.label(RichText::new(e).color(BAD));
                }
            });
        });
    }

    // -----------------------------------------------------------------------------------------
    // Settings

    fn qmk_settings_page(&mut self, ui: &mut Ui, def: Option<&Definition>) {
        let i = self.selected;
        let info = self.devices[i].driver.info().clone();
        let connected = self.devices[i].connected == Some(true);
        let mut import = false;
        let mut save_backup = false;
        let mut load_backup = false;
        {
            let Some(q) = self.devices[i].qmk.as_mut() else { return };
            let snap = q.snap.clone();
            ui.columns(2, |cols| {
                card(&mut cols[0], |ui| {
                    ui.label(RichText::new("Keyboard").size(16.0).strong().color(Color32::WHITE));
                    ui.add_space(4.0);
                    let row = |ui: &mut Ui, a: &str, b: String| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(a).color(TEXT_DIM));
                            ui.label(b);
                        });
                    };
                    row(ui, "USB id", format!("{:04X}:{:04X}", info.vendor_id, info.product_id));
                    row(ui, "Definition", def.map(|d| d.name.clone()).unwrap_or_else(|| "none (import one for the keymap)".into()));
                    if let Some(s) = &snap {
                        row(ui, "VIA protocol", s.protocol.to_string());
                        row(ui, "Layers", s.layers.to_string());
                        row(ui, "Macros", format!("{} ({} bytes)", s.macro_count, s.macro_size));
                        if let Some(f) = s.firmware.filter(|f| *f != 0) {
                            row(ui, "Firmware version", format!("{f:#010x}"));
                        }
                    }
                    ui.add_space(6.0);
                    if ui.button(if def.is_some() { "Replace VIA definition…" } else { "Import VIA definition…" }).clicked() {
                        import = true;
                    }
                });
                card(&mut cols[1], |ui| {
                    ui.label(RichText::new("Backup").size(16.0).strong().color(Color32::WHITE));
                    ui.label(RichText::new("Saves the keymap, macros and lighting to a file (compatible with VIA's Save + Load), or puts a saved file back.").color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        let ok = connected && snap.is_some() && def.is_some();
                        if ui.add_enabled(ok, egui::Button::new("Save to file…")).clicked() {
                            save_backup = true;
                        }
                        if ui.add_enabled(ok, egui::Button::new("Load from file…")).clicked() {
                            load_backup = true;
                        }
                    });
                });
            });
            ui.add_space(10.0);

            if let (Some(d), Some(s)) = (def, snap.as_ref()) {
                if !d.options.is_empty() {
                    card(ui, |ui| {
                        ui.label(RichText::new("Layout options").size(16.0).strong().color(Color32::WHITE));
                        ui.label(RichText::new("Which physical keys your keyboard has, where the PCB allows more than one.").color(TEXT_DIM));
                        let mut choices = d.unpack_options(s.layout_options);
                        let before = choices.clone();
                        for (n, o) in d.options.iter().enumerate() {
                            if o.choices.is_empty() {
                                let mut on = choices[n] == 1;
                                toggle(ui, &mut on, &o.label, "");
                                choices[n] = on as usize;
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(&o.label);
                                    for (c, name) in o.choices.iter().enumerate() {
                                        if segment(ui, name, choices[n] == c) {
                                            choices[n] = c;
                                        }
                                    }
                                });
                            }
                        }
                        if choices != before {
                            let v = d.pack_options(&choices);
                            if let Some(s) = q.snap.as_mut() {
                                s.layout_options = v;
                            }
                            q.send(Job::LayoutOptions(v));
                        }
                    });
                    ui.add_space(10.0);
                }
            }

            card(ui, |ui| {
                ui.label(RichText::new("Reset").size(16.0).strong().color(Color32::WHITE));
                ui.horizontal_wrapped(|ui| {
                    let defn = qmk_driver(&self.devices[i]).and_then(|d| d.definition());
                    let q = self.devices[i].qmk.as_mut().unwrap();
                    if confirm_button(ui, q, "keymap", "Reset keymap to the firmware's default", connected) {
                        q.send(Job::ResetKeymap(defn));
                    }
                    if snap.as_ref().is_some_and(|s| s.macro_count > 0) && confirm_button(ui, q, "macros", "Clear all macros", connected) {
                        q.send(Job::ResetMacros);
                        q.macro_edit.iter_mut().for_each(String::clear);
                        if let Some(s) = q.snap.as_mut() {
                            s.macros.iter_mut().for_each(String::clear);
                        }
                    }
                    if confirm_button(ui, q, "boot", "Restart into bootloader", connected) {
                        q.send(Job::Bootloader(bootloader_fallback(info.vendor_id, info.product_id)));
                    }
                });
            });
        }
        if import {
            self.import_definition();
        }
        if save_backup {
            self.save_backup();
        }
        if load_backup {
            self.load_backup();
        }
    }

    fn save_backup(&mut self) {
        let i = self.selected;
        let Some(def) = qmk_driver(&self.devices[i]).and_then(|d| d.definition()) else { return };
        let Some(snap) = self.devices[i].qmk.as_ref().and_then(|q| q.snap.clone()) else { return };
        let name = format!("{}.layout.json", def.name.replace(['/', '\\', ':'], "-"));
        let Some(path) = rfd::FileDialog::new().add_filter("Keyboard layout", &["json"]).set_file_name(&name).save_file() else { return };
        let text = serde_json::to_string_pretty(&qmk::backup(&def, &snap)).unwrap_or_default();
        match std::fs::write(&path, text) {
            Ok(()) => self.set_status(format!("Saved to {}.", path.display()), true),
            Err(e) => self.set_status(format!("Couldn't save: {e}"), false),
        }
    }

    fn load_backup(&mut self) {
        let i = self.selected;
        let Some(def) = qmk_driver(&self.devices[i]).and_then(|d| d.definition()) else { return };
        let Some(snap) = self.devices[i].qmk.as_ref().and_then(|q| q.snap.clone()) else { return };
        let Some(path) = rfd::FileDialog::new().add_filter("Keyboard layout", &["json"]).pick_file() else { return };
        let job = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<qmk::Backup>(&t).map_err(|e| format!("Not a layout file: {e}")))
            .and_then(|b| qmk::restore_job(&def, &snap, &b));
        match job {
            Ok(job) => {
                if let Some(q) = self.devices[i].qmk.as_mut() {
                    q.send(job);
                    q.send(Job::Load(Some(def)));
                    q.loading = true;
                }
                self.set_status("Restoring…".into(), true);
            }
            Err(e) => self.set_status(e, false),
        }
    }
}

/// Keyboards whose firmware restarts into the bootloader on a raw HID message of its own (VIA
/// protocol 12 firmware no longer has a bootloader command).
fn bootloader_fallback(vid: u16, pid: u16) -> Option<Vec<u8>> {
    match (vid, pid) {
        // Cooler Master SK620 (github.com/clawntl/qmk-sk620): the message its flasher sends
        (0xFEED, 0x0620) => Some(b"\x5bSK620-BOOTLOADER".to_vec()),
        _ => None,
    }
}

/// A button that needs a second click.
fn confirm_button(ui: &mut Ui, q: &mut QmkState, id: &'static str, text: &str, enabled: bool) -> bool {
    let armed = q.confirm == Some(id);
    let label = if armed { format!("{text}: click again to confirm") } else { text.to_string() };
    let btn = egui::Button::new(RichText::new(label).color(if armed { Color32::BLACK } else { Color32::WHITE })).fill(if armed { BAD } else { KEYCAP });
    if ui.add_enabled(enabled, btn).clicked() {
        if armed {
            q.confirm = None;
            return true;
        }
        q.confirm = Some(id);
    }
    false
}

/// Shortens text to fit a key cap.
fn fit(s: &str, width: f32, font: f32) -> String {
    let max = ((width / (font * 0.55)).floor() as usize).max(2);
    if s.chars().count() > max {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

fn group_items(groups: &[&str], custom: &[CustomKeycode]) -> Vec<(u16, String, String)> {
    keycodes::all()
        .iter()
        .filter(|k| groups.contains(&k.group))
        .map(|k| (k.code, keycodes::cap(k.code, custom).0, k.short.to_string()))
        .collect()
}

/// A grid of keycode buttons; returns the one clicked.
fn key_grid(ui: &mut Ui, items: &[(u16, String, String)], current: u16) -> Option<u16> {
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (code, label, name) in items {
            let on = *code == current;
            let text = if label.is_empty() { name.clone() } else { label.clone() };
            let btn = egui::Button::new(RichText::new(text).size(12.5).color(if on { Color32::BLACK } else { Color32::WHITE }))
                .fill(if on { ACCENT } else { KEYCAP })
                .corner_radius(CornerRadius::same(6))
                .min_size(Vec2::new(46.0, 30.0));
            if ui.add(btn).on_hover_text(name).clicked() {
                out = Some(*code);
            }
        }
    });
    ui.add_space(4.0);
    out
}

/// A drop-down of plain keys (letters, numbers, editing keys, F-keys, modifiers).
fn basic_combo(ui: &mut Ui, id: &str, code: &mut u16) {
    if *code == 0 {
        *code = 0x04;
    }
    egui::ComboBox::from_id_salt(id)
        .selected_text(keycodes::cap(*code, &[]).0)
        .height(320.0)
        .width(140.0)
        .show_ui(ui, |ui| {
            for k in keycodes::all().iter().filter(|k| k.code <= 0xFF && (k.group == "basic" || k.group == "modifiers")) {
                let label = keycodes::cap(k.code, &[]).0;
                ui.selectable_value(code, k.code, format!("{label}   {}", k.short));
            }
        });
}

/// Draws one menu control; returns the new value bytes when changed.
fn control(ui: &mut Ui, c: &Control, cur: &[u8]) -> Option<Vec<u8>> {
    let mut out = None;
    match &c.kind {
        ControlKind::Range { min, max } => {
            let mut v = c.number(cur).clamp(*min, *max);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&c.label).color(TEXT_DIM));
                let r = ui.add(egui::Slider::new(&mut v, *min..=*max).show_value(true));
                if r.changed() {
                    out = Some(c.bytes(v));
                }
            });
        }
        ControlKind::Dropdown(opts) => {
            let v = c.number(cur);
            ui.label(RichText::new(&c.label).color(TEXT_DIM));
            if opts.len() > 6 {
                // effect lists: a grid of buttons
                let per_row = ((ui.available_width() / 170.0).floor() as usize).max(2);
                let w = (ui.available_width() - 6.0 * (per_row as f32 - 1.0)) / per_row as f32;
                for chunk in opts.chunks(per_row) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (label, val) in chunk {
                            let on = *val == v;
                            let btn = egui::Button::new(RichText::new(label).color(if on { Color32::BLACK } else { Color32::WHITE }))
                                .fill(if on { ACCENT } else { KEYCAP })
                                .corner_radius(CornerRadius::same(8))
                                .min_size(Vec2::new(w, 30.0));
                            if ui.add(btn).clicked() && !on {
                                out = Some(c.bytes(*val));
                            }
                        }
                    });
                }
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (label, val) in opts {
                        if segment(ui, label, *val == v) && *val != v {
                            out = Some(c.bytes(*val));
                        }
                    }
                });
            }
        }
        ControlKind::Toggle { off, on } => {
            let mut b = c.number(cur) == *on;
            toggle(ui, &mut b, &c.label, "");
            if b != (c.number(cur) == *on) {
                out = Some(c.bytes(if b { *on } else { *off }));
            }
        }
        ControlKind::Color => {
            let (mut h, mut s) = (cur.first().copied().unwrap_or(0), cur.get(1).copied().unwrap_or(255));
            ui.horizontal(|ui| {
                ui.label(RichText::new(&c.label).color(TEXT_DIM));
                let (rect, _) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 12.0, hsv(h, s));
            });
            ui.horizontal(|ui| {
                ui.label("Hue");
                let a = ui.add(egui::Slider::new(&mut h, 0..=255).show_value(false)).changed();
                ui.label("Saturation");
                let b = ui.add(egui::Slider::new(&mut s, 0..=255).show_value(false)).changed();
                if a || b {
                    out = Some(vec![h, s]);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (name, hh, ss) in [("Red", 0u8, 255u8), ("Orange", 21, 255), ("Yellow", 43, 255), ("Green", 85, 255), ("Cyan", 128, 255), ("Blue", 170, 255), ("Purple", 191, 255), ("Pink", 234, 200), ("White", 0, 0)] {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                    ui.painter().circle_filled(rect.center(), 10.0, hsv(hh, ss));
                    if (h, s) == (hh, ss) {
                        ui.painter().circle_stroke(rect.center(), 12.0, Stroke::new(2.0, Color32::WHITE));
                    }
                    if resp.on_hover_text(name).clicked() {
                        out = Some(vec![hh, ss]);
                    }
                }
            });
        }
        ControlKind::Keycode => {
            let v = c.number(cur) as u16;
            ui.horizontal(|ui| {
                ui.label(RichText::new(&c.label).color(TEXT_DIM));
                pill(ui, &keycodes::name(v), ACCENT);
            });
        }
    }
    out
}

/// QMK's hue/saturation (0–255 each) at full brightness.
fn hsv(h: u8, s: u8) -> Color32 {
    let hh = h as f32 / 255.0 * 6.0;
    let s = s as f32 / 255.0;
    let i = hh.floor() as i32 % 6;
    let f = hh - hh.floor();
    let (p, q, t) = (1.0 - s, 1.0 - s * f, 1.0 - s * (1.0 - f));
    let (r, g, b) = match i {
        0 => (1.0, t, p),
        1 => (q, 1.0, p),
        2 => (p, 1.0, t),
        3 => (p, q, 1.0),
        4 => (t, p, 1.0),
        _ => (1.0, p, q),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// The demo keyboard, shared between connections.
struct SharedFake(std::sync::Arc<std::sync::Mutex<qmk::via::FakeKeyboard>>);

impl qmk::via::Transport for SharedFake {
    fn exchange(&mut self, m: &[u8; qmk::via::MSG_LEN]) -> Result<[u8; qmk::via::MSG_LEN], String> {
        self.0.lock().map_err(|_| "demo keyboard broke".to_string())?.exchange(m)
    }
}
