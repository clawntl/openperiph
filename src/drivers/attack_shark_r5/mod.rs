//! Driver for the Attack Shark R5 ULTRA mouse ("JXC" firmware, USB 373E:0046 cable /
//! 373E:0047 receiver). Message layouts, codes and limits are written down, with their sources,
//! in docs/protocols/attack-shark-r5-ultra.md.
//!
//! Every message is one 64-byte feature report (report id 0) on the collection with usage page
//! 0xFFFF, usage 0:
//!   request `[id 00][status 00][00][target 02][length tag][category][command][profile][args…]`
//!   reply   `[id 00][status]  [00][target]   [length tag][category][command][data…]`
//! A read is the command | 0x80. Reply status A1 = ok, A0 = only the receiver answered (mouse
//! asleep, keep polling), A2 = unsupported, A3 = rejected. The firmware is known to crash on
//! malformed packets, so only the fixed frames in `Frame` are ever sent, and the bootloader and
//! profile-reset commands are never used.

use std::thread::sleep;
use std::time::Duration;

use crate::device::*;
use crate::hid;

const VID: u16 = 0x373E;
const PID_CABLE: u16 = 0x0046;
const PID_RECEIVER: u16 = 0x0047;
const USAGE_PAGE: u16 = 0xFFFF;
const USAGE: u16 = 0x0000;
const REPORT_LEN: usize = 65; // report id + 64 bytes
const TARGET_MOUSE: u8 = 0x02;

const OK: u8 = 0xA1;
const ASLEEP: u8 = 0xA0;
const UNSUPPORTED: u8 = 0xA2;

const STAGES: usize = 6;
const PROFILES: u8 = 3;
const SLEEP_NEVER: u32 = 0xFFFF;

/// `(length tag, category, command)` of every message this driver sends.
#[derive(Clone, Copy)]
struct Frame(u8, u8, u8);

const GET_FIRMWARE: Frame = Frame(0x10, 0x00, 0x81);
const GET_BATTERY: Frame = Frame(0x02, 0x00, 0x83);
const GET_PROFILE: Frame = Frame(0x01, 0x00, 0x85);
const SET_PROFILE: Frame = Frame(0x01, 0x00, 0x05);
const GET_DEBOUNCE: Frame = Frame(0x02, 0x00, 0x88);
const SET_DEBOUNCE: Frame = Frame(0x02, 0x00, 0x08);
const GET_SLEEP: Frame = Frame(0x03, 0x00, 0x87);
const SET_SLEEP: Frame = Frame(0x03, 0x00, 0x07);
const GET_POLL: Frame = Frame(0x02, 0x01, 0x80);
const SET_POLL: Frame = Frame(0x02, 0x01, 0x00);
const GET_DPI: Frame = Frame(0x0A, 0x01, 0x81);
/// The vendor's own driver always uses length tag 26 (room for 6 stages).
const SET_DPI: Frame = Frame(0x1A, 0x01, 0x01);
const GET_STAGE: Frame = Frame(0x02, 0x01, 0x82);
const SET_STAGE: Frame = Frame(0x02, 0x01, 0x02);
const GET_ANGLE: Frame = Frame(0x02, 0x01, 0x84);
const SET_ANGLE: Frame = Frame(0x02, 0x01, 0x04);
const GET_LOD: Frame = Frame(0x02, 0x01, 0x88);
const SET_LOD: Frame = Frame(0x02, 0x01, 0x08);
const GET_MOTION_SYNC: Frame = Frame(0x02, 0x01, 0x89);
const SET_MOTION_SYNC: Frame = Frame(0x02, 0x01, 0x09);
const GET_RIPPLE: Frame = Frame(0x02, 0x01, 0x8A);
const SET_RIPPLE: Frame = Frame(0x02, 0x01, 0x0A);
/// "Hunting Shark competitive mode" (the vendor's TrackingMode): newer firmware only.
const GET_COMPETITIVE: Frame = Frame(0x02, 0x01, 0x93);
const SET_COMPETITIVE: Frame = Frame(0x02, 0x01, 0x13);

/// Polling rate in Hz to its device code. Reads may also answer 0x10 for 1000 Hz.
const POLL_CODES: [(u32, u8); 7] = [(125, 0x08), (250, 0x04), (500, 0x02), (1000, 0x01), (2000, 0x20), (4000, 0x40), (8000, 0x80)];

fn poll_code(hz: u32) -> Option<u8> {
    POLL_CODES.iter().find(|(h, _)| *h == hz).map(|(_, c)| *c)
}

fn poll_hz(code: u8) -> Option<u32> {
    if code == 0x10 {
        return Some(1000);
    }
    POLL_CODES.iter().find(|(_, c)| *c == code).map(|(h, _)| *h)
}

/// One request buffer.
fn request(f: Frame, args: &[u8]) -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[3] = TARGET_MOUSE;
    buf[4] = f.0;
    buf[5] = f.1;
    buf[6] = f.2;
    let n = args.len().min(REPORT_LEN - 7);
    buf[7..7 + n].copy_from_slice(&args[..n]);
    buf
}

/// The DPI table message: profile, stage count, then X and Y (high byte first) per stage.
fn dpi_request(profile: u8, stages: &[u32]) -> [u8; REPORT_LEN] {
    let mut args = vec![profile, stages.len() as u8];
    for &d in stages.iter().take(STAGES) {
        let v = d as u16;
        args.extend_from_slice(&[(v >> 8) as u8, v as u8, (v >> 8) as u8, v as u8]);
    }
    request(SET_DPI, &args)
}

/// Reads the stage table out of a GET_DPI reply.
fn parse_dpi(reply: &[u8]) -> Vec<u32> {
    let count = (reply[8] as usize).clamp(1, STAGES);
    (0..count).map(|i| ((reply[9 + 4 * i] as u32) << 8) | reply[10 + 4 * i] as u32).collect()
}

pub struct AttackSharkR5 {
    info: DeviceInfo,
}

impl AttackSharkR5 {
    pub fn new() -> Self {
        AttackSharkR5 {
            info: DeviceInfo {
                id: "attack-shark-r5-ultra".into(),
                name: "R5 ULTRA".into(),
                maker: "Attack Shark".into(),
                kind: DeviceKind::Mouse,
                vendor_id: VID,
                product_id: PID_RECEIVER,
            },
        }
    }
}

fn spec(wired: bool) -> MouseSpec {
    MouseSpec {
        profiles: PROFILES,
        dpi_min: 100,
        dpi_max: 42000,
        dpi_step: 50,
        max_stages: STAGES,
        // DefaultDPIColors in the vendor's model list (ARGB): red, blue, green, magenta, yellow, white.
        stage_colours: vec![[255, 0, 0], [0, 0, 255], [0, 255, 0], [255, 0, 255], [255, 255, 0], [255, 255, 255]],
        poll_rates: if wired { vec![125, 250, 500, 1000] } else { vec![125, 250, 500, 1000, 2000, 4000, 8000] },
        lift_off: vec![(0x87, "0.7 mm"), (0x01, "1 mm"), (0x02, "2 mm")],
        debounce_max: 20,
        sleep_options: vec![(15, "15 s"), (60, "1 min"), (180, "3 min"), (300, "5 min"), (600, "10 min"), (1800, "30 min")],
        angle_snap: true,
        competitive: true,
    }
}

fn default_state() -> MouseState {
    MouseState {
        profile: 1,
        dpi_stages: vec![400, 800, 1600, 3200, 6400, 12800],
        active_stage: 1,
        poll_hz: 1000,
        lift_off: 0x01,
        motion_sync: false,
        ripple: false,
        angle_snap: false,
        competitive: false,
        debounce_ms: 4,
        sleep_secs: 60,
    }
}

// ---------------------------------------------------------------------------------------------
// Device access

struct Link {
    dev: hid::Handle,
    wired: bool,
}

enum Fail {
    Unsupported,
    Other(String),
}

impl Link {
    fn open() -> Result<Link, String> {
        // Prefer the cable when both are plugged in.
        for (pid, wired) in [(PID_CABLE, true), (PID_RECEIVER, false)] {
            let cols = hid::collections(VID, pid);
            if let Some(c) = cols.iter().find(|c| c.usage_page == USAGE_PAGE && c.usage == USAGE) {
                let dev = hid::open(&c.path).ok_or("Couldn't open the mouse (quit any other mouse app first).")?;
                return Ok(Link { dev, wired });
            }
        }
        Err("The mouse isn't connected.".into())
    }

    /// Sends one request and waits for the mouse's matching reply.
    fn exchange(&self, req: &[u8; REPORT_LEN], slow: bool) -> Result<[u8; REPORT_LEN], Fail> {
        if !self.dev.set_feature(req) {
            return Err(Fail::Other("The mouse didn't accept a message.".into()));
        }
        let waits = [if slow { 100 } else { 50 }, 30, 30, 100, 300, 300];
        let mut asleep = false;
        for ms in waits {
            sleep(Duration::from_millis(ms));
            let mut r = [0u8; REPORT_LEN];
            if !self.dev.get_feature(&mut r) {
                continue;
            }
            if r[3] != req[3] || r[6] != req[6] {
                continue;
            }
            match r[1] {
                OK => return Ok(r),
                ASLEEP => asleep = true,
                UNSUPPORTED => return Err(Fail::Unsupported),
                _ => return Err(Fail::Other("The mouse refused a setting.".into())),
            }
        }
        Err(Fail::Other(if asleep { "The mouse is asleep: move it to wake it, then try again.".into() } else { "The mouse didn't answer.".into() }))
    }

    fn get(&self, f: Frame, args: &[u8]) -> Result<[u8; REPORT_LEN], String> {
        self.exchange(&request(f, args), false).map_err(|e| match e {
            Fail::Unsupported => "The mouse doesn't support a setting this app asked for.".into(),
            Fail::Other(s) => s,
        })
    }

    fn set(&self, f: Frame, args: &[u8]) -> Result<(), String> {
        self.get(f, args).map(|_| ())
    }

    fn active_profile(&self) -> Result<u8, String> {
        let r = self.get(GET_PROFILE, &[0])?;
        Ok(if (1..=PROFILES).contains(&r[7]) { r[7] } else { 1 })
    }
}

impl Driver for AttackSharkR5 {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { key_remap: false, fn_layer: false, lighting: false, mouse: true, qmk: false }
    }

    fn detect(&self) -> bool {
        [PID_CABLE, PID_RECEIVER].iter().any(|pid| hid::collections(VID, *pid).iter().any(|c| c.usage_page == USAGE_PAGE && c.usage == USAGE))
    }

    fn usb_ids(&self) -> Vec<(u16, u16)> {
        vec![(VID, PID_CABLE), (VID, PID_RECEIVER)]
    }

    fn mouse_spec(&self, wired: bool) -> Option<MouseSpec> {
        Some(spec(wired))
    }

    fn default_mouse(&self) -> Option<MouseState> {
        Some(default_state())
    }

    fn read_mouse(&self, profile: Option<u8>) -> Result<(MouseState, MouseStatus), String> {
        let link = Link::open()?;
        let mut status = MouseStatus { wired: link.wired, ..Default::default() };
        if let Ok(r) = link.exchange(&request(GET_FIRMWARE, &[]), true) {
            status.firmware = Some(format!("{}.{}.{}.{}", r[7], r[8], r[9], r[10]));
        }
        if let Ok(r) = link.exchange(&request(GET_BATTERY, &[]), true) {
            status.charging = r[7] == 1;
            status.battery = (1..=100).contains(&r[8]).then_some(r[8]);
        }
        let pp = match profile {
            Some(p) => p.clamp(1, PROFILES),
            None => link.active_profile()?,
        };
        let dpi = link.get(GET_DPI, &[pp, STAGES as u8])?;
        let stages = parse_dpi(&dpi);
        let stage = link.get(GET_STAGE, &[pp])?[8] as usize;
        let poll = link.get(GET_POLL, &[pp])?[8];
        let lod = link.get(GET_LOD, &[pp])?[8];
        let flag = |f: Frame| link.exchange(&request(f, &[pp]), false).ok().map(|r| r[8] == 1);
        // Older firmware answers "unsupported" (A2): then the switch is hidden.
        let competitive = flag(GET_COMPETITIVE);
        status.competitive_supported = Some(competitive.is_some());
        let debounce = link.get(GET_DEBOUNCE, &[pp])?[8];
        let s = link.get(GET_SLEEP, &[pp])?;
        let d = default_state();
        let state = MouseState {
            profile: pp,
            active_stage: stage.saturating_sub(1).min(stages.len() - 1),
            dpi_stages: stages,
            poll_hz: poll_hz(poll).unwrap_or(d.poll_hz),
            lift_off: if spec(link.wired).lift_off.iter().any(|(c, _)| *c == lod) { lod } else { d.lift_off },
            motion_sync: flag(GET_MOTION_SYNC).unwrap_or(false),
            ripple: flag(GET_RIPPLE).unwrap_or(false),
            angle_snap: flag(GET_ANGLE).unwrap_or(false),
            competitive: competitive.unwrap_or(false),
            debounce_ms: debounce.min(20),
            sleep_secs: ((s[8] as u32) << 8) | s[9] as u32,
        };
        Ok((state, status))
    }

    fn apply_mouse(&self, st: &MouseState, progress: Progress) -> Result<(), String> {
        progress("Connecting to the mouse…");
        let link = Link::open()?;
        let sp = spec(link.wired);
        let pp = st.profile.clamp(1, PROFILES);
        if link.active_profile()? != pp {
            progress("Switching onboard profile…");
            link.set(SET_PROFILE, &[pp])?;
        }

        progress("Sending DPI stages…");
        let stages: Vec<u32> = st.dpi_stages.iter().take(STAGES).map(|d| (d / sp.dpi_step * sp.dpi_step).clamp(sp.dpi_min, sp.dpi_max)).collect();
        if stages.is_empty() {
            return Err("Keep at least one DPI stage.".into());
        }
        link.exchange(&dpi_request(pp, &stages), false).map_err(|_| "The mouse refused the DPI stages.".to_string())?;
        let stage = st.active_stage.min(stages.len() - 1) as u8 + 1;
        link.set(SET_STAGE, &[pp, stage])?;

        progress("Sending polling rate and sensor settings…");
        if let Some(code) = poll_code(st.poll_hz).filter(|_| sp.poll_rates.contains(&st.poll_hz)) {
            link.set(SET_POLL, &[pp, code])?;
        }
        if sp.lift_off.iter().any(|(c, _)| *c == st.lift_off) {
            link.set(SET_LOD, &[pp, st.lift_off])?;
        }
        link.set(SET_MOTION_SYNC, &[pp, st.motion_sync as u8])?;
        link.set(SET_RIPPLE, &[pp, st.ripple as u8])?;
        // Not every firmware has angle snapping: a refusal here is not an error.
        let _ = link.exchange(&request(SET_ANGLE, &[pp, st.angle_snap as u8]), false);
        // Competitive mode: only written when the mouse knows it and it needs changing.
        if let Ok(r) = link.exchange(&request(GET_COMPETITIVE, &[pp]), false) {
            if (r[8] == 1) != st.competitive {
                link.set(SET_COMPETITIVE, &[pp, st.competitive as u8])?;
            }
        }

        progress("Sending power settings…");
        link.set(SET_DEBOUNCE, &[pp, st.debounce_ms.min(sp.debounce_max)])?;
        let secs = if st.sleep_secs == 0 { SLEEP_NEVER } else { st.sleep_secs.min(0xFEFF) };
        link.set(SET_SLEEP, &[pp, (secs >> 8) as u8, secs as u8])?;

        progress("Checking…");
        let back = parse_dpi(&link.get(GET_DPI, &[pp, STAGES as u8])?);
        if back != stages {
            return Err("The mouse didn't keep the DPI stages as sent.".into());
        }
        progress("Done");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames() {
        let r = request(SET_POLL, &[2, 0x80]);
        assert_eq!(r.len(), 65);
        assert_eq!(&r[..9], &[0x00, 0x00, 0x00, 0x02, 0x02, 0x01, 0x00, 0x02, 0x80]);
        assert!(r[9..].iter().all(|b| *b == 0));
        let r = request(GET_PROFILE, &[0]);
        assert_eq!(&r[..8], &[0, 0, 0, 0x02, 0x01, 0x00, 0x85, 0x00]);
        let r = request(SET_SLEEP, &[1, 0x00, 0x3C]);
        assert_eq!(&r[3..10], &[0x02, 0x03, 0x00, 0x07, 0x01, 0x00, 0x3C]);
        // competitive mode, as the vendor's setTrackingMode / getTrackingMode build them
        let r = request(SET_COMPETITIVE, &[1, 1]);
        assert_eq!(&r[3..9], &[0x02, 0x02, 0x01, 0x13, 0x01, 0x01]);
        let r = request(GET_COMPETITIVE, &[1]);
        assert_eq!(&r[3..8], &[0x02, 0x02, 0x01, 0x93, 0x01]);
    }

    #[test]
    fn dpi_table() {
        let r = dpi_request(1, &[800, 1600, 42000]);
        assert_eq!(&r[3..9], &[0x02, 0x1A, 0x01, 0x01, 0x01, 0x03]);
        assert_eq!(&r[9..13], &[0x03, 0x20, 0x03, 0x20]);
        assert_eq!(&r[13..17], &[0x06, 0x40, 0x06, 0x40]);
        assert_eq!(&r[17..21], &[0xA4, 0x10, 0xA4, 0x10]);
        // A reply carries the same layout one byte later (count at 8, stages from 9).
        let mut reply = [0u8; REPORT_LEN];
        reply[8] = 3;
        reply[9..21].copy_from_slice(&r[9..21]);
        assert_eq!(parse_dpi(&reply), vec![800, 1600, 42000]);
    }

    /// Read-only check against a plugged-in mouse: only GET messages are sent. Run with
    /// `cargo test read_settings_from_mouse -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn read_settings_from_mouse() {
        let (m, s) = AttackSharkR5::new().read_mouse(None).expect("mouse");
        println!("{s:?}");
        println!("{m:?}");
    }

    #[test]
    fn poll_rates() {
        assert_eq!(poll_code(125), Some(0x08));
        assert_eq!(poll_code(8000), Some(0x80));
        assert_eq!(poll_hz(0x10), Some(1000));
        assert_eq!(poll_hz(0x20), Some(2000));
        assert_eq!(poll_code(300), None);
    }
}
