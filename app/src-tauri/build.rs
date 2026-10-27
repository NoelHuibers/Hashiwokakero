//! Tauri's build step: reads `tauri.conf.json` and the capabilities, and generates the
//! permission schemas in `gen/schemas`.
//!
//! With the app's commands listed here, Tauri generates a permission `allow-<command>` for each
//! of them, and a command may only be called by a window whose capability grants it
//! (`capabilities/default.json`). Without the list, every window could call every command. The
//! same for the commands of the app's own plugin `hashi` (`hashi:allow-<command>`), which
//! `HashiPlugin.kt` answers on Android and `ios/` on iOS, a Swift package this step builds.

fn main() {
    let commands = tauri_build::AppManifest::new().commands(&[
        "generate_puzzle",
        "solve_puzzle",
        "puzzle_cnf",
    ]);
    let hashi = tauri_build::InlinedPlugin::new().commands(&[
        "share",
        "system_bars",
        "games_sign_in",
        "games_report",
        "games_show",
    ]);
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(commands)
            .plugin("hashi", hashi),
    )
    .expect("Tauri's build step failed");

    // Builds for iOS run on a Mac only
    #[cfg(target_os = "macos")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        ios::link_plugin();
    }
}

/// The iOS side of the plugin `hashi`, linked into the app's library the way `tauri-plugin`
/// links a plugin crate's (`Builder::ios_path`), which an app's own plugin can't use.
#[cfg(target_os = "macos")]
mod ios {
    use std::fs;
    use std::path::Path;

    /// Builds the Swift package `ios/` with `swift build` and links it. Its classes import Tauri's
    /// Swift API, the package of the `tauri` crate (`DEP_TAURI_IOS_LIBRARY_PATH`), copied to
    /// `.tauri/tauri-api` where `ios/Package.swift` looks for it. For the iOS of
    /// `IPHONEOS_DEPLOYMENT_TARGET`: Xcode's, or `.cargo/config.toml`'s outside Xcode.
    pub(super) fn link_plugin() {
        let api = std::env::var("DEP_TAURI_IOS_LIBRARY_PATH")
            .expect("the tauri crate names its Swift API");
        let copy = Path::new(".tauri/tauri-api");
        let _ = fs::remove_dir_all(copy);
        copy_dir(Path::new(&api), copy).expect("copying Tauri's Swift API failed");
        println!("cargo:rerun-if-env-changed=DEP_TAURI_IOS_LIBRARY_PATH");
        println!("cargo:rerun-if-env-changed=IPHONEOS_DEPLOYMENT_TARGET");
        println!("cargo:rerun-if-changed=ios/Package.swift");
        println!("cargo:rerun-if-changed=ios/Sources");
        tauri_utils::build::link_apple_library("hashi-plugin", "ios");
    }

    /// Copies the folder `from` to `to`, without the Swift build's leftovers and the tests.
    fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let name = entry.file_name();
            if [".build", "Package.resolved", "Tests"]
                .iter()
                .any(|n| name == *n)
            {
                continue;
            }
            if entry.file_type()?.is_dir() {
                copy_dir(&entry.path(), &to.join(&name))?;
            } else {
                fs::copy(entry.path(), to.join(&name))?;
            }
        }
        Ok(())
    }
}
