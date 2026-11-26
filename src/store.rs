//! Remembers each device's settings between runs (the keyboard can't report its key table back,
//! so the app keeps its own copy). Stored as JSON under %APPDATA%\OpenPeriph on Windows and
//! ~/.config/OpenPeriph on Linux, along with imported VIA keyboard definitions.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::device::{KeySettings, LightingState, MouseState};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Saved {
    #[serde(default)]
    pub keys: KeySettings,
    #[serde(default)]
    pub lighting: Option<LightingState>,
    #[serde(default)]
    pub mouse: Option<MouseState>,
}

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let dir = base.join("OpenPeriph");
    // The app was called "Peripherals" before; carry its saved settings over once.
    let old = base.join("Peripherals");
    if !dir.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &dir);
    }
    Some(dir)
}

pub fn load(device_id: &str) -> Saved {
    dir()
        .map(|d| d.join(format!("{device_id}.json")))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(device_id: &str, saved: &Saved) -> Result<(), String> {
    let d = dir().ok_or("No folder to save settings in.")?;
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{device_id}.json")), text).map_err(|e| e.to_string())
}

fn definitions_dir() -> Option<PathBuf> {
    dir().map(|d| d.join("definitions"))
}

/// Every imported VIA definition, as JSON text.
pub fn imported_definitions() -> Vec<String> {
    let Some(d) = definitions_dir() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(d) else { return Vec::new() };
    let mut files: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    files.sort();
    files.into_iter().filter_map(|p| std::fs::read_to_string(p).ok()).collect()
}

/// Keeps a copy of a VIA definition so the keyboard is recognised next time.
pub fn import_definition(text: &str, vendor_id: u16, product_id: u16) -> Result<(), String> {
    let d = definitions_dir().ok_or("No folder to save settings in.")?;
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{vendor_id:04x}_{product_id:04x}.json")), text).map_err(|e| e.to_string())
}
