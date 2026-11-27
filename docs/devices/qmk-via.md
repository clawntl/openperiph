# QMK and VIA keyboards

OpenPeriph works with any keyboard running [QMK](https://qmk.fm) firmware built with VIA
(`VIA_ENABLE = yes`). It finds the keyboard through QMK's raw HID interface, so there's nothing to
set up. Changes are saved on the keyboard as you make them, as in VIA: there's no Apply button,
and nothing has to keep running afterwards.

## What you can change

- **Keymap** on every layer, with any QMK keycode: letters and symbols, shortcuts (modifiers +
  key), media and mouse keys, layer keys (MO, TG, TO, OSL, TT, LT…), mod-tap, one-shot
  modifiers, macros, lighting keys, QMK keys (boot, debug…) and the keyboard's own keycodes.
  **Any** takes a keycode typed by name or number.
- **Lighting**: the keyboard's own VIA menus, or the standard ones for the lighting its firmware
  has (RGB Matrix, RGB Light, LED Matrix, backlight). Colours have a hue × saturation picker, a
  hex box and presets.
- **Macros**: text, `{KC_ENT}` to tap a key, `{KC_LCTL,KC_C}` for a shortcut, `{+KC_LSFT}` /
  `{-KC_LSFT}` to hold and release a key, `{200}` to wait 200 ms.
- **Layout options** (split backspace, ISO Enter…).
- **Backup and restore** to a file that VIA can read too, or reset the keymap or macros.

## Keyboard definitions

QMK keyboards don't report where their keys are, so the keymap page needs the keyboard's **VIA
definition** (a JSON file). Lighting and macros work without one.

- Bundled: the Cooler Master SK620.
- Others: download the keyboard's JSON from [the-via/keyboards](https://github.com/the-via/keyboards)
  (folder `v3`) or get it from the keyboard's maker. Then use **Import VIA definition** on the
  keyboard's page. OpenPeriph keeps a copy in its settings folder (`%APPDATA%\OpenPeriph\definitions` on
  Windows, `~/.config/OpenPeriph/definitions` on Linux).

Keyboards with an older VIA protocol than 12 (firmware from before QMK 0.20) aren't supported.

How it works: [protocols/qmk-via.md](../protocols/qmk-via.md).
