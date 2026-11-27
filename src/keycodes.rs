//! Device-independent key functions: what a key can be set to do.
//! Keyboard keys use standard USB HID usage numbers, media keys use USB consumer usages, so any
//! keyboard driver can translate them into its own format.

use serde::{Deserialize, Serialize};

/// Modifier bits, in USB HID order.
pub const MOD_CTRL: u8 = 0x01;
pub const MOD_SHIFT: u8 = 0x02;
pub const MOD_ALT: u8 = 0x04;
pub const MOD_WIN: u8 = 0x08;

/// What a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyAction {
    /// Whatever the key does out of the box.
    #[default]
    Default,
    /// A keyboard key (USB usage), optionally held together with Ctrl/Shift/Alt/Win.
    Key { mods: u8, usage: u8 },
    /// A mouse button or wheel movement.
    Mouse(MouseAction),
    /// A media or application key (USB consumer usage).
    Media(u16),
    /// The key does nothing.
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseAction {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    WheelUp,
    WheelDown,
}

impl MouseAction {
    pub const ALL: [MouseAction; 7] =
        [MouseAction::Left, MouseAction::Right, MouseAction::Middle, MouseAction::Back, MouseAction::Forward, MouseAction::WheelUp, MouseAction::WheelDown];
    pub fn name(self) -> &'static str {
        match self {
            MouseAction::Left => "Left click",
            MouseAction::Right => "Right click",
            MouseAction::Middle => "Middle click",
            MouseAction::Back => "Back",
            MouseAction::Forward => "Forward",
            MouseAction::WheelUp => "Scroll up",
            MouseAction::WheelDown => "Scroll down",
        }
    }
}

/// Media and application keys (USB consumer usages): every code the original's key converter
/// (function 0x40cbe0, table at 0x40d294) can produce.
pub const MEDIA: &[(u16, &str)] = &[
    (0x00CD, "Play / Pause"),
    (0x00B7, "Stop"),
    (0x00B6, "Previous track"),
    (0x00B5, "Next track"),
    (0x00E9, "Volume up"),
    (0x00EA, "Volume down"),
    (0x00E2, "Mute"),
    (0x0183, "Media player"),
    (0x018A, "Email"),
    (0x0192, "Calculator"),
    (0x0194, "My Computer"),
    (0x0221, "Search"),
    (0x0223, "Browser home"),
    (0x0224, "Browser back"),
    (0x0225, "Browser forward"),
    (0x0226, "Browser stop"),
    (0x0227, "Refresh"),
    (0x022A, "Favourites"),
];

/// Keyboard keys a key can be turned into, grouped for the picker.
pub const KEY_GROUPS: &[(&str, &[(u8, &str)])] = &[
    (
        "Letters",
        &[
            (0x04, "A"),
            (0x05, "B"),
            (0x06, "C"),
            (0x07, "D"),
            (0x08, "E"),
            (0x09, "F"),
            (0x0A, "G"),
            (0x0B, "H"),
            (0x0C, "I"),
            (0x0D, "J"),
            (0x0E, "K"),
            (0x0F, "L"),
            (0x10, "M"),
            (0x11, "N"),
            (0x12, "O"),
            (0x13, "P"),
            (0x14, "Q"),
            (0x15, "R"),
            (0x16, "S"),
            (0x17, "T"),
            (0x18, "U"),
            (0x19, "V"),
            (0x1A, "W"),
            (0x1B, "X"),
            (0x1C, "Y"),
            (0x1D, "Z"),
        ],
    ),
    (
        "Numbers & symbols",
        &[
            (0x1E, "1"),
            (0x1F, "2 \""),
            (0x20, "3 £"),
            (0x21, "4"),
            (0x22, "5"),
            (0x23, "6"),
            (0x24, "7"),
            (0x25, "8"),
            (0x26, "9"),
            (0x27, "0"),
            (0x2D, "-"),
            (0x2E, "="),
            (0x2F, "["),
            (0x30, "]"),
            (0x31, "# ~"),
            (0x33, ";"),
            (0x34, "' @"),
            (0x35, "` ¬"),
            (0x36, ","),
            (0x37, "."),
            (0x38, "/"),
            (0x64, "\\ |"),
        ],
    ),
    (
        "Function keys",
        &[
            (0x3A, "F1"),
            (0x3B, "F2"),
            (0x3C, "F3"),
            (0x3D, "F4"),
            (0x3E, "F5"),
            (0x3F, "F6"),
            (0x40, "F7"),
            (0x41, "F8"),
            (0x42, "F9"),
            (0x43, "F10"),
            (0x44, "F11"),
            (0x45, "F12"),
            (0x68, "F13"),
            (0x69, "F14"),
            (0x6A, "F15"),
            (0x6B, "F16"),
            (0x6C, "F17"),
            (0x6D, "F18"),
            (0x6E, "F19"),
            (0x6F, "F20"),
            (0x70, "F21"),
            (0x71, "F22"),
            (0x72, "F23"),
            (0x73, "F24"),
        ],
    ),
    (
        "Editing & navigation",
        &[
            (0x28, "Enter"),
            (0x29, "Esc"),
            (0x2A, "Backspace"),
            (0x2B, "Tab"),
            (0x2C, "Space"),
            (0x39, "Caps Lock"),
            (0x46, "Print Screen"),
            (0x47, "Scroll Lock"),
            (0x48, "Pause"),
            (0x49, "Insert"),
            (0x4A, "Home"),
            (0x4B, "Page Up"),
            (0x4C, "Delete"),
            (0x4D, "End"),
            (0x4E, "Page Down"),
            (0x4F, "Right"),
            (0x50, "Left"),
            (0x51, "Down"),
            (0x52, "Up"),
            (0x65, "Menu"),
        ],
    ),
    (
        "Modifiers",
        &[
            (0xE0, "Left Ctrl"),
            (0xE1, "Left Shift"),
            (0xE2, "Left Alt"),
            (0xE3, "Left Win"),
            (0xE4, "Right Ctrl"),
            (0xE5, "Right Shift"),
            (0xE6, "Right Alt"),
            (0xE7, "Right Win"),
        ],
    ),
    (
        "Number pad",
        &[
            (0x53, "Num Lock"),
            (0x54, "Pad /"),
            (0x55, "Pad *"),
            (0x56, "Pad -"),
            (0x57, "Pad +"),
            (0x58, "Pad Enter"),
            (0x59, "Pad 1"),
            (0x5A, "Pad 2"),
            (0x5B, "Pad 3"),
            (0x5C, "Pad 4"),
            (0x5D, "Pad 5"),
            (0x5E, "Pad 6"),
            (0x5F, "Pad 7"),
            (0x60, "Pad 8"),
            (0x61, "Pad 9"),
            (0x62, "Pad 0"),
            (0x63, "Pad ."),
        ],
    ),
];

pub fn usage_name(usage: u8) -> String {
    for (_, keys) in KEY_GROUPS {
        if let Some((_, n)) = keys.iter().find(|(u, _)| *u == usage) {
            return (*n).to_string();
        }
    }
    format!("Key 0x{usage:02X}")
}

pub fn media_name(usage: u16) -> String {
    MEDIA.iter().find(|(u, _)| *u == usage).map(|(_, n)| (*n).to_string()).unwrap_or_else(|| format!("Media 0x{usage:03X}"))
}

pub fn mods_text(mods: u8) -> String {
    let mut parts = Vec::new();
    if mods & (MOD_CTRL | MOD_CTRL << 4) != 0 {
        parts.push("Ctrl");
    }
    if mods & (MOD_SHIFT | MOD_SHIFT << 4) != 0 {
        parts.push("Shift");
    }
    if mods & (MOD_ALT | MOD_ALT << 4) != 0 {
        parts.push("Alt");
    }
    if mods & (MOD_WIN | MOD_WIN << 4) != 0 {
        parts.push("Win");
    }
    parts.join("+")
}

impl KeyAction {
    /// Plain description for the person.
    pub fn describe(&self) -> String {
        match *self {
            KeyAction::Default => "Default".into(),
            KeyAction::Key { mods: 0, usage } => usage_name(usage),
            KeyAction::Key { mods, usage } => format!("{}+{}", mods_text(mods), usage_name(usage)),
            KeyAction::Mouse(m) => m.name().into(),
            KeyAction::Media(u) => media_name(u),
            KeyAction::Disabled => "Disabled".into(),
        }
    }

    /// Very short text to print on a key cap.
    pub fn short(&self) -> String {
        let s = self.describe();
        if s.chars().count() > 9 {
            let cut: String = s.chars().take(8).collect();
            format!("{cut}…")
        } else {
            s
        }
    }
}

/// Windows virtual-key code to USB usage, for the keys this app knows about
/// (pairs checked against the original program's table at 0x59df18).
pub fn vk_to_usage(vk: u8) -> Option<u8> {
    Some(match vk {
        0x41..=0x5A => vk - 0x41 + 0x04,
        0x31..=0x39 => vk - 0x31 + 0x1E,
        0x30 => 0x27,
        0x70..=0x7B => vk - 0x70 + 0x3A,
        0x0D => 0x28,
        0x1B => 0x29,
        0x08 => 0x2A,
        0x09 => 0x2B,
        0x20 => 0x2C,
        0xBD => 0x2D,
        0xBB => 0x2E,
        0xDB => 0x2F,
        0xDD => 0x30,
        0xDC => 0x31,
        0xBA => 0x33,
        0xDE => 0x34,
        0xC0 => 0x35,
        0xBC => 0x36,
        0xBE => 0x37,
        0xBF => 0x38,
        0x14 => 0x39,
        0x2C => 0x46,
        0x91 => 0x47,
        0x13 => 0x48,
        0x2D => 0x49,
        0x24 => 0x4A,
        0x21 => 0x4B,
        0x2E => 0x4C,
        0x23 => 0x4D,
        0x22 => 0x4E,
        0x27 => 0x4F,
        0x25 => 0x50,
        0x28 => 0x51,
        0x26 => 0x52,
        0xE2 => 0x64,
        0x5D => 0x65,
        0xA2 => 0xE0,
        0xA0 => 0xE1,
        0xA4 => 0xE2,
        0x5B => 0xE3,
        0xA3 => 0xE4,
        0xA1 => 0xE5,
        0xA5 => 0xE6,
        0x5C => 0xE7,
        _ => return None,
    })
}
