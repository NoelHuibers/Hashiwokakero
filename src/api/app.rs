//! The app's side of the API headers (docs/mobile.md): the server it calls, who it says it is,
//! and the session token it keeps instead of the website's cookie.
//!
//! Only the app's build (`csr`, started by `hashi::mount`) sends any of it; the website's calls
//! go out as they always did. The app's page runs on an origin of its own (`tauri.localhost`),
//! so its calls go to the website's server, cross-origin, and the session cookie of hashi.rs
//! would be a third-party one there. The app keeps the token in its local storage instead,
//! which only the app can read: on Android the `WebView`'s storage is in the app's private data.

use std::sync::OnceLock;

use crate::seo::SITE_URL;

/// The app on Google Play.
const PLAY_STORE_URL: &str = "https://play.google.com/store/apps/details?id=rs.hashi";

/// TODO(step 13 of docs/mobile.md): the app's page on the App Store, once it is listed there.
/// Until then the website, where the newest version always runs.
const APP_STORE_URL: &str = SITE_URL;

/// Where the desktop builds are (docs/stores.md, Desktop builds): the GitHub releases of the app's
/// tags, which have its installers for Windows, macOS and Linux.
const DESKTOP_DOWNLOAD_URL: &str = "https://github.com/NoelHuibers/Hashiwokakero/releases";

/// The Windows app from the Microsoft Store (docs/stores.md, Microsoft Store): its Store ID
/// (Partner Center, Product identity), which the release's web bundle gets from the repository
/// variable `MS_STORE_APP_ID` as `HASHI_MS_STORE_ID`.
const MICROSOFT_STORE_ID: Option<&str> = option_env!("HASHI_MS_STORE_ID");

/// The desktops the app runs on, as the app's plugin names them to the page
/// (`window.__HASHI_DESKTOP__`, `app/src-tauri/src/desktop.rs`): Rust's `std::env::consts::OS`,
/// or `msstore` for the Windows app installed from the Microsoft Store, which the Store updates.
const DESKTOPS: [&str; 4] = ["windows", "macos", "linux", "msstore"];

/// Who the app is: set once by [`start`], `None` on the website.
#[cfg(feature = "csr")]
static APP: OnceLock<App> = OnceLock::new();

#[cfg(feature = "csr")]
#[derive(Debug)]
struct App {
    platform: &'static str,
    /// The value of `x-hashi-app`.
    header: String,
}

/// Where the page runs when it is the app: the origins of the Tauri `WebView` (Android with
/// and without `useHttpsScheme`, iOS).
#[cfg(any(feature = "csr", test))]
const APP_ORIGINS: [&str; 3] = [
    "https://tauri.localhost",
    "http://tauri.localhost",
    "tauri://localhost",
];

/// The server the app calls from its own origin: hashi.rs, or the server named by
/// `HASHI_SERVER_URL` when the app was built. The app's content security policy
/// (`app/src-tauri/tauri.conf.json`, `connect-src`) has to allow it as well.
///
/// `None` keeps the calls on the page's origin: the bundle in a browser, where a proxy passes
/// `/api` on (`pnpm run app:web`, the smoke test in e2e/app).
#[cfg(any(feature = "csr", test))]
fn server_url(origin: &str, configured: Option<&'static str>) -> Option<&'static str> {
    APP_ORIGINS.contains(&origin).then(|| {
        configured
            .map(|url| url.trim_end_matches('/'))
            .filter(|url| !url.is_empty())
            .unwrap_or(SITE_URL)
    })
}

/// The platform in `x-hashi-app`: the desktop the app's plugin names (`desktop`, one of
/// [`DESKTOPS`]), else the phone of the browser's user agent. iPads ask for the desktop site and
/// say they are a Mac, one with a touch screen. Anything else is `web`, which the server accepts
/// like any platform it doesn't know.
#[cfg(any(feature = "csr", test))]
fn platform(desktop: Option<&str>, user_agent: &str, touch_points: i32) -> &'static str {
    if let Some(desktop) = DESKTOPS.into_iter().find(|&d| Some(d) == desktop) {
        desktop
    } else if user_agent.contains("Android") {
        "android"
    } else if ["iPhone", "iPad", "iPod"]
        .iter()
        .any(|device| user_agent.contains(device))
        || (user_agent.contains("Macintosh") && touch_points > 1)
    {
        "ios"
    } else {
        "web"
    }
}

/// Where an update of the app is, for the "Update" button of `ToastAction::Update`.
fn store_url(platform: &str) -> &'static str {
    match platform {
        "android" => PLAY_STORE_URL,
        "ios" => APP_STORE_URL,
        "msstore" => MICROSOFT_STORE_URL.get_or_init(|| {
            microsoft_store_url(MICROSOFT_STORE_ID).unwrap_or_else(|| SITE_URL.to_string())
        }),
        desktop if DESKTOPS.contains(&desktop) => DESKTOP_DOWNLOAD_URL,
        _ => SITE_URL,
    }
}

/// [`microsoft_store_url`] of the build's Store ID, else the website.
static MICROSOFT_STORE_URL: OnceLock<String> = OnceLock::new();

/// The app's page in the Microsoft Store, by its Store ID (12 letters and digits, starting with
/// a 9): the browser offers to open it in the Store app, where the update is. `None` without a
/// Store ID (a build for no release) or with a malformed one.
fn microsoft_store_url(id: Option<&str>) -> Option<String> {
    let id = id?.trim();
    let well_formed =
        id.len() == 12 && id.starts_with('9') && id.bytes().all(|b| b.is_ascii_alphanumeric());
    well_formed.then(|| {
        format!(
            "https://apps.microsoft.com/detail/{}",
            id.to_ascii_lowercase()
        )
    })
}

/// The store page of this app, see [`store_url`]; the website where there is no app.
pub fn update_url() -> &'static str {
    #[cfg(feature = "csr")]
    let platform = APP.get().map_or("web", |app| app.platform);
    #[cfg(not(feature = "csr"))]
    let platform = "web";
    store_url(platform)
}

/// Sets up the app's calls before the first one: the server they go to, and the app's identity.
///
/// `version` is the app's, `major.minor.patch`, which the server compares with its minimum.
#[cfg(feature = "csr")]
pub fn start(version: &'static str) {
    use leptos::prelude::window;
    let window = window();
    let origin = window.location().origin().unwrap_or_default();
    if let Some(url) = server_url(&origin, option_env!("HASHI_SERVER_URL")) {
        leptos::server_fn::client::set_server_url(url);
    }
    let navigator = window.navigator();
    // Set by the app's plugin on the desktop only
    let desktop = js_sys::Reflect::get(&window, &"__HASHI_DESKTOP__".into())
        .ok()
        .and_then(|value| value.as_string());
    let platform = platform(
        desktop.as_deref(),
        &navigator.user_agent().unwrap_or_default(),
        navigator.max_touch_points(),
    );
    let _ = APP.set(App {
        platform,
        header: format!("{platform}/{version}"),
    });
}

/// The `x-hashi-app` header of the app's calls, `None` before [`start`].
#[cfg(feature = "csr")]
pub(super) fn app_header() -> Option<&'static str> {
    APP.get().map(|app| app.header.as_str())
}

/// The platform the app runs on: the phone, `android` or `ios`, or the desktop, `windows`,
/// `macos`, `linux` or `msstore` (`web` in a browser); `None` before [`start`].
#[cfg(feature = "csr")]
pub(super) fn app_platform() -> Option<&'static str> {
    APP.get().map(|app| app.platform)
}

/// The app's platform (`android`, `ios`, a desktop, `web` in a browser) and version, for the feedback
/// form's context; `None` on the website.
pub fn app_identity() -> Option<(&'static str, &'static str)> {
    #[cfg(feature = "csr")]
    {
        APP.get().map(|app| {
            let version = app.header.split_once('/').map_or("", |(_, v)| v);
            (app.platform, version)
        })
    }
    #[cfg(not(feature = "csr"))]
    {
        None
    }
}

/// A key pressed, with its modifiers, for [`history_step`].
#[cfg(any(feature = "csr", test))]
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the modifiers of a `KeyboardEvent`, as it has them"
)]
pub(super) struct Keys<'a> {
    pub(super) key: &'a str,
    pub(super) alt: bool,
    pub(super) ctrl: bool,
    pub(super) meta: bool,
    pub(super) shift: bool,
}

/// Where a key in the desktop app on `platform` goes in the history, as in the browsers there:
/// one page back (`-1`) or forward (`1`), `None` for other keys. On macOS ⌘[ and ⌘] (Safari's),
/// on Linux Alt+← and Alt+→. On Windows nothing: `WebView2` has Alt+← and Alt+→ itself, and the
/// phones have their back gesture.
#[cfg(any(feature = "csr", test))]
pub(super) fn history_step(platform: &str, keys: Keys<'_>) -> Option<i32> {
    let only = |alt, meta| keys.alt == alt && keys.meta == meta && !keys.ctrl && !keys.shift;
    match (platform, keys.key) {
        ("macos", "[") if only(false, true) => Some(-1),
        ("macos", "]") if only(false, true) => Some(1),
        ("linux", "ArrowLeft") if only(true, false) => Some(-1),
        ("linux", "ArrowRight") if only(true, false) => Some(1),
        _ => None,
    }
}

/// The local storage key of the session token.
#[cfg(any(feature = "csr", test))]
const SESSION_KEY: &str = "hashi.session";

/// The session token the app holds, if it holds a well-formed one.
#[cfg(any(feature = "csr", test))]
pub(super) fn session_token() -> Option<String> {
    crate::state::load_raw(SESSION_KEY).filter(|token| well_formed(token))
}

/// 64 hex digits, like the server's tokens: anything else in storage is no session.
#[cfg(any(feature = "csr", test))]
fn well_formed(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The headers the app adds to a call: who it is, and its session if it has one.
#[cfg(any(feature = "csr", test))]
pub(super) fn request_headers(app: &str, token: Option<&str>) -> Vec<(&'static str, String)> {
    let mut headers = vec![(super::APP_HEADER, app.to_string())];
    if let Some(token) = token {
        headers.push((
            "authorization",
            format!("Bearer {}{token}", super::SESSION_BEARER),
        ));
    }
    headers
}

/// Takes the session token of a response (`x-hashi-session`, `received`): a new one replaces
/// the token, an empty one ends the session.
///
/// Only when the token is still the one the call was sent with (`sent`): a call that went out
/// before a sign in and comes back after it renewed the session it was sent with, the one the
/// sign in replaced.
#[cfg(any(feature = "csr", test))]
pub(super) fn keep_session(sent: Option<&str>, received: Option<&str>) {
    let Some(received) = received else {
        return;
    };
    if session_token().as_deref() != sent {
        return;
    }
    if received.is_empty() {
        crate::state::remove_item(SESSION_KEY);
    } else if well_formed(received) {
        crate::state::store_raw(SESSION_KEY, received);
    }
}

/// The server functions that make a session for a visitor without one: a guest of the first
/// daily, group, rival, report or block, or the account of a sign in.
#[cfg(any(feature = "csr", test))]
const SESSION_MAKERS: [&str; 8] = [
    "start_daily",
    "create_group",
    "join_group",
    "set_rival",
    "report_player",
    "block_player",
    "login",
    "register",
];

/// Whether the call of `url` may make a session (see [`SESSION_MAKERS`]).
#[cfg(any(feature = "csr", test))]
pub(super) fn may_make_session(url: &str) -> bool {
    url.rsplit_once("/api/")
        .and_then(|(_, name)| name.split(['?', '#']).next())
        .is_some_and(|name| SESSION_MAKERS.contains(&name))
}

/// One session for calls that start together. Without a token, two calls that each make a
/// guest (two rivals marked at once, say) would make two, and the token of the last answer
/// would win, with half the progress on the other guest.
#[cfg(feature = "csr")]
pub(super) static SESSION_GATE: futures::lock::Mutex<()> = futures::lock::Mutex::new(());

/// Waits until a call may go out, before its headers are set.
///
/// With a token, at once. Without one, after the call that may be making it: a call that may
/// make a session (`maker`) holds `gate` until its answer's token is kept, the guard this
/// returns; the others wait for that and go out with the token it brought.
#[cfg(any(feature = "csr", test))]
pub(super) async fn admit(
    gate: &futures::lock::Mutex<()>,
    maker: bool,
) -> Option<futures::lock::MutexGuard<'_, ()>> {
    if session_token().is_some() {
        return None;
    }
    let guard = gate.lock().await;
    // The call before may have brought a token
    (maker && session_token().is_none()).then_some(guard)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as Shared};

    use futures::channel::oneshot;
    use futures::executor::LocalPool;
    use futures::lock::Mutex;
    use futures::task::LocalSpawnExt;

    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn forget() {
        crate::state::remove_item(SESSION_KEY);
    }

    #[test]
    fn the_app_calls_the_website_from_its_own_origins_only() {
        for origin in APP_ORIGINS {
            assert_eq!(
                server_url(origin, None),
                Some("https://hashi.rs"),
                "{origin}"
            );
            // A server of the build's choice, with or without a slash at the end
            assert_eq!(
                server_url(origin, Some("http://10.0.2.2:3000/")),
                Some("http://10.0.2.2:3000")
            );
            assert_eq!(server_url(origin, Some("")), Some("https://hashi.rs"));
        }
        // The bundle in a browser: the proxy in front of it passes /api on
        for origin in [
            "http://127.0.0.1:8080",
            "http://localhost:3002",
            "https://hashi.rs",
            "https://tauri.localhost.example",
            "",
        ] {
            assert_eq!(server_url(origin, None), None, "{origin}");
            assert_eq!(server_url(origin, Some("https://staging.hashi.rs")), None);
        }
    }

    #[test]
    fn the_platform_comes_from_the_desktop_or_the_user_agent() {
        let android = "Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36";
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148";
        let ipad = "Mozilla/5.0 (iPad; CPU OS 12_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148";
        let mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
        let linux = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36";
        let windows = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0";
        assert_eq!(platform(None, android, 5), "android");
        assert_eq!(platform(None, iphone, 5), "ios");
        assert_eq!(platform(None, ipad, 5), "ios");
        // An iPad asking for the desktop site, and a real Mac (its trackpad has no touch points)
        assert_eq!(platform(None, mac, 5), "ios");
        assert_eq!(platform(None, mac, 0), "web");
        assert_eq!(platform(None, linux, 0), "web");
        assert_eq!(platform(None, windows, 10), "web");
        assert_eq!(platform(None, "", 0), "web");
        // The desktop app: its plugin says which one, whatever the user agent (a touch screen
        // on Windows, Linux's WebKitGTK that looks like Safari)
        assert_eq!(platform(Some("windows"), windows, 10), "windows");
        assert_eq!(platform(Some("macos"), mac, 0), "macos");
        assert_eq!(platform(Some("linux"), mac, 0), "linux");
        assert_eq!(platform(Some("msstore"), windows, 10), "msstore");
        // Only the desktops it knows: Rust's other systems are left to the user agent
        assert_eq!(platform(Some("freebsd"), linux, 0), "web");
        assert_eq!(platform(Some(""), android, 5), "android");
    }

    fn keys(key: &str) -> Keys<'_> {
        Keys {
            key,
            ..Keys::default()
        }
    }

    #[test]
    fn the_desktops_keys_go_back_and_forward() {
        let command = |key| Keys {
            meta: true,
            ..keys(key)
        };
        let alt = |key| Keys {
            alt: true,
            ..keys(key)
        };
        assert_eq!(history_step("macos", command("[")), Some(-1));
        assert_eq!(history_step("macos", command("]")), Some(1));
        assert_eq!(history_step("linux", alt("ArrowLeft")), Some(-1));
        assert_eq!(history_step("linux", alt("ArrowRight")), Some(1));
        // WebView2 does it itself, and the phones have their gestures
        for platform in ["windows", "msstore", "android", "ios", "web"] {
            assert_eq!(history_step(platform, alt("ArrowLeft")), None, "{platform}");
            assert_eq!(history_step(platform, command("[")), None, "{platform}");
        }
    }

    #[test]
    fn other_keys_stay_with_the_page() {
        // The board's arrows, a bracket typed, the other systems' keys
        assert_eq!(history_step("linux", keys("ArrowLeft")), None);
        assert_eq!(history_step("macos", keys("[")), None);
        assert_eq!(
            history_step(
                "macos",
                Keys {
                    alt: true,
                    ..keys("ArrowLeft")
                }
            ),
            None
        );
        assert_eq!(
            history_step(
                "linux",
                Keys {
                    meta: true,
                    ..keys("[")
                }
            ),
            None
        );
        // With another modifier too: a shortcut of its own
        let shifted = Keys {
            alt: true,
            shift: true,
            ..keys("ArrowLeft")
        };
        assert_eq!(history_step("linux", shifted), None);
        let ctrl = Keys {
            meta: true,
            ctrl: true,
            ..keys("[")
        };
        assert_eq!(history_step("macos", ctrl), None);
    }

    #[test]
    fn updates_come_from_the_store_of_the_platform() {
        assert_eq!(
            store_url("android"),
            "https://play.google.com/store/apps/details?id=rs.hashi"
        );
        assert_eq!(store_url("ios"), APP_STORE_URL);
        for desktop in ["windows", "macos", "linux"] {
            assert_eq!(
                store_url(desktop),
                "https://github.com/NoelHuibers/Hashiwokakero/releases"
            );
        }
        // The Store's app: its page in the Store, the website in a build without the Store ID
        assert_eq!(
            store_url("msstore"),
            microsoft_store_url(MICROSOFT_STORE_ID)
                .as_deref()
                .unwrap_or(SITE_URL)
        );
        assert_eq!(
            microsoft_store_url(Some("9NBLGGH4R315")).as_deref(),
            Some("https://apps.microsoft.com/detail/9nblggh4r315")
        );
        for wrong in [
            "",
            "9NBLGGH4R31",
            "9NBLGGH4R3155",
            "8NBLGGH4R315",
            "9NBLGGH4R31/",
        ] {
            assert_eq!(microsoft_store_url(Some(wrong)), None, "{wrong}");
        }
        assert_eq!(microsoft_store_url(None), None);
        assert_eq!(store_url("web"), "https://hashi.rs");
        // Outside the app
        assert_eq!(update_url(), "https://hashi.rs");
    }

    #[test]
    fn calls_say_who_they_are_and_send_the_session() {
        assert_eq!(
            request_headers("android/1.2.3", None),
            [("x-hashi-app", "android/1.2.3".to_string())]
        );
        assert_eq!(
            request_headers("ios/1.0.0", Some(TOKEN)),
            [
                ("x-hashi-app", "ios/1.0.0".to_string()),
                ("authorization", format!("Bearer hs1.{TOKEN}")),
            ]
        );
    }

    #[test]
    fn the_token_of_an_answer_is_kept_and_an_empty_one_ends_the_session() {
        forget();
        // No header: nothing changes
        keep_session(None, None);
        assert_eq!(session_token(), None);
        // A new guest
        keep_session(None, Some(TOKEN));
        assert_eq!(session_token().as_deref(), Some(TOKEN));
        assert_eq!(
            crate::state::load_raw("hashi.session").as_deref(),
            Some(TOKEN)
        );
        // A renewal sends the same one again, a sign in another one
        keep_session(Some(TOKEN), Some(TOKEN));
        assert_eq!(session_token().as_deref(), Some(TOKEN));
        keep_session(Some(TOKEN), Some(OTHER));
        assert_eq!(session_token().as_deref(), Some(OTHER));
        // Not a token: ignored
        keep_session(Some(OTHER), Some("hs1.nope"));
        assert_eq!(session_token().as_deref(), Some(OTHER));
        // Logged out, or the account deleted
        keep_session(Some(OTHER), Some(""));
        assert_eq!(session_token(), None);
        assert_eq!(crate::state::load_raw("hashi.session"), None);
    }

    /// A call sent before a sign in and answered after it brings the old session back; it
    /// must not replace the new one, or end it.
    #[test]
    fn an_answer_to_an_older_session_is_ignored() {
        forget();
        keep_session(None, Some(OTHER));
        keep_session(Some(TOKEN), Some(TOKEN));
        keep_session(Some(TOKEN), Some(""));
        keep_session(None, Some(TOKEN));
        assert_eq!(session_token().as_deref(), Some(OTHER));
        forget();
    }

    #[test]
    fn a_stored_value_that_is_no_token_is_no_session() {
        crate::state::store_raw("hashi.session", "not a token");
        assert_eq!(session_token(), None);
        crate::state::store_raw("hashi.session", &TOKEN[1..]);
        assert_eq!(session_token(), None);
        forget();
    }

    #[test]
    fn the_calls_that_may_make_a_session() {
        for url in [
            "/api/start_daily",
            "https://hashi.rs/api/set_rival",
            "http://127.0.0.1:3002/api/login?x=1",
            "/api/register",
            "/api/join_group",
            "/api/create_group",
        ] {
            assert!(may_make_session(url), "{url}");
        }
        for url in [
            "/api/current_user",
            "/api/daily_today",
            "/api/start_daily_x",
            "/start_daily",
        ] {
            assert!(!may_make_session(url), "{url}");
        }
    }

    /// Every server function that may make a session is in [`SESSION_MAKERS`], and only those:
    /// the ones that make a guest (`ensure_user`), sign in or register.
    #[test]
    fn the_session_makers_are_the_server_functions_that_make_sessions() {
        let sources = [
            include_str!("account.rs"),
            include_str!("admin.rs"),
            include_str!("banzuke.rs"),
            include_str!("contact.rs"),
            include_str!("daily.rs"),
            include_str!("dojo.rs"),
            include_str!("groups.rs"),
            include_str!("hanko.rs"),
            include_str!("kadai.rs"),
            include_str!("moderation.rs"),
            include_str!("puzzle.rs"),
            include_str!("replay.rs"),
            include_str!("rivals.rs"),
            include_str!("river.rs"),
        ];
        let mut makers: Vec<_> = sources
            .iter()
            .flat_map(|source| source.split("#[server").skip(1))
            .filter(|function| {
                ["auth::ensure_user(", "auth::login(", "auth::register("]
                    .iter()
                    .any(|call| function.contains(call))
            })
            .filter_map(|function| {
                let name = function.split_once("pub async fn ")?.1;
                Some(name.split_once('(')?.0)
            })
            .collect();
        makers.sort_unstable();
        let mut listed = SESSION_MAKERS.to_vec();
        listed.sort_unstable();
        assert_eq!(makers, listed);
    }

    /// What a call does in [`super::super::client`]: waits to be admitted, sends with the token
    /// it finds, and keeps the token of its answer, which `answer` brings when the test says.
    async fn call(
        gate: &Mutex<()>,
        maker: bool,
        answer: oneshot::Receiver<Option<&'static str>>,
        sent: Arc<Shared<Vec<Option<String>>>>,
    ) {
        let _admitted = admit(gate, maker).await;
        let token = session_token();
        sent.lock().unwrap().push(token.clone());
        let received = answer.await.unwrap();
        keep_session(token.as_deref(), received);
    }

    /// Two calls that may each make a guest, at once: the second waits for the first one's
    /// guest and goes out with its token, instead of making a second guest.
    #[test]
    fn calls_without_a_session_wait_for_the_one_making_it() {
        forget();
        let gate: &'static Mutex<()> = Box::leak(Box::new(Mutex::new(())));
        let mut pool = LocalPool::new();
        let sent = Arc::new(Shared::new(Vec::new()));
        let (first, first_answer) = oneshot::channel();
        let (second, second_answer) = oneshot::channel();
        let (third, third_answer) = oneshot::channel();
        let spawner = pool.spawner();
        spawner
            .spawn_local(call(gate, true, first_answer, sent.clone()))
            .unwrap();
        spawner
            .spawn_local(call(gate, true, second_answer, sent.clone()))
            .unwrap();
        // One that can't make a session waits too, so it doesn't go out without one
        spawner
            .spawn_local(call(gate, false, third_answer, sent.clone()))
            .unwrap();
        pool.run_until_stalled();
        assert_eq!(*sent.lock().unwrap(), [None]);
        first.send(Some(TOKEN)).unwrap();
        pool.run_until_stalled();
        let with_token = Some(TOKEN.to_string());
        assert_eq!(
            *sent.lock().unwrap(),
            [None, with_token.clone(), with_token]
        );
        second.send(None).unwrap();
        third.send(None).unwrap();
        pool.run_until_stalled();
        assert_eq!(session_token().as_deref(), Some(TOKEN));
        forget();
    }

    /// A first call that fails brings no token: the next one may make the session then.
    #[test]
    fn a_failed_first_call_lets_the_next_one_make_the_session() {
        forget();
        let gate: &'static Mutex<()> = Box::leak(Box::new(Mutex::new(())));
        let mut pool = LocalPool::new();
        let sent = Arc::new(Shared::new(Vec::new()));
        let (first, first_answer) = oneshot::channel();
        let (second, second_answer) = oneshot::channel();
        let spawner = pool.spawner();
        spawner
            .spawn_local(call(gate, true, first_answer, sent.clone()))
            .unwrap();
        spawner
            .spawn_local(call(gate, true, second_answer, sent.clone()))
            .unwrap();
        pool.run_until_stalled();
        first.send(None).unwrap();
        pool.run_until_stalled();
        assert_eq!(*sent.lock().unwrap(), [None, None]);
        second.send(Some(OTHER)).unwrap();
        pool.run_until_stalled();
        assert_eq!(session_token().as_deref(), Some(OTHER));
        forget();
    }

    /// With a session, or for calls that can't make one, nobody waits.
    #[test]
    fn calls_go_out_at_once_with_a_session_or_none_being_made() {
        let gate = Mutex::new(());
        let held = gate.try_lock().unwrap();
        crate::state::store_raw("hashi.session", TOKEN);
        assert!(futures::executor::block_on(admit(&gate, true)).is_none());
        drop(held);
        forget();
        // Without a session, a call that can't make one doesn't hold the others up
        assert!(futures::executor::block_on(admit(&gate, false)).is_none());
        assert!(gate.try_lock().is_some());
        // One that can holds the gate until its answer is in
        let guard = futures::executor::block_on(admit(&gate, true));
        assert!(guard.is_some());
        assert!(gate.try_lock().is_none());
        drop(guard);
        assert!(gate.try_lock().is_some());
    }
}
