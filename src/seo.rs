//! Metadata for search engines and social previews: one [`PageMeta`] per route, plus the
//! JSON-LD structured data that describes the game and the rules.

use std::fmt::Write as _;

use leptos::prelude::*;
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
        <Title text=move || title.get() />
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

    #[test]
    fn sitemap_lists_every_language() {
        let xml = sitemap();
        let pages = SITEMAP.len() * Locale::get_all().len();
        assert_eq!(xml.matches("<url>").count(), pages);
        assert!(xml.contains("<loc>https://hashi.rs/</loc>"));
        assert!(xml.contains("<loc>https://hashi.rs/de/rules</loc>"));
        assert!(xml.contains("hreflang=\"zh-Hant\" href=\"https://hashi.rs/zh-Hant/lab\""));
    }
}
