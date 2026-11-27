# Protocol: Cooler Master SK620 firmware updates

Code: `src/qmk/firmware.rs`. Installed version: VIA keyboard value 4 (`major << 16 | minor << 8 |
patch`; firmware before 1.0.1 reports 0). Latest: `GET https://api.github.com/repos/clawntl/qmk-sk620/releases/latest`,
asset `cm_sk620_default.bin`.

| Step | How |
|---|---|
| Keep settings | VIA: read keymap, `06` reset, read defaults, write the keymap back; then back up keymap, macros, values |
| Bootloader | raw HID message `5B "SK620-BOOTLOADER"` (the firmware calls `reset_keyboard()`), then wait up to 10 s for USB 04D9:8010 |
| Protection | read option bytes at 0x1FF00000; flash security or protected pages means stock firmware: refuse |
| Erase | page erase `00 08` from 0 to the image end, wait 5 s |
| Write / check | `01 01` / `01 00` with 52 bytes each; every 30 packets wait 100 ms and read the status (HID GET_REPORT): one `O` or `F` per finished packet |
| Restart | `04 00` |
| Put back | after the restarted keyboard is read: keys still at the old default take the new default; everything else, macros and values as saved |

ISP packets are 64 bytes: `cmd, sub, crc16 (LE), arg1 (u32 LE), arg2 (u32 LE), data[52]`, the
CRC16-XMODEM computed with the CRC bytes zeroed.
