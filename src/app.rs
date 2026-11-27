//! The window: a device list on the left, and for the chosen device a "Keys" page (click a key,
//! pick what it does) and a "Lighting" page (effect, colour, speed, brightness). QMK keyboards
//! get their own pages (keymap, lighting, macros, settings) from `qmk_ui`.

mod qmk_ui;
mod shell;
mod theme;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::device::{Driver, EffectSettings, KeySettings, LightingState, MouseState, MouseStatus, Presence};
use crate::keycodes::{KeyAction, MouseAction, KEY_GROUPS, MEDIA, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};
use crate::selfupdate::{self, AppRelease, InstallKind};
use crate::{drivers, hid, qmk, store};

use shell::View;
use theme::{card, ACCENT, BAD, GOOD, KEYCAP, TEXT_DIM, WARN};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Keys,
    Lighting,
    Mouse,
    Macros,
    Settings,
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
    const ALL: [(Kind, &'static str); 6] =
        [(Kind::Default, "Default"), (Kind::Key, "Another key"), (Kind::Shortcut, "Shortcut"), (Kind::Mouse, "Mouse"), (Kind::Media, "Media"), (Kind::Disabled, "Off")];
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
    /// Device index, plugged in, needs permission (Linux).
    Detected(usize, Presence, bool),
    /// QMK keyboards found on the system.
    Discovered(Vec<hid::Collection>),
    Qmk(usize, qmk::Event),
    Firmware(usize, qmk_ui::FwMsg),
    /// Updating OpenPeriph itself.
    AppLatest(Result<AppRelease, String>),
    AppProgress(String, f32),
    AppInstalled(Result<PathBuf, String>),
    Access(Result<(), String>),
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
    /// Plugged in, but this user may not open it yet (Linux, no udev rule).
    needs_access: bool,
    /// A wireless device whose receiver is plugged in but which doesn't answer (off or asleep).
    receiver_only: bool,
    qmk: Option<qmk_ui::QmkState>,
}

impl DeviceEntry {
    fn new(driver: Arc<dyn Driver>) -> DeviceEntry {
        let saved = store::load(&driver.info().id);
        let lighting = saved.lighting.clone().unwrap_or_else(|| driver.default_lighting());
        let mouse = saved.mouse.clone().or_else(|| driver.default_mouse());
        let qmk = driver.capabilities().qmk.then(qmk_ui::QmkState::default);
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
            needs_access: false,
            receiver_only: false,
            qmk,
        }
    }

    fn dirty(&self) -> bool {
        self.keys_dirty || self.lighting_dirty || self.mouse_dirty
    }
}

pub struct OpenPeriphApp {
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
    discovering: bool,
    app_release: Option<Result<AppRelease, String>>,
    app_update: Option<(String, f32)>,
    install_kind: InstallKind,
    /// Home, a device, or About.
    view: View,
    prefs: store::Prefs,
    pictures: shell::Pictures,
    /// Checks only: a device shown as plugged in.
    force_connected: Option<usize>,
    /// Ask about a new OpenPeriph release (once per run).
    app_popup: bool,
    /// Ask about new firmware for this device (once per run).
    fw_popup: Option<usize>,
    /// Hidden option for checks (`--page`).
    page: String,
}

impl OpenPeriphApp {
    /// `page` (a hidden option for checks): "lighting", or "mouse" to open the first mouse.
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, page: Option<String>, demo: bool) -> Self {
        theme::install(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut devices: Vec<DeviceEntry> = drivers::all().into_iter().map(DeviceEntry::new).collect();
        if demo {
            devices.push(DeviceEntry::new(qmk_ui::demo_driver()));
        }
        let page = page.unwrap_or_default().to_ascii_lowercase();
        // With --demo, checks open on the simulated QMK keyboard.
        let first = if demo { devices.len() - 1 } else { 0 };
        let (selected, tab) = match page.as_str() {
            "mouse" | "mouse-device" => {
                (devices.iter().position(|d| d.driver.capabilities().mouse).unwrap_or(0), if page == "mouse" { Tab::Mouse } else { Tab::Settings })
            }
            "x78" | "x78-lighting" | "x78-any" => (
                devices.iter().position(|d| d.driver.capabilities().key_remap && !d.driver.capabilities().qmk).unwrap_or(0),
                if page == "x78" { Tab::Keys } else { Tab::Lighting },
            ),
            "lighting" | "colour" => (first, Tab::Lighting),
            "macros" => (first, Tab::Macros),
            "settings" | "update" | "flashing" => (first, Tab::Settings),
            _ => (first, Tab::Keys),
        };
        let view = match page.as_str() {
            "about" => View::About,
            "home" | "" => View::Home,
            _ => View::Device,
        };
        let force_connected = matches!(page.as_str(), "mouse" | "mouse-device" | "x78" | "x78-lighting" | "x78-any").then_some(selected);
        OpenPeriphApp {
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
            discovering: false,
            app_release: None,
            app_update: None,
            install_kind: selfupdate::install_kind(),
            view,
            prefs: store::prefs(),
            pictures: shell::Pictures::default(),
            force_connected,
            app_popup: false,
            fw_popup: None,
            page,
        }
        .start_app_check(&cc.egui_ctx)
        .with_background(&cc.egui_ctx)
    }

    fn with_background(mut self, ctx: &egui::Context) -> Self {
        if self.page == "x78-any" {
            let d = &mut self.devices[self.selected];
            if let Some(i) = d.driver.effects().iter().position(|e| e.any_colour) {
                d.lighting.effect = i;
            }
        }
        self.apply_background(ctx);
        self
    }

    /// Cleans up after an earlier update and looks for a newer OpenPeriph.
    fn start_app_check(mut self, ctx: &egui::Context) -> Self {
        if self.screenshot_mode() {
            // no network in checks: show what an available update looks like
            // pretend there's an update only for the check that shows it
            let fake = self.page == "app-update";
            let tag = if fake { "v9.9.9".to_string() } else { format!("v{}", env!("CARGO_PKG_VERSION")) };
            let release = crate::net::GhRelease { tag, ..Default::default() };
            self.app_release = Some(Ok(AppRelease { release, version: if fake { 0x090909 } else { 0 } }));
            self.app_popup = self.page == "app-update";
            return self;
        }
        selfupdate::clean_up();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::AppLatest(selfupdate::latest()));
            ctx.request_repaint();
        });
        self
    }

    fn screenshot_mode(&self) -> bool {
        self.screenshot.is_some()
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Detected(i, presence, access) => {
                    self.detecting = false;
                    let d = &mut self.devices[i];
                    let was = d.connected;
                    let c = presence == Presence::Connected || (self.screenshot.is_some() && self.force_connected == Some(i));
                    let was_receiver_only = d.receiver_only;
                    d.receiver_only = presence == Presence::ReceiverOnly && !c;
                    d.connected = Some(c);
                    d.needs_access = access;
                    if d.qmk.is_some() {
                        self.qmk_connection_changed(i, ctx);
                        let d = &self.devices[i];
                        if was == Some(true) && !c {
                            let name = d.driver.info().name.clone();
                            self.set_status(format!("{name} was unplugged."), false);
                        }
                        continue;
                    }
                    if c && !d.read_once && !self.screenshot.is_some() && !self.busy && !access {
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
                    let name = d.driver.info().name.clone();
                    if was == Some(true) && !c {
                        if d.receiver_only {
                            self.set_status(format!("{name} is switched off or asleep (its receiver is still plugged in)."), false);
                        } else {
                            self.set_status(format!("{name} was unplugged."), false);
                        }
                    } else if was_receiver_only && c {
                        self.set_status(format!("{name} is back."), true);
                    }
                }
                Msg::Lighting(i, r) => match r {
                    Ok(mut l) => {
                        let d = &mut self.devices[i];
                        // The keyboard can't report an app-made colour: keep ours, and show
                        // "Any colour" again if that's what the keyboard is still set to.
                        l.any = d.lighting.any;
                        let effects = d.driver.effects();
                        if let Some(any) = effects.iter().position(|e| e.any_colour) {
                            let shows_group = effects.get(l.effect).is_some_and(|e| e.groups > 0);
                            if d.lighting.effect == any && shows_group && l.group == 4 {
                                l.effect = any;
                            }
                        }
                        if !d.lighting_dirty {
                            d.lighting = l;
                        }
                        let name = d.driver.info().name.clone();
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
                        let name = d.driver.info().name.clone();
                        self.set_status(format!("Read the current settings from {name}."), true);
                    }
                    Err(e) => self.set_status(e, false),
                },
                Msg::Progress(p) => self.status = p,
                Msg::Discovered(found) => {
                    self.discovering = false;
                    self.add_discovered(found);
                }
                Msg::Qmk(i, e) => self.qmk_event(i, e),
                Msg::Firmware(i, m) => self.firmware_msg(i, m),
                Msg::AppLatest(r) => {
                    if let Ok(a) = &r {
                        if a.newer() {
                            self.set_status(format!("OpenPeriph {} is available.", a.release.tag), true);
                            self.app_popup = true;
                        }
                    }
                    self.app_release = Some(r);
                }
                Msg::AppProgress(stage, f) => {
                    self.status = format!("Updating OpenPeriph: {stage} {}%", (f * 100.0) as u32);
                    self.app_update = Some((stage, f));
                }
                Msg::AppInstalled(r) => {
                    self.app_update = None;
                    match r {
                        Ok(program) => selfupdate::restart(&program),
                        Err(e) => self.set_status(format!("OpenPeriph wasn't updated: {e}"), false),
                    }
                }
                Msg::Access(r) => match r {
                    Ok(()) => self.set_status("Access granted. Unplug the device and plug it back in.".into(), true),
                    Err(e) => self.set_status(e, false),
                },
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
                    let presence = driver.presence();
                    let access = presence != Presence::Absent && needs_access(&driver.usb_ids());
                    let _ = tx.send(Msg::Detected(i, presence, access));
                    ctx.request_repaint();
                });
            }
            if !self.discovering && !self.screenshot_mode() {
                self.discovering = true;
                let tx = self.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Discovered(qmk::scan()));
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
        let saved = store::Saved { keys: d.keys.clone(), lighting: Some(d.lighting.clone()), mouse: d.mouse.clone() };
        if !self.screenshot_mode() {
            let _ = store::save(&d.driver.info().id, &saved);
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

    /// Devices to list: the ones plugged in.
    /// Devices to show: the ones plugged in, plus a wireless one that went to sleep while its
    /// page is open (so the page doesn't vanish under you).
    fn visible_devices(&self) -> Vec<usize> {
        (0..self.devices.len())
            .filter(|&i| {
                let d = &self.devices[i];
                d.connected == Some(true) || (d.receiver_only && self.view == View::Device && self.selected == i)
            })
            .collect()
    }

    /// Starts updating OpenPeriph (or opens the release page when it can't update itself).
    fn start_app_update(&mut self, ctx: &egui::Context) {
        let Some(Ok(a)) = &self.app_release else {
            return;
        };
        let a = a.clone();
        if self.install_kind == InstallKind::Manual {
            ctx.open_url(egui::OpenUrl::new_tab(&a.release.page_url));
            return;
        }
        self.app_update = Some(("Starting".into(), 0.0));
        let kind = self.install_kind.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let report = |stage: &str, f: f32| {
                let _ = tx.send(Msg::AppProgress(stage.to_string(), f));
                ctx.request_repaint();
            };
            let r = selfupdate::install(&a, &kind, &report);
            let _ = tx.send(Msg::AppInstalled(r));
            ctx.request_repaint();
        });
    }

    fn app_update_hint(&self) -> &'static str {
        match &self.install_kind {
            InstallKind::Pacman => "Downloads the new Arch package and installs it with pacman (asks for your password), then restarts OpenPeriph.",
            InstallKind::Replace(_) => "Downloads the new version, replaces this one and restarts OpenPeriph.",
            InstallKind::WindowsSetup => "Downloads the new installer, installs it quietly and restarts OpenPeriph.",
            InstallKind::Manual => "This copy can't be replaced from here, so this opens the release page.",
        }
    }

    /// "Update to vX" in the sidebar, or its progress.
    fn app_update_box(&mut self, ui: &mut Ui) {
        if let Some((stage, f)) = self.app_update.clone() {
            progress_bar(ui, f, &stage);
            return;
        }
        let Some(Ok(a)) = &self.app_release else {
            return;
        };
        if !a.newer() {
            return;
        }
        let tag = a.release.tag.clone();
        let flashing = self.devices.iter().any(|d| d.qmk.as_ref().is_some_and(|q| q.flashing()));
        let manual = self.install_kind == InstallKind::Manual;
        let text = if manual { format!("{tag} available: open the release") } else { format!("Update OpenPeriph to {tag}") };
        let btn = egui::Button::new(RichText::new(text).family(theme::semibold()).color(Color32::BLACK))
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 28.0));
        let hint = self.app_update_hint();
        if ui.add_enabled(!flashing && !self.screenshot_mode(), btn).on_hover_text(hint).clicked() {
            self.start_app_update(ui.ctx());
        }
    }

    /// Popups offering updates (OpenPeriph first, then keyboard firmware).
    fn update_popups(&mut self, ctx: &egui::Context) {
        if self.app_popup {
            let Some(Ok(a)) = self.app_release.clone() else {
                self.app_popup = false;
                return;
            };
            let mut close = false;
            let mut update = false;
            let resp = egui::Modal::new(egui::Id::unique("app-update")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(theme::title(format!("OpenPeriph {} is available", a.release.tag)));
                ui.label(RichText::new(format!("You have v{}.", env!("CARGO_PKG_VERSION"))).color(TEXT_DIM));
                release_notes(ui, &a.release.body);
                ui.label(RichText::new(self.app_update_hint()).size(11.5).color(TEXT_DIM));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let label = if self.install_kind == InstallKind::Manual { "Open the release page" } else { "Update now" };
                    if ui.add(primary_button(label)).clicked() {
                        update = true;
                    }
                    if ui.button("Later").clicked() {
                        close = true;
                    }
                });
            });
            if update {
                self.app_popup = false;
                self.start_app_update(ctx);
            } else if close || resp.should_close() {
                self.app_popup = false;
            }
            return;
        }
        let Some(i) = self.fw_popup else { return };
        let info = self.devices[i].driver.info().clone();
        let Some((tag, installed, notes)) = self.devices[i].qmk.as_ref().and_then(|q| q.release_summary()) else {
            self.fw_popup = None;
            return;
        };
        let mut close = false;
        let mut update = false;
        let resp = egui::Modal::new(egui::Id::unique("fw-update")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(theme::title(format!("New firmware for your {}", info.name)));
            ui.label(RichText::new(format!("{tag} is available; the keyboard has {installed}.")).color(TEXT_DIM));
            release_notes(ui, &notes);
            ui.label(
                RichText::new("Updating takes about 20 seconds. Your keymap, macros and lighting are saved first and put back afterwards.").size(12.0).color(TEXT_DIM),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button(&format!("Update to {tag}"))).clicked() {
                    update = true;
                }
                if ui.button("Later").clicked() {
                    close = true;
                }
            });
        });
        if update {
            self.fw_popup = None;
            self.selected = i;
            self.view = View::Device;
            self.tab = Tab::Settings;
            self.firmware_update_now(i);
        } else if close || resp.should_close() {
            self.fw_popup = None;
        }
    }

    /// Right-hand side of a page's title row (laid out right to left): access, firmware, Apply.
    fn header_actions(&mut self, ui: &mut Ui) {
        let d = &self.devices[self.selected];
        let caps = d.driver.capabilities();
        let connected = d.connected == Some(true);
        let dirty = d.dirty();
        let needs_access = d.needs_access;
        let battery = if caps.mouse && connected { d.mouse_status.battery.map(|b| (b, d.mouse_status.charging, d.mouse_status.wired)) } else { None };
        if needs_access {
            if ui.add(primary_button("Allow access")).on_hover_text("Adds a udev rule so you can use this device without root (asks for your password).").clicked() {
                let ids = self.devices[self.selected].driver.usb_ids();
                let tx = self.tx.clone();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Access(hid::install_udev_rules(&ids)));
                    ctx.request_repaint();
                });
            }
            pill(ui, "Linux needs permission to use this device", WARN);
            return;
        }
        if caps.qmk {
            if let Some(tag) = self.devices[self.selected].qmk.as_ref().and_then(|q| q.update_available()) {
                if ui.add(primary_button(&format!("Firmware {tag} available"))).clicked() {
                    self.fw_popup = Some(self.selected);
                }
            }
            if connected {
                pill(ui, "Saved on the keyboard as you go", GOOD);
            } else {
                pill(ui, "Plug in the keyboard to change it", TEXT_DIM);
            }
            return;
        }
        let label = if self.busy { "Sending…" } else { "Apply to device" };
        let enabled = connected && dirty && !self.busy;
        if ui.add_enabled(enabled, primary_button(label)).clicked() {
            self.apply(ui.ctx());
        }
        if dirty {
            pill(ui, "Unsent changes", WARN);
        } else if connected {
            pill(ui, "Up to date", GOOD);
        }
        if !connected {
            let text = if self.devices[self.selected].receiver_only { "Switch the mouse on or move it to apply" } else { "Plug in the device to apply" };
            pill(ui, text, TEXT_DIM);
        }
        if let Some((pct, charging, wired)) = battery {
            let text = match (charging, wired) {
                (true, _) => format!("Battery {pct}% · charging"),
                (false, true) => format!("Battery {pct}% · cable"),
                _ => format!("Battery {pct}%"),
            };
            pill(ui, &text, if pct <= 15 { BAD } else { ACCENT });
        }
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
            ui.label(RichText::new(if self.fn_layer { "What a key does while Fn is held." } else { "Click a key, then choose what it does." }).color(TEXT_DIM));
        });
        ui.add_space(10.0);

        let avail = ui.available_width();
        let kb_h = (avail * 0.3).clamp(190.0, 290.0);
        card(ui, |ui| {
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, kb_h), Sense::hover());
            self.draw_keys(ui, rect);
        });
        ui.add_space(12.0);
        card(ui, |ui| self.key_editor(ui));
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
            let r = Rect::from_min_max(origin + Vec2::new(k.rect[0], k.rect[1]) * scale, origin + Vec2::new(k.rect[2], k.rect[3]) * scale).shrink(1.0);
            let id = ui.scope_id().with(("key", k.id));
            let resp = ui.interact(r, id, if k.locked { Sense::hover() } else { Sense::click() });
            let action = map.get(&k.id).copied().unwrap_or_default();
            let custom = action != KeyAction::Default;
            let selected = self.selected_key == Some(k.id);
            let mut fill = if k.locked { Color32::from_rgb(32, 36, 45) } else { KEYCAP };
            if custom {
                fill = Color32::from_rgb(44, 72, 58);
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
            ui.label(theme::title(title));
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
                        ui.label(RichText::new("+"));
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
            ui.label(theme::title("Effect"));
            ui.add_space(8.0);
            let per_row = 3;
            let w = (ui.available_width() - 8.0 * (per_row as f32 - 1.0)) / per_row as f32;
            for row in effects.chunks(per_row).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (j, e) in row.1.iter().enumerate() {
                        let idx = row.0 * per_row + j;
                        let sel = d.lighting.effect == idx;
                        let btn = chip(e.name, sel).min_size(Vec2::new(w, 30.0));
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
            ui.label(theme::title(format!("{} settings", info.name)));
            ui.add_space(8.0);
            preview(ui, &d.lighting, &info, driver.as_ref());
            ui.add_space(12.0);
            if info.off {
                ui.label(RichText::new("The lights will be switched off.").color(TEXT_DIM));
            }
            if info.any_colour {
                let a = &mut d.lighting.any;
                let [r, g, b] = a.rgb;
                let (h, s) = qmk_ui::parse_hex(&format!("{r:02x}{g:02x}{b:02x}")).unwrap_or((0, 0));
                ui.label(RichText::new("Colour").color(TEXT_DIM));
                if let Some((h, s)) = qmk_ui::colour_picker(ui, egui::Id::unique("x78-any-colour"), h, s) {
                    let c = qmk_ui::hsv_colour(h, s);
                    a.rgb = [c.r(), c.g(), c.b()];
                    changed = true;
                }
                ui.add_space(8.0);
                let labels = driver.brightness_labels();
                ui.label(RichText::new("Brightness").color(TEXT_DIM));
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::Slider::new(&mut a.brightness, 1..=driver.max_brightness()).show_value(false)).changed();
                    ui.label(labels.get(a.brightness as usize - 1).cloned().unwrap_or_default());
                });
                ui.add_space(6.0);
                ui.label(theme::small(
                    "The whole keyboard in one colour. It's kept in colour group M5 on the keyboard (Fn+5 switches back to it); groups M1–M4 aren't changed.",
                ));
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

impl OpenPeriphApp {
    /// Keeps the open page one the selected device has.
    fn fix_tab(&mut self) {
        let caps = self.devices[self.selected].driver.capabilities();
        if caps.qmk {
            self.qmk_fix_tab();
            return;
        }
        let ok = match self.tab {
            Tab::Keys => caps.key_remap,
            Tab::Lighting => caps.lighting,
            Tab::Mouse => caps.mouse,
            Tab::Settings => true,
            Tab::Macros => false,
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
        let Some(spec) = driver.mouse_spec(connected && status.wired) else {
            return;
        };
        let Some(mut m) = self.devices[i].mouse.clone() else {
            return;
        };
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
        let wide = ui.available_width() > 900.0;
        ui.horizontal_top(|ui| {
            if wide {
                // the mouse, like on its box
                let (art, _) = ui.allocate_exact_size(Vec2::new(190.0, 250.0), Sense::hover());
                let ctx = ui.ctx().clone();
                let p = ui.painter().clone();
                p.circle_filled(art.center(), 90.0, ACCENT.gamma_multiply(0.04));
                self.device_art(&ctx, &p, i, art.shrink(12.0), false);
                ui.add_space(6.0);
            }
            ui.vertical(|ui| {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(theme::title("DPI stages"));
                        ui.add_space(10.0);
                        ui.label(RichText::new("Click a stage to edit it. The DPI button on the mouse steps through them.").color(TEXT_DIM));
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
                                .min_size(Vec2::new(104.0, 58.0));
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
                        ui.label(RichText::new(format!("Stage {}", e + 1)).family(theme::semibold()).color(Color32::WHITE));
                        let v = &mut m.dpi_stages[e];
                        ui.add(egui::Slider::new(v, spec.dpi_min..=spec.dpi_max).step_by(spec.dpi_step as f64).logarithmic(true).show_value(false));
                        ui.add(egui::DragValue::new(v).range(spec.dpi_min..=spec.dpi_max).speed(spec.dpi_step as f64).suffix(" DPI"));
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
            });
        });
        ui.add_space(12.0);

        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                ui.label(theme::title("Performance"));
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
                    ui.label(RichText::new("Over the cable the mouse polls up to 1 kHz; the receiver allows up to 8 kHz.").size(11.5).color(TEXT_DIM));
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
                // Shown unless the mouse said it doesn't know this setting (older firmware).
                if spec.competitive && status.competitive_supported != Some(false) {
                    toggle(
                        ui,
                        &mut m.competitive,
                        "Hunting Shark competitive mode",
                        "Runs the sensor at full performance: lower latency and steadier tracking, but the battery runs down faster.",
                    );
                }
            });
            card(&mut cols[1], |ui| {
                ui.label(theme::title("Buttons & power"));
                ui.add_space(6.0);
                ui.label(RichText::new("Click debounce").color(TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut m.debounce_ms, 0..=spec.debounce_max).show_value(false));
                    ui.label(format!("{} ms", m.debounce_ms));
                });
                ui.label(RichText::new("Lower reacts faster; raise it if single clicks register twice.").size(11.5).color(TEXT_DIM));
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
                    ui.label(RichText::new(format!("Currently {} s (set elsewhere).", m.sleep_secs)).size(11.5).color(TEXT_DIM));
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

impl eframe::App for OpenPeriphApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);

        theme::paint_background(&ctx, &ui.painter().clone(), ctx.content_rect());
        egui::Panel::top("top").exact_size(46.0).resizable(false).frame(Frame::NONE).show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").exact_size(26.0).resizable(false).frame(Frame::NONE).show(ui, |ui| self.status_bar(ui));
        // A device that was unplugged: back to Home.
        if self.view == View::Device && self.devices[self.selected].connected == Some(false) && !self.devices[self.selected].receiver_only {
            self.view = View::Home;
        }
        egui::CentralPanel::default().frame(Frame::NONE.inner_margin(Margin { left: 16, right: 16, top: 14, bottom: 12 })).show(ui, |ui| match self.view {
            View::Home => self.home_page(ui),
            View::About => self.about_page(ui),
            View::Device => self.device_view(ui),
        });

        self.update_popups(&ctx);

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

/// On Linux: a device of these ids is plugged in but its hidraw node can't be opened.
fn needs_access(ids: &[(u16, u16)]) -> bool {
    ids.iter().any(|&(v, p)| hid::collections(v, p).iter().any(hid::permission_problem))
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

/// A progress bar with its label above it in white, readable however full the bar is.
fn progress_bar(ui: &mut Ui, fraction: f32, label: &str) {
    let f = fraction.clamp(0.0, 1.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("{}%", (f * 100.0).round() as u32)).family(theme::semibold()).color(theme::TEXT));
        });
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(3), Color32::from_white_alpha(16));
    let filled = Rect::from_min_size(rect.min, Vec2::new(rect.width() * f, rect.height()));
    p.rect_filled(filled, CornerRadius::same(3), ACCENT);
    ui.ctx().request_repaint_after(Duration::from_millis(100));
}

/// The accent-coloured button for the main action.
fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).family(theme::semibold()).color(Color32::BLACK))
        .fill(ACCENT)
        .corner_radius(CornerRadius::same(7))
        .min_size(Vec2::new(0.0, 28.0))
}

/// The first lines of a release's notes, as plain text.
fn release_notes(ui: &mut Ui, notes: &str) {
    let text: Vec<(bool, String)> = notes
        .lines()
        .map(|l| l.replace("**", "").replace('`', "").trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('|') && !l.starts_with("---"))
        .map(|l| match l.strip_prefix('#') {
            Some(h) => (true, h.trim_start_matches('#').trim().to_string()),
            None => match l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")) {
                Some(item) => (false, format!("•  {item}")),
                None => (false, l),
            },
        })
        .take(10)
        .collect();
    if text.is_empty() {
        return;
    }
    ui.add_space(6.0);
    egui::Frame::new().fill(theme::BG1).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            for (heading, l) in text {
                if heading {
                    ui.label(RichText::new(l).family(theme::semibold()).color(theme::TEXT));
                } else {
                    ui.label(RichText::new(l).size(12.5).color(Color32::from_rgb(205, 211, 221)));
                }
            }
        });
    });
    ui.add_space(6.0);
}

fn pill(ui: &mut Ui, text: &str, colour: Color32) {
    let font = FontId::proportional(11.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, colour);
    let size = galley.size() + Vec2::new(26.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), colour.gamma_multiply(0.13));
    p.circle_filled(Pos2::new(rect.min.x + 10.0, rect.center().y), 2.5, colour);
    p.galley(Pos2::new(rect.min.x + 17.0, rect.center().y - galley.size().y / 2.0), galley, colour);
}

/// A choice among a few: tinted with the accent when chosen.
fn chip(text: &str, on: bool) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).size(12.5).color(if on { Color32::WHITE } else { theme::TEXT }))
        .fill(if on { ACCENT.gamma_multiply(0.22) } else { Color32::from_white_alpha(7) })
        .stroke(Stroke::new(1.0, if on { ACCENT.gamma_multiply(0.9) } else { Color32::from_white_alpha(14) }))
        .corner_radius(CornerRadius::same(7))
        .min_size(Vec2::new(0.0, 26.0))
}

fn segment(ui: &mut Ui, text: &str, on: bool) -> bool {
    ui.add(chip(text, on)).clicked()
}

fn key_picker(ui: &mut Ui, usage: &mut u8) {
    egui::ComboBox::from_id_salt("key_picker").selected_text(crate::keycodes::usage_name(*usage)).height(360.0).width(200.0).show_ui(ui, |ui| {
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
    p.rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(14, 16, 21));
    let s = l.per_effect.get(l.effect).copied();
    let level = if info.any_colour { Some(l.any.brightness) } else { s.map(|s| s.brightness) };
    let bright = level.map(|b| b as f32 / driver.max_brightness() as f32).unwrap_or(1.0);
    let t = ui.input(|i| i.time) as f32;
    for k in &layout.keys {
        let r = Rect::from_min_max(rect.min + Vec2::new(k.rect[0], k.rect[1]) * scale, rect.min + Vec2::new(k.rect[2], k.rect[3]) * scale).shrink(0.8);
        let x = k.rect[0] / layout.width;
        let colour = if info.off {
            Color32::from_rgb(40, 42, 48)
        } else if info.any_colour {
            let [r, g, b] = l.any.rgb;
            Color32::from_rgb(r, g, b)
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

/// One DPI stage card: colour strip, stage number, value, "in use" mark. Returns true when clicked.
fn stage_chip(ui: &mut Ui, index: usize, dpi: u32, colour: Color32, active: bool, editing: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(104.0, 58.0), Sense::click());
    let p = ui.painter();
    let fill = if editing {
        Color32::from_white_alpha(18)
    } else if resp.hovered() {
        Color32::from_white_alpha(12)
    } else {
        Color32::from_white_alpha(6)
    };
    p.rect_filled(rect, CornerRadius::same(10), fill);
    p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(10.0, 8.0), Vec2::new(rect.width() - 20.0, 4.0)), CornerRadius::same(2), colour);
    p.text(rect.min + Vec2::new(10.0, 24.0), Align2::LEFT_CENTER, format!("Stage {}", index + 1), FontId::proportional(11.5), TEXT_DIM);
    if active {
        p.text(rect.right_top() + Vec2::new(-10.0, 24.0), Align2::RIGHT_CENTER, "IN USE", FontId::proportional(10.0), ACCENT);
    }
    p.text(rect.min + Vec2::new(10.0, 41.0), Align2::LEFT_CENTER, format!("{dpi}"), FontId::new(18.0, theme::semibold()), Color32::WHITE);
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
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(32.0, 18.0), Sense::click());
        if resp.clicked() {
            *on = !*on;
        }
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(9), if *on { ACCENT } else { Color32::from_white_alpha(22) });
        let x = if *on { rect.max.x - 9.0 } else { rect.min.x + 9.0 };
        p.circle_filled(Pos2::new(x, rect.center().y), 6.5, if *on { Color32::BLACK } else { theme::TEXT });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.label(RichText::new(label).color(theme::TEXT));
            if !hint.is_empty() {
                ui.label(theme::small(hint));
            }
        });
    });
    ui.add_space(4.0);
}
