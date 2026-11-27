# Development

OpenPeriph is written in Rust with [egui](https://github.com/emilk/egui).

## Building

You need the Rust toolchain. On Windows with the GNU toolchain, you also need MinGW's
`dlltool`/`as` on `PATH`.

```
cargo build --release
cargo test
target\release\openperiph.exe      # Windows
./target/release/openperiph         # Linux
```

Options:

- `--demo` adds a simulated SK620, so you can try the QMK pages without one.
- `--screenshot <file.png> --page <page>` opens the window quietly, draws it, saves a picture and
  exits. Pages: `home`, `keys`, `lighting`, `colour`, `macros`, `settings`, `mouse`,
  `mouse-device`, `x78`, `x78-lighting`, `about`, `fw-update`, `app-update`, `flashing`. The
  pictures in `docs/images` are made this way.

Code is formatted with `cargo fmt` (see `rustfmt.toml`).

## Layout of the code

| Path | What |
| --- | --- |
| `src/app.rs`, `src/app/` | The window: `shell.rs` (top bar, home, navigation, about), `theme.rs` (colours, fonts, background, icons, device drawings), `qmk_ui.rs` (QMK pages) |
| `src/drivers/` | One driver per device type |
| `src/qmk/` | VIA protocol, definitions, keycodes, macros, firmware updates |
| `src/hid.rs` | HID on Windows and Linux (hidraw) |
| `src/selfupdate.rs`, `src/net.rs` | Updates from GitHub |
| `definitions/` | Bundled VIA definitions |
| `packaging/` | Arch package, desktop file, udev rules |

## Adding a device

A QMK keyboard with VIA needs no code. Add its VIA definition to `definitions/` and to `BUNDLED`
in `src/qmk/definition.rs`.

Any other device is a driver implementing the `Driver` trait in `src/device.rs`. The keyboard and
mouse parts have empty defaults, so a driver only implements what its hardware has. See
`src/drivers/ciy_x78` (keyboard) and `src/drivers/attack_shark_r5` (mouse), then add the driver to
`src/drivers/mod.rs`. Add its USB ids to `packaging/linux/60-openperiph.rules`, and write down its
protocol in `docs/protocols/`.

To regenerate the QMK keycode table from a QMK checkout:
`python3 tools/gen_qmk_keycodes.py /path/to/qmk_firmware > src/qmk/keycodes_table.rs`.

## Releasing

GitHub Actions tests and builds every push and pull request for Windows, Linux and Arch.

To release:

1. Write what changed under **Unreleased** in [CHANGELOG.md](../CHANGELOG.md). This becomes the
   release notes.
2. On GitHub, open **Actions → Release → Run workflow**, pick **patch**, **minor** or **major**,
   and run it.

The workflow raises the version in `Cargo.toml` and commits it to `main`. It then builds
everything and publishes the release with `openperiph.exe`, the Linux program, the Arch package, a
zip, a tar.gz and `SHA256SUMS`. Installed copies of OpenPeriph offer the update when they start.

Ordinary pushes are only built, never released. To release the version that's already in
`Cargo.toml`, pick **none** instead of patch, minor or major.

## Credits

- [Inter](https://rsms.me/inter/) typeface by Rasmus Andersson, under the SIL Open Font Licence
  (`assets/fonts/Inter-LICENSE.txt`).
- The Attack Shark R5 ULTRA photo (`assets/devices/`) is Attack Shark's product photo, with the
  background removed.
- The QMK/VIA support follows QMK's own source. The device protocols credit their sources in
  `docs/protocols/`.
