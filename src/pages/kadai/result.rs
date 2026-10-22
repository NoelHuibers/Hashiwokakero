//! The result card once the viewer solved a group puzzle or gave up: the time, the place in the
//! group, and where to go next.

use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::components::celebration::Stamp;
use crate::components::layout::use_localized;
use crate::components::ui::card::Card;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{Flag, MyPuzzle, TodayState, format_duration};
use crate::state::{Game, Source};

use super::replay::ReplayLink;
use super::state::Play;

/// Loads a finished puzzle into the practice board, to play it again for fun.
#[component]
pub fn PracticeLink(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let practice_path = use_localized("/practice");
    let practice = expect_context::<Game>();
    let navigate = use_navigate();
    let go = move |_| {
        let Some((puzzle, title)) = play.page.with_untracked(|p| {
            p.as_ref().and_then(|p| {
                p.puzzle
                    .clone()
                    .map(|puzzle| (puzzle, p.summary.title.clone().unwrap_or_default()))
            })
        }) else {
            return;
        };
        let solution = play
            .page
            .with_untracked(|p| p.as_ref().and_then(|p| p.solution.clone()));
        practice.set_puzzle(puzzle, solution, Some(true), Source::Imported(title));
        navigate(&practice_path(), NavigateOptions::default());
    };
    view! {
        <button
            type="button"
            class="inline-flex min-h-10 items-center justify-center text-sm text-primary underline-offset-4 hover:underline"
            on:click=go
        >
            {t!(i18n, kadai.result.practice)}
        </button>
    }
}

#[component]
pub fn ResultCard(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let back = move || {
        let group = play.page.with(|p| p.as_ref().map_or(0, |p| p.group_id));
        format!("{}/groups/{group}", prefix(i18n.get_locale()))
    };
    let group_name = move || {
        play.page
            .with(|p| p.as_ref().map(|p| p.group_name.clone()))
            .unwrap_or_default()
    };
    let standing = move || play.page.with(|p| p.as_ref().map(|p| p.summary.mine));
    let flag = move || {
        play.page.with(|p| {
            p.as_ref()
                .and_then(|p| p.attempt.as_ref())
                .and_then(|a| a.flag)
        })
    };
    // Ranked solvers so far
    let ranked = move || {
        play.page.with(|p| {
            p.as_ref().map_or(0, |p| {
                p.board
                    .iter()
                    .filter(|e| matches!(e.state, TodayState::Ranked { .. }))
                    .count()
            })
        })
    };
    view! {
        <Card class="items-center gap-3 overflow-hidden px-6 text-center" attr:data-testid="kadai-result">
            {move || match standing() {
                Some(MyPuzzle::GaveUp) => view! {
                    <span class="hanko grid size-16 rotate-[-6deg] place-items-center rounded-lg font-pixel text-4xl">
                        "解"
                    </span>
                    <h2 class="text-xl font-black">{t!(i18n, kadai.result.revealed_title)}</h2>
                    <p class="text-sm text-muted-foreground">{t!(i18n, kadai.result.revealed_text)}</p>
                }
                    .into_any(),
                Some(MyPuzzle::Solved { time_ms, rank }) => {
                    let reason = flag().map(|flag| {
                        let locale = i18n.get_locale();
                        let text = match flag {
                            Flag::TooFast => td_string!(locale, daily.flag.too_fast),
                            Flag::Pace => td_string!(locale, daily.flag.pace),
                            _ => td_string!(locale, daily.flag.log),
                        };
                        view! {
                            <p class="rounded-md border border-warning/40 bg-warning-light px-3 py-2 text-start text-sm text-warning-dark">
                                {text}
                            </p>
                        }
                    });
                    view! {
                        <Stamp kanji="完" class="size-20 text-5xl" />
                        <h2 class="text-2xl font-black">"見事！ " {t!(i18n, common.game.well_done)}</h2>
                        <p class="font-pixel text-3xl tabular-nums" data-testid="kadai-time">{format_duration(time_ms)}</p>
                        {reason}
                        <p class="text-sm text-muted-foreground" data-testid="kadai-place">
                            {move || match rank {
                                Some(rank) => t_string!(i18n, kadai.result.rank, rank, total = ranked(), group = group_name()),
                                None => t_string!(i18n, kadai.result.unranked).to_string(),
                            }}
                        </p>
                    }
                        .into_any()
                }
                _ => ().into_any(),
            }}
            <div class="flex w-full flex-col gap-2">
                <A
                    href=back
                    attr:class="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border bg-card text-sm font-medium shadow-xs hover:bg-accent"
                >
                    <span class="font-pixel text-primary" aria-hidden="true">"仲"</span>
                    {move || t_string!(i18n, kadai.result.back, group = group_name())}
                </A>
                {move || {
                    let ids = play.page.with(|p| p.as_ref().map(|p| (p.group_id, p.summary.id)));
                    ids.map(|(group, puzzle)| view! { <ReplayLink group puzzle /> })
                }}
                <PracticeLink play />
            </div>
        </Card>
    }
}
