//! The social preview images: PNGs per language and per day, their headers, and the pages that
//! name them.

use reqwest::header::{ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING};

use super::*;
use crate::i18n::Locale;
use crate::model::date_string;
use crate::seo::{og_daily_image, og_image};

#[tokio::test]
async fn preview_images_are_pngs_cached_by_language_and_day() {
    let server = Server::start().await;
    let browser = server.browser();
    let today = date_string(daily::today());
    for (path, cache) in [
        ("/og/de.png".to_string(), "public, max-age=86400"),
        ("/og/zh-Hant.png".into(), "public, max-age=86400"),
        // Where links shared before point: the English card
        ("/og.png".into(), "public, max-age=86400"),
        // A day's card never changes
        (
            format!("/og/daily/{today}/ja.png"),
            "public, max-age=31536000, immutable",
        ),
    ] {
        let res = browser
            .http
            .get(format!("{}{path}", server.url))
            .header(ACCEPT_ENCODING, "br, gzip")
            .send()
            .await
            .unwrap();
        assert_status(&res, StatusCode::OK);
        assert_eq!(res.headers()[CONTENT_TYPE], "image/png", "{path}");
        assert_eq!(res.headers()[CACHE_CONTROL], cache, "{path}");
        // A PNG is compressed already
        assert!(res.headers().get(CONTENT_ENCODING).is_none(), "{path}");
        let png = res.bytes().await.unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "{path}");
        // 1200×630
        assert_eq!(&png[16..24], [0, 0, 4, 176, 0, 0, 2, 118], "{path}");
    }
    // The second time from memory, the same image
    let first = browser.get("/og/de.png").await.bytes().await.unwrap();
    let second = browser.get("/og/de.png").await.bytes().await.unwrap();
    assert_eq!(first, second);

    for path in [
        "/og/xx.png".to_string(),
        "/og/de.jpg".into(),
        "/og/daily/2020-01-01/en.png".into(),
        "/og/daily/today/en.png".into(),
        format!("/og/daily/{today}/xx.png"),
        // Days that have no daily yet
        format!("/og/daily/{}/en.png", date_string(daily::today() + 2)),
    ] {
        let res = browser.get(&path).await;
        assert_status(&res, StatusCode::NOT_FOUND);
        assert_eq!(res.headers()[CACHE_CONTROL], "no-cache", "{path}");
    }
}

/// The `content` of a `<meta>` in the head of a page.
fn meta<'a>(html: &'a str, attribute: &str) -> &'a str {
    let tag = html
        .split("<meta ")
        .find(|tag| tag.contains(&format!("=\"{attribute}\"")))
        .unwrap_or_else(|| panic!("no {attribute}"));
    tag.split("content=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
}

#[tokio::test]
async fn pages_name_the_image_of_their_language_the_home_page_todays() {
    let server = Server::start().await;
    let browser = server.browser();
    let today = daily::today();
    for (path, image) in [
        ("/de/rules", og_image(Locale::de)),
        ("/zh-Hant/practice", og_image(Locale::zh_Hant)),
        ("/ranks", og_image(Locale::en)),
        // The home page is the daily: today's card, in its language
        ("/", og_daily_image(Locale::en, today)),
        ("/ar", og_daily_image(Locale::ar, today)),
    ] {
        let html = browser.get(path).await.text().await.unwrap();
        assert_eq!(meta(&html, "og:image"), image, "{path}");
        assert_eq!(meta(&html, "twitter:image"), image, "{path}");
        assert_eq!(meta(&html, "og:image:type"), "image/png", "{path}");
        assert!(!meta(&html, "og:image:alt").is_empty(), "{path}");
    }
    let html = browser.get("/de").await.text().await.unwrap();
    assert!(
        html.contains(&format!("/og/daily/{}/de.png", date_string(today))),
        "{html}"
    );
    // The description of today's card: its number, in German
    let alt = meta(&html, "og:image:alt");
    let number = crate::model::daily_number(today);
    assert!(alt.contains(&format!("#{number}")), "{alt}");
}
