# Protocol: Attack Shark R5 ULTRA mouse

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
