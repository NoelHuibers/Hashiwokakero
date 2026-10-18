//! The puzzle board: a pixel art sea with islands and bridges, built by click, drag or tap.

mod geometry;
mod input;
pub mod layers;

use common::{Board, Pos};
use leptos::ev::PointerEvent;
use leptos::prelude::*;

use super::sprites::{BoardDefs, CELL};
use crate::i18n::{t_string, use_i18n};
use geometry::{cell_edges, cell_of, edge_at, edge_in_direction, midpoint, whole_px};
use input::{Drag, Touches};
use layers::{BlockedMark, Bridges, GhostBridge, GridDots, HoverCell, Islands};

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

/// Fingers that move less than this (CSS pixels) still tap.
const TAP_SLOP: f64 = 10.0;
/// The largest zoom gives cells of about this many CSS pixels.
const MAX_CELL_PX: f64 = 56.0;
/// One press of + or −.
const ZOOM_STEP: f64 = 1.4;

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
    let i18n = use_i18n();
    let svg_ref = NodeRef::<leptos::svg::Svg>::new();
    let cells = Memo::new(move |_| board.with(cell_edges));
    let degrees = Memo::new(move |_| board.with(|b| bridges.with(|br| b.check(br).degrees)));

    let hover_edge = RwSignal::new(None::<usize>);
    let hover_cell = RwSignal::new(None::<Pos>);
    let preview_edge = RwSignal::new(None::<usize>);
    let selected = RwSignal::new(None::<usize>);
    let blocked = RwSignal::new(None::<(f64, f64, u32)>);
    let drag = StoredValue::new(None::<Drag>);
    let touches = StoredValue::new(Touches::default());
    // 1 shows the whole board, bigger zooms in and the board scrolls
    let scale = RwSignal::new(1.0_f64);
    let scroller = NodeRef::<leptos::html::Div>::new();

    // A new puzzle resets all interaction state
    Effect::new(move |_| {
        board.track();
        hover_edge.set(None);
        preview_edge.set(None);
        selected.set(None);
        drag.set_value(None);
        scale.set(1.0);
    });

    let view_box = move || {
        board.with(|b| {
            let w = i32::from(b.puzzle.width) * CELL + 2 * PAD;
            let h = i32::from(b.puzzle.height) * CELL + 2 * PAD;
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
        let px =
            (f64::from(ev.client_x()) - rect.left()) / rect.width() * f64::from(w) - f64::from(PAD);
        let py =
            (f64::from(ev.client_y()) - rect.top()) / rect.height() * f64::from(h) - f64::from(PAD);
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
        let next = (current.cast_signed() + delta)
            .rem_euclid(3)
            .cast_unsigned();
        let blocked = current == 0
            && next > 0
            && board.with_untracked(|b| bridges.with_untracked(|br| b.is_blocked(br, edge)));
        if blocked {
            crate::audio::play(crate::audio::Sfx::Blocked);
            flash_blocked(edge);
        } else {
            crate::audio::play(match next {
                0 => crate::audio::Sfx::Remove,
                1 => crate::audio::Sfx::Place,
                _ => crate::audio::Sfx::Double,
            });
            cb.run((edge, delta));
        }
    };

    // A chime when an island gets all its bridges (not when a game is loaded or reset)
    if mode == BoardMode::Play {
        Effect::new(move |prev: Option<(usize, Vec<u8>)>| {
            let done = board.with(|b| {
                degrees.with(|d| {
                    b.puzzle
                        .islands
                        .iter()
                        .zip(d)
                        .filter(|(i, d)| i.required == **d)
                        .count()
                })
            });
            let now = bridges.get();
            if let Some((before, last)) = prev {
                let changed = now.len() == last.len()
                    && now.iter().zip(&last).filter(|(a, b)| a != b).count() == 1;
                if changed && done > before {
                    crate::audio::play(crate::audio::Sfx::Island);
                }
            }
            (done, now)
        });
    }

    // The zoom that makes cells about MAX_CELL_PX wide, given how wide the board fits
    let max_scale = move || {
        let width = scroller
            .get_untracked()
            .map_or(0.0, |el| f64::from(el.client_width()));
        let (w, _) = view_box();
        let cells = f64::from(w) / f64::from(CELL);
        if width <= 0.0 {
            4.0
        } else {
            (MAX_CELL_PX * cells / width).clamp(1.0, 6.0)
        }
    };
    // Zooms to `target`, keeping the board point under `focus` (screen coordinates) in place
    let zoom_to = move |target: f64, focus: (f64, f64)| {
        let Some(el) = scroller.get_untracked() else {
            return;
        };
        let old = scale.get_untracked();
        let new = target.clamp(1.0, max_scale());
        if (new - old).abs() < 1e-3 {
            return;
        }
        let rect = el.get_bounding_client_rect();
        let (fx, fy) = (focus.0 - rect.left(), focus.1 - rect.top());
        let (cx, cy) = (
            f64::from(el.scroll_left()) + fx,
            f64::from(el.scroll_top()) + fy,
        );
        let ratio = new / old;
        scale.set(new);
        // After the new size is laid out
        request_animation_frame(move || {
            el.set_scroll_left(whole_px(cx * ratio - fx));
            el.set_scroll_top(whole_px(cy * ratio - fy));
        });
    };
    let pan_by = move |(dx, dy): (f64, f64)| {
        if let Some(el) = scroller.get_untracked() {
            el.set_scroll_left(el.scroll_left() - whole_px(dx));
            el.set_scroll_top(el.scroll_top() - whole_px(dy));
        }
    };
    let zoom_step = move |factor: f64| {
        let Some(el) = scroller.get_untracked() else {
            return;
        };
        let rect = el.get_bounding_client_rect();
        let center = (
            rect.left() + rect.width() / 2.0,
            rect.top() + rect.height() / 2.0,
        );
        zoom_to(scale.get_untracked() * factor, center);
    };

    let on_down = move |ev: PointerEvent| {
        if mode == BoardMode::View {
            return;
        }
        let touch = ev.pointer_type() == "touch";
        if touch {
            let (x, y) = (f64::from(ev.client_x()), f64::from(ev.client_y()));
            let fingers = touches.try_update_value(|t| {
                t.active.retain(|(id, ..)| *id != ev.pointer_id());
                t.active.push((ev.pointer_id(), x, y));
                if t.active.len() == 1 {
                    *t = Touches {
                        active: t.active.clone(),
                        start: (x, y),
                        ..Touches::default()
                    };
                }
                t.active.len()
            });
            if fingers.unwrap_or(1) >= 2 {
                // A pinch: whatever the first finger started is off
                drag.set_value(None);
                preview_edge.set(None);
                let current = scale.get_untracked();
                touches.update_value(|t| {
                    t.moved = true;
                    t.tap = None;
                    t.pinch = t.spread().map(|d| (d, current));
                });
                return;
            }
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
        let island =
            board.with_untracked(|b| cell_of(b, px, py).and_then(|p| b.puzzle.island_at(p)));
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
            if touch {
                touches.update_value(|t| t.tap = Some(edge));
            } else {
                selected.set(None);
                cycle(edge, if backwards { -1 } else { 1 });
            }
        }
    };

    let on_move = move |ev: PointerEvent| {
        if mode == BoardMode::View {
            return;
        }
        if ev.pointer_type() == "touch" {
            let (x, y) = (f64::from(ev.client_x()), f64::from(ev.client_y()));
            let mut pan = None;
            let mut pinch = None;
            touches.update_value(|t| {
                let before = t.midpoint();
                let last = t
                    .active
                    .iter()
                    .find(|(id, ..)| *id == ev.pointer_id())
                    .map(|&(_, lx, ly)| (lx, ly));
                if let Some(p) = t.active.iter_mut().find(|(id, ..)| *id == ev.pointer_id()) {
                    *p = (ev.pointer_id(), x, y);
                }
                if (x - t.start.0).hypot(y - t.start.1) > TAP_SLOP {
                    t.moved = true;
                    t.tap = None;
                }
                if let (Some((d0, s0)), Some(d), Some(mid), Some(before)) =
                    (t.pinch, t.spread(), t.midpoint(), before)
                {
                    pinch = Some((
                        s0 * d / d0.max(1.0),
                        mid,
                        (mid.0 - before.0, mid.1 - before.1),
                    ));
                } else if t.active.len() == 1 && t.moved {
                    pan = last.map(|(lx, ly)| (x - lx, y - ly));
                }
            });
            if let Some((target, mid, shift)) = pinch {
                zoom_to(target, mid);
                pan_by(shift);
                return;
            }
            // One finger on a zoomed board moves it, unless it drags from an island
            if let Some(shift) = pan
                && drag.with_value(Option::is_none)
                && scale.get_untracked() > 1.0
            {
                pan_by(shift);
                return;
            }
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
        if ev.pointer_type() == "touch" {
            let mut tap = None;
            touches.update_value(|t| {
                t.active.retain(|(id, ..)| *id != ev.pointer_id());
                if t.active.len() < 2 {
                    t.pinch = None;
                }
                if t.active.is_empty() && !t.moved {
                    tap = t.tap.take();
                }
            });
            if let Some(edge) = tap {
                selected.set(None);
                cycle(edge, 1);
                return;
            }
        }
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
                let (Some(cb), Some(pos)) = (on_cell, hover_cell.get_untracked()) else {
                    return;
                };
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

    let frame = if framed { 14 } else { 0 };
    // Big boards get small cells on phones: pinch, or use + and −, to zoom in
    let can_zoom =
        move || mode != BoardMode::View && board.with(|b| b.puzzle.width.max(b.puzzle.height) >= 9);
    let zoomed = move || scale.get() > 1.0 + 1e-3;
    // Outside of view!, where `>` would end the tag
    let at_max = move || scale.get() >= max_scale() - 1e-3;
    let frame_style = move || {
        let (w, h) = view_box();
        format!(
            "width: calc(min(100%, calc({max_height} * {w} / {h} + {frame}px)) * {:.4});",
            scale.get()
        )
    };

    view! {
        <div>
        <div
            node_ref=scroller
            class=move || if zoomed() {
                "max-h-[72vh] overflow-auto overscroll-contain rounded-2xl"
            } else {
                ""
            }
        >
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
            aria-label=move || t_string!(i18n, common.board.label)
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointercancel=move |ev: PointerEvent| {
                drag.set_value(None);
                preview_edge.set(None);
                touches.update_value(|t| {
                    t.active.retain(|(id, ..)| *id != ev.pointer_id());
                    t.pinch = None;
                    t.tap = None;
                });
            }
            on:pointerleave=on_leave
            on:contextmenu=|ev| ev.prevent_default()
        >
            <BoardDefs />
            {move || {
                let (w, h) = view_box();
                view! { <rect x=-PAD y=-PAD width=w height=h fill="url(#sea)" /> }
            }}
            <GridDots board mode />
            <g>
                <Bridges board bridges wrong hint />
            </g>
            <g>
                <GhostBridge board bridges preview_edge hover_edge />
            </g>
            <g>
                <Islands board degrees mode selected />
            </g>
            <HoverCell hover_cell mode />
            <BlockedMark blocked />
        </svg>
        </div>
        </div>
        <Show when=can_zoom>
            <div class="mt-2 flex items-center justify-end gap-2">
                <span class="me-auto text-xs text-muted-foreground md:hidden">
                    {move || if zoomed() { t_string!(i18n, common.board.zoom_hint) } else { t_string!(i18n, common.board.pinch) }}
                </span>
                <div class="flex items-center overflow-hidden rounded-lg border bg-card shadow-xs">
                    <button
                        type="button"
                        class="grid h-9 w-10 place-items-center font-pixel text-lg disabled:opacity-40"
                        title=move || t_string!(i18n, common.board.zoom_out)
                        aria-label=move || t_string!(i18n, common.board.zoom_out)
                        disabled=move || !zoomed()
                        on:click=move |_| zoom_step(1.0 / ZOOM_STEP)
                    >
                        "－"
                    </button>
                    <button
                        type="button"
                        class="h-9 min-w-14 border-x px-2 font-pixel text-xs tabular-nums disabled:opacity-60"
                        title=move || t_string!(i18n, common.board.fit)
                        aria-label=move || t_string!(i18n, common.board.fit)
                        disabled=move || !zoomed()
                        on:click=move |_| scale.set(1.0)
                    >
                        {move || format!("{:.0}%", scale.get() * 100.0)}
                    </button>
                    <button
                        type="button"
                        class="grid h-9 w-10 place-items-center font-pixel text-lg disabled:opacity-40"
                        title=move || t_string!(i18n, common.board.zoom)
                        aria-label=move || t_string!(i18n, common.board.zoom)
                        disabled=at_max
                        on:click=move |_| zoom_step(ZOOM_STEP)
                    >
                        "＋"
                    </button>
                </div>
            </div>
        </Show>
        </div>
    }
}
