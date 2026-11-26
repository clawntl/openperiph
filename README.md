# OpenPeriph

A small, modern settings app for PC peripherals, written in Rust with
[eframe/egui](https://github.com/emilk/egui), for **Windows and Linux**. It talks to devices
directly: through Windows' own HID functions, or Linux's hidraw devices.

## Supported devices

| Device | What you can change |
|---|---|
| **Any QMK keyboard with VIA** (found automatically by QMK's raw HID interface) | Keymap on every layer (any QMK keycode: layers, mod-tap, layer-tap, shortcuts, media, mouse, lighting, macros, QMK keys, the keyboard's own keycodes), macros, lighting (the keyboard's VIA menus, or the standard ones for the lighting its firmware has), layout options, backup and restore (VIA-compatible files), reset keymap or macros. Needs the keyboard's VIA definition for the keymap: bundled ones are below, others can be imported |
| **Cooler Master SK620** with [QMK](https://github.com/clawntl/qmk-sk620) (USB FEED:0620) | All of the above, definition bundled; also restarts it into the bootloader for flashing |
| **CIY X78** keyboard (UK ISO, USB 258A:0016) | Remap any key on the normal and Fn layers (other keys, shortcuts, mouse buttons, media keys, off); lighting effect, colour, speed, brightness, direction, game-mode groups |
| **Attack Shark R5 ULTRA** mouse (USB 373E:0046 cable / 373E:0047 receiver) | DPI stages (up to 6, 100–42 000), stage in use, onboard profile, polling rate (to 8 kHz on the receiver), lift-off distance, motion sync, ripple control, angle snapping, click debounce, sleep timer; shows battery and firmware |

How each device's messages work, and where every value came from, is written down in
[MECHANICS.md](MECHANICS.md).

QMK keyboards are changed the way VIA changes them: every edit is saved on the keyboard straight
away, so there's no Apply button for them, and nothing has to stay running afterwards.

## Build and run

Needs the Rust toolchain (on Windows with the GNU toolchain, also MinGW's `dlltool`/`as` on
`PATH`).

```
cargo build --release
target\release\openperiph.exe      # Windows
./target/release/openperiph         # Linux
```

Options: `--demo` adds a simulated QMK keyboard (an SK620) to try the QMK pages without one.
`--screenshot <file.png> [--page keys|lighting|macros|settings|mouse]` opens the window quietly,
draws it, saves a picture and exits.

## Linux

Linux only lets root open HID devices until a udev rule allows it. When a device needs one,
OpenPeriph shows **Allow access**: it adds a rule for that device to
`/etc/udev/rules.d/60-openperiph.rules` (asking for your password through `pkexec`). Unplug and
replug the device afterwards. To do it by hand instead, add a line like this for each device and
run `sudo udevadm control --reload && sudo udevadm trigger`:

```
KERNEL=="hidraw*", ATTRS{idVendor}=="feed", ATTRS{idProduct}=="0620", MODE="0660", TAG+="uaccess"
```

File dialogs (importing definitions, backups) go through the desktop's XDG portal, which KDE,
GNOME and most desktops provide.

## QMK keyboard definitions

QMK keyboards don't report their physical layout, so the keymap page uses the keyboard's VIA
definition. Bundled: `definitions/` (the SK620). For another keyboard, get its JSON from
[the-via/keyboards](https://github.com/the-via/keyboards) (folder `v3`) or its maker and use
**Import VIA definition** on the keyboard's Keymap or Settings page; a copy is kept in the
settings folder (`%APPDATA%\OpenPeriph\definitions` or `~/.config/OpenPeriph/definitions`).
Keyboards whose firmware uses an older VIA protocol than 12 (before QMK 0.20) aren't supported.

To regenerate the QMK keycode table from a QMK checkout:
`python3 tools/gen_qmk_keycodes.py /path/to/qmk_firmware > src/qmk/keycodes_table.rs`.

## Adding a device

A QMK keyboard with VIA needs no code: add its VIA definition to `definitions/` and to
`BUNDLED` in `src/qmk/definition.rs`.

Anything else is a driver implementing the `Driver` trait in `src/device.rs`; keyboard and mouse
parts have empty defaults, so a driver only implements what its hardware has. See
`src/drivers/ciy_x78` (keyboard) and `src/drivers/attack_shark_r5` (mouse), then add the new
driver to `src/drivers/mod.rs`.

## Notes

- The CIY X78 protocol was worked out from the vendor's Windows driver; the R5 ULTRA protocol
  from Attack Shark's web driver and the documented facts in
  [Shark Deck](https://github.com/alexandrewillame/shark-deck) (no code from either is included).
- The QMK/VIA support follows QMK's own source (protocol version 12); see MECHANICS.md.
- Not affiliated with CIY, Attack Shark, Cooler Master, QMK or VIA. Use at your own risk: settings are written to the
  device's own memory.

## Releases

Every push to `main` is built by GitHub Actions. To publish a release, bump `version` in
`Cargo.toml`, commit, then push a matching tag:

```
git tag v0.1.0
git push origin v0.1.0
```

The Release workflow builds `openperiph.exe` for Windows and `openperiph` for Linux (x86-64) and
attaches them (plus a zip and a tar.gz) to a new GitHub release.
