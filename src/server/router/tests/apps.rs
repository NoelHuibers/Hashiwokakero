//! The app's version gate and its cross-origin calls (see `server::apps`): apps below
//! `HASHI_APP_MIN_VERSION` get `AppOutdated` before anything runs, and the app's origins may
//! call the API from their `WebView`.

use reqwest::Method;
use reqwest::header::{
    ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE,
    ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, CACHE_CONTROL, HeaderValue,
    ORIGIN,
};

use super::*;
use crate::api::{APP_HEADER, BUILD_HEADER, CurrentUser, GeneratePuzzle, Register};

const ANDROID: &str = "https://tauri.localhost";
const IOS: &str = "tauri://localhost";

/// The headers of an app call: its version, from its origin.
fn app(version: &str, origin: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(APP_HEADER, HeaderValue::from_str(version).unwrap());
    headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
    headers
}

/// A server that answers apps from 1.2.0 on.
async fn with_minimum() -> Server {
    Server::with_apps(&[("HASHI_APP_MIN_VERSION", "1.2.0")], |_| {}).await
}

async fn generate(browser: &Browser, headers: HeaderMap) -> Called<common::GeneratedPuzzle> {
    browser
        .call_sending(
            GeneratePuzzle {
                params: GenerateParams {
                    width: 7,
                    height: 7,
                    difficulty: Difficulty::Easy,
                    seed: 1,
                },
            },
            headers,
        )
        .await
}

#[track_caller]
fn header(headers: &HeaderMap, name: impl reqwest::header::AsHeaderName) -> Option<&str> {
    headers.get(name).map(|v| v.to_str().unwrap())
}

/// Below the minimum the app gets `AppOutdated`, encoded like any server function's error so
/// that its usual error handling reads it; from the minimum on it gets its answer.
#[tokio::test]
async fn apps_below_the_minimum_are_asked_to_update() {
    let server = with_minimum().await;
    let browser = server.browser();
    for version in [
        "android/1.1.9",
        "android/0.9.0",
        "ios/1.1.0",
        "fuchsia/1.0.0",
    ] {
        let called = browser
            .call_sending(CurrentUser {}, app(version, ANDROID))
            .await;
        assert_eq!(api_error(called.result), ApiError::AppOutdated, "{version}");
        assert_eq!(
            header(&called.headers, "serverfnerror"),
            Some(CurrentUser::PATH)
        );
        assert_eq!(header(&called.headers, CONTENT_TYPE), Some("text/plain"));
        // The app can read it from its origin, and it is never stored
        assert_eq!(
            header(&called.headers, ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(ANDROID)
        );
        assert_eq!(header(&called.headers, CACHE_CONTROL), Some("no-cache"));
        assert_eq!(header(&called.headers, BUILD_HEADER), Some("0123abcd"));
    }
    for version in [
        "android/1.2.0",
        "ios/1.2.1",
        "android/1.10.0",
        "android/2.0.0",
    ] {
        let called = browser
            .call_sending(CurrentUser {}, app(version, IOS))
            .await;
        assert!(called.result.is_ok(), "{version}: {:?}", called.result);
    }
}

/// The server function doesn't run for an outdated app: no account is made.
#[tokio::test]
async fn an_outdated_app_runs_nothing() {
    let server = with_minimum().await;
    let called = server
        .browser()
        .call_sending(
            Register {
                username: "outdated_app".into(),
                password: PASSWORD.into(),
            },
            app("android/1.0.0", ANDROID),
        )
        .await;
    assert_eq!(api_error(called.result), ApiError::AppOutdated);
    let mut rows = conn()
        .await
        .query(
            "SELECT COUNT(*) FROM users WHERE username = 'outdated_app'",
            (),
        )
        .await
        .unwrap();
    let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(count, 0);
}

/// Only the app sends the header: a value it can't have sent is outdated too.
#[tokio::test]
async fn an_unreadable_version_is_outdated() {
    let server = with_minimum().await;
    let browser = server.browser();
    for version in [
        "1.2.0",
        "android",
        "android/1.2",
        "android/v1.2.0",
        "Android/1.2.0",
    ] {
        let called = browser
            .call_sending(CurrentUser {}, app(version, ANDROID))
            .await;
        assert_eq!(api_error(called.result), ApiError::AppOutdated, "{version}");
    }
}

/// Without a minimum every app is answered, whatever it says it is.
#[tokio::test]
async fn without_a_minimum_every_app_is_answered() {
    let server = Server::start().await;
    let browser = server.browser();
    for version in ["android/0.0.1", "ios/1.0.0", "garbage"] {
        let called = browser
            .call_sending(CurrentUser {}, app(version, ANDROID))
            .await;
        assert!(called.result.is_ok(), "{version}: {:?}", called.result);
    }
}

/// The web never sends the header and is never turned away; neither are pages and plain
/// endpoints, the gate is for server functions.
#[tokio::test]
async fn the_web_and_pages_are_never_gated() {
    let server = with_minimum().await;
    let browser = server.browser();
    assert!(browser.call(CurrentUser {}).await.is_ok());
    let mut empty = HeaderMap::new();
    empty.insert(APP_HEADER, HeaderValue::from_static(""));
    assert!(
        browser
            .call_sending(CurrentUser {}, empty)
            .await
            .result
            .is_ok()
    );

    let outdated = server.browser_sending(app("android/1.0.0", ANDROID));
    for path in ["/", "/de/ranks", "/dojo/board.json", "/sitemap.xml"] {
        assert_status(&outdated.get(path).await, StatusCode::OK);
    }
}

/// Outdated calls are answered before admission: they don't use up the network's budget for
/// expensive calls (32 a minute), and the same network's current app still gets through.
#[tokio::test]
async fn outdated_calls_cost_no_budget() {
    let server = with_minimum().await;
    let browser = server.browser();
    for _ in 0..40 {
        let called = generate(&browser, app("android/1.0.0", ANDROID)).await;
        assert_eq!(api_error(called.result), ApiError::AppOutdated);
    }
    let called = generate(&browser, app("android/1.2.0", ANDROID)).await;
    assert!(called.result.is_ok(), "{:?}", called.result);
}

/// A preflight as the app's `WebView` sends it before a server function call.
fn preflight(browser: &Browser, path: &str, origin: &str) -> reqwest::RequestBuilder {
    browser
        .http
        .request(Method::OPTIONS, format!("{}{path}", browser.url))
        .header(ORIGIN, origin)
        .header(ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(
            ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization,content-type,x-hashi-app",
        )
}

/// The layer answers preflights itself, for the app's origins: no admission, no 404 page, no
/// server function.
#[tokio::test]
async fn preflights_from_the_app_are_answered() {
    let server = Server::with(|o| o.env = Env::PROD).await;
    let browser = server.browser();
    for origin in [ANDROID, IOS] {
        for path in [
            GeneratePuzzle::PATH,
            "/api/no_such_function",
            "/no-such-page",
        ] {
            let res = preflight(&browser, path, origin).send().await.unwrap();
            assert_status(&res, StatusCode::OK);
            let headers = res.headers();
            assert_eq!(header(headers, ACCESS_CONTROL_ALLOW_ORIGIN), Some(origin));
            assert_eq!(
                header(headers, ACCESS_CONTROL_ALLOW_METHODS),
                Some("GET,POST")
            );
            assert_eq!(
                header(headers, ACCESS_CONTROL_ALLOW_HEADERS),
                Some("content-type,accept,authorization,x-hashi-app")
            );
            assert_eq!(header(headers, ACCESS_CONTROL_MAX_AGE), Some("7200"));
            // The app sends bearer tokens, never cookies
            assert_eq!(header(headers, ACCESS_CONTROL_ALLOW_CREDENTIALS), None);
            assert!(res.bytes().await.unwrap().is_empty());
        }
    }
}

/// Preflights never reach admission: a network's preflights leave its budget for the calls.
#[tokio::test]
async fn preflights_cost_no_budget() {
    let server = Server::start().await;
    let browser = server.browser();
    for _ in 0..40 {
        let res = preflight(&browser, GeneratePuzzle::PATH, ANDROID)
            .send()
            .await
            .unwrap();
        assert_status(&res, StatusCode::OK);
    }
    let called = generate(&browser, app("android/1.0.0", ANDROID)).await;
    assert!(called.result.is_ok(), "{:?}", called.result);
}

/// The app reads the build and its new session from the answer, which a cross-origin page
/// can only do when they are exposed.
#[tokio::test]
async fn answers_to_the_app_expose_its_headers() {
    let server = Server::start().await;
    let called = server
        .browser()
        .call_sending(
            Register {
                username: "exposed_app".into(),
                password: PASSWORD.into(),
            },
            app("android/1.0.0", ANDROID),
        )
        .await;
    assert!(called.result.is_ok(), "{:?}", called.result);
    let headers = &called.headers;
    assert_eq!(header(headers, ACCESS_CONTROL_ALLOW_ORIGIN), Some(ANDROID));
    assert_eq!(
        header(headers, ACCESS_CONTROL_EXPOSE_HEADERS),
        Some("x-hashi-build,x-hashi-session")
    );
    assert_eq!(header(headers, ACCESS_CONTROL_ALLOW_CREDENTIALS), None);
    // A new account: its token comes in the exposed header
    assert!(headers.contains_key(crate::api::SESSION_HEADER));
    assert!(headers.contains_key(BUILD_HEADER));
}

/// Other sites get no `Allow-Origin`, so their pages can't read the API.
#[tokio::test]
async fn other_origins_are_not_allowed() {
    let server = Server::start().await;
    let browser = server.browser();
    for origin in [
        "https://evil.example",
        "https://tauri.localhost.evil.example",
        "null",
    ] {
        let res = preflight(&browser, CurrentUser::PATH, origin)
            .send()
            .await
            .unwrap();
        assert_eq!(
            header(res.headers(), ACCESS_CONTROL_ALLOW_ORIGIN),
            None,
            "{origin}"
        );
        let called = browser
            .call_sending(CurrentUser {}, app("android/1.0.0", origin))
            .await;
        assert_eq!(
            header(&called.headers, ACCESS_CONTROL_ALLOW_ORIGIN),
            None,
            "{origin}"
        );
    }
    // Nor without an origin, as from the site itself
    let called = browser.call_with_headers(CurrentUser {}).await;
    assert_eq!(header(&called.headers, ACCESS_CONTROL_ALLOW_ORIGIN), None);
}

/// The endpoints for scripts keep answering every site with `*`.
#[tokio::test]
async fn script_endpoints_stay_open_to_every_site() {
    let server = Server::start().await;
    for origin in [None, Some("https://evil.example")] {
        let mut headers = HeaderMap::new();
        if let Some(origin) = origin {
            headers.insert(ORIGIN, HeaderValue::from_static(origin));
        }
        let browser = server.browser_sending(headers);
        for path in ["/dojo/board.json", "/dojo/seasons.json"] {
            let res = browser.get(path).await;
            assert_status(&res, StatusCode::OK);
            assert_eq!(
                header(res.headers(), ACCESS_CONTROL_ALLOW_ORIGIN),
                Some("*"),
                "{path} {origin:?}"
            );
        }
    }
}

/// The app's dev server runs on localhost: allowed in development, never in production.
#[tokio::test]
async fn localhost_is_an_origin_in_development_only() {
    const LOCAL: [&str; 3] = [
        "http://localhost:1420",
        "http://127.0.0.1:8080",
        "http://localhost",
    ];
    let dev = Server::start().await;
    for origin in LOCAL {
        let res = preflight(&dev.browser(), CurrentUser::PATH, origin)
            .send()
            .await
            .unwrap();
        assert_eq!(
            header(res.headers(), ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(origin)
        );
    }
    drop(dev);
    let prod = Server::with(|o| o.env = Env::PROD).await;
    for origin in LOCAL {
        let res = preflight(&prod.browser(), CurrentUser::PATH, origin)
            .send()
            .await
            .unwrap();
        assert_eq!(
            header(res.headers(), ACCESS_CONTROL_ALLOW_ORIGIN),
            None,
            "{origin}"
        );
        let called = prod
            .browser()
            .call_sending(CurrentUser {}, app("android/1.0.0", origin))
            .await;
        assert_eq!(
            header(&called.headers, ACCESS_CONTROL_ALLOW_ORIGIN),
            None,
            "{origin}"
        );
    }
}

/// `HASHI_APP_ORIGINS` replaces the default origins.
#[tokio::test]
async fn the_origins_can_be_configured() {
    let server = Server::with_apps(
        &[(
            "HASHI_APP_ORIGINS",
            "https://app.example,http://tauri.localhost",
        )],
        |o| o.env = Env::PROD,
    )
    .await;
    let browser = server.browser();
    for (origin, allowed) in [
        ("https://app.example", true),
        ("http://tauri.localhost", true),
        (ANDROID, false),
        (IOS, false),
    ] {
        let res = preflight(&browser, CurrentUser::PATH, origin)
            .send()
            .await
            .unwrap();
        assert_eq!(
            header(res.headers(), ACCESS_CONTROL_ALLOW_ORIGIN),
            allowed.then_some(origin),
            "{origin}"
        );
    }
}
