//! Any keyboard running QMK with VIA. Unlike the other drivers this one is created per keyboard
//! found (plus one for each keyboard with a bundled definition, so it is listed while unplugged);
//! the QMK pages in the window do the rest through `crate::qmk`.

use std::sync::{Arc, RwLock};

use crate::device::{Capabilities, DeviceInfo, DeviceKind, Driver};
use crate::hid;
use crate::qmk::{self, definition::Definition};

pub struct QmkDriver {
    info: DeviceInfo,
    pub serial: String,
    pub definition: RwLock<Option<Arc<Definition>>>,
    /// Simulated keyboard (`--demo`): always connected.
    pub demo: bool,
}

impl QmkDriver {
    pub fn new(vendor_id: u16, product_id: u16, serial: String, name: String, maker: String, definition: Option<Definition>) -> QmkDriver {
        let name = definition.as_ref().map(|d| d.name.clone()).unwrap_or(name);
        let id = if serial.is_empty() {
            format!("qmk-{vendor_id:04x}-{product_id:04x}")
        } else {
            format!("qmk-{vendor_id:04x}-{product_id:04x}-{}", serial.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>())
        };
        QmkDriver {
            info: DeviceInfo { id, name, maker, kind: DeviceKind::Keyboard, vendor_id, product_id },
            serial,
            definition: RwLock::new(definition.map(Arc::new)),
            demo: false,
        }
    }

    /// The firmware it runs: "QMK", or "SMK" for keyboards we know run that.
    pub fn firmware_name(&self) -> &'static str {
        qmk::firmware::firmware_name(self.info.vendor_id, self.info.product_id)
    }

    pub fn definition(&self) -> Option<Arc<Definition>> {
        self.definition.read().ok().and_then(|d| d.clone())
    }

    pub fn set_definition(&self, d: Definition) {
        if let Ok(mut slot) = self.definition.write() {
            *slot = Some(Arc::new(d));
        }
    }

    /// The keyboard's raw HID interface, if plugged in.
    pub fn find(&self) -> Option<hid::Collection> {
        let (v, p) = (self.info.vendor_id, self.info.product_id);
        hid::enumerate(&|vv, pp| vv == v && pp == p)
            .into_iter()
            .find(|c| c.usage_page == qmk::via::USAGE_PAGE && c.usage == qmk::via::USAGE && (self.serial.is_empty() || c.serial == self.serial))
    }

    /// Same keyboard as this interface?
    pub fn matches(&self, c: &hid::Collection) -> bool {
        (c.vendor_id, c.product_id) == (self.info.vendor_id, self.info.product_id) && (self.serial.is_empty() || c.serial == self.serial)
    }
}

impl Driver for QmkDriver {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { qmk: true, ..Default::default() }
    }

    fn detect(&self) -> bool {
        self.demo || self.find().is_some()
    }

    fn as_qmk(&self) -> Option<&QmkDriver> {
        Some(self)
    }
}
