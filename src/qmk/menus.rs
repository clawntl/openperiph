//! VIA settings menus: lighting and keyboard-specific options. A definition lists built-in menus
//! by name ("qmk_rgb_matrix") or describes its own; both become the same controls here. Each
//! control reads and writes one value through VIA's custom-value commands: `[channel, value id]`.

use std::collections::HashMap;

use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum ControlKind {
    Range {
        min: i64,
        max: i64,
    },
    /// (label, value)
    Dropdown(Vec<(String, i64)>),
    Toggle {
        off: i64,
        on: i64,
    },
    /// Hue and saturation, one byte each.
    Color,
    Keycode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub label: String,
    pub kind: ControlKind,
    /// The value's name, used by `showIf` expressions (`id_qmk_rgb_matrix_effect`).
    pub key: String,
    pub channel: u8,
    pub id: u8,
    pub show_if: Option<String>,
}

impl Control {
    /// Bytes the value takes.
    pub fn len(&self) -> usize {
        match &self.kind {
            ControlKind::Range { max, .. } if *max > 255 => 2,
            ControlKind::Color | ControlKind::Keycode => 2,
            ControlKind::Dropdown(o) if o.iter().any(|(_, v)| *v > 255) => 2,
            _ => 1,
        }
    }

    /// A value's bytes as a number (colour: hue only).
    pub fn number(&self, bytes: &[u8]) -> i64 {
        match self.len() {
            2 if !matches!(self.kind, ControlKind::Color) => u16::from_be_bytes([bytes.first().copied().unwrap_or(0), bytes.get(1).copied().unwrap_or(0)]) as i64,
            _ => bytes.first().copied().unwrap_or(0) as i64,
        }
    }

    pub fn bytes(&self, v: i64) -> Vec<u8> {
        if self.len() == 2 && !matches!(self.kind, ControlKind::Color) {
            (v as u16).to_be_bytes().to_vec()
        } else {
            vec![v as u8]
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub label: String,
    pub controls: Vec<Control>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub label: String,
    pub sections: Vec<Section>,
}

// ---------------------------------------------------------------------------------------------
// Built-in menus (the ones VIA has under these names)

const RGB_MATRIX_EFFECTS: &[&str] = &[
    "All off",
    "Solid colour",
    "Alphas mods",
    "Gradient up down",
    "Gradient left right",
    "Breathing",
    "Band sat.",
    "Band val.",
    "Pinwheel sat.",
    "Pinwheel val.",
    "Spiral sat.",
    "Spiral val.",
    "Cycle all",
    "Cycle left right",
    "Cycle up down",
    "Rainbow chevron",
    "Cycle out in",
    "Cycle out in dual",
    "Cycle pinwheel",
    "Cycle spiral",
    "Dual beacon",
    "Rainbow beacon",
    "Rainbow pinwheels",
    "Flower blooming",
    "Raindrops",
    "Jellybean raindrops",
    "Hue breathing",
    "Hue pendulum",
    "Hue wave",
    "Pixel fractal",
    "Pixel flow",
    "Pixel rain",
    "Typing heatmap",
    "Digital rain",
    "Solid reactive simple",
    "Solid reactive",
    "Solid reactive wide",
    "Solid reactive multi wide",
    "Solid reactive cross",
    "Solid reactive multi cross",
    "Solid reactive nexus",
    "Solid reactive multi nexus",
    "Splash",
    "Multi splash",
    "Solid splash",
    "Solid multi splash",
];

const RGBLIGHT_EFFECTS: &[&str] = &[
    "All off",
    "Solid colour",
    "Breathing 1",
    "Breathing 2",
    "Breathing 3",
    "Breathing 4",
    "Rainbow mood 1",
    "Rainbow mood 2",
    "Rainbow mood 3",
    "Rainbow swirl 1",
    "Rainbow swirl 2",
    "Rainbow swirl 3",
    "Rainbow swirl 4",
    "Rainbow swirl 5",
    "Rainbow swirl 6",
    "Snake 1",
    "Snake 2",
    "Snake 3",
    "Snake 4",
    "Snake 5",
    "Snake 6",
    "Knight 1",
    "Knight 2",
    "Knight 3",
    "Christmas",
    "Gradient 1",
    "Gradient 2",
    "Gradient 3",
    "Gradient 4",
    "Gradient 5",
    "Gradient 6",
    "Gradient 7",
    "Gradient 8",
    "Gradient 9",
    "Gradient 10",
    "RGB test",
    "Alternating",
    "Twinkle 1",
    "Twinkle 2",
    "Twinkle 3",
    "Twinkle 4",
    "Twinkle 5",
    "Twinkle 6",
];

const LED_MATRIX_EFFECTS: &[&str] = &[
    "All off",
    "Solid",
    "Alphas mods",
    "Breathing",
    "Band",
    "Band pinwheel",
    "Band spiral",
    "Cycle left right",
    "Cycle up down",
    "Cycle out in",
    "Dual beacon",
    "Solid reactive simple",
    "Solid reactive wide",
    "Solid reactive multi wide",
    "Solid reactive cross",
    "Solid reactive multi cross",
    "Solid reactive nexus",
    "Solid reactive multi nexus",
    "Solid splash",
    "Solid multi splash",
    "Wave left right",
    "Wave up down",
];

fn effects(names: &[&str]) -> Value {
    Value::Array(names.iter().enumerate().map(|(i, n)| json!([n, i])).collect())
}

/// The JSON of a built-in menu, in VIA's custom-menu format.
pub fn builtin(name: &str) -> Option<Value> {
    let backlight = || {
        json!({"label": "Backlight", "content": [
            {"label": "Brightness", "type": "range", "options": [0, 255], "content": ["id_qmk_backlight_brightness", 1, 1]},
            {"label": "Breathing", "type": "toggle", "content": ["id_qmk_backlight_effect", 1, 2]}
        ]})
    };
    let underglow = || {
        json!({"label": "Underglow", "content": [
            {"label": "Brightness", "type": "range", "options": [0, 255], "content": ["id_qmk_rgblight_brightness", 2, 1]},
            {"label": "Effect", "type": "dropdown", "options": effects(RGBLIGHT_EFFECTS), "content": ["id_qmk_rgblight_effect", 2, 2]},
            {"showIf": "{id_qmk_rgblight_effect} != 0", "label": "Effect speed", "type": "range", "options": [0, 255], "content": ["id_qmk_rgblight_effect_speed", 2, 3]},
            {"showIf": "{id_qmk_rgblight_effect} != 0", "label": "Colour", "type": "color", "content": ["id_qmk_rgblight_color", 2, 4]}
        ]})
    };
    Some(match name {
        "qmk_backlight" => json!({"label": "Lighting", "content": [backlight()]}),
        "qmk_rgblight" => json!({"label": "Lighting", "content": [underglow()]}),
        "qmk_backlight_rgblight" => {
            json!({"label": "Lighting", "content": [backlight(), underglow()]})
        }
        "qmk_rgb_matrix" => {
            json!({"label": "Lighting", "content": [{"label": "Backlight", "content": [
                {"label": "Brightness", "type": "range", "options": [0, 255], "content": ["id_qmk_rgb_matrix_brightness", 3, 1]},
                {"label": "Effect", "type": "dropdown", "options": effects(RGB_MATRIX_EFFECTS), "content": ["id_qmk_rgb_matrix_effect", 3, 2]},
                {"showIf": "{id_qmk_rgb_matrix_effect} != 0", "label": "Effect speed", "type": "range", "options": [0, 255], "content": ["id_qmk_rgb_matrix_effect_speed", 3, 3]},
                {"showIf": "{id_qmk_rgb_matrix_effect} != 0", "label": "Colour", "type": "color", "content": ["id_qmk_rgb_matrix_color", 3, 4]}
            ]}]})
        }
        "qmk_led_matrix" => {
            json!({"label": "Lighting", "content": [{"label": "Backlight", "content": [
                {"label": "Brightness", "type": "range", "options": [0, 255], "content": ["id_qmk_led_matrix_brightness", 5, 1]},
                {"label": "Effect", "type": "dropdown", "options": effects(LED_MATRIX_EFFECTS), "content": ["id_qmk_led_matrix_effect", 5, 2]},
                {"showIf": "{id_qmk_led_matrix_effect} != 0", "label": "Effect speed", "type": "range", "options": [0, 255], "content": ["id_qmk_led_matrix_effect_speed", 5, 3]}
            ]}]})
        }
        "qmk_audio" => json!({"label": "Audio", "content": [{"label": "Audio", "content": [
            {"label": "Enable audio", "type": "toggle", "content": ["id_qmk_audio_enable", 4, 1]},
            {"label": "Enable audio clicky", "type": "toggle", "content": ["id_qmk_audio_clicky_enable", 4, 2]}
        ]}]}),
        _ => return None,
    })
}

/// The built-in menu for a lighting channel the firmware reports having.
pub fn builtin_for_channel(channel: u8) -> Option<&'static str> {
    match channel {
        1 => Some("qmk_backlight"),
        2 => Some("qmk_rgblight"),
        3 => Some("qmk_rgb_matrix"),
        4 => Some("qmk_audio"),
        5 => Some("qmk_led_matrix"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing

fn parse_control(v: &Value) -> Option<Control> {
    let content = v["content"].as_array()?;
    let key = content.first()?.as_str()?.to_string();
    let channel = content.get(1)?.as_u64()? as u8;
    let id = content.get(2)?.as_u64()? as u8;
    let label = super::keycodes::british(v["label"].as_str().unwrap_or(""));
    let opts = &v["options"];
    let kind = match v["type"].as_str()? {
        "range" => {
            let a = opts.as_array();
            let min = a.and_then(|a| a.first()).and_then(Value::as_i64).unwrap_or(0);
            let max = a.and_then(|a| a.get(1)).and_then(Value::as_i64).unwrap_or(255);
            ControlKind::Range { min, max }
        }
        "dropdown" | "select" => {
            let items = opts.as_array()?;
            ControlKind::Dropdown(
                items
                    .iter()
                    .enumerate()
                    .filter_map(|(i, o)| match o {
                        Value::String(s) => Some((super::keycodes::british(s), i as i64)),
                        Value::Array(a) => Some((super::keycodes::british(a.first()?.as_str()?), a.get(1)?.as_i64()?)),
                        _ => None,
                    })
                    .collect(),
            )
        }
        "toggle" => {
            let a = opts.as_array();
            let off = a.and_then(|a| a.first()).and_then(Value::as_i64).unwrap_or(0);
            let on = a.and_then(|a| a.get(1)).and_then(Value::as_i64).unwrap_or(1);
            ControlKind::Toggle { off, on }
        }
        "color" => ControlKind::Color,
        "keycode" => ControlKind::Keycode,
        _ => return None,
    };
    Some(Control { label, kind, key, channel, id, show_if: v["showIf"].as_str().map(str::to_string) })
}

/// Collects controls from a section's content, descending into nested groups.
fn collect(content: &Value, label: &str, out: &mut Vec<Section>) {
    let Some(items) = content.as_array() else {
        return;
    };
    let mut here = Section { label: label.to_string(), controls: Vec::new() };
    for it in items {
        if it.get("type").is_some() {
            if let Some(c) = parse_control(it) {
                here.controls.push(c);
            }
        } else if it.get("content").is_some() {
            let l = super::keycodes::british(it["label"].as_str().unwrap_or(label));
            collect(&it["content"], &l, out);
        }
    }
    if !here.controls.is_empty() {
        out.push(here);
    }
}

/// Turns a definition's `menus` into menus. `None` entries for names this app doesn't know.
pub fn parse_menus(menus: &[Value]) -> Vec<Menu> {
    let mut out: Vec<Menu> = Vec::new();
    for m in menus {
        let v = match m {
            Value::String(s) => match builtin(s) {
                Some(v) => v,
                None => continue,
            },
            other => other.clone(),
        };
        let label = super::keycodes::british(v["label"].as_str().unwrap_or("Settings"));
        let mut sections = Vec::new();
        collect(&v["content"], &label, &mut sections);
        if sections.is_empty() {
            continue;
        }
        // Menus with the same name (two built-ins) are merged.
        if let Some(existing) = out.iter_mut().find(|e| e.label == label) {
            existing.sections.extend(sections);
        } else {
            out.push(Menu { label, sections });
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// showIf expressions: {id_name} values, numbers, == != < <= > >=, && || !, parentheses

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(i64),
    Op(&'static str),
    Open,
    Close,
}

fn tokens(expr: &str, values: &HashMap<String, i64>) -> Option<Vec<Tok>> {
    let b = expr.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c == '{' {
            let end = expr[i..].find('}')? + i;
            let name = expr[i + 1..end].trim();
            out.push(Tok::Num(values.get(name).copied().unwrap_or(0)));
            i = end + 1;
        } else if c.is_ascii_digit() || (c == '-' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            i += 1;
            while i < b.len() && (b[i] as char).is_ascii_alphanumeric() {
                i += 1;
            }
            let s = &expr[start..i];
            let n = if let Some(h) = s.strip_prefix("0x") { i64::from_str_radix(h, 16).ok()? } else { s.parse().ok()? };
            out.push(Tok::Num(n));
        } else if c == '(' {
            out.push(Tok::Open);
            i += 1;
        } else if c == ')' {
            out.push(Tok::Close);
            i += 1;
        } else {
            let two = expr.get(i..i + 2).unwrap_or("");
            let op = ["==", "!=", "<=", ">=", "&&", "||"].iter().find(|o| **o == two).copied();
            if let Some(op) = op {
                out.push(Tok::Op(op));
                i += 2;
            } else {
                let one = ["<", ">", "!"].iter().find(|o| o.starts_with(c))?;
                out.push(Tok::Op(*one));
                i += 1;
            }
        }
    }
    Some(out)
}

struct Parser {
    t: Vec<Tok>,
    p: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.p)
    }
    fn or(&mut self) -> Option<i64> {
        let mut v = self.and()?;
        while self.peek() == Some(&Tok::Op("||")) {
            self.p += 1;
            let r = self.and()?;
            v = ((v != 0) || (r != 0)) as i64;
        }
        Some(v)
    }
    fn and(&mut self) -> Option<i64> {
        let mut v = self.cmp()?;
        while self.peek() == Some(&Tok::Op("&&")) {
            self.p += 1;
            let r = self.cmp()?;
            v = ((v != 0) && (r != 0)) as i64;
        }
        Some(v)
    }
    fn cmp(&mut self) -> Option<i64> {
        let a = self.unary()?;
        if let Some(Tok::Op(op)) = self.peek().cloned() {
            if ["==", "!=", "<", "<=", ">", ">="].contains(&op) {
                self.p += 1;
                let b = self.unary()?;
                return Some(match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    _ => a >= b,
                } as i64);
            }
        }
        Some(a)
    }
    fn unary(&mut self) -> Option<i64> {
        match self.peek().cloned()? {
            Tok::Op("!") => {
                self.p += 1;
                Some((self.unary()? == 0) as i64)
            }
            Tok::Open => {
                self.p += 1;
                let v = self.or()?;
                if self.peek() != Some(&Tok::Close) {
                    return None;
                }
                self.p += 1;
                Some(v)
            }
            Tok::Num(n) => {
                self.p += 1;
                Some(n)
            }
            _ => None,
        }
    }
}

/// Evaluates a `showIf`; an expression this can't read shows the control.
pub fn show_if(expr: &str, values: &HashMap<String, i64>) -> bool {
    let Some(t) = tokens(expr, values) else {
        return true;
    };
    let mut p = Parser { t, p: 0 };
    match p.or() {
        Some(v) if p.p == p.t.len() => v != 0,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_rgb_matrix_menu() {
        let m = parse_menus(&[json!("qmk_rgb_matrix")]);
        assert_eq!(m.len(), 1);
        let c = &m[0].sections[0].controls;
        assert_eq!(c.len(), 4);
        assert_eq!((c[1].channel, c[1].id), (3, 2));
        assert!(matches!(&c[1].kind, ControlKind::Dropdown(o) if o.len() == RGB_MATRIX_EFFECTS.len()));
        assert_eq!(c[3].len(), 2);
    }

    #[test]
    fn conditions() {
        let mut v = HashMap::new();
        v.insert("id_a".to_string(), 3);
        v.insert("id_b".to_string(), 0);
        assert!(show_if("{id_a} != 0", &v));
        assert!(!show_if("{id_b} != 0", &v));
        assert!(show_if("{id_a} == 3 && !({id_b} > 1)", &v));
        assert!(show_if("{id_b} == 1 || {id_a} >= 3", &v));
        assert!(!show_if("{id_a} < 2 || {id_b} == 9", &v));
        assert!(show_if("garbage ((", &v));
    }

    #[test]
    fn custom_menu_from_sk620_definition() {
        let d = super::super::definition::Definition::parse(super::super::definition::BUNDLED[0]).unwrap();
        let m = parse_menus(&d.menus);
        assert_eq!(m[0].label, "Lighting");
        let effect = &m[0].sections[0].controls[1];
        let ControlKind::Dropdown(opts) = &effect.kind else { panic!() };
        assert_eq!(opts[1], ("Solid colour".to_string(), 1));
        assert!(opts.len() > 20);
    }
}
