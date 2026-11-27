//! QMK dynamic macros as text, in the same notation VIA uses:
//!
//! - plain text is typed as it is (ASCII only, as QMK's send_string types it)
//! - `{KC_A}` taps a key; `{KC_LCTL,KC_C}` holds the first keys and taps the last (a shortcut)
//! - `{+KC_LSFT}` presses a key down, `{-KC_LSFT}` lets it go
//! - `{250}` waits 250 milliseconds
//! - `\{` and `\\` type a literal brace or backslash
//!
//! On the keyboard, macros sit one after another in a byte buffer, each ending with 0. Key
//! actions are the byte 1 followed by 1 (tap), 2 (down) or 3 (up) and a keycode byte; a delay is
//! 1, 4, the milliseconds in ASCII digits, and `|`.

use super::keycodes::{self, CustomKeycode};

const PREFIX: u8 = 0x01;
const TAP: u8 = 0x01;
const DOWN: u8 = 0x02;
const UP: u8 = 0x03;
const DELAY: u8 = 0x04;

/// Splits the keyboard's buffer into `count` macros, as text.
pub fn decode(buf: &[u8], count: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    for _ in 0..count {
        let mut s = String::new();
        while i < buf.len() && buf[i] != 0 {
            let b = buf[i];
            if b == PREFIX && i + 1 < buf.len() {
                let kind = buf[i + 1];
                match kind {
                    TAP | DOWN | UP if i + 2 < buf.len() => {
                        let name = keycodes::name(buf[i + 2] as u16);
                        let sign = match kind {
                            DOWN => "+",
                            UP => "-",
                            _ => "",
                        };
                        s.push_str(&format!("{{{sign}{name}}}"));
                        i += 3;
                    }
                    DELAY => {
                        let mut j = i + 2;
                        let mut ms = String::new();
                        while j < buf.len() && buf[j].is_ascii_digit() {
                            ms.push(buf[j] as char);
                            j += 1;
                        }
                        if j < buf.len() && buf[j] == b'|' {
                            j += 1;
                        }
                        s.push_str(&format!("{{{ms}}}"));
                        i = j;
                    }
                    _ => i += 2,
                }
                continue;
            }
            match b {
                b'{' => s.push_str("\\{"),
                b'\\' => s.push_str("\\\\"),
                0x20..=0x7E | b'\n' | b'\t' => s.push(b as char),
                _ => {}
            }
            i += 1;
        }
        i += 1; // the 0 that ends this macro
        out.push(s);
    }
    out
}

/// Turns macro texts back into the keyboard's buffer.
pub fn encode(macros: &[String], custom: &[CustomKeycode]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (n, m) in macros.iter().enumerate() {
        encode_one(m, custom, &mut out).map_err(|e| format!("Macro {n}: {e}"))?;
        out.push(0);
    }
    Ok(out)
}

fn key(name: &str, custom: &[CustomKeycode]) -> Result<u8, String> {
    let code = keycodes::parse(name, custom).ok_or_else(|| format!("\"{name}\" isn't a keycode"))?;
    if code > 0xFF {
        return Err(format!("{name} can't be used in a macro (only basic keys and modifiers can)"));
    }
    Ok(code as u8)
}

fn encode_one(text: &str, custom: &[CustomKeycode], out: &mut Vec<u8>) -> Result<(), String> {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(e @ ('{' | '\\')) => out.push(e as u8),
                Some(other) => {
                    out.push(b'\\');
                    push_char(other, out)?;
                }
                None => out.push(b'\\'),
            },
            '{' => {
                let mut inner = String::new();
                let mut closed = false;
                for d in chars.by_ref() {
                    if d == '}' {
                        closed = true;
                        break;
                    }
                    inner.push(d);
                }
                if !closed {
                    return Err("a { has no matching }".into());
                }
                let inner = inner.trim();
                if let Ok(ms) = inner.parse::<u32>() {
                    out.extend_from_slice(&[PREFIX, DELAY]);
                    out.extend_from_slice(ms.to_string().as_bytes());
                    out.push(b'|');
                } else if let Some(k) = inner.strip_prefix('+') {
                    out.extend_from_slice(&[PREFIX, DOWN, key(k.trim(), custom)?]);
                } else if let Some(k) = inner.strip_prefix('-') {
                    out.extend_from_slice(&[PREFIX, UP, key(k.trim(), custom)?]);
                } else {
                    let keys: Vec<u8> = inner.split(',').map(|k| key(k.trim(), custom)).collect::<Result<_, _>>()?;
                    if keys.len() == 1 {
                        out.extend_from_slice(&[PREFIX, TAP, keys[0]]);
                    } else {
                        for k in &keys {
                            out.extend_from_slice(&[PREFIX, DOWN, *k]);
                        }
                        for k in keys.iter().rev() {
                            out.extend_from_slice(&[PREFIX, UP, *k]);
                        }
                    }
                }
            }
            other => push_char(other, out)?,
        }
    }
    Ok(())
}

fn push_char(c: char, out: &mut Vec<u8>) -> Result<(), String> {
    if c == '\n' || c == '\t' || (' '..='~').contains(&c) {
        out.push(c as u8);
        Ok(())
    } else {
        Err(format!("\"{c}\" can't be typed by a macro; use {{KC_…}} keys for anything outside plain ASCII"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let none: [CustomKeycode; 0] = [];
        let texts =
            vec!["Hello, world!".to_string(), "{KC_LCTL,KC_C}{100}{+KC_LSFT}abc{-KC_LSFT}".to_string(), String::new(), "braces \\{ and \\\\ backslash".to_string()];
        let buf = encode(&texts, &none).unwrap();
        assert_eq!(&buf[..13], b"Hello, world!");
        assert_eq!(
            decode(&buf, 4),
            vec![
                "Hello, world!".to_string(),
                "{KC_LCTL}{KC_C}".replace("{KC_LCTL}{KC_C}", "{+KC_LCTL}{+KC_C}{-KC_C}{-KC_LCTL}{100}{+KC_LSFT}abc{-KC_LSFT}"),
                String::new(),
                "braces \\{ and \\\\ backslash".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_what_qmk_cant_type() {
        let none: [CustomKeycode; 0] = [];
        assert!(encode(&["café".to_string()], &none).is_err());
        assert!(encode(&["{MO(1)}".to_string()], &none).is_err());
        assert!(encode(&["{KC_A".to_string()], &none).is_err());
    }
}
