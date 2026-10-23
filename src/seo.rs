//! Metadata for search engines and social previews: one [`PageMeta`] per route, plus the
//! JSON-LD structured data that describes the game and the rules.

use std::fmt::Write as _;

use leptos::prelude::*;
use leptos::text_prop::TextProp;
use leptos_i18n::Locale as _;
use leptos_meta::{Link, Meta, Script, Title};
use serde_json::json;

use crate::i18n::{Locale, td_string, use_i18n};
use crate::lang::{localized, og_locale};

pub const SITE_URL: &str = "https://hashi.rs";
pub const SITE_NAME: &str = "Hashi";
pub const AUTHOR: &str = "Noel Huibers";
pub const AUTHOR_URL: &str = "https://github.com/NoelHuibers";
pub const REPO_URL: &str = "https://github.com/NoelHuibers/Hashiwokakero";
/// The English preview image. Pages in another language use [`og_image`].
pub const OG_IMAGE: &str = "https://hashi.rs/og/en.png";

/// Where the links people share point (results, invites): the site the page runs on, the
/// website in the app, whose page runs on an origin of its own (`tauri.localhost`).
/// Browser only.
pub fn public_origin() -> String {
    if cfg!(feature = "csr") {
        SITE_URL.to_string()
    } else {
        window().location().origin().unwrap_or_default()
    }
}

/// A link to something only the server has (downloads, JSON): a path of this site, but in the
/// app, which only has the pages, the website's.
pub fn server_link(path: &str) -> String {
    server_link_in(cfg!(feature = "csr"), path)
}

/// The `target` of a [`server_link`]: in the app a new window, which is the phone's browser
/// (see [`opens_outside`]). The app's `WebView` shouldn't navigate away to the website.
pub fn server_link_target() -> Option<&'static str> {
    cfg!(feature = "csr").then_some("_blank")
}

/// Whether a link to `href` (the whole URL) leaves a page of `origin`.
///
/// A page of another site, or an e-mail to write: the app opens these with the phone's browser
/// or mail app instead of in its `WebView` (`api::native`).
pub fn opens_outside(href: &str, origin: &str) -> bool {
    if href.starts_with("mailto:") {
        return true;
    }
    let web = href.starts_with("https://") || href.starts_with("http://");
    let same = href
        .strip_prefix(origin)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']));
    web && !same
}

/// [`server_link`] in the app (`app`) or on the website.
fn server_link_in(app: bool, path: &str) -> String {
    if app {
        format!("{SITE_URL}{path}")
    } else {
        path.to_string()
    }
}

/// The pages of `https://hashi.rs` whose links open in the Android app (App Links).
///
/// Besides [`APP_LINK_PREFIXES`]: the daily (`/`, where a daily result's share text points),
/// practice (`/practice`, the practice's), and the list of groups. In every language,
/// `/ja/practice` too. The links people send each other, to pages that show what only the
/// player's own session knows. Not the rest: the rules and the legal pages, the admin's, the Dojo
/// with its uploads, and nothing the server answers itself (`/api`, `/og`, the downloads), which
/// the app doesn't have. `plugins.deep-link.mobile` in `app/src-tauri/tauri.conf.json` lists them
/// for Android (a test compares the two).
pub const APP_LINK_PATHS: &[&str] = &["/", "/practice", "/groups"];

/// The paths whose pages below them open in the app, like [`APP_LINK_PATHS`]: invites to a group
/// (`/g/<code>`), the pages of a group (its puzzles, its replays) and players' pages.
pub const APP_LINK_PREFIXES: &[&str] = &["/g/", "/groups/", "/player/"];

/// The app's page for the App Link `url`, with its query and fragment.
///
/// The page the app shows when Android opens it with the link. `None` for every other URL:
/// another site, another page, a path with anything but letters, digits, `-` and `_` between its
/// slashes (no `..`, which the browser would resolve to another page).
pub fn app_link(url: &str) -> Option<String> {
    let rest = url.strip_prefix(SITE_URL)?;
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let (path, after) = rest.split_at(end);
    let path = if path.is_empty() { "/" } else { path };
    let plain = path.starts_with('/')
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b));
    if !plain {
        return None;
    }
    let page = crate::lang::unlocalized(path);
    let covered = APP_LINK_PATHS.contains(&page.as_str())
        || APP_LINK_PREFIXES
            .iter()
            .any(|prefix| page.starts_with(prefix));
    covered.then(|| format!("{path}{after}"))
}

/// The preview image (`og:image`) of the pages in a language, drawn by `server::og`.
pub fn og_image(locale: Locale) -> String {
    format!("{SITE_URL}/og/{}.png", locale.as_str())
}

/// The preview image of a day's daily, for the home page. The date is in the URL, so link
/// previews that cached yesterday's don't show it today.
pub fn og_daily_image(locale: Locale, day: i64) -> String {
    format!(
        "{SITE_URL}/og/daily/{}/{}.png",
        crate::model::date_string(day),
        locale.as_str()
    )
}

/// Today's day number (UTC), as the server counts the dailies.
fn today() -> i64 {
    #[cfg(feature = "ssr")]
    {
        crate::server::daily::today()
    }
    #[cfg(not(feature = "ssr"))]
    {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "milliseconds since 1970 fit an i64"
        )]
        crate::model::day_of(js_sys::Date::now() as i64)
    }
}

/// The preview image of a page and its description. The home page is today's daily: its card.
fn page_image(locale: Locale, path: &str) -> (String, String) {
    let alt = td_string!(locale, common.og_image_alt);
    if path != "/" {
        return (og_image(locale), alt.to_string());
    }
    let day = today();
    let difficulty = crate::model::daily_difficulty(day);
    let (w, h) = difficulty.size();
    let alt = format!(
        "{} · {} · {} {w}×{h}. {alt}",
        td_string!(
            locale,
            daily.title,
            number = crate::model::daily_number(day)
        ),
        crate::lang::date_long(locale, day),
        crate::lang::difficulty_name(locale, difficulty),
    );
    (og_daily_image(locale, day), alt)
}

/// Title, description, canonical URL, Open Graph and Twitter tags of a page.
#[component]
pub fn PageMeta(
    /// Shown in the tab and as the search result title, keep it under 60 characters.
    #[prop(into)]
    title: Signal<String>,
    /// The search result snippet, keep it under 160 characters.
    #[prop(into)]
    description: Signal<String>,
    /// Path of the page, e.g. `/rules`.
    path: &'static str,
    /// JSON-LD structured data, see [`site_ld`] and [`faq_ld`].
    #[prop(optional)]
    json_ld: Option<String>,
) -> impl IntoView {
    let i18n = use_i18n();
    // The language is fixed per request on the server, where crawlers read these tags
    let locale = i18n.get_locale_untracked();
    let url = format!("{SITE_URL}{}", localized(locale, path));
    let alternates = Locale::get_all()
        .iter()
        .map(|l| {
            view! {
                <Link
                    rel="alternate"
                    hreflang=l.as_str()
                    href=format!("{SITE_URL}{}", localized(*l, path))
                />
            }
        })
        .collect_view();
    let og_alternates = Locale::get_all()
        .iter()
        .filter(|l| **l != locale)
        .map(|l| view! { <Meta property="og:locale:alternate" content=og_locale(*l) /> })
        .collect_view();
    let (image, image_alt) = page_image(locale, path);
    view! {
        <Title text=title_text(move || title.get()) />
        <Meta name="description" content=move || description.get() />
        <Link rel="canonical" href=url.clone() />
        {alternates}
        <Link rel="alternate" hreflang="x-default" href=format!("{SITE_URL}{path}") />
        <Meta property="og:type" content="website" />
        <Meta property="og:site_name" content=SITE_NAME />
        <Meta property="og:locale" content=og_locale(locale) />
        {og_alternates}
        <Meta property="og:title" content=move || title.get() />
        <Meta property="og:description" content=move || description.get() />
        <Meta property="og:url" content=url />
        <Meta property="og:image" content=image.clone() />
        <Meta property="og:image:type" content="image/png" />
        <Meta property="og:image:width" content="1200" />
        <Meta property="og:image:height" content="630" />
        <Meta property="og:image:alt" content=image_alt.clone() />
        <Meta name="twitter:card" content="summary_large_image" />
        <Meta name="twitter:title" content=move || title.get() />
        <Meta name="twitter:description" content=move || description.get() />
        <Meta name="twitter:image" content=image />
        <Meta name="twitter:image:alt" content=image_alt />
        {json_ld.map(|json| view! { <Script type_="application/ld+json">{json}</Script> })}
    }
}

/// The `text` of a `<Title>`, which every page's `<Title>` gets from here.
///
/// In the browser it is the closure, so the tab's title follows the page. On the server it is
/// the text the closure gives while the page renders. There `leptos_meta` keeps the closure and
/// calls it only after the body is rendered, to write `<title>` into the head
/// (`ServerMetaContextOutput::inject_meta_context`). `leptos_axum` runs that in its own future,
/// after a `tick().await`, outside the request's `Sandboxed` arena. A signal is looked up in the
/// arena that is current on the thread. When tokio moved the response to another worker during
/// that tick, the closure looked in that worker's arena, which belonged to another request or to
/// none. Its signals weren't there, and it panicked with "Tried to access a reactive value that
/// has already been disposed" before the response's headers went out, or it found the signals
/// of another request and gave that request's title. Meta tags, links and `<html>` attributes are
/// written while the body renders, inside the request's arena, so only the title needs this.
pub fn title_text(text: impl Fn() -> String + Send + Sync + 'static) -> TextProp {
    #[cfg(feature = "ssr")]
    {
        TextProp::from(untrack(text))
    }
    #[cfg(not(feature = "ssr"))]
    {
        TextProp::from(text)
    }
}

fn author() -> serde_json::Value {
    json!({
        "@type": "Person",
        "@id": format!("{SITE_URL}/#author"),
        "name": AUTHOR,
        "url": AUTHOR_URL,
    })
}

/// The website and the game itself, for the home page.
pub fn site_ld(locale: Locale) -> String {
    let lang = locale.as_str();
    json!({
        "@context": "https://schema.org",
        "@graph": [
            {
                "@type": "WebSite",
                "@id": format!("{SITE_URL}/#website"),
                "url": SITE_URL,
                "name": SITE_NAME,
                "description": td_string!(locale, common.ld.site),
                "inLanguage": lang,
                "publisher": { "@id": format!("{SITE_URL}/#author") },
            },
            {
                "@type": "VideoGame",
                "@id": format!("{SITE_URL}/#game"),
                "name": "Hashi – Hashiwokakero",
                "alternateName": ["Hashiwokakero", "Bridges", "橋をかけろ", "Hashi"],
                "url": format!("{SITE_URL}{}", localized(locale, "/")),
                "image": og_image(locale),
                "description": td_string!(locale, common.ld.game),
                "genre": ["Puzzle", "Logic"],
                "gamePlatform": "Web browser",
                "playMode": "SinglePlayer",
                "applicationCategory": "Game",
                "operatingSystem": "Any",
                "inLanguage": Locale::get_all().iter().map(|l| l.as_str()).collect::<Vec<_>>(),
                "isAccessibleForFree": true,
                "offers": { "@type": "Offer", "price": "0", "priceCurrency": "EUR" },
                "author": { "@id": format!("{SITE_URL}/#author") },
                "isBasedOn": "https://en.wikipedia.org/wiki/Hashiwokakero",
                "codeRepository": REPO_URL,
            },
            author(),
        ]
    })
    .to_string()
}

/// The questions on the rules page, also the source of its structured data.
pub fn rules_faq(locale: Locale) -> [(&'static str, &'static str); 5] {
    [
        (
            td_string!(locale, rules.faq.what_q),
            td_string!(locale, rules.faq.what_a),
        ),
        (
            td_string!(locale, rules.faq.how_many_q),
            td_string!(locale, rules.faq.how_many_a),
        ),
        (
            td_string!(locale, rules.faq.cross_q),
            td_string!(locale, rules.faq.cross_a),
        ),
        (
            td_string!(locale, rules.faq.connected_q),
            td_string!(locale, rules.faq.connected_a),
        ),
        (
            td_string!(locale, rules.faq.unique_q),
            td_string!(locale, rules.faq.unique_a),
        ),
    ]
}

/// Questions and answers, for the rules page.
pub fn faq_ld(locale: Locale) -> String {
    json!({
        "@context": "https://schema.org",
        "@type": "FAQPage",
        "inLanguage": locale.as_str(),
        "mainEntity": rules_faq(locale)
            .iter()
            .map(|(q, a)| json!({
                "@type": "Question",
                "name": q,
                "acceptedAnswer": { "@type": "Answer", "text": a },
            }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

/// The pages in the sitemap with their change frequency and priority.
const SITEMAP: [(&str, &str, &str); 9] = [
    ("/", "daily", "1.0"),
    ("/practice", "monthly", "0.8"),
    ("/ranks", "daily", "0.7"),
    ("/rules", "monthly", "0.8"),
    ("/create", "monthly", "0.6"),
    ("/lab", "monthly", "0.5"),
    ("/dojo", "daily", "0.7"),
    ("/dojo/seasons", "monthly", "0.4"),
    ("/dojo/benchmarks", "monthly", "0.4"),
];

/// `sitemap.xml` with every page in every language, each listing its translations.
pub fn sitemap() -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    for (path, freq, priority) in SITEMAP {
        let alternates: String = Locale::get_all()
            .iter()
            .map(|l| {
                format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{SITE_URL}{}\"/>\n",
                    l.as_str(),
                    localized(*l, path)
                )
            })
            .chain(std::iter::once(format!(
                "    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{SITE_URL}{path}\"/>\n"
            )))
            .collect();
        for l in Locale::get_all() {
            let _ = write!(
                xml,
                "  <url>\n    <loc>{SITE_URL}{}</loc>\n{alternates}    <changefreq>{freq}</changefreq>\n    <priority>{priority}</priority>\n  </url>\n",
                localized(*l, path)
            );
        }
    }
    xml.push_str("</urlset>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app opens these in the phone's browser or mail app, the rest in its `WebView`.
    #[test]
    fn links_to_other_sites_and_mail_leave_the_page() {
        let app = "https://tauri.localhost";
        for href in [
            "https://hashi.rs/dojo/seasons/3/results.csv",
            "https://play.google.com/store/apps/details?id=rs.hashi",
            "https://github.com/NoelHuibers/Hashiwokakero",
            "https://wa.me/?text=hi",
            "http://example.com/",
            "mailto:hello@hashi.rs",
            // Not the app's origin, only its name at the start
            "https://tauri.localhost.example/",
            "https://tauri.localhost:8443/rules",
        ] {
            assert!(opens_outside(href, app), "{href}");
        }
        for href in [
            "https://tauri.localhost",
            "https://tauri.localhost/",
            "https://tauri.localhost/ja/rules",
            "https://tauri.localhost?x=1",
            "https://tauri.localhost#top",
            "javascript:void(0)",
            "blob:https://tauri.localhost/0e7c",
            "",
        ] {
            assert!(!opens_outside(href, app), "{href}");
        }
        // On the website, its own pages stay
        assert!(!opens_outside("https://hashi.rs/rules", SITE_URL));
    }

    /// The links people share open their page in the app, in their language; other pages of the
    /// site and other sites don't.
    #[test]
    fn app_links_are_the_shared_pages() {
        for (url, page) in [
            ("https://hashi.rs/g/k7m2xq_9", "/g/k7m2xq_9"),
            ("https://hashi.rs/ja/g/k7m2xq_9", "/ja/g/k7m2xq_9"),
            ("https://hashi.rs/zh-Hant/g/abc", "/zh-Hant/g/abc"),
            // A daily's result, in English and in German
            ("https://hashi.rs/", "/"),
            ("https://hashi.rs", "/"),
            ("https://hashi.rs/de", "/de"),
            ("https://hashi.rs/practice", "/practice"),
            ("https://hashi.rs/ko/practice", "/ko/practice"),
            ("https://hashi.rs/groups", "/groups"),
            ("https://hashi.rs/groups/12", "/groups/12"),
            (
                "https://hashi.rs/fr/groups/12/replay/20400",
                "/fr/groups/12/replay/20400",
            ),
            (
                "https://hashi.rs/groups/12/puzzles/3",
                "/groups/12/puzzles/3",
            ),
            ("https://hashi.rs/player/42", "/player/42"),
            (
                "https://hashi.rs/player/42?from=ranks#record",
                "/player/42?from=ranks#record",
            ),
            ("https://hashi.rs?x=1", "/?x=1"),
        ] {
            assert_eq!(app_link(url).as_deref(), Some(page), "{url}");
        }
        for url in [
            "https://hashi.rs/admin",
            "https://hashi.rs/ja/admin",
            "https://hashi.rs/dojo",
            "https://hashi.rs/dojo/seasons/3/set.tar.gz",
            "https://hashi.rs/dojo/board.json",
            "https://hashi.rs/api/daily_today",
            "https://hashi.rs/og/en.png",
            "https://hashi.rs/rules",
            "https://hashi.rs/de/rules",
            "https://hashi.rs/kontakt",
            "https://hashi.rs/sitemap.xml",
            "https://hashi.rs/.well-known/assetlinks.json",
            "https://hashi.rs/replay/20400",
            // A language the site doesn't have, English spelled out
            "https://hashi.rs/xx/g/abc",
            "https://hashi.rs/en/g/abc",
            // The browser would make other pages of these
            "https://hashi.rs/g/../admin",
            "https://hashi.rs/g/%2e%2e/admin",
            "https://hashi.rs/g/..%2Fadmin",
            "https://hashi.rs/g/x\\..\\..\\admin",
            "https://hashi.rs//admin.example/g/x",
            "https://hashi.rs/practice/",
            "https://hashi.rs/gx",
            // Other sites, other schemes
            "https://hashi.rs.example/g/abc",
            "https://hashi.rs:8443/g/abc",
            "https://hashi.rs@example.com/g/abc",
            "http://hashi.rs/g/abc",
            "https://www.hashi.rs/g/abc",
            "https://tauri.localhost/g/abc",
            "hashi://g/abc",
            "",
        ] {
            assert_eq!(app_link(url), None, "{url}");
        }
    }

    /// Android's intent filter (`plugins.deep-link.mobile` of the app's tauri.conf.json, which
    /// tauri-plugin-deep-link writes into the manifest) lists the same pages as [`app_link`], in
    /// every language.
    #[test]
    fn the_app_claims_the_app_links() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../app/src-tauri/tauri.conf.json")).unwrap();
        let every_language = |paths: &[&str]| -> Vec<String> {
            paths
                .iter()
                .flat_map(|path| {
                    Locale::get_all()
                        .iter()
                        .map(|locale| localized(*locale, path))
                })
                .collect()
        };
        let expected = json!([{
            "scheme": ["https"],
            "host": "hashi.rs",
            "appLink": true,
            "path": every_language(APP_LINK_PATHS),
            "pathPrefix": every_language(APP_LINK_PREFIXES),
        }]);
        assert_eq!(
            conf["plugins"]["deep-link"]["mobile"],
            expected,
            "plugins.deep-link.mobile in app/src-tauri/tauri.conf.json should be\n{}",
            serde_json::to_string_pretty(&expected).unwrap()
        );
        // Each of them is an App Link
        for path in every_language(APP_LINK_PATHS) {
            assert_eq!(app_link(&format!("{SITE_URL}{path}")), Some(path.clone()));
        }
        for prefix in every_language(APP_LINK_PREFIXES) {
            let path = format!("{prefix}x1");
            assert_eq!(app_link(&format!("{SITE_URL}{path}")), Some(path.clone()));
        }
    }

    #[test]
    fn the_app_links_to_the_servers_files_on_the_website() {
        assert_eq!(server_link("/dojo/board.json"), "/dojo/board.json");
        assert_eq!(
            server_link_in(false, "/dojo-dev-2.tar.gz"),
            "/dojo-dev-2.tar.gz"
        );
        assert_eq!(
            server_link_in(true, "/dojo/seasons/3/results.csv"),
            "https://hashi.rs/dojo/seasons/3/results.csv"
        );
    }

    #[test]
    fn sitemap_lists_every_language() {
        let xml = sitemap();
        let pages = SITEMAP.len() * Locale::get_all().len();
        assert_eq!(xml.matches("<url>").count(), pages);
        assert!(xml.contains("<loc>https://hashi.rs/</loc>"));
        assert!(xml.contains("<loc>https://hashi.rs/de/rules</loc>"));
        assert!(xml.contains("hreflang=\"zh-Hant\" href=\"https://hashi.rs/zh-Hant/lab\""));
    }

    #[test]
    fn preview_images_per_language_and_for_the_home_page_per_day() {
        use crate::model::{LAUNCH_DAY, daily_number};

        assert_eq!(og_image(Locale::en), OG_IMAGE);
        assert_eq!(og_image(Locale::zh_Hant), "https://hashi.rs/og/zh-Hant.png");
        assert_eq!(
            og_daily_image(Locale::de, LAUNCH_DAY),
            "https://hashi.rs/og/daily/2026-10-17/de.png"
        );

        let (image, alt) = page_image(Locale::ja, "/rules");
        assert_eq!(image, og_image(Locale::ja));
        assert_eq!(alt, td_string!(Locale::ja, common.og_image_alt));
        // The home page is today's daily
        let day = today();
        let (image, alt) = page_image(Locale::fr, "/");
        assert_eq!(image, og_daily_image(Locale::fr, day));
        let title = td_string!(Locale::fr, daily.title, number = daily_number(day));
        assert!(alt.starts_with(&title), "{alt}");
        assert!(
            alt.ends_with(td_string!(Locale::fr, common.og_image_alt)),
            "{alt}"
        );
    }

    /// See [`title_text`]. The server reads a title after the arena of its page stopped being
    /// the thread's current one. Read through the closure, this panicked: "Tried to access a
    /// reactive value that has already been disposed".
    #[cfg(feature = "ssr")]
    #[test]
    fn titles_are_read_outside_the_arena_of_their_page() {
        let page = Owner::new_root(None);
        let text = page.with(|| {
            let name = RwSignal::new("aiko".to_string());
            title_text(move || format!("{} · Hashi", name.get()))
        });
        // Another request's owner: its arena becomes the thread's current one, as when another
        // request renders on this worker, or tokio moves the response to a worker that did
        let _other_request = Owner::new_root(None);
        assert_eq!(&*text.get(), "aiko · Hashi");
    }

    /// What `leptos_axum` does with a page: it renders the body inside the request's owner, then
    /// writes the head with the page's `<title>` outside it. Here on another thread, as when
    /// tokio moves the response to another worker in between.
    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn the_head_gets_the_title_of_the_page_in_its_language() {
        use futures::StreamExt as _;
        use leptos_meta::ServerMetaContext;

        use crate::i18n::t_string;

        // For the tasks leptos and leptos_i18n start
        _ = any_spawner::Executor::init_tokio();
        let (meta, head) = ServerMetaContext::new();
        let page = Owner::new_root(None);
        let body = page.with(|| {
            provide_context(meta);
            leptos_meta::provide_meta_context();
            crate::app::provide_server_i18n();
            let i18n = use_i18n();
            i18n.set_locale(Locale::ja);
            view! {
                <PageMeta
                    title=Signal::derive(move || t_string!(i18n, rules.meta.title).to_string())
                    description=Signal::derive(move || t_string!(i18n, rules.meta.description).to_string())
                    path="/rules"
                />
            }
            .to_html()
        });
        let page_html = format!("<!DOCTYPE html><html><head></head><body>{body}</body></html>");
        let html = std::thread::spawn(move || {
            let worker = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            worker.block_on(async move {
                let html = head.inject_meta_context(futures::stream::iter([page_html]));
                html.await.collect::<String>().await
            })
        })
        .join()
        .expect("writing the head panicked");
        let title = td_string!(Locale::ja, rules.meta.title);
        assert!(
            html.contains(&format!("<title>{title}</title></head>")),
            "{html}"
        );
        // The tags written with the body have it too
        assert!(
            html.contains(&format!("property=\"og:title\" content=\"{title}\"")),
            "{html}"
        );
        drop(page);
    }

    /// Every `<Title>` of the site gets its text from [`title_text`].
    #[test]
    fn every_title_goes_through_title_text() {
        fn sources(dir: &std::path::Path, found: &mut Vec<(std::path::PathBuf, String)>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, found);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    found.push((path, text));
                }
            }
        }
        let mut found = Vec::new();
        sources(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        let titles: Vec<_> = found
            .iter()
            .flat_map(|(path, text)| text.lines().map(move |line| (path, line.trim())))
            .filter(|(_, line)| line.starts_with("<Title "))
            .collect();
        // PageMeta, the 404 page, the account page and a player's page
        assert!(titles.len() >= 4, "{titles:?}");
        for (path, line) in titles {
            assert!(
                line.starts_with("<Title text=title_text("),
                "{}: {line}",
                path.display()
            );
        }
    }
}
