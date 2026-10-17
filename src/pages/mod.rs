pub mod create;
pub mod lab;
pub mod play;
pub mod rules;

use common::Puzzle;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::puzzle_cnf;
use crate::components::celebration::Stamp;
use crate::state::{ToastKind, download, toast};

/// Downloads the DIMACS encoding of a puzzle.
pub fn download_cnf(puzzle: Puzzle) {
    spawn_local(async move {
        match puzzle_cnf(puzzle).await {
            Ok(cnf) => download("hashi.cnf", &cnf, "text/plain"),
            Err(e) => toast(ToastKind::Error, format!("Could not encode the puzzle: {e}")),
        }
    });
}

#[component]
pub fn NotFound() -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        if let Some(resp) = use_context::<leptos_axum::ResponseOptions>() {
            resp.set_status(axum::http::StatusCode::NOT_FOUND);
        }
    }
    view! {
        <section class="mx-auto flex max-w-xl flex-col items-center gap-4 px-6 py-24 text-center">
            <Stamp kanji="迷" class="size-24 text-6xl" />
            <h1 class="text-3xl font-black">"Lost at sea"</h1>
            <p class="text-muted-foreground">"There is no bridge to this page. 迷子 — let's get you back to the islands."</p>
            <A href="/" attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                "Back to the game"
            </A>
        </section>
    }
}
