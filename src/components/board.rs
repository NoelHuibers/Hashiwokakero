use common::{Board, Orientation, Pos};
use leptos::ev::PointerEvent;
use leptos::prelude::*;

use super::sprites::{BoardDefs, BridgeSprite, CELL, Decor, Span};

/// Space around the grid, decorations stick out of the top row.
const PAD: i32 = 6;
/// Pointer movement (art pixels) after which pressing an island becomes a drag.
const DRAG_THRESHOLD: f64 = 5.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoardMode {
    /// Build bridges.
    #[default]
    Play,
    /// Place and change islands.
    Edit,
    /// Read only.
    View,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellAction {
    Increment,
    Decrement,
    Set(u8),
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    island: usize,
    start: (f64, f64),
}

fn span_of(board: &Board, edge: usize) -> Span {
    let (a, b) = board.endpoints(edge);
    Span {
        horizontal: board.edges[edge].orientation == Orientation::Horizontal,
        a: (a.x as i32, a.y as i32),
        b: (b.x as i32, b.y as i32),
    }
}

/// Midpoint of an edge in art pixels.
fn midpoint(board: &Board, edge: usize) -> (f64, f64) {
    let (a, b) = board.endpoints(edge);
    let c = CELL as f64;
    (
        (a.x as f64 + b.x as f64) / 2.0 * c + c / 2.0,
        (a.y as f64 + b.y as f64) / 2.0 * c + c / 2.0,
    )
}

/// For every cell (row major) the edges passing over it.
fn cell_edges(board: &Board) -> Vec<Vec<usize>> {
    let w = board.puzzle.width as usize;
    let mut cells = vec![Vec::new(); w * board.puzzle.height as usize];
    for e in 0..board.edges.len() {
        for p in board.cells(e) {
            cells[p.y as usize * w + p.x as usize].push(e);
        }
    }
    cells
}

/// The edge whose gap contains the point, preferring the line closest to it.
fn edge_at(board: &Board, cells: &[Vec<usize>], px: f64, py: f64) -> Option<usize> {
    let pos = cell_of(board, px, py)?;
    if board.puzzle.island_at(pos).is_some() {
        return None;
    }
    let candidates = cells.get(pos.y as usize * board.puzzle.width as usize + pos.x as usize)?;
    let c = CELL as f64;
    candidates.iter().copied().min_by(|&e, &f| {
        let dist = |edge: usize| match board.edges[edge].orientation {
            Orientation::Horizontal => (py - (pos.y as f64 * c + c / 2.0)).abs(),
            Orientation::Vertical => (px - (pos.x as f64 * c + c / 2.0)).abs(),
        };
        dist(e).total_cmp(&dist(f))
    })
}

fn cell_of(board: &Board, px: f64, py: f64) -> Option<Pos> {
    if px < 0.0 || py < 0.0 {
        return None;
    }
    let (x, y) = ((px / CELL as f64) as u32, (py / CELL as f64) as u32);
    (x < board.puzzle.width as u32 && y < board.puzzle.height as u32).then(|| Pos::new(x as u8, y as u8))
}

/// The edge leaving `island` in the direction of the drag vector, if any.
fn edge_in_direction(board: &Board, island: usize, dx: f64, dy: f64) -> Option<usize> {
    let horizontal = dx.abs() >= dy.abs();
    let forward = if horizontal { dx > 0.0 } else { dy > 0.0 };
    board.incident[island].iter().copied().find(|&e| {
        let edge = board.edges[e];
        let is_horizontal = edge.orientation == Orientation::Horizontal;
        // `a` is left of / above `b`, so going forward from `a` follows the edge
        is_horizontal == horizontal && (edge.a == island) == forward
    })
}

/// The puzzle board: a pixel art sea with islands and bridges.
#[component]
pub fn BoardView(
    #[prop(into)] board: Signal<Board>,
    #[prop(into)] bridges: Signal<Vec<u8>>,
    #[prop(optional)] mode: BoardMode,
    /// Play mode: an edge was clicked, +1 to add a bridge (cycling 2 → 0), -1 to remove one.
    #[prop(optional)]
    on_cycle: Option<Callback<(usize, i8)>>,
    /// Edit mode: a cell was clicked or a digit typed while hovering it.
    #[prop(optional)]
    on_cell: Option<Callback<(Pos, CellAction)>>,
    /// Edges to flash as wrong.
    #[prop(into, optional)]
    wrong: Signal<Vec<usize>>,
    /// Edge to highlight as a hint.
    #[prop(into, optional)]
    hint: Signal<Option<usize>>,
    /// Max height of the board, CSS length.
    #[prop(into, optional, default = "78vh".to_string())]
    max_height: String,
    /// Draw the wooden frame around the sea.
    #[prop(optional)]
    framed: bool,
) -> impl IntoView {
    let svg_ref = NodeRef::<leptos::svg::Svg>::new();
    let cells = Memo::new(move |_| board.with(cell_edges));
    let degrees = Memo::new(move |_| board.with(|b| bridges.with(|br| b.check(br).degrees)));

    let hover_edge = RwSignal::new(None::<usize>);
    let hover_cell = RwSignal::new(None::<Pos>);
    let preview_edge = RwSignal::new(None::<usize>);
    let selected = RwSignal::new(None::<usize>);
    let blocked = RwSignal::new(None::<(f64, f64, u32)>);
    let drag = StoredValue::new(None::<Drag>);

    // A new puzzle resets all interaction state
    Effect::new(move |_| {
        board.track();
        hover_edge.set(None);
        preview_edge.set(None);
        selected.set(None);
        drag.set_value(None);
    });

    let view_box = move || {
        board.with(|b| {
            let w = b.puzzle.width as i32 * CELL + 2 * PAD;
            let h = b.puzzle.height as i32 * CELL + 2 * PAD;
            (w, h)
        })
    };

    let to_svg = move |ev: &PointerEvent| -> Option<(f64, f64)> {
        let svg = svg_ref.get_untracked()?;
        let rect = svg.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let (w, h) = view_box();
        let px = (ev.client_x() as f64 - rect.left()) / rect.width() * w as f64 - PAD as f64;
        let py = (ev.client_y() as f64 - rect.top()) / rect.height() * h as f64 - PAD as f64;
        Some((px, py))
    };

    let flash_blocked = move |edge: usize| {
        let (x, y) = board.with_untracked(|b| midpoint(b, edge));
        let key = blocked.get_untracked().map_or(0, |(_, _, k)| k + 1);
        blocked.set(Some((x, y, key)));
        set_timeout(
            move || {
                if blocked.get_untracked().is_some_and(|(_, _, k)| k == key) {
                    blocked.set(None);
                }
            },
            std::time::Duration::from_millis(650),
        );
    };

    let cycle = move |edge: usize, delta: i8| {
        let Some(cb) = on_cycle else { return };
        let current = bridges.with_untracked(|b| b.get(edge).copied().unwrap_or(0));
        let next = (current as i8 + delta).rem_euclid(3) as u8;
        let blocked = current == 0
            && next > 0
            && board.with_untracked(|b| bridges.with_untracked(|br| b.is_blocked(br, edge)));
        if blocked {
            flash_blocked(edge);
        } else {
            cb.run((edge, delta));
        }
    };

    let on_down = move |ev: PointerEvent| {
        if mode == BoardMode::View {
            return;
        }
        let Some((px, py)) = to_svg(&ev) else { return };
        let backwards = ev.button() == 2 || ev.shift_key();
        if mode == BoardMode::Edit {
            if let (Some(cb), Some(pos)) = (on_cell, board.with_untracked(|b| cell_of(b, px, py))) {
                cb.run((
                    pos,
                    if backwards {
                        CellAction::Decrement
                    } else {
                        CellAction::Increment
                    },
                ));
            }
            return;
        }
        let island = board.with_untracked(|b| cell_of(b, px, py).and_then(|p| b.puzzle.island_at(p)));
        if let Some(island) = island {
            if ev.button() == 0 {
                drag.set_value(Some(Drag {
                    island,
                    start: (px, py),
                }));
                if let Some(svg) = svg_ref.get_untracked() {
                    let _ = svg.set_pointer_capture(ev.pointer_id());
                }
            }
            return;
        }
        let edge = board.with_untracked(|b| cells.with_untracked(|c| edge_at(b, c, px, py)));
        if let Some(edge) = edge {
            selected.set(None);
            cycle(edge, if backwards { -1 } else { 1 });
        }
    };

    let on_move = move |ev: PointerEvent| {
        if mode == BoardMode::View {
            return;
        }
        let Some((px, py)) = to_svg(&ev) else { return };
        if mode == BoardMode::Edit {
            hover_cell.set(board.with_untracked(|b| cell_of(b, px, py)));
            return;
        }
        if let Some(d) = drag.get_value() {
            let (dx, dy) = (px - d.start.0, py - d.start.1);
            let edge = if dx.hypot(dy) >= DRAG_THRESHOLD {
                board.with_untracked(|b| edge_in_direction(b, d.island, dx, dy))
            } else {
                None
            };
            preview_edge.set(edge);
            return;
        }
        let edge = board.with_untracked(|b| cells.with_untracked(|c| edge_at(b, c, px, py)));
        if hover_edge.get_untracked() != edge {
            hover_edge.set(edge);
        }
    };

    let on_up = move |ev: PointerEvent| {
        let Some(d) = drag.get_value() else { return };
        drag.set_value(None);
        if let Some(svg) = svg_ref.get_untracked() {
            let _ = svg.release_pointer_capture(ev.pointer_id());
        }
        if let Some(edge) = preview_edge.get_untracked() {
            preview_edge.set(None);
            selected.set(None);
            cycle(edge, 1);
            return;
        }
        // A click on an island: select it, or build towards it from the selected one
        match selected.get_untracked() {
            Some(s) if s == d.island => selected.set(None),
            Some(s) => match board.with_untracked(|b| b.edge_between(s, d.island)) {
                Some(edge) => {
                    selected.set(None);
                    cycle(edge, 1);
                }
                None => selected.set(Some(d.island)),
            },
            None => selected.set(Some(d.island)),
        }
    };

    let on_leave = move |_| {
        hover_edge.set(None);
        hover_cell.set(None);
    };

    // Digits set the hovered cell in the editor
    if mode == BoardMode::Edit {
        Effect::new(move |_| {
            let handle = window_event_listener(leptos::ev::keydown, move |ev| {
                let (Some(cb), Some(pos)) = (on_cell, hover_cell.get_untracked()) else { return };
                let action = match ev.key().as_str() {
                    k if k.len() == 1 && ('0'..='8').contains(&k.chars().next().unwrap_or(' ')) => {
                        CellAction::Set(k.as_bytes()[0] - b'0')
                    }
                    "Backspace" | "Delete" | " " => CellAction::Set(0),
                    "+" | "ArrowUp" => CellAction::Increment,
                    "-" | "ArrowDown" => CellAction::Decrement,
                    _ => return,
                };
                ev.prevent_default();
                cb.run((pos, action));
            });
            on_cleanup(move || handle.remove());
        });
    }

    let bridges_layer = move || {
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
    };

    // What the next click would build on the hovered gap / dragged direction
    let ghost_layer = move || {
        let edge = preview_edge.get().or_else(|| hover_edge.get())?;
        board.with(|b| {
            let current = bridges.with(|br| br.get(edge).copied().unwrap_or(0));
            let blocked = current == 0 && bridges.with(|br| b.is_blocked(br, edge));
            (current < 2 && !blocked)
                .then(|| view! { <BridgeSprite span=span_of(b, edge) count=current + 1 class="ghost" /> })
        })
    };

    let islands_layer = move || {
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
                    let mut c = String::from("island");
                    if mode == BoardMode::Play {
                        let d = degrees.with(|d| d.get(i).copied().unwrap_or(0));
                        if d == required {
                            c.push_str(" done");
                        } else if d > required {
                            c.push_str(" over");
                        }
                        if selected.get() == Some(i) {
                            c.push_str(" selected");
                        }
                    }
                    c
                };
                view! {
                    <g
                        class=class
                        transform=format!("translate({},{})", x as i32 * CELL, y as i32 * CELL)
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
    };

    let grid_dots = move || {
        (mode == BoardMode::Edit).then(|| {
            board.with(|b| {
                let mut dots = Vec::new();
                for y in 0..b.puzzle.height {
                    for x in 0..b.puzzle.width {
                        if b.puzzle.island_at(Pos::new(x, y)).is_none() {
                            dots.push(view! {
                                <rect
                                    class="grid-dot"
                                    x=x as i32 * CELL + 7
                                    y=y as i32 * CELL + 7
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
    };

    let hover_cell_layer = move || {
        let pos = hover_cell.get()?;
        (mode == BoardMode::Edit).then(|| {
            view! {
                <rect
                    x=pos.x as i32 * CELL
                    y=pos.y as i32 * CELL
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
    };

    let blocked_layer = move || {
        blocked.get().map(|(x, y, key)| {
            let (x, y) = (x.round() as i32, y.round() as i32);
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
    };

    let frame = if framed { 14 } else { 0 };
    let frame_style = move || {
        let (w, h) = view_box();
        format!("width: min(100%, calc({max_height} * {w} / {h} + {frame}px));")
    };

    view! {
        <div
            class="mx-auto"
            class=(["rounded-2xl", "bg-[var(--px-wood)]", "p-[7px]", "pixel-frame"], framed)
            style=frame_style
        >
        <svg
            node_ref=svg_ref
            class="board-svg w-full"
            class:rounded-xl=framed
            class:cursor-crosshair=mode == BoardMode::Edit
            style=move || {
                let (w, h) = view_box();
                format!("aspect-ratio: {w} / {h};")
            }
            viewBox=move || {
                let (w, h) = view_box();
                format!("{} {} {w} {h}", -PAD, -PAD)
            }
            xmlns="http://www.w3.org/2000/svg"
            role="img"
            aria-label="Hashiwokakero board"
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointercancel=move |_| {
                drag.set_value(None);
                preview_edge.set(None);
            }
            on:pointerleave=on_leave
            on:contextmenu=|ev| ev.prevent_default()
        >
            <BoardDefs />
            {move || {
                let (w, h) = view_box();
                view! { <rect x=-PAD y=-PAD width=w height=h fill="url(#sea)" /> }
            }}
            {grid_dots}
            <g>{bridges_layer}</g>
            <g>{ghost_layer}</g>
            <g>{islands_layer}</g>
            {hover_cell_layer}
            {blocked_layer}
        </svg>
        </div>
    }
}
