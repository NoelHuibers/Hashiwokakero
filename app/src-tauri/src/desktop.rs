//! The desktop side of the app's plugin `hashi` (Windows, macOS, Linux), where the phones have
//! `HashiPlugin.kt` and `HashiPlugin.swift`: the two commands of it that make sense on a
//! computer, answered in Rust.
//!
//! - `share` with a file (the CNF of a puzzle, the workshop's puzzle as text): the system's save
//!   dialog, then the file where the player chose. Text alone (a result) the page shares itself
//!   on the desktop, with the `WebView`'s share sheet or the clipboard (`components::share`), so
//!   it never comes here.
//! - `system_bars`: the window's title bar follows the page's theme, as the phones' system bars
//!   do.
//!
//! The games services (`games_*`) have no desktop side: the page doesn't call them there
//! (`api::native`), and Tauri would refuse them.

use std::path::Path;

use tauri::{AppHandle, Manager as _, Runtime, Theme, Window};
use tauri_plugin_dialog::DialogExt as _;

/// The script the page finds before its own: which desktop it runs on (`windows`, `macos` or
/// `linux`), read by `api::app` for `x-hashi-app` and the update's download page.
pub(crate) fn init_script() -> String {
    format!(
        "window.__HASHI_DESKTOP__ = {};",
        serde_json::Value::from(std::env::consts::OS)
    )
}

/// Saves `text` as the file `file` where the player chooses, through the system's save dialog
/// (starting in the Downloads folder, where a browser would put it); nothing when they cancel it.
/// Without a file there is nothing to do here (see the module).
#[tauri::command]
pub(crate) async fn share<R: Runtime>(
    app: AppHandle<R>,
    window: Window<R>,
    text: String,
    file: Option<String>,
) -> Result<(), String> {
    let Some(name) = file else {
        return Err("sharing text is the page's on the desktop".into());
    };
    let mut dialog = app.dialog().file().set_file_name(&name).set_parent(&window);
    if let Ok(downloads) = app.path().download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    if let Some(extension) = extension(&name) {
        dialog = dialog.add_filter(extension.to_uppercase(), &[extension]);
    }
    // The dialog waits for the player: on a blocking thread, not on the async runtime's
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .map_err(|e| e.to_string())?;
    let Some(path) = chosen else {
        return Ok(());
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// The extension of the file name `name` (`cnf` of `puzzle.cnf`), for the dialog's filter.
fn extension(name: &str) -> Option<&str> {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
}

/// The window's theme, and with it its title bar's, as the page's: dark or light.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its window by value"
)]
pub(crate) fn system_bars<R: Runtime>(window: Window<R>, dark: bool) -> Result<(), String> {
    let theme = if dark { Theme::Dark } else { Theme::Light };
    window.set_theme(Some(theme)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_takes_the_extension_of_the_file() {
        assert_eq!(extension("hashi-7x7.cnf"), Some("cnf"));
        assert_eq!(extension("puzzle.txt"), Some("txt"));
        assert_eq!(extension("puzzle"), None);
        assert_eq!(extension("puzzle."), None);
    }

    #[test]
    fn the_page_learns_which_desktop_it_runs_on() {
        let script = init_script();
        assert_eq!(
            script,
            format!("window.__HASHI_DESKTOP__ = \"{}\";", std::env::consts::OS)
        );
    }
}
