use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use super::leaderboard::AccountButton;
use super::sprites::{BOAT, PixelSprite};
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

#[component]
fn NavLink(href: &'static str, jp: &'static str, label: &'static str) -> impl IntoView {
    let location = use_location();
    let active = move || {
        let path = location.pathname.get();
        if href == "/" { path == "/" } else { path.starts_with(href) }
    };
    view! {
        <A
            href=href
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
                {label}
            </span>
        </A>
    }
}

#[component]
pub fn ThemeToggle() -> impl IntoView {
    let settings = expect_context::<Settings>();
    view! {
        <button
            class="grid size-10 shrink-0 place-items-center rounded-lg border bg-card text-lg transition-transform hover:-translate-y-0.5 active:translate-y-0"
            title=move || if settings.dark.get() { "Switch to day 昼" } else { "Switch to night 夜" }
            aria-label="Toggle dark mode"
            on:click=move |_| settings.dark.update(|d| *d = !*d)
        >
            <span class="font-pixel leading-none">{move || if settings.dark.get() { "夜" } else { "昼" }}</span>
        </button>
    }
}

#[component]
pub fn Header() -> impl IntoView {
    view! {
        <header class="sticky top-0 z-40 border-b border-border/70 bg-background/85 backdrop-blur-md">
            <div class="mx-auto flex max-w-7xl items-center gap-2 px-3 py-2.5 sm:gap-3 sm:px-6">
                <A href="/" attr:class="flex items-center gap-3">
                    <Hanko class="size-10 shrink-0 text-xl sm:size-11 sm:text-2xl" />
                    <span class="hidden flex-col leading-none sm:flex">
                        <span class="font-pixel text-2xl tracking-wide">"Hashi"</span>
                        <span class="text-[11px] font-bold tracking-[0.2em] text-muted-foreground">
                            "橋をかけろ · BRIDGES"
                        </span>
                    </span>
                </A>
                <nav class="-my-1 ml-auto flex items-center gap-0.5 overflow-x-auto py-1 sm:gap-1.5">
                    <NavLink href="/" jp="今日" label="Daily" />
                    <NavLink href="/practice" jp="練習" label="Practice" />
                    <NavLink href="/ranks" jp="順位" label="Ranks" />
                    <NavLink href="/create" jp="作る" label="Create" />
                    <NavLink href="/lab" jp="研究" label="Lab" />
                </nav>
                <AccountButton />
                <ThemeToggle />
            </div>
        </header>
    }
}

#[component]
pub fn Footer() -> impl IntoView {
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
                        <span class="text-foreground">"build bridges"</span>
                        ". Solved by SAT, played by you."
                    </p>
                </div>
                <p class="flex flex-wrap items-center gap-x-1.5 sm:justify-end">
                    <span>
                        "Made by "
                        <a
                            class="font-semibold text-foreground underline decoration-primary/50 underline-offset-4 hover:text-primary"
                            href=crate::seo::AUTHOR_URL
                            rel="author noopener"
                            target="_blank"
                        >
                            {crate::seo::AUTHOR}
                        </a>
                    </span>
                    <span aria-hidden="true">"·"</span>
                    <A href="/rules" attr:class="underline decoration-primary/50 underline-offset-4 hover:text-foreground">
                        "How to play 遊び方"
                    </A>
                    <span aria-hidden="true">"·"</span>
                    <a
                        class="underline decoration-primary/50 underline-offset-4 hover:text-foreground"
                        href=crate::seo::REPO_URL
                        target="_blank"
                        rel="noopener"
                    >
                        "Open source on GitHub"
                    </a>
                    <span aria-hidden="true">"·"</span>
                    <span>"Rust · Leptos · Turso · own CDCL & DPLL solvers"</span>
                </p>
            </div>
        </footer>
    }
}

#[component]
pub fn Toaster() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    view! {
        <div class="pointer-events-none fixed bottom-4 right-4 z-50 flex w-[min(92vw,22rem)] flex-col gap-2">
            <For
                each=move || toasts.0.get()
                key=|t| t.id
                children=move |t| {
                    let (mark, class) = match t.kind {
                        ToastKind::Success => ("◎", "border-success/40 bg-success-light text-success-dark"),
                        ToastKind::Error => ("✕", "border-destructive/40 bg-card text-destructive"),
                        ToastKind::Info => ("◇", "border-border bg-card text-foreground"),
                    };
                    view! {
                        <div class=format!(
                            "pointer-events-auto flex items-start gap-3 rounded-lg border px-4 py-3 text-sm shadow-lg animate-in fade-in slide-in-from-bottom-2 {class}",
                        )>
                            <span class="font-pixel">{mark}</span>
                            <span>{t.text}</span>
                        </div>
                    }
                }
            />
        </div>
    }
}
