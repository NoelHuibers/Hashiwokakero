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
/// `linux`, or `msstore` for the Windows app from the Microsoft Store, `snap` for the Linux app
/// from the Snap Store), read by `api::app` for `x-hashi-app` and the update's download page.
pub(crate) fn init_script() -> String {
    let os = std::env::consts::OS;
    let package = package(
        os,
        std::env::current_exe().ok().as_deref(),
        std::env::var_os("SNAP").as_deref().map(Path::new),
    );
    format!(
        "window.__HASHI_DESKTOP__ = {};",
        serde_json::Value::from(desktop(os, package))
    )
}

/// What installed the app, where a store does: its updates come from there, not from the
/// GitHub releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Package {
    /// The installers of the GitHub releases (and anything else)
    None,
    /// The Microsoft Store's MSIX package
    Msix,
    /// The Snap Store's snap
    Snap,
}

/// The desktop as the page knows it: Rust's name of the system, except for the apps of a store.
fn desktop(os: &'static str, package: Package) -> &'static str {
    match package {
        Package::None => os,
        Package::Msix => "msstore",
        Package::Snap => "snap",
    }
}

/// What installed the program `exe` on the system `os`, given the environment's `SNAP`:
///
/// - **An MSIX package**, the Microsoft Store's (docs/stores.md, Microsoft Store): the package's
///   manifest, `AppxManifest.xml`, lies beside the program. Windows installs a package into a
///   folder of its own and runs the program from there; the NSIS and MSI installers put no
///   manifest there.
/// - **A snap**, the Snap Store's (docs/stores.md, Snap Store): snapd starts it with `SNAP`, the
///   snap's folder (`/snap/hashi/12`), and the program lies in it. `SNAP` alone isn't enough:
///   programs started from a classic snap (a terminal of the VS Code snap) inherit it, and would
///   take the `.deb`'s app for a snap.
fn package(os: &str, exe: Option<&Path>, snap: Option<&Path>) -> Package {
    let Some(exe) = exe else {
        return Package::None;
    };
    match os {
        "windows"
            if exe
                .parent()
                .is_some_and(|dir| dir.join("AppxManifest.xml").is_file()) =>
        {
            Package::Msix
        }
        "linux" if snap.is_some_and(|snap| snap.is_absolute() && exe.starts_with(snap)) => {
            Package::Snap
        }
        _ => Package::None,
    }
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
        // Not from a package: this test's program has no AppxManifest.xml beside it, and runs
        // from no snap
        let script = init_script();
        assert_eq!(
            script,
            format!("window.__HASHI_DESKTOP__ = \"{}\";", std::env::consts::OS)
        );
        assert_eq!(desktop("windows", Package::None), "windows");
        assert_eq!(desktop("windows", Package::Msix), "msstore");
        assert_eq!(desktop("linux", Package::None), "linux");
        assert_eq!(desktop("linux", Package::Snap), "snap");
        assert_eq!(desktop("macos", Package::None), "macos");
    }

    #[test]
    fn a_package_has_its_manifest_beside_the_program() {
        let dir = std::env::temp_dir().join(format!("hashi-msix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("hashi.exe");
        let msix = |exe: Option<&Path>| package("windows", exe, None);
        assert_eq!(msix(Some(&exe)), Package::None);
        std::fs::write(dir.join("AppxManifest.xml"), "<Package/>").unwrap();
        assert_eq!(msix(Some(&exe)), Package::Msix);
        // Only on Windows
        assert_eq!(package("linux", Some(&exe), None), Package::None);
        // A folder of that name is no manifest
        std::fs::remove_file(dir.join("AppxManifest.xml")).unwrap();
        std::fs::create_dir(dir.join("AppxManifest.xml")).unwrap();
        assert_eq!(msix(Some(&exe)), Package::None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(msix(None), Package::None);
        assert_eq!(msix(Some(Path::new("hashi.exe"))), Package::None);
    }

    #[test]
    fn a_snap_runs_its_program_from_the_snaps_folder() {
        let exe = Path::new("/snap/hashi/12/usr/bin/hashi");
        let snap = Path::new("/snap/hashi/12");
        assert_eq!(package("linux", Some(exe), Some(snap)), Package::Snap);
        // The .deb's app started from a classic snap's terminal, which passes its SNAP on
        let code = Path::new("/snap/code/215");
        assert_eq!(
            package("linux", Some(Path::new("/usr/bin/hashi")), Some(code)),
            Package::None
        );
        assert_eq!(package("linux", Some(exe), Some(code)), Package::None);
        // Without SNAP, an empty or a relative one, without a program, on another system
        assert_eq!(package("linux", Some(exe), None), Package::None);
        assert_eq!(
            package("linux", Some(exe), Some(Path::new(""))),
            Package::None
        );
        assert_eq!(
            package(
                "linux",
                Some(Path::new("snap/usr/bin/hashi")),
                Some(Path::new("snap"))
            ),
            Package::None
        );
        assert_eq!(package("linux", None, Some(snap)), Package::None);
        assert_eq!(package("macos", Some(exe), Some(snap)), Package::None);
        // A folder whose name starts like the snap's is another one
        assert_eq!(
            package(
                "linux",
                Some(Path::new("/snap/hashi/123/usr/bin/hashi")),
                Some(snap)
            ),
            Package::None
        );
    }
}
