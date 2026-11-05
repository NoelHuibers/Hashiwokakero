//! Navigation between the pages: the links in the header and the tab bar on phones.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use super::use_localized;
use crate::components::hanko::HankoDot;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::unlocalized;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Daily,
    Play,
    Ranks,
    Groups,
    Create,
    Lab,
}

/// Path and the decorative kanji of each page in the navigation: all of them in the header,
/// all but the Lab in the tab bar ([`TAB_BAR`]).
pub(super) const NAV: [(Page, &str, &str); 6] = [
    (Page::Daily, "/", "今日"),
    (Page::Play, "/play", "遊ぶ"),
    (Page::Ranks, "/ranks", "順位"),
    (Page::Groups, "/groups", "仲間"),
    (Page::Create, "/create", "作る"),
    (Page::Lab, "/lab", "研究"),
];

/// The pages of the tab bar on phones: the players'. The Lab is for people who write solvers;
/// on a phone the solver panels of Play and the workshop, the help menu, the rules and the footer
/// link to it.
pub(super) const TAB_BAR: [Page; 5] = [
    Page::Daily,
    Page::Play,
    Page::Ranks,
    Page::Groups,
    Page::Create,
];

#[component]
fn PageName(page: Page) -> impl IntoView {
    let i18n = use_i18n();
    match page {
        Page::Daily => t!(i18n, common.nav.daily).into_any(),
        Page::Play => t!(i18n, common.nav.play).into_any(),
        Page::Ranks => t!(i18n, common.nav.ranks).into_any(),
        Page::Groups => t!(i18n, common.nav.groups).into_any(),
        Page::Create => t!(i18n, common.nav.create).into_any(),
        Page::Lab => t!(i18n, common.nav.lab).into_any(),
    }
}

/// Whether the current location (in any language) is `href`.
fn is_active(href: &str, path: &str) -> bool {
    let path = unlocalized(path);
    match href {
        "/" => path == "/",
        // Its tabs and solvers, and the Dojo's old addresses on their way there
        "/lab" => in_section(&path, "/lab") || in_section(&path, "/dojo"),
        // Profiles are opened from the leaderboards
        "/ranks" => path.starts_with("/ranks") || path.starts_with("/player"),
        // Invite links lead into a group
        "/groups" => path.starts_with("/groups") || path.starts_with("/g/"),
        _ => in_section(&path, href),
    }
}

/// Whether `path` is `section` or a page below it, by whole segments: `/play` isn't `/player/3`.
fn in_section(path: &str, section: &str) -> bool {
    path == section
        || path
            .strip_prefix(section)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Tab bar at the bottom of the screen on phones and tablets, the header has no room for the
/// links. The players' five pages ([`TAB_BAR`]), not the Lab.
#[component]
pub fn MobileNav() -> impl IntoView {
    let i18n = use_i18n();
    let location = use_location();
    view! {
        <nav
            class="fixed inset-x-0 bottom-0 z-40 border-t border-border/70 bg-background/90 pr-(--safe-right) pb-(--safe-bottom) pl-(--safe-left) backdrop-blur-md lg:hidden"
            aria-label=move || t_string!(i18n, common.nav.main)
        >
            // Even columns, unless a name needs more room than its share: Indonesian's PERINGKAT
            // takes a little from the others rather than being cut off
            <div class="mx-auto grid max-w-md grid-cols-[repeat(5,minmax(min-content,1fr))]">
                {NAV
                    .into_iter()
                    .filter(|(page, ..)| TAB_BAR.contains(page))
                    .map(|(page, href, jp)| {
                        let active = move || is_active(href, &location.pathname.get());
                        let to = use_localized(href);
                        view! {
                            // See `NavLink`
                            <A
                                href=to
                                exact=href == "/"
                                attr:class=move || {
                                    format!(
                                        "flex min-h-14 flex-col items-center justify-center gap-0.5 transition-colors {}",
                                        if active() { "text-primary" } else { "text-foreground/60" },
                                    )
                                }
                            >
                                <span class=move || {
                                    format!(
                                        "relative grid h-7 min-w-11 place-items-center rounded-full px-2 font-pixel text-base leading-none transition-colors {}",
                                        if active() { "bg-primary text-primary-foreground" } else { "" },
                                    )
                                }>
                                    {jp}
                                    // New hanko on one's results
                                    {(page == Page::Groups).then(|| view! { <HankoDot class="absolute top-0 end-0.5" /> })}
                                </span>
                                // Smaller and without the spacing on the narrowest phones: five
                                // names like SPIELEN and GRUPPEN share 320 px (see tab-bar.spec.ts)
                                <span class="tab-label max-w-full truncate px-0.5 text-[10px] font-bold uppercase tracking-wider max-[360px]:px-0 max-[360px]:text-[9px] max-[360px]:tracking-normal">
                                    <PageName page />
                                </span>
                            </A>
                        }
                    })
                    .collect_view()}
            </div>
        </nav>
    }
}

#[component]
pub(super) fn NavLink(page: Page, href: &'static str, jp: &'static str) -> impl IntoView {
    let location = use_location();
    let active = move || is_active(href, &location.pathname.get());
    let to = use_localized(href);
    view! {
        // `A` marks a link as the current page (`aria-current`) when the location is under it:
        // the home page of a language, `/de`, would be the current page on every German page
        <A
            href=to
            exact=href == "/"
            attr:class=move || {
                format!(
                    "group relative flex flex-col items-center whitespace-nowrap rounded-lg px-2 py-1.5 transition-colors sm:px-3 {}",
                    if active() {
                        "bg-primary text-primary-foreground shadow-sm"
                    } else {
                        "text-foreground/75 hover:bg-secondary hover:text-foreground"
                    },
                )
            }
        >
            <span class="font-pixel text-sm leading-none sm:text-base">{jp}</span>
            {(page == Page::Groups).then(|| view! { <HankoDot class="absolute top-1 end-1" /> })}
            <span class="text-[9px] font-bold uppercase tracking-[0.12em] opacity-80 sm:text-[10px] sm:tracking-[0.18em]">
                <PageName page />
            </span>
        </A>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_light_up_their_section() {
        assert!(is_active("/groups", "/groups"));
        assert!(is_active("/groups", "/de/groups/12"));
        // An invite link leads into a group
        assert!(is_active("/groups", "/ja/g/abcdefgh23"));
        assert!(!is_active("/groups", "/ranks"));
        assert!(is_active("/ranks", "/player/3"));
        assert!(is_active("/", "/de"));
        assert!(!is_active("/", "/de/groups"));
        assert!(is_active("/play", "/play"));
        assert!(is_active("/play", "/ja/play"));
        // A profile belongs to the ranks, not to Play
        assert!(!is_active("/play", "/player/3"));
        assert!(!is_active("/play", "/de/player/3"));
        assert!(is_active("/create", "/create"));
    }

    /// The Lab lights up on its tabs, its solvers' pages and the Dojo's old addresses.
    #[test]
    fn the_lab_covers_its_tabs() {
        for path in [
            "/lab",
            "/de/lab",
            "/lab/leaderboard",
            "/ja/lab/submit",
            "/lab/benchmarks",
            "/zh-Hant/lab/seasons",
            "/lab/solver/12",
            "/ar/lab/solver/12",
            "/dojo",
            "/de/dojo/seasons",
            "/dojo/12",
        ] {
            assert!(is_active("/lab", path), "{path}");
        }
        for path in ["/", "/labs", "/dojox", "/play", "/rules", "/de/create"] {
            assert!(!is_active("/lab", path), "{path}");
        }
        for (_, href, _) in NAV.into_iter().filter(|(_, href, _)| *href != "/lab") {
            assert!(!is_active(href, "/lab/leaderboard"), "{href}");
        }
    }

    /// The tab bar has the players' pages, in the header's order, but the Lab.
    #[test]
    fn the_tab_bar_leaves_out_the_lab() {
        let pages: Vec<Page> = NAV.into_iter().map(|(page, ..)| page).collect();
        let bar: Vec<Page> = pages
            .iter()
            .copied()
            .filter(|p| TAB_BAR.contains(p))
            .collect();
        assert_eq!(bar, TAB_BAR);
        assert!(!TAB_BAR.contains(&Page::Lab));
        assert!(pages.contains(&Page::Lab));
        let source = include_str!("nav.rs");
        assert!(source.contains(&format!("grid-cols-[repeat({},", TAB_BAR.len())));
    }
}
