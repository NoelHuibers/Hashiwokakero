//! Small multiples: a little board per member, all on the same clock, their bridges the real
//! wooden ones and their islands turning green as they get all theirs.

use common::Board;
use leptos::prelude::*;

use crate::components::sprites::{BoardDefs, BridgeSprite, CELL, PENNANT, PixelSprite, Span};
use crate::i18n::{t_string, td_string, use_i18n};
use crate::model::{LaneOutcome, colour_var, format_score, progress};

use super::stage::{PAD, StaticIslands};
use super::state::Theatre;

fn span_of(board: &Board, edge: usize) -> Span {
    let (a, b) = board.endpoints(edge);
    Span {
        horizontal: board.edges[edge].orientation == common::Orientation::Horizontal,
        a: (i32::from(a.x), i32::from(a.y)),
        b: (i32::from(b.x), i32::from(b.y)),
    }
}

/// Which islands have all their bridges.
pub fn islands_done(board: &Board, bridges: &[u8]) -> Vec<bool> {
    let degrees = board.check(bridges).degrees;
    board
        .puzzle
        .islands
        .iter()
        .zip(degrees)
        .map(|(island, degree)| degree == island.required)
        .collect()
}

/// One member's little board.
#[component]
fn Mini(theatre: Theatre, board: StoredValue<Board>, lane: usize) -> impl IntoView {
    let i18n = use_i18n();
    let Some(info) = theatre.lane(lane) else {
        return ().into_any();
    };
    let colour = colour_var(info.colour);
    let bar = colour.clone();
    let bridges = Memo::new(move |_| {
        theatre
            .boards
            .with(|b| b.get(lane).map(|b| b.bridges.clone()).unwrap_or_default())
    });
    let done = Signal::derive(move || board.with_value(|b| bridges.with(|br| islands_done(b, br))));
    let percent = move || bridges.with(|b| progress(b, theatre.total));
    let finish = info.finish();
    let score = match info.outcome {
        LaneOutcome::Solved { time_ms, .. } => time_ms,
        _ => 0,
    };
    let crossed = Memo::new(move |_| finish.filter(|&at| theatre.now() >= at));
    let (w, h) = board.with_value(|b| {
        (
            i32::from(b.puzzle.width) * CELL + 2 * PAD,
            i32::from(b.puzzle.height) * CELL + 2 * PAD,
        )
    });
    let edges = board.with_value(|b| {
        (0..b.edges.len())
            .map(|e| (e, span_of(b, e)))
            .collect::<Vec<_>>()
    });
    let name = info.name.clone();
    let label = move || {
        td_string!(
            i18n.get_locale(),
            replay.legend.progress,
            name = name.clone(),
            percent = percent()
        )
    };
    view! {
        <li
            class="flex min-w-0 flex-col gap-1.5 rounded-xl border bg-card p-2 shadow-xs"
            style=format!("--member: {colour}; border-top: 4px solid {colour}")
            data-lane=info.user_id
        >
            <div class="flex min-w-0 items-center gap-1.5 text-sm">
                <span class="size-2.5 shrink-0 rounded-[3px]" style=format!("background: {colour}") aria-hidden="true"></span>
                <span class="min-w-0 truncate font-medium" class:font-black=info.user_id == theatre.data.with_value(|d| d.me)>
                    {info.name.clone()}
                </span>
                <span class="ms-auto shrink-0 font-pixel text-xs tabular-nums text-muted-foreground">
                    {move || match crossed.get() {
                        Some(_at) => view! {
                            <span class="inline-flex items-center gap-1 text-foreground">
                                <PixelSprite art=&PENNANT class="h-3 w-auto" />
                                {format_score(score)}
                            </span>
                        }
                            .into_any(),
                        None => view! { <span>{format!("{}%", percent())}</span> }.into_any(),
                    }}
                </span>
            </div>
            <svg
                class="board-svg w-full rounded-lg"
                style=format!("aspect-ratio: {w} / {h};")
                viewBox=format!("{} {} {w} {h}", -PAD, -PAD)
                xmlns="http://www.w3.org/2000/svg"
                role="img"
                aria-label=label
            >
                <rect x=-PAD y=-PAD width=w height=h fill="url(#sea)" />
                <g>
                    {edges
                        .into_iter()
                        .map(|(edge, span)| {
                            let count = Memo::new(move |_| bridges.with(|b| b.get(edge).copied().unwrap_or(0)));
                            move || {
                                let c = count.get();
                                (c > 0).then(|| view! { <BridgeSprite span count=c /> })
                            }
                        })
                        .collect_view()}
                </g>
                <g>
                    <StaticIslands board done />
                </g>
            </svg>
            <div class="h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden="true">
                <div class="h-full rounded-full transition-[width] duration-150" style=move || format!("width: {}%; background: {bar}", percent())></div>
            </div>
        </li>
    }
    .into_any()
}

/// A little board for every shown member.
#[component]
pub fn Apart(theatre: Theatre) -> impl IntoView {
    let i18n = use_i18n();
    let Some(puzzle) = theatre.data.with_value(|d| d.puzzle.clone()) else {
        return ().into_any();
    };
    let board = StoredValue::new(Board::new(puzzle));
    let shown = Memo::new(move |_| theatre.shown_lanes());
    view! {
        // The sea, islands and bridges the little boards use, once
        <svg class="absolute size-0" aria-hidden="true">
            <BoardDefs />
        </svg>
        <ul
            class="grid grid-cols-2 gap-2 sm:grid-cols-3 sm:gap-3"
            aria-label=move || t_string!(i18n, replay.controls.apart)
            data-testid="replay-apart"
        >
            <For each=move || shown.get() key=|lane| *lane let:lane>
                <Mini theatre board lane />
            </For>
        </ul>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use common::{Pos, Puzzle};

    use super::*;

    #[test]
    fn islands_are_done_with_all_their_bridges() {
        let mut puzzle = Puzzle::empty(5, 5);
        puzzle.set(Pos::new(0, 0), 2);
        puzzle.set(Pos::new(4, 0), 3);
        puzzle.set(Pos::new(4, 4), 1);
        let board = Board::new(puzzle);
        assert_eq!(islands_done(&board, &[0, 0]), [false, false, false]);
        assert_eq!(islands_done(&board, &[2, 0]), [true, false, false]);
        assert_eq!(islands_done(&board, &[2, 1]), [true, true, true]);
        assert_eq!(span_of(&board, 1).a, (4, 0));
    }
}
