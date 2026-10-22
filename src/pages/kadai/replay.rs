//! A group puzzle in the replay theatre (再生), `/groups/:id/puzzles/:puzzle/replay`: every
//! member's solve of it at once, from the move logs. Open once the viewer is done with it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::load_puzzle_replay;
use crate::components::celebration::Stamp;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::prefix;
use crate::model::ReplayData;
use crate::pages::replay::TheatreView;
use crate::seo::title_text;
use crate::state::{Auth, error_toast};

use super::puzzle_path;

/// The path of a group puzzle's replay (without the language).
pub fn replay_path(group: i64, puzzle: i64) -> String {
    format!("{}/replay", puzzle_path(group, puzzle))
}

/// A link to the replay, for the cards next to the board.
#[component]
pub fn ReplayLink(group: i64, puzzle: i64) -> impl IntoView {
    let i18n = use_i18n();
    let href = move || {
        format!(
            "{}{}",
            prefix(i18n.get_locale()),
            replay_path(group, puzzle)
        )
    };
    view! {
        <A
            href=href
            attr:class="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border bg-card px-3 text-sm font-medium shadow-xs hover:bg-accent"
            attr:data-testid="kadai-replay"
        >
            <span class="font-pixel text-primary" aria-hidden="true">"再"</span>
            {t!(i18n, replay.watch)}
        </A>
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Loaded {
    Loading,
    Missing,
    Ready(ReplayData),
}

#[component]
pub fn KadaiReplayPage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let params = use_params_map();
    let group =
        Memo::new(move |_| params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok())));
    let puzzle =
        Memo::new(move |_| params.with(|p| p.get("puzzle").and_then(|id| id.parse::<i64>().ok())));
    let loaded = RwSignal::new(Loaded::Loading);
    Effect::new(move |_| {
        let (group, puzzle) = (group.get(), puzzle.get());
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if !auth.loaded.get() {
            return;
        }
        let Some(id) = puzzle else {
            loaded.set(Loaded::Missing);
            return;
        };
        spawn_local(async move {
            match load_puzzle_replay(id).await {
                Ok(Some(data)) if data.group.as_ref().map(|g| g.id) == group => {
                    loaded.try_set(Loaded::Ready(data));
                }
                Ok(_) => {
                    loaded.try_set(Loaded::Missing);
                }
                Err(e) => error_toast(e),
            }
        });
    });
    let back = move || {
        let p = prefix(i18n.get_locale());
        match (group.get(), puzzle.get()) {
            (Some(g), Some(id)) => format!("{p}{}", puzzle_path(g, id)),
            _ => format!("{p}/groups"),
        }
    };
    view! {
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, replay.title))) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-7xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back exact=true attr:class="w-fit text-sm text-primary hover:underline">
                "← " {t!(i18n, kadai.replay.back)}
            </A>
            <div class="flex items-center gap-3">
                <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl">
                    "再"
                </span>
                <div class="min-w-0">
                    <p class="font-pixel text-sm text-primary">"再生 · saisei"</p>
                    <h1 class="text-3xl font-black">{t!(i18n, replay.title)}</h1>
                    <p class="text-sm text-muted-foreground">{t!(i18n, kadai.title)}</p>
                </div>
            </div>
            {move || match loaded.get() {
                Loaded::Loading => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Loaded::Missing => view! {
                    <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-16 text-center" data-testid="kadai-missing">
                        <Stamp kanji="迷" class="size-20 text-5xl" />
                        <h2 class="text-2xl font-black">{t!(i18n, kadai.page.missing)}</h2>
                    </div>
                }
                    .into_any(),
                Loaded::Ready(data) if data.locked => view! {
                    <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-12 text-center" data-testid="replay-locked">
                        <Stamp kanji="待" class="size-20 text-5xl" />
                        <h2 class="text-2xl font-black">{t!(i18n, replay.locked.title)}</h2>
                        <p class="text-muted-foreground">{t!(i18n, kadai.replay.locked)}</p>
                    </div>
                }
                    .into_any(),
                Loaded::Ready(data) if data.end() == 0 => view! {
                    <div class="mx-auto flex max-w-xl flex-col items-center gap-3 py-12 text-center" data-testid="replay-empty">
                        <span class="font-pixel text-5xl text-sea" aria-hidden="true">"〜"</span>
                        <h2 class="text-xl font-black">{t!(i18n, replay.empty.title)}</h2>
                        <p class="text-muted-foreground">{t!(i18n, kadai.replay.empty)}</p>
                    </div>
                }
                    .into_any(),
                Loaded::Ready(data) => view! { <TheatreView data only=None /> }.into_any(),
            }}
        </section>
    }
}
