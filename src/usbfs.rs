//! Raw USB control transfers on Linux, through usbfs (`/dev/bus/usb/BBB/DDD`), for devices the
//! kernel's HID driver won't take. The Sinowealth ISP bootloader is one: its USB device appears
//! but no hidraw node does, so its HID reports are sent as plain SET_REPORT / GET_REPORT
//! control requests instead (sinowisp falls back to raw USB the same way).

use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// `_IOC(dir, 'U', nr, size)`, the usbfs ioctl numbers.
const fn ioc(dir: u64, nr: u64, size: usize) -> u64 {
    (dir << 30) | ((size as u64) << 16) | ((b'U' as u64) << 8) | nr
}
const READ: u64 = 2;
const WRITE_READ: u64 = 3;

#[repr(C)]
struct CtrlTransfer {
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    timeout_ms: u32,
    data: *mut libc::c_void,
}

#[repr(C)]
struct IoctlRequest {
    ifno: libc::c_int,
    code: libc::c_int,
    data: *mut libc::c_void,
}

const USBDEVFS_CONTROL: u64 = ioc(WRITE_READ, 0, std::mem::size_of::<CtrlTransfer>());
const USBDEVFS_CLAIMINTERFACE: u64 = ioc(READ, 15, std::mem::size_of::<libc::c_uint>());
const USBDEVFS_RELEASEINTERFACE: u64 = ioc(READ, 16, std::mem::size_of::<libc::c_uint>());
const USBDEVFS_IOCTL: u64 = ioc(WRITE_READ, 18, std::mem::size_of::<IoctlRequest>());
/// `_IO('U', 22)`: unbinds the kernel driver from an interface.
const USBDEVFS_DISCONNECT: u64 = ((b'U' as u64) << 8) | 22;

/// The device node of a USB device on the bus, by its ids.
pub fn find(vendor_id: u16, product_ids: &[u16]) -> Option<PathBuf> {
    for e in fs::read_dir("/sys/bus/usb/devices").ok()?.flatten() {
        let read = |f: &str| fs::read_to_string(e.path().join(f)).unwrap_or_default().trim().to_lowercase();
        if read("idVendor") != format!("{vendor_id:04x}") || !product_ids.iter().any(|p| read("idProduct") == format!("{p:04x}")) {
            continue;
        }
        let (bus, dev) = (read("busnum").parse::<u32>().ok()?, read("devnum").parse::<u32>().ok()?);
        return Some(PathBuf::from(format!("/dev/bus/usb/{bus:03}/{dev:03}")));
    }
    None
}

/// One claimed interface of a USB device.
pub struct Device {
    file: File,
    iface: u32,
}

impl Device {
    /// Opens the device and claims `iface`, taking it from a kernel driver if one has it.
    pub fn open(path: &Path, iface: u32) -> io::Result<Device> {
        let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        let fd = file.as_raw_fd();
        let claim = || {
            let n: libc::c_uint = iface;
            unsafe { libc::ioctl(fd, USBDEVFS_CLAIMINTERFACE as _, &n) }
        };
        if claim() < 0 {
            let mut req = IoctlRequest { ifno: iface as libc::c_int, code: USBDEVFS_DISCONNECT as libc::c_int, data: std::ptr::null_mut() };
            unsafe { libc::ioctl(fd, USBDEVFS_IOCTL as _, &mut req) };
            if claim() < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Device { file, iface })
    }

    /// One control request; `data` is sent (host to device) or filled (device to host),
    /// as `request_type` says. Returns the bytes moved.
    pub fn control(&self, request_type: u8, request: u8, value: u16, index: u16, data: &mut [u8], timeout_ms: u32) -> io::Result<usize> {
        let mut t = CtrlTransfer { request_type, request, value, index, length: data.len() as u16, timeout_ms, data: data.as_mut_ptr() as *mut libc::c_void };
        let n = unsafe { libc::ioctl(self.file.as_raw_fd(), USBDEVFS_CONTROL as _, &mut t) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    /// HID SET_REPORT (feature), `report[0]` being its ID.
    pub fn set_feature(&self, report: &[u8]) -> io::Result<()> {
        let mut buf = report.to_vec();
        self.control(0x21, 0x09, 0x0300 | report[0] as u16, self.iface as u16, &mut buf, 5000).map(|_| ())
    }

    /// HID GET_REPORT (feature) for report `buf[0]`, into `buf` (ID first).
    pub fn get_feature(&self, buf: &mut [u8]) -> io::Result<usize> {
        let id = buf[0];
        self.control(0xA1, 0x01, 0x0300 | id as u16, self.iface as u16, buf, 5000)
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let n: libc::c_uint = self.iface;
        unsafe { libc::ioctl(self.file.as_raw_fd(), USBDEVFS_RELEASEINTERFACE as _, &n) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers the kernel headers give on 64-bit Linux.
    #[test]
    fn ioctl_numbers() {
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(USBDEVFS_CONTROL, 0xC018_5500);
            assert_eq!(USBDEVFS_IOCTL, 0xC010_5512);
        }
        assert_eq!(USBDEVFS_CLAIMINTERFACE, 0x8004_550F);
        assert_eq!(USBDEVFS_RELEASEINTERFACE, 0x8004_5510);
        assert_eq!(USBDEVFS_DISCONNECT, 0x5516);
    }
}
