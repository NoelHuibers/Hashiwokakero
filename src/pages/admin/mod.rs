//! The admin dashboard: today at a glance, daily reviews, the Dojo and contact messages.
//!
//! Only for the usernames in `HASHI_ADMINS`; everyone else sees the usual "not found". English
//! only.

pub mod daily;
pub mod dojo;
pub mod messages;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Meta;
use leptos_router::hooks::use_query_map;

use crate::api::load_admin_overview;
use crate::lang::error_text;
use crate::model::AdminOverview;
use crate::pages::NotFound;
use crate::state::{Auth, ToastKind, toast};

use self::daily::DailyAdmin;
use self::dojo::DojoAdmin;
use self::messages::MessagesAdmin;

fn error(e: &ServerFnError) {
    let e = e.clone();
    toast(ToastKind::Error, move |l| error_text(l, &e));
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let auth = expect_context::<Auth>();
    // Only decided in the browser, once the session is known: the server renders a placeholder
    let mounted = RwSignal::new(false);
    Effect::new(move |_| mounted.set(true));
    let admin = move || mounted.get() && auth.user.with(|u| u.as_ref().is_some_and(|u| u.admin));
    view! {
        <Meta name="robots" content="noindex" />
        <Show
            when=admin
            fallback=move || {
                if mounted.get() && auth.loaded.get() {
                    view! { <NotFound /> }.into_any()
                } else {
                    view! { <p class="py-24 text-center text-muted-foreground">"…"</p> }.into_any()
                }
            }
        >
            <Dashboard />
        </Show>
    }
}

#[component]
fn Dashboard() -> impl IntoView {
    let query = use_query_map();
    let tab = RwSignal::new(match query.with_untracked(|q| q.get("tab")).as_deref() {
        Some("dojo") => "dojo",
        Some("messages") => "messages",
        _ => "daily",
    });
    let overview = RwSignal::new(None::<AdminOverview>);
    let load_overview = move || {
        spawn_local(async move {
            match load_admin_overview().await {
                Ok(o) => overview.set(Some(o)),
                Err(e) => error(&e),
            }
        });
    };
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            load_overview();
        }
    });
    // A segmented control, full width on phones; the count is what waits for a look
    let tab_button = move |id: &'static str,
                           label: &'static str,
                           count: fn(&AdminOverview) -> i64| {
        let count = move || overview.with(|o| o.as_ref().map_or(0, count));
        view! {
            <button
                type="button"
                role="tab"
                aria-selected=move || (tab.get() == id).to_string()
                class="group inline-flex h-9 items-center justify-center gap-2 rounded-md px-4 text-sm font-medium transition-colors aria-selected:bg-primary aria-selected:text-primary-foreground aria-selected:shadow-sm not-aria-selected:text-muted-foreground not-aria-selected:hover:text-foreground"
                on:click=move |_| tab.set(id)
            >
                {label}
                <Show when=move || count() != 0>
                    <span class="min-w-5 rounded-full bg-primary px-1.5 text-center text-[11px] font-bold leading-5 text-primary-foreground tabular-nums group-aria-selected:bg-primary-foreground group-aria-selected:text-primary">
                        {count}
                    </span>
                </Show>
            </button>
        }
    };
    view! {
        <section class="mx-auto flex max-w-7xl flex-col gap-5 px-4 py-6 sm:gap-6 sm:px-6">
            <div>
                <p class="font-pixel text-sm text-primary">"管理 · kanri"</p>
                <h1 class="text-3xl font-black">"Admin"</h1>
            </div>
            <Overview overview />
            <div role="tablist" class="grid grid-cols-3 gap-1 rounded-lg border bg-muted/60 p-1 sm:inline-grid sm:w-fit">
                {tab_button("daily", "Daily review", |o| o.flagged)}
                {tab_button("dojo", "SAT Dojo", |o| o.dojo_pending)}
                {tab_button("messages", "Messages", |o| o.messages)}
            </div>
            {move || match tab.get() {
                "dojo" => view! { <DojoAdmin on_change=Callback::new(move |()| load_overview()) /> }.into_any(),
                "messages" => view! { <MessagesAdmin on_change=Callback::new(move |()| load_overview()) /> }.into_any(),
                _ => view! { <DailyAdmin on_change=Callback::new(move |()| load_overview()) /> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn Overview(overview: RwSignal<Option<AdminOverview>>) -> impl IntoView {
    // A big number with a short line under it: long values in the pixel font fill a phone's tile
    let tile = move |label: &'static str, f: fn(&AdminOverview) -> (i64, String, bool)| {
        let stat = Memo::new(move |_| overview.with(|o| o.as_ref().map(f)));
        let alert = move || stat.with(|s| s.as_ref().is_some_and(|s| s.2));
        view! {
            <div
                class="flex flex-col gap-1 rounded-lg border bg-card p-3 sm:px-4"
                class:border-warning=alert
                class:bg-warning-light=alert
            >
                <p class="text-[11px] font-bold uppercase tracking-wider text-muted-foreground">{label}</p>
                <p class="font-pixel text-3xl leading-none tabular-nums">
                    {move || stat.with(|s| s.as_ref().map_or_else(|| "…".into(), |s| s.0.to_string()))}
                </p>
                <p class="text-xs text-muted-foreground">{move || stat.with(|s| s.as_ref().map(|s| s.1.clone()))}</p>
            </div>
        }
    };
    view! {
        <div class="grid grid-cols-2 gap-2 sm:gap-3 md:grid-cols-4">
            {tile("Players", |o| (o.users, format!("{} registered", o.registered), false))}
            {tile("Today's daily", |o| (o.started, format!("started · {} ranked", o.finished), false))}
            {tile("Flagged today", |o| {
                (o.flagged, if o.flagged == 0 { "all clear".into() } else { "to review".into() }, o.flagged != 0)
            })}
            {tile("Dojo", |o| (o.dojo_total, format!("submissions · {} waiting", o.dojo_pending), false))}
        </div>
    }
}

/// Lays a section's card out flat on phones: its rows are cards themselves, a card around them
/// would only take width.
const SECTION: &str =
    "gap-4 max-sm:gap-3 max-sm:border-0 max-sm:bg-transparent max-sm:py-0 max-sm:shadow-none";

/// One number of a phone card: a small label over the value.
fn phone_stat(label: &'static str, value: String, class: &'static str) -> impl IntoView {
    view! {
        <div class="min-w-0">
            <dt class="text-[10px] font-bold uppercase leading-tight tracking-wider text-muted-foreground">{label}</dt>
            <dd class=format!("truncate text-sm tabular-nums {class}")>{value}</dd>
        </div>
    }
}
