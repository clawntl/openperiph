#![cfg_attr(not(test), windows_subsystem = "windows")]

//! OpenPeriph: a simple settings app for PC devices, for Windows and Linux. Supported: any QMK
//! keyboard with VIA (keymap, macros, lighting), the CIY X78 keyboard (key remapping and
//! lighting) and the Attack Shark R5 ULTRA mouse (DPI, polling, sensor, power).
//! See docs/protocols/ for how the app talks to each.

mod app;
mod device;
mod drivers;
mod hid;
mod keycodes;
mod net;
mod qmk;
mod selfupdate;
mod store;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut screenshot: Option<PathBuf> = None;
    // Hidden option for checks: --page lighting | mouse | macros | settings
    let mut page: Option<String> = None;
    // --demo adds a simulated QMK keyboard (an SK620) to try the QMK pages without one.
    let mut demo = false;
    while let Some(a) = args.next() {
        if a == "--screenshot" {
            screenshot = args.next().map(PathBuf::from);
        } else if a == "--page" {
            page = args.next();
        } else if a == "--demo" {
            demo = true;
        } else if a == "--help" || a == "-h" {
            println!("openperiph [--demo] [--screenshot FILE.png [--page home|keys|lighting|colour|macros|settings|mouse|x78|about|fw-update|app-update|flashing]]");
            return Ok(());
        }
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("OpenPeriph")
        .with_inner_size([1240.0, 780.0])
        .with_min_inner_size([980.0, 640.0])
        // matches openperiph.desktop, so Linux desktops show the menu entry's icon
        .with_app_id("openperiph");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/openperiph.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    if screenshot.is_some() {
        // Open quietly behind other windows so a check never gets in the way.
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("OpenPeriph", options, Box::new(move |cc| Ok(Box::new(app::OpenPeriphApp::new(cc, screenshot, page, demo)))))
}
