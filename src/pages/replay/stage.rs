//! Everyone on one board.
//!
//! The day's islands, and on every bridge a lane per shown member, a
//! ribbon in their colour (one for a single bridge, two for a double), on a wooden deck so every
//! colour stands out against the sea. The newest move of each member rings for a moment.

use common::{Board, Orientation};
use leptos::prelude::*;

use crate::components::sprites::{BoardDefs, CELL, Decor};
use crate::i18n::{td_string, use_i18n};
use crate::model::{colour_var, daily_number, lane_offset, lane_pitch};

use super::state::Theatre;

/// Space around the grid, decorations stick out of the top row (as on the board).
pub const PAD: i32 = 6;
/// How long (real time, ms) a new move rings.
const RING_MS: f64 = 450.0;

/// Where a bridge between two islands runs, in art pixels: from `from` to `to` along its
/// direction, `across` the middle of the row or column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    pub horizontal: bool,
    pub from: f64,
    pub to: f64,
    pub across: f64,
}

impl Run {
    /// The water between the islands of an edge, like the bridge sprites.
    pub fn of(board: &Board, edge: usize) -> Self {
        let (a, b) = board.endpoints(edge);
        let c = f64::from(CELL);
        if board.edges[edge].orientation == Orientation::Horizontal {
            Self {
                horizontal: true,
                from: f64::from(a.x) * c + 13.0,
                to: f64::from(b.x) * c + 3.0,
                across: f64::from(a.y) * c + 8.0,
            }
        } else {
            Self {
                horizontal: false,
                from: f64::from(a.y) * c + 13.0,
                to: f64::from(b.y) * c + 3.0,
                across: f64::from(a.x) * c + 8.0,
            }
        }
    }

    /// A strip along the run, `offset` from its middle, `width` thick: `(x, y, w, h)`.
    pub fn strip(self, offset: f64, width: f64) -> (f64, f64, f64, f64) {
        let start = self.across + offset - width / 2.0;
        if self.horizontal {
            (self.from, start, self.to - self.from, width)
        } else {
            (start, self.from, width, self.to - self.from)
        }
    }

    pub fn middle(self) -> (f64, f64) {
        let along = f64::midpoint(self.from, self.to);
        if self.horizontal {
            (along, self.across)
        } else {
            (self.across, along)
        }
    }
}

/// The strips of a member's lane: one ribbon for a single bridge, two for a double.
pub fn ribbons(offset: f64, pitch: f64, count: u8) -> Vec<(f64, f64)> {
    match count {
        0 => Vec::new(),
        1 => vec![(offset, pitch * 0.55)],
        _ => vec![
            (offset - pitch * 0.24, pitch * 0.3),
            (offset + pitch * 0.24, pitch * 0.3),
        ],
    }
}

fn rect(
    class: &'static str,
    fill: Option<String>,
    (x, y, w, h): (f64, f64, f64, f64),
) -> impl IntoView {
    view! {
        <rect
            class=class
            x=format!("{x:.2}")
            y=format!("{y:.2}")
            width=format!("{w:.2}")
            height=format!("{h:.2}")
            fill=fill
        />
    }
}

/// The islands of a board with their numbers, as on the board but read only. `done` marks an
/// island whose bridges are all there.
#[component]
pub fn StaticIslands(
    board: StoredValue<Board>,
    #[prop(optional, into)] done: Option<Signal<Vec<bool>>>,
) -> impl IntoView {
    board.with_value(|b| {
        b.puzzle
            .islands
            .iter()
            .enumerate()
            .map(|(i, island)| {
                let (x, y) = (island.pos.x, island.pos.y);
                let class = move || match done {
                    Some(done) if done.with(|d| d.get(i).copied().unwrap_or(false)) => "island done",
                    _ => "island",
                };
                view! {
                    <g class=class style="cursor: default" transform=format!("translate({},{})", i32::from(x) * CELL, i32::from(y) * CELL)>
                        {Decor::for_pos(x, y).id().map(|href| view! { <use href=href /> })}
                        <use href="#isl" />
                        <text class="num" x="8" y="8.4" text-anchor="middle" dominant-baseline="central">
                            {island.required}
                        </text>
                    </g>
                }
            })
            .collect_view()
    })
}

/// The bridges of one edge: a deck under the shown lanes, a ribbon per member on it.
#[component]
fn EdgeLanes(theatre: Theatre, run: Run, edge: usize, shown: Memo<Vec<usize>>) -> impl IntoView {
    // (lanes shown, the ones with a bridge here: (slot, colour, count))
    let here = Memo::new(move |_| {
        let shown = shown.get();
        let on: Vec<(usize, u8, u8)> = theatre.boards.with(|boards| {
            shown
                .iter()
                .enumerate()
                .filter_map(|(slot, &lane)| {
                    let count = boards.get(lane)?.bridges.get(edge).copied().unwrap_or(0);
                    let colour = theatre.data.with_value(|d| d.lanes[lane].colour);
                    (count > 0).then_some((slot, colour, count))
                })
                .collect()
        });
        (shown.len(), on)
    });
    move || {
        let (n, on) = here.get();
        if on.is_empty() {
            return None;
        }
        let pitch = lane_pitch(n);
        let count = on.len();
        #[expect(clippy::cast_precision_loss, reason = "a group has at most 50 members")]
        let deck = n as f64 * pitch + 1.0;
        Some(view! {
            <g data-edge=edge data-lanes=count>
                {rect("px-sea-deep", None, run.strip(0.6, deck))}
                {rect("", Some("var(--px-sand)".into()), run.strip(0.0, deck))}
                {on
                    .into_iter()
                    .flat_map(|(slot, colour, count)| {
                        let offset = lane_offset(slot, n);
                        ribbons(offset, pitch, count)
                            .into_iter()
                            .map(move |(o, w)| rect("", Some(colour_var(colour)), run.strip(o, w)))
                    })
                    .collect_view()}
            </g>
        })
    }
}

/// The ring around a member's newest move, while it's new.
#[component]
fn Rings(
    theatre: Theatre,
    board: StoredValue<Board>,
    shown: Memo<Vec<usize>>,
    still: bool,
) -> impl IntoView {
    move || {
        if still {
            return Vec::new();
        }
        let t = theatre.time.get();
        let speed = f64::from(theatre.speed.get().max(1));
        let n = shown.with(Vec::len);
        let mut rings = Vec::new();
        theatre.boards.with(|boards| {
            for (slot, &lane) in shown.with(Clone::clone).iter().enumerate() {
                let Some(applied) = boards.get(lane).map(|b| b.applied).filter(|&a| a > 0) else {
                    continue;
                };
                let (event, colour) = theatre
                    .data
                    .with_value(|d| (d.lanes[lane].log[applied - 1], d.lanes[lane].colour));
                let age = (t - f64::from(event.t)) / speed;
                if !(0.0..RING_MS).contains(&age) {
                    continue;
                }
                let run = board.with_value(|b| Run::of(b, usize::from(event.edge)));
                let (mut x, mut y) = run.middle();
                if run.horizontal {
                    y += lane_offset(slot, n);
                } else {
                    x += lane_offset(slot, n);
                }
                let k = age / RING_MS;
                rings.push(view! {
                    <circle
                        cx=format!("{x:.2}")
                        cy=format!("{y:.2}")
                        r=format!("{:.2}", 2.0 + 7.0 * k)
                        fill="none"
                        stroke=colour_var(colour)
                        stroke-width="1.5"
                        opacity=format!("{:.2}", 1.0 - k)
                        style="shape-rendering: auto"
                    />
                });
            }
        });
        rings
    }
}

/// Everyone's bridges on one board.
#[component]
pub fn Stage(theatre: Theatre, #[prop(optional)] still: bool) -> impl IntoView {
    let i18n = use_i18n();
    let Some(puzzle) = theatre.data.with_value(|d| d.puzzle.clone()) else {
        return ().into_any();
    };
    let board = StoredValue::new(Board::new(puzzle));
    let (w, h) = board.with_value(|b| {
        (
            i32::from(b.puzzle.width) * CELL + 2 * PAD,
            i32::from(b.puzzle.height) * CELL + 2 * PAD,
        )
    });
    let shown = Memo::new(move |_| theatre.shown_lanes());
    let day = theatre.data.with_value(|d| d.day);
    let puzzle_replay = theatre.data.with_value(|d| d.group_puzzle.is_some());
    let label = move || {
        let names = shown.with(|s| {
            theatre.data.with_value(|d| {
                s.iter()
                    .map(|&i| d.lanes[i].name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
        });
        let locale = i18n.get_locale();
        if puzzle_replay {
            td_string!(locale, kadai.replay.stage, names)
        } else {
            td_string!(
                locale,
                replay.stage.label,
                number = daily_number(day),
                names
            )
        }
    };
    let edges = board.with_value(|b| {
        (0..b.edges.len())
            .map(|e| (e, Run::of(b, e)))
            .collect::<Vec<_>>()
    });
    view! {
        <svg
            class="board-svg w-full"
            style=format!("aspect-ratio: {w} / {h};")
            viewBox=format!("{} {} {w} {h}", -PAD, -PAD)
            xmlns="http://www.w3.org/2000/svg"
            role="img"
            aria-label=label
            data-testid="replay-stage"
        >
            <BoardDefs />
            <rect x=-PAD y=-PAD width=w height=h fill="url(#sea)" />
            <g>
                {edges
                    .into_iter()
                    .map(|(edge, run)| view! { <EdgeLanes theatre run edge shown /> })
                    .collect_view()}
            </g>
            <g>
                <StaticIslands board />
            </g>
            <g aria-hidden="true">
                <Rings theatre board shown still />
            </g>
        </svg>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use common::{Pos, Puzzle};

    use super::*;

    #[test]
    fn runs_cover_the_water_between_the_islands() {
        let mut puzzle = Puzzle::empty(5, 5);
        puzzle.set(Pos::new(0, 0), 2);
        puzzle.set(Pos::new(4, 0), 3);
        puzzle.set(Pos::new(4, 4), 1);
        let board = Board::new(puzzle);
        let across = Run::of(&board, board.edge_between(0, 1).unwrap());
        assert_eq!(
            across,
            Run {
                horizontal: true,
                from: 13.0,
                to: 67.0,
                across: 8.0
            }
        );
        assert_eq!(across.strip(1.0, 2.0), (13.0, 8.0, 54.0, 2.0));
        assert_eq!(across.middle(), (40.0, 8.0));
        let down = Run::of(&board, board.edge_between(1, 2).unwrap());
        assert!(!down.horizontal);
        assert_eq!(down.strip(-1.0, 2.0), (70.0, 13.0, 2.0, 54.0));
        assert_eq!(down.middle(), (72.0, 40.0));
    }

    #[test]
    fn a_double_bridge_is_two_ribbons() {
        assert_eq!(ribbons(0.0, 4.0, 0), []);
        assert_eq!(ribbons(1.0, 4.0, 1).len(), 1);
        let double = ribbons(1.0, 4.0, 2);
        assert_eq!(double.len(), 2);
        // Side by side, not overlapping, inside the lane
        let (a, b) = (double[0], double[1]);
        assert!(b.0 - a.0 >= f64::midpoint(a.1, b.1));
        assert!(a.0 - a.1 / 2.0 >= 1.0 - 2.0 && b.0 + b.1 / 2.0 <= 1.0 + 2.0);
    }
}
