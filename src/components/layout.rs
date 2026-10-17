use leptos::prelude::*;
use leptos_i18n::Locale as _;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use super::leaderboard::AccountButton;
use super::sprites::{BOAT, PixelSprite};
use crate::i18n::{Locale, t, t_string, use_i18n};
use crate::lang::{localized, native_name, unlocalized};
use crate::state::{Settings, ToastKind, Toasts};

/// The hanko 判子 seal with 橋 (bridge), used as logo.
#[component]
pub fn Hanko(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <span
            class=format!(
                "hanko inline-grid place-items-center rounded-md font-pixel select-none rotate-[-6deg] {class}",
            )
            aria-hidden="true"
        >
            "橋"
        </span>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Daily,
    Practice,
    Ranks,
    Create,
    Lab,
}

/// Path and the decorative kanji of each page in the navigation.
const NAV: [(Page, &str, &str); 5] = [
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
    if href == "/" { path == "/" } else { path.starts_with(href) }
}

/// A link to `path` in the current language.
pub fn use_localized(path: &'static str) -> impl Fn() -> String + Copy + Send + Sync + 'static {
    let i18n = use_i18n();
    move || localized(i18n.get_locale(), path)
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
fn NavLink(page: Page, href: &'static str, jp: &'static str) -> impl IntoView {
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

#[component]
pub fn ThemeToggle() -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    view! {
        <button
            class="grid size-10 shrink-0 place-items-center rounded-lg border bg-card text-lg transition-transform hover:-translate-y-0.5 active:translate-y-0"
            title=move || {
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

/// The language picker: a small button with the language code, a native select on top of it
/// (accessible, and phones show their own picker).
#[component]
pub fn LanguagePicker() -> impl IntoView {
    let i18n = use_i18n();
    let on_change = move |ev| {
        let id = event_target_value(&ev);
        if let Some(locale) = Locale::get_all().iter().find(|l| l.as_str() == id) {
            i18n.set_locale(*locale);
        }
    };
    view! {
        <label class="relative grid h-10 min-w-10 shrink-0 cursor-pointer place-items-center rounded-lg border bg-card px-2 text-xs font-bold uppercase transition-transform hover:-translate-y-0.5">
            <span aria-hidden="true" class="flex items-center gap-1">
                <svg viewBox="0 0 24 24" class="size-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="12" cy="12" r="10" />
                    <path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                </svg>
                <span class="hidden sm:inline">
                    {move || i18n.get_locale().as_str().split('-').next().unwrap_or_default().to_string()}
                </span>
            </span>
            <span class="sr-only">{t!(i18n, common.language)}</span>
            <select
                class="absolute inset-0 cursor-pointer opacity-0"
                aria-label=move || t_string!(i18n, common.language)
                prop:value=move || i18n.get_locale().as_str()
                on:change=on_change
            >
                {Locale::get_all()
                    .iter()
                    .map(|l| {
                        let l = *l;
                        view! {
                            <option value=l.as_str() lang=l.as_str()>
                                {native_name(l)}
                            </option>
                        }
                    })
                    .collect_view()}
            </select>
        </label>
    }
}

#[component]
pub fn Header() -> impl IntoView {
    let i18n = use_i18n();
    let home = use_localized("/");
    view! {
        <header class="sticky top-0 z-40 border-b border-border/70 bg-background/85 backdrop-blur-md">
            <div class="mx-auto flex max-w-7xl items-center gap-2 px-3 py-2.5 sm:gap-3 sm:px-6">
                <A href=home attr:class="flex items-center gap-3">
                    <Hanko class="size-10 shrink-0 text-xl sm:size-11 sm:text-2xl" />
                    <span class="hidden flex-col leading-none sm:flex">
                        <span class="font-pixel text-2xl tracking-wide">"Hashi"</span>
                        <span class="text-[11px] font-bold uppercase tracking-[0.2em] text-muted-foreground">
                            "橋をかけろ · " {t!(i18n, common.tagline_short)}
                        </span>
                    </span>
                </A>
                <span class="flex flex-col leading-none sm:hidden">
                    <span class="font-pixel text-xl tracking-wide">"Hashi"</span>
                    <span class="text-[10px] font-bold tracking-[0.18em] text-muted-foreground">"橋をかけろ"</span>
                </span>
                <nav
                    class="ms-auto hidden items-center gap-1.5 md:flex"
                    aria-label=move || t_string!(i18n, common.nav.main)
                >
                    {NAV.into_iter().map(|(page, href, jp)| view! { <NavLink page href jp /> }).collect_view()}
                </nav>
                <span class="ms-auto md:hidden"></span>
                <AccountButton />
                <LanguagePicker />
                <ThemeToggle />
            </div>
        </header>
    }
}

#[component]
pub fn Footer() -> impl IntoView {
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    let author = move || {
        view! {
            <a
                class="font-semibold text-foreground underline decoration-primary/50 underline-offset-4 hover:text-primary"
                href=crate::seo::AUTHOR_URL
                rel="author noopener"
                target="_blank"
            >
                {crate::seo::AUTHOR}
            </a>
        }
    };
    view! {
        <footer class="relative mt-16 overflow-hidden border-t">
            <div class="seigaiha absolute inset-0 opacity-80" aria-hidden="true"></div>
            <div class="pointer-events-none absolute inset-x-0 top-3 h-8" aria-hidden="true">
                <div class="drift absolute w-12">
                    <PixelSprite art=&BOAT class="bob w-12" />
                </div>
            </div>
            <div class="relative mx-auto flex max-w-7xl flex-col gap-3 px-6 pb-8 pt-14 text-sm text-muted-foreground sm:flex-row sm:items-end sm:justify-between">
                <div class="flex items-center gap-3">
                    <Hanko class="size-8 text-base" />
                    <p>
                        "Hashiwokakero 橋をかけろ — "
                        {t!(i18n, common.footer.tagline, <em> = <span class="text-foreground" />)}
                    </p>
                </div>
                <p class="flex flex-wrap items-center gap-x-1.5 sm:justify-end">
                    <span>{t!(i18n, common.footer.made_by, author)}</span>
                    <span aria-hidden="true">"·"</span>
                    <A href=rules attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, common.footer.how_to_play)} " 遊び方"
                    </A>
                    <span aria-hidden="true">"·"</span>
                    <a
                        class="underline decoration-primary/50 underline-offset-4 hover:text-foreground"
                        href=crate::seo::REPO_URL
                        target="_blank"
                        rel="noopener"
                    >
                        {t!(i18n, common.footer.open_source)}
                    </a>
                    <span aria-hidden="true">"·"</span>
                    <span>{t!(i18n, common.footer.tech)}</span>
                </p>
            </div>
        </footer>
    }
}

#[component]
pub fn Toaster() -> impl IntoView {
    let i18n = use_i18n();
    let toasts = expect_context::<Toasts>();
    view! {
        <div class="pointer-events-none fixed bottom-[calc(4.5rem+env(safe-area-inset-bottom))] end-4 z-50 flex w-[min(92vw,22rem)] flex-col gap-2 md:bottom-4">
            <For
                each=move || toasts.0.get()
                key=|t| t.id
                children=move |t| {
                    let (mark, class) = match t.kind {
                        ToastKind::Success => ("◎", "border-success/40 bg-success-light text-success-dark"),
                        ToastKind::Error => ("✕", "border-destructive/40 bg-card text-destructive"),
                        ToastKind::Info => ("◇", "border-border bg-card text-foreground"),
                    };
                    let text = t.text.clone();
                    view! {
                        <div class=format!(
                            "pointer-events-auto flex items-start gap-3 rounded-lg border px-4 py-3 text-sm shadow-lg animate-in fade-in slide-in-from-bottom-2 {class}",
                        )>
                            <span class="font-pixel">{mark}</span>
                            <span>{move || text(i18n.get_locale())}</span>
                        </div>
                    }
                }
            />
        </div>
    }
}
