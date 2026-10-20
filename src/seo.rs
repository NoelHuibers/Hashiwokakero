//! Metadata for search engines and social previews: one [`PageMeta`] per route, plus the
//! JSON-LD structured data that describes the game and the rules.

use std::fmt::Write as _;

use leptos::prelude::*;
use leptos::text_prop::TextProp;
use leptos_i18n::Locale as _;
use leptos_meta::{Link, Meta, Script, Title};
use serde_json::json;

use crate::i18n::{Locale, t_string, td_string, use_i18n};
use crate::lang::{localized, og_locale};

pub const SITE_URL: &str = "https://hashi.rs";
pub const SITE_NAME: &str = "Hashi";
pub const AUTHOR: &str = "Noel Huibers";
pub const AUTHOR_URL: &str = "https://github.com/NoelHuibers";
pub const REPO_URL: &str = "https://github.com/NoelHuibers/Hashiwokakero";
pub const OG_IMAGE: &str = "https://hashi.rs/og.png";

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
    let image_alt = move || t_string!(i18n, common.og_image_alt).to_string();
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
        <Meta property="og:image" content=OG_IMAGE />
        <Meta property="og:image:width" content="1200" />
        <Meta property="og:image:height" content="630" />
        <Meta property="og:image:alt" content=image_alt />
        <Meta name="twitter:card" content="summary_large_image" />
        <Meta name="twitter:title" content=move || title.get() />
        <Meta name="twitter:description" content=move || description.get() />
        <Meta name="twitter:image" content=OG_IMAGE />
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
                "image": OG_IMAGE,
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
const SITEMAP: [(&str, &str, &str); 8] = [
    ("/", "daily", "1.0"),
    ("/practice", "monthly", "0.8"),
    ("/ranks", "daily", "0.7"),
    ("/rules", "monthly", "0.8"),
    ("/create", "monthly", "0.6"),
    ("/lab", "monthly", "0.5"),
    ("/dojo", "daily", "0.7"),
    ("/dojo/seasons", "monthly", "0.4"),
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

    #[test]
    fn sitemap_lists_every_language() {
        let xml = sitemap();
        let pages = SITEMAP.len() * Locale::get_all().len();
        assert_eq!(xml.matches("<url>").count(), pages);
        assert!(xml.contains("<loc>https://hashi.rs/</loc>"));
        assert!(xml.contains("<loc>https://hashi.rs/de/rules</loc>"));
        assert!(xml.contains("hreflang=\"zh-Hant\" href=\"https://hashi.rs/zh-Hant/lab\""));
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
