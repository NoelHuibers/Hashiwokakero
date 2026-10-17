//! Metadata for search engines and social previews: one [`PageMeta`] per route, plus the
//! JSON-LD structured data that describes the game and the rules.

use leptos::prelude::*;
use leptos_meta::{Link, Meta, Script, Title};
use serde_json::json;

pub const SITE_URL: &str = "https://hashi.rs";
pub const SITE_NAME: &str = "Hashi";
pub const AUTHOR: &str = "Noel Huibers";
pub const AUTHOR_URL: &str = "https://github.com/NoelHuibers";
pub const REPO_URL: &str = "https://github.com/NoelHuibers/Hashiwokakero";
pub const OG_IMAGE: &str = "https://hashi.rs/og.png";
const OG_IMAGE_ALT: &str =
    "A solved Hashiwokakero puzzle in pixel art: numbered islands joined by wooden and vermilion bridges";

/// Title, description, canonical URL, Open Graph and Twitter tags of a page.
#[component]
pub fn PageMeta(
    /// Shown in the tab and as the search result title, keep it under 60 characters.
    title: &'static str,
    /// The search result snippet, keep it under 160 characters.
    description: &'static str,
    /// Path of the page, e.g. `/rules`.
    path: &'static str,
    /// JSON-LD structured data, see [`site_ld`] and [`faq_ld`].
    #[prop(optional)]
    json_ld: Option<String>,
) -> impl IntoView {
    let url = format!("{SITE_URL}{path}");
    view! {
        <Title text=title />
        <Meta name="description" content=description />
        <Link rel="canonical" href=url.clone() />
        <Meta property="og:type" content="website" />
        <Meta property="og:site_name" content=SITE_NAME />
        <Meta property="og:locale" content="en_US" />
        <Meta property="og:title" content=title />
        <Meta property="og:description" content=description />
        <Meta property="og:url" content=url />
        <Meta property="og:image" content=OG_IMAGE />
        <Meta property="og:image:width" content="1200" />
        <Meta property="og:image:height" content="630" />
        <Meta property="og:image:alt" content=OG_IMAGE_ALT />
        <Meta name="twitter:card" content="summary_large_image" />
        <Meta name="twitter:title" content=title />
        <Meta name="twitter:description" content=description />
        <Meta name="twitter:image" content=OG_IMAGE />
        <Meta name="twitter:image:alt" content=OG_IMAGE_ALT />
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
pub fn site_ld() -> String {
    json!({
        "@context": "https://schema.org",
        "@graph": [
            {
                "@type": "WebSite",
                "@id": format!("{SITE_URL}/#website"),
                "url": SITE_URL,
                "name": SITE_NAME,
                "description": "Hashiwokakero, the Japanese bridges puzzle, playable for free in the browser.",
                "inLanguage": "en",
                "publisher": { "@id": format!("{SITE_URL}/#author") },
            },
            {
                "@type": "VideoGame",
                "@id": format!("{SITE_URL}/#game"),
                "name": "Hashi – Hashiwokakero",
                "alternateName": ["Hashiwokakero", "Bridges", "橋をかけろ", "Hashi"],
                "url": SITE_URL,
                "image": OG_IMAGE,
                "description": "Connect the islands with bridges so that every island has as many bridges as its number says. Five difficulties, pixel art, and every puzzle has exactly one solution.",
                "genre": ["Puzzle", "Logic"],
                "gamePlatform": "Web browser",
                "playMode": "SinglePlayer",
                "applicationCategory": "Game",
                "operatingSystem": "Any",
                "inLanguage": "en",
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

/// Questions and answers, for the rules page.
pub fn faq_ld(items: &[(&str, &str)]) -> String {
    json!({
        "@context": "https://schema.org",
        "@type": "FAQPage",
        "mainEntity": items
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

/// The questions on the rules page, also the source of its structured data.
pub const RULES_FAQ: [(&str, &str); 5] = [
    (
        "What is Hashiwokakero?",
        "Hashiwokakero (橋をかけろ, “build bridges”), also called Bridges or Hashi, is a Japanese logic puzzle published by Nikoli. The grid shows numbered islands; you connect them with horizontal and vertical bridges so that every island has exactly as many bridges as its number, no bridges cross, and all islands form one connected group.",
    ),
    (
        "How many bridges can connect two islands?",
        "At most two. A single bridge or a double bridge may join two islands that are in the same row or column with nothing in between.",
    ),
    (
        "Can bridges cross each other?",
        "No. Bridges run straight horizontally or vertically and may neither cross other bridges nor pass over islands.",
    ),
    (
        "Do all islands have to be connected?",
        "Yes. In a finished puzzle you can travel from any island to any other over the bridges, the islands form a single network.",
    ),
    (
        "Is there always exactly one solution?",
        "On Hashi, yes: every generated puzzle is checked by a SAT solver, which proves that no second solution exists, so you never have to guess.",
    ),
];
