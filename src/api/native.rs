//! The bridge to the app (`app/src-tauri`). Only in the app's build (`csr`); the website never
//! has Tauri.
//!
//! The page calls `window.__TAURI_INTERNALS__.invoke`, which Tauri's `WebView` defines, without
//! Tauri's npm package:
//!
//! - the app's commands, called like server functions: the offline practice runs the server
//!   functions of `puzzle.rs` on the phone itself (see `practice`). A command takes the server
//!   function's JSON body as one string `args` and answers with the JSON of its answer, also a
//!   string: JavaScript's numbers would round the 64-bit seeds of `GenerateParams`. It fails with
//!   the server function's error text, `E:<code>` included (`ApiError`);
//! - what the `WebView` can't do itself on the phone: links out of the app open in the browser
//!   (Tauri's opener plugin), sharing goes through the system's share sheet, also for files the
//!   website downloads, and the system bars' icons follow the page's theme (the app's plugin,
//!   `HashiPlugin.kt` in `gen/android`). The capabilities in `capabilities/default.json` allow
//!   exactly these.

use leptos::prelude::{ServerFnError, document, window};
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

/// Runs a plugin's command (`plugin:<name>|<command>`) with `args`, in the background: nothing
/// waits for it, a failure is only logged. `false` outside the app.
fn run(command: &'static str, args: &impl Serialize) -> bool {
    let Some((internals, invoke)) = invoke() else {
        return false;
    };
    let args = serde_json::to_string(args)
        .ok()
        .and_then(|json| js_sys::JSON::parse(&json).ok())
        .unwrap_or_else(|| js_sys::Object::new().into());
    let promise = match invoke.call2(&internals, &command.into(), &args) {
        Ok(promise) => js_sys::Promise::resolve(&promise),
        Err(e) => js_sys::Promise::reject(&e),
    };
    leptos::task::spawn_local(async move {
        if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
            leptos::logging::warn!("{command} failed: {}", failed(&e));
        }
    });
    true
}

/// Opens `url` in the phone's browser, or the app that takes it (`mailto:`), through Tauri's
/// opener plugin. The app's capabilities allow `https:` and `mailto:` URLs only. `false`
/// outside the app.
pub(crate) fn open_url(url: &str) -> bool {
    #[derive(Serialize)]
    struct Args<'a> {
        url: &'a str,
    }
    run("plugin:opener|open_url", &Args { url })
}

/// Opens the system's share sheet with `text`, or with `text` as a file `name` of type `mime`
/// (`file`). `false` outside the app.
pub(crate) fn share(text: &str, file: Option<(&str, &str)>) -> bool {
    #[derive(Serialize)]
    struct Args<'a> {
        text: &'a str,
        file: Option<&'a str>,
        mime: &'a str,
    }
    let (file, mime) = file.map_or((None, "text/plain"), |(name, mime)| (Some(name), mime));
    run("plugin:hashi|share", &Args { text, file, mime })
}

/// Dark icons on the system bars for the light theme, light ones for the dark theme. The page
/// draws behind the bars (`--app-inset-*` in `style/tailwind.css`), so their icons have to
/// follow the page's theme, not the phone's.
pub(crate) fn system_bars(dark: bool) {
    #[derive(Serialize)]
    struct Args {
        dark: bool,
    }
    run("plugin:hashi|system_bars", &Args { dark });
}

/// Sends every link that leads out of the app (`seo::opens_outside`) to [`open_url`]: the
/// `WebView` has no tabs, and would leave the app's pages for the website's. The app itself
/// refuses such navigations as well (`src-tauri/src/lib.rs`).
///
/// One listener on the document, before the router's on the window: it takes the clicks the
/// router leaves alone, links to other origins.
pub(crate) fn open_links_outside() {
    use wasm_bindgen::closure::Closure;
    if !available() {
        return;
    }
    let origin = window().location().origin().unwrap_or_default();
    let on_click = Closure::<dyn Fn(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
        if e.default_prevented() || e.button() != 0 {
            return;
        }
        let link = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| el.closest("a[href]").ok().flatten())
            .and_then(|a| a.dyn_into::<web_sys::HtmlAnchorElement>().ok());
        if let Some(link) = link
            && crate::seo::opens_outside(&link.href(), &origin)
        {
            e.prevent_default();
            open_url(&link.href());
        }
    });
    let _ = document().add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref());
    // As long as the page
    on_click.forget();
}
