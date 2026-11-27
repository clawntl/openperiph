//! Driver for the CIY X78 keyboard (USB 258A:0016).
//! Every number here is written down, with where it came from, in docs/protocols/ciy-x78.md.

mod layout;

use std::thread::sleep;
use std::time::Duration;

use crate::device::*;
use crate::hid;
use crate::keycodes::{media_name, vk_to_usage, KeyAction, MouseAction};

use layout::{key_by_id, uk_shifted, DEFAULT_FN, ENTER_ID, ENTER_LOWER, FN_KEY_ID, KEYS, LAYOUT_H, LAYOUT_W};

const VID: u16 = 0x258A;
const PID: u16 = 0x0016;
const USAGE_PAGE: u16 = 0xFF00;
const USAGE: u16 = 0x0001;
const DATA_LEN: usize = 1032; // report id 6 + 7-byte header + payload
const CMD_FEATURE_LEN: u16 = 6;

const UNLOCK: [u8; 9] = [0x05, 0x01, 0xAA, 0xBB, 0x2F, 0x3E, 0x00, 0x00, 0x00];
const READ_SETTINGS: [u8; 7] = [0x05, 0x83, 0x00, 0x00, 0x00, 0x00, 0x00];
const HDR_WRITE_SETTINGS: [u8; 7] = [0x03, 0, 0, 0, 0, 0, 0];
const HDR_WRITE_KEYS: [u8; 7] = [0x04, 0xC8, 0x00, 0x40, 0x00, 0x00, 0x00];
/// Colour groups ("SetLedMatrix", 0x43f7e0): `09 address 00 40 00 00 00`, two groups per message.
/// Addresses from `GaoshouAddr` = 0xAC, +4 per message: AC = M1+M2, B0 = M3+M4, B4 = M5 + nothing.
/// "Any colour" uses M5, the only group that has a message to itself, so M1–M4 are never touched.
const HDR_WRITE_GROUP_M5: [u8; 7] = [0x09, 0xB4, 0x00, 0x40, 0x00, 0x00, 0x00];
const GROUP_LEN: usize = 396; // three planes of 132 key slots: blue, green, red (RGBIndex=2,1,0)
const ANY_COLOUR_GROUP: u8 = 4; // M5
/// Brightness levels as a share of full colour (Cfg.ini BriScale=5,25,50,75,100).
const BRI_SCALE: [u16; 5] = [5, 25, 50, 75, 100];
const SETTINGS_LEN: usize = 136;
const GLOBAL_LEN: usize = 8;
const PROFILE_LEN: usize = 64;

// Lighting profile layout.
const P_GAME_FLAG: usize = 4;
const P_EFFECT: usize = 5;
const P_GROUP: usize = 6;
const P_PARAMS: usize = 18;
const GROUP_BASE: u8 = 0x20;
const COLOURFUL: u8 = 7;
const DIRECTION_BIT: u8 = 8;
const EFFECT_GAME: usize = 14;
const EFFECT_OFF: usize = 15;
/// Not a keyboard effect: Game mode showing group M5, filled with one colour by this app.
const EFFECT_ANY: usize = 16;
const EFFECT_RAINBOW_WHEEL: usize = 9; // effect code 10, reversed direction (Direct1=10,8,0)
const LEVELS: u8 = 5;

// Key table layout.
const KEY_ENTRIES: usize = 132;
const FN_SLOTS: usize = 48;
const FN_SINGLE: usize = 132;
const FN_COMBO: usize = 180;

pub struct CiyX78 {
    info: DeviceInfo,
    layout: KeyboardLayout,
}

impl CiyX78 {
    pub fn new() -> Self {
        // The original's key rectangles leave wide gaps; grow each cap a little and move the
        // whole board to start at 0,0.
        const GROW: f32 = 3.0;
        const OX: f32 = 23.0 - GROW;
        const OY: f32 = 35.0 - GROW;
        let keys = KEYS
            .iter()
            .map(|k| LayoutKey {
                id: k.id,
                label: k.label,
                rect: [k.rect[0] - GROW - OX, k.rect[1] - GROW - OY, k.rect[2] + GROW - OX, k.rect[3] + GROW - OY],
                locked: k.id == FN_KEY_ID,
                shifted: uk_shifted(k.label),
                second: (k.id == ENTER_ID).then(|| {
                    let r = ENTER_LOWER;
                    [r[0] - GROW - OX, r[1] - GROW - OY, r[2] + GROW - OX, r[3] + GROW - OY]
                }),
            })
            .collect();
        CiyX78 {
            info: DeviceInfo { id: "ciy-x78".into(), name: "CIY X78".into(), maker: "CIY".into(), kind: DeviceKind::Keyboard, vendor_id: VID, product_id: PID },
            layout: KeyboardLayout { keys, width: LAYOUT_W, height: LAYOUT_H },
        }
    }
}

/// `LedOptN = icon, speed, brightness, direction, colourful, colour` (Cfg.ini); which control
/// each value shows is in function 0x417c50. Brightness is 1 for every effect listed.
const fn fx(name: &'static str, speed: bool, colour: bool, direction: bool) -> EffectInfo {
    EffectInfo { name, speed, brightness: true, direction, colour, colourful: colour, groups: 0, off: false, any_colour: false }
}

/// Effects in the keyboard's own order (effect code = position + 1, `EffectHw` default at 0x43bb13).
const EFFECTS: [EffectInfo; 17] = [
    fx("Colour loop", true, false, false),
    fx("Breathing", true, true, false),
    fx("Always on", false, true, false),
    fx("Ripples", true, true, false),
    fx("Reaction", true, true, false),
    fx("Flash away", true, true, false),
    fx("Sine wave", true, true, false),
    fx("Raindrops", true, true, false),
    fx("Neon stream", true, false, true),
    fx("Rainbow wheel", true, false, true),
    fx("Adorn", true, false, false),
    fx("Stars", true, true, false),
    fx("Shadow", true, true, false),
    fx("Snake", true, true, false),
    EffectInfo { name: "Game mode", speed: false, brightness: false, direction: false, colour: false, colourful: false, groups: 5, off: false, any_colour: false },
    EffectInfo { name: "Off", speed: false, brightness: false, direction: false, colour: false, colourful: false, groups: 0, off: true, any_colour: false },
    EffectInfo { name: "Any colour", speed: false, brightness: true, direction: false, colour: false, colourful: false, groups: 0, off: false, any_colour: true },
];

/// Basic colours 0..6, from the colour-button table built in function 0x4168e0
/// (Windows colour values 0xff, 0xff0000, 0xff00, 0xffff, 0xff00ff, 0xffff00, 0xffffff).
const COLOURS: [NamedColour; 7] = [
    NamedColour { name: "Red", rgb: [255, 0, 0] },
    NamedColour { name: "Blue", rgb: [0, 0, 255] },
    NamedColour { name: "Green", rgb: [0, 255, 0] },
    NamedColour { name: "Yellow", rgb: [255, 255, 0] },
    NamedColour { name: "Purple", rgb: [255, 0, 255] },
    NamedColour { name: "Cyan", rgb: [0, 255, 255] },
    NamedColour { name: "White", rgb: [255, 255, 255] },
];

// ---------------------------------------------------------------------------------------------
// Pure encoding (no device needed; covered by the tests at the bottom)

/// Bytes the keyboard stores for one key function.
fn encode_action(action: KeyAction, default_vk: u8) -> [u8; 4] {
    match action {
        KeyAction::Default => match default_vk {
            0xFA => [0x20, 0, 0, 0],
            vk => [0, 0, 0, vk_to_usage(vk).unwrap_or(0)],
        },
        KeyAction::Key { mods, usage } => [0, 0, mods, usage],
        KeyAction::Mouse(m) => {
            let code = match m {
                MouseAction::Left => 0xF0,
                MouseAction::Right => 0xF1,
                MouseAction::Middle => 0xF2,
                MouseAction::Back => 0xF3,
                MouseAction::Forward => 0xF4,
                MouseAction::WheelUp => 0xF7,
                MouseAction::WheelDown => 0xF8,
            };
            [0x07, 0, 0, code]
        }
        KeyAction::Media(u) => [0x04, 0, (u >> 8) as u8, u as u8],
        KeyAction::Disabled => [0, 0, 0, 0],
    }
}

/// The 1024-byte key table for these settings.
pub fn build_key_table(keys: &KeySettings) -> [u8; 1024] {
    let mut table = [0u8; 1024];
    let mut put = |entry: usize, bytes: [u8; 4]| table[entry * 4..entry * 4 + 4].copy_from_slice(&bytes);

    // Fn slots: first the keys with a built-in Fn function, in Cfg.ini order, then keys the
    // person gave a new Fn-layer function.
    let mut slots: Vec<u8> = DEFAULT_FN.iter().map(|(id, _, _)| *id).collect();
    for id in keys.fn_layer.keys() {
        if !slots.contains(id) && slots.len() < FN_SLOTS && *id != FN_KEY_ID {
            slots.push(*id);
        }
    }

    for k in KEYS {
        let normal = keys.normal.get(&k.id).copied().unwrap_or_default();
        let normal_bytes = if k.id == FN_KEY_ID { [0x20, 0, 0, 0] } else { encode_action(normal, k.vk) };
        let m = k.matrix as usize;
        if m >= KEY_ENTRIES {
            continue;
        }
        if let Some(slot) = slots.iter().position(|id| *id == k.id) {
            let fn_action = keys.fn_layer.get(&k.id).copied().unwrap_or_default();
            let fn_bytes = match fn_action {
                KeyAction::Default => DEFAULT_FN
                    .iter()
                    .find(|(id, _, _)| *id == k.id)
                    .map(|(_, code, _)| code.to_be_bytes())
                    // No built-in Fn function: the Fn layer does the same as the normal layer.
                    .unwrap_or(normal_bytes),
                other => encode_action(other, k.vk),
            };
            put(m, [0x02, 0, 0, slot as u8]);
            put(FN_SINGLE + slot, normal_bytes);
            put(FN_COMBO + slot, fn_bytes);
        } else {
            put(m, normal_bytes);
        }
    }
    table
}

/// A full data report: id 6, the 7-byte header, then the payload, padded to 1032 bytes.
fn data_report(header: &[u8; 7], payload: &[u8]) -> Vec<u8> {
    let mut buf = vec![0u8; DATA_LEN];
    buf[0] = 0x06;
    buf[1..8].copy_from_slice(header);
    let n = payload.len().min(DATA_LEN - 8);
    buf[8..8 + n].copy_from_slice(&payload[..n]);
    buf
}

/// Reads profile 1 of a settings block into the app's lighting model.
pub fn decode_lighting(settings: &[u8; SETTINGS_LEN]) -> LightingState {
    let p = &settings[GLOBAL_LEN..GLOBAL_LEN + PROFILE_LEN];
    let mut state = default_lighting();
    let code = p[P_EFFECT] as usize;
    state.effect = if p[P_GAME_FLAG] == 1 {
        EFFECT_GAME
    } else if code == 0 || code > 14 {
        EFFECT_OFF
    } else {
        code - 1
    };
    state.group = p[P_GROUP].saturating_sub(GROUP_BASE).min(4);
    for i in 0..14 {
        let c = p[P_PARAMS + 2 * i];
        let sb = p[P_PARAMS + 2 * i + 1];
        let colour_nibble = c & 0x0F;
        let high = c >> 4;
        let dir_set = high == DIRECTION_BIT;
        let direction = if i == EFFECT_RAINBOW_WHEEL { u8::from(!dir_set) } else { u8::from(dir_set) };
        state.per_effect[i] = EffectSettings {
            colour: if colour_nibble >= COLOURFUL { None } else { Some(colour_nibble) },
            speed: ((sb >> 4) + 1).clamp(1, LEVELS),
            brightness: ((sb & 0x0F) + 1).clamp(1, LEVELS),
            direction,
        };
    }
    state
}

/// Writes the lighting choices into profile 1 of a settings block, keeping every other byte.
pub fn encode_lighting(settings: &mut [u8; SETTINGS_LEN], state: &LightingState) {
    let p = &mut settings[GLOBAL_LEN..GLOBAL_LEN + PROFILE_LEN];
    let any = state.effect == EFFECT_ANY;
    p[P_GAME_FLAG] = u8::from(state.effect == EFFECT_GAME || any);
    p[P_EFFECT] = match state.effect {
        EFFECT_OFF => 0,
        EFFECT_ANY => EFFECT_GAME as u8 + 1,
        e => e as u8 + 1,
    };
    p[P_GROUP] = GROUP_BASE + if any { ANY_COLOUR_GROUP } else { state.group.min(4) };
    for (i, s) in state.per_effect.iter().enumerate().take(14) {
        let dir_set = if i == EFFECT_RAINBOW_WHEEL { s.direction == 0 } else { s.direction == 1 };
        let colour = s.colour.map(|c| c.min(6)).unwrap_or(COLOURFUL);
        p[P_PARAMS + 2 * i] = colour | if dir_set { DIRECTION_BIT << 4 } else { 0 };
        let speed = s.speed.clamp(1, LEVELS) - 1;
        let bright = s.brightness.clamp(1, LEVELS) - 1;
        p[P_PARAMS + 2 * i + 1] = (speed << 4) | bright;
    }
    p[62] = 0x5A;
    p[63] = 0xA5;
}

/// Colour group M5's message payload: every key the same colour, dimmed to the brightness level.
/// Then M5's 396 bytes, then 396 zero bytes (the message's second half has no group).
pub fn any_colour_payload(any: &AnyColour) -> Vec<u8> {
    let scale = BRI_SCALE[(any.brightness.clamp(1, LEVELS) - 1) as usize];
    let dim = |v: u8| ((v as u16 * scale + 50) / 100) as u8;
    let [r, g, b] = any.rgb.map(dim);
    let mut out = vec![0u8; GROUP_LEN * 2];
    // Every one of the 132 slots, not only the keys' matrix slots: on the keyboard, filling only
    // those left some LEDs dark, so the planes seem to follow the LEDs' own numbering (the
    // "light index" of Cfg.ini's [KEY] lines), not the key table's. One colour for everything
    // doesn't need to know which is which.
    out[..KEY_ENTRIES].fill(b);
    out[KEY_ENTRIES..2 * KEY_ENTRIES].fill(g);
    out[2 * KEY_ENTRIES..3 * KEY_ENTRIES].fill(r);
    out
}

fn default_lighting() -> LightingState {
    // LEDParam: colourful, speed 5, brightness 5; "Always on" defaults to colour 6.
    let mut per_effect = vec![EffectSettings { colour: None, speed: 5, brightness: 5, direction: 0 }; 16];
    per_effect[2].colour = Some(6);
    LightingState { effect: 13, per_effect, group: 0, any: AnyColour::default() }
}

// ---------------------------------------------------------------------------------------------
// Device access

struct Link {
    data: hid::Handle,
    cmd: hid::Handle,
}

fn retry(mut f: impl FnMut() -> bool) -> bool {
    for attempt in 0..3 {
        if f() {
            return true;
        }
        if attempt < 2 {
            sleep(Duration::from_millis(100));
        }
    }
    false
}

impl Link {
    fn open() -> Result<Link, String> {
        let cols = hid::collections(VID, PID);
        if cols.is_empty() {
            return Err("The keyboard isn't connected.".into());
        }
        let vendor: Vec<_> = cols.iter().filter(|c| c.usage_page == USAGE_PAGE && c.usage == USAGE).collect();
        let data = vendor.iter().find(|c| c.feature_len as usize == DATA_LEN).ok_or("The keyboard's settings channel wasn't found.")?;
        let cmd = vendor.iter().find(|c| c.feature_len == CMD_FEATURE_LEN).ok_or("The keyboard's command channel wasn't found.")?;
        let data = hid::open(&data.path).ok_or("Couldn't open the keyboard (is another app using it?).")?;
        let cmd = hid::open(&cmd.path).ok_or("Couldn't open the keyboard (is another app using it?).")?;
        Ok(Link { data, cmd })
    }

    fn command(&self, bytes: &[u8]) -> Result<(), String> {
        if retry(|| self.cmd.set_feature(bytes)) {
            Ok(())
        } else {
            Err("The keyboard didn't accept a command.".into())
        }
    }

    fn write(&self, header: &[u8; 7], payload: &[u8]) -> Result<(), String> {
        let buf = data_report(header, payload);
        if retry(|| self.data.set_feature(&buf)) {
            Ok(())
        } else {
            Err("The keyboard didn't accept the new settings.".into())
        }
    }

    fn read_settings(&self) -> Result<[u8; SETTINGS_LEN], String> {
        self.command(&READ_SETTINGS)?;
        sleep(Duration::from_millis(10));
        let mut buf = vec![0u8; DATA_LEN];
        buf[0] = 0x06;
        if !retry(|| {
            buf.fill(0);
            buf[0] = 0x06;
            self.data.get_feature(&mut buf)
        }) {
            return Err("Couldn't read the keyboard's settings.".into());
        }
        if buf[1] != 0x83 {
            return Err("The keyboard sent an unexpected reply.".into());
        }
        let mut out = [0u8; SETTINGS_LEN];
        out.copy_from_slice(&buf[8..8 + SETTINGS_LEN]);
        Ok(out)
    }
}

impl Driver for CiyX78 {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { key_remap: true, fn_layer: true, lighting: true, mouse: false, qmk: false }
    }

    fn layout(&self) -> &KeyboardLayout {
        &self.layout
    }

    fn effects(&self) -> &[EffectInfo] {
        &EFFECTS
    }

    fn colours(&self) -> &[NamedColour] {
        &COLOURS
    }

    fn max_speed(&self) -> u8 {
        LEVELS
    }

    fn max_brightness(&self) -> u8 {
        LEVELS
    }

    fn brightness_labels(&self) -> Vec<String> {
        // Cfg.ini BriScale=5,25,50,75,100
        ["5%", "25%", "50%", "75%", "100%"].iter().map(|s| s.to_string()).collect()
    }

    fn default_lighting(&self) -> LightingState {
        default_lighting()
    }

    fn default_description(&self, key_id: u8, fn_layer: bool) -> String {
        if fn_layer {
            if let Some((_, code, name)) = DEFAULT_FN.iter().find(|(id, _, _)| *id == key_id) {
                if code >> 24 == 0x04 {
                    return media_name((*code & 0xFFFF) as u16);
                }
                return (*name).to_string();
            }
            return "Same as normal".into();
        }
        match key_by_id(key_id) {
            Some(k) if k.id == FN_KEY_ID => "Fn (switches layer)".into(),
            Some(k) => vk_to_usage(k.vk).map(crate::keycodes::usage_name).unwrap_or_else(|| k.label.into()),
            None => "Unknown".into(),
        }
    }

    fn detect(&self) -> bool {
        hid::collections(VID, PID).iter().any(|c| c.usage_page == USAGE_PAGE)
    }

    fn read_lighting(&self) -> Result<LightingState, String> {
        let link = Link::open()?;
        let settings = link.read_settings()?;
        Ok(decode_lighting(&settings))
    }

    fn apply(&self, keys: Option<&KeySettings>, lighting: Option<&LightingState>, progress: Progress) -> Result<(), String> {
        progress("Connecting to the keyboard…");
        let link = Link::open()?;
        link.command(&UNLOCK)?;
        sleep(Duration::from_millis(20));

        let mut settings = None;
        if lighting.is_some() {
            progress("Reading current settings…");
            let mut s = link.read_settings()?;
            sleep(Duration::from_millis(30));
            // Same as the original: lights off while the rest is written.
            let mut quiet = s;
            quiet[GLOBAL_LEN + P_EFFECT] = 0;
            quiet[GLOBAL_LEN + P_GROUP] = 0;
            link.write(&HDR_WRITE_SETTINGS, &quiet)?;
            sleep(Duration::from_millis(90));
            if let Some(l) = lighting {
                encode_lighting(&mut s, l);
            }
            settings = Some(s);
        }

        if let Some(l) = lighting.filter(|l| l.effect == EFFECT_ANY) {
            progress("Sending the colour…");
            link.write(&HDR_WRITE_GROUP_M5, &any_colour_payload(&l.any))?;
            sleep(Duration::from_millis(40));
        }

        if let Some(k) = keys {
            progress("Sending key assignments…");
            link.write(&HDR_WRITE_KEYS, &build_key_table(k))?;
            sleep(Duration::from_millis(30));
        }

        if let Some(s) = settings {
            progress("Sending lighting…");
            link.write(&HDR_WRITE_SETTINGS, &s)?;
            sleep(Duration::from_millis(20));
        }
        progress("Done");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keycodes::MOD_CTRL;

    fn entry(t: &[u8], i: usize) -> [u8; 4] {
        [t[i * 4], t[i * 4 + 1], t[i * 4 + 2], t[i * 4 + 3]]
    }

    #[test]
    fn default_table() {
        let t = build_key_table(&KeySettings::default());
        // Esc (matrix 0) has an Fn function: pointer to slot 0.
        assert_eq!(entry(&t, 0), [0x02, 0, 0, 0]);
        assert_eq!(entry(&t, FN_SINGLE), [0, 0, 0, 0x29]);
        assert_eq!(entry(&t, FN_COMBO), [0x0e, 0, 0, 0x04]);
        // Q (matrix 8) is a plain key.
        assert_eq!(entry(&t, 8), [0, 0, 0, 0x14]);
        // Enter becomes usage 0x28, Fn key is 20 00 00 00.
        assert_eq!(entry(&t, 81), [0, 0, 0, 0x28]);
        assert_eq!(entry(&t, 59), [0x20, 0, 0, 0]);
        // F8 (slot 8) Fn layer = next track.
        assert_eq!(entry(&t, FN_COMBO + 8), [0x04, 0, 0, 0xB5]);
        // Nothing past entry 227.
        assert!(t[228 * 4..].iter().all(|b| *b == 0));
    }

    #[test]
    fn remaps() {
        let mut k = KeySettings::default();
        k.normal.insert(51, KeyAction::Key { mods: 0, usage: 0xE0 }); // Caps -> Left Ctrl
        k.normal.insert(35, KeyAction::Key { mods: MOD_CTRL, usage: 0x06 }); // Q -> Ctrl+C
        k.normal.insert(2, KeyAction::Media(0xCD)); // F1 -> play (F1 has an Fn slot)
        k.fn_layer.insert(52, KeyAction::Mouse(MouseAction::Left)); // Fn+A -> left click
        let t = build_key_table(&k);
        assert_eq!(entry(&t, 3), [0, 0, 0, 0xE0]);
        assert_eq!(entry(&t, 8), [0, 0, MOD_CTRL, 0x06]);
        assert_eq!(entry(&t, 12), [0x02, 0, 0, 1]);
        assert_eq!(entry(&t, FN_SINGLE + 1), [0x04, 0, 0, 0xCD]);
        assert_eq!(entry(&t, FN_COMBO + 1), [0x04, 0, 0x01, 0x94]);
        let slot = DEFAULT_FN.len();
        assert_eq!(entry(&t, 9), [0x02, 0, 0, slot as u8]);
        assert_eq!(entry(&t, FN_SINGLE + slot), [0, 0, 0, 0x04]);
        assert_eq!(entry(&t, FN_COMBO + slot), [0x07, 0, 0, 0xF0]);
    }

    #[test]
    fn lighting_round_trip() {
        let mut s = [0u8; SETTINGS_LEN];
        s[0] = 0x11; // global bytes are kept
        s[GLOBAL_LEN + PROFILE_LEN + 3] = 0x22; // second profile is kept
        let mut l = default_lighting();
        l.effect = 9;
        l.per_effect[9] = EffectSettings { colour: None, speed: 2, brightness: 4, direction: 1 };
        l.per_effect[1] = EffectSettings { colour: Some(3), speed: 5, brightness: 1, direction: 0 };
        encode_lighting(&mut s, &l);
        let p = &s[GLOBAL_LEN..];
        assert_eq!(p[P_EFFECT], 10);
        assert_eq!(p[P_GAME_FLAG], 0);
        assert_eq!(p[P_PARAMS + 18], 0x07); // rainbow wheel, direction 1 -> no bit
        assert_eq!(p[P_PARAMS + 19], 0x13);
        assert_eq!(p[P_PARAMS + 2], 0x03);
        assert_eq!(p[P_PARAMS + 3], 0x40);
        assert_eq!(&p[62..64], &[0x5A, 0xA5]);
        assert_eq!(s[0], 0x11);
        assert_eq!(s[GLOBAL_LEN + PROFILE_LEN + 3], 0x22);
        let back = decode_lighting(&s);
        assert_eq!(back.effect, 9);
        assert_eq!(back.per_effect[9], l.per_effect[9]);
        assert_eq!(back.per_effect[1], l.per_effect[1]);

        l.effect = EFFECT_OFF;
        encode_lighting(&mut s, &l);
        assert_eq!(s[GLOBAL_LEN + P_EFFECT], 0);
        l.effect = EFFECT_GAME;
        l.group = 2;
        encode_lighting(&mut s, &l);
        assert_eq!(s[GLOBAL_LEN + P_GAME_FLAG], 1);
        assert_eq!(s[GLOBAL_LEN + P_EFFECT], 15);
        assert_eq!(s[GLOBAL_LEN + P_GROUP], 0x22);
    }

    /// Read-only check against a plugged-in keyboard: asks for the settings block (the same
    /// request the original makes) and prints it. Nothing is written. Run with
    /// `cargo test read_settings_from_keyboard -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn read_settings_from_keyboard() {
        let link = Link::open().expect("keyboard");
        let s = link.read_settings().expect("settings");
        for (i, chunk) in s.chunks(16).enumerate() {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
            println!("{:3}: {}", i * 16, hex.join(" "));
        }
        let l = decode_lighting(&s);
        println!("signature: {:02X} {:02X}", s[GLOBAL_LEN + 62], s[GLOBAL_LEN + 63]);
        println!("effect: {} ({})", l.effect, EFFECTS[l.effect].name);
        if let Some(p) = l.per_effect.get(l.effect) {
            println!("settings: {p:?}");
        }
    }

    #[test]
    fn any_colour() {
        let mut s = [0u8; SETTINGS_LEN];
        let mut l = default_lighting();
        l.effect = EFFECT_ANY;
        l.any = AnyColour { rgb: [255, 127, 0], brightness: 5 };
        encode_lighting(&mut s, &l);
        let p = &s[GLOBAL_LEN..];
        assert_eq!((p[P_GAME_FLAG], p[P_EFFECT], p[P_GROUP]), (1, 0x0F, 0x24));
        let m = any_colour_payload(&l.any);
        assert_eq!(m.len(), 792);
        // Esc (slot 0): blue, green, red planes
        assert_eq!((m[0], m[132], m[264]), (0x00, 0x7F, 0xFF));
        // Space (35) and Up (100), and 57
        assert_eq!((m[35], m[132 + 35], m[264 + 35]), (0x00, 0x7F, 0xFF));
        assert_eq!(m[264 + 57], 0xFF);
        assert_eq!(m[264 + 100], 0xFF);
        // every slot, even ones with no key in the key table
        assert!((0..132).all(|k| (m[k], m[132 + k], m[264 + k]) == (0x00, 0x7F, 0xFF)));
        // second half untouched
        assert!(m[396..].iter().all(|b| *b == 0));
        // dimmer: 50 %
        let half = any_colour_payload(&AnyColour { rgb: [255, 127, 0], brightness: 3 });
        assert_eq!((half[0], half[132], half[264]), (0, 64, 128));
        let r = data_report(&HDR_WRITE_GROUP_M5, &m);
        assert_eq!(&r[..8], &[0x06, 0x09, 0xB4, 0x00, 0x40, 0x00, 0x00, 0x00]);
        // reading it back gives Game mode, group M5
        let back = decode_lighting(&s);
        assert_eq!((back.effect, back.group), (EFFECT_GAME, ANY_COLOUR_GROUP));
    }

    #[test]
    fn report_shape() {
        let r = data_report(&HDR_WRITE_KEYS, &[0xAB; 1024]);
        assert_eq!(r.len(), 1032);
        assert_eq!(&r[..8], &[0x06, 0x04, 0xC8, 0x00, 0x40, 0x00, 0x00, 0x00]);
        assert_eq!(r[8], 0xAB);
        assert_eq!(r[1031], 0xAB);
    }
}
