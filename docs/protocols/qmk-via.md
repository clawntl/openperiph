# Protocol: QMK keyboards with VIA

Any keyboard running [QMK](https://qmk.fm) built with `VIA_ENABLE = yes`. Everything here comes
from QMK's own source (`quantum/via.c`, `quantum/via.h`, `quantum/dynamic_keymap.c`,
`quantum/send_string`), protocol version 12 (QMK 0.20 and later). Code: `src/qmk/`.

## Device

| What | Value |
|---|---|
| Collection | usage page 0xFF60, usage 0x61 (QMK's raw HID interface), any vendor and product id |
| Reports | 32-byte output report out, 32-byte input report back; no report ids |
| Transport | Windows: `WriteFile` / `ReadFile` (overlapped, 500 ms timeout). Linux: `write` / `poll` + `read` on `/dev/hidrawN` |
| Answers | every message is answered with 32 bytes starting with the same command byte, or `FF` if the firmware doesn't know it |

## Messages (byte 0 = command)

| Command | Sent | Answer |
|---|---|---|
| 01 protocol version | `01` | `01 hi lo` |
| 02 get keyboard value | `02 id` (01 uptime, 02 layout options, 03 switch matrix, 04 firmware version) | `02 id b3 b2 b1 b0` (big-endian) |
| 03 set keyboard value | `03 02 b3 b2 b1 b0` (layout options) | echo |
| 04 / 05 get / set keycode | `04 layer row col` / `05 layer row col hi lo` | `.. .. .. .. hi lo` |
| 06 reset keymap | `06` | echo; the keymap goes back to the firmware's |
| 07 / 08 / 09 set / get / save value | `07 channel id data…`, `08 channel id`, `09 channel` | `08 channel id data…`; `FF` if the channel isn't built in |
| 0B bootloader | `0B` | (not in protocol 12; answered `FF`) |
| 0C macro count | `0C` | `0C n` |
| 0D macro buffer size | `0D` | `0D hi lo` |
| 0E / 0F get / set macro buffer | `0E off_hi off_lo len` / `0F off_hi off_lo len data…` (len ≤ 28) | data at byte 4 |
| 10 reset macros | `10` | echo |
| 11 layer count | `11` | `11 n` |
| 12 / 13 get / set keymap buffer | `12 off_hi off_lo len` / `13 off_hi off_lo len data…` (len ≤ 28) | keycodes big-endian, layer by layer, row by row |

Lighting channels: 1 backlight, 2 underglow (rgblight), 3 RGB Matrix, 4 audio, 5 LED Matrix. On
each, value 1 is brightness, 2 effect (0 = off, then the effects compiled into the firmware, in
QMK's order), 3 speed, 4 colour (hue, saturation). Values are applied straight away; this app
sends `09 channel` once a value has stopped changing for 0.4 s.

## Keyboard definitions

QMK firmware doesn't describe its physical layout, so the keymap page needs the keyboard's VIA
definition (JSON, version 3): name, `vendorId`, `productId`, `matrix` size, `layouts.keymap` in
keyboard-layout-editor format with each key's legend `"row,col"` (and `"group,choice"` in the
bottom-right legend for layout options), `layouts.labels`, `menus`, `customKeycodes`.
Definitions are bundled (`definitions/`) or imported (kept in the settings folder). Layout options
are packed into one 32-bit value, the last option in the lowest bits, each taking enough bits for
its number of choices.

## Keycodes

16-bit, QMK's "latest" numbering (`src/qmk/keycodes_table.rs`, generated from QMK's keycode spec
with `tools/gen_qmk_keycodes.py`). Ranges with arguments: mods+key 0x0100 (`mods << 8 | key`),
mod-tap 0x2000, layer-tap 0x4000 (`layer << 8 | key`), layer-mod 0x5000, TO 0x5200, MO 0x5220,
DF 0x5240, TG 0x5260, OSL 0x5280, OSM 0x52A0, TT 0x52C0, PDF 0x52E0, tap dance 0x5700, macros
0x7700, keyboard-specific 0x7E00, user 0x7E40. Modifier bits: Ctrl 1, Shift 2, Alt 4, GUI 8,
right-hand 0x10.

## Macros

One buffer, macros one after another, each ending in `00`. Plain bytes are typed as ASCII. `01 01
kc` taps a key, `01 02 kc` presses it, `01 03 kc` releases it, `01 04 digits |` waits (ms). When
writing, the buffer's last byte is first set to `FF` and cleared last, so the keyboard never plays
a half-written buffer.

Firmware updates for the Cooler Master SK620: [cooler-master-sk620.md](cooler-master-sk620.md).
