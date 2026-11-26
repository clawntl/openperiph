//! A simulated Cooler Master SK620 running QMK with VIA, for tests, screenshots and trying the
//! app without the keyboard (`openperiph --demo`).

use super::keycodes;
use super::via::FakeKeyboard;

/// The SK620's default keymap (base and Fn layers) in matrix order, from its QMK keymap.
#[rustfmt::skip]
pub const SK620_LAYERS: [[[&str; 8]; 9]; 2] = [
    [
        ["KC_ESC", "KC_1", "KC_2", "KC_3", "KC_4", "KC_5", "KC_6", "KC_7"],
        ["KC_TAB", "KC_Q", "KC_W", "KC_E", "KC_R", "KC_T", "KC_Y", "KC_U"],
        ["KC_CAPS", "KC_A", "KC_S", "KC_D", "KC_F", "KC_G", "KC_H", "KC_J"],
        ["KC_LSFT", "KC_Z", "KC_X", "KC_C", "KC_V", "KC_B", "KC_N", "KC_M"],
        ["KC_LCTL", "KC_LGUI", "KC_LALT", "KC_8", "KC_9", "KC_0", "KC_MINS", "KC_BSPC"],
        ["KC_DOWN", "KC_NO", "KC_EQL", "KC_I", "KC_O", "KC_P", "KC_LBRC", "KC_NO"],
        ["KC_NUHS", "KC_RGHT", "KC_RBRC", "KC_K", "KC_L", "KC_SCLN", "KC_QUOT", "KC_ENT"],
        ["KC_NUBS", "KC_UP", "KC_SPC", "KC_COMM", "KC_DOT", "KC_SLSH", "KC_RSFT", "KC_LEFT"],
        ["KC_NO", "KC_NO", "KC_NO", "KC_RALT", "MO(1)", "KC_NO", "KC_DEL", "KC_NO"],
    ],
    [
        ["KC_GRV", "KC_F1", "KC_F2", "KC_F3", "KC_F4", "KC_F5", "KC_F6", "KC_F7"],
        ["RM_TOGG", "RM_HUEU", "RM_SATU", "RM_SPDU", "KC_TRNS", "KC_TRNS", "KC_TRNS", "KC_PSCR"],
        ["KC_TRNS", "RM_HUED", "RM_SATD", "RM_SPDD", "KC_TRNS", "KC_TRNS", "KC_TRNS", "KC_MPLY"],
        ["KC_TRNS", "KC_TRNS", "KC_TRNS", "KC_TRNS", "KC_TRNS", "QK_BOOT", "KC_TRNS", "KC_TRNS"],
        ["KC_TRNS", "KC_TRNS", "KC_TRNS", "KC_F8", "KC_F9", "KC_F10", "KC_F11", "KC_EJCT"],
        ["RM_VALD", "KC_NO", "KC_F12", "KC_SCRL", "KC_PAUS", "KC_INS", "KC_HOME", "KC_NO"],
        ["KC_TRNS", "RM_NEXT", "KC_PGUP", "KC_MPRV", "KC_MNXT", "KC_END", "KC_PGDN", "KC_TRNS"],
        ["KC_TRNS", "RM_VALU", "KC_TRNS", "KC_VOLD", "KC_VOLU", "KC_MUTE", "GU_TOGG", "RM_PREV"],
        ["KC_NO", "KC_NO", "KC_NO", "KC_TRNS", "KC_TRNS", "KC_NO", "KC_TRNS", "KC_NO"],
    ],
];

pub fn fake_sk620() -> FakeKeyboard {
    let base: Vec<u16> = SK620_LAYERS[0].iter().flatten().map(|n| keycodes::parse(n, &[]).unwrap_or(0)).collect();
    let mut kb = FakeKeyboard::new(9, 8, 4, &base);
    for (i, n) in SK620_LAYERS[1].iter().flatten().enumerate() {
        kb.keymap[72 + i] = keycodes::parse(n, &[]).unwrap_or(1);
    }
    kb.default_keymap = kb.keymap.clone();
    let m = b"Hello from QMK!\0\x01\x02\xe0\x01\x01\x06\x01\x03\xe0\0".to_vec();
    kb.macros[..m.len()].copy_from_slice(&m);
    kb
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_name_in_the_demo_keymap_is_known() {
        for layer in super::SK620_LAYERS {
            for n in layer.iter().flatten() {
                assert!(super::keycodes::parse(n, &[]).is_some(), "{n}");
            }
        }
    }
}
