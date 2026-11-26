//! Talking to USB HID devices, shared by every driver.
//!
//! - Windows: Windows' own HID functions (setupapi + hid.dll), the same route the CIY X78's
//!   original program's helper library takes.
//! - Linux: the kernel's hidraw interface (`/dev/hidraw*`), found through sysfs. Each hidraw node
//!   is one USB interface; its report descriptor is split into top-level collections here so the
//!   rest of the app sees the same thing on both systems.
//!
//! Report buffers follow the Windows convention everywhere: byte 0 is the report ID (0 when the
//! device doesn't number its reports), and lengths include that byte.

/// One top-level HID collection (a "device" in Windows terms).
#[derive(Clone, Debug)]
pub struct Collection {
    /// What to pass to `open`: a Windows device path or `/dev/hidrawN`.
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    /// Longest report of each kind in bytes, including the report ID byte; 0 = none.
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
}

// ---------------------------------------------------------------------------------------------
// Report descriptor parsing (used on Linux; kept portable so it can be tested anywhere)

/// What a report descriptor says about one top-level collection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DescCollection {
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
}

/// Splits a report descriptor into its top-level (application) collections. Also returns
/// whether the device numbers its reports.
#[cfg_attr(windows, allow(dead_code))]
pub fn parse_descriptor(d: &[u8]) -> (Vec<DescCollection>, bool) {
    use std::collections::BTreeMap;
    #[derive(Clone, Copy, Default)]
    struct Globals {
        usage_page: u16,
        report_size: u32,
        report_count: u32,
        report_id: u8,
    }
    let mut g = Globals::default();
    let mut stack: Vec<Globals> = Vec::new();
    let mut usages: Vec<u32> = Vec::new();
    let mut depth = 0usize;
    let mut out: Vec<DescCollection> = Vec::new();
    // bits per (kind, report id) for the current top-level collection
    let mut bits: BTreeMap<(u8, u8), u32> = BTreeMap::new();
    let mut numbered = false;

    let finish = |bits: &mut BTreeMap<(u8, u8), u32>, c: &mut DescCollection| {
        let len = |kind: u8| {
            bits.iter().filter(|((k, _), _)| *k == kind).map(|(_, b)| b.div_ceil(8) + 1).max().unwrap_or(0) as u16
        };
        c.input_len = len(0);
        c.output_len = len(1);
        c.feature_len = len(2);
        bits.clear();
    };

    let mut i = 0usize;
    while i < d.len() {
        let prefix = d[i];
        if prefix == 0xFE {
            // long item: size byte, tag byte, data
            let size = *d.get(i + 1).unwrap_or(&0) as usize;
            i += 3 + size;
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            n => n as usize,
        };
        if i + 1 + size > d.len() {
            break;
        }
        let mut data = 0u32;
        for (k, b) in d[i + 1..i + 1 + size].iter().enumerate() {
            data |= (*b as u32) << (8 * k);
        }
        let kind = (prefix >> 2) & 3;
        let tag = prefix >> 4;
        i += 1 + size;
        match (kind, tag) {
            // main items
            (0, 8) | (0, 9) | (0, 11) => {
                if depth > 0 {
                    let k = match tag {
                        8 => 0,
                        9 => 1,
                        _ => 2,
                    };
                    *bits.entry((k, g.report_id)).or_default() += g.report_size * g.report_count;
                }
                usages.clear();
            }
            (0, 10) => {
                if depth == 0 {
                    let u = usages.first().copied().unwrap_or(0);
                    let page = if size == 4 || u > 0xFFFF { (u >> 16) as u16 } else { g.usage_page };
                    out.push(DescCollection { usage_page: page, usage: (u & 0xFFFF) as u16, ..Default::default() });
                }
                depth += 1;
                usages.clear();
            }
            (0, 12) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(c) = out.last_mut() {
                        finish(&mut bits, c);
                    }
                }
                usages.clear();
            }
            // global items
            (1, 0) => g.usage_page = data as u16,
            (1, 7) => g.report_size = data,
            (1, 8) => {
                g.report_id = data as u8;
                numbered = true;
            }
            (1, 9) => g.report_count = data,
            (1, 10) => stack.push(g),
            (1, 11) => g = stack.pop().unwrap_or_default(),
            // local items
            (2, 0) => usages.push(if size == 4 { data } else { data & 0xFFFF }),
            _ => {}
        }
    }
    (out, numbered)
}

// ---------------------------------------------------------------------------------------------
// Windows

#[cfg(windows)]
mod imp {
    use super::Collection;
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::{null, null_mut};

    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    };
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetFeature, HidD_GetHidGuid,
        HidD_GetManufacturerString, HidD_GetPreparsedData, HidD_GetProductString,
        HidD_GetSerialNumberString, HidD_SetFeature, HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows_sys::Win32::System::IO::{CancelIo, GetOverlappedResult, OVERLAPPED};

    const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;

    pub struct Handle {
        h: HANDLE,
        /// Event for overlapped reads and writes (null for feature-only handles).
        event: HANDLE,
    }

    // The handle is only used from one thread at a time.
    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.h);
                if !self.event.is_null() {
                    CloseHandle(self.event);
                }
            }
        }
    }

    fn wide(path: &str) -> Vec<u16> {
        path.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open_raw(path: &str, access: u32, flags: u32) -> Option<HANDLE> {
        let w = wide(path);
        let h = unsafe {
            CreateFileW(w.as_ptr(), access, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, flags, null_mut())
        };
        if h == INVALID_HANDLE_VALUE || h.is_null() {
            None
        } else {
            Some(h)
        }
    }

    /// Open for feature reports.
    pub fn open(path: &str) -> Option<Handle> {
        let h = open_raw(path, GENERIC_READ | GENERIC_WRITE, 0).or_else(|| open_raw(path, 0, 0))?;
        Some(Handle { h, event: null_mut() })
    }

    /// Open for input and output reports (and feature reports).
    pub fn open_io(path: &str) -> Result<Handle, String> {
        let h = open_raw(path, GENERIC_READ | GENERIC_WRITE, FILE_FLAG_OVERLAPPED)
            .ok_or("Couldn't open the device (is another app using it?).")?;
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() {
            unsafe { CloseHandle(h) };
            return Err("Couldn't set up the device connection.".into());
        }
        Ok(Handle { h, event })
    }

    impl Handle {
        pub fn set_feature(&self, buf: &[u8]) -> bool {
            unsafe { HidD_SetFeature(self.h, buf.as_ptr() as *const c_void, buf.len() as u32) }
        }
        pub fn get_feature(&self, buf: &mut [u8]) -> bool {
            unsafe { HidD_GetFeature(self.h, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) }
        }

        /// Waits for an overlapped operation; on timeout cancels it and waits for the cancel so
        /// the buffer is no longer in use when this returns.
        fn finish(&self, ov: &mut OVERLAPPED, started: i32, timeout_ms: u32) -> Option<usize> {
            unsafe {
                if started == 0 && GetLastError() != ERROR_IO_PENDING {
                    return None;
                }
                let mut n = 0u32;
                if WaitForSingleObject(self.event, timeout_ms) != WAIT_OBJECT_0 {
                    CancelIo(self.h);
                    GetOverlappedResult(self.h, ov, &mut n, 1);
                    return None;
                }
                if GetOverlappedResult(self.h, ov, &mut n, 0) == 0 {
                    return None;
                }
                Some(n as usize)
            }
        }

        /// Sends an output report (byte 0 = report ID).
        pub fn write(&self, buf: &[u8], timeout_ms: u32) -> bool {
            unsafe {
                let mut ov: OVERLAPPED = zeroed();
                ov.hEvent = self.event;
                windows_sys::Win32::System::Threading::ResetEvent(self.event);
                let started = WriteFile(self.h, buf.as_ptr(), buf.len() as u32, null_mut(), &mut ov);
                self.finish(&mut ov, started, timeout_ms).is_some()
            }
        }

        /// Reads one input report (byte 0 = report ID). Returns its length, or `None` on timeout.
        pub fn read(&self, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
            unsafe {
                let mut ov: OVERLAPPED = zeroed();
                ov.hEvent = self.event;
                windows_sys::Win32::System::Threading::ResetEvent(self.event);
                let started = ReadFile(self.h, buf.as_mut_ptr(), buf.len() as u32, null_mut(), &mut ov);
                self.finish(&mut ov, started, timeout_ms)
            }
        }
    }

    fn hid_string(h: HANDLE, f: unsafe extern "system" fn(HANDLE, *mut c_void, u32) -> bool) -> String {
        let mut buf = [0u16; 128];
        let ok = unsafe { f(h, buf.as_mut_ptr() as *mut c_void, (buf.len() * 2) as u32) };
        if !ok {
            return String::new();
        }
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }

    /// Every HID collection on the system; `filter` sees vendor and product id first.
    pub fn enumerate(filter: &dyn Fn(u16, u16) -> bool) -> Vec<Collection> {
        let mut out = Vec::new();
        unsafe {
            let mut guid: GUID = zeroed();
            HidD_GetHidGuid(&mut guid);
            let set = SetupDiGetClassDevsW(&guid, null(), null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
            if set as isize == -1 {
                return out;
            }
            let mut index = 0u32;
            loop {
                let mut iface: SP_DEVICE_INTERFACE_DATA = zeroed();
                iface.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
                if SetupDiEnumDeviceInterfaces(set, null(), &guid, index, &mut iface) == 0 {
                    break;
                }
                index += 1;
                let mut needed = 0u32;
                SetupDiGetDeviceInterfaceDetailW(set, &iface, null_mut(), 0, &mut needed, null_mut());
                if needed == 0 {
                    continue;
                }
                // u64 storage keeps the structure aligned.
                let mut storage = vec![0u64; (needed as usize).div_ceil(8) + 1];
                let detail = storage.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
                (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(set, &iface, detail, needed, null_mut(), null_mut()) == 0 {
                    continue;
                }
                let path_ptr = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
                let mut len = 0usize;
                while *path_ptr.add(len) != 0 {
                    len += 1;
                }
                let path = String::from_utf16_lossy(std::slice::from_raw_parts(path_ptr, len));

                let Some(h) = open_raw(&path, 0, 0) else { continue };
                let h = Handle { h, event: null_mut() };
                let mut attrs: HIDD_ATTRIBUTES = zeroed();
                attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
                if !HidD_GetAttributes(h.h, &mut attrs) || !filter(attrs.VendorID, attrs.ProductID) {
                    continue;
                }
                let mut pre = zeroed();
                if !HidD_GetPreparsedData(h.h, &mut pre) {
                    continue;
                }
                let mut caps: HIDP_CAPS = zeroed();
                let ok = HidP_GetCaps(pre, &mut caps) == HIDP_STATUS_SUCCESS;
                HidD_FreePreparsedData(pre);
                if ok {
                    out.push(Collection {
                        path,
                        vendor_id: attrs.VendorID,
                        product_id: attrs.ProductID,
                        usage_page: caps.UsagePage,
                        usage: caps.Usage,
                        input_len: caps.InputReportByteLength,
                        output_len: caps.OutputReportByteLength,
                        feature_len: caps.FeatureReportByteLength,
                        manufacturer: hid_string(h.h, HidD_GetManufacturerString),
                        product: hid_string(h.h, HidD_GetProductString),
                        serial: hid_string(h.h, HidD_GetSerialNumberString),
                    });
                }
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        out
    }

    /// Nothing to set up on Windows: HID devices are open to the signed-in person.
    pub fn permission_problem(_c: &Collection) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------------------------
// Linux

#[cfg(target_os = "linux")]
mod imp {
    use super::{parse_descriptor, Collection};
    use std::fs;
    use std::os::fd::AsRawFd;
    use std::path::{Path, PathBuf};

    pub struct Handle {
        file: fs::File,
        /// The device numbers its reports, so reads already start with the report ID.
        numbered: bool,
    }

    // _IOC(_IOC_READ | _IOC_WRITE, 'H', nr, len) on Linux's generic ioctl layout.
    fn ioc_rw(nr: u64, len: usize) -> u64 {
        (3u64 << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | nr
    }

    fn sys_dir(dev_path: &str) -> Option<PathBuf> {
        let name = Path::new(dev_path).file_name()?.to_str()?;
        Some(Path::new("/sys/class/hidraw").join(name).join("device"))
    }

    fn read_trim(p: &Path) -> String {
        fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
    }

    fn numbered(dev_path: &str) -> bool {
        sys_dir(dev_path)
            .and_then(|d| fs::read(d.join("report_descriptor")).ok())
            .map(|d| parse_descriptor(&d).1)
            .unwrap_or(false)
    }

    fn open_file(path: &str) -> std::io::Result<fs::File> {
        fs::OpenOptions::new().read(true).write(true).open(path)
    }

    /// Open for feature reports.
    pub fn open(path: &str) -> Option<Handle> {
        let file = open_file(path).ok()?;
        Some(Handle { file, numbered: numbered(path) })
    }

    /// Open for input and output reports (and feature reports).
    pub fn open_io(path: &str) -> Result<Handle, String> {
        match open_file(path) {
            Ok(file) => Ok(Handle { file, numbered: numbered(path) }),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(
                "No permission to use this device yet. Click \"Allow access\" (it adds a udev rule), then unplug and replug it."
                    .into(),
            ),
            Err(_) => Err("Couldn't open the device (is another app using it?).".into()),
        }
    }

    impl Handle {
        pub fn set_feature(&self, buf: &[u8]) -> bool {
            let r = unsafe { libc::ioctl(self.file.as_raw_fd(), ioc_rw(0x06, buf.len()) as _, buf.as_ptr()) };
            r >= 0
        }
        pub fn get_feature(&self, buf: &mut [u8]) -> bool {
            let r = unsafe { libc::ioctl(self.file.as_raw_fd(), ioc_rw(0x07, buf.len()) as _, buf.as_mut_ptr()) };
            r >= 0
        }

        /// Sends an output report (byte 0 = report ID, 0 if unnumbered: hidraw uses the same rule).
        pub fn write(&self, buf: &[u8], _timeout_ms: u32) -> bool {
            let r = unsafe { libc::write(self.file.as_raw_fd(), buf.as_ptr() as *const libc::c_void, buf.len()) };
            r == buf.len() as isize
        }

        /// Reads one input report (byte 0 = report ID). Returns its length, or `None` on timeout.
        pub fn read(&self, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
            let mut pfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            let r = unsafe { libc::poll(&mut pfd, 1, timeout_ms as i32) };
            if r <= 0 || pfd.revents & libc::POLLIN == 0 {
                return None;
            }
            let (dst, extra) = if self.numbered { (&mut buf[..], 0) } else { (&mut buf[1..], 1) };
            let n = unsafe { libc::read(self.file.as_raw_fd(), dst.as_mut_ptr() as *mut libc::c_void, dst.len()) };
            if n < 0 {
                return None;
            }
            if !self.numbered {
                buf[0] = 0;
            }
            Some(n as usize + extra)
        }
    }

    /// Every HID collection on the system; `filter` sees vendor and product id first.
    pub fn enumerate(filter: &dyn Fn(u16, u16) -> bool) -> Vec<Collection> {
        let mut out = Vec::new();
        let Ok(dir) = fs::read_dir("/sys/class/hidraw") else { return out };
        let mut nodes: Vec<_> = dir.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        nodes.sort_by_key(|n| n.trim_start_matches("hidraw").parse::<u32>().unwrap_or(0));
        for node in nodes {
            let dev = Path::new("/sys/class/hidraw").join(&node).join("device");
            let uevent = read_trim(&dev.join("uevent"));
            let mut vid = 0u16;
            let mut pid = 0u16;
            let mut name = String::new();
            let mut uniq = String::new();
            for line in uevent.lines() {
                if let Some(v) = line.strip_prefix("HID_ID=") {
                    // bus:vendor:product, each in hex
                    let parts: Vec<&str> = v.split(':').collect();
                    if parts.len() == 3 {
                        vid = u32::from_str_radix(parts[1], 16).unwrap_or(0) as u16;
                        pid = u32::from_str_radix(parts[2], 16).unwrap_or(0) as u16;
                    }
                } else if let Some(v) = line.strip_prefix("HID_NAME=") {
                    name = v.to_string();
                } else if let Some(v) = line.strip_prefix("HID_UNIQ=") {
                    uniq = v.to_string();
                }
            }
            if !filter(vid, pid) {
                continue;
            }
            // For USB, the USB device two levels up has separate maker and product strings.
            let real = fs::canonicalize(&dev).unwrap_or(dev.clone());
            let usb = real.parent().and_then(|p| p.parent()).map(Path::to_path_buf);
            let (mut manufacturer, mut product) = (String::new(), String::new());
            if let Some(u) = &usb {
                manufacturer = read_trim(&u.join("manufacturer"));
                product = read_trim(&u.join("product"));
                if uniq.is_empty() {
                    uniq = read_trim(&u.join("serial"));
                }
            }
            if product.is_empty() {
                product = name.clone();
            }
            let Ok(desc) = fs::read(dev.join("report_descriptor")) else { continue };
            let (cols, _) = parse_descriptor(&desc);
            for c in cols {
                out.push(Collection {
                    path: format!("/dev/{node}"),
                    vendor_id: vid,
                    product_id: pid,
                    usage_page: c.usage_page,
                    usage: c.usage,
                    input_len: c.input_len,
                    output_len: c.output_len,
                    feature_len: c.feature_len,
                    manufacturer: manufacturer.clone(),
                    product: product.clone(),
                    serial: uniq.clone(),
                });
            }
        }
        out
    }

    /// The device node exists but this user may not open it (no udev rule yet).
    pub fn permission_problem(c: &Collection) -> bool {
        matches!(open_file(&c.path), Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied)
    }
}

// ---------------------------------------------------------------------------------------------
// Anything else: no devices.

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::Collection;
    pub struct Handle;
    pub fn open(_path: &str) -> Option<Handle> {
        None
    }
    pub fn open_io(_path: &str) -> Result<Handle, String> {
        Err("This system isn't supported yet.".into())
    }
    impl Handle {
        pub fn set_feature(&self, _buf: &[u8]) -> bool {
            false
        }
        pub fn get_feature(&self, _buf: &mut [u8]) -> bool {
            false
        }
        pub fn write(&self, _buf: &[u8], _timeout_ms: u32) -> bool {
            false
        }
        pub fn read(&self, _buf: &mut [u8], _timeout_ms: u32) -> Option<usize> {
            None
        }
    }
    pub fn enumerate(_filter: &dyn Fn(u16, u16) -> bool) -> Vec<Collection> {
        Vec::new()
    }
    pub fn permission_problem(_c: &Collection) -> bool {
        false
    }
}

pub use imp::{enumerate, open, open_io, permission_problem, Handle};

/// Every HID collection of the device with this vendor and product id.
pub fn collections(vid: u16, pid: u16) -> Vec<Collection> {
    enumerate(&|v, p| v == vid && p == pid)
}

// ---------------------------------------------------------------------------------------------
// Linux permissions: a udev rule that lets the signed-in person use a device's hidraw nodes.

/// Where the rule file goes.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const UDEV_RULES: &str = "/etc/udev/rules.d/60-openperiph.rules";

/// The rule line for one device.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn udev_rule(vid: u16, pid: u16) -> String {
    format!(
        "KERNEL==\"hidraw*\", ATTRS{{idVendor}}==\"{vid:04x}\", ATTRS{{idProduct}}==\"{pid:04x}\", MODE=\"0660\", TAG+=\"uaccess\""
    )
}

/// Adds rules for these devices (keeping existing ones) through pkexec, which asks for the
/// password in a system window, then reloads udev. Linux only.
#[cfg(target_os = "linux")]
pub fn install_udev_rules(ids: &[(u16, u16)]) -> Result<(), String> {
    let existing = std::fs::read_to_string(UDEV_RULES).unwrap_or_default();
    let mut text = if existing.is_empty() {
        "# OpenPeriph: let the signed-in user talk to these devices' HID interfaces\n".to_string()
    } else {
        existing.clone()
    };
    for &(v, p) in ids {
        let line = udev_rule(v, p);
        if !existing.contains(&line) {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&line);
            text.push('\n');
        }
    }
    if text == existing {
        return Ok(());
    }
    let tmp = std::env::temp_dir().join("openperiph-udev.rules");
    std::fs::write(&tmp, &text).map_err(|e| e.to_string())?;
    let script = format!(
        "install -m 0644 '{}' '{}' && udevadm control --reload && udevadm trigger --subsystem-match=hidraw",
        tmp.display(),
        UDEV_RULES
    );
    let status = std::process::Command::new("pkexec")
        .args(["sh", "-c", &script])
        .status()
        .map_err(|_| "pkexec isn't available; add the rule by hand (see the README).".to_string())?;
    let _ = std::fs::remove_file(&tmp);
    if status.success() {
        Ok(())
    } else {
        Err("Access wasn't granted (the password window was closed or refused).".into())
    }
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub fn install_udev_rules(_ids: &[(u16, u16)]) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read-only: lists the CIY X78's HID collections. Run with
    /// `cargo test list_keyboard_collections -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn list_keyboard_collections() {
        for c in collections(0x258A, 0x0016) {
            println!("usage page {:04X} usage {:04X} feature length {}", c.usage_page, c.usage, c.feature_len);
        }
    }

    /// QMK's raw HID interface: vendor page FF60, usage 61, 32-byte in and out, no report IDs.
    #[test]
    fn parses_qmk_raw_hid_descriptor() {
        let d = [
            0x06, 0x60, 0xFF, 0x09, 0x61, 0xA1, 0x01, // usage page FF60, usage 61, application
            0x09, 0x62, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x20, 0x75, 0x08, 0x81, 0x02, // input 32x8
            0x09, 0x63, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 0x20, 0x75, 0x08, 0x91, 0x02, // output 32x8
            0xC0,
        ];
        let (cols, numbered) = parse_descriptor(&d);
        assert!(!numbered);
        assert_eq!(
            cols,
            vec![DescCollection { usage_page: 0xFF60, usage: 0x61, input_len: 33, output_len: 33, feature_len: 0 }]
        );
    }

    /// Two top-level collections in one interface with report IDs (keyboard + consumer control).
    #[test]
    fn parses_shared_descriptor_with_report_ids() {
        let d = [
            0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, // keyboard, report 1
            0x75, 0x08, 0x95, 0x08, 0x81, 0x00, // 8 bytes in
            0x75, 0x01, 0x95, 0x05, 0x91, 0x02, 0x95, 0x03, 0x91, 0x01, // 1 byte out (LEDs)
            0xC0, //
            0x05, 0x0C, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x03, // consumer, report 3
            0x75, 0x10, 0x95, 0x01, 0x81, 0x00, // 2 bytes in
            0x85, 0x04, 0x75, 0x08, 0x95, 0x3F, 0xB1, 0x02, // feature report 4, 63 bytes
            0xC0,
        ];
        let (cols, numbered) = parse_descriptor(&d);
        assert!(numbered);
        assert_eq!(cols.len(), 2);
        assert_eq!((cols[0].usage_page, cols[0].usage, cols[0].input_len, cols[0].output_len), (1, 6, 9, 2));
        assert_eq!((cols[1].usage_page, cols[1].usage, cols[1].input_len, cols[1].feature_len), (0x0C, 1, 3, 64));
    }
}
