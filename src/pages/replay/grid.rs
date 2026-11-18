//! Small multiples: a little board per member, all on the same clock, their bridges the real
//! wooden ones and their islands turning green as they get all theirs.
//!
//! The card of a little board ([`MiniCard`]) is the race's too, for the players who are done and
//! watch the others.

use common::Board;
use leptos::prelude::*;

use crate::components::sprites::{BoardDefs, BridgeSprite, CELL, PENNANT, PixelSprite, Span};
use crate::i18n::{t_string, td_string, use_i18n};
use crate::model::{LaneOutcome, colour_var, format_score, progress};

use super::stage::{PAD, StaticIslands};
use super::state::{Theatre, apart_style};

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

/// A little board on a card: the player's colour on top, their name with `children` at its end
/// (how they stand), the board with their bridges and the islands that have all theirs, and a
/// bar of `percent` below. The replay's small multiples and the race's watchers use it; the page
/// needs the [`BoardDefs`] once.
#[component]
pub fn MiniCard(
    board: StoredValue<Board>,
    #[prop(into)] bridges: Signal<Vec<u8>>,
    colour: u8,
    name: String,
    /// The viewer's own: their name in bold.
    #[prop(optional)]
    bold: bool,
    #[prop(into)] label: Signal<String>,
    #[prop(into)] percent: Signal<u32>,
    children: Children,
) -> impl IntoView {
    let colour = colour_var(colour);
    let bar = colour.clone();
    let done = Signal::derive(move || board.with_value(|b| bridges.with(|br| islands_done(b, br))));
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
    view! {
        <li
            class="flex min-w-0 flex-col gap-1.5 rounded-xl border bg-card p-2 shadow-xs"
            style=format!("--member: {colour}; border-top: 4px solid {colour}")
        >
            <div class="flex min-w-0 items-center gap-1.5 text-sm">
                <span class="size-2.5 shrink-0 rounded-[3px]" style=format!("background: {colour}") aria-hidden="true"></span>
                <span class="min-w-0 truncate font-medium" class:font-black=bold>
                    {name}
                </span>
                <span class="ms-auto shrink-0 font-pixel text-xs tabular-nums text-muted-foreground">
                    {children()}
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
                <div class="h-full rounded-full transition-[width] duration-150" style=move || format!("width: {}%; background: {bar}", percent.get())></div>
            </div>
        </li>
    }
}

/// One member's little board.
#[component]
fn Mini(theatre: Theatre, board: StoredValue<Board>, lane: usize) -> impl IntoView {
    let i18n = use_i18n();
    let Some(info) = theatre.lane(lane) else {
        return ().into_any();
    };
    let bridges = Memo::new(move |_| {
        theatre
            .boards
            .with(|b| b.get(lane).map_or_default(|b| b.bridges.clone()))
    });
    let percent = Signal::derive(move || bridges.with(|b| progress(b, theatre.total)));
    let finish = info.finish();
    let score = match info.outcome {
        LaneOutcome::Solved { time_ms, .. } => time_ms,
        _ => 0,
    };
    let crossed = Memo::new(move |_| finish.filter(|&at| theatre.now() >= at));
    let name = info.name.clone();
    let label = Signal::derive(move || {
        td_string!(
            i18n.get_locale(),
            replay.legend.progress,
            name = name.clone(),
            percent = percent.get()
        )
    });
    view! {
        <MiniCard
            board
            bridges
            colour=info.colour
            name=info.name.clone()
            bold=info.user_id == theatre.data.with_value(|d| d.me)
            label
            percent
            attr:data-lane=info.user_id
        >
            {move || match crossed.get() {
                Some(_at) => view! {
                    <span class="inline-flex items-center gap-1 text-foreground">
                        <PixelSprite art=&PENNANT class="h-3 w-auto" />
                        {format_score(score)}
                    </span>
                }
                    .into_any(),
                None => view! { <span>{format!("{}%", percent.get())}</span> }.into_any(),
            }}
        </MiniCard>
    }
    .into_any()
}

/// A little board for every shown member, in as many columns as keep them about as tall as
/// the together board (`state::apart_grid`).
#[component]
pub fn Apart(theatre: Theatre) -> impl IntoView {
    let i18n = use_i18n();
    let Some(puzzle) = theatre.data.with_value(|d| d.puzzle.clone()) else {
        return ().into_any();
    };
    // The little boards' seas, width / height: the grid's columns and width follow from it
    let aspect = f64::from(i32::from(puzzle.width) * CELL + 2 * PAD)
        / f64::from(i32::from(puzzle.height) * CELL + 2 * PAD);
    let board = StoredValue::new(Board::new(puzzle));
    let shown = Memo::new(move |_| theatre.shown_lanes());
    view! {
        // The sea, islands and bridges the little boards use, once
        <svg class="absolute size-0" aria-hidden="true">
            <BoardDefs />
        </svg>
        <ul
            class="replay-apart"
            style=move || apart_style(shown.with(Vec::len), aspect)
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
