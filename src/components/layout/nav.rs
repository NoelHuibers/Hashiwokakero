//! Navigation between the pages: the links in the header and the tab bar on phones.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use super::use_localized;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::unlocalized;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Daily,
    Practice,
    Ranks,
    Create,
    Lab,
}

/// Path and the decorative kanji of each page in the navigation.
pub(super) const NAV: [(Page, &str, &str); 5] = [
    (Page::Daily, "/", "今日"),
    (Page::Practice, "/practice", "練習"),
    (Page::Ranks, "/ranks", "順位"),
    (Page::Create, "/create", "作る"),
    (Page::Lab, "/lab", "研究"),
];

#[component]
fn PageName(page: Page) -> impl IntoView {
    let i18n = use_i18n();
    match page {
        Page::Daily => t!(i18n, common.nav.daily).into_any(),
        Page::Practice => t!(i18n, common.nav.practice).into_any(),
        Page::Ranks => t!(i18n, common.nav.ranks).into_any(),
        Page::Create => t!(i18n, common.nav.create).into_any(),
        Page::Lab => t!(i18n, common.nav.lab).into_any(),
    }
}

/// Whether the current location (in any language) is `href`.
fn is_active(href: &str, path: &str) -> bool {
    let path = unlocalized(path);
    match href {
        "/" => path == "/",
        // The dojo belongs to the lab
        "/lab" => path.starts_with("/lab") || path.starts_with("/dojo"),
        // Profiles are opened from the leaderboards
        "/ranks" => path.starts_with("/ranks") || path.starts_with("/player"),
        _ => path.starts_with(href),
    }
}

/// Tab bar at the bottom of the screen on phones, the header has no room for five links.
#[component]
pub fn MobileNav() -> impl IntoView {
    let i18n = use_i18n();
    let location = use_location();
    view! {
        <nav
            class="fixed inset-x-0 bottom-0 z-40 border-t border-border/70 bg-background/90 pb-[env(safe-area-inset-bottom)] backdrop-blur-md md:hidden"
            aria-label=move || t_string!(i18n, common.nav.main)
        >
            <div class="mx-auto grid max-w-md grid-cols-5">
                {NAV
                    .into_iter()
                    .map(|(page, href, jp)| {
                        let active = move || is_active(href, &location.pathname.get());
                        let to = use_localized(href);
                        view! {
                            <A
                                href=to
                                attr:class=move || {
                                    format!(
                                        "flex min-h-14 flex-col items-center justify-center gap-0.5 transition-colors {}",
                                        if active() { "text-primary" } else { "text-foreground/60" },
                                    )
                                }
                            >
                                <span class=move || {
                                    format!(
                                        "grid h-7 min-w-11 place-items-center rounded-full px-2 font-pixel text-base leading-none transition-colors {}",
                                        if active() { "bg-primary text-primary-foreground" } else { "" },
                                    )
                                }>{jp}</span>
                                <span class="max-w-full truncate px-0.5 text-[10px] font-bold uppercase tracking-wider">
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
        <A
            href=to
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
            <span class="text-[9px] font-bold uppercase tracking-[0.12em] opacity-80 sm:text-[10px] sm:tracking-[0.18em]">
                <PageName page />
            </span>
        </A>
    }
}
