//! The layers of the board's SVG that are drawn from its state: bridges, islands and markers.

use common::{Board, Pos};
use leptos::prelude::*;

use super::BoardMode;
use super::geometry::{span_of, whole_px};
use crate::components::sprites::{BridgeSprite, CELL, Decor};

/// The built bridges, the wrong ones and the hint highlighted.
#[component]
pub(super) fn Bridges(
    board: Signal<Board>,
    bridges: Signal<Vec<u8>>,
    wrong: Signal<Vec<usize>>,
    hint: Signal<Option<usize>>,
) -> impl IntoView {
    move || {
        let b = board.get();
        (0..b.edges.len())
            .map(|e| {
                let span = span_of(&b, e);
                let count = Memo::new(move |_| bridges.with(|br| br.get(e).copied().unwrap_or(0)));
                move || {
                    let c = count.get();
                    let mut class = String::new();
                    if wrong.with(|w| w.contains(&e)) {
                        class.push_str("wrong ");
                    }
                    if hint.get() == Some(e) {
                        class.push_str("hint ");
                    }
                    (c > 0).then(|| view! { <BridgeSprite span=span count=c class=class /> })
                }
            })
            .collect_view()
    }
}

/// What the next click would build on the hovered gap / dragged direction.
#[component]
pub(super) fn GhostBridge(
    board: Signal<Board>,
    bridges: Signal<Vec<u8>>,
    preview_edge: RwSignal<Option<usize>>,
    hover_edge: RwSignal<Option<usize>>,
) -> impl IntoView {
    move || {
        let edge = preview_edge.get().or_else(|| hover_edge.get())?;
        board.with(|b| {
            let current = bridges.with(|br| br.get(edge).copied().unwrap_or(0));
            let blocked = current == 0 && bridges.with(|br| b.is_blocked(br, edge));
            (current < 2 && !blocked).then(
                || view! { <BridgeSprite span=span_of(b, edge) count=current + 1 class="ghost" /> },
            )
        })
    }
}

/// The islands with their numbers; in play mode marked done, over or selected.
#[component]
pub(super) fn Islands(
    board: Signal<Board>,
    degrees: Memo<Vec<u8>>,
    mode: BoardMode,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    move || {
        let b = board.get();
        b.puzzle
            .islands
            .iter()
            .enumerate()
            .map(|(i, island)| {
                let (x, y) = (island.pos.x, island.pos.y);
                let required = island.required;
                let decor = Decor::for_pos(x, y).id();
                let class = move || {
                    let mut classes = String::from("island");
                    if mode == BoardMode::Play {
                        let degree = degrees.with(|d| d.get(i).copied().unwrap_or(0));
                        if degree == required {
                            classes.push_str(" done");
                        } else if degree > required {
                            classes.push_str(" over");
                        }
                        if selected.get() == Some(i) {
                            classes.push_str(" selected");
                        }
                    }
                    classes
                };
                view! {
                    <g
                        class=class
                        transform=format!("translate({},{})", i32::from(x) * CELL, i32::from(y) * CELL)
                        data-island=i
                    >
                        {decor.map(|href| view! { <use href=href /> })}
                        <use href="#isl" />
                        <text class="num" x="8" y="8.4" text-anchor="middle" dominant-baseline="central">
                            {required}
                        </text>
                        <rect class="sel" x="0.5" y="0.5" width="15" height="15" />
                    </g>
                }
            })
            .collect_view()
    }
}

/// Dots on the empty cells in the editor.
#[component]
pub(super) fn GridDots(board: Signal<Board>, mode: BoardMode) -> impl IntoView {
    move || {
        (mode == BoardMode::Edit).then(|| {
            board.with(|b| {
                let mut dots = Vec::new();
                for y in 0..b.puzzle.height {
                    for x in 0..b.puzzle.width {
                        if b.puzzle.island_at(Pos::new(x, y)).is_none() {
                            dots.push(view! {
                                <rect
                                    class="grid-dot"
                                    x=i32::from(x) * CELL + 7
                                    y=i32::from(y) * CELL + 7
                                    width="2"
                                    height="2"
                                />
                            });
                        }
                    }
                }
                dots.collect_view()
            })
        })
    }
}

/// A dashed outline around the hovered cell in the editor.
#[component]
pub(super) fn HoverCell(hover_cell: RwSignal<Option<Pos>>, mode: BoardMode) -> impl IntoView {
    move || {
        let pos = hover_cell.get()?;
        (mode == BoardMode::Edit).then(|| {
            view! {
                <rect
                    x=i32::from(pos.x) * CELL
                    y=i32::from(pos.y) * CELL
                    width=CELL
                    height=CELL
                    fill="none"
                    stroke="var(--px-gold)"
                    stroke-width="1"
                    stroke-dasharray="2 2"
                    pointer-events="none"
                />
            }
        })
    }
}

/// A red pixel cross where a bridge would cross another one.
#[component]
pub(super) fn BlockedMark(blocked: RwSignal<Option<(f64, f64, u32)>>) -> impl IntoView {
    move || {
        blocked.get().map(|(x, y, key)| {
            let (x, y) = (whole_px(x), whole_px(y));
            view! {
                <g class="blocked-x" data-key=key pointer-events="none">
                    <rect x=x - 3 y=y - 3 width="2" height="2" class="px-shu" />
                    <rect x=x + 1 y=y - 3 width="2" height="2" class="px-shu" />
                    <rect x=x - 1 y=y - 1 width="2" height="2" class="px-shu" />
                    <rect x=x - 3 y=y + 1 width="2" height="2" class="px-shu" />
                    <rect x=x + 1 y=y + 1 width="2" height="2" class="px-shu" />
                </g>
            }
        })
    }
}
