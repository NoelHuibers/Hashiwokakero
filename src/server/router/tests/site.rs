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
    // The font of the script, already in the page the server sends (hydration fails otherwise)
    let html = text(browser.get("/ko/rules").await).await;
    assert!(html.contains("/fonts/noto-sans-kr.css"));
    let html = text(browser.get("/de/rules").await).await;
    assert!(!html.contains("/fonts/noto-sans"));

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

/// Each page's `<title>` in the language of its path. `leptos_meta` writes it into the head after
/// the body, see `seo::title_text`.
#[tokio::test]
async fn pages_have_their_titles_in_their_language() {
    use crate::i18n::{Locale, td_string};

    let server = Server::start().await;
    let browser = server.browser();
    let hashi = |title: &str| format!("{title} · Hashi");
    for (path, title) in [
        ("/", td_string!(Locale::en, daily.meta.title).to_string()),
        (
            "/ja/rules",
            td_string!(Locale::ja, rules.meta.title).to_string(),
        ),
        (
            "/fr/ranks",
            td_string!(Locale::fr, ranks.meta.title).to_string(),
        ),
        ("/de/kontakt", hashi(td_string!(Locale::de, contact.title))),
        ("/de/account", hashi(td_string!(Locale::de, account.title))),
        // No such player (a player's name: `pages::a_players_page_names_them_in_link_previews`)
        (
            "/es/player/0",
            hashi(td_string!(Locale::es, common.not_found.title)),
        ),
        (
            "/no-such-page",
            hashi(td_string!(Locale::en, common.not_found.title)),
        ),
    ] {
        let html = text(browser.get(path).await).await;
        assert!(
            html.contains(&format!("<title>{title}</title>")),
            "{path}: {title}, not in {}",
            &html[..html.len().min(1500)]
        );
    }
}

/// The header renders above the routes. It used to follow the browser's language on the server,
/// so a first visit to /ja/rules from an English browser got an English header until hydration.
#[tokio::test]
async fn the_header_follows_the_language_of_the_path() {
    let server = Server::start().await;
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT_LANGUAGE, "en-US,en;q=0.9".parse().unwrap());
    let browser = server.browser_sending(headers);

    let html = text(browser.get("/ja/rules").await).await;
    assert!(html.contains(r#"aria-label="メインナビゲーション""#));
    assert!(!html.contains("Main navigation"));
    assert!(!html.contains(r#"aria-label="How to play""#));

    let html = text(browser.get("/rules").await).await;
    assert!(html.contains(r#"aria-label="Main navigation""#));
}

/// CSS hides the logo's text on phones and the sign-in text below the desktop breakpoint.
/// The server must send useful localized names before hydration too.
#[tokio::test]
async fn mobile_header_links_have_localized_names() {
    use crate::i18n::{Locale, td_string};

    let server = Server::start().await;
    let browser = server.browser();
    for (path, locale) in [
        ("/rules", Locale::en),
        ("/ja/rules", Locale::ja),
        ("/ar/rules", Locale::ar),
    ] {
        let html = text(browser.get(path).await).await;
        let header = html
            .split("<header")
            .nth(1)
            .unwrap()
            .split("</header>")
            .next()
            .unwrap();
        let home = header
            .split("<a ")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        let name = format!("Hashi · {}", td_string!(locale, common.nav.daily));
        assert!(home.contains(&format!("aria-label=\"{name}\"")), "{home}");
        let account = header
            .split("<a ")
            .map(|a| a.split('>').next().unwrap())
            .find(|a| a.contains("/account\""))
            .unwrap();
        let name = td_string!(locale, common.sign_in);
        assert!(
            account.contains(&format!("aria-label=\"{name}\"")),
            "{account}"
        );
    }
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

/// Records the panics of every thread while it lives, then puts the previous hook back. The panic
/// hook is global: the messages of other tests' panics land here too, so look for the one you mean.
struct Panics {
    seen: Arc<std::sync::Mutex<Vec<String>>>,
    previous: Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync>,
}

impl Panics {
    fn record() -> Self {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let previous: Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync> =
            std::panic::take_hook().into();
        std::panic::set_hook(Box::new({
            let seen = Arc::clone(&seen);
            let previous = Arc::clone(&previous);
            move |info| {
                seen.lock().unwrap().push(info.to_string());
                previous(info);
            }
        }));
        Self { seen, previous }
    }

    fn matching(&self, text: &str) -> Vec<String> {
        let seen = self.seen.lock().unwrap();
        seen.iter().filter(|m| m.contains(text)).cloned().collect()
    }
}

impl Drop for Panics {
    fn drop(&mut self) {
        // A failed test unwinds through here, and the hook can't be changed then
        if !std::thread::panicking() {
            let previous = Arc::clone(&self.previous);
            std::panic::set_hook(Box::new(move |info| previous(info)));
        }
    }
}

/// `leptos_i18n`'s context runs an effect on the server, in a task of its own, that copied the
/// language into the cookie. On a multi-threaded runtime that task could still run while the
/// request ended and its signals were dropped, and panicked with "Tried to access a reactive
/// value that has already been disposed" (a few percent of all pages). Many pages at once, on
/// several threads like the real server, and not one of those panics.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pages_do_not_touch_disposed_signals() {
    let server = Server::start().await;
    let panics = Panics::record();
    let mut clients = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let browser = server.browser();
        clients.spawn(async move {
            for _ in 0..25 {
                for path in ["/", "/ja/rules", "/de/kontakt", "/no-such-page"] {
                    let res = browser.get(path).await;
                    assert!(res.status().is_success() || res.status() == StatusCode::NOT_FOUND);
                    res.bytes().await.unwrap();
                }
            }
        });
    }
    while let Some(client) = clients.join_next().await {
        client.unwrap();
    }
    // The tasks of the last pages may still run
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let disposed = panics.matching("disposed");
    assert!(
        disposed.is_empty(),
        "{} panics, the first: {}",
        disposed.len(),
        disposed[0]
    );
}

/// The search engines' consoles verify the site with a file of theirs at the root, `IndexNow` with
/// the key file (`server::search`): none of them without the settings.
#[tokio::test]
async fn without_tokens_there_are_no_verification_files() {
    let server = Server::start().await;
    let browser = server.browser();
    for path in [
        "/BingSiteAuth.xml",
        "/yandex_0123456789abcdef.html",
        "/3f2b8a9c4d1e4f6a9b7c0e5d1a2b3c4d.txt",
    ] {
        assert_status(&browser.get(path).await, StatusCode::NOT_FOUND);
    }
}

/// With them, each file exactly as its console offers it, for `HEAD` too, never cached for good
/// (a changed token is there with the next deploy). Other names stay 404s, and the site's own
/// files are still there.
#[tokio::test]
async fn search_engines_find_their_files() {
    const KEY: &str = "3f2b8a9c-4d1e-4f6a-9b7c-0e5d1a2b3c4d";
    let server = Server::with_search(&[
        ("HASHI_BING_SITE_AUTH", "0123456789ABCDEF0123456789ABCDEF"),
        ("HASHI_YANDEX_VERIFICATION", "yandex_0123456789abcdef.html"),
        ("HASHI_INDEXNOW_KEY", KEY),
    ])
    .await;
    let browser = server.browser();
    let key_file = format!("/{KEY}.txt");
    for (path, content_type, body) in [
        (
            "/BingSiteAuth.xml",
            "application/xml",
            "<?xml version=\"1.0\"?>\n<users>\n\t<user>0123456789ABCDEF0123456789ABCDEF</user>\n</users>\n"
                .to_string(),
        ),
        (
            "/yandex_0123456789abcdef.html",
            "text/html; charset=utf-8",
            "<html>\n    <head>\n        <meta http-equiv=\"Content-Type\" content=\"text/html; charset=UTF-8\">\n    </head>\n    <body>Verification: 0123456789abcdef</body>\n</html>\n"
                .to_string(),
        ),
        (key_file.as_str(), "text/plain; charset=utf-8", KEY.to_string()),
    ] {
        let res = browser.get(path).await;
        assert_status(&res, StatusCode::OK);
        assert_eq!(res.headers()[CONTENT_TYPE], content_type, "{path}");
        assert_eq!(res.headers()[CACHE_CONTROL], "no-cache", "{path}");
        assert_eq!(text(res).await, body, "{path}");
        let res = browser
            .http
            .head(format!("{}{path}", server.url))
            .send()
            .await
            .unwrap();
        assert_status(&res, StatusCode::OK);
    }
    for path in [
        "/yandex_0000000000000000.html",
        "/google0123456789abcdef.html",
        "/0123456789abcdef.txt",
        "/de/BingSiteAuth.xml",
        "/BingSiteAuth.xml/",
    ] {
        assert_status(&browser.get(path).await, StatusCode::NOT_FOUND);
    }
    assert_status(&browser.get("/sitemap.xml").await, StatusCode::OK);
    assert_status(&browser.get("/rules").await, StatusCode::OK);
}
