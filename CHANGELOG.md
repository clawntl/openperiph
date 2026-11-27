# Changelog

Write changes under **Unreleased** as you make them. Releasing turns that heading into the new
version, and the section becomes the release notes on GitHub.

## Unreleased

## v0.6.4 (2026-11-27)

- Windows, CIY X78 on SMK: fixed "couldn't read the keyboard" and lighting changes not working.
  Its VIA messages go as feature reports, and Windows needs a non-overlapped handle for those;
  with an overlapped one the answer could be read before the keyboard had sent it.
- Windows: keyboards are found much faster, and the window no longer stops responding while
  one is being found. Device names are asked for once per device instead of on every look
  (each is a USB request that waits behind whatever the device is doing), connecting only looks
  at the keyboard's own interfaces, and the lookup runs off the window's thread.

## v0.6.3 (2026-11-27)

- CIY X78 on SMK: fixed updates stopping with "The keyboard didn't switch to its update mode".
  The keyboard did switch, but on Linux the kernel's HID driver doesn't take its update mode, so
  there was nothing for OpenPeriph to open. OpenPeriph now talks to it over raw USB in that case
  (as sinowisp does), asking once for permission to use it. The update mode is also found by its
  report numbers (5 and 6) rather than their sizes. If it still can't be used, the message says
  what was seen, and nothing on the keyboard is changed.
- During an update the keyboard's page stays open, instead of going back to Home when the
  keyboard drops off USB into its update mode.

## v0.6.2 (2026-11-27)

- CIY X78 on SMK, after an update: a window walks you through unplugging and plugging the
  keyboard back in (the keyboard stays in its update mode until you do), ticking off each step as
  it happens, and closes once the keyboard is back. The Firmware card shows the same steps.
- The Firmware card no longer says an update is available while the keyboard hasn't been read
  yet (it compared the release with "no version"); it says it's reading the keyboard instead.

## v0.6.1 (2026-11-27)

- CIY X78 running SMK: **firmware updates** from the Device page, as on the SK620. New releases of
  [smk-ciy-x78](https://github.com/clawntl/smk-ciy-x78) are offered; OpenPeriph switches the
  keyboard to its update mode, checks its bootloader is the known one (before erasing anything),
  writes, reads back and checks, and then asks you to unplug and replug the keyboard. "More
  options" can also flash a `.hex` build of your own.
- Keyboards running SMK are labelled as SMK keyboards, not QMK ones.

## v0.6.0 (2026-11-27)

- CIY X78 running [SMK](https://github.com/clawntl/smk-ciy-x78) firmware (0.2.0 or newer): shown
  as **CIY X78 (SMK)** with the keymap (four layers) and lighting pages, through VIA. Its
  definition is bundled. VIA messages can now go over a feature report, for firmware like SMK
  that has no endpoint for QMK's raw HID interface.
- CIY X78: the keyboard running SMK is no longer also listed as the CIY firmware one.

## v0.5.3 (2026-11-27)

- CIY X78: Any colour now lights every LED (some stayed dark in v0.5.2).

## v0.5.2 (2026-11-27)

- CIY X78: the Enter key is drawn as the ISO Enter it is (L-shaped, over two rows).
- CIY X78: every key with a Shift character shows it above the key's own one, as on the caps
  (UK layout: ¬ ! " £ $ % ^ & * ( ) _ + { } : @ ~ | < > ?).
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
