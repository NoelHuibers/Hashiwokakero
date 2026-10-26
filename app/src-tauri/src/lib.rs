//! The mobile app: a Tauri window with the web bundle of `app/` (the UI rendered in the
//! browser, see `../README.md`). Android and iOS start it through `run`, the desktop (for
//! development only) through `main.rs`.
//!
//! The page calls the commands below for the offline practice instead of the server functions
//! of the same names (`src/api/practice.rs` in the repository root), with the server functions'
//! JSON as text both ways (see `hashi_offline::json`). Only the main window may call them
//! (`capabilities/default.json`, the permissions from `build.rs`).
//!
//! Links out of the app open in the phone's browser through Tauri's opener plugin; the app's own
//! plugin ([`hashi`]) shares through the system's share sheet, sets the system bars' icons and
//! keeps the `WebView` on the app's pages. Links to the app's pages on `https://hashi.rs` (App Links,
//! `plugins.deep-link` in `tauri.conf.json`) open the app through the deep-link plugin, and the
//! page shows them (`api::native::follow_app_links`).

use tauri::plugin::TauriPlugin;
use tauri::{Manager as _, Runtime, Url, Webview};
use tauri_plugin_opener::OpenerExt as _;

/// Starts the app: one window with `index.html` of the bundle (`tauri.conf.json`).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // The page sends the links itself (`api::native` in the repository root), the plugin's
        // own script for them isn't needed
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        // Starts with Android's intent and hears of the next ones, see `hashi` for why they never
        // reach the `WebView` as navigations
        .plugin(tauri_plugin_deep_link::init())
        .plugin(hashi())
        .invoke_handler(tauri::generate_handler![
            generate_puzzle,
            solve_puzzle,
            puzzle_cnf
        ])
        .run(tauri::generate_context!())
        .expect("the app failed to start");
}

// The iOS side of the plugin `hashi`, `ios/Sources/HashiPlugin.swift` (linked by `build.rs`)
#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_hashi);

/// The app's own plugin: its commands are `HashiPlugin.kt` on Android (`gen/android`) and
/// `HashiPlugin.swift` on iOS (`ios/`), the `share` and `system_bars` of `api::native`.
///
/// It also keeps the `WebView` on the app's pages: a page of another site (a link the page
/// didn't send to the opener, a script's `location`) opens in the browser instead when it is
/// an https one, else nowhere. Tauri lets the `WebView` go anywhere by itself. That includes
/// `https://hashi.rs/g/<code>` and the other App Links: an App Link the app is opened with
/// isn't a navigation, the deep-link plugin hands it to the page, whose router shows its page
/// in the app (`api::native::follow_app_links`). From a page of the app such a link goes to
/// the system, which brings it back to the app once it verified the app's links (Android's App
/// Links, iOS's Universal Links).
fn hashi<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("hashi")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            api.register_android_plugin("rs.hashi", "HashiPlugin")?;
            #[cfg(target_os = "ios")]
            api.register_ios_plugin(init_plugin_hashi)?;
            // The desktop, for development, has no side
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let _ = api;
            Ok(())
        })
        .on_navigation(|webview, url| {
            if in_app(webview, url) {
                return true;
            }
            if url.scheme() == "https" {
                // Not here: the opener asks the system, which may need this thread
                let app = webview.app_handle().clone();
                let url = url.to_string();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = app.opener().open_url(url, None::<&str>) {
                        eprintln!("could not open a link: {e}");
                    }
                });
            }
            false
        })
        .build()
}

/// Whether `url` is a page of the app: its bundle (`tauri.localhost` on Android, `localhost`
/// on iOS) or, in development, the development server (`devUrl`).
fn in_app<R: Runtime>(webview: &Webview<R>, url: &Url) -> bool {
    let bundle = match url.scheme() {
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        "tauri" => url.host_str() == Some("localhost"),
        // The empty page the WebView starts with
        "about" => true,
        _ => false,
    };
    bundle
        || (cfg!(debug_assertions)
            && webview
                .config()
                .build
                .dev_url
                .as_ref()
                .is_some_and(|dev| dev.origin() == url.origin()))
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
