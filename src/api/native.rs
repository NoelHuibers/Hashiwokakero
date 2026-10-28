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
//!   `HashiPlugin.kt` in `gen/android`, `HashiPlugin.swift` in `ios/`). The capabilities in
//!   `capabilities/default.json` allow exactly these;
//! - the App Links the app is opened with (`tauri-plugin-deep-link`): their pages are the app's
//!   own, the router shows them (see `follow_app_links`);
//! - the phone's games service, Game Center or Google Play Games (`games_sign_in`,
//!   `games_report`, `games_show`, with the arguments of `api::games`), through the app's plugin.

use futures::FutureExt as _;
use futures::future::{LocalBoxFuture, Shared};
use leptos::prelude::{ServerFnError, document, window};
use leptos_router::NavigateOptions;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::{JsCast, JsValue};

use super::games::{
    self, Achievement, Board, Platform, PlayIds, Report, Reported, Score, Show, Status,
};
use crate::model::UserStats;

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
/// follow the page's theme, not the phone's (on iOS the status bar's, through the window's
/// style).
pub(crate) fn system_bars(dark: bool) {
    #[derive(Serialize)]
    struct Args {
        dark: bool,
    }
    run("plugin:hashi|system_bars", &Args { dark });
}

thread_local! {
    /// The answer of the page's one `games_sign_in`, for everyone who waits for it.
    static GAMES: std::cell::RefCell<Option<Shared<LocalBoxFuture<'static, Status>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Signs the player in to the phone's games service, Game Center or Google Play Games
/// (`api::games`), without asking: the platform signs in whoever is signed in on the phone. Asked
/// once per page, at the start (`mount`); later calls get the same answer. Never fails: outside
/// the app, or when the plugin refuses (a desktop build), the service isn't there.
///
/// iOS may want to show its sign-in sheet; the plugin does that once per install at most, and
/// never in the simulator (`HashiPlugin.swift`). Android signs in by itself at the start
/// (Play Games' automatic sign-in), the command only asks whether it did.
#[expect(
    clippy::future_not_send,
    reason = "a JavaScript promise, awaited on the page's one thread (`spawn_local`, `LocalResource`)"
)]
pub(crate) fn games_sign_in() -> impl Future<Output = Status> {
    GAMES.with(|games| {
        games
            .borrow_mut()
            .get_or_insert_with(|| ask_games().boxed_local().shared())
            .clone()
    })
}

#[expect(clippy::future_not_send, reason = "as `games_sign_in`")]
async fn ask_games() -> Status {
    let Some((internals, invoke)) = invoke() else {
        return Status::default();
    };
    let answer = match invoke.call1(&internals, &"plugin:hashi|games_sign_in".into()) {
        Ok(promise) => {
            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&promise)).await
        }
        Err(e) => Err(e),
    };
    let status = answer
        .map_err(|e| failed(&e).to_string())
        .and_then(|answer| {
            // A plugin without the command's answer: the service isn't there
            if answer.is_null() || answer.is_undefined() {
                return Ok(Status::default());
            }
            let json = js_sys::JSON::stringify(&answer).map_err(|e| failed(&e).to_string())?;
            serde_json::from_str(&String::from(json)).map_err(|e| e.to_string())
        });
    status.unwrap_or_else(|e| {
        leptos::logging::warn!("games_sign_in failed: {e}");
        Status::default()
    })
}

/// Reports `achievements` and `scores` to the phone's games service, once its sign-in is done.
/// What the server says the player has, mapped to the platform's ids by the caller (Apple's
/// vendor ids, Play's from `app/achievements-play.json`); see [`Report::new`] for what is left
/// out. In the background, a failure is only logged. Nothing outside the app, nor where the
/// service isn't there; the plugin skips it while nobody is signed in.
pub(crate) fn games_report(
    achievements: impl IntoIterator<Item = Achievement>,
    scores: impl IntoIterator<Item = Score>,
) {
    let Some(report) = Report::new(achievements, scores) else {
        return;
    };
    if !available() {
        return;
    }
    leptos::task::spawn_local(async move {
        if games_sign_in().await.available {
            run("plugin:hashi|games_report", &report);
        }
    });
}

thread_local! {
    /// What the page reported to the games service so far (`games::Reported`).
    static REPORTED: std::cell::RefCell<Reported> = std::cell::RefCell::default();
}

/// Reports the player's stamps and scores as the server has them (`my_stats`) to the phone's
/// games service: at the start (`mount`), and whenever the server says something changed, a
/// daily it took (the daily page's `submitted`) or stamps earned since the last visit. Only for
/// a player the start's sign-in signed in, otherwise the server isn't even asked. Never fails,
/// a failed call is only logged.
pub(crate) fn games_sync() {
    if !available() {
        return;
    }
    leptos::task::spawn_local(async {
        if !games_sign_in().await.signed_in {
            return;
        }
        match super::my_stats().await {
            Ok(Some(stats)) => games_progress(&stats),
            // No session: no stamps
            Ok(None) => {}
            Err(e) => leptos::logging::warn!("my_stats for the games service failed: {e}"),
        }
    });
}

/// Reports what the server's statistics `stats` of the player say, those the page has already
/// (the account page's), see [`games_sync`]: [`games::report`] with the ids of the phone's
/// platform, only what this page hasn't reported yet.
pub(crate) fn games_progress(stats: &UserStats) {
    if !available() {
        return;
    }
    let Some(platform) = super::app::app_platform().and_then(Platform::of) else {
        return;
    };
    let Some(report) = games::report(platform, PlayIds::embedded(), stats) else {
        return;
    };
    leptos::task::spawn_local(async move {
        if !games_sign_in().await.signed_in {
            return;
        }
        if let Some(fresh) = REPORTED.with_borrow_mut(|reported| reported.fresh(report)) {
            games_report(fresh.achievements, fresh.scores);
        }
    });
}

/// Opens the games service's own screens, its achievements or its leaderboards, after its
/// sign-in when nobody is signed in yet (a tap on a button: the one time the plugin may ask).
pub(crate) fn games_show(kind: Board) {
    run("plugin:hashi|games_show", &Show { kind });
}

/// The name of the phone's games service, for its button: `None` outside the app.
pub(crate) fn games_service() -> Option<&'static str> {
    super::app::app_platform().and_then(games::service)
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

/// Where the page notes the last App Link it showed, for the page loaded anew (a reload): the
/// plugin still has the one the app started with, which it shouldn't show again. The session's
/// storage lasts as long as the `WebView`: the next start of the app begins without it.
const APP_LINK_KEY: &str = "hashi.app-link";

/// Shows the pages of the App Links the app is opened with (`seo::app_link`, the intent filter
/// of `plugins.deep-link` in `tauri.conf.json`): the one it started with, instead of the page it
/// started on, and those that bring it back to the front later, on top of the page it was on.
/// With the router: the app's page stays, only another of its pages shows.
///
/// Android hands the links to tauri-plugin-deep-link, the page asks it for the first one
/// (`get_current`) and hears of the next ones through Tauri's events (`deep-link://new-url`,
/// what the plugin's `onOpenUrl` listens to). Links to other pages of the site, which the
/// plugin doesn't pass on anyway, change nothing.
pub(crate) fn follow_app_links(navigate: impl Fn(&str, NavigateOptions) + Clone + 'static) {
    use wasm_bindgen::closure::Closure;
    let Some((internals, invoke)) = invoke() else {
        return;
    };
    let first = invoke.call1(&internals, &"plugin:deep-link|get_current".into());
    let start = navigate.clone();
    leptos::task::spawn_local(async move {
        let urls = match first {
            Ok(promise) => {
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&promise)).await
            }
            Err(e) => Err(e),
        };
        match urls {
            Ok(urls) => {
                if let Some(url) = first_url(&urls) {
                    show_app_link(&url, true, &start);
                }
            }
            Err(e) => leptos::logging::warn!("get_current failed: {}", failed(&e)),
        }
    });

    // Tauri's `listen`: the handler is a callback of the page that Tauri calls with each event
    let Some(transform) = js_sys::Reflect::get(&internals, &"transformCallback".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    else {
        return;
    };
    let on_event = Closure::<dyn Fn(JsValue)>::new(move |event: JsValue| {
        let urls = js_sys::Reflect::get(&event, &"payload".into()).unwrap_or(JsValue::NULL);
        if let Some(url) = first_url(&urls) {
            show_app_link(&url, false, &navigate);
        }
    });
    let Ok(handler) = transform.call1(&internals, on_event.as_ref()) else {
        return;
    };
    // As long as the page
    on_event.forget();
    let args = js_sys::Object::new();
    let target = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&target, &"kind".into(), &"Any".into());
    let _ = js_sys::Reflect::set(&args, &"event".into(), &"deep-link://new-url".into());
    let _ = js_sys::Reflect::set(&args, &"target".into(), &target);
    let _ = js_sys::Reflect::set(&args, &"handler".into(), &handler);
    let listening = invoke.call2(&internals, &"plugin:event|listen".into(), &args);
    leptos::task::spawn_local(async move {
        let listening = match listening {
            Ok(promise) => {
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&promise)).await
            }
            Err(e) => Err(e),
        };
        if let Err(e) = listening {
            leptos::logging::warn!("listening for App Links failed: {}", failed(&e));
        }
    });
}

/// The first URL of the plugin's list (`get_current`, an event's payload), if there is one.
fn first_url(urls: &JsValue) -> Option<String> {
    js_sys::Array::is_array(urls)
        .then(|| js_sys::Array::from(urls).get(0).as_string())
        .flatten()
}

/// Shows the page of the App Link `url`: the app's page replaced by it at the `start`, a new
/// page in the history (which the back button leaves) later. Not when the app shows that page
/// already, nor at the start of a page loaded anew that showed it (see [`APP_LINK_KEY`]).
fn show_app_link(url: &str, start: bool, navigate: &impl Fn(&str, NavigateOptions)) {
    let Some(page) = crate::seo::app_link(url) else {
        return;
    };
    let storage = window().session_storage().ok().flatten();
    let shown = storage
        .as_ref()
        .and_then(|s| s.get_item(APP_LINK_KEY).ok().flatten());
    if start && shown.as_deref() == Some(url) {
        return;
    }
    if let Some(storage) = &storage {
        let _ = storage.set_item(APP_LINK_KEY, url);
    }
    let location = window().location();
    let here = [location.pathname(), location.search(), location.hash()]
        .into_iter()
        .map(Result::unwrap_or_default)
        .collect::<String>();
    if here == page {
        return;
    }
    navigate(
        &page,
        NavigateOptions {
            replace: start,
            ..NavigateOptions::default()
        },
    );
}
