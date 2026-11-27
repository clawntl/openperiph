//! Generic support for keyboards running QMK with VIA enabled: any such keyboard is found by its
//! raw HID interface, and with a VIA definition (bundled or imported) its keymap, macros,
//! lighting and layout options can be changed, the same things VIA's web app does.
//!
//! Each connected keyboard gets a worker thread that owns the connection and runs `Job`s in
//! order, so the window never waits on the keyboard.

pub mod definition;
pub mod demo;
pub mod firmware;
pub mod keycodes;
mod keycodes_table;
pub mod macros;
pub mod menus;
pub mod sinowealth;
pub mod via;

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::hid;
use definition::Definition;
use menus::Menu;
use via::{Transport, Via};

/// Every QMK raw HID interface on the system (one per keyboard).
pub fn scan() -> Vec<hid::Collection> {
    let mut out: Vec<hid::Collection> = hid::enumerate(&|_, _| true).into_iter().filter(|c| c.usage_page == via::USAGE_PAGE && c.usage == via::USAGE).collect();
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Everything read from a keyboard.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub protocol: u16,
    pub layers: usize,
    /// layers x rows x cols; empty without a definition.
    pub keymap: Vec<Vec<Vec<u16>>>,
    pub macro_count: usize,
    pub macro_size: usize,
    pub macros: Vec<String>,
    pub layout_options: u32,
    pub menus: Vec<Menu>,
    pub values: HashMap<(u8, u8), Vec<u8>>,
    pub firmware: Option<u32>,
}

/// A saved copy of a keyboard's settings. The `name`, `vendorProductId`, `layers` and `macros`
/// fields follow VIA's own "Save" file, so files can go back and forth.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Backup {
    pub name: String,
    #[serde(rename = "vendorProductId")]
    pub vendor_product_id: u32,
    /// Per layer, every matrix position row by row, as QMK keycode names.
    pub layers: Vec<Vec<String>>,
    #[serde(default)]
    pub macros: Vec<String>,
    #[serde(default, rename = "layoutOptions", skip_serializing_if = "Option::is_none")]
    pub layout_options: Option<u32>,
    /// "channel:id" -> value bytes.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub values: HashMap<String, Vec<u8>>,
}

pub enum Job {
    Load(Option<Arc<Definition>>),
    SetKey {
        layer: u8,
        row: u8,
        col: u8,
        code: u16,
    },
    SetValue {
        channel: u8,
        id: u8,
        data: Vec<u8>,
    },
    LayoutOptions(u32),
    Macros(Vec<u8>),
    ResetKeymap(Option<Arc<Definition>>),
    ResetMacros,
    Restore {
        keymap: Vec<Vec<Vec<u16>>>,
        macros: Option<Vec<u8>>,
        layout_options: Option<u32>,
        values: Vec<((u8, u8), Vec<u8>)>,
    },
    /// Restart into the bootloader; the optional message is a keyboard-specific fallback for
    /// firmware without VIA's bootloader command.
    Bootloader(Option<Vec<u8>>),
}

pub enum Event {
    Loaded(Result<Snapshot, String>),
    /// A job finished: a message for the status line, or an error.
    Done(Result<String, String>),
}

pub type Connect = Box<dyn Fn() -> Result<Box<dyn Transport>, String> + Send>;

/// Opens the raw HID interface at this path. Only the keyboard's own collections are looked at:
/// listing every HID device costs a USB request per device for its names on Windows, which
/// made finding the keyboard slow there.
pub fn connect_hid(path: String, vendor_id: u16, product_id: u16) -> Connect {
    Box::new(move || {
        let c = hid::collections(vendor_id, product_id).into_iter().find(|c| c.path == path && c.usage_page == via::USAGE_PAGE).ok_or("The keyboard isn't connected.")?;
        Ok(Box::new(via::HidTransport::open(&c)?) as Box<dyn Transport>)
    })
}

/// Finds the keyboard's VIA interface when the worker connects (off the window's thread).
pub fn connect_driver(find: impl Fn() -> Option<hid::Collection> + Send + 'static) -> Connect {
    Box::new(move || {
        let c = find().ok_or("The keyboard isn't connected.")?;
        Ok(Box::new(via::HidTransport::open(&c)?) as Box<dyn Transport>)
    })
}

/// Starts a worker. `notify` is called after every event (to wake the window).
pub fn spawn_worker(connect: Connect, notify: impl Fn(Event) + Send + 'static) -> Sender<Job> {
    let (tx, rx) = channel::<Job>();
    std::thread::spawn(move || worker(connect, rx, notify));
    tx
}

fn worker(connect: Connect, rx: Receiver<Job>, notify: impl Fn(Event)) {
    let mut via: Option<Via<Box<dyn Transport>>> = None;
    let mut unsaved: Vec<u8> = Vec::new();
    let mut pending: Option<Job> = None;
    loop {
        let job = match pending.take() {
            Some(j) => j,
            None => match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(j) => j,
                Err(RecvTimeoutError::Timeout) => {
                    // Lighting values are written live and stored once the person stops changing them.
                    if !unsaved.is_empty() {
                        if let Some(v) = via.as_mut() {
                            for ch in unsaved.drain(..) {
                                let _ = v.save_values(ch);
                            }
                        }
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        // Only the newest of several queued changes to the same value matters (slider drags).
        let job = match job {
            Job::SetValue { channel, id, mut data } => {
                loop {
                    match rx.try_recv() {
                        Ok(Job::SetValue { channel: c, id: i, data: d }) if (c, i) == (channel, id) => data = d,
                        Ok(other) => {
                            pending = Some(other);
                            break;
                        }
                        Err(_) => break,
                    }
                }
                Job::SetValue { channel, id, data }
            }
            j => j,
        };
        if via.is_none() {
            match connect() {
                Ok(t) => via = Some(Via::new(t)),
                Err(e) => {
                    notify(match job {
                        Job::Load(_) => Event::Loaded(Err(e)),
                        _ => Event::Done(Err(e)),
                    });
                    continue;
                }
            }
        }
        let v = via.as_mut().unwrap();
        let is_load = matches!(job, Job::Load(_));
        let result = run(v, job, &mut unsaved);
        let failed = match &result {
            Ok(_) => false,
            Err(_) => true,
        };
        match result {
            Ok(Outcome::Snapshot(s)) => notify(Event::Loaded(Ok(*s))),
            Ok(Outcome::Message(m)) => notify(Event::Done(Ok(m))),
            Err(e) if is_load => notify(Event::Loaded(Err(e))),
            Err(e) => notify(Event::Done(Err(e))),
        }
        if failed {
            // reconnect next time (the keyboard may have been unplugged)
            via = None;
        }
    }
}

enum Outcome {
    Snapshot(Box<Snapshot>),
    Message(String),
}

fn load<T: Transport + ?Sized>(v: &mut Via<Box<T>>, def: Option<&Definition>) -> Result<Snapshot, String> {
    let protocol = v.protocol_version().map_err(|e| {
        if e.contains("didn't answer") {
            "The keyboard has QMK's raw HID interface but doesn't answer VIA messages, so its firmware probably doesn't have VIA enabled (VIA_ENABLE = yes).".to_string()
        } else {
            e
        }
    })?;
    if protocol < 9 {
        return Err(format!("This keyboard speaks VIA protocol {protocol}, which is too old for this app."));
    }
    let mut s = Snapshot { protocol, ..Default::default() };
    s.layers = v.layer_count()? as usize;
    if let Some(d) = def {
        s.keymap = v.read_keymap(s.layers, d.rows, d.cols)?;
        s.layout_options = v.layout_options()?;
    }
    s.firmware = v.firmware_version()?;
    s.macro_count = v.macro_count()? as usize;
    if s.macro_count > 0 {
        s.macro_size = v.macro_buffer_size()? as usize;
        let buf = v.read_macro_buffer(s.macro_size)?;
        s.macros = macros::decode(&buf, s.macro_count);
    }
    // Menus: the definition's, or the built-in ones for the lighting the firmware has.
    s.menus = match def {
        Some(d) if !d.menus.is_empty() => menus::parse_menus(&d.menus),
        _ => {
            let mut names = Vec::new();
            for ch in [1u8, 2, 3, 5, 4] {
                if v.has_channel(ch)? {
                    if let Some(n) = menus::builtin_for_channel(ch) {
                        names.push(serde_json::Value::String(n.into()));
                    }
                }
            }
            menus::parse_menus(&names)
        }
    };
    for m in &s.menus {
        for sec in &m.sections {
            for c in &sec.controls {
                if let Some(b) = v.get_value(c.channel, c.id, c.len())? {
                    s.values.insert((c.channel, c.id), b);
                }
            }
        }
    }
    Ok(s)
}

fn run<T: Transport + ?Sized>(v: &mut Via<Box<T>>, job: Job, unsaved: &mut Vec<u8>) -> Result<Outcome, String> {
    Ok(match job {
        Job::Load(def) => Outcome::Snapshot(Box::new(load(v, def.as_deref())?)),
        Job::SetKey { layer, row, col, code } => {
            v.set_keycode(layer, row, col, code)?;
            Outcome::Message(format!("Saved {} to the keyboard.", keycodes::name(code)))
        }
        Job::SetValue { channel, id, data } => {
            v.set_value(channel, id, &data)?;
            if !unsaved.contains(&channel) {
                unsaved.push(channel);
            }
            Outcome::Message("Lighting updated.".into())
        }
        Job::LayoutOptions(o) => {
            v.set_layout_options(o)?;
            Outcome::Message("Layout options saved.".into())
        }
        Job::Macros(buf) => {
            let size = v.macro_buffer_size()? as usize;
            v.write_macro_buffer(&buf, size)?;
            Outcome::Message("Macros saved to the keyboard.".into())
        }
        Job::ResetKeymap(def) => {
            v.reset_keymap()?;
            Outcome::Snapshot(Box::new(load(v, def.as_deref())?))
        }
        Job::ResetMacros => {
            v.reset_macros()?;
            Outcome::Message("Macros cleared.".into())
        }
        Job::Restore { keymap, macros, layout_options, values } => {
            v.write_keymap(&keymap)?;
            if let Some(o) = layout_options {
                v.set_layout_options(o)?;
            }
            if let Some(m) = macros {
                let size = v.macro_buffer_size()? as usize;
                v.write_macro_buffer(&m, size)?;
            }
            let mut channels = Vec::new();
            for ((ch, id), data) in values {
                v.set_value(ch, id, &data)?;
                if !channels.contains(&ch) {
                    channels.push(ch);
                }
            }
            for ch in channels {
                v.save_values(ch)?;
            }
            Outcome::Message("Restored the saved settings to the keyboard.".into())
        }
        Job::Bootloader(fallback) => {
            let done = v.bootloader()?
                || match fallback {
                    Some(m) => {
                        let mut msg = [0u8; via::MSG_LEN];
                        msg[..m.len().min(via::MSG_LEN)].copy_from_slice(&m[..m.len().min(via::MSG_LEN)]);
                        // the keyboard restarts straight away, so no answer is expected
                        let _ = v.t.exchange(&msg);
                        true
                    }
                    None => false,
                };
            if done {
                Outcome::Message("The keyboard restarted into its bootloader. Flash it, or unplug it to go back.".into())
            } else {
                return Err("This firmware doesn't allow restarting into the bootloader from VIA. Use its reset key or button.".into());
            }
        }
    })
}

/// The keymap the firmware starts with, read without losing the current one: reads the keymap,
/// resets it to the firmware's default, reads that, and writes the current one back.
pub fn read_default_keymap(c: &hid::Collection, layers: usize, rows: usize, cols: usize) -> Result<Vec<Vec<Vec<u16>>>, String> {
    let connect = connect_hid(c.path.clone(), c.vendor_id, c.product_id);
    let mut v = Via::new(connect()?);
    let current = v.read_keymap(layers, rows, cols)?;
    v.reset_keymap()?;
    let defaults = v.read_keymap(layers, rows, cols);
    v.write_keymap(&current)?;
    defaults
}

/// After a firmware update: only the keys the person changed from the old firmware's defaults
/// are put back, so keys the new firmware changed (fixes) come through. Macros and lighting are
/// put back as they were.
pub fn changes_only(b: &Backup, old_defaults: &[Vec<Vec<u16>>], new_keymap: &[Vec<Vec<u16>>], def: &Definition) -> Backup {
    let mut out = b.clone();
    for (l, layer) in out.layers.iter_mut().enumerate() {
        for (i, name) in layer.iter_mut().enumerate() {
            let (r, c) = (i / def.cols, i % def.cols);
            let old = old_defaults.get(l).and_then(|x| x.get(r)).and_then(|x| x.get(c)).copied();
            let mine = keycodes::parse(name, &def.custom_keycodes);
            if old.is_some() && mine == old {
                if let Some(new) = new_keymap.get(l).and_then(|x| x.get(r)).and_then(|x| x.get(c)) {
                    *name = keycodes::name(*new);
                }
            }
        }
    }
    out
}

/// Makes a backup from what was read.
pub fn backup(def: &Definition, s: &Snapshot) -> Backup {
    Backup {
        name: def.name.clone(),
        vendor_product_id: ((def.vendor_id as u32) << 16) | def.product_id as u32,
        layers: s.keymap.iter().map(|l| l.iter().flatten().map(|k| keycodes::name(*k)).collect()).collect(),
        macros: s.macros.clone(),
        layout_options: if def.options.is_empty() { None } else { Some(s.layout_options) },
        values: s.values.iter().map(|((c, i), v)| (format!("{c}:{i}"), v.clone())).collect(),
    }
}

/// Turns a backup into a restore job, checking it fits this keyboard.
pub fn restore_job(def: &Definition, s: &Snapshot, b: &Backup) -> Result<Job, String> {
    let vp = ((def.vendor_id as u32) << 16) | def.product_id as u32;
    if b.vendor_product_id != 0 && b.vendor_product_id != vp {
        return Err(format!("This file is for another keyboard ({}).", if b.name.is_empty() { "unknown" } else { &b.name }));
    }
    let cells = def.rows * def.cols;
    let mut keymap = s.keymap.clone();
    for (l, layer) in b.layers.iter().enumerate().take(s.layers) {
        if layer.len() != cells {
            return Err(format!("Layer {l} in the file has {} keys; this keyboard has {cells}.", layer.len()));
        }
        for (i, name) in layer.iter().enumerate() {
            let code = keycodes::parse(name, &def.custom_keycodes).ok_or_else(|| format!("Unknown keycode \"{name}\" in the file."))?;
            keymap[l][i / def.cols][i % def.cols] = code;
        }
    }
    let macros = if b.macros.is_empty() || s.macro_count == 0 {
        None
    } else {
        let mut m = b.macros.clone();
        m.resize(s.macro_count, String::new());
        m.truncate(s.macro_count);
        Some(macros::encode(&m, &def.custom_keycodes)?)
    };
    let mut values = Vec::new();
    for (k, v) in &b.values {
        if let Some((c, i)) = k.split_once(':') {
            if let (Ok(c), Ok(i)) = (c.parse::<u8>(), i.parse::<u8>()) {
                if s.values.contains_key(&(c, i)) {
                    values.push(((c, i), v.clone()));
                }
            }
        }
    }
    Ok(Job::Restore { keymap, macros, layout_options: b.layout_options, values })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn worker_loads_edits_and_restores() {
        let def = Arc::new(Definition::parse(definition::BUNDLED[0]).unwrap());
        let kb = Arc::new(Mutex::new(demo::fake_sk620()));
        struct Shared(Arc<Mutex<via::FakeKeyboard>>);
        impl Transport for Shared {
            fn exchange(&mut self, m: &[u8; via::MSG_LEN]) -> Result<[u8; via::MSG_LEN], String> {
                self.0.lock().unwrap().exchange(m)
            }
        }
        let k2 = kb.clone();
        let connect: Connect = Box::new(move || Ok(Box::new(Shared(k2.clone())) as Box<dyn Transport>));
        let (etx, erx) = channel();
        let jobs = spawn_worker(connect, move |e| {
            let _ = etx.send(e);
        });
        let wait = || erx.recv_timeout(Duration::from_secs(5)).unwrap();

        jobs.send(Job::Load(Some(def.clone()))).unwrap();
        let Event::Loaded(Ok(s)) = wait() else { panic!("load failed") };
        assert_eq!(s.layers, 4);
        assert_eq!(s.keymap[0][0][0], 0x0029); // Esc
        assert_eq!(s.menus[0].label, "Lighting");
        assert_eq!(s.values.get(&(3, 2)), Some(&vec![5]));

        jobs.send(Job::SetKey { layer: 2, row: 0, col: 0, code: 0x7C00 }).unwrap();
        assert!(matches!(wait(), Event::Done(Ok(_))));
        for b in [10u8, 50, 90] {
            jobs.send(Job::SetValue { channel: 3, id: 1, data: vec![b] }).unwrap();
        }
        assert!(matches!(wait(), Event::Done(Ok(_))));
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(kb.lock().unwrap().saved_rgb[0], 90);
        // the three changes may have been merged into fewer messages: drop the rest
        while erx.try_recv().is_ok() {}

        let b = backup(&def, &{
            jobs.send(Job::Load(Some(def.clone()))).unwrap();
            let Event::Loaded(Ok(s)) = wait() else { panic!() };
            s
        });
        assert_eq!(b.layers[2][0], "QK_BOOT");
        let text = serde_json::to_string(&b).unwrap();
        let b2: Backup = serde_json::from_str(&text).unwrap();
        jobs.send(Job::ResetKeymap(Some(def.clone()))).unwrap();
        let Event::Loaded(Ok(s2)) = wait() else { panic!() };
        assert_eq!(s2.keymap[2][0][0], 0x0001);
        jobs.send(restore_job(&def, &s2, &b2).unwrap()).unwrap();
        assert!(matches!(wait(), Event::Done(Ok(_))));
        assert_eq!(kb.lock().unwrap().keymap[2 * 72], 0x7C00);
    }

    #[test]
    fn update_keeps_only_personal_changes() {
        let def = Definition::parse(definition::BUNDLED[0]).unwrap();
        let old: Vec<Vec<Vec<u16>>> = vec![vec![vec![4; 8]; 9]; 2];
        let mut mine = old.clone();
        mine[0][0][0] = 0x29; // the person remapped this key
        let mut new = old.clone();
        new[1][4][4] = 0x7F; // the new firmware changed this default
        let s = Snapshot { layers: 2, keymap: mine, ..Default::default() };
        let b = changes_only(&backup(&def, &s), &old, &new, &def);
        assert_eq!(b.layers[0][0], "KC_ESC");
        assert_eq!(keycodes::parse(&b.layers[1][4 * 8 + 4], &[]), Some(0x7F));
    }
}
