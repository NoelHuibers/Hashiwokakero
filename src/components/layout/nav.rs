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
    Practice,
    Ranks,
    Groups,
    Create,
    Lab,
}

/// Path and the decorative kanji of each page in the navigation.
pub(super) const NAV: [(Page, &str, &str); 6] = [
    (Page::Daily, "/", "今日"),
    (Page::Practice, "/practice", "練習"),
    (Page::Ranks, "/ranks", "順位"),
    (Page::Groups, "/groups", "仲間"),
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
        // The dojo belongs to the lab
        "/lab" => path.starts_with("/lab") || path.starts_with("/dojo"),
        // Profiles are opened from the leaderboards
        "/ranks" => path.starts_with("/ranks") || path.starts_with("/player"),
        // Invite links lead into a group
        "/groups" => path.starts_with("/groups") || path.starts_with("/g/"),
        _ => path.starts_with(href),
    }
}

/// Tab bar at the bottom of the screen on phones, the header has no room for six links.
#[component]
pub fn MobileNav() -> impl IntoView {
    let i18n = use_i18n();
    let location = use_location();
    view! {
        <nav
            class="fixed inset-x-0 bottom-0 z-40 border-t border-border/70 bg-background/90 pr-(--safe-right) pb-(--safe-bottom) pl-(--safe-left) backdrop-blur-md md:hidden"
            aria-label=move || t_string!(i18n, common.nav.main)
        >
            // Even columns, unless a name needs more room than its share: Indonesian's PERINGKAT
            // takes a little from the others rather than being cut off
            <div class="mx-auto grid max-w-md grid-cols-[repeat(6,minmax(min-content,1fr))]">
                {NAV
                    .into_iter()
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
                                // Smaller and without the spacing on the narrowest phones: six
                                // names like PRACTICE and GROUPS share 320 px (see tab-bar.spec.ts)
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
    }
}
