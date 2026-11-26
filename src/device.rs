//! The generic side of the app: what any supported peripheral looks like to the interface.
//! Each brand or model is a `Driver` in `drivers/`; the window only talks to this trait, so new
//! hardware is added by writing one more driver and listing it in `drivers::all()`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::keycodes::KeyAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    #[allow(dead_code)]
    Headset,
}

impl DeviceKind {
    pub fn name(self) -> &'static str {
        match self {
            DeviceKind::Keyboard => "Keyboard",
            DeviceKind::Mouse => "Mouse",
            DeviceKind::Headset => "Headset",
        }
    }
}

/// Who made the device and how to recognise it.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    /// Stable id, used for the saved-settings file name.
    pub id: String,
    pub name: String,
    pub maker: String,
    pub kind: DeviceKind,
    pub vendor_id: u16,
    pub product_id: u16,
}

/// What the settings pages may show for a device.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub key_remap: bool,
    pub fn_layer: bool,
    pub lighting: bool,
    /// DPI, polling rate, sensor and power settings.
    pub mouse: bool,
    /// A QMK keyboard with VIA: keymap, macros, lighting menus and settings pages of its own.
    pub qmk: bool,
}

// ---------------------------------------------------------------------------------------------
// Mice

/// What a mouse offers.
#[derive(Clone, Debug)]
pub struct MouseSpec {
    /// Onboard profiles, numbered from 1.
    pub profiles: u8,
    pub dpi_min: u32,
    pub dpi_max: u32,
    pub dpi_step: u32,
    pub max_stages: usize,
    /// Colour shown for each DPI stage.
    pub stage_colours: Vec<[u8; 3]>,
    /// Polling rates in Hz, on the current connection.
    pub poll_rates: Vec<u32>,
    /// Lift-off distances: device code and label.
    pub lift_off: Vec<(u8, &'static str)>,
    pub debounce_max: u8,
    /// Sleep timer choices: seconds and label.
    pub sleep_options: Vec<(u32, &'static str)>,
    pub angle_snap: bool,
}

/// The settings of one onboard profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseState {
    pub profile: u8,
    pub dpi_stages: Vec<u32>,
    /// 0-based.
    pub active_stage: usize,
    pub poll_hz: u32,
    pub lift_off: u8,
    pub motion_sync: bool,
    pub ripple: bool,
    pub angle_snap: bool,
    pub debounce_ms: u8,
    pub sleep_secs: u32,
}

/// Live facts about a mouse that aren't settings.
#[derive(Clone, Debug, Default)]
pub struct MouseStatus {
    pub wired: bool,
    pub battery: Option<u8>,
    pub charging: bool,
    pub firmware: Option<String>,
}

/// One key as drawn on screen.
#[derive(Clone, Debug)]
pub struct LayoutKey {
    pub id: u8,
    pub label: &'static str,
    /// x1, y1, x2, y2 in the layout's own units.
    pub rect: [f32; 4],
    /// Can't be changed (for example the Fn key).
    pub locked: bool,
}

#[derive(Clone, Debug, Default)]
pub struct KeyboardLayout {
    pub keys: Vec<LayoutKey>,
    pub width: f32,
    pub height: f32,
}

/// One lighting effect and which settings it takes.
#[derive(Clone, Copy, Debug)]
pub struct EffectInfo {
    pub name: &'static str,
    pub speed: bool,
    pub brightness: bool,
    pub direction: bool,
    pub colour: bool,
    pub colourful: bool,
    /// Number of stored colour groups the effect switches between (0 = none).
    pub groups: u8,
    /// Turns the lights off.
    pub off: bool,
}

/// Settings of one effect. Levels run from 1 to the driver's maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectSettings {
    /// Basic colour number, or `None` for the colourful (rainbow) version.
    pub colour: Option<u8>,
    pub speed: u8,
    pub brightness: u8,
    /// 0 or 1.
    pub direction: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingState {
    pub effect: usize,
    pub per_effect: Vec<EffectSettings>,
    /// Colour group for effects with groups.
    pub group: u8,
}

/// Remapped keys by key id; keys not listed keep their default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySettings {
    pub normal: BTreeMap<u8, KeyAction>,
    pub fn_layer: BTreeMap<u8, KeyAction>,
}

/// A named colour the device can show.
#[derive(Clone, Copy, Debug)]
pub struct NamedColour {
    pub name: &'static str,
    pub rgb: [u8; 3],
}

/// What a step of applying settings reports back.
pub type Progress<'a> = &'a dyn Fn(&str);

static EMPTY_LAYOUT: KeyboardLayout = KeyboardLayout { keys: Vec::new(), width: 1.0, height: 1.0 };

/// Every device driver. Keyboard and mouse parts have empty defaults, so a driver only
/// implements what its hardware has (announced through `capabilities`).
pub trait Driver: Send + Sync {
    fn info(&self) -> &DeviceInfo;
    fn capabilities(&self) -> Capabilities;
    /// Whether the device is plugged in.
    fn detect(&self) -> bool;
    /// USB vendor and product ids the device may show up as (for Linux permissions).
    fn usb_ids(&self) -> Vec<(u16, u16)> {
        vec![(self.info().vendor_id, self.info().product_id)]
    }
    /// The QMK side of the device, for QMK keyboards.
    fn as_qmk(&self) -> Option<&crate::drivers::qmk::QmkDriver> {
        None
    }

    // — keyboards ---------------------------------------------------------------------------
    fn layout(&self) -> &KeyboardLayout {
        &EMPTY_LAYOUT
    }
    fn effects(&self) -> &[EffectInfo] {
        &[]
    }
    fn colours(&self) -> &[NamedColour] {
        &[]
    }
    fn max_speed(&self) -> u8 {
        1
    }
    fn max_brightness(&self) -> u8 {
        1
    }
    /// Label for each brightness level, lowest first.
    fn brightness_labels(&self) -> Vec<String> {
        Vec::new()
    }
    fn default_lighting(&self) -> LightingState {
        LightingState { effect: 0, per_effect: Vec::new(), group: 0 }
    }
    /// What a key does out of the box on the given layer, in words.
    fn default_description(&self, _key_id: u8, _fn_layer: bool) -> String {
        String::new()
    }
    /// Read the lighting settings stored on the device.
    fn read_lighting(&self) -> Result<LightingState, String> {
        Err("This device has no lighting settings.".into())
    }
    /// Send settings to the device. `None` leaves that part as it is.
    fn apply(
        &self,
        _keys: Option<&KeySettings>,
        _lighting: Option<&LightingState>,
        _progress: Progress,
    ) -> Result<(), String> {
        Err("This device has no key or lighting settings.".into())
    }

    // — mice --------------------------------------------------------------------------------
    /// What the mouse offers on its current connection (cable or receiver).
    fn mouse_spec(&self, _wired: bool) -> Option<MouseSpec> {
        None
    }
    fn default_mouse(&self) -> Option<MouseState> {
        None
    }
    /// Read one onboard profile (the active one when `None`).
    fn read_mouse(&self, _profile: Option<u8>) -> Result<(MouseState, MouseStatus), String> {
        Err("This device has no mouse settings.".into())
    }
    /// Write a profile's settings and make it the active one.
    fn apply_mouse(&self, _state: &MouseState, _progress: Progress) -> Result<(), String> {
        Err("This device has no mouse settings.".into())
    }
}
