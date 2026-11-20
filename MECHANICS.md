# Mechanics sheet: CIY X78 keyboard settings (UK ISO layout)

Decompiled with Ghidra 12.1.4 through Craft Anything (`native_function_pseudocode`):
`FUN_0043f4e0` (unlock command), `FUN_0043f6b0` (read settings), `FUN_0040dac0` (lighting
profile writer), `FUN_0043c970` (effect defaults loader), `FUN_0040cbe0` (key function → code),
`FUN_0040d300` (key table filler), `FUN_004168e0` (lighting page set-up, colour buttons),
`FUN_00417c50` (effect controls). The remaining addresses were read as disassembly.

Everything here was read from `OemDrv.exe` (32-bit MFC, stripped), its helper `Lowerdev.dll`
and its `Cfg.ini` / `text.xml`. Addresses are virtual addresses in OemDrv.exe unless marked
`Lowerdev`. Values marked **estimated** were looked for in the code and not found.

## Device

| What | Value | Source |
|---|---|---|
| USB vendor / product | 0x258A / 0x0016 | Cfg.ini `VID`, `PID`; path match `VID_%04X&PID_%04X` at 0x43f0d0 |
| Collection | usage page 0xFF00, usage 0x01 | `OpenHidDevice(0xff00, 1, filter)` at 0x43f16b / 0x43f18e |
| Data channel | the collection whose feature report is 0x408 (1032) bytes, report id 6 | filter 0x43f030 |
| Command channel | the collection whose feature report is 6 bytes, report id 5 | filter 0x43f040 |
| Transport | `HidD_SetFeature` / `HidD_GetFeature` | Lowerdev exports `SetFeature` 0x100014f0, `GetFeature` 0x10001510 |
| Retries | 3 tries, 100 ms apart | 0x43f460, 0x43f3e0, 0x43f4e0 |

## Messages

Command channel (report id 5):

| Name | Bytes sent | Length | Source |
|---|---|---|---|
| Unlock ("SetPassword") | `05 01 AA BB 2F 3E 00 00 00` | 9 | 0x43f4e0, `PSD=AA,BB,2F,3E,0` in Cfg.ini, called at 0x40dd23 |
| Ask for settings ("GetProfile") | `05 83 00 00 00 00 00` | 7 | 0x43f6b0 |

Data channel (report id 6, always 1032 bytes = id + 7-byte header + payload):

| Name | Header (after id 06) | Payload | Source |
|---|---|---|---|
| Settings reply (GetFeature) | `83 ...` (byte 1 must be 0x83) | 136 settings bytes at payload offset 7 | 0x43f794 / 0x43f79d |
| Write settings ("SetProfile") | `03 00 00 00 00 00 00` | 136 settings bytes | 0x43f5b0 |
| Write key table ("Matrix") | `04 C8 00 40 00 00 00` | 1024 bytes | 0x43f890, called 0x40e6ab |
| Write per-key colours ("SetLedMatrix") | `09 addr 00 40 00 00 00` | 792 bytes | 0x43f7e0 (not used by this app) |
| Write macro block ("SetMacroData") | `05 addr 00 40 00 00 00` | 1024 bytes | 0x43fa90 (not used by this app) |

Apply order in the original ("ApplySetting", 0x40dc10): unlock, wait 20 ms; read settings,
wait 30 ms; write settings with profile bytes 5 and 6 cleared, wait 90 ms; (colours); write key
table, wait 30 ms; write the edited settings again. This app follows the same order.

**Checked on the real keyboard (read-only, 2026-11-20):** it exposes FF00/0001 collections with
feature lengths 6 and 1032, and its settings reply matched the layout below (see
`shots/keyboard_settings_read.txt`). No writes were tested.

## Settings block (136 bytes)

`8 global bytes` + `profile 1 (64 bytes)` + `profile 2 (64 bytes)` (log labels
"Cfg(Load-Global)", "Cfg(Load-Pro1)", "Cfg(Save-Pro2)" at 0x40ddaa / 0x40e6ec).
The app edits profile 1 (`OurCfg` default 0, 0x43b3a2) and keeps every other byte as read.

Profile layout (builder 0x40dac0, offsets from defaults at 0x43b494 / 0x43b4aa):

| Byte | Meaning | Source |
|---|---|---|
| 4 (`LedModeSelectOffset`=4) | 1 when Game Mode is chosen, else 0 | 0x40dadc |
| 5 | effect code = effect number + 1 (`EffectHw` default 1..18, 0x43bb13); 0 = lights off (`RGBOFFIndex`=15) | 0x40daf5 |
| 6 | 0x20 (`StartGp`) + game group (M1..M5 = 0..4) | 0x40db14 |
| 18 + 2·i (`EffectOffset`=18) | low nibble: colour 0..6, or 7 = colourful; high nibble: direction (0 or 8) | 0x40db5c..0x40dbc4 |
| 19 + 2·i | high nibble: speed − 1; low nibble: brightness − 1 | 0x40dbc7..0x40dbdd |
| 62, 63 | signature 0x5A, 0xA5 | 0x40dbfa |

Direction: the original stores `8` for direction choice 1 and `0` otherwise, except for effects
listed in `Direct1=10,8,0` (Rainbow wheel, code 10) where it is reversed (0x40db80..0x40dbc4).

Effect record in the original (decompiled `FUN_0040dac0` and its reverse `FUN_0043c970`, record
at `profile + 0x33e4 + i·0x30`): `+0x08` → high nibble of byte 19+2i, `+0x0c` → low nibble,
`+0x10` direction choice, `+0x14` colour index, `+0x34` colourful flag. `FUN_00417c50` (shows an
effect's controls) puts `+0x08` on the **speed** slider (window `+0x137c`, position `+0x1768`) and
`+0x0c` on the **brightness** slider (window `+0x17c4`, position `+0x1bb0`): high nibble = speed,
low nibble = brightness, each stored as level − 1 (`FUN_0043c970`: level = nibble + 1).

Effects (names from `text.xml` `tc_led_mode1..16`). `LedOptN = icon, speed, brightness,
direction, colourful, colour`: `FUN_00417c50` copies them to `+0x2360` (speed slider shown),
`+0x2368` (brightness slider), `+0x2364` (direction buttons), `+0x2370` (colourful box),
`+0x2374` (the 7 colour buttons). Game mode (`GaoshouIndex`=14) and Off (`RGBOFFIndex`=15)
hide all of them.

| # | Name | speed | bright | direction | colour | colourful |
|---|---|---|---|---|---|---|
| 0 | Colour loop | yes | yes | no | no | no |
| 1 | Breathing | yes | yes | no | yes | yes |
| 2 | Always on | no | yes | no | yes | yes |
| 3 | Ripples | yes | yes | no | yes | yes |
| 4 | Reaction | yes | yes | no | yes | yes |
| 5 | Flash away | yes | yes | no | yes | yes |
| 6 | Sine wave | yes | yes | no | yes | yes |
| 7 | Raindrops | yes | yes | no | yes | yes |
| 8 | Neon stream | yes | yes | yes | no | no |
| 9 | Rainbow wheel | yes | yes | yes | no | no |
| 10 | Adorn | yes | yes | no | no | no |
| 11 | Stars | yes | yes | no | yes | yes |
| 12 | Shadow | yes | yes | no | yes | yes |
| 13 | Snake | yes | yes | no | yes | yes |
| 14 | Game mode (groups M1..M5) | – | – | – | – | – |
| 15 | Off | – | – | – | – | – |

Levels: `SpeedNum=5`, `BriNum=5`, `BriScale=5,25,50,75,100` (%). Defaults per effect from
`LEDParam` (colour byte 07 = colourful, 06 for Always on; speed/brightness byte 0x44 = level 5/5).

Basic colours 0..6 (decompiled `FUN_004168e0`, the lighting page set-up): the seven colour
buttons get the Windows colour values `0xff, 0xff0000, 0xff00, 0xffff, 0xff00ff, 0xffff00,
0xffffff` (Windows stores blue-green-red), i.e. **red, blue, green, yellow, purple, cyan, white**;
each button's number (`+0x88`) is its colour index. This matches "Always on" defaulting to 6
(white). Which end of the speed scale is fast: **estimated** level 5 = fastest (slider labels
"Slow"/"Fast" exist, `tc_adv_str16/18`, but their placement wasn't traced).

Direction when reading back from the keyboard: the original never decodes the keyboard's bytes
(it keeps its own profile file); this app reads them with the exact reverse of `FUN_0040dac0`.

## Key table (1024 bytes = 256 entries × 4 bytes)

Built by "FillMatrix" 0x40d770 and 0x40d300:

| Entries | Meaning |
|---|---|
| 0..131 | one per key, at its matrix index (column × 6 + row; 4th-from-last number of each `[KEY]` line) |
| 132..179 | "Fn single" table: the normal function of a key that also has an Fn function |
| 180..227 | "Fn combo" table: that key's Fn-layer function |
| 228..255 | zero |

Each entry is written as 4 bytes `[type, b1, b2, b3]` (the program byte-swaps its internal code,
0x40d39c):

| Function | Bytes | Source |
|---|---|---|
| Normal key (with optional Ctrl/Shift/Alt/Win) | `00 00 mods usage` (mods bits 1 Ctrl, 2 Shift, 4 Alt, 8 Win, ×16 for right side) | 0x40cd22..0x40cd6f |
| Modifier alone | `06 00 00 E0..E6` (mask 1,2,4,8,0x10,0x20,0x40 → E0..E6; mask 0x80 → E3, a quirk of the original's table at 0x40d060) | `FUN_0040cbe0` |
| Mouse button | `07 00 00 code`, codes F0, F1, F2, F3, F4, F7, F8 (and 0x130 for one special entry); names follow USB button order: left, right, middle, back, forward, wheel up, wheel down — **names estimated** | `FUN_0040cbe0`, jump table 0x40d01c |
| Media / app key | `04 00 hi lo`; the full list the original can send: 183, CD, B7, B6, B5, E9, EA, E2, 18A, 192, 194, 221, 223, 224, 225, 226, 227, 22A | `FUN_0040cbe0`, table 0x40d294 |
| Fn key | `20 00 00 00` | 0x40cc8f |
| Disabled | `00 00 00 00` **estimated** (first entry of the special-function table returns 0) | 0x40cd9b |
| Key with an Fn function | `02 00 00 n` and tables `[132+n]`, `[180+n]` | 0x40d3d7..0x40d415 |

Enter (Windows key 0x0D) becomes usage 0x28 (0x40cd06). Windows key code → USB usage pairs from the
table at 0x59df18. Default Fn functions from Cfg.ini `[FN]`; slots `n` are numbered in the order
the `[FN]` section lists them (**inferred**: the firmware only follows the slot number).


---

# Mechanics sheet: Attack Shark R5 ULTRA mouse

Sources: Attack Shark's own web driver ("MOUSE HUB", archived at
github.com/nyan2d/attack-shark-web-driver-dump: `Config/env-models.json` and the message
functions in `static/js/index-f6bb20fa.js`), and the documented protocol in Shark Deck
(github.com/alexandrewillame/shark-deck, `docs/protocol/mouse-jxc.md`; GPL-3.0, so only its facts
are used here, no code). Checked read-only on the person's mouse (`shots/mouse_settings_read.txt`).

| What | Value | Source |
|---|---|---|
| USB ids | 373E:0046 cable, 373E:0047 receiver | env-models.json `PIDWired` / `PIDWireless` |
| Collection | usage page 0xFFFF, usage 0, feature report 0, 64 bytes | Shark Deck doc; vendor `sendFeatureReport(0, 64 bytes)` |
| Request | `[00 status][00][02 target][length tag][category][command][profile][args]` | vendor builds `r[2]=2, r[3]=tag, r[4]=cat, r[5]=cmd, r[6]=profile` |
| Reply | byte 1 status (A1 ok, A0 mouse asleep, A2 unsupported, A3 rejected), echoes target and command; value at byte 8 (profile number at byte 7) | Shark Deck doc; vendor reads `i[8]` / `o[7]` |
| Waits | 50 ms after a request (100 ms for firmware/battery), then poll up to ~1 s | Shark Deck doc |
| Profiles | 3 onboard; read `01 00 85 00`, write `01 00 05 P` | vendor `getProfileID` |
| DPI table | read `0A 01 81 P 06`; write tag **1A** `01 01 P n` + n × (X hi, X lo, Y hi, Y lo); 100–42000 in steps of 50, 6 stages | vendor `setDPIStageInfo` (fixed tag 26), `DPIMax`, `DPIMaxStageNum` |
| Stage in use | read `02 01 82 P`, write `02 01 02 P s` (1-based) | vendor `getActiveDPI` |
| Polling rate | read `02 01 80 P`, write `02 01 00 P code`; 125=08, 250=04, 500=02, 1000=01 (reads may say 10), 2000=20, 4000=40, 8000=80; cable 125–1000, receiver to 8000 | Shark Deck doc (codes); vendor `i[7]==16 → 1`, `PollingRateWired/Wireless` |
| Lift-off | read `02 01 88 P`, write `02 01 08 P code`; 0.7 mm = 0x87, 1 mm = 01, 2 mm = 02 | vendor `setLOD` (`r*10 | 128` under 1 mm), `LOD=0.7;1;2` |
| Motion sync / ripple / angle snap | `02 01 09/0A/04 P 0|1` (reads 89/8A/84) | vendor `setMotionSync`, `setAngleSnap`; Shark Deck doc |
| Debounce | read `02 00 88 P`, write `02 00 08 P ms`, 0–20 ms | vendor `setDebounceTime`; Shark Deck doc (range) |
| Sleep | read `03 00 87 P`, write `03 00 07 P hi lo` in seconds, 65535 = never; offered 15 s, 1, 3, 5, 10, 30 min | vendor `setSleepTime`, `SleepTimeGrade`; Shark Deck doc (unit) |
| Battery / firmware | `02 00 83` (byte 7 charging, 8 %), `10 00 81` (bytes 7–10) | vendor `getFirmwareVersion`; Shark Deck doc |
| Stage colours (display only) | red, blue, green, magenta, yellow, white | env-models.json `DefaultDPIColors` |
| Never sent | bootloader (category B0), per-profile reset `01 00 0D` | Shark Deck doc: the firmware crashes on malformed packets |

Not in this version: lighting, button remapping and macros for the mouse (not documented by
either source in a form that could be checked).
