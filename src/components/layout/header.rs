//! The header: logo, navigation and the buttons for rules, language, settings and account.

use leptos::prelude::*;
use leptos_router::components::A;

use super::language::LanguagePicker;
use super::nav::{NAV, NavLink};
use super::settings::SettingsButton;
use super::{Hanko, use_localized};
use crate::components::leaderboard::AccountButton;
use crate::i18n::{t, t_string, use_i18n};
use crate::state::Settings;

#[component]
pub fn Header() -> impl IntoView {
    let i18n = use_i18n();
    let home = use_localized("/");
    view! {
        <header class="sticky top-0 z-40 border-b border-border/70 bg-background/85 pt-(--safe-top) backdrop-blur-md">
            // Every button is 44 px wide below lg (fingers), and they all fit 320 px in every language:
            // the narrowest phones get less room around and between them (see header.spec.ts)
            <div class="mx-auto flex max-w-7xl items-center gap-1 px-2 py-2.5 min-[360px]:gap-2 min-[360px]:px-3 sm:gap-3 sm:px-6">
                <A
                    href=home
                    exact=true
                    attr:class="flex items-center gap-3"
                    attr:aria-label=move || format!("Hashi · {}", t_string!(i18n, common.nav.daily))
                >
                    <Hanko class="size-11 shrink-0 text-xl sm:text-2xl" />
                    <span class="hidden flex-col leading-none sm:flex">
                        <span class="font-pixel text-2xl tracking-wide">"Hashi"</span>
                        <span class="text-[11px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
                            "橋をかけろ · " {t!(i18n, common.tagline_short)}
                        </span>
                    </span>
                </A>
                <span class="flex flex-col leading-none sm:hidden">
                    <span class="font-pixel text-xl tracking-wide">"Hashi"</span>
                    <span class="text-[10px] font-bold tracking-[0.18em] text-muted-foreground max-[360px]:tracking-normal">"橋をかけろ"</span>
                </span>
                // From lg on: six links with their names don't fit next to the buttons on a
                // tablet in most languages, it has the tab bar
                <nav
                    class="ms-auto hidden items-center gap-1.5 lg:flex"
                    aria-label=move || t_string!(i18n, common.nav.main)
                >
                    {NAV.into_iter().map(|(page, href, jp)| view! { <NavLink page href jp /> }).collect_view()}
                </nav>
                <span class="ms-auto lg:hidden"></span>
                <HelpButton />
                <LanguagePicker />
                <SettingsButton />
                <ThemeToggle />
                <AccountButton />
            </div>
        </header>
    }
}

#[component]
pub fn ThemeToggle() -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    view! {
        <button
            class="hidden size-11 shrink-0 place-items-center rounded-lg border bg-card text-lg lg:size-10 transition-transform hover:-translate-y-0.5 active:translate-y-0 sm:grid"
            data-tooltip=move || {
                if settings.dark.get() {
                    t_string!(i18n, common.theme.to_day)
                } else {
                    t_string!(i18n, common.theme.to_night)
                }
            }
            aria-label=move || t_string!(i18n, common.theme.toggle)
            on:click=move |_| settings.dark.update(|d| *d = !*d)
        >
            <span class="font-pixel leading-none">{move || if settings.dark.get() { "夜" } else { "昼" }}</span>
        </button>
    }
}

/// The rules, reachable from every page (on phones the footer link is far down).
#[component]
pub fn HelpButton() -> impl IntoView {
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    view! {
        <A
            href=rules
            attr:class="grid size-11 shrink-0 place-items-center rounded-lg border bg-card font-pixel lg:size-10 text-lg leading-none transition-transform hover:-translate-y-0.5"
            attr:data-tooltip=move || t_string!(i18n, common.footer.how_to_play)
            attr:aria-label=move || t_string!(i18n, common.footer.how_to_play)
        >
            "?"
        </A>
    }
}
