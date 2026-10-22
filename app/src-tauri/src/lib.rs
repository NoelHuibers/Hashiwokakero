//! The mobile app: a Tauri window with the web bundle of `app/` (the UI rendered in the
//! browser, see `../README.md`). Android and iOS start it through `run`, the desktop (for
//! development only) through `main.rs`.
//!
//! The page calls the commands below for the offline practice instead of the server functions
//! of the same names (`src/api/practice.rs` in the repository root), with the server functions'
//! JSON as text both ways (see `hashi_offline::json`). Only the main window may call them
//! (`capabilities/default.json`, the permissions from `build.rs`).

/// Starts the app: one window with `index.html` of the bundle (`tauri.conf.json`).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            generate_puzzle,
            solve_puzzle,
            puzzle_cnf
        ])
        .run(tauri::generate_context!())
        .expect("the app failed to start");
}

// Async commands run on Tauri's async runtime, never on the main thread; the computations go to
// its blocking threads (`hashi_offline::Pool`)

/// The server function `generate_puzzle` on the phone.
#[tauri::command]
async fn generate_puzzle(args: String) -> Result<String, String> {
    hashi_offline::json::generate_puzzle(&args).await
}

/// The server function `solve_puzzle` on the phone.
#[tauri::command]
async fn solve_puzzle(args: String) -> Result<String, String> {
    hashi_offline::json::solve_puzzle(&args).await
}

/// The server function `puzzle_cnf` on the phone.
#[tauri::command]
async fn puzzle_cnf(args: String) -> Result<String, String> {
    hashi_offline::json::puzzle_cnf(&args).await
}
