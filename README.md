# Peripherals

A small, modern settings app for PC peripherals, written in Rust with
[eframe/egui](https://github.com/emilk/egui). Windows only for now (it talks to devices through
Windows' own HID functions).

## Supported devices

| Device | What you can change |
|---|---|
| **CIY X78** keyboard (UK ISO, USB 258A:0016) | Remap any key on the normal and Fn layers (other keys, shortcuts, mouse buttons, media keys, off); lighting effect, colour, speed, brightness, direction, game-mode groups |
| **Attack Shark R5 ULTRA** mouse (USB 373E:0046 cable / 373E:0047 receiver) | DPI stages (up to 6, 100–42 000), stage in use, onboard profile, polling rate (to 8 kHz on the receiver), lift-off distance, motion sync, ripple control, angle snapping, click debounce, sleep timer; shows battery and firmware |

How each device's messages work, and where every value came from, is written down in
[MECHANICS.md](MECHANICS.md).

## Build and run

Needs the Rust toolchain (and, with the GNU toolchain, MinGW's `dlltool`/`as` on `PATH`).

```
cargo build --release
target\release\keyboard_settings.exe
```

`--screenshot <file.png>` opens the window quietly, draws it, saves a picture and exits.

## Adding a device

Each device is a driver implementing the `Driver` trait in `src/device.rs`; keyboard and mouse
parts have empty defaults, so a driver only implements what its hardware has. See
`src/drivers/ciy_x78` (keyboard) and `src/drivers/attack_shark_r5` (mouse), then add the new
driver to `src/drivers/mod.rs`.

## Notes

- The CIY X78 protocol was worked out from the vendor's Windows driver; the R5 ULTRA protocol
  from Attack Shark's web driver and the documented facts in
  [Shark Deck](https://github.com/alexandrewillame/shark-deck) (no code from either is included).
- Not affiliated with CIY or Attack Shark. Use at your own risk: settings are written to the
  device's own memory.
