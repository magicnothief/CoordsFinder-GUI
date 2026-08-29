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

/// This fork's version, from `coordsfinder-gui`'s own manifest.
const GUI_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result {
    // An optional config path, so the GUI can be the "open with" target for
    // .conf files and can be launched from a shell the same way as the CLI.
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    // Both versions in the title: the GUI has its own release line, and the
    // engine tracks the upstream release it was built from. A bug report can
    // then name both without the reporter having to dig for either.
    let title = format!(
        "CoordsFinder GUI {GUI_VERSION}  (engine {})",
        coordsfinder::VERSION
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 880.0])
            .with_min_inner_size([1000.0, 640.0])
            .with_app_id("coordsfinder-gui")
            .with_title(&title),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| Ok(Box::new(app::CoordsFinderApp::new(cc, initial)))),
    )
}
