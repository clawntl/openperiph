//! Firmware updates for QMK keyboards whose firmware is published as GitHub releases and whose
//! bootloader this app can talk to. So far: the Cooler Master SK620 (github.com/clawntl/qmk-sk620),
//! whose Holtek HT32 ROM bootloader speaks Holtek's ISP protocol over USB HID.
//!
//! The installed version comes from VIA (keyboard value 4, `major << 16 | minor << 8 | patch`);
//! the newest from the GitHub API. Updating restarts the keyboard into its bootloader with a raw
//! HID message, erases the application pages, writes, verifies and restarts it.
//!
//! The ISP protocol follows the SK620 project's `tools/flash/sk620_flash.py` (itself based on
//! hansemro's ht32-dfu-tool): 64-byte packets `cmd sub crc16 arg1 arg2 data[52]`, CRC16-XMODEM
//! over the packet with the CRC bytes zeroed; results are read with a HID GET_REPORT, one byte
//! per finished command, 'O' (passed) or 'F' (failed).

use std::time::{Duration, Instant};

use crate::hid;

/// Where a keyboard's firmware comes from and how it is flashed.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub vendor_id: u16,
    pub product_id: u16,
    /// GitHub `owner/repo` whose releases carry the firmware.
    pub repo: &'static str,
    /// Release asset to download.
    pub asset: &'static str,
    /// Raw HID message (32 bytes, zero-padded) that restarts the keyboard into its bootloader.
    pub boot_message: &'static [u8],
    pub isp_vendor_id: u16,
    pub isp_product_id: u16,
}

pub const SOURCES: &[Source] = &[Source {
    vendor_id: 0xFEED,
    product_id: 0x0620,
    repo: "clawntl/qmk-sk620",
    asset: "cm_sk620_default.bin",
    boot_message: b"\x5bSK620-BOOTLOADER",
    isp_vendor_id: 0x04D9,
    isp_product_id: 0x8010,
}];

pub fn source_for(vid: u16, pid: u16) -> Option<&'static Source> {
    SOURCES.iter().find(|s| (s.vendor_id, s.product_id) == (vid, pid))
}

/// `v1.2.3` / `1.2.3` -> `0x010203`.
pub fn parse_version(tag: &str) -> Option<u32> {
    let t = tag.trim().trim_start_matches(['v', 'V']);
    let mut parts = t.split('.').map(|p| p.parse::<u32>().ok());
    let (a, b, c) = (parts.next()??, parts.next().unwrap_or(Some(0))?, parts.next().unwrap_or(Some(0))?);
    (a < 256 && b < 256 && c < 256).then_some((a << 16) | (b << 8) | c)
}

pub fn version_text(v: u32) -> String {
    format!("v{}.{}.{}", v >> 16, (v >> 8) & 0xFF, v & 0xFF)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub version: u32,
    pub download_url: String,
    pub sha256: Option<String>,
    pub page_url: String,
}

/// The newest release that has the firmware file.
pub fn latest(src: &Source) -> Result<Release, String> {
    let r = crate::net::latest_release(src.repo).map_err(|e| format!("Couldn't check for firmware updates: {e}"))?;
    let version = parse_version(&r.tag).ok_or_else(|| format!("The latest release, {}, has no version number.", r.tag))?;
    let a = r.assets.iter().find(|a| a.name == src.asset).ok_or_else(|| format!("Release {} has no {}.", r.tag, src.asset))?;
    Ok(Release { tag: r.tag.clone(), version, download_url: a.url.clone(), sha256: a.sha256.clone(), page_url: r.page_url })
}

pub fn download(r: &Release) -> Result<Vec<u8>, String> {
    let data = crate::net::download(&r.download_url, 512 * 1024, &|_| {})?;
    crate::net::verify(&data, r.sha256.as_deref())?;
    Ok(data)
}

const FLASH_SIZE: usize = 128 * 1024;
const OPTION_BYTES: u32 = 0x1FF0_0000;
const CHUNK: usize = 52;

/// A firmware image for address 0 of a 128 KiB HT32: fits, and starts with a vector table.
pub fn check_image(data: &[u8]) -> Result<(), String> {
    if data.is_empty() || data.len() > FLASH_SIZE - 512 {
        return Err(format!("{} bytes doesn't fit the keyboard's flash.", data.len()));
    }
    let sp = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let reset = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if !(0x2000_0000 < sp && sp <= 0x2000_4000 && reset & 1 == 1 && (reset as usize) < data.len()) {
        return Err("That file isn't SK620 firmware (it doesn't start with a valid vector table).".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Holtek ISP

fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for b in data {
        crc ^= (*b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

pub fn packet(cmd: u8, sub: u8, arg1: u32, arg2: u32, data: &[u8]) -> [u8; 64] {
    let mut p = [0u8; 64];
    p[0] = cmd;
    p[1] = sub;
    p[4..8].copy_from_slice(&arg1.to_le_bytes());
    p[8..12].copy_from_slice(&arg2.to_le_bytes());
    let n = data.len().min(CHUNK);
    p[12..12 + n].copy_from_slice(&data[..n]);
    let crc = crc16_xmodem(&p);
    p[2..4].copy_from_slice(&crc.to_le_bytes());
    p
}

fn cmd_info() -> [u8; 64] {
    packet(0x03, 0x00, 0, 0, &[])
}
fn cmd_read(addr: u32, n: u32) -> [u8; 64] {
    packet(0x01, 0x02, addr, addr + n - 1, &[])
}
fn cmd_write(addr: u32, d: &[u8]) -> [u8; 64] {
    packet(0x01, 0x01, addr, addr + d.len() as u32 - 1, d)
}
fn cmd_check(addr: u32, d: &[u8]) -> [u8; 64] {
    packet(0x01, 0x00, addr, addr + d.len() as u32 - 1, d)
}
fn cmd_page_erase(addr: u32, n: u32) -> [u8; 64] {
    packet(0x00, 0x08, addr, addr + n - 1, &[])
}
fn cmd_reset_app() -> [u8; 64] {
    packet(0x04, 0x00, 0, 0, &[])
}

/// The bootloader's USB connection (real, or simulated in tests).
pub trait IspLink {
    fn send(&mut self, p: &[u8; 64]) -> Result<(), String>;
    fn receive(&mut self, timeout_ms: u32) -> Result<[u8; 64], String>;
    /// The status report: one 'O' or 'F' per command finished since the last one.
    fn status(&mut self) -> Result<[u8; 64], String>;
    fn wait(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

pub struct HidIsp {
    dev: hid::Handle,
    out_len: usize,
    in_len: usize,
}

impl HidIsp {
    pub fn open(c: &hid::Collection) -> Result<HidIsp, String> {
        let dev = hid::open_io(&c.path)?;
        Ok(HidIsp { dev, out_len: (c.output_len as usize).max(65), in_len: (c.input_len as usize).max(65) })
    }
}

impl IspLink for HidIsp {
    fn send(&mut self, p: &[u8; 64]) -> Result<(), String> {
        let mut buf = vec![0u8; self.out_len];
        buf[1..65].copy_from_slice(p);
        if self.dev.write(&buf, 2000) {
            Ok(())
        } else {
            Err("The bootloader didn't accept a message.".into())
        }
    }
    fn receive(&mut self, timeout_ms: u32) -> Result<[u8; 64], String> {
        let mut buf = vec![0u8; self.in_len];
        let n = self.dev.read(&mut buf, timeout_ms).ok_or("The bootloader didn't answer.")?;
        let mut out = [0u8; 64];
        let m = n.saturating_sub(1).min(64);
        out[..m].copy_from_slice(&buf[1..1 + m]);
        Ok(out)
    }
    fn status(&mut self) -> Result<[u8; 64], String> {
        let mut buf = vec![0u8; self.in_len];
        if !self.dev.get_input_report(&mut buf) {
            return Err("Couldn't read the bootloader's status.".into());
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(&buf[1..65]);
        Ok(out)
    }
}

pub struct IspInfo {
    pub version: u16,
    pub page_size: u32,
    pub flash_size: u32,
}

fn info<L: IspLink>(isp: &mut L) -> Result<IspInfo, String> {
    isp.send(&cmd_info())?;
    let r = isp.receive(2000)?;
    let version = u16::from_le_bytes([r[2], r[3]]);
    let page_size = u16::from_le_bytes([r[6], r[7]]) as u32;
    let pages = u16::from_le_bytes([r[8], r[9]]) as u32;
    if page_size == 0 {
        return Err("The bootloader sent an unexpected answer.".into());
    }
    Ok(IspInfo { version, page_size, flash_size: page_size * pages })
}

/// Whether the chip still has the stock firmware's read protection, which only a mass erase
/// (with the SEL2 pads bridged) clears.
fn protected<L: IspLink>(isp: &mut L, image_len: usize, page_size: u32) -> Result<bool, String> {
    isp.send(&cmd_read(OPTION_BYTES, 64))?;
    let ob = isp.receive(2000)?;
    let security = ob[16] & 1 == 0;
    let pp = |i: usize| u32::from_le_bytes([ob[i * 4], ob[i * 4 + 1], ob[i * 4 + 2], ob[i * 4 + 3]]);
    let pages = (image_len as u32).div_ceil(page_size);
    let locked_page = (0..pages).any(|p| (pp((p / 32) as usize) >> (p % 32)) & 1 == 0);
    Ok(security || locked_page)
}

/// Sends write or check commands for the whole image, reading the status every 30 commands.
fn stream<L: IspLink>(
    isp: &mut L,
    image: &[u8],
    check: bool,
    progress: &dyn Fn(f32),
) -> Result<(), String> {
    let _ = isp.status(); // clear an old status
    let mut outstanding = 0usize;
    let mut off = 0usize;
    while off < image.len() {
        let chunk = &image[off..(off + CHUNK).min(image.len())];
        let p = if check { cmd_check(off as u32, chunk) } else { cmd_write(off as u32, chunk) };
        isp.send(&p)?;
        outstanding += 1;
        let last = off + chunk.len() >= image.len();
        if last || outstanding >= 30 {
            isp.wait(100);
            let s = isp.status()?;
            let passed = s.iter().filter(|b| **b == b'O').count();
            let failed = s.iter().filter(|b| **b == b'F').count();
            if failed > 0 {
                return Err(format!("{} failed near 0x{off:05X}.", if check { "Checking" } else { "Writing" }));
            }
            outstanding = outstanding.saturating_sub(passed);
        }
        off += chunk.len();
        progress(off as f32 / image.len() as f32);
    }
    Ok(())
}

/// Erases, writes, verifies and restarts. `progress(stage, fraction)`.
pub fn flash_with<L: IspLink>(isp: &mut L, image: &[u8], progress: &dyn Fn(&str, f32)) -> Result<(), String> {
    check_image(image)?;
    let i = info(isp)?;
    if image.len() as u32 > i.flash_size {
        return Err("The firmware is bigger than the keyboard's flash.".into());
    }
    if protected(isp, image.len(), i.page_size)? {
        return Err("This keyboard still has Cooler Master's protected firmware. Do the first flash with sk620_flash \
                    (it needs the SEL2 pads bridged); after that, updates work from here."
            .into());
    }
    progress("Erasing", 0.0);
    isp.send(&cmd_page_erase(0, image.len() as u32))?;
    isp.wait(5000);
    stream(isp, image, false, &|f| progress("Writing", f))?;
    stream(isp, image, true, &|f| progress("Checking", f))?;
    progress("Restarting", 1.0);
    isp.send(&cmd_reset_app())?;
    let _ = i.version;
    Ok(())
}

fn find(vid: u16, pid: u16, usage_page: Option<u16>) -> Option<hid::Collection> {
    hid::collections(vid, pid).into_iter().find(|c| usage_page.is_none_or(|u| c.usage_page == u))
}

/// The whole update: restart into the bootloader, flash, restart.
pub fn update(src: &Source, image: &[u8], progress: &dyn Fn(&str, f32)) -> Result<(), String> {
    check_image(image)?;
    if find(src.isp_vendor_id, src.isp_product_id, None).is_none() {
        let raw = find(src.vendor_id, src.product_id, Some(super::via::USAGE_PAGE)).ok_or("The keyboard isn't connected.")?;
        progress("Restarting into the bootloader", 0.0);
        let dev = hid::open_io(&raw.path)?;
        let mut msg = vec![0u8; (raw.output_len as usize).max(33)];
        let n = src.boot_message.len().min(32);
        msg[1..1 + n].copy_from_slice(&src.boot_message[..n]);
        let _ = dev.write(&msg, 500); // it may drop off USB before the write completes
        drop(dev);
    }
    let start = Instant::now();
    let isp = loop {
        if let Some(c) = find(src.isp_vendor_id, src.isp_product_id, None) {
            break c;
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err("The keyboard didn't restart into its bootloader. Press Fn+B and try again.".into());
        }
        std::thread::sleep(Duration::from_millis(300));
    };
    // Linux: the bootloader is a different USB device and may need its own udev rule.
    if hid::permission_problem(&isp) {
        progress("Asking for permission to use the bootloader", 0.0);
        hid::install_udev_rules(&[(src.isp_vendor_id, src.isp_product_id)])?;
        std::thread::sleep(Duration::from_millis(1500));
    }
    std::thread::sleep(Duration::from_millis(300));
    let mut link = HidIsp::open(&isp)?;
    flash_with(&mut link, image, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulated HT32 bootloader: 128 KiB, 512-byte pages, optionally still protected.
    struct FakeIsp {
        flash: Vec<u8>,
        protected: bool,
        queue: Vec<[u8; 64]>,
        results: Vec<u8>,
        reset: bool,
        corrupt_at: Option<usize>,
    }

    impl FakeIsp {
        fn new(protected: bool) -> FakeIsp {
            FakeIsp { flash: vec![0xFF; FLASH_SIZE], protected, queue: Vec::new(), results: Vec::new(), reset: false, corrupt_at: None }
        }
    }

    impl IspLink for FakeIsp {
        fn send(&mut self, p: &[u8; 64]) -> Result<(), String> {
            let mut q = *p;
            q[2] = 0;
            q[3] = 0;
            assert_eq!(crc16_xmodem(&q), u16::from_le_bytes([p[2], p[3]]), "bad CRC");
            let a1 = u32::from_le_bytes([p[4], p[5], p[6], p[7]]) as usize;
            let a2 = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
            let mut reply = [0u8; 64];
            match (p[0], p[1]) {
                (0x03, 0x00) => {
                    reply[2..4].copy_from_slice(&0x103u16.to_le_bytes());
                    reply[6..8].copy_from_slice(&512u16.to_le_bytes());
                    reply[8..10].copy_from_slice(&255u16.to_le_bytes());
                    self.queue.push(reply);
                }
                (0x01, 0x02) => {
                    if a1 == OPTION_BYTES as usize {
                        let pp = if self.protected { 0xFFFF_F000u32 } else { 0xFFFF_FFFF };
                        for i in 0..4 {
                            reply[i * 4..i * 4 + 4].copy_from_slice(&pp.to_le_bytes());
                        }
                        reply[16] = if self.protected { 0xFC } else { 0xFF };
                    }
                    self.queue.push(reply);
                }
                (0x00, 0x08) => {
                    for b in &mut self.flash[a1..=a2] {
                        *b = 0xFF;
                    }
                }
                (0x01, 0x01) => {
                    let n = a2 - a1 + 1;
                    let mut d = p[12..12 + n].to_vec();
                    if self.corrupt_at == Some(a1) {
                        d[0] ^= 1;
                    }
                    self.flash[a1..=a2].copy_from_slice(&d);
                    self.results.push(b'O');
                }
                (0x01, 0x00) => {
                    let n = a2 - a1 + 1;
                    self.results.push(if self.flash[a1..=a2] == p[12..12 + n] { b'O' } else { b'F' });
                }
                (0x04, 0x00) => self.reset = true,
                _ => panic!("unexpected command {:02x} {:02x}", p[0], p[1]),
            }
            Ok(())
        }
        fn receive(&mut self, _t: u32) -> Result<[u8; 64], String> {
            if self.queue.is_empty() {
                return Err("no answer".into());
            }
            Ok(self.queue.remove(0))
        }
        fn status(&mut self) -> Result<[u8; 64], String> {
            let mut s = [0u8; 64];
            for (i, r) in self.results.drain(..).take(64).enumerate() {
                s[i] = r;
            }
            Ok(s)
        }
        fn wait(&mut self, _ms: u64) {}
    }

    fn image() -> Vec<u8> {
        let mut img: Vec<u8> = (0..33_000u32).map(|i| (i * 7 % 251) as u8).collect();
        img[..4].copy_from_slice(&0x2000_0400u32.to_le_bytes());
        img[4..8].copy_from_slice(&0x191u32.to_le_bytes());
        img
    }

    #[test]
    fn flashes_and_verifies() {
        let mut isp = FakeIsp::new(false);
        let img = image();
        flash_with(&mut isp, &img, &|_, _| {}).unwrap();
        assert_eq!(&isp.flash[..img.len()], &img[..]);
        assert!(isp.reset);
    }

    #[test]
    fn refuses_protected_stock_chip() {
        let mut isp = FakeIsp::new(true);
        let e = flash_with(&mut isp, &image(), &|_, _| {}).unwrap_err();
        assert!(e.contains("sk620_flash"));
        assert!(!isp.reset);
    }

    #[test]
    fn reports_failed_verify() {
        let mut isp = FakeIsp::new(false);
        isp.corrupt_at = Some(52 * 40);
        let e = flash_with(&mut isp, &image(), &|_, _| {}).unwrap_err();
        assert!(e.starts_with("Checking failed"), "{e}");
        assert!(!isp.reset);
    }

    /// Network: asks GitHub for the SK620's latest release and downloads it.
    /// `cargo test live_release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_release() {
        let r = latest(&SOURCES[0]).unwrap();
        println!("{r:?}");
        let img = download(&r).unwrap();
        check_image(&img).unwrap();
        println!("{} bytes", img.len());
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.0.1"), Some(0x010001));
        assert_eq!(parse_version("2.3"), Some(0x020300));
        assert_eq!(parse_version("latest"), None);
        assert_eq!(version_text(0x010203), "v1.2.3");
        assert!(check_image(&image()).is_ok());
        assert!(check_image(b"nonsense").is_err());
    }
}
