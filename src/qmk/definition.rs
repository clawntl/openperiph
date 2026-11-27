//! VIA keyboard definitions: the JSON file that tells VIA (and this app) a QMK keyboard's name,
//! USB ids, matrix size, physical layout and settings menus. The layout is in the format of
//! keyboard-layout-editor.com ("KLE"), with each key's legend set to its matrix position.
//!
//! Definitions for supported keyboards are bundled; others can be imported (VIA's own definitions
//! are in https://github.com/the-via/keyboards, folder v3).

use serde_json::Value;

use super::keycodes::CustomKeycode;

/// One key of the physical layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub row: u8,
    pub col: u8,
    /// Position and size in key units. Rotated keys have their centre rotated; the cap itself is
    /// drawn upright.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Second rectangle of odd-shaped keys (ISO Enter, big-ass Enter), relative to `x, y`.
    pub second: Option<[f32; 4]>,
    /// Layout option this key belongs to: (option index, choice).
    pub option: Option<(usize, usize)>,
    pub encoder: bool,
}

/// A layout option ("Split backspace", or a choice like "Bottom row: ANSI / Tsangan").
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutOption {
    pub label: String,
    /// Empty for an on/off option.
    pub choices: Vec<String>,
}

impl LayoutOption {
    pub fn count(&self) -> usize {
        if self.choices.is_empty() {
            2
        } else {
            self.choices.len()
        }
    }
}

#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub rows: usize,
    pub cols: usize,
    pub keys: Vec<Key>,
    pub options: Vec<LayoutOption>,
    /// The `menus` entry as written (names of built-in menus or custom menu objects).
    pub menus: Vec<Value>,
    pub custom_keycodes: Vec<CustomKeycode>,
}

fn hex_id(v: &Value) -> Option<u16> {
    match v {
        Value::String(s) => {
            let s = s.trim();
            let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
            u16::from_str_radix(h, 16).ok()
        }
        Value::Number(n) => n.as_u64().map(|n| n as u16),
        _ => None,
    }
}

fn f(v: &Value) -> Option<f32> {
    v.as_f64().map(|x| x as f32)
}

impl Definition {
    pub fn parse(text: &str) -> Result<Definition, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("Not a valid JSON file: {e}"))?;
        let name = v["name"].as_str().unwrap_or("QMK keyboard").to_string();
        let vendor_id = hex_id(&v["vendorId"]).ok_or("The definition has no vendorId.")?;
        let product_id = hex_id(&v["productId"]).ok_or("The definition has no productId.")?;
        let rows = v["matrix"]["rows"].as_u64().ok_or("The definition has no matrix size.")? as usize;
        let cols = v["matrix"]["cols"].as_u64().ok_or("The definition has no matrix size.")? as usize;
        let layouts = &v["layouts"];
        let kle = layouts["keymap"].as_array().ok_or("The definition has no layouts.keymap.")?;
        let keys = parse_kle(kle)?;
        if keys.is_empty() {
            return Err("The definition's layout has no keys.".into());
        }
        if let Some(k) = keys.iter().find(|k| k.row as usize >= rows || k.col as usize >= cols) {
            return Err(format!("Key {},{} is outside the {rows}x{cols} matrix.", k.row, k.col));
        }
        let mut options = Vec::new();
        if let Some(labels) = layouts["labels"].as_array() {
            for l in labels {
                match l {
                    Value::String(s) => options.push(LayoutOption { label: s.clone(), choices: Vec::new() }),
                    Value::Array(a) if !a.is_empty() => options.push(LayoutOption {
                        label: a[0].as_str().unwrap_or("").to_string(),
                        choices: a[1..].iter().map(|c| c.as_str().unwrap_or("").to_string()).collect(),
                    }),
                    _ => {}
                }
            }
        }
        let menus = v["menus"].as_array().cloned().unwrap_or_default();
        let custom_keycodes = v["customKeycodes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|c| CustomKeycode {
                        name: c["name"].as_str().unwrap_or("").to_string(),
                        title: c["title"].as_str().unwrap_or("").to_string(),
                        short: c["shortName"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Definition { name, vendor_id, product_id, rows, cols, keys, options, menus, custom_keycodes })
    }

    /// Whether a key is shown with the chosen layout options.
    pub fn shown(&self, k: &Key, choices: &[usize]) -> bool {
        match k.option {
            None => true,
            Some((o, c)) => choices.get(o).copied().unwrap_or(0) == c,
        }
    }

    /// The keys shown with these layout options, moved so each option group's chosen keys sit
    /// where the group's first choice is drawn (as VIA does).
    pub fn visible_keys(&self, choices: &[usize]) -> Vec<Key> {
        let mut out = Vec::new();
        for k in &self.keys {
            if !self.shown(k, choices) {
                continue;
            }
            let mut k = k.clone();
            if let Some((o, c)) = k.option {
                if c != 0 {
                    // shift by the offset between this choice's keys and choice 0's keys
                    let min = |ch: usize| self.keys.iter().filter(|q| q.option == Some((o, ch))).fold((f32::MAX, f32::MAX), |(x, y), q| (x.min(q.x), y.min(q.y)));
                    let (x0, y0) = min(0);
                    let (xc, yc) = min(c);
                    if x0 != f32::MAX && xc != f32::MAX {
                        k.x += x0 - xc;
                        k.y += y0 - yc;
                    }
                }
            }
            out.push(k);
        }
        out
    }

    /// Layout options packed into VIA's 32-bit value: the last option takes the lowest bits.
    pub fn pack_options(&self, choices: &[usize]) -> u32 {
        let mut v = 0u32;
        let mut shift = 0u32;
        for (i, o) in self.options.iter().enumerate().rev() {
            let bits = bits_for(o.count());
            v |= (choices.get(i).copied().unwrap_or(0) as u32 & ((1 << bits) - 1)) << shift;
            shift += bits;
        }
        v
    }

    pub fn unpack_options(&self, v: u32) -> Vec<usize> {
        let mut out = vec![0usize; self.options.len()];
        let mut shift = 0u32;
        for (i, o) in self.options.iter().enumerate().rev() {
            let bits = bits_for(o.count());
            out[i] = ((v >> shift) & ((1 << bits) - 1)) as usize;
            shift += bits;
        }
        out
    }
}

fn bits_for(n: usize) -> u32 {
    let mut b = 1;
    while (1usize << b) < n {
        b += 1;
    }
    b
}

/// Reads KLE rows. Properties carry over between keys the way KLE defines them: x/y offsets and
/// sizes apply to the next key only, rotation applies until changed.
fn parse_kle(rows: &[Value]) -> Result<Vec<Key>, String> {
    let mut keys = Vec::new();
    let (mut y, mut r, mut rx, mut ry) = (0f32, 0f32, 0f32, 0f32);
    for row in rows {
        let Some(items) = row.as_array() else {
            // a metadata object before the rows
            continue;
        };
        let mut x = rx;
        let (mut w, mut h, mut x2, mut y2, mut w2, mut h2) = (1f32, 1f32, 0f32, 0f32, 0f32, 0f32);
        let mut decal = false;
        let mut has_second = false;
        for it in items {
            match it {
                Value::Object(p) => {
                    if let Some(v) = p.get("r").and_then(f) {
                        r = v;
                    }
                    if let Some(v) = p.get("rx").and_then(f) {
                        rx = v;
                        x = rx;
                        y = ry;
                    }
                    if let Some(v) = p.get("ry").and_then(f) {
                        ry = v;
                        x = rx;
                        y = ry;
                    }
                    if let Some(v) = p.get("x").and_then(f) {
                        x += v;
                    }
                    if let Some(v) = p.get("y").and_then(f) {
                        y += v;
                    }
                    if let Some(v) = p.get("w").and_then(f) {
                        w = v;
                        if !has_second {
                            w2 = v;
                        }
                    }
                    if let Some(v) = p.get("h").and_then(f) {
                        h = v;
                        if !has_second {
                            h2 = v;
                        }
                    }
                    for (k, slot) in [("x2", &mut x2), ("y2", &mut y2), ("w2", &mut w2), ("h2", &mut h2)] {
                        if let Some(v) = p.get(k).and_then(f) {
                            *slot = v;
                            has_second = true;
                        }
                    }
                    if p.get("d").and_then(Value::as_bool) == Some(true) {
                        decal = true;
                    }
                }
                Value::String(label) => {
                    if !decal {
                        let parts: Vec<&str> = label.split('\n').collect();
                        let (row, col) = parse_pair(parts[0]).ok_or_else(|| format!("Key label \"{}\" isn't a matrix position like \"0,1\".", parts[0]))?;
                        // the layout option sits in the bottom-right legend (index 3), or index 8
                        let option = parts.get(3).and_then(|s| parse_pair(s)).or_else(|| parts.get(8).and_then(|s| parse_pair(s)));
                        let encoder = parts.get(9).map(|s| s.trim() == "e").unwrap_or(false) || parts.get(10).map(|s| s.trim() == "e").unwrap_or(false);
                        // rotate the key's centre about (rx, ry)
                        let (mut kx, mut ky) = (x, y);
                        if r != 0.0 {
                            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                            let (s, c) = r.to_radians().sin_cos();
                            let (dx, dy) = (cx - rx, cy - ry);
                            kx = rx + dx * c - dy * s - w / 2.0;
                            ky = ry + dx * s + dy * c - h / 2.0;
                        }
                        keys.push(Key {
                            row: row as u8,
                            col: col as u8,
                            x: kx,
                            y: ky,
                            w,
                            h,
                            second: has_second.then_some([x2, y2, w2, h2]),
                            option: option.map(|(a, b)| (a, b)),
                            encoder,
                        });
                    }
                    x += w;
                    w = 1.0;
                    h = 1.0;
                    x2 = 0.0;
                    y2 = 0.0;
                    w2 = 0.0;
                    h2 = 0.0;
                    has_second = false;
                    decal = false;
                }
                _ => {}
            }
        }
        y += 1.0;
    }
    // Move everything so the layout starts at 0,0.
    let minx = keys.iter().map(|k| k.x + k.second.map_or(0.0, |s| s[0].min(0.0))).fold(f32::MAX, f32::min);
    let miny = keys.iter().map(|k| k.y + k.second.map_or(0.0, |s| s[1].min(0.0))).fold(f32::MAX, f32::min);
    for k in &mut keys {
        k.x -= minx;
        k.y -= miny;
    }
    Ok(keys)
}

fn parse_pair(s: &str) -> Option<(usize, usize)> {
    let (a, b) = s.trim().split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Definitions shipped with the app.
pub const BUNDLED: &[&str] = &[include_str!("../../definitions/cm_sk620_iso.json"), include_str!("../../definitions/ciy_x78_smk_iso.json")];

/// Bundled plus imported definitions.
pub fn all_definitions() -> Vec<Definition> {
    let mut out: Vec<Definition> = BUNDLED.iter().filter_map(|t| Definition::parse(t).ok()).collect();
    for d in crate::store::imported_definitions() {
        if let Ok(def) = Definition::parse(&d) {
            // an imported file replaces a bundled one for the same keyboard
            out.retain(|o| (o.vendor_id, o.product_id) != (def.vendor_id, def.product_id));
            out.push(def);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_definitions_parse() {
        for t in BUNDLED {
            let d = Definition::parse(t).unwrap();
            assert!(!d.keys.is_empty());
        }
        let d = Definition::parse(BUNDLED[0]).unwrap();
        assert_eq!((d.vendor_id, d.product_id, d.rows, d.cols, d.keys.len()), (0xFEED, 0x0620, 9, 8, 65));
        let enter = d.keys.iter().find(|k| (k.row, k.col) == (6, 7)).unwrap();
        assert_eq!((enter.x, enter.y, enter.w, enter.h), (13.75, 1.0, 1.25, 2.0));
        assert_eq!(enter.second, Some([-0.25, 0.0, 1.5, 1.0]));

        // the CIY X78 on SMK: UK ISO TKL, Enter at matrix (3, 13)
        let d = Definition::parse(BUNDLED[1]).unwrap();
        assert_eq!((d.vendor_id, d.product_id, d.rows, d.cols, d.keys.len()), (0x258A, 0x0016, 6, 18, 88));
        let enter = d.keys.iter().find(|k| (k.row, k.col) == (3, 13)).unwrap();
        assert_eq!((enter.x, enter.w, enter.h), (13.75, 1.25, 2.0));
        assert_eq!(enter.second, Some([-0.25, 0.0, 1.5, 1.0]));
        // its own keycodes follow the firmware's enum from QK_KB_0
        assert_eq!(d.custom_keycodes[0].name, "FX_NEXT");
        assert_eq!(d.custom_keycodes[13].name, "FX_SET_OFF");
        // every row ends at the same right edge
        let right = d.keys.iter().map(|k| k.x + k.w).fold(0.0f32, f32::max);
        assert_eq!(right, 18.25);
    }

    #[test]
    fn layout_options_pack_like_via() {
        let text = r#"{"name":"t","vendorId":"0x1","productId":"0x2","matrix":{"rows":1,"cols":4},
            "layouts":{"labels":["Split backspace",["Bottom row","ANSI","Tsangan","WKL"]],
            "keymap":[["0,0","0,1\n\n\n0,0",{"x":-1},"0,2\n\n\n0,1","0,3\n\n\n1,2"]]}}"#;
        let d = Definition::parse(text).unwrap();
        assert_eq!(d.options.len(), 2);
        assert_eq!(d.options[1].count(), 3);
        // option 1 (3 choices, 2 bits) in the low bits, option 0 above it
        assert_eq!(d.pack_options(&[1, 2]), 0b1_10);
        assert_eq!(d.unpack_options(0b1_10), vec![1, 2]);
        let v = d.visible_keys(&[1, 0]);
        assert!(v.iter().any(|k| (k.row, k.col) == (0, 2)));
        assert!(!v.iter().any(|k| (k.row, k.col) == (0, 1)));
        // the chosen key is drawn where choice 0's key would be
        let k = v.iter().find(|k| (k.row, k.col) == (0, 2)).unwrap();
        assert_eq!(k.x, 1.0);
    }
}
