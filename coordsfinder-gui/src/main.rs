//! Graphical front-end for CoordsFinder.
//!
//! The window drives the same library the command-line tool uses: configs are
//! parsed by `coordsfinder::config`, and scans run on the CPU or wgpu backends
//! from `coordsfinder::cpu` and `coordsfinder::gpu`.

// A release build is a windowed app, so it should not open a console. Debug
// builds keep the console so panics and logs stay visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod grid;
mod history;
mod model;
mod runner;
mod ui;

use std::path::PathBuf;

fn main() -> eframe::Result {
    // An optional config path, so the GUI can be the "open with" target for
    // .conf files and can be launched from a shell the same way as the CLI.
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 880.0])
            .with_min_inner_size([1000.0, 640.0])
            .with_app_id("coordsfinder-gui")
            .with_title("CoordsFinder"),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "CoordsFinder",
        options,
        Box::new(move |cc| Ok(Box::new(app::CoordsFinderApp::new(cc, initial)))),
    )
}
