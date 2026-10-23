//! Tauri's build step: reads `tauri.conf.json` and the capabilities, and generates the
//! permission schemas in `gen/schemas`.
//!
//! With the app's commands listed here, Tauri generates a permission `allow-<command>` for each
//! of them, and a command may only be called by a window whose capability grants it
//! (`capabilities/default.json`). Without the list, every window could call every command. The
//! same for the commands of the app's own plugin `hashi` (`hashi:allow-<command>`), which
//! `HashiPlugin.kt` answers on Android.

fn main() {
    let commands = tauri_build::AppManifest::new().commands(&[
        "generate_puzzle",
        "solve_puzzle",
        "puzzle_cnf",
    ]);
    let hashi = tauri_build::InlinedPlugin::new().commands(&["share", "system_bars"]);
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(commands)
            .plugin("hashi", hashi),
    )
    .expect("Tauri's build step failed");
}
