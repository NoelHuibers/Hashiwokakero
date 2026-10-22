//! Every page as the server renders it.
//!
//! Pages don't wait for data on the server: the visitor (`Auth::refresh`), the daily, the
//! boards, submissions and the admin's lists are all loaded in effects once the page is
//! hydrated. So the server sends every visitor the same page, the frame and whatever doesn't
//! depend on data, with "Loading…" where the data goes; the browser tests (e2e/) cover what fills
//! in. Except a player's page, rendered once the player is loaded, for the title and the preview
//! of links to it, and the daily's header, which the server renders for its day. These tests make
//! sure every route renders without panicking, in its language, with its title, headings, texts
//! and links.

use std::collections::BTreeSet;

use common::Difficulty;
use leptos_i18n::Locale as _;

use super::*;
use crate::api::{SendMessage, StartDaily, SubmitDaily};
use crate::i18n::{Locale, td_string};
use crate::lang::{difficulty_name, localized, unlocalized};
use crate::model::{CHECK_PENALTY_MS, HINT_PENALTY_MS, MoveEvent};
use crate::pages::daily::about::penalty;
use crate::pages::legal::CONTACT_EMAIL;
use crate::seo::{SITE_URL, og_image, rules_faq};
use crate::server::contact::{MAX_MESSAGE, MIN_MESSAGE};

/// Every route of the app with a URL to request it by.
const PAGES: [(&str, &str); 23] = [
    ("/", "/"),
    ("/practice", "/practice"),
    ("/ranks", "/ranks"),
    ("/groups", "/groups"),
    ("/groups/{id}", "/groups/1"),
    ("/groups/{id}/replay/{day}", "/groups/1/replay/20747"),
    ("/groups/{id}/puzzles/{puzzle}", "/groups/1/puzzles/1"),
    (
        "/groups/{id}/puzzles/{puzzle}/replay",
        "/groups/1/puzzles/1/replay",
    ),
    ("/replay/{day}", "/replay/20747"),
    ("/g/{code}", "/g/abcdefgh23"),
    // Nobody, user ids start at 1
    ("/player/{id}", "/player/0"),
    ("/account", "/account"),
    ("/create", "/create"),
    ("/lab", "/lab"),
    ("/dojo", "/dojo"),
    ("/admin", "/admin"),
    ("/dojo/seasons", "/dojo/seasons"),
    ("/dojo/benchmarks", "/dojo/benchmarks"),
    ("/dojo/{id}", "/dojo/1"),
    ("/rules", "/rules"),
    ("/impressum", "/impressum"),
    ("/datenschutz", "/datenschutz"),
    ("/kontakt", "/kontakt"),
];

/// English, one with a script font, one written right to left.
const LANGUAGES: [Locale; 4] = [Locale::en, Locale::de, Locale::ja, Locale::ar];

/// What a page has to show, whoever asks.
#[derive(Default)]
struct Expected {
    /// `None`: no title of its own.
    title: Option<String>,
    texts: Vec<String>,
    /// Texts that only the browser may show (the visitor's own things).
    absent: Vec<String>,
    /// The link in the header's navigation marked as the current page.
    nav: Option<&'static str>,
    /// The page search engines know it by; private pages aren't indexed.
    canonical: Option<&'static str>,
    /// 200 OK, unless the page shows something that isn't there.
    status: StatusCode,
}

fn strings<const N: usize>(texts: [&str; N]) -> Vec<String> {
    texts.into_iter().map(String::from).collect()
}

fn hashi(title: &str) -> String {
    format!("{title} · Hashi")
}

/// What each route shows before the browser takes over.
#[expect(clippy::too_many_lines, reason = "one arm per route, a table")]
fn expected(route: &str, l: Locale) -> Expected {
    let loading = td_string!(l, common.game.loading);
    match route {
        "/" => Expected {
            title: Some(td_string!(l, daily.meta.title).into()),
            texts: strings([
                // The board, the ranking and the result come with today's daily
                loading,
                td_string!(l, daily.penalty),
                td_string!(l, daily.ranking.title),
                td_string!(l, daily.ranking.all),
                td_string!(l, daily.week.title),
                td_string!(l, daily.week.text),
                td_string!(l, daily.week.next),
                // The introduction for search engines: the rules, the daily, the first tips
                td_string!(l, daily.about.title),
                td_string!(l, daily.about.how_title),
                td_string!(l, rules.rule.count_title),
                td_string!(l, rules.rule.connected),
                td_string!(l, daily.about.daily_title),
                td_string!(l, daily.about.day),
                td_string!(l, daily.about.give_up),
                td_string!(l, daily.about.tips_title),
                td_string!(l, rules.tips.corner_title),
                td_string!(l, rules.tips.ones),
                td_string!(l, rules.tips.count_title),
                td_string!(l, daily.about.faq_title),
                td_string!(l, rules.faq.unique_q),
                td_string!(l, daily.about.account_a),
                td_string!(l, daily.about.next_q),
                td_string!(l, daily.about.to_rules),
                td_string!(l, daily.about.to_practice),
            ])
            .into_iter()
            .chain([td_string!(
                l,
                daily.about.penalty,
                check = penalty(CHECK_PENALTY_MS),
                hint = penalty(HINT_PENALTY_MS)
            )])
            .collect(),
            // The game's buttons only once the daily is started (the text below the daily
            // names the give up button), and the tips only the rules page has
            absent: strings([
                td_string!(l, common.game.undo),
                td_string!(l, rules.tips.block_title),
                td_string!(l, rules.tips.connect_title),
            ]),
            nav: Some("/"),
            canonical: Some("/"),
            ..Expected::default()
        },
        "/practice" => Expected {
            title: Some(td_string!(l, practice.meta.title).into()),
            texts: [
                td_string!(l, practice.heading),
                td_string!(l, practice.generating),
                td_string!(l, practice.card.title),
                td_string!(l, practice.card.text),
                td_string!(l, practice.generate),
                td_string!(l, practice.legend.title),
                td_string!(l, practice.legend.build),
                td_string!(l, common.game.undo),
                td_string!(l, common.game.hint),
            ]
            .into_iter()
            .map(String::from)
            .chain(Difficulty::ALL.map(|d| difficulty_name(l, d)))
            .collect(),
            nav: Some("/practice"),
            canonical: Some("/practice"),
            ..Expected::default()
        },
        "/ranks" => Expected {
            title: Some(td_string!(l, ranks.meta.title).into()),
            texts: strings([
                td_string!(l, ranks.title),
                td_string!(l, ranks.intro),
                td_string!(l, ranks.earlier),
                td_string!(l, ranks.later),
                td_string!(l, ranks.month.title),
                td_string!(l, ranks.month.text),
                td_string!(l, ranks.fame.title),
                td_string!(l, ranks.fame.solved),
                td_string!(l, ranks.fame.medals),
                td_string!(l, ranks.fame.streaks),
                loading,
            ]),
            nav: Some("/ranks"),
            canonical: Some("/ranks"),
            ..Expected::default()
        },
        "/player/{id}" => Expected {
            title: Some(hashi(td_string!(l, common.not_found.title))),
            texts: strings([
                td_string!(l, ranks.player.back),
                td_string!(l, ranks.player.not_found),
            ]),
            absent: strings([td_string!(l, ranks.player.record)]),
            status: StatusCode::NOT_FOUND,
            // Profiles are opened from the leaderboards
            nav: Some("/ranks"),
            ..Expected::default()
        },
        // Whose groups they are is only known in the browser
        "/groups" => Expected {
            title: Some(hashi(td_string!(l, groups.title))),
            texts: strings([
                td_string!(l, groups.title),
                td_string!(l, groups.intro),
                td_string!(l, groups.list.title),
                td_string!(l, groups.list.text),
                td_string!(l, groups.create.title),
                td_string!(l, groups.create.name),
                td_string!(l, groups.create.button),
                td_string!(l, groups.create.invite_hint),
                loading,
            ]),
            absent: strings([
                td_string!(l, groups.ready.title),
                td_string!(l, groups.list.none),
            ]),
            nav: Some("/groups"),
            ..Expected::default()
        },
        // Only members see a group, the browser asks once it knows who's visiting
        "/groups/{id}" => Expected {
            title: Some(hashi(td_string!(l, groups.page.title))),
            texts: strings([td_string!(l, groups.page.back), loading]),
            absent: strings([
                td_string!(l, groups.today.title),
                td_string!(l, groups.page.not_found),
            ]),
            nav: Some("/groups"),
            ..Expected::default()
        },
        // The logs are only for members, and today's only once done: the browser asks
        "/groups/{id}/replay/{day}" => Expected {
            title: Some(hashi(td_string!(l, replay.title))),
            texts: strings([
                td_string!(l, replay.title),
                td_string!(l, groups.page.back),
                loading,
            ]),
            absent: strings([
                td_string!(l, replay.locked.title),
                td_string!(l, replay.legend.title),
            ]),
            nav: Some("/groups"),
            ..Expected::default()
        },
        // Only members see a group's puzzles, and the puzzle only once the clock runs: the browser
        // asks
        "/groups/{id}/puzzles/{puzzle}" => Expected {
            title: Some(hashi(td_string!(l, kadai.page.title))),
            texts: strings([td_string!(l, groups.page.back), loading]),
            absent: strings([
                td_string!(l, kadai.page.missing),
                td_string!(l, kadai.ranking.title),
            ]),
            nav: Some("/groups"),
            ..Expected::default()
        },
        "/groups/{id}/puzzles/{puzzle}/replay" => Expected {
            title: Some(hashi(td_string!(l, replay.title))),
            texts: strings([
                td_string!(l, replay.title),
                td_string!(l, kadai.replay.back),
                loading,
            ]),
            absent: strings([td_string!(l, replay.locked.title)]),
            nav: Some("/groups"),
            ..Expected::default()
        },
        "/replay/{day}" => Expected {
            title: Some(hashi(td_string!(l, replay.own_title))),
            texts: strings([
                td_string!(l, replay.own_title),
                td_string!(l, replay.page.back_daily),
                loading,
            ]),
            absent: strings([td_string!(l, replay.locked.title)]),
            ..Expected::default()
        },
        // The invite link joins nobody by being opened
        "/g/{code}" => Expected {
            title: Some(hashi(td_string!(l, groups.join.page_title))),
            texts: strings([loading]),
            absent: strings([
                td_string!(l, groups.join.button),
                td_string!(l, groups.join.invalid_title),
            ]),
            nav: Some("/groups"),
            ..Expected::default()
        },
        "/account" => Expected {
            title: Some(hashi(td_string!(l, account.title))),
            // Who is visiting is only known in the browser: everyone gets the sign in forms
            texts: strings([
                td_string!(l, account.title),
                td_string!(l, account.record),
                td_string!(l, account.no_record),
                td_string!(l, account.no_dailies),
                td_string!(l, account.sign_in_cta),
                td_string!(l, account.create),
                td_string!(l, common.sign_in),
                td_string!(l, account.username),
                td_string!(l, account.password),
                td_string!(l, account.password_hint),
            ]),
            absent: strings([
                td_string!(l, account.sign_out),
                td_string!(l, account.delete.title),
                td_string!(l, account.public_profile),
                "Admin dashboard",
            ]),
            ..Expected::default()
        },
        "/create" => Expected {
            title: Some(td_string!(l, create.meta.title).into()),
            texts: strings([
                td_string!(l, create.title),
                td_string!(l, create.intro),
                td_string!(l, create.grid),
                td_string!(l, create.width),
                td_string!(l, create.height),
                td_string!(l, create.fill),
                td_string!(l, create.import),
                td_string!(l, create.export),
                td_string!(l, create.play),
                td_string!(l, common.game.solve),
            ]),
            nav: Some("/create"),
            canonical: Some("/create"),
            ..Expected::default()
        },
        "/lab" => Expected {
            title: Some(td_string!(l, lab.meta.title).into()),
            texts: strings([
                td_string!(l, lab.title),
                td_string!(l, lab.intro),
                td_string!(l, lab.puzzle.title),
                td_string!(l, lab.puzzle.none),
                td_string!(l, lab.puzzle.game),
                td_string!(l, lab.puzzle.workshop),
                td_string!(l, lab.puzzle.fresh),
                td_string!(l, lab.solvers.title),
                td_string!(l, lab.solvers.timeout),
                td_string!(l, lab.start),
                td_string!(l, lab.results.title),
                td_string!(l, lab.results.empty),
            ]),
            nav: Some("/lab"),
            canonical: Some("/lab"),
            ..Expected::default()
        },
        // The Dojo is being reworked: its title, heading and the parts that wait for data
        "/dojo" => Expected {
            title: Some(td_string!(l, dojo.meta.title).into()),
            texts: strings([td_string!(l, dojo.title)]),
            // The dojo belongs to the lab
            nav: Some("/lab"),
            canonical: Some("/dojo"),
            ..Expected::default()
        },
        "/dojo/seasons" => Expected {
            title: Some(td_string!(l, dojo.seasons.meta_title).into()),
            texts: strings([
                td_string!(l, dojo.seasons.back),
                td_string!(l, dojo.seasons.title),
                loading,
            ]),
            nav: Some("/lab"),
            canonical: Some("/dojo/seasons"),
            ..Expected::default()
        },
        "/dojo/benchmarks" => Expected {
            title: Some(td_string!(l, dojo.community.meta_title).into()),
            texts: strings([
                td_string!(l, dojo.seasons.back),
                td_string!(l, dojo.community.title),
                td_string!(l, dojo.community.form_title),
                td_string!(l, dojo.community.pool_title),
                loading,
            ]),
            nav: Some("/lab"),
            canonical: Some("/dojo/benchmarks"),
            ..Expected::default()
        },
        "/dojo/{id}" => Expected {
            title: Some(td_string!(l, dojo.meta.title).into()),
            texts: strings([td_string!(l, dojo.detail.back), loading]),
            absent: strings([td_string!(l, dojo.detail.not_found)]),
            nav: Some("/lab"),
            canonical: Some("/dojo"),
            ..Expected::default()
        },
        // Decided in the browser once the session is known: the dashboard, or the "not found"
        // everyone else gets. Nothing gives it away before.
        "/admin" => Expected {
            title: None,
            texts: strings(["…"]),
            absent: strings([
                "Admin",
                "Daily review",
                "Messages",
                td_string!(l, common.not_found.title),
            ]),
            ..Expected::default()
        },
        "/rules" => Expected {
            title: Some(td_string!(l, rules.meta.title).into()),
            texts: strings([
                td_string!(l, rules.title),
                td_string!(l, rules.intro),
                td_string!(l, rules.rule.count_title),
                td_string!(l, rules.rule.double_title),
                td_string!(l, rules.rule.straight_title),
                td_string!(l, rules.rule.connected_title),
                td_string!(l, rules.example),
                td_string!(l, rules.tips.title),
                td_string!(l, rules.tips.intro),
                td_string!(l, rules.tips.corner_title),
                td_string!(l, rules.tips.corner),
                td_string!(l, rules.tips.ones_title),
                td_string!(l, rules.tips.ones),
                td_string!(l, rules.tips.count_title),
                td_string!(l, rules.tips.count),
                td_string!(l, rules.tips.block_title),
                td_string!(l, rules.tips.block),
                td_string!(l, rules.tips.connect_title),
                td_string!(l, rules.tips.connect),
                td_string!(l, rules.controls.title),
                td_string!(l, rules.hood.title),
                td_string!(l, rules.faq_title),
            ])
            .into_iter()
            // The questions, also in the page's structured data
            .chain(
                rules_faq(l)
                    .into_iter()
                    .flat_map(|(q, a)| [String::from(q), String::from(a)]),
            )
            .collect(),
            canonical: Some("/rules"),
            ..Expected::default()
        },
        // German law: the German text binds, the English one follows in every language
        "/impressum" => Expected {
            title: Some(hashi(td_string!(l, common.legal.imprint))),
            texts: strings([
                td_string!(l, common.legal.imprint),
                td_string!(l, common.legal.english),
                "Angaben gemäß § 5 DDG",
                "c/o Online-Impressum 12002",
                "53757 Sankt Augustin",
                "Information according to § 5 DDG",
                CONTACT_EMAIL,
            ]),
            canonical: Some("/impressum"),
            ..Expected::default()
        },
        "/datenschutz" => Expected {
            title: Some(hashi(td_string!(l, common.legal.privacy))),
            texts: strings([
                td_string!(l, common.legal.privacy),
                td_string!(l, common.legal.english),
                "Verantwortlicher",
                "Controller",
                CONTACT_EMAIL,
            ]),
            canonical: Some("/datenschutz"),
            ..Expected::default()
        },
        "/kontakt" => Expected {
            title: Some(hashi(td_string!(l, contact.title))),
            texts: strings([
                td_string!(l, contact.title),
                td_string!(l, contact.intro),
                td_string!(l, contact.name),
                td_string!(l, contact.email),
                td_string!(l, contact.email_hint),
                td_string!(l, contact.message),
                td_string!(l, contact.send),
                td_string!(l, contact.by_mail),
                CONTACT_EMAIL,
            ]),
            absent: strings([td_string!(l, contact.sent_title)]),
            canonical: Some("/kontakt"),
            ..Expected::default()
        },
        _ => panic!("{route}: what should it show?"),
    }
}

/// The route of a path from the route list: `/de/player/{id}` -> `/player/{id}`. English pages
/// have routes with and without `/en`.
fn route_of(path: &str) -> String {
    let path = unlocalized(path);
    match path.strip_prefix("/en") {
        Some("") => "/".into(),
        Some(rest) if rest.starts_with('/') => rest.into(),
        _ => path,
    }
}

/// What a reader sees of the page between header and footer: no tags, scripts or entities,
/// spaces collapsed.
fn visible_text(html: &str) -> String {
    let start = html.find("<main").expect("no <main>");
    let end = start + html[start..].find("</main>").expect("no </main>");
    let mut text = String::new();
    let mut rest = &html[start..end];
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = if rest.starts_with("<script") {
            rest.find("</script>").expect("unclosed <script>") + "</script>".len()
        } else {
            rest.find('>').expect("unclosed tag") + 1
        };
        rest = &rest[end..];
    }
    text.push_str(rest);
    collapse(&unescape(&text))
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

/// The text of the page's `<title>`.
fn title(html: &str) -> Option<String> {
    let start = html.find("<title>")? + "<title>".len();
    let end = start + html[start..].find("</title>")?;
    Some(unescape(&html[start..end]))
}

/// The `href` of the header's navigation link highlighted as the current page.
fn current_nav(html: &str) -> Option<&str> {
    let start = html.find("<header")?;
    let header = &html[start..start + html[start..].find("</header>")?];
    header
        .split("<a ")
        .map(|a| &a[..a.find('>').unwrap_or(a.len())])
        .filter(|attrs| attrs.contains("bg-primary text-primary-foreground shadow-sm"))
        .find_map(|attrs| attrs.strip_prefix("href=\"")?.split('"').next())
}

/// The `href`s of the links marked as the page the visitor is on (`aria-current="page"`).
fn current_links(html: &str) -> Vec<&str> {
    html.split("<a ")
        .skip(1)
        .map(|a| &a[..a.find('>').unwrap_or(a.len())])
        .filter(|attrs| attrs.contains(r#"aria-current="page""#))
        .filter_map(|attrs| attrs.strip_prefix("href=\"")?.split('"').next())
        .collect()
}

/// The page with the values of its `nonce` (and leptos' `crossorigin`, which repeats it)
/// blanked: they change with every response.
fn without_nonces(html: &str) -> String {
    let mut out = html.to_string();
    for attr in ["nonce=\"", "crossorigin=\""] {
        let mut from = 0;
        while let Some(found) = out[from..].find(attr) {
            let start = from + found + attr.len();
            let end = start + out[start..].find('"').unwrap();
            out.replace_range(start..end, "");
            from = start;
        }
    }
    out
}

/// The routes of the app, `App`'s own list, are the ones tested here.
#[tokio::test]
async fn every_route_is_tested() {
    let routes: BTreeSet<String> = leptos_axum::generate_route_list(crate::app::App)
        .iter()
        .map(|r| route_of(r.path()))
        .collect();
    let tested: BTreeSet<String> = PAGES.iter().map(|(route, _)| (*route).into()).collect();
    assert_eq!(
        routes, tested,
        "a route without a test, or a test without a route"
    );
}

#[tokio::test]
async fn every_page_is_rendered_on_the_server() {
    let server = Server::start().await;
    let browser = server.browser();
    for l in LANGUAGES {
        for (route, url) in PAGES {
            let path = localized(l, url);
            let res = browser.get(&path).await;
            let expected = expected(route, l);
            assert_status(&res, expected.status);
            let html = res.text().await.unwrap();

            let lang = format!("<html lang=\"{}\" dir=\"", l.as_str());
            assert!(html.contains(&lang), "{path}: {}", &html[..200]);
            let rtl = html.contains(&format!("{lang}rtl\""));
            assert_eq!(rtl, l == Locale::ar, "{path}");

            assert_eq!(title(&html), expected.title, "{path}");
            assert_eq!(
                current_nav(&html),
                expected.nav.map(|p| localized(l, p)).as_deref(),
                "{path}"
            );
            let canonical = expected.canonical.map(|p| {
                format!(
                    "<link href=\"{SITE_URL}{}\" rel=\"canonical\">",
                    localized(l, p)
                )
            });
            match &canonical {
                Some(link) => assert!(html.contains(link), "{path}: {link}"),
                None => assert!(!html.contains("rel=\"canonical\""), "{path}"),
            }
            // Private pages stay out of search engines
            assert_eq!(
                html.contains(r#"<meta name="robots" content="noindex""#),
                canonical.is_none(),
                "{path}"
            );

            // Marked as the current page: links here, and to the section the page is in. Every
            // page of a language is under its home page by path, but that isn't their section.
            let links = current_links(&html);
            for link in &links {
                let section = *link != localized(l, "/") && path.starts_with(&format!("{link}/"));
                assert!(
                    *link == path || section,
                    "{path}: {link} is marked as the current page"
                );
            }
            if expected.nav == Some(url) {
                assert!(links.contains(&path.as_str()), "{path}: {links:?}");
            }

            let text = visible_text(&html);
            for expected in &expected.texts {
                assert!(
                    text.contains(&collapse(expected)),
                    "{path}: {expected:?} in {text}"
                );
            }
            for absent in &expected.absent {
                assert!(
                    !text.contains(absent.as_str()),
                    "{path}: {absent:?} in {text}"
                );
            }
        }
    }
}

/// The parts of pages that don't show up as text.
#[tokio::test]
async fn pages_link_and_describe_themselves() {
    let server = Server::start().await;
    let browser = server.browser();
    let page = |path: &'static str| {
        let browser = &browser;
        async move { browser.get(path).await.text().await.unwrap() }
    };

    // Structured data for search engines
    let home = page("/de").await;
    assert!(home.contains(r#""@type":"VideoGame""#));
    assert!(home.contains(r#""inLanguage":"de""#));
    // The home page leads to the tips in its language
    assert!(home.contains(r#"href="/de/rules#tips""#));
    assert!(home.contains(r#"href="/de/practice""#));
    let rules = page("/ja/rules").await;
    assert!(rules.contains(r#""@type":"FAQPage""#));
    // The boards of the tips say what they show
    assert!(rules.contains(r#"id="tips""#));
    for label in [
        td_string!(Locale::ja, rules.tips.corner_board),
        td_string!(Locale::ja, rules.tips.connect_board),
    ] {
        assert!(
            rules.contains(&format!(r#"aria-label="{label}""#)),
            "{label}"
        );
    }
    assert!(rules.contains(&format!(r#""name":"{}""#, rules_faq(Locale::ja)[0].0)));
    // Every language of a page, for search engines to pick from
    for l in Locale::get_all() {
        let link = format!(
            r#"<link href="{SITE_URL}{}" hreflang="{}" rel="alternate">"#,
            localized(*l, "/rules"),
            l.as_str()
        );
        assert!(rules.contains(&link), "{link}");
    }
    assert!(rules.contains(&format!(
        r#"<link href="{SITE_URL}/rules" hreflang="x-default" rel="alternate">"#
    )));

    // Paths in the legal texts lead to the page in the reader's language
    let imprint = page("/de/impressum").await;
    assert!(imprint.contains(r#"href="/de/kontakt""#));
    assert!(imprint.contains(&format!(r#"href="mailto:{CONTACT_EMAIL}""#)));
    // And the legal texts say which language they are in
    assert!(imprint.contains(r#"<div lang="de" class="flex flex-col gap-5">"#));
    assert!(imprint.contains(r#"<div lang="en" class="flex flex-col gap-5">"#));

    // The contact form asks for what the server takes
    let contact = page("/kontakt").await;
    assert!(contact.contains(&format!(
        r#"minlength="{MIN_MESSAGE}" maxlength="{MAX_MESSAGE}""#
    )));
    // The field for bots, out of the way of people
    assert!(contact.contains(r#"<input id="contact-website" type="text" tabindex="-1""#));

    // Signing up asks for a password the server accepts, signing in takes any
    let account = page("/account").await;
    assert!(
        account.contains(
            r#"id="register-password" autocomplete="new-password" required minlength="8""#
        )
    );
    assert!(account.contains(r#"id="login-password" autocomplete="current-password" required"#));
}

#[tokio::test]
async fn unknown_pages_are_lost_at_sea_in_their_language() {
    let server = Server::start().await;
    let browser = server.browser();
    for l in LANGUAGES {
        let path = localized(l, "/no-such-page");
        let res = browser.get(&path).await;
        assert_status(&res, StatusCode::NOT_FOUND);
        let html = res.text().await.unwrap();
        assert!(
            html.contains(&format!("<html lang=\"{}\"", l.as_str())),
            "{path}"
        );
        assert_eq!(
            title(&html),
            Some(hashi(td_string!(l, common.not_found.title))),
            "{path}"
        );
        assert!(html.contains(r#"<meta name="robots" content="noindex""#));
        let text = visible_text(&html);
        assert!(
            text.contains(td_string!(l, common.not_found.text)),
            "{path}"
        );
        assert!(
            text.contains(td_string!(l, common.not_found.back)),
            "{path}"
        );
        // Back to the home page of the language, which isn't the page the visitor is on
        assert!(
            html.contains(&format!(
                "<a href=\"{}\" class=\"rounded-md",
                localized(l, "/")
            )),
            "{path}"
        );
        assert_eq!(current_links(&html), Vec::<&str>::new(), "{path}");
    }
}

/// One tap per bridge, 1.5 s apart, as a person would play.
fn played(solution: &[u8]) -> Vec<MoveEvent> {
    let mut log = Vec::new();
    for (edge, &count) in solution.iter().enumerate() {
        for c in 1..=count {
            log.push(MoveEvent {
                t: u32::try_from(log.len() + 1).unwrap() * 1500,
                edge: u16::try_from(edge).unwrap(),
                count: c,
                tap: true,
            });
        }
    }
    log
}

/// What the server renders doesn't depend on who asks: the browser starts every page as a
/// visitor nobody knows yet (`Auth::loaded` is false) and hydrates what the server sent, so a
/// page that showed the session's data on the server would fail to hydrate. With a guest who
/// started today's daily, a player who finished it (on the leaderboard, with a profile and
/// stamps) and an admin with a message waiting, every page is the one a new visitor gets.
#[tokio::test]
async fn every_visitor_gets_the_same_page() {
    let server = Server::start().await;
    let guest = server.browser();
    guest.call(StartDaily { device: None }).await.unwrap();

    let player = Browser::signed_up(&server, "seen_by_nobody").await;
    let day = player.call(StartDaily { device: None }).await.unwrap().day;
    let conn = conn().await;
    let id = user_id(&conn, "seen_by_nobody").await;
    // Played for ten minutes, not too fast to be ranked
    conn.execute(
        "UPDATE attempts SET started_at = started_at - 600000 WHERE user_id = ?1 AND day = ?2",
        params![id, day],
    )
    .await
    .unwrap();
    let solution = daily::get_or_create(&conn, day).await.unwrap().solution;
    let result = player
        .call(SubmitDaily {
            day,
            bridges: solution.clone(),
            moves: 1,
            log: played(&solution),
            utc_offset: None,
        })
        .await
        .unwrap();
    assert!(result.rank >= 1);

    let admin = Browser::admin(&server).await;
    server
        .browser()
        .call(SendMessage {
            name: "Kim".into(),
            email: String::new(),
            message: "The ranks page doesn't show my time.".into(),
            website: String::new(),
        })
        .await
        .unwrap();

    let nobody = server.browser();
    let profile = format!("/player/{id}");
    for (_, url) in PAGES {
        let url = if url == "/player/0" { &profile } else { url };
        for l in [Locale::en, Locale::ar] {
            let path = localized(l, url);
            let page = without_nonces(&nobody.get(&path).await.text().await.unwrap());
            // Only their public page names them, the same for everyone
            assert_eq!(page.contains("seen_by_nobody"), url == profile, "{path}");
            for (who, visitor) in [("guest", &guest), ("player", &player), ("admin", &admin)] {
                let seen = without_nonces(&visitor.get(&path).await.text().await.unwrap());
                assert!(seen == page, "{path} differs for the {who}");
            }
        }
    }
}

/// The text of the page's first `<h1>`.
fn heading(html: &str) -> Option<String> {
    let start = html.find("<h1")?;
    let end = start + html[start..].find("</h1>")?;
    Some(visible_text(&format!(
        "<main>{}</h1></main>",
        &html[start..end]
    )))
}

/// The `content` of a `<meta>` by its `property` or `name`.
fn meta(html: &str, key: &str) -> Option<String> {
    ["property", "name"].iter().find_map(|attr| {
        let tag = format!("<meta {attr}=\"{key}\" content=\"");
        let start = html.find(&tag)? + tag.len();
        let end = start + html[start..].find('"')?;
        Some(unescape(&html[start..end]))
    })
}

/// The home page's heading is today's daily, from the server's clock, before the daily is
/// loaded: search engines and link previews used to find no `<h1>` at all.
#[tokio::test]
async fn the_home_page_is_headed_by_todays_daily() {
    use crate::lang::date_long;
    use crate::model::{WEEKDAY_KANJI, daily_difficulty, daily_number, weekday};

    let server = Server::start().await;
    let browser = server.browser();
    for l in [Locale::en, Locale::de, Locale::ja] {
        let before = daily::today();
        let html = browser.get(&localized(l, "/")).await.text().await.unwrap();
        // At midnight the page may have been rendered on either day
        let day = (before..=daily::today())
            .find(|&day| {
                heading(&html) == Some(td_string!(l, daily.title, number = daily_number(day)))
            })
            .unwrap_or_else(|| panic!("{l:?}: {:?}", heading(&html)));
        let text = visible_text(&html);
        let difficulty = daily_difficulty(day);
        let (width, height) = difficulty.size();
        for shown in [
            date_long(l, day),
            WEEKDAY_KANJI[weekday(day)].to_string(),
            format!("{} {}", difficulty.kanji(), difficulty_name(l, difficulty)),
            format!("{width}×{height}"),
        ] {
            assert!(text.contains(&shown), "{l:?}: {shown:?} in {text}");
        }
        // The title stays the same every day
        assert_eq!(
            title(&html).as_deref(),
            Some(td_string!(l, daily.meta.title))
        );
    }
}

/// A link to a player's page shows their name and record where it's shared. The page stays
/// out of search engines.
#[tokio::test]
async fn a_players_page_names_them_in_link_previews() {
    let server = Server::start().await;
    Browser::signed_up(&server, "shared_link").await;
    let id = user_id(&conn().await, "shared_link").await;
    let browser = server.browser();
    for l in [Locale::en, Locale::de] {
        let path = localized(l, &format!("/player/{id}"));
        let res = browser.get(&path).await;
        assert_status(&res, StatusCode::OK);
        let html = res.text().await.unwrap();
        let name = "shared_link · Hashi";
        assert_eq!(title(&html).as_deref(), Some(name), "{path}");
        assert_eq!(meta(&html, "og:title").as_deref(), Some(name), "{path}");
        assert_eq!(
            meta(&html, "twitter:title").as_deref(),
            Some(name),
            "{path}"
        );
        let description = format!(
            "{} {}",
            td_string!(l, ranks.player.preview, name = "shared_link"),
            td_string!(l, account.no_dailies)
        );
        for key in ["description", "og:description", "twitter:description"] {
            assert_eq!(meta(&html, key), Some(description.clone()), "{path}: {key}");
        }
        assert_eq!(meta(&html, "og:type").as_deref(), Some("profile"));
        assert_eq!(meta(&html, "og:url"), Some(format!("{SITE_URL}{path}")));
        assert_eq!(meta(&html, "og:image"), Some(og_image(l)), "{path}");
        assert_eq!(
            meta(&html, "og:image:alt").as_deref(),
            Some(td_string!(l, common.og_image_alt)),
            "{path}"
        );
        assert_eq!(
            meta(&html, "twitter:card").as_deref(),
            Some("summary_large_image")
        );
        assert_eq!(meta(&html, "robots").as_deref(), Some("noindex"));
        assert!(!html.contains("rel=\"canonical\""), "{path}");
        // And the page itself, without waiting for the browser
        assert_eq!(heading(&html).as_deref(), Some("shared_link"), "{path}");
        assert!(
            visible_text(&html).contains(td_string!(l, ranks.player.record)),
            "{path}"
        );
    }
}
