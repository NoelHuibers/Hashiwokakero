use leptos::prelude::*;
use leptos_i18n::Locale as _;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use super::leaderboard::AccountButton;
use super::sprites::{BOAT, PixelSprite};
use super::ui::switch::Switch;
use crate::i18n::{Locale, t, t_string, use_i18n};
use crate::lang::{localized, native_name, unlocalized};
use crate::state::{Settings, ToastKind, Toasts};

/// The hanko 判子 seal with 橋 (bridge), used as logo.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
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
    match href {
        "/" => path == "/",
        // The dojo belongs to the lab
        "/lab" => path.starts_with("/lab") || path.starts_with("/dojo"),
        _ => path.starts_with(href),
    }
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
            class="hidden size-10 shrink-0 place-items-center rounded-lg border bg-card text-lg transition-transform hover:-translate-y-0.5 active:translate-y-0 sm:grid"
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

/// The rules, reachable from every page (on phones the footer link is far down).
#[component]
pub fn HelpButton() -> impl IntoView {
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    view! {
        <A
            href=rules
            attr:class="grid size-10 shrink-0 place-items-center rounded-lg border bg-card font-pixel text-lg leading-none transition-transform hover:-translate-y-0.5"
            attr:title=move || t_string!(i18n, common.footer.how_to_play)
            attr:aria-label=move || t_string!(i18n, common.footer.how_to_play)
        >
            "?"
        </A>
    }
}

/// Closes a pop-up on a click outside of `root` and on Escape (focus goes back to `button`).
fn use_dismiss(
    open: RwSignal<bool>,
    root: NodeRef<leptos::html::Div>,
    button: NodeRef<leptos::html::Button>,
) {
    let outside = window_event_listener(leptos::ev::pointerdown, move |ev| {
        if !open.get_untracked() {
            return;
        }
        let inside = root.get_untracked().is_some_and(|root| {
            ev.target()
                .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Node>(t).ok())
                .is_some_and(|t| root.contains(Some(&t)))
        });
        if !inside {
            open.set(false);
        }
    });
    let escape = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" && open.get_untracked() {
            open.set(false);
            if let Some(button) = button.get_untracked() {
                let _ = button.focus();
            }
        }
    });
    on_cleanup(move || {
        outside.remove();
        escape.remove();
    });
}

/// Lucide's "settings" cog.
const COG: &str = "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z";

/// Settings: day or night, sound effects and music; saved in the browser.
#[component]
pub fn SettingsButton() -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    let open = RwSignal::new(false);
    let root = NodeRef::<leptos::html::Div>::new();
    let button = NodeRef::<leptos::html::Button>::new();
    use_dismiss(open, root, button);
    view! {
        <div node_ref=root class="relative shrink-0">
            <button
                node_ref=button
                class="grid size-10 place-items-center rounded-lg border bg-card transition-transform hover:-translate-y-0.5"
                class:ring-2=move || open.get()
                class:ring-primary=move || open.get()
                aria-haspopup="true"
                aria-expanded=move || open.get().to_string()
                aria-label=move || t_string!(i18n, common.settings)
                title=move || t_string!(i18n, common.settings)
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <svg viewBox="0 0 24 24" class="size-5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d=COG />
                    <circle cx="12" cy="12" r="3" />
                </svg>
            </button>
            <Show when=move || open.get()>
                <div class="absolute end-0 top-full z-50 mt-2 flex w-64 flex-col gap-1 rounded-xl border bg-card p-3 shadow-2xl animate-in fade-in zoom-in-95">
                    <p class="pb-1 font-pixel text-xs text-muted-foreground">"設定 · " {t!(i18n, common.settings)}</p>
                    <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 text-sm font-medium">
                        <span>{t!(i18n, common.theme.day)} " / " {t!(i18n, common.theme.night)}</span>
                        <Switch
                            checked=settings.dark
                            thumb=Signal::derive(move || if settings.dark.get() { "夜" } else { "昼" })
                            aria_label=Signal::derive(move || t_string!(i18n, common.theme.toggle).to_string())
                        />
                    </label>
                    <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 text-sm font-medium">
                        {t!(i18n, common.audio.sound)}
                        <Switch
                            checked=settings.sound
                            aria_label=Signal::derive(move || t_string!(i18n, common.audio.sound).to_string())
                        />
                    </label>
                    <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 text-sm font-medium">
                        <span>
                            {t!(i18n, common.audio.music)}
                            <span class="block text-xs font-normal text-muted-foreground">{t!(i18n, common.audio.music_hint)}</span>
                        </span>
                        <Switch
                            checked=settings.music
                            aria_label=Signal::derive(move || t_string!(i18n, common.audio.music).to_string())
                        />
                    </label>
                </div>
            </Show>
        </div>
    }
}

/// The language picker: a button with the language code that opens a panel with every language
/// in its own script. The entries are links to the same page in that language, so they work
/// before the app is loaded and crawlers can follow them.
#[component]
pub fn LanguagePicker() -> impl IntoView {
    let i18n = use_i18n();
    let location = use_location();
    let open = RwSignal::new(false);
    let root = NodeRef::<leptos::html::Div>::new();
    let button = NodeRef::<leptos::html::Button>::new();

    use_dismiss(open, root, button);

    // The current page in another language, keeping the query
    let href = move |l: Locale| {
        format!(
            "{}{}",
            localized(l, &unlocalized(&location.pathname.get())),
            location
                .search
                .get()
                .is_empty()
                .then(String::new)
                .unwrap_or_else(|| format!("?{}", location.search.get()))
        )
    };
    // Short label: the language code, the two Chinese scripts by their own name
    let code = |l: Locale| match l.as_str() {
        "zh" => "简".to_string(),
        "zh-Hant" => "繁".to_string(),
        other => other.to_string(),
    };

    view! {
        <div node_ref=root class="relative shrink-0">
            <button
                node_ref=button
                class="grid h-10 min-w-10 place-items-center rounded-lg border bg-card px-2 text-xs font-bold uppercase transition-transform hover:-translate-y-0.5"
                class:ring-2=move || open.get()
                class:ring-primary=move || open.get()
                aria-haspopup="true"
                aria-expanded=move || open.get().to_string()
                aria-label=move || t_string!(i18n, common.language)
                title=move || t_string!(i18n, common.language)
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="flex items-center gap-1" aria-hidden="true">
                    <svg viewBox="0 0 24 24" class="size-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="10" />
                        <path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                    </svg>
                    <span class="hidden sm:inline">{move || code(i18n.get_locale())}</span>
                </span>
            </button>
            <Show when=move || open.get()>
                // Phones: across the screen below the header, the button isn't at the edge
                <div class="fixed inset-x-3 top-16 z-50 rounded-xl border bg-card p-2 shadow-2xl animate-in fade-in zoom-in-95 sm:absolute sm:inset-x-auto sm:end-0 sm:top-full sm:mt-2 sm:w-[26rem]">
                    <p class="px-2 pb-2 pt-1 font-pixel text-xs text-muted-foreground">
                        "言語 · " {t!(i18n, common.language)}
                    </p>
                    <ul class="grid max-h-[min(70vh,28rem)] grid-cols-2 gap-1 overflow-y-auto">
                        {Locale::get_all()
                            .iter()
                            .map(|l| {
                                let l = *l;
                                let current = move || i18n.get_locale() == l;
                                view! {
                                    <li>
                                        <a
                                            href=move || href(l)
                                            lang=l.as_str()
                                            hreflang=l.as_str()
                                            aria-current=move || current().then_some("true")
                                            class=move || {
                                                format!(
                                                    "flex min-h-10 items-center justify-between gap-2 rounded-lg px-3 py-2 text-sm transition-colors {}",
                                                    if current() {
                                                        "bg-primary text-primary-foreground"
                                                    } else {
                                                        "hover:bg-secondary"
                                                    },
                                                )
                                            }
                                            on:click=move |_| open.set(false)
                                        >
                                            <span class="truncate">{native_name(l)}</span>
                                            <span class="shrink-0 font-pixel text-[11px] uppercase opacity-70">{code(l)}</span>
                                        </a>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                </div>
            </Show>
        </div>
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
                <HelpButton />
                <LanguagePicker />
                <SettingsButton />
                <ThemeToggle />
            </div>
        </header>
    }
}

#[component]
pub fn Footer() -> impl IntoView {
    let i18n = use_i18n();
    let rules = use_localized("/rules");
    let impressum = use_localized("/impressum");
    let dojo = use_localized("/dojo");
    let privacy = use_localized("/datenschutz");
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
            <div class="@container pointer-events-none absolute inset-x-0 top-3 h-8" aria-hidden="true">
                <div class="drift absolute left-0 w-12">
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
                    <A href=dojo attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, dojo.title)} " 道場"
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
                    <span aria-hidden="true">"·"</span>
                    <A href=impressum attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, common.legal.imprint)}
                    </A>
                    <span aria-hidden="true">"·"</span>
                    <A href=privacy attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        {t!(i18n, common.legal.privacy)}
                    </A>
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
