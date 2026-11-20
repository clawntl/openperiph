//! Talking to USB HID devices through Windows' own HID functions (setupapi + hid.dll),
//! the same route the original program's helper library takes. Shared by every driver.

#[derive(Clone, Debug)]
pub struct Collection {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub path: Vec<u16>,
    pub usage_page: u16,
    pub usage: u16,
    pub feature_len: u16,
}

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
        HidD_GetPreparsedData, HidD_SetFeature, HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;

    pub struct Handle(HANDLE);

    // The handle is only used from one thread at a time.
    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn open_path(path: &[u16], access: u32) -> Option<Handle> {
        let h = unsafe {
            CreateFileW(
                path.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE || h.is_null() {
            None
        } else {
            Some(Handle(h))
        }
    }

    pub fn open(path: &[u16]) -> Option<Handle> {
        open_path(path, GENERIC_READ | GENERIC_WRITE).or_else(|| open_path(path, 0))
    }

    impl Handle {
        pub fn set_feature(&self, buf: &[u8]) -> bool {
            unsafe { HidD_SetFeature(self.0, buf.as_ptr() as *const c_void, buf.len() as u32) }
        }
        pub fn get_feature(&self, buf: &mut [u8]) -> bool {
            unsafe { HidD_GetFeature(self.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) }
        }
    }

    /// Every HID collection of the device with this vendor and product id.
    pub fn collections(vid: u16, pid: u16) -> Vec<Collection> {
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
                let mut path: Vec<u16> = std::slice::from_raw_parts(path_ptr, len).to_vec();
                path.push(0);

                let Some(h) = open_path(&path, 0) else { continue };
                let mut attrs: HIDD_ATTRIBUTES = zeroed();
                attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
                if !HidD_GetAttributes(h.0, &mut attrs) || attrs.VendorID != vid || attrs.ProductID != pid {
                    continue;
                }
                let mut pre = zeroed();
                if !HidD_GetPreparsedData(h.0, &mut pre) {
                    continue;
                }
                let mut caps: HIDP_CAPS = zeroed();
                let ok = HidP_GetCaps(pre, &mut caps) == HIDP_STATUS_SUCCESS;
                HidD_FreePreparsedData(pre);
                if ok {
                    out.push(Collection {
                        path,
                        usage_page: caps.UsagePage,
                        usage: caps.Usage,
                        feature_len: caps.FeatureReportByteLength,
                    });
                }
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        out
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Collection;
    pub struct Handle;
    pub fn open(_path: &[u16]) -> Option<Handle> {
        None
    }
    impl Handle {
        pub fn set_feature(&self, _buf: &[u8]) -> bool {
            false
        }
        pub fn get_feature(&self, _buf: &mut [u8]) -> bool {
            false
        }
    }
    pub fn collections(_vid: u16, _pid: u16) -> Vec<Collection> {
        Vec::new()
    }
}

pub use imp::{collections, open, Handle};

#[cfg(test)]
mod tests {
    /// Read-only: lists the CIY X78's HID collections. Run with
    /// `cargo test list_keyboard_collections -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn list_keyboard_collections() {
        for c in super::collections(0x258A, 0x0016) {
            println!("usage page {:04X} usage {:04X} feature length {}", c.usage_page, c.usage, c.feature_len);
        }
    }
}
