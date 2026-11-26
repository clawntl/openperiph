//! QMK keycodes (the "latest" numbering, used by VIA protocol 12 firmware: QMK 0.20 from 2023
//! onwards): names, parsing, and the short text printed on a key cap.
//!
//! Single keycodes come from QMK's own keycode spec (`keycodes_table.rs`, generated). Keycodes
//! that take arguments, such as `MO(1)` or `LT(2, KC_A)`, are numeric ranges handled here.

use std::sync::OnceLock;

use super::keycodes_table::KEYCODES;

#[derive(Clone, Copy, Debug)]
pub struct Kc {
    pub code: u16,
    /// QMK's full name, for example `KC_ENTER`.
    pub name: &'static str,
    /// QMK's shortest alias, for example `KC_ENT`.
    pub short: &'static str,
    /// QMK's own label, when it has one ("Enter").
    pub label: &'static str,
    pub group: &'static str,
    /// Every alias QMK accepts.
    pub aliases: &'static [&'static str],
}

/// A keyboard-specific keycode from a VIA definition's `customKeycodes`.
#[derive(Clone, Debug, Default)]
pub struct CustomKeycode {
    pub name: String,
    pub title: String,
    pub short: String,
}

pub const KC_NO: u16 = 0x0000;
pub const KC_TRNS: u16 = 0x0001;

const QK_MODS: u16 = 0x0100;
const QK_MOD_TAP: u16 = 0x2000;
const QK_LAYER_TAP: u16 = 0x4000;
const QK_LAYER_MOD: u16 = 0x5000;
const QK_TO: u16 = 0x5200;
const QK_MOMENTARY: u16 = 0x5220;
const QK_DEF_LAYER: u16 = 0x5240;
const QK_TOGGLE_LAYER: u16 = 0x5260;
const QK_ONE_SHOT_LAYER: u16 = 0x5280;
const QK_ONE_SHOT_MOD: u16 = 0x52A0;
const QK_LAYER_TAP_TOGGLE: u16 = 0x52C0;
const QK_PERSISTENT_DEF_LAYER: u16 = 0x52E0;
const QK_TAP_DANCE: u16 = 0x5700;
pub const QK_MACRO: u16 = 0x7700;
pub const QK_KB: u16 = 0x7E00;
const QK_USER: u16 = 0x7E40;

/// Modifier bits as QMK packs them into keycodes: Ctrl, Shift, Alt, GUI, and "right-hand".
pub const MOD_CTL: u8 = 0x01;
pub const MOD_SFT: u8 = 0x02;
pub const MOD_ALT: u8 = 0x04;
pub const MOD_GUI: u8 = 0x08;
pub const MOD_RIGHT: u8 = 0x10;

fn by_code() -> &'static std::collections::HashMap<u16, &'static Kc> {
    static MAP: OnceLock<std::collections::HashMap<u16, &'static Kc>> = OnceLock::new();
    MAP.get_or_init(|| KEYCODES.iter().map(|k| (k.code, k)).collect())
}

fn by_name() -> &'static std::collections::HashMap<&'static str, u16> {
    static MAP: OnceLock<std::collections::HashMap<&'static str, u16>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = std::collections::HashMap::new();
        for k in KEYCODES {
            m.insert(k.name, k.code);
            for a in k.aliases {
                m.entry(*a).or_insert(k.code);
            }
        }
        // Aliases QMK keeps in its headers rather than the spec, plus VIA's older names.
        for (n, c) in EXTRA_NAMES {
            m.entry(*n).or_insert(*c);
        }
        m
    })
}

/// Names that appear in keymaps and VIA backups but aren't aliases in QMK's keycode spec.
const EXTRA_NAMES: &[(&str, u16)] = &[
    ("_______", KC_TRNS),
    ("XXXXXXX", KC_NO),
    ("KC_TRANSPARENT", KC_TRNS),
    ("RESET", 0x7C00),
    ("RGB_TOG", 0x7820),
    ("RGB_MOD", 0x7821),
    ("RGB_RMOD", 0x7822),
    ("RGB_HUI", 0x7823),
    ("RGB_HUD", 0x7824),
    ("RGB_SAI", 0x7825),
    ("RGB_SAD", 0x7826),
    ("RGB_VAI", 0x7827),
    ("RGB_VAD", 0x7828),
    ("RGB_SPI", 0x7829),
    ("RGB_SPD", 0x782A),
];

pub fn lookup(code: u16) -> Option<&'static Kc> {
    by_code().get(&code).copied()
}

pub fn all() -> &'static [Kc] {
    KEYCODES
}

// ---------------------------------------------------------------------------------------------
// Modifiers

/// `MOD_LCTL | MOD_LSFT` style text for a 5-bit modifier mask.
pub fn mod_mask_name(m: u8) -> String {
    let side = if m & MOD_RIGHT != 0 { 'R' } else { 'L' };
    let mut parts = Vec::new();
    for (bit, n) in [(MOD_CTL, "CTL"), (MOD_SFT, "SFT"), (MOD_ALT, "ALT"), (MOD_GUI, "GUI")] {
        if m & bit != 0 {
            parts.push(format!("MOD_{side}{n}"));
        }
    }
    if parts.is_empty() {
        "0".into()
    } else {
        parts.join(" | ")
    }
}

/// Human text for a modifier mask: "Ctrl+Shift", "Right Alt".
pub fn mod_mask_words(m: u8) -> String {
    let mut parts = Vec::new();
    for (bit, n) in [(MOD_CTL, "Ctrl"), (MOD_SFT, "Shift"), (MOD_ALT, "Alt"), (MOD_GUI, "Win")] {
        if m & bit != 0 {
            parts.push(n);
        }
    }
    let s = parts.join("+");
    if m & MOD_RIGHT != 0 {
        format!("Right {s}")
    } else {
        s
    }
}

fn parse_mod_mask(s: &str) -> Option<u8> {
    let mut m = 0u8;
    for part in s.split('|') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(v) = parse_number(p) {
            m |= v as u8;
            continue;
        }
        let v = match p {
            "MOD_LCTL" => MOD_CTL,
            "MOD_LSFT" => MOD_SFT,
            "MOD_LALT" => MOD_ALT,
            "MOD_LGUI" => MOD_GUI,
            "MOD_RCTL" => MOD_RIGHT | MOD_CTL,
            "MOD_RSFT" => MOD_RIGHT | MOD_SFT,
            "MOD_RALT" => MOD_RIGHT | MOD_ALT,
            "MOD_RGUI" => MOD_RIGHT | MOD_GUI,
            "MOD_MEH" => MOD_CTL | MOD_SFT | MOD_ALT,
            "MOD_HYPR" => MOD_CTL | MOD_SFT | MOD_ALT | MOD_GUI,
            _ => return None,
        };
        m |= v;
    }
    Some(m & 0x1F)
}

/// Modifier wrapper functions (`LCTL(kc)`) and their mask.
const MOD_FUNCS: &[(&str, u8)] = &[
    ("LCTL", MOD_CTL),
    ("LSFT", MOD_SFT),
    ("LALT", MOD_ALT),
    ("LGUI", MOD_GUI),
    ("LCMD", MOD_GUI),
    ("LWIN", MOD_GUI),
    ("LOPT", MOD_ALT),
    ("RCTL", MOD_RIGHT | MOD_CTL),
    ("RSFT", MOD_RIGHT | MOD_SFT),
    ("RALT", MOD_RIGHT | MOD_ALT),
    ("ALGR", MOD_RIGHT | MOD_ALT),
    ("RGUI", MOD_RIGHT | MOD_GUI),
    ("RCMD", MOD_RIGHT | MOD_GUI),
    ("RWIN", MOD_RIGHT | MOD_GUI),
    ("ROPT", MOD_RIGHT | MOD_ALT),
    ("C", MOD_CTL),
    ("S", MOD_SFT),
    ("A", MOD_ALT),
    ("G", MOD_GUI),
    ("MEH", MOD_CTL | MOD_SFT | MOD_ALT),
    ("HYPR", MOD_CTL | MOD_SFT | MOD_ALT | MOD_GUI),
];

// ---------------------------------------------------------------------------------------------
// Names

fn basic_name(code: u16) -> String {
    lookup(code).map(|k| k.short.to_string()).unwrap_or_else(|| format!("0x{code:04X}"))
}

/// The QMK expression for a keycode: `KC_A`, `MO(1)`, `LT(2, KC_SPC)`, `LCTL(KC_C)`, or hex.
pub fn name(code: u16) -> String {
    if let Some(k) = lookup(code) {
        return k.short.to_string();
    }
    let arg5 = code & 0x1F;
    match code {
        0x0100..=0x1FFF => {
            let m = ((code >> 8) & 0x1F) as u8;
            let kc = basic_name(code & 0xFF);
            let right = m & MOD_RIGHT != 0;
            let mut out = kc;
            for (bit, l, r) in [(MOD_GUI, "LGUI", "RGUI"), (MOD_ALT, "LALT", "RALT"), (MOD_SFT, "LSFT", "RSFT"), (MOD_CTL, "LCTL", "RCTL")] {
                if m & bit != 0 {
                    out = format!("{}({out})", if right { r } else { l });
                }
            }
            out
        }
        0x2000..=0x3FFF => format!("MT({}, {})", mod_mask_name(((code >> 8) & 0x1F) as u8), basic_name(code & 0xFF)),
        0x4000..=0x4FFF => format!("LT({}, {})", (code >> 8) & 0xF, basic_name(code & 0xFF)),
        0x5000..=0x51FF => format!("LM({}, {})", (code >> 5) & 0xF, mod_mask_name((code & 0x1F) as u8)),
        0x5200..=0x521F => format!("TO({arg5})"),
        0x5220..=0x523F => format!("MO({arg5})"),
        0x5240..=0x525F => format!("DF({arg5})"),
        0x5260..=0x527F => format!("TG({arg5})"),
        0x5280..=0x529F => format!("OSL({arg5})"),
        0x52A0..=0x52BF => format!("OSM({})", mod_mask_name(arg5 as u8)),
        0x52C0..=0x52DF => format!("TT({arg5})"),
        0x52E0..=0x52FF => format!("PDF({arg5})"),
        0x5700..=0x57FF => format!("TD({})", code & 0xFF),
        0x7700..=0x777F => format!("MC_{}", code - QK_MACRO),
        0x7E00..=0x7E3F => format!("QK_KB_{}", code - QK_KB),
        0x7E40..=0x7FFF => format!("QK_USER_{}", code - QK_USER),
        _ => format!("0x{code:04X}"),
    }
}

fn parse_number(s: &str) -> Option<u16> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u16::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

/// Splits `F(a, b)` into `F` and its top-level arguments.
fn call(s: &str) -> Option<(&str, Vec<&str>)> {
    let open = s.find('(')?;
    if !s.ends_with(')') {
        return None;
    }
    let f = s[..open].trim();
    let inner = &s[open + 1..s.len() - 1];
    let mut args = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim());
    Some((f, args))
}

/// Reads a keycode written as a QMK name or expression, or a number. `custom` are the
/// keyboard's own keycodes (QK_KB_0 upwards) by name.
pub fn parse(s: &str, custom: &[CustomKeycode]) -> Option<u16> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(v) = parse_number(s) {
        return Some(v);
    }
    let upper = s.to_ascii_uppercase();
    if let Some(&c) = by_name().get(upper.as_str()) {
        return Some(c);
    }
    if let Some(&c) = by_name().get(format!("KC_{upper}").as_str()) {
        return Some(c);
    }
    if let Some(i) = custom.iter().position(|c| c.name.eq_ignore_ascii_case(s)) {
        return Some(QK_KB + i as u16);
    }
    for (prefix, base, max) in [("MC_", QK_MACRO, 127u16), ("M", QK_MACRO, 127), ("QK_KB_", QK_KB, 63), ("QK_USER_", QK_USER, 447)] {
        if let Some(n) = upper.strip_prefix(prefix).and_then(|n| n.parse::<u16>().ok()) {
            if n <= max {
                return Some(base + n);
            }
        }
    }
    let (f, args) = call(&upper)?;
    let layer = |a: &str| parse_number(a).filter(|l| *l < 32);
    let one = |base: u16| -> Option<u16> {
        if args.len() == 1 {
            Some(base | layer(args[0])?)
        } else {
            None
        }
    };
    match f {
        "TO" => one(QK_TO),
        "MO" => one(QK_MOMENTARY),
        "DF" => one(QK_DEF_LAYER),
        "TG" => one(QK_TOGGLE_LAYER),
        "OSL" => one(QK_ONE_SHOT_LAYER),
        "TT" => one(QK_LAYER_TAP_TOGGLE),
        "PDF" => one(QK_PERSISTENT_DEF_LAYER),
        "OSM" if args.len() == 1 => Some(QK_ONE_SHOT_MOD | parse_mod_mask(args[0])? as u16),
        "TD" if args.len() == 1 => Some(QK_TAP_DANCE | (parse_number(args[0])? & 0xFF)),
        "LT" if args.len() == 2 => {
            let l = parse_number(args[0]).filter(|l| *l < 16)?;
            let kc = parse(args[1], custom).filter(|k| *k <= 0xFF)?;
            Some(QK_LAYER_TAP | (l << 8) | kc)
        }
        "LM" if args.len() == 2 => {
            let l = parse_number(args[0]).filter(|l| *l < 16)?;
            Some(QK_LAYER_MOD | (l << 5) | parse_mod_mask(args[1])? as u16)
        }
        "MT" if args.len() == 2 => {
            let m = parse_mod_mask(args[0])?;
            let kc = parse(args[1], custom).filter(|k| *k <= 0xFF)?;
            Some(QK_MOD_TAP | ((m as u16) << 8) | kc)
        }
        _ => {
            // LCTL(KC_A), LCTL_T(KC_A) and friends
            if args.len() != 1 {
                return None;
            }
            let (tap, fname) = match f.strip_suffix("_T") {
                Some(n) => (true, n),
                None => (false, f),
            };
            let (_, m) = MOD_FUNCS.iter().find(|(n, _)| *n == fname)?;
            let inner = parse(args[0], custom)?;
            if tap {
                (inner <= 0xFF).then(|| QK_MOD_TAP | ((*m as u16) << 8) | inner)
            } else if inner <= 0xFF || (QK_MODS..=0x1FFF).contains(&inner) {
                // mods stack; a right-hand modifier makes all of them right-hand, as in QMK
                let existing = ((inner >> 8) & 0x1F) as u8;
                let mm = (existing | *m) & 0x1F;
                Some(QK_MODS | ((mm as u16) << 8) | (inner & 0xFF))
            } else {
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Key-cap text

/// QMK and VIA write American English; the app shows British spellings.
pub fn british(s: &str) -> String {
    let mut out = s.to_string();
    for (us, uk) in [
        ("Colors", "Colours"),
        ("colors", "colours"),
        ("Color", "Colour"),
        ("color", "colour"),
        ("COLOR", "COLOUR"),
        ("Favorites", "Favourites"),
        ("favorites", "favourites"),
        ("Favorite", "Favourite"),
        ("Center", "Centre"),
        ("center", "centre"),
        ("Gray", "Grey"),
        ("gray", "grey"),
    ] {
        out = out.replace(us, uk);
    }
    out
}

/// Friendly text for keycodes QMK gives no label to.
const FRIENDLY: &[(&str, &str)] = &[
    ("KC_NO", ""),
    ("KC_TRNS", "▽"),
    ("QK_BOOT", "Boot"),
    ("QK_RBT", "Reboot"),
    ("EE_CLR", "Clear EEPROM"),
    ("DB_TOGG", "Debug"),
    ("QK_GESC", "Esc `"),
    ("CW_TOGG", "Caps Word"),
    ("QK_REP", "Repeat"),
    ("QK_AREP", "Alt Repeat"),
    ("QK_LLCK", "Layer Lock"),
    ("QK_LOCK", "Key Lock"),
    ("AS_TOGG", "Auto Shift"),
    ("GU_TOGG", "Win Lock"),
    ("GU_OFF", "Win On"),
    ("GU_ON", "Win Off"),
    ("NK_TOGG", "NKRO"),
    ("SC_LSPO", "LSft ("),
    ("SC_RSPC", "RSft )"),
    ("KC_LCTL", "LCtrl"),
    ("KC_LSFT", "LShift"),
    ("KC_LALT", "LAlt"),
    ("KC_LGUI", "LWin"),
    ("KC_RCTL", "RCtrl"),
    ("KC_RSFT", "RShift"),
    ("KC_RALT", "AltGr"),
    ("KC_RGUI", "RWin"),
    ("RM_TOGG", "RGB Toggle"),
    ("RM_NEXT", "RGB Next"),
    ("RM_PREV", "RGB Prev"),
    ("RM_HUEU", "Hue +"),
    ("RM_HUED", "Hue −"),
    ("RM_SATU", "Sat +"),
    ("RM_SATD", "Sat −"),
    ("RM_VALU", "Bright +"),
    ("RM_VALD", "Bright −"),
    ("RM_SPDU", "Speed +"),
    ("RM_SPDD", "Speed −"),
    ("RM_ON", "RGB On"),
    ("RM_OFF", "RGB Off"),
    ("KC_EJCT", "Eject"),
];

fn friendly(short: &str) -> Option<&'static str> {
    FRIENDLY.iter().find(|(n, _)| *n == short).map(|(_, f)| *f)
}

/// Turns `QK_RGB_MATRIX_TOGGLE` / `UG_TOGG` style names into words.
fn words_from_name(n: &str) -> String {
    let n = n
        .trim_start_matches("KC_")
        .trim_start_matches("QK_")
        .trim_start_matches("MS_")
        .replace('_', " ");
    let mut out = String::new();
    for (i, w) in n.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let lw = w.to_ascii_lowercase();
        let mut c = lw.chars();
        if let Some(f) = c.next() {
            out.push(f.to_ascii_uppercase());
            out.extend(c);
        }
    }
    out
}

/// Short text for one plain keycode.
fn single_label(code: u16, custom: &[CustomKeycode]) -> String {
    if (QK_KB..QK_KB + 64).contains(&code) {
        if let Some(c) = custom.get((code - QK_KB) as usize) {
            return if !c.short.is_empty() { c.short.clone() } else { c.name.clone() };
        }
    }
    if let Some(k) = lookup(code) {
        if let Some(f) = friendly(k.short) {
            return f.to_string();
        }
        if !k.label.is_empty() {
            return british(k.label);
        }
        let s = k.short;
        for (p, w) in [("UG_", "Glow "), ("BL_", "Light "), ("LM_", "LED "), ("MC_", "M")] {
            if let Some(rest) = s.strip_prefix(p) {
                return british(&format!("{w}{}", words_from_name(rest)));
            }
        }
        return british(&words_from_name(s));
    }
    name(code)
}

/// Text for a key cap: a main line and an optional second line (for tap-hold keys).
pub fn cap(code: u16, custom: &[CustomKeycode]) -> (String, Option<String>) {
    let arg5 = code & 0x1F;
    match code {
        0x0100..=0x1FFF => {
            let m = ((code >> 8) & 0x1F) as u8;
            (single_label(code & 0xFF, custom), Some(mod_mask_words(m)))
        }
        0x2000..=0x3FFF => (single_label(code & 0xFF, custom), Some(format!("hold {}", mod_mask_words(((code >> 8) & 0x1F) as u8)))),
        0x4000..=0x4FFF => (single_label(code & 0xFF, custom), Some(format!("hold L{}", (code >> 8) & 0xF))),
        0x5000..=0x51FF => (format!("LM {}", (code >> 5) & 0xF), Some(mod_mask_words((code & 0x1F) as u8))),
        0x5200..=0x521F => (format!("TO {arg5}"), None),
        0x5220..=0x523F => (format!("MO {arg5}"), None),
        0x5240..=0x525F => (format!("DF {arg5}"), None),
        0x5260..=0x527F => (format!("TG {arg5}"), None),
        0x5280..=0x529F => (format!("OSL {arg5}"), None),
        0x52A0..=0x52BF => ("One shot".into(), Some(mod_mask_words(arg5 as u8))),
        0x52C0..=0x52DF => (format!("TT {arg5}"), None),
        0x52E0..=0x52FF => (format!("PDF {arg5}"), None),
        0x5700..=0x57FF => (format!("TD {}", code & 0xFF), None),
        0x7700..=0x777F => (format!("M{}", code - QK_MACRO), None),
        _ => (single_label(code, custom), None),
    }
}

/// One line describing a keycode, for the editor.
pub fn describe(code: u16, custom: &[CustomKeycode]) -> String {
    match code {
        KC_NO => "Nothing (the key is off)".into(),
        KC_TRNS => "Transparent: falls through to the layer below".into(),
        0x0100..=0x1FFF => format!("{} + {}", mod_mask_words(((code >> 8) & 0x1F) as u8), single_label(code & 0xFF, custom)),
        0x2000..=0x3FFF => format!("{} when tapped, {} when held", single_label(code & 0xFF, custom), mod_mask_words(((code >> 8) & 0x1F) as u8)),
        0x4000..=0x4FFF => format!("{} when tapped, layer {} while held", single_label(code & 0xFF, custom), (code >> 8) & 0xF),
        0x5000..=0x51FF => format!("Layer {} with {} while held", (code >> 5) & 0xF, mod_mask_words((code & 0x1F) as u8)),
        0x5200..=0x521F => format!("Switch to layer {}", code & 0x1F),
        0x5220..=0x523F => format!("Layer {} while held", code & 0x1F),
        0x5240..=0x525F => format!("Make layer {} the base layer", code & 0x1F),
        0x5260..=0x527F => format!("Turn layer {} on or off", code & 0x1F),
        0x5280..=0x529F => format!("Layer {} for the next key press", code & 0x1F),
        0x52A0..=0x52BF => format!("{} for the next key press", mod_mask_words((code & 0x1F) as u8)),
        0x52C0..=0x52DF => format!("Layer {} while held, toggled by tapping", code & 0x1F),
        0x52E0..=0x52FF => format!("Make layer {} the base layer, saved", code & 0x1F),
        0x7700..=0x777F => format!("Macro {}", code - QK_MACRO),
        _ => {
            if (QK_KB..QK_KB + 64).contains(&code) {
                if let Some(c) = custom.get((code - QK_KB) as usize) {
                    return if c.title.is_empty() { c.name.clone() } else { c.title.clone() };
                }
            }
            let (a, _) = cap(code, custom);
            if a.is_empty() {
                name(code)
            } else {
                a
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        let custom = vec![CustomKeycode { name: "MY_KEY".into(), ..Default::default() }];
        for code in [
            0x0004u16, 0x0028, 0x00A8, 0x0001, 0x0000, 0x0104, 0x0306, 0x1104, 0x2104, 0x2F2C, 0x4129, 0x5021, 0x5201,
            0x5221, 0x5242, 0x5263, 0x5284, 0x52A2, 0x52C1, 0x5701, 0x7700, 0x7705, 0x7C00, 0x7840, 0x7E00, 0x7E41,
        ] {
            let n = name(code);
            assert_eq!(parse(&n, &custom), Some(code), "{n} ({code:#06X})");
        }
    }

    #[test]
    fn parses_common_forms() {
        let none: [CustomKeycode; 0] = [];
        assert_eq!(parse("KC_A", &none), Some(0x0004));
        assert_eq!(parse("a", &none), Some(0x0004));
        assert_eq!(parse("MO(1)", &none), Some(0x5221));
        assert_eq!(parse("LT(2, KC_SPC)", &none), Some(0x422C));
        assert_eq!(parse("LCTL(KC_C)", &none), Some(0x0106));
        assert_eq!(parse("C(S(KC_ESC))", &none), Some(0x0329));
        assert_eq!(parse("LCTL_T(KC_A)", &none), Some(0x2104));
        assert_eq!(parse("MT(MOD_LCTL | MOD_LSFT, KC_A)", &none), Some(0x2304));
        assert_eq!(parse("QK_BOOT", &none), Some(0x7C00));
        assert_eq!(parse("RGB_TOG", &none), Some(0x7820));
        assert_eq!(parse("0x7E05", &none), Some(0x7E05));
        assert_eq!(parse("nonsense(", &none), None);
    }

    #[test]
    fn caps_read_well() {
        let none: [CustomKeycode; 0] = [];
        assert_eq!(cap(0x0004, &none).0, "A");
        assert_eq!(cap(0x5221, &none).0, "MO 1");
        assert_eq!(cap(0x0106, &none), ("C".into(), Some("Ctrl".into())));
        assert_eq!(cap(0x7C00, &none).0, "Boot");
    }
}
