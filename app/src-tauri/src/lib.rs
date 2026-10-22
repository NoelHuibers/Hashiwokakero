//! The mobile app: a Tauri window with the web bundle of `app/` (the UI rendered in the
//! browser, see `../README.md`). Android and iOS start it through `run`, the desktop (for
//! development only) through `main.rs`.

/// Starts the app: one window with `index.html` of the bundle (`tauri.conf.json`).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("the app failed to start");
}
