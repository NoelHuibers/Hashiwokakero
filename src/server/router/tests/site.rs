//! Pages, static files, the sitemap, caching and compression.

use reqwest::header::{ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONTENT_ENCODING};

use super::*;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

async fn text(res: Response) -> String {
    res.text().await.unwrap()
}

#[tokio::test]
async fn pages_are_rendered_in_their_language() {
    let server = Server::start().await;
    let browser = server.browser();
    for (path, lang) in [
        ("/", "en"),
        ("/rules", "en"),
        ("/de/rules", "de"),
        ("/ja/ranks", "ja"),
        ("/zh-Hant/dojo", "zh-Hant"),
        ("/kontakt", "en"),
        ("/de/kontakt", "de"),
    ] {
        let res = browser.get(path).await;
        assert_status(&res, StatusCode::OK);
        assert!(
            res.headers()[CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html"),
            "{path}"
        );
        let html = text(res).await;
        assert!(html.starts_with("<!DOCTYPE html>"), "{path}");
        assert!(
            html.contains(&format!("<html lang=\"{lang}\"")),
            "{path}: {}",
            &html[..200]
        );
        // The hydration script and the stylesheet, with the hashes of this build
        assert!(html.contains("/pkg/hashi.0123abcd.js"), "{path}");
        assert!(html.contains("/pkg/hashi.0123abcd.css"), "{path}");
    }
    let html = text(browser.get("/ar").await).await;
    assert!(html.contains("dir=\"rtl\""));

    // The browser's language decides on pages without a language in the path
    let res = browser
        .http
        .get(format!("{}/rules", server.url))
        .header(ACCEPT_LANGUAGE, "fr-FR,fr;q=0.9")
        .send()
        .await
        .unwrap();
    assert!(text(res).await.contains("<html lang=\"fr\""));
}

#[tokio::test]
async fn unknown_paths_are_not_found() {
    let server = Server::start().await;
    let browser = server.browser();
    for path in [
        "/no-such-page",
        "/de/no-such-page",
        "/pkg/hashi.missing.js",
        "/api/nope",
    ] {
        let res = browser.get(path).await;
        assert_status(&res, StatusCode::NOT_FOUND);
    }
    // A missing server function: the router knows no such route either
    let res = browser.post("/api/nope").send().await.unwrap();
    assert_status(&res, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sitemap() {
    let server = Server::start().await;
    let res = server.browser().get("/sitemap.xml").await;
    assert_status(&res, StatusCode::OK);
    assert_eq!(res.headers()[CONTENT_TYPE], "application/xml");
    let xml = text(res).await;
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<loc>https://hashi.rs/</loc>"));
    assert!(xml.contains("<loc>https://hashi.rs/de/rules</loc>"));
}

#[tokio::test]
async fn hashed_files_are_cached_for_good() {
    let server = Server::start().await;
    let browser = server.browser();
    for (path, cache) in [
        ("/pkg/hashi.0123abcd.js", IMMUTABLE),
        ("/fonts/murecho.0123abcd.woff2", IMMUTABLE),
        // Its name stays the same, the files it points to change
        ("/fonts/fonts.css", "no-cache"),
        ("/", "no-cache"),
        ("/de/rules", "no-cache"),
        ("/sitemap.xml", "no-cache"),
        ("/dojo/board.json", "no-cache"),
    ] {
        let res = browser.get(path).await;
        assert_status(&res, StatusCode::OK);
        assert_eq!(res.headers()[CACHE_CONTROL], cache, "{path}");
    }
    // A hashed name that isn't there (a page from another deploy, say): a browser would keep
    // the error for a year
    for path in ["/pkg/hashi.fedcba98.js", "/fonts/murecho.fedcba98.woff2"] {
        let res = browser.get(path).await;
        assert_status(&res, StatusCode::NOT_FOUND);
        assert_eq!(res.headers()[CACHE_CONTROL], "no-cache", "{path}");
    }
    let called = browser.call_with_headers(crate::api::CurrentUser {}).await;
    called.result.unwrap();
    assert_eq!(called.headers[CACHE_CONTROL], "no-cache");

    // Without hashes in the names a new build has the same URLs. (One server at a time.)
    drop(server);
    let server = Server::with(|options| options.hash_files = false).await;
    let res = server.browser().get("/pkg/hashi.0123abcd.js").await;
    assert_eq!(res.headers()[CACHE_CONTROL], "no-cache");
}

#[tokio::test]
async fn text_is_compressed_fonts_are_not() {
    let server = Server::start().await;
    let browser = server.browser();
    let get = |path: &str, encoding: &str| {
        browser
            .http
            .get(format!("{}{path}", server.url))
            .header(ACCEPT_ENCODING, encoding)
            .send()
    };
    for path in ["/", "/de/rules", "/pkg/hashi.0123abcd.js", "/sitemap.xml"] {
        for encoding in ["br", "gzip"] {
            let res = get(path, encoding).await.unwrap();
            assert_status(&res, StatusCode::OK);
            assert_eq!(
                res.headers()
                    .get(CONTENT_ENCODING)
                    .map(|v| v.to_str().unwrap()),
                Some(encoding),
                "{path}"
            );
        }
        // Brotli when the browser takes both
        let res = get(path, "gzip, deflate, br").await.unwrap();
        assert_eq!(res.headers()[CONTENT_ENCODING], "br", "{path}");
        let res = get(path, "identity").await.unwrap();
        assert!(res.headers().get(CONTENT_ENCODING).is_none(), "{path}");
    }
    // woff2 is compressed already
    let res = get("/fonts/murecho.0123abcd.woff2", "br, gzip")
        .await
        .unwrap();
    assert_status(&res, StatusCode::OK);
    assert!(res.headers().get(CONTENT_ENCODING).is_none());
    assert_eq!(res.bytes().await.unwrap().len(), 4096);

    // A compressed page is the same page
    let res = get("/rules", "gzip").await.unwrap();
    let zipped = res.bytes().await.unwrap();
    let plain = text(browser.get("/rules").await).await;
    assert!(zipped.len() < plain.len() / 2);
}
