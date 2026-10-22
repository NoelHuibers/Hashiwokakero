//! The app's commands (`app/src-tauri`), called like server functions: the offline practice
//! runs the server functions of `puzzle.rs` on the phone itself (see `practice`). Only in the
//! app's build (`csr`); the website never has Tauri.
//!
//! The page calls `window.__TAURI_INTERNALS__.invoke`, which Tauri's `WebView` defines, without
//! Tauri's npm package. A command takes the server function's JSON body as one string `args`
//! and answers with the JSON of its answer, also a string: JavaScript's numbers would round the
//! 64-bit seeds of `GenerateParams`. It fails with the server function's error text, `E:<code>`
//! included (`ApiError`).

use leptos::prelude::ServerFnError;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::{JsCast, JsValue};

/// Tauri's `invoke` and the object it belongs to, if the page runs in the app's `WebView`.
fn invoke() -> Option<(JsValue, js_sys::Function)> {
    let internals = js_sys::Reflect::get(&js_sys::global(), &"__TAURI_INTERNALS__".into()).ok()?;
    let invoke = js_sys::Reflect::get(&internals, &"invoke".into()).ok()?;
    Some((internals, invoke.dyn_into().ok()?))
}

/// Whether the page runs in the app, where the commands are. In a browser (the app's bundle on
/// a development server, its smoke test) the server functions go to the server.
pub(super) fn available() -> bool {
    invoke().is_some()
}

/// Calls the app's `command` with the arguments of the server function of the same name.
///
/// Its future is `Send` like the server functions': a JavaScript promise never leaves the page's
/// one thread, the callers await it in `spawn_local`.
pub(super) fn call<T: DeserializeOwned>(
    command: &'static str,
    args: &impl Serialize,
) -> impl Future<Output = Result<T, ServerFnError>> + Send {
    let body = serde_json::to_string(args).map_err(|e| error(e.to_string()));
    send_wrapper::SendWrapper::new(async move {
        let (internals, invoke) =
            invoke().ok_or_else(|| error("the app's commands are missing"))?;
        let payload = js_sys::Object::new();
        js_sys::Reflect::set(&payload, &"args".into(), &body?.into()).map_err(|e| failed(&e))?;
        let promise = invoke
            .call2(&internals, &command.into(), &payload)
            .map_err(|e| failed(&e))?;
        let answer = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&promise))
            .await
            .map_err(|e| failed(&e))?;
        let answer = answer
            .as_string()
            .ok_or_else(|| error("the app's command answered no text"))?;
        serde_json::from_str(&answer).map_err(|e| error(e.to_string()))
    })
}

/// The error of a server function that failed on the server with `text`.
fn error(text: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError(text.into())
}

/// A command's error, or Tauri's (a command the page may not call, say).
fn failed(error: &JsValue) -> ServerFnError {
    self::error(error.as_string().unwrap_or_else(|| format!("{error:?}")))
}
