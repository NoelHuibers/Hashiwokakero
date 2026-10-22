//! The app on the desktop, for development (`cargo tauri dev`). The phones start `run` of the
//! library instead.

// No console window next to the app on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    hashi_tauri_lib::run();
}
