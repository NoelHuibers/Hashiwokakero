//! The build every response and page names, the hash of the site's WASM (`server::build_id`),
//! which lets a tab of an older build tell an outdated call from an error (`api::client`).

use super::*;
use crate::api::BUILD_HEADER;
use crate::model::is_outdated_error;

/// The `wasm:` hash of the hash.txt the tests write (see `setup`).
const BUILD: &str = "0123abcd";

fn build_of(headers: &HeaderMap) -> Option<&str> {
    headers.get(BUILD_HEADER).map(|v| v.to_str().unwrap())
}

#[tokio::test]
async fn responses_and_pages_name_the_build() {
    let server = Server::start().await;
    let browser = server.browser();
    // Pages, static files, the 404 page, plain endpoints
    for path in [
        "/",
        "/de/ranks",
        "/pkg/hashi.0123abcd.js",
        "/no-such-page",
        "/sitemap.xml",
        "/dojo/board.json",
    ] {
        let res = browser.get(path).await;
        assert_eq!(build_of(res.headers()), Some(BUILD), "{path}");
    }
    // Server functions, answers and errors alike
    let called = browser
        .call_with_headers(crate::api::DailyLeaderboard { day: None })
        .await;
    assert!(called.result.is_ok());
    assert_eq!(build_of(&called.headers), Some(BUILD));
    let called = browser
        .call_with_headers(crate::api::Login {
            username: "nobody_of_this_build".into(),
            password: PASSWORD.into(),
        })
        .await;
    assert_eq!(api_error(called.result), ApiError::WrongLogin);
    assert_eq!(build_of(&called.headers), Some(BUILD));

    // The page names the build of its WASM, for the tab to compare
    for path in ["/", "/ja/rules", "/no-such-page"] {
        let html = browser.get(path).await.text().await.unwrap();
        assert!(
            html.contains(r#"<meta name="hashi-build" content="0123abcd">"#),
            "{path}"
        );
    }
}

/// Without hashed file names there is no hash.txt to read and no build to compare: nothing is
/// sent, and a tab never takes an error for an outdated version.
#[tokio::test]
async fn without_hashed_files_there_is_no_build() {
    let server = Server::with(|o| o.hash_files = false).await;
    let browser = server.browser();
    let res = browser.get("/").await;
    assert_eq!(build_of(res.headers()), None);
    let html = res.text().await.unwrap();
    assert!(html.contains(r#"<meta name="hashi-build">"#));
    let called = browser
        .call_with_headers(crate::api::DailyLeaderboard { day: None })
        .await;
    assert!(called.result.is_ok());
    assert_eq!(build_of(&called.headers), None);

    // A missing hash.txt is the same (a server at a time, see `Server`)
    drop(server);
    let server = Server::with(|o| o.hash_file = "/no/such/hash.txt".into()).await;
    let res = server.browser().get("/").await;
    assert_eq!(build_of(res.headers()), None);
}

/// A tab of an older build calls a server function whose arguments changed: the server can't
/// read them. The tab, whose page names another build than the answer, is asked to reload; with
/// its server's build, the same error would be a bug and is shown as one.
#[tokio::test]
async fn changed_arguments_from_another_build_are_an_outdated_client() {
    let server = Server::start().await;
    let browser = server.browser();
    let res = browser
        .post(<crate::api::DailyLeaderboard as ServerFn>::PATH)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        // `day` was a date once, say
        .body(r#"{"day":"2026-10-20"}"#)
        .send()
        .await
        .unwrap();
    assert!(res.status().is_server_error() || res.status().is_client_error());
    let headers = res.headers().clone();
    let error = ServerFnError::de(res.bytes().await.unwrap());
    // The JSON codec reports arguments it can't read like the client an answer it can't read
    assert!(
        matches!(error, ServerFnError::Deserialization(_)),
        "{error:?}"
    );
    let server_build = build_of(&headers);
    assert_eq!(server_build, Some(BUILD));
    assert!(is_outdated_error(
        &error,
        Some("an older build"),
        server_build
    ));
    assert!(!is_outdated_error(&error, Some(BUILD), server_build));

    // An error the server meant stays itself, also for a tab of another build
    let called = browser
        .call_with_headers(crate::api::Login {
            username: "nobody_of_this_build".into(),
            password: PASSWORD.into(),
        })
        .await;
    let error = called.result.unwrap_err();
    assert!(!is_outdated_error(
        &error,
        Some("an older build"),
        build_of(&called.headers)
    ));
}

/// What cargo-leptos writes to hash.txt, with or without the split WASM's lines.
#[test]
fn the_build_is_the_hash_of_the_wasm() {
    use super::super::wasm_hash;
    assert_eq!(
        wasm_hash(
            "js: wSP2sZK69Iiu_no7mY_kfg\nwasm: uMxFW8aXYHI3xOgOtD2kiQ\ncss: 5cfwDkJcT9ALaQHSU6vQJg\n"
        ),
        Some("uMxFW8aXYHI3xOgOtD2kiQ")
    );
    assert_eq!(wasm_hash("  wasm:x-_1  \r\nsplit: y\n"), Some("x-_1"));
    assert_eq!(wasm_hash("js: a\ncss: c\n"), None);
    assert_eq!(wasm_hash("wasm: \n"), None);
    assert_eq!(wasm_hash(""), None);
}
