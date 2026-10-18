//! The admin dashboard: today at a glance, reviewing daily results and running the Dojo. Only for
//! the usernames in `HASHI_ADMINS`; everyone else sees the usual "not found". English only.

pub mod daily;
pub mod dojo;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Meta;
use leptos_router::hooks::use_query_map;

use crate::api::load_admin_overview;
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::lang::error_text;
use crate::model::AdminOverview;
use crate::pages::NotFound;
use crate::state::{Auth, ToastKind, toast};

use self::daily::DailyAdmin;
use self::dojo::DojoAdmin;

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
    let tab = RwSignal::new(
        if query.with_untracked(|q| q.get("tab").as_deref() == Some("dojo")) {
            "dojo"
        } else {
            "daily"
        },
    );
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
    let tab_button = move |id: &'static str, label: &'static str| {
        view! {
            <Button
                variant=Signal::derive(move || {
                    if tab.get() == id { ButtonVariant::Default } else { ButtonVariant::Outline }
                })
                size=ButtonSize::Sm
                on:click=move |_| tab.set(id)
            >
                {label}
            </Button>
        }
    };
    view! {
        <section class="mx-auto flex max-w-7xl flex-col gap-6 px-4 py-6 sm:px-6">
            <div>
                <p class="font-pixel text-sm text-primary">"管理 · kanri"</p>
                <h1 class="text-3xl font-black">"Admin"</h1>
            </div>
            <Overview overview />
            <div class="flex gap-2">
                {tab_button("daily", "Daily review")}
                {tab_button("dojo", "SAT Dojo")}
            </div>
            {move || match tab.get() {
                "dojo" => view! { <DojoAdmin on_change=Callback::new(move |()| load_overview()) /> }.into_any(),
                _ => view! { <DailyAdmin on_change=Callback::new(move |()| load_overview()) /> }.into_any(),
            }}
        </section>
    }
}

#[component]
fn Overview(overview: RwSignal<Option<AdminOverview>>) -> impl IntoView {
    let tile = |label: &'static str, value: Signal<String>| {
        view! {
            <div class="rounded-lg border bg-card px-4 py-3">
                <p class="text-xs font-bold uppercase tracking-wider text-muted-foreground">{label}</p>
                <p class="font-pixel text-2xl tabular-nums">{move || value.get()}</p>
            </div>
        }
    };
    let stat = move |f: fn(&AdminOverview) -> String| {
        Signal::derive(move || overview.with(|o| o.as_ref().map_or_else(|| "…".into(), f)))
    };
    view! {
        <div class="grid grid-cols-2 gap-3 md:grid-cols-4">
            {tile("Players", stat(|o| format!("{} ({} registered)", o.users, o.registered)))}
            {tile("Today's daily", stat(|o| format!("{} started · {} ranked", o.started, o.finished)))}
            {tile("Flagged today", stat(|o| o.flagged.to_string()))}
            {tile("Dojo", stat(|o| format!("{} submissions · {} waiting", o.dojo_total, o.dojo_pending)))}
        </div>
    }
}
