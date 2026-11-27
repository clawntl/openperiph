//! The window around the pages: background, top bar with a tab per device, the home page of
//! device cards, a device's navigation panel, the About page and the pictures of the hardware.

use std::collections::HashMap;

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

use super::theme::{self, dim, glass, icon, page_title, small, title, ArtKey, Glass, Icon, ACCENT, BAD, GOOD, SECOND, TEXT, TEXT_DIM, TEXT_MUT, WARN};
use super::{progress_bar, OpenPeriphApp, Tab};
use crate::device::DeviceKind;
use crate::store;

/// What the window shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Home,
    Device,
    About,
}

/// Photos of devices that come with the app, loaded when first shown.
#[derive(Default)]
pub struct Pictures {
    loaded: HashMap<String, Option<egui::TextureHandle>>,
}

impl Pictures {
    /// A photo that comes with the app, for devices that have one.
    pub fn bundled(&mut self, ctx: &egui::Context, info: &crate::device::DeviceInfo) -> Option<egui::TextureHandle> {
        let (name, bytes): (&str, &'static [u8]) = match (info.id.as_str(), info.vendor_id, info.product_id) {
            ("attack-shark-r5-ultra", _, _) => ("attack-shark-r5-ultra", include_bytes!("../../assets/devices/attack-shark-r5-ultra.png")),
            ("ciy-x78", _, _) | (_, 0x258A, 0x0016) => ("ciy-x78", include_bytes!("../../assets/devices/ciy-x78.png")),
            (_, 0xFEED, 0x0620) => ("cooler-master-sk620", include_bytes!("../../assets/devices/cooler-master-sk620.png")),
            _ => return None,
        };
        let key = format!("bundled-{name}");
        self.loaded.entry(key.clone()).or_insert_with(|| theme::texture_from_bytes(ctx, &key, bytes)).clone()
    }
}

impl OpenPeriphApp {
    /// Reads the background choice and makes the background.
    pub(super) fn apply_background(&mut self, ctx: &egui::Context) {
        let choice = match self.prefs.background.as_str() {
            "smoke" => theme::BackgroundChoice::Smoke,
            "picture" => match store::picture("background") {
                Some(p) => theme::BackgroundChoice::Picture(p),
                None => theme::BackgroundChoice::Plain,
            },
            _ => theme::BackgroundChoice::Plain,
        };
        if let Err(e) = theme::set_background(ctx, &choice) {
            self.set_status(e, false);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Top bar

    pub(super) fn top_bar(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        ui.painter().add(glass(ui.ctx(), full.expand(1.0), 0.0, Glass::Panel));
        ui.painter().line_segment([full.left_bottom(), full.right_bottom()], Stroke::new(1.0, theme::LINE));
        let inner = full.shrink2(Vec2::new(14.0, 0.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
            // logo
            let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
            logo(ui.painter(), r);
            ui.label(RichText::new("OpenPeriph").size(14.0).family(theme::semibold()).color(TEXT));
            ui.add_space(14.0);
            if top_tab(ui, Icon::Home, "Home", self.view == View::Home).clicked() {
                self.view = View::Home;
            }
            for i in self.visible_devices() {
                let d = &self.devices[i];
                let kind = d.driver.info().kind;
                let name = d.driver.info().name.clone();
                let on = self.view == View::Device && self.selected == i;
                let ic = if kind == DeviceKind::Mouse { Icon::Mouse } else { Icon::Keyboard };
                let badge = if d.needs_access {
                    Some(WARN)
                } else if d.qmk.as_ref().is_some_and(|q| q.update_available().is_some() || q.flashing()) {
                    Some(ACCENT)
                } else {
                    None
                };
                let r = top_tab(ui, ic, &name, on);
                if let Some(c) = badge {
                    ui.painter().circle_filled(r.rect.min + Vec2::new(22.0, r.rect.height() / 2.0 - 7.0), 3.5, c);
                }
                if r.clicked() {
                    self.open_device(i);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Info, self.view == View::About).on_hover_text("About, appearance and supported devices").clicked() {
                    self.view = View::About;
                }
                if let Some(Ok(a)) = &self.app_release {
                    if a.newer() && self.app_update.is_none() {
                        let b = egui::Button::new(RichText::new(format!("Update to {}", a.release.tag)).size(12.0).color(Color32::BLACK))
                            .fill(ACCENT)
                            .corner_radius(CornerRadius::same(12));
                        if ui.add(b).clicked() {
                            self.app_popup = true;
                        }
                    }
                }
                if let Some((stage, f)) = &self.app_update {
                    ui.label(small(format!("Updating OpenPeriph: {stage} {}%", (f * 100.0) as u32)).color(ACCENT));
                }
            });
        });
    }

    pub(super) fn open_device(&mut self, i: usize) {
        if self.selected != i {
            self.selected = i;
            self.tab = Tab::Keys;
        }
        self.view = View::Device;
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        ui.painter().add(glass(ui.ctx(), full.expand(1.0), 0.0, Glass::Panel));
        ui.painter().line_segment([full.left_top(), full.right_top()], Stroke::new(1.0, theme::LINE));
        ui.scope_builder(UiBuilder::new().max_rect(full.shrink2(Vec2::new(16.0, 0.0))).layout(Layout::left_to_right(Align::Center)), |ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 3.5, if self.status_ok { GOOD } else { BAD });
            ui.label(RichText::new(&self.status).size(12.0).color(TEXT_DIM));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(small(format!("v{}", env!("CARGO_PKG_VERSION"))));
            });
        });
    }

    // -----------------------------------------------------------------------------------------
    // Home

    pub(super) fn home_page(&mut self, ui: &mut Ui) {
        let visible = self.visible_devices();
        ui.add_space(6.0);
        ui.label(page_title("Your devices"));
        ui.label(dim(match visible.len() {
            0 => "Nothing supported is plugged in.".to_string(),
            1 => "1 device connected. Pick it to change its settings.".to_string(),
            n => format!("{n} devices connected. Pick one to change its settings."),
        }));
        ui.add_space(14.0);
        if visible.is_empty() {
            theme::card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(18.0);
                    let (r, _) = ui.allocate_exact_size(Vec2::new(240.0, 110.0), Sense::hover());
                    let kb = Rect::from_min_size(r.min, Vec2::new(170.0, 70.0));
                    theme::keyboard_art(ui.painter(), kb.translate(Vec2::new(0.0, 20.0)), &generic_keys(), None, false);
                    theme::mouse_art(ui.painter(), Rect::from_min_size(r.min + Vec2::new(185.0, 10.0), Vec2::new(55.0, 95.0)), Color32::from_rgb(38, 40, 46));
                    ui.add_space(10.0);
                    ui.label(title("Plug in a keyboard or mouse"));
                    ui.label(dim("Supported devices show up here as soon as they're connected."));
                    ui.add_space(6.0);
                    if ui.button("See the supported devices").clicked() {
                        self.view = View::About;
                    }
                    ui.add_space(14.0);
                });
            });
            return;
        }
        let gap = 14.0;
        let avail = ui.available_width();
        let per_row = ((avail + gap) / (300.0 + gap)).floor().max(1.0) as usize;
        let w = (avail - gap * (per_row as f32 - 1.0)) / per_row as f32;
        let mut open = None;
        for row in visible.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for &i in row {
                    if self.device_card(ui, i, w) {
                        open = Some(i);
                    }
                }
            });
            ui.add_space(gap);
        }
        if let Some(i) = open {
            self.open_device(i);
        }
    }

    fn device_card(&mut self, ui: &mut Ui, i: usize, w: f32) -> bool {
        let h = 250.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        p.add(glass(&ctx, rect, 14.0, Glass::Card));
        if resp.hovered() {
            p.rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.0, ACCENT.gamma_multiply(0.7)), StrokeKind::Inside);
        }
        // soft glow behind the device
        let art = Rect::from_min_max(rect.min + Vec2::new(18.0, 16.0), Pos2::new(rect.max.x - 18.0, rect.min.y + 170.0));
        p.circle_filled(art.center(), art.height() * 0.42, ACCENT.gamma_multiply(0.05));
        self.device_art(&ctx, &p, i, art, false);
        let d = &self.devices[i];
        let info = d.driver.info();
        let y = rect.min.y + 186.0;
        p.text(Pos2::new(rect.min.x + 18.0, y), Align2::LEFT_TOP, &info.name, FontId::new(15.0, theme::semibold()), TEXT);
        let what = if d.driver.capabilities().qmk { "QMK keyboard".to_string() } else { info.kind.name().to_string() };
        p.text(Pos2::new(rect.min.x + 18.0, y + 22.0), Align2::LEFT_TOP, format!("{} · {what}", info.maker), FontId::proportional(12.0), TEXT_DIM);
        let (txt, col) = self.device_state(i);
        let g = p.layout_no_wrap(txt.to_string(), FontId::proportional(11.5), col);
        let chip = Rect::from_min_size(Pos2::new(rect.min.x + 18.0, y + 42.0), g.size() + Vec2::new(20.0, 6.0));
        p.rect_filled(chip, CornerRadius::same(9), col.gamma_multiply(0.14));
        p.circle_filled(Pos2::new(chip.min.x + 8.0, chip.center().y), 2.5, col);
        p.galley(Pos2::new(chip.min.x + 14.0, chip.min.y + 3.0), g, col);
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    }

    /// A short state for a device: connected, needs access, firmware update…
    pub(super) fn device_state(&self, i: usize) -> (&'static str, Color32) {
        let d = &self.devices[i];
        if d.needs_access {
            return ("Needs access", WARN);
        }
        if let Some(q) = &d.qmk {
            if q.flashing() {
                return ("Updating firmware…", ACCENT);
            }
            if q.update_available().is_some() {
                return ("Firmware update available", ACCENT);
            }
        }
        if d.receiver_only {
            return ("Off or asleep: move it to wake it", WARN);
        }
        if d.dirty() {
            return ("Changes not sent yet", WARN);
        }
        match d.connected {
            Some(true) => ("Connected", GOOD),
            Some(false) => ("Not plugged in", TEXT_MUT),
            None => ("Looking…", TEXT_MUT),
        }
    }

    // -----------------------------------------------------------------------------------------
    // A device: navigation on the left, the page on the right

    pub(super) fn device_view(&mut self, ui: &mut Ui) {
        self.fix_tab();
        let full = ui.max_rect();
        let nav_w = 212.0;
        let nav = Rect::from_min_size(full.min, Vec2::new(nav_w, full.height()));
        let content = Rect::from_min_max(Pos2::new(nav.max.x + 16.0, full.min.y), full.max);
        ui.painter().add(glass(ui.ctx(), nav, 14.0, Glass::Panel));
        ui.scope_builder(UiBuilder::new().max_rect(nav.shrink(10.0)), |ui| self.nav(ui));
        ui.scope_builder(UiBuilder::new().max_rect(content), |ui| {
            self.page_header(ui);
            let busy = self.devices[self.selected].qmk.as_ref().and_then(|q| q.fw_progress());
            if let (Some((stage, f)), false) = (busy, self.tab == Tab::Settings) {
                theme::card(ui, |ui| {
                    progress_bar(ui, f, &format!("Updating the firmware: {stage}"));
                    ui.label(RichText::new("Don't unplug the keyboard.").color(WARN));
                });
                ui.add_space(8.0);
            }
            let qmk = self.devices[self.selected].qmk.is_some();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(2.0);
                match (qmk, self.tab) {
                    (true, tab) => self.qmk_page(ui, tab),
                    (_, Tab::Keys) => self.keys_page(ui),
                    (_, Tab::Lighting) => self.lighting_page(ui),
                    (_, Tab::Mouse) => self.mouse_page(ui),
                    (_, Tab::Settings) => self.device_settings_page(ui),
                    _ => {}
                }
                ui.add_space(12.0);
            });
        });
    }

    /// The pages a device has, with their names.
    pub(super) fn nav_items(&self) -> Vec<(Tab, &'static str)> {
        let caps = self.devices[self.selected].driver.capabilities();
        if caps.qmk {
            return self.qmk_tabs_available().into_iter().map(|(t, n)| (t, if t == Tab::Settings { "Device" } else { n })).collect();
        }
        let mut v = Vec::new();
        if caps.key_remap {
            v.push((Tab::Keys, "Keys"));
        }
        if caps.lighting {
            v.push((Tab::Lighting, "Lighting"));
        }
        if caps.mouse {
            v.push((Tab::Mouse, "Performance"));
        }
        v.push((Tab::Settings, "Device"));
        v
    }

    fn nav(&mut self, ui: &mut Ui) {
        let i = self.selected;
        let ctx = ui.ctx().clone();
        let (art, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 112.0), Sense::hover());
        let p = ui.painter().clone();
        self.device_art(&ctx, &p, i, art.shrink2(Vec2::new(6.0, 8.0)), false);
        let d = &self.devices[i];
        let info = d.driver.info().clone();
        ui.add_space(4.0);
        ui.label(RichText::new(&info.name).size(14.0).family(theme::semibold()).color(TEXT));
        let what = if d.driver.capabilities().qmk { "QMK keyboard" } else { info.kind.name() };
        ui.label(small(format!("{what} · {:04X}:{:04X}", info.vendor_id, info.product_id)));
        let (txt, col) = self.device_state(i);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 3.0, col);
            ui.label(RichText::new(txt).size(11.5).color(col));
        });
        ui.add_space(8.0);
        let (sep, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(sep, CornerRadius::ZERO, theme::LINE);
        ui.add_space(8.0);
        for (t, name) in self.nav_items() {
            let ic = match t {
                Tab::Keys => Icon::Keymap,
                Tab::Lighting => Icon::Lighting,
                Tab::Macros => Icon::Macros,
                Tab::Mouse => Icon::Gauge,
                Tab::Settings => Icon::Device,
            };
            if nav_item(ui, ic, name, self.tab == t) {
                self.tab = t;
            }
        }
    }

    /// The page's title, with the device's state and actions on the right.
    fn page_header(&mut self, ui: &mut Ui) {
        let name = self.nav_items().into_iter().find(|(t, _)| *t == self.tab).map(|(_, n)| n).unwrap_or("");
        ui.horizontal(|ui| {
            ui.set_min_height(36.0);
            ui.label(page_title(name));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.header_actions(ui));
        });
        ui.add_space(8.0);
    }

    // -----------------------------------------------------------------------------------------
    // "About this device": the picture, its name and a few facts as tiles

    /// Returns true when `action` (a small button under the tiles) was clicked.
    pub(super) fn about_device_card(&mut self, ui: &mut Ui, tiles: &[(&str, String)], action: Option<&str>) -> bool {
        let i = self.selected;
        let info = self.devices[i].driver.info().clone();
        let ctx = ui.ctx().clone();
        let mut clicked = false;
        let what = if self.devices[i].driver.capabilities().qmk { "QMK keyboard" } else { info.kind.name() };
        theme::card(ui, |ui| {
            let wide = ui.available_width() > 640.0;
            let mut body = |ui: &mut Ui| {
                ui.label(RichText::new(&info.name).size(18.0).family(theme::semibold()).color(TEXT));
                ui.label(dim(format!("{} · {what}", info.maker)));
                ui.add_space(10.0);
                tile_grid(ui, tiles);
                if let Some(a) = action {
                    ui.add_space(8.0);
                    clicked = ui.button(a).clicked();
                }
            };
            // the picture first (needs self mutably), then the text beside or under it
            let size = if wide { Vec2::new(260.0, 150.0) } else { Vec2::new(ui.available_width(), 130.0) };
            if wide {
                ui.horizontal_top(|ui| {
                    let (art, _) = ui.allocate_exact_size(size, Sense::hover());
                    let p = ui.painter().clone();
                    p.circle_filled(art.center(), 70.0, ACCENT.gamma_multiply(0.04));
                    self.device_art(&ctx, &p, i, art.shrink(8.0), true);
                    ui.add_space(14.0);
                    ui.vertical(|ui| body(ui));
                });
            } else {
                let (art, _) = ui.allocate_exact_size(size, Sense::hover());
                let p = ui.painter().clone();
                self.device_art(&ctx, &p, i, art.shrink(8.0), true);
                body(ui);
            }
        });
        ui.add_space(10.0);
        clicked
    }

    /// The Device page for devices without firmware settings.
    pub(super) fn device_settings_page(&mut self, ui: &mut Ui) {
        let d = &self.devices[self.selected];
        let info = d.driver.info().clone();
        let connected = d.connected == Some(true);
        let st = d.mouse_status.clone();
        let mouse = d.driver.capabilities().mouse;
        let receiver_only = d.receiver_only;
        let mut tiles: Vec<(&str, String)> = vec![("USB id", format!("{:04X}:{:04X}", info.vendor_id, info.product_id))];
        tiles.push((
            "Connection",
            match (connected, mouse, st.wired) {
                (false, _, _) if receiver_only => "Receiver only (mouse off)".into(),
                (false, _, _) => "Not plugged in".into(),
                (true, true, true) => "Cable".into(),
                (true, true, false) => "Receiver".into(),
                (true, false, _) => "USB".into(),
            },
        ));
        if mouse {
            if let Some(b) = st.battery {
                tiles.push(("Battery", if st.charging { format!("{b}% · charging") } else { format!("{b}%") }));
            }
        }
        if let Some(fw) = st.firmware.clone() {
            tiles.push(("Firmware", fw));
        }
        self.about_device_card(ui, &tiles, None);
    }

    // -----------------------------------------------------------------------------------------
    // Pictures of the hardware

    /// The device's photo if the app has one, otherwise a drawing of it.
    pub(super) fn device_art(&mut self, ctx: &egui::Context, p: &egui::Painter, i: usize, rect: Rect, legends: bool) {
        let info = self.devices[i].driver.info().clone();
        if let Some(tex) = self.pictures.bundled(ctx, &info) {
            theme::photo(p, rect, &tex);
            return;
        }
        let d = &self.devices[i];
        if d.driver.info().kind == DeviceKind::Mouse {
            theme::mouse_art(p, rect, Color32::from_rgb(34, 36, 41));
            return;
        }
        let (keys, glow) = self.keyboard_keys(i);
        theme::keyboard_art(p, rect, &keys, glow, legends);
    }

    fn keyboard_keys(&self, i: usize) -> (Vec<ArtKey>, Option<Color32>) {
        let d = &self.devices[i];
        if let Some(q) = d.driver.as_qmk() {
            let snap = d.qmk.as_ref().and_then(|s| s.snap.as_ref());
            let glow = snap.and_then(|s| {
                let effect = s.values.get(&(3, 2)).and_then(|v| v.first().copied()).unwrap_or(0);
                let colour = s.values.get(&(3, 4))?;
                (effect != 0).then(|| super::qmk_ui::hsv_colour(colour[0], *colour.get(1).unwrap_or(&255)))
            });
            if let Some(def) = q.definition() {
                let choices = snap.map(|s| def.unpack_options(s.layout_options)).unwrap_or_else(|| def.unpack_options(0));
                let custom = def.custom_keycodes.clone();
                let keys = def
                    .visible_keys(&choices)
                    .into_iter()
                    .filter(|k| !k.encoder)
                    .map(|k| {
                        let code = snap.and_then(|s| s.keymap.first()).and_then(|l| l.get(k.row as usize)).and_then(|r| r.get(k.col as usize)).copied();
                        ArtKey {
                            rect: Rect::from_min_size(Pos2::new(k.x, k.y), Vec2::new(k.w, k.h)),
                            second: k.second.map(|s| Rect::from_min_size(Pos2::new(k.x + s[0], k.y + s[1]), Vec2::new(s[2], s[3]))),
                            label: code.map(|c| crate::qmk::keycodes::cap(c, &custom).0).unwrap_or_default(),
                        }
                    })
                    .collect();
                return (keys, glow);
            }
            return (generic_keys(), glow);
        }
        let layout = d.driver.layout();
        if layout.keys.is_empty() {
            return (generic_keys(), None);
        }
        let keys = layout
            .keys
            .iter()
            .map(|k| ArtKey {
                rect: Rect::from_min_max(Pos2::new(k.rect[0], k.rect[1]), Pos2::new(k.rect[2], k.rect[3])),
                second: k.second.map(|s| Rect::from_min_max(Pos2::new(s[0], s[1]), Pos2::new(s[2], s[3]))),
                label: k.label.to_string(),
            })
            .collect();
        (keys, None)
    }

    // -----------------------------------------------------------------------------------------
    // About

    pub(super) fn about_page(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        ui.label(page_title("About OpenPeriph"));
        ui.label(dim(format!("Version {} · settings for your keyboards and mice, without the vendor software", env!("CARGO_PKG_VERSION"))));
        ui.add_space(14.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |cols| {
                theme::card(&mut cols[0], |ui| {
                    ui.label(title("Updates"));
                    ui.add_space(2.0);
                    match &self.app_release {
                        Some(Ok(a)) if a.newer() => {
                            ui.label(RichText::new(format!("{} is available.", a.release.tag)).color(ACCENT));
                        }
                        Some(Ok(_)) => {
                            ui.label(RichText::new("You have the latest version.").color(GOOD));
                        }
                        Some(Err(e)) => {
                            ui.label(dim(format!("Couldn't check: {e}")));
                        }
                        None => {
                            ui.label(dim("Checking…"));
                        }
                    }
                    ui.add_space(4.0);
                    self.app_update_box(ui);
                    ui.add_space(4.0);
                    ui.hyperlink_to("github.com/clawntl/openperiph", "https://github.com/clawntl/openperiph");
                });
                theme::card(&mut cols[1], |ui| self.appearance(ui));
            });
            ui.add_space(12.0);
            theme::card(ui, |ui| {
                ui.label(title("Supported devices"));
                ui.label(dim("Only the ones that are plugged in appear in the top bar."));
                ui.add_space(8.0);
                let ctx = ui.ctx().clone();
                egui::Grid::new("supported").num_columns(5).spacing([20.0, 6.0]).min_row_height(46.0).show(ui, |ui| {
                    for h in ["", "Device", "Type", "USB id", "Status"] {
                        ui.label(small(h));
                    }
                    ui.end_row();
                    for i in 0..self.devices.len() {
                        let (art, _) = ui.allocate_exact_size(Vec2::new(84.0, 44.0), Sense::hover());
                        let p = ui.painter().clone();
                        self.device_art(&ctx, &p, i, art.shrink(2.0), false);
                        let d = &self.devices[i];
                        let info = d.driver.info();
                        let demo = d.driver.as_qmk().is_some_and(|q| q.demo);
                        ui.label(RichText::new(format!("{}{}", info.name, if demo { " (simulated)" } else { "" })).color(TEXT));
                        ui.label(dim(if d.driver.capabilities().qmk { "Keyboard · QMK/VIA".to_string() } else { info.kind.name().to_string() }));
                        ui.label(dim(format!("{:04X}:{:04X}", info.vendor_id, info.product_id)));
                        let (t, c) = match d.connected {
                            Some(true) => ("Connected", GOOD),
                            Some(false) if d.receiver_only => ("Receiver only: mouse off or asleep", WARN),
                            Some(false) => ("Not plugged in", TEXT_MUT),
                            None => ("Looking…", TEXT_MUT),
                        };
                        ui.label(RichText::new(t).color(c));
                        ui.end_row();
                    }
                    let (art, _) = ui.allocate_exact_size(Vec2::new(84.0, 44.0), Sense::hover());
                    theme::keyboard_art(ui.painter(), art.shrink(2.0), &generic_keys(), None, false);
                    ui.label(RichText::new("Any other QMK keyboard with VIA").color(TEXT));
                    ui.label(dim("Keyboard · QMK/VIA"));
                    ui.label(dim("any"));
                    ui.label(RichText::new("Found when plugged in").color(TEXT_MUT));
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(small(
                    "Other QMK keyboards need their VIA definition for the keymap (import it on the keyboard's Device page); lighting and macros work without it.",
                ));
            });
        });
    }

    fn appearance(&mut self, ui: &mut Ui) {
        ui.label(title("Appearance"));
        ui.label(dim("What's behind the panels. With smoke or a picture, the panels are see-through."));
        ui.add_space(6.0);
        let cur = match self.prefs.background.as_str() {
            "smoke" => 1,
            "picture" => 2,
            _ => 0,
        };
        let mut choice = None;
        ui.horizontal(|ui| {
            if super::segment(ui, "Plain", cur == 0) {
                choice = Some("plain");
            }
            if super::segment(ui, "Smoke", cur == 1) {
                choice = Some("smoke");
            }
            if super::segment(ui, "Your picture…", cur == 2) {
                choice = Some("picture");
            }
        });
        ui.add_space(2.0);
        ui.label(small("Your picture is darkened a little so text stays easy to read."));
        if let Some(c) = choice {
            if c == "picture" {
                let Some(path) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "webp"]).set_title("Background picture").pick_file() else {
                    return;
                };
                if let Err(e) = store::keep_picture("background", &path) {
                    self.set_status(e, false);
                    return;
                }
            }
            self.prefs.background = c.to_string();
            if let Err(e) = store::save_prefs(&self.prefs) {
                self.set_status(format!("Couldn't save the choice: {e}"), false);
            }
            let ctx = ui.ctx().clone();
            self.apply_background(&ctx);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Widgets

fn logo(p: &egui::Painter, r: Rect) {
    // the same as the app icon: a ring from green (top left) to blue (bottom right), and a dot
    let c = r.center();
    let rad = r.width() * 0.36;
    let n = 48;
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let mid = Vec2::angled((a0 + a1) / 2.0);
        let t = ((mid.x + mid.y) / 2.0_f32.sqrt() * 0.5 + 0.5).clamp(0.0, 1.0);
        let col = theme::lerp_colour(ACCENT, SECOND, t);
        p.line_segment([c + Vec2::angled(a0) * rad, c + Vec2::angled(a1) * rad], Stroke::new(r.width() * 0.17, col));
    }
    let dot = c + Vec2::angled(-std::f32::consts::FRAC_PI_4) * rad;
    p.circle_filled(dot, r.width() * 0.17, theme::BG1);
    p.circle_filled(dot, r.width() * 0.11, Color32::WHITE);
}

/// A tab in the top bar: icon and name, underlined when open.
fn top_tab(ui: &mut Ui, ic: Icon, text: &str, on: bool) -> egui::Response {
    let font = FontId::proportional(12.5);
    let colour = if on { TEXT } else { TEXT_DIM };
    let g = ui.painter().layout_no_wrap(text.to_string(), font, colour);
    let size = Vec2::new(g.size().x + 40.0, ui.available_height().max(30.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if resp.hovered() && !on {
        p.rect_filled(rect.shrink2(Vec2::new(0.0, 8.0)), CornerRadius::same(7), Color32::from_white_alpha(8));
    }
    let col = if on {
        ACCENT
    } else if resp.hovered() {
        TEXT
    } else {
        TEXT_DIM
    };
    icon(p, Rect::from_center_size(Pos2::new(rect.min.x + 16.0, rect.center().y), Vec2::splat(14.0)), ic, col);
    p.galley(Pos2::new(rect.min.x + 30.0, rect.center().y - g.size().y / 2.0), g, if resp.hovered() { TEXT } else { colour });
    if on {
        p.rect_filled(Rect::from_min_max(Pos2::new(rect.min.x + 6.0, rect.max.y - 2.0), Pos2::new(rect.max.x - 6.0, rect.max.y)), CornerRadius::same(1), ACCENT);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn icon_button(ui: &mut Ui, ic: Icon, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() || on {
        p.rect_filled(rect, CornerRadius::same(8), if on { ACCENT.gamma_multiply(0.16) } else { Color32::from_white_alpha(10) });
    }
    icon(
        p,
        rect.shrink(7.0),
        ic,
        if on {
            ACCENT
        } else if resp.hovered() {
            TEXT
        } else {
            TEXT_DIM
        },
    );
    resp
}

/// A row in a device's navigation.
fn nav_item(ui: &mut Ui, ic: Icon, text: &str, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, CornerRadius::same(8), ACCENT.gamma_multiply(0.12));
        p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(0.0, 7.0), Vec2::new(2.5, rect.height() - 14.0)), CornerRadius::same(1), ACCENT);
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), Color32::from_white_alpha(8));
    }
    let col = if on { ACCENT } else { TEXT_DIM };
    icon(p, Rect::from_center_size(Pos2::new(rect.min.x + 20.0, rect.center().y), Vec2::splat(15.0)), ic, col);
    p.text(
        Pos2::new(rect.min.x + 38.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.0),
        if on {
            TEXT
        } else if resp.hovered() {
            TEXT
        } else {
            TEXT_DIM
        },
    );
    ui.add_space(2.0);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// A 65% board, for keyboards whose layout isn't known.
fn generic_keys() -> Vec<ArtKey> {
    let rows: [&[f32]; 5] = [
        &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0],
        &[1.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.5, 1.0],
        &[1.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.25, 1.0],
        &[2.25, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.75, 1.0, 1.0],
        &[1.25, 1.25, 1.25, 6.25, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    ];
    let mut keys = Vec::new();
    for (y, row) in rows.iter().enumerate() {
        let mut x = 0.0;
        for &w in row.iter() {
            keys.push(ArtKey { rect: Rect::from_min_size(Pos2::new(x, y as f32), Vec2::new(w, 1.0)), second: None, label: String::new() });
            x += w;
        }
    }
    keys
}

/// Facts as tiles: a small caption over a value, two to four to a row.
fn tile_grid(ui: &mut Ui, tiles: &[(&str, String)]) {
    let gap = 8.0;
    let avail = ui.available_width();
    let per_row = ((avail + gap) / (150.0 + gap)).floor().clamp(1.0, 4.0) as usize;
    let w = (avail - gap * (per_row as f32 - 1.0)) / per_row as f32;
    for row in tiles.chunks(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (caption, value) in row {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 54.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(r, CornerRadius::same(9), Color32::from_white_alpha(6));
                p.rect_stroke(r, CornerRadius::same(9), Stroke::new(1.0, Color32::from_white_alpha(12)), StrokeKind::Inside);
                p.text(r.min + Vec2::new(12.0, 10.0), Align2::LEFT_TOP, caption.to_uppercase(), FontId::proportional(10.5), TEXT_MUT);
                let font = FontId::new(14.0, theme::semibold());
                let g = p.layout_no_wrap(value.clone(), font.clone(), TEXT);
                let shown = if g.size().x > w - 24.0 {
                    resp.on_hover_text(value.as_str());
                    let mut t: String = value.chars().collect();
                    while !t.is_empty() && p.layout_no_wrap(format!("{t}…"), font.clone(), TEXT).size().x > w - 24.0 {
                        t.pop();
                    }
                    format!("{t}…")
                } else {
                    value.clone()
                };
                p.text(r.min + Vec2::new(12.0, 28.0), Align2::LEFT_TOP, shown, font, TEXT);
            }
        });
        ui.add_space(gap - 6.0);
    }
}
