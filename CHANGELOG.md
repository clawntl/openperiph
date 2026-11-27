# Changelog

Write changes under **Unreleased** as you make them. Releasing turns that heading into the new
version, and the section becomes the release notes on GitHub.

## Unreleased

- CIY X78: the Enter key is drawn as the ISO Enter it is (L-shaped, over two rows).
- CIY X78: **Any colour** on the Lighting page: the whole keyboard in any colour (colour picker,
  hex box, presets, brightness). Stored in the keyboard's colour group M5; M1–M4 aren't touched.
- Attack Shark R5 ULTRA: with only the receiver plugged in, the mouse is shown only while it's
  switched on and awake (OpenPeriph asks it for its battery level to check).

## v0.5.1 (2026-11-27)

- Windows installer (`openperiph-setup-x64.exe`): installs for your user, with a Start menu entry
  and an uninstaller; installed copies update through it. The portable `openperiph.exe` stays.
- New app icon, the same in the taskbar, the Start menu, the Linux menu and the app's top bar.
- The Device page opens with an "About this device" card: the picture, name and a few facts as
  tiles. The Picture section is gone.

## v0.5.0 (2026-11-27)

- Attack Shark R5 ULTRA: **Hunting Shark competitive mode** on the Performance page (runs the
  sensor at full performance; uses more battery). Hidden if the mouse's firmware doesn't have it.

## v0.4.0 (2026-11-27)

- New look: a neutral navy-grey theme with pale green buttons, the Inter typeface, smaller and
  tidier controls, and line icons.
- A tab for each plugged-in device along the top, and a Home page with a card for each one.
- Each device has its own navigation (Keymap, Lighting, Macros, Performance, Device) with a
  picture of it: drawn from its real key layout (in its current lighting colour for QMK
  keyboards), product photos of the SK620, X78 and R5 ULTRA, or a photo of your own (Device page → Picture).
- Appearance (About page): plain, faint smoke, or a picture of your own behind the panels.
- Firmware updates: asks whether to update when newer SK620 firmware is out, with the release
  notes. The Device page shows one status line (installed → latest) and one Update button; the
  rest is under More options. Readable progress, shown on every page while updating.
- OpenPeriph asks whether to update itself when a new version is out.
- Colour picker for QMK lighting: a hue × saturation area, a hex box and presets. Effects that
  make their own colours say so and offer Solid colour.
- Only plugged-in devices are shown; About lists every supported device.
- British spelling in labels.
- Documentation split into pages per device (`docs/`).

## v0.2.0 (2026-11-26)

- Firmware updates for the SK620 from GitHub, keeping your keymap, macros and lighting.
- OpenPeriph updates itself from GitHub releases (Windows, Linux, Arch package).
- Arch Linux / CachyOS package.

## v0.1.0 (2026-11-20)

- QMK/VIA keyboards: keymap, macros, lighting, backups. Linux support. CIY X78 and Attack Shark
  R5 ULTRA.
