# CIY X78

The CIY X78 is a UK ISO keyboard (USB 258A:0016). It runs its own firmware, not QMK.

![The X78's lighting page](../images/x78-lighting.png)

## What you can change

- **Keys** on the normal and **Fn** layers: another key, a shortcut, a mouse button, a media key,
  or off.
- **Lighting**: effect, colour, speed, brightness, direction, and the game-mode colour groups.
  The effects only have 7 colours (red, blue, green, yellow, purple, cyan, white) or colourful:
  that's all the keyboard's effect settings can hold.
- **Any colour**: the whole keyboard in any colour you pick, with a brightness level. This uses
  the keyboard's per-key colours (Game mode, group M5), the same way the original app shows
  colours like orange. Fn+5 switches back to it on the keyboard. Groups M1–M4 aren't changed.

Changes are made in the app first. **Apply to device** then sends them. OpenPeriph keeps its own
copy of your key settings, because the keyboard can't report its key table back.

## With SMK firmware

[smk-ciy-x78](https://github.com/clawntl/smk-ciy-x78) is open firmware for this keyboard, built on
SMK. From its version 0.2.0 it speaks VIA, and OpenPeriph shows the keyboard as
**CIY X78 (SMK)**, with the QMK pages: the keymap on four layers (the keyboard keeps it, and it
can be read back), and lighting (effect, brightness, speed and a colour for the solid effect).
The definition is bundled (`definitions/ciy_x78_smk_iso.json`).

**Firmware updates**: the Device page offers new releases of smk-ciy-x78, as for the SK620. The
update goes through the keyboard's own update mode (its ISP bootloader, with the protocol of
[sinowisp](https://github.com/karstx/sinowisp)): OpenPeriph first reads the bootloader and only
carries on if it is the one we know (MD5 `571ea8b3…`); it never writes the bootloader. After
writing, it reads everything back and checks it before letting the new firmware start. This
bootloader doesn't restart the keyboard, so unplug it and plug it back in when asked. The keymap
is kept in the keyboard's data flash, which updates don't touch.

SMK has no free USB endpoint for QMK's usual raw HID interface, so it carries VIA's messages as
feature report 8 on its second interface. OpenPeriph uses that automatically for any VIA
interface with a 33-byte feature report and no output report.

## Notes

The protocol was worked out from the vendor's Windows driver. No vendor code is included. Every
value, and where it came from, is in [protocols/ciy-x78.md](../protocols/ciy-x78.md) and
[protocols/ciy-x78.json](../protocols/ciy-x78.json).
