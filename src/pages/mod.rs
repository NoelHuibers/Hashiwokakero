pub mod account;
pub mod admin;
pub mod contact;
pub mod create;
pub mod daily;
pub mod dojo;
pub mod lab;
pub mod legal;
pub mod play;
pub mod player;
pub mod ranks;
pub mod rules;

use common::Puzzle;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;

use crate::api::puzzle_cnf;
use crate::components::celebration::Stamp;
use crate::components::layout::use_localized;
use crate::i18n::{t, t_string, use_i18n};
use crate::seo::title_text;
use crate::state::{download, error_toast};

/// Downloads the DIMACS encoding of a puzzle.
pub fn download_cnf(puzzle: Puzzle) {
    spawn_local(async move {
        match puzzle_cnf(puzzle).await {
            Ok(cnf) => download("hashi.cnf", &cnf, "text/plain"),
            Err(e) => error_toast(e),
        }
    });
}

/// Answers the page's request with 404 Not Found (on the server, the browser has no status).
pub fn not_found_status() {
    #[cfg(feature = "ssr")]
    {
        if let Some(resp) = use_context::<leptos_axum::ResponseOptions>() {
            resp.set_status(axum::http::StatusCode::NOT_FOUND);
        }
    }
}

#[component]
pub fn NotFound() -> impl IntoView {
    not_found_status();
    let i18n = use_i18n();
    let home = use_localized("/");
    view! {
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, common.not_found.title))) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-xl flex-col items-center gap-4 px-6 py-24 text-center">
            <Stamp kanji="迷" class="size-24 text-6xl" />
            <h1 class="text-3xl font-black">{t!(i18n, common.not_found.title)}</h1>
            <p class="text-muted-foreground">{t!(i18n, common.not_found.text)} " 迷子"</p>
            <A href=home exact=true attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                {t!(i18n, common.not_found.back)}
            </A>
        </section>
    }
}
