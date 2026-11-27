//! Firmware updates through the ISP bootloader in Sinowealth keyboard chips (SH68F89 and its
//! kin), as on the CIY X78 running SMK (github.com/clawntl/smk-ciy-x78).
//!
//! The protocol follows sinowisp (github.com/karstx/sinowisp, MIT), which is what the SMK
//! project flashes with: the running firmware is switched into the bootloader with feature report
//! 5 (`75`), and the bootloader (USB 0603:1020) takes commands as feature report 5 and moves
//! 2 KiB pages as feature report 6. Writing skips the first three bytes of flash (the reset
//! vector stays the bootloader's) and keeps the firmware's own jump at the end of the firmware
//! area instead, which "enable firmware" completes. The bootloader is never written.
//!
//! Some bootloaders scramble bytes in transit, so a bootloader is only trusted when its MD5 (read
//! through its own transform) is one we know; anything else stops before flash is erased.

use std::time::{Duration, Instant};

use md5::{Digest, Md5};

use crate::hid;

pub const ISP_VENDOR_ID: u16 = 0x0603;
pub const ISP_PRODUCT_IDS: [u16; 2] = [0x1020, 0x1021];

const REPORT_CMD: u8 = 0x05;
const REPORT_XFER: u8 = 0x06;
const CMD_LEN: usize = 6;

/// Asks the running firmware (CIY's or SMK) to restart into the bootloader.
const CMD_ISP_MODE: u8 = 0x75;
const CMD_ENABLE_FIRMWARE: u8 = 0x55;
const CMD_INIT_READ: u8 = 0x52;
const CMD_INIT_WRITE: u8 = 0x57;
const CMD_ERASE: u8 = 0x45;

const XFER_READ_PAGE: u8 = 0x72;
const XFER_WRITE_PAGE: u8 = 0x77;

/// Flash layout of a chip as its ISP bootloader sees it.
#[derive(Clone, Copy, Debug)]
pub struct Chip {
    /// Bytes before the bootloader: the most a firmware can use.
    pub firmware_size: usize,
    pub bootloader_size: usize,
    pub page_size: usize,
}

/// SH68F89 (CIY X77/X78): 60 KiB of firmware, then a 4 KiB bootloader.
pub const SH68F89: Chip = Chip { firmware_size: 61440, bootloader_size: 4096, page_size: 2048 };

/// A bootloader we know, and how it mangles bytes in transit (`offset` is within the page).
pub struct Bootloader {
    pub md5: &'static str,
    pub read: fn(usize, u8) -> u8,
    pub write: fn(usize, u8) -> u8,
}

/// The CIY X77/X78's bootloader (sinowisp's `ciy-x77`): measured against an ICP read.
fn read_571ea8b3(offset: usize, b: u8) -> u8 {
    if offset < 6 {
        b
    } else {
        b.wrapping_sub(0x5e)
    }
}
fn write_571ea8b3(_offset: usize, b: u8) -> u8 {
    b.wrapping_add(0x5a)
}

pub const BOOTLOADERS: &[Bootloader] = &[Bootloader { md5: "571ea8b315654c39046e4cc3b1e43777", read: read_571ea8b3, write: write_571ea8b3 }];

fn md5_hex(data: &[u8]) -> String {
    Md5::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// Firmware images

/// Intel HEX (as SMK builds it) to bytes: data records only, gaps filled with 0.
pub fn from_ihex(text: &str, max_len: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let bad = || format!("Line {} of the HEX file isn't valid.", n + 1);
        let hex = line.strip_prefix(':').ok_or_else(bad)?;
        if hex.len() % 2 != 0 || hex.len() < 10 {
            return Err(bad());
        }
        let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16)).collect::<Result<_, _>>().map_err(|_| bad())?;
        if bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0 {
            return Err(format!("Line {} of the HEX file has a bad checksum.", n + 1));
        }
        let count = bytes[0] as usize;
        if bytes.len() != count + 5 {
            return Err(bad());
        }
        let addr = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
        match bytes[3] {
            0x00 => {
                let end = addr + count;
                if end > max_len {
                    return Err("The firmware doesn't fit the keyboard's flash.".into());
                }
                if out.len() < end {
                    out.resize(end, 0);
                }
                out[addr..end].copy_from_slice(&bytes[4..4 + count]);
            }
            0x01 => break,
            0x03 | 0x05 => {}
            _ => return Err("The HEX file uses addresses beyond 64 KiB, so it isn't firmware for this keyboard.".into()),
        }
    }
    Ok(out)
}

/// A firmware file (Intel HEX or raw binary) as the image to write: padded with zeros to the
/// whole firmware area, starting with a jump (LJMP), clear of the five bytes at the end the
/// bootloader uses, and built for this keyboard (its USB ids appear in it).
pub fn image_from(data: &[u8], chip: &Chip, vendor_id: u16, product_id: u16) -> Result<Vec<u8>, String> {
    let mut img = if data.first() == Some(&b':') {
        from_ihex(std::str::from_utf8(data).map_err(|_| "That HEX file isn't text.".to_string())?, chip.firmware_size)?
    } else {
        data.to_vec()
    };
    if img.is_empty() {
        return Err("The firmware file is empty.".into());
    }
    let tail = chip.firmware_size - 5;
    if img.len() > chip.firmware_size || img.len() > tail && img[tail..].iter().any(|b| *b != 0) {
        return Err(format!("{} bytes doesn't fit the keyboard's flash.", img.len()));
    }
    let target = u16::from_be_bytes([img.get(1).copied().unwrap_or(0xFF), img.get(2).copied().unwrap_or(0xFF)]) as usize;
    if img[0] != 0x02 || target >= chip.firmware_size {
        return Err("That file isn't firmware for this keyboard (it doesn't start with a jump into the firmware).".into());
    }
    let mut ids = vendor_id.to_le_bytes().to_vec();
    ids.extend_from_slice(&product_id.to_le_bytes());
    if !img.windows(4).any(|w| w == ids) {
        return Err(format!("That file isn't firmware for this keyboard (it isn't built for USB {vendor_id:04X}:{product_id:04X})."));
    }
    img.resize(chip.firmware_size, 0);
    Ok(img)
}

// ---------------------------------------------------------------------------------------------
// The bootloader

/// The bootloader's USB connection (real, or simulated in tests).
pub trait IspLink {
    /// A feature report, report ID first: 5 goes to the command collection, 6 to the transfer one.
    fn send(&mut self, report: &[u8]) -> Result<(), String>;
    /// Reads feature report `buf[0]` into `buf`.
    fn get(&mut self, buf: &mut [u8]) -> Result<(), String>;
    fn wait(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

fn command<L: IspLink>(isp: &mut L, cmd: u8, addr: usize) -> Result<(), String> {
    let r: [u8; CMD_LEN] = [REPORT_CMD, cmd, (addr & 0xFF) as u8, (addr >> 8) as u8, 0, 0];
    isp.send(&r)
}

/// `len` bytes from `addr`, exactly as the bootloader sends them (no transform).
fn read_raw<L: IspLink>(isp: &mut L, chip: &Chip, addr: usize, len: usize, progress: &dyn Fn(f32)) -> Result<Vec<u8>, String> {
    command(isp, CMD_INIT_READ, addr)?;
    let pages = len / chip.page_size;
    let mut out = Vec::with_capacity(len);
    for p in 0..pages {
        let mut buf = vec![0u8; chip.page_size + 2];
        buf[0] = REPORT_XFER;
        isp.get(&mut buf)?;
        if buf[1] != XFER_READ_PAGE {
            return Err("The bootloader sent an unexpected answer while reading.".into());
        }
        out.extend_from_slice(&buf[2..]);
        progress((p + 1) as f32 / pages as f32);
    }
    Ok(out)
}

fn untransform(raw: &[u8], chip: &Chip, read: fn(usize, u8) -> u8) -> Vec<u8> {
    raw.iter().enumerate().map(|(i, b)| read(i % chip.page_size, *b)).collect()
}

/// Which known bootloader this is, by reading it. Needs "enable firmware" first.
fn identify<'a, L: IspLink>(isp: &mut L, chip: &Chip, known: &'a [Bootloader]) -> Result<&'a Bootloader, String> {
    let raw = read_raw(isp, chip, chip.firmware_size, chip.bootloader_size, &|_| {})?;
    known.iter().find(|b| md5_hex(&untransform(&raw, chip, b.read)) == b.md5).ok_or_else(|| {
        "This keyboard's bootloader isn't one OpenPeriph knows, so it hasn't been touched. Nothing was changed; unplug the keyboard and plug it back in.".to_string()
    })
}

/// Checks the bootloader, erases, writes, reads back and checks, then enables the new firmware.
/// The keyboard stays in its bootloader until it's unplugged. `progress(stage, fraction)`.
pub fn flash_with<L: IspLink>(isp: &mut L, chip: &Chip, image: &[u8], progress: &dyn Fn(&str, f32)) -> Result<(), String> {
    flash_with_bootloaders(isp, chip, image, BOOTLOADERS, progress)
}

fn flash_with_bootloaders<L: IspLink>(isp: &mut L, chip: &Chip, image: &[u8], known: &[Bootloader], progress: &dyn Fn(&str, f32)) -> Result<(), String> {
    if image.len() != chip.firmware_size {
        return Err("The firmware image is the wrong size.".into());
    }
    progress("Checking the bootloader", 0.0);
    // reading needs the firmware "enabled"; for firmware already installed through the
    // bootloader that byte is set already
    command(isp, CMD_ENABLE_FIRMWARE, 0)?;
    isp.wait(100);
    let boot = identify(isp, chip, known)?;

    progress("Erasing", 0.0);
    command(isp, CMD_ERASE, 0)?;
    isp.wait(2000);

    // the bootloader keeps its own reset vector at 0, and jumps to the firmware through a copy
    // of the firmware's at the end of the firmware area
    let mut fw = image.to_vec();
    fw.copy_within(1..3, chip.firmware_size - 4);
    command(isp, CMD_INIT_WRITE, 0)?;
    let pages = chip.firmware_size / chip.page_size;
    for p in 0..pages {
        let page = &fw[p * chip.page_size..(p + 1) * chip.page_size];
        let mut r = Vec::with_capacity(chip.page_size + 2);
        r.push(REPORT_XFER);
        r.push(XFER_WRITE_PAGE);
        r.extend(page.iter().enumerate().map(|(i, b)| (boot.write)(i, *b)));
        isp.send(&r)?;
        progress("Writing", (p + 1) as f32 / pages as f32);
    }

    fw[chip.firmware_size - 4..chip.firmware_size - 2].fill(0);
    let raw = read_raw(isp, chip, 0, chip.firmware_size, &|f| progress("Checking", f))?;
    let back = untransform(&raw, chip, boot.read);
    if let Some(at) = (0..fw.len()).find(|i| fw[*i] != back[*i]) {
        return Err(format!("Checking failed at 0x{at:04X}. The new firmware wasn't started, so the keyboard stays in its update mode: try the update again."));
    }

    progress("Starting the new firmware", 1.0);
    command(isp, CMD_ENABLE_FIRMWARE, 0)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Over USB

pub struct HidIsp {
    cmd: hid::Handle,
    xfer: hid::Handle,
}

impl HidIsp {
    /// The bootloader's command and transfer collections (on Linux the same hidraw node, on
    /// Windows two).
    pub fn open(cmd: &hid::Collection, xfer: &hid::Collection) -> Result<HidIsp, String> {
        let cmd = hid::open(&cmd.path).ok_or("Couldn't open the keyboard's update mode.")?;
        let xfer = hid::open(&xfer.path).ok_or("Couldn't open the keyboard's update mode.")?;
        Ok(HidIsp { cmd, xfer })
    }
}

impl IspLink for HidIsp {
    fn send(&mut self, report: &[u8]) -> Result<(), String> {
        let dev = if report[0] == REPORT_XFER { &self.xfer } else { &self.cmd };
        if dev.set_feature(report) {
            Ok(())
        } else {
            Err("The keyboard's update mode didn't accept a message (was it unplugged?).".into())
        }
    }
    fn get(&mut self, buf: &mut [u8]) -> Result<(), String> {
        let dev = if buf[0] == REPORT_XFER { &self.xfer } else { &self.cmd };
        if dev.get_feature(buf) {
            Ok(())
        } else {
            Err("The keyboard's update mode didn't answer.".into())
        }
    }
}

/// Whether a keyboard is sitting in its update mode (the bootloader's USB device is present).
pub fn in_update_mode() -> bool {
    #[cfg(target_os = "linux")]
    if crate::usbfs::find(ISP_VENDOR_ID, &ISP_PRODUCT_IDS).is_some() {
        return true;
    }
    ISP_PRODUCT_IDS.iter().any(|p| !hid::collections(ISP_VENDOR_ID, *p).is_empty())
}

/// The bootloader's collections, when a keyboard is in update mode: the ones carrying feature
/// reports 5 (commands) and 6 (pages), as sinowisp picks them. On Linux both are usually the
/// same hidraw node; on Windows each collection is its own device.
fn find_isp(_chip: &Chip) -> Option<(hid::Collection, hid::Collection)> {
    for pid in ISP_PRODUCT_IDS {
        let cols = hid::collections(ISP_VENDOR_ID, pid);
        let cmd = cols.iter().find(|c| c.feature_ids.contains(&REPORT_CMD));
        let xfer = cols.iter().find(|c| c.feature_ids.contains(&REPORT_XFER));
        if let (Some(c), Some(x)) = (cmd, xfer) {
            return Some((c.clone(), x.clone()));
        }
    }
    None
}

/// What the system shows of a keyboard in update mode, for the error when it can't be used.
fn update_mode_report() -> String {
    let mut seen = Vec::new();
    for pid in ISP_PRODUCT_IDS {
        for c in hid::collections(ISP_VENDOR_ID, pid) {
            seen.push(format!("{} usage {:04X}:{:04X} feature reports {:?} ({} bytes)", c.path, c.usage_page, c.usage, c.feature_ids, c.feature_len));
        }
    }
    #[cfg(target_os = "linux")]
    if seen.is_empty() {
        // is there a USB device at all (the HID driver may not have taken it)?
        let usb = std::fs::read_dir("/sys/bus/usb/devices")
            .map(|d| {
                d.flatten().any(|e| {
                    let id = |f: &str| std::fs::read_to_string(e.path().join(f)).unwrap_or_default().trim().to_lowercase();
                    id("idVendor") == "0603" && ISP_PRODUCT_IDS.iter().any(|p| id("idProduct") == format!("{p:04x}"))
                })
            })
            .unwrap_or(false);
        return if usb {
            "The keyboard's update mode (USB 0603:1020) is on USB, but couldn't be used; see `sudo dmesg | tail`.".into()
        } else {
            "No update-mode device (USB 0603:1020) appeared.".into()
        };
    }
    if seen.is_empty() {
        "No update-mode device (USB 0603:1020) appeared.".into()
    } else {
        format!("Seen: {}", seen.join("; "))
    }
}

/// The whole update: switch the keyboard into its bootloader, flash, enable. The keyboard then
/// needs unplugging and plugging back in (this bootloader doesn't restart it).
pub fn update(vendor_id: u16, product_id: u16, chip: &Chip, image: &[u8], progress: &dyn Fn(&str, f32)) -> Result<(), String> {
    let image = image_from(image, chip, vendor_id, product_id)?;
    if find_isp(chip).is_none() {
        let ctl = hid::collections(vendor_id, product_id)
            .into_iter()
            .find(|c| c.usage_page == 0xFF00 && c.feature_len as usize == CMD_LEN)
            .ok_or("The keyboard isn't connected.")?;
        progress("Switching to update mode", 0.0);
        let dev = hid::open(&ctl.path).ok_or("Couldn't open the keyboard.")?;
        let _ = dev.set_feature(&[REPORT_CMD, CMD_ISP_MODE, 0, 0, 0, 0]); // it drops off USB as it goes
        drop(dev);
        std::thread::sleep(Duration::from_millis(1500));
    }
    let start = Instant::now();
    let mut asked = false;
    #[cfg(target_os = "linux")]
    let mut raw_seen = 0u32;
    loop {
        if let Some((cmd, xfer)) = find_isp(chip) {
            // Linux: the update mode is another USB device, which may need its own udev rule
            if !asked && hid::permission_problem(&cmd) {
                asked = true;
                progress("Asking for permission to use the keyboard's update mode", 0.0);
                hid::install_udev_rules(&ISP_PRODUCT_IDS.map(|p| (ISP_VENDOR_ID, p)))?;
                std::thread::sleep(Duration::from_millis(1500));
                continue;
            }
            std::thread::sleep(Duration::from_millis(300));
            let mut link = HidIsp::open(&cmd, &xfer)?;
            return flash_with(&mut link, chip, &image, progress);
        }
        // Linux: the kernel's HID driver may not take the update mode at all (no hidraw node
        // appears), so its reports go as raw USB control requests instead. Seen twice in a
        // row, so a HID driver still binding gets its chance first.
        #[cfg(target_os = "linux")]
        if let Some(node) = crate::usbfs::find(ISP_VENDOR_ID, &ISP_PRODUCT_IDS) {
            raw_seen += 1;
            if raw_seen >= 2 {
                match crate::usbfs::Device::open(&node, 0) {
                    Ok(dev) => {
                        let mut link = UsbIsp(dev);
                        return flash_with(&mut link, chip, &image, progress);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !asked => {
                        asked = true;
                        progress("Asking for permission to use the keyboard's update mode", 0.0);
                        hid::install_usb_udev_rules(&ISP_PRODUCT_IDS.map(|p| (ISP_VENDOR_ID, p)))?;
                        std::thread::sleep(Duration::from_millis(1500));
                        continue;
                    }
                    Err(e) => return Err(format!("Couldn't open the keyboard's update mode ({e}), so nothing was changed. Unplug the keyboard and plug it back in.")),
                }
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(format!(
                "The keyboard didn't switch to its update mode, so nothing was changed. Unplug it, plug it back in and try again. ({})",
                update_mode_report()
            ));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// The update mode over raw USB (Linux, when no HID driver has it).
#[cfg(target_os = "linux")]
struct UsbIsp(crate::usbfs::Device);

#[cfg(target_os = "linux")]
impl IspLink for UsbIsp {
    fn send(&mut self, report: &[u8]) -> Result<(), String> {
        self.0.set_feature(report).map_err(|e| format!("The keyboard's update mode didn't accept a message ({e})."))
    }
    fn get(&mut self, buf: &mut [u8]) -> Result<(), String> {
        let n = self.0.get_feature(buf).map_err(|e| format!("The keyboard's update mode didn't answer ({e})."))?;
        if n < buf.len() {
            return Err(format!("The keyboard's update mode sent {n} bytes instead of {}.", buf.len()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CIY X78's bootloader, simulated: flash as the chip holds it, the transit scrambling,
    /// the skipped reset vector and the jump at the end of the firmware area.
    struct FakeBoot {
        flash: Vec<u8>,
        addr: usize,
        enabled: bool,
        erased: bool,
        corrupt_page: Option<usize>,
    }

    const FW: usize = SH68F89.firmware_size;
    const PAGE: usize = SH68F89.page_size;

    impl FakeBoot {
        fn new() -> FakeBoot {
            let mut flash = vec![0u8; FW + SH68F89.bootloader_size];
            // a made-up bootloader: tests accept its MD5 (flash_known), the real list doesn't
            for (i, b) in flash[FW..].iter_mut().enumerate() {
                *b = (i * 13 % 251) as u8;
            }
            flash[..3].copy_from_slice(&[0x02, 0xF0, 0x00]);
            FakeBoot { flash, addr: 0, enabled: false, erased: false, corrupt_page: None }
        }
        /// Addresses 1 and 2 (the jump's target) live at the end of the firmware area, as
        /// sinowisp's own simulation has it; that end reads as zeros.
        fn physical(a: usize) -> usize {
            match a {
                1 | 2 => FW - 4 + (a - 1),
                _ => a,
            }
        }
        fn byte_out(&self, a: usize) -> u8 {
            let v = if a == 0 {
                0x02
            } else if (FW - 5..FW - 2).contains(&a) {
                0
            } else {
                self.flash[Self::physical(a)]
            };
            let i = a % PAGE;
            if i < 6 {
                v
            } else {
                v.wrapping_add(0x5e)
            }
        }
    }

    impl IspLink for FakeBoot {
        fn send(&mut self, r: &[u8]) -> Result<(), String> {
            match (r[0], r[1]) {
                (REPORT_CMD, CMD_ENABLE_FIRMWARE) => {
                    self.enabled = true;
                    self.flash[FW - 5] = 0x02;
                }
                (REPORT_CMD, CMD_ERASE) => {
                    self.flash[..FW].fill(0xFF);
                    self.flash[..3].copy_from_slice(&[0x02, 0xF0, 0x00]);
                    self.erased = true;
                    self.enabled = false;
                }
                (REPORT_CMD, CMD_INIT_READ) | (REPORT_CMD, CMD_INIT_WRITE) => self.addr = r[2] as usize | (r[3] as usize) << 8,
                (REPORT_XFER, XFER_WRITE_PAGE) => {
                    assert_eq!(r.len(), PAGE + 2);
                    for (i, b) in r[2..].iter().enumerate() {
                        let a = self.addr + i;
                        if a != 0 {
                            let mut v = b.wrapping_sub(0x5a);
                            if self.corrupt_page == Some(self.addr / PAGE) && i == 100 {
                                v ^= 1;
                            }
                            self.flash[Self::physical(a)] = v;
                        }
                    }
                    self.addr += PAGE;
                }
                _ => panic!("unexpected report {:02x?}", &r[..2]),
            }
            Ok(())
        }
        fn get(&mut self, buf: &mut [u8]) -> Result<(), String> {
            assert_eq!(buf[0], REPORT_XFER);
            assert!(self.enabled || self.erased, "read before enabling");
            buf[1] = XFER_READ_PAGE;
            for i in 0..PAGE {
                buf[2 + i] = self.byte_out(self.addr + i);
            }
            self.addr += PAGE;
            Ok(())
        }
        fn wait(&mut self, _ms: u64) {}
    }

    /// SMK-like image: LJMP to 0x0100, the X78's USB ids in a descriptor, as Intel HEX.
    fn hex_image() -> String {
        let mut img: Vec<u8> = (0..23_000usize).map(|i| (i * 7 % 253) as u8).collect();
        img[..3].copy_from_slice(&[0x02, 0x01, 0x00]);
        img[500..504].copy_from_slice(&[0x8A, 0x25, 0x16, 0x00]);
        let mut s = String::new();
        for (n, chunk) in img.chunks(16).enumerate() {
            let a = n * 16;
            let mut rec = vec![chunk.len() as u8, (a >> 8) as u8, a as u8, 0];
            rec.extend_from_slice(chunk);
            let sum = rec.iter().fold(0u8, |x, y| x.wrapping_add(*y));
            rec.push(sum.wrapping_neg());
            s += &format!(":{}\n", rec.iter().map(|b| format!("{b:02X}")).collect::<String>());
        }
        s + ":00000001FF\n"
    }

    #[test]
    fn reads_hex_and_checks_images() {
        let img = image_from(hex_image().as_bytes(), &SH68F89, 0x258A, 0x0016).unwrap();
        assert_eq!(img.len(), FW);
        assert_eq!(&img[..3], &[0x02, 0x01, 0x00]);
        assert!(image_from(hex_image().as_bytes(), &SH68F89, 0x258A, 0x0017).unwrap_err().contains("258A:0017"));
        assert!(image_from(b"nonsense", &SH68F89, 0x258A, 0x0016).is_err());
        let mut bad = hex_image().into_bytes();
        bad[12] = b'0'; // break a checksum
        assert!(image_from(&bad, &SH68F89, 0x258A, 0x0016).is_err());
        assert!(image_from(&vec![0x02; FW], &SH68F89, 0x258A, 0x0016).is_err(), "too big");
    }

    #[test]
    fn known_bootloader_md5_is_the_x78s() {
        assert_eq!(BOOTLOADERS[0].md5, "571ea8b315654c39046e4cc3b1e43777");
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        // the transforms undo each other past the first six bytes of a page
        for b in 0..=255u8 {
            assert_eq!(read_571ea8b3(10, write_571ea8b3(10, b).wrapping_add(4)), b);
        }
    }

    #[test]
    fn flashes_verifies_and_enables() {
        let mut boot = FakeBoot::new();
        let img = image_from(hex_image().as_bytes(), &SH68F89, 0x258A, 0x0016).unwrap();
        flash_known(&mut boot, &img).unwrap();
        assert!(boot.enabled);
        assert_eq!(&boot.flash[3..FW - 5], &img[3..FW - 5]);
        assert_eq!(&boot.flash[..3], &[0x02, 0xF0, 0x00], "the bootloader keeps the reset vector");
        assert_eq!(&boot.flash[FW - 5..FW - 2], &[0x02, 0x01, 0x00], "the firmware is reached through the jump at the end");
    }

    #[test]
    fn stops_on_a_failed_check_without_enabling() {
        let mut boot = FakeBoot::new();
        boot.corrupt_page = Some(7);
        let img = image_from(hex_image().as_bytes(), &SH68F89, 0x258A, 0x0016).unwrap();
        let e = flash_known(&mut boot, &img).unwrap_err();
        assert!(e.starts_with("Checking failed at 0x3864"), "{e}");
        assert!(!boot.enabled);
    }

    #[test]
    fn refuses_an_unknown_bootloader_before_erasing() {
        let mut boot = FakeBoot::new();
        let img = image_from(hex_image().as_bytes(), &SH68F89, 0x258A, 0x0016).unwrap();
        let e = flash_with(&mut boot, &SH68F89, &img, &|_, _| {}).unwrap_err();
        assert!(e.contains("isn't one OpenPeriph knows"), "{e}");
        assert!(!boot.erased);
    }

    /// flash_with, with the simulated bootloader accepted as known.
    fn flash_known(boot: &mut FakeBoot, img: &[u8]) -> Result<(), String> {
        let raw: Vec<u8> = (FW..FW + SH68F89.bootloader_size).map(|a| boot.byte_out(a)).collect();
        let md5 = md5_hex(&untransform(&raw, &SH68F89, read_571ea8b3));
        let known = Bootloader { md5: Box::leak(md5.into_boxed_str()), read: read_571ea8b3, write: write_571ea8b3 };
        flash_with_bootloaders(boot, &SH68F89, img, &[known], &|_, _| {})
    }
}
