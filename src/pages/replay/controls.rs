//! The theatre's controls: play and pause, a move back or on, the speed, together or apart.
//!
//! The timeline is a strip of sea a little boat sails along, with a pennant in each member's colour
//! where they finished and a mark where they took a hint or a check.

use leptos::ev::{KeyboardEvent, PointerEvent};
use leptos::prelude::*;

use crate::components::sprites::{BOAT, PENNANT, PixelSprite, runs};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::model::{MarkKind, SPEEDS, colour_var, format_duration, fraction};

use super::state::{Layout, Theatre};

/// A button of a segmented control (speed, layout), the look of the ranking's tabs.
#[component]
fn Segment(
    #[prop(into)] pressed: Signal<bool>,
    #[prop(into)] label: Signal<String>,
    on_press: impl Fn() + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <button
            type="button"
            aria-pressed=move || pressed.get().to_string()
            aria-label=label
            data-tooltip=label
            class="flex min-h-9 shrink-0 items-center gap-1 rounded-md px-2.5 text-sm font-medium whitespace-nowrap transition-colors focus-visible:outline-2 focus-visible:outline-ring"
            class=(["bg-background", "text-foreground", "shadow-sm"], move || pressed.get())
            class=(["text-foreground/60", "hover:text-foreground"], move || !pressed.get())
            on:click=move |_| on_press()
        >
            {children()}
        </button>
    }
}

/// What the keys do anywhere on the theatre's page, `true` if it was one of them.
pub fn on_key(theatre: Theatre, key: &str, shift: bool) -> bool {
    match key {
        " " | "k" => theatre.toggle_play(),
        "ArrowRight" if shift => theatre.jump(true),
        "ArrowLeft" if shift => theatre.jump(false),
        "ArrowRight" => theatre.step(true),
        "ArrowLeft" => theatre.step(false),
        "Home" => {
            theatre.pause();
            theatre.seek(0.0);
        }
        "End" => {
            theatre.pause();
            theatre.seek(f64::from(theatre.end));
        }
        _ => return false,
    }
    true
}

/// A place on the timeline, 0 to 1. The last pixels at either end are the end: a finger
/// can't hit the very last millisecond.
pub fn snap(f: f64) -> f64 {
    match f {
        f if f <= 0.005 => 0.0,
        f if f >= 0.995 => 1.0,
        f => f,
    }
}

/// The element a pointer event's handler is on: the timeline, not the part of it under the
/// pointer.
fn current(ev: &PointerEvent) -> Option<web_sys::Element> {
    wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(ev.current_target()?).ok()
}

/// Where on the timeline a pointer is, 0 to 1.
fn pointer_fraction(ev: &PointerEvent) -> Option<f64> {
    let el = current(ev)?;
    let rect = el.get_bounding_client_rect();
    (rect.width() > 0.0).then(|| snap((f64::from(ev.client_x()) - rect.left()) / rect.width()))
}

/// The timeline: the sea strip, the boat at the clock's time, pennants and marks.
#[component]
fn Timeline(theatre: Theatre) -> impl IntoView {
    let i18n = use_i18n();
    let end = theatre.end;
    let at = move || fraction(theatre.now(), end) * 100.0;
    let dragging = StoredValue::new(false);
    let seek_to = move |ev: &PointerEvent| {
        if let Some(f) = pointer_fraction(ev) {
            theatre.seek(f * f64::from(end));
        }
    };
    let lanes: Vec<_> = (0..theatre.lanes())
        .filter_map(|i| theatre.lane(i).map(|l| (i, l)))
        .collect();
    let flags = lanes
        .iter()
        .filter_map(|(i, l)| l.finish().map(|f| (*i, l.name.clone(), l.colour, f)))
        .map(|(i, name, colour, finish)| {
            let tip = move || {
                td_string!(i18n.get_locale(), replay.flag, name = name.clone(), time = format_duration(i64::from(finish)))
            };
            view! {
                <span
                    class="absolute bottom-0 -translate-x-px transition-opacity"
                    class=(["opacity-25"], move || !theatre.shown(i))
                    style=format!("left: {:.3}%; --member: {}", fraction(finish, end) * 100.0, colour_var(colour))
                    data-tooltip=tip.clone()
                    aria-label=tip
                    role="img"
                    data-flag=i
                >
                    <PixelSprite art=&PENNANT class="h-4 w-auto" />
                </span>
            }
        })
        .collect_view();
    let marks = lanes
        .iter()
        .flat_map(|(i, l)| {
            let (i, name, colour) = (*i, l.name.clone(), l.colour);
            // A newer server's kinds of help this build doesn't know get no marker
            l.marks
                .iter()
                .filter(|m| m.kind != MarkKind::Unknown)
                .map(move |m| (i, name.clone(), colour, *m))
        })
        .map(|(i, name, colour, mark)| {
            let tip = move || {
                let l = i18n.get_locale();
                let time = format_duration(i64::from(mark.t));
                match mark.kind {
                    MarkKind::Hint => td_string!(l, replay.marks.hint, name = name.clone(), time),
                    MarkKind::Check | MarkKind::Unknown => {
                        td_string!(l, replay.marks.check, name = name.clone(), time)
                    }
                }
            };
            let shape = match mark.kind {
                MarkKind::Hint => "rounded-full",
                MarkKind::Check | MarkKind::Unknown => "rotate-45 rounded-[1px]",
            };
            view! {
                <span
                    class=format!("absolute top-0.5 size-2 -translate-x-1/2 ring-1 ring-card {shape}")
                    class=(["opacity-25"], move || !theatre.shown(i))
                    style=format!("left: {:.3}%; background: {}", fraction(mark.t, end) * 100.0, colour_var(colour))
                    data-tooltip=tip.clone()
                    aria-label=tip
                    role="img"
                    data-mark=format!("{:?}", mark.kind).to_lowercase()
                ></span>
            }
        })
        .collect_view();
    let value_text = move || {
        td_string!(
            i18n.get_locale(),
            replay.controls.position,
            time = format_duration(i64::from(theatre.now())),
            end = format_duration(i64::from(end))
        )
    };
    view! {
        // Time runs left to right in every language, like on a clock's dial
        <div dir="ltr" class="relative select-none px-2">
            <div class="relative h-5">{flags}</div>
            <div
                class="relative h-6 cursor-pointer touch-none rounded-md outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
                role="slider"
                tabindex="0"
                aria-label=move || t_string!(i18n, replay.controls.timeline)
                aria-valuemin="0"
                aria-valuemax=end
                aria-valuenow=move || theatre.now()
                aria-valuetext=value_text
                data-testid="replay-timeline"
                on:pointerdown=move |ev: PointerEvent| {
                    if let Some(el) = current(&ev) {
                        let _ = el.set_pointer_capture(ev.pointer_id());
                    }
                    dragging.set_value(true);
                    theatre.pause();
                    seek_to(&ev);
                }
                on:pointermove=move |ev: PointerEvent| {
                    if dragging.get_value() {
                        seek_to(&ev);
                    }
                }
                on:pointerup=move |_| dragging.set_value(false)
                on:pointercancel=move |_| dragging.set_value(false)
                on:keydown=move |ev: KeyboardEvent| {
                    // The page's keys, the slider's own way of moving would be the same
                    if on_key(theatre, &ev.key(), ev.shift_key()) {
                        ev.prevent_default();
                        ev.stop_propagation();
                    }
                }
            >
                // The water still ahead is faint, the stretch sailed in the sea's colour (the light
                // one at night, where the sea itself is darker than the card)
                <div class="absolute inset-x-0 top-1/2 h-2 -translate-y-1/2 overflow-hidden rounded-full bg-foreground/10">
                    <div class="h-full rounded-full bg-[var(--px-sea)] dark:bg-[var(--px-sea-light)]" style=move || format!("width: {:.3}%", at())></div>
                </div>
                <span class="pointer-events-none absolute top-1/2 -translate-x-1/2 -translate-y-[70%]" style=move || format!("left: {:.3}%", at())>
                    <PixelSprite art=&BOAT class="h-6 w-auto drop-shadow-sm" />
                </span>
            </div>
            <div class="relative h-3">{marks}</div>
        </div>
    }
}

/// Pixel icons for the transport buttons, drawn in the button's text colour.
const PLAY: [&str; 11] = [
    "x.......", "xx......", "xxxx....", "xxxxx...", "xxxxxxx.", "xxxxxxxx", "xxxxxxx.", "xxxxx...",
    "xxxx....", "xx......", "x.......",
];
const PAUSE: [&str; 11] = ["xxx..xxx"; 11];
const AGAIN: [&str; 9] = [
    "...x.....",
    "..xxxxx..",
    ".xxx...x.",
    "..x.....x",
    "x.......x",
    "x.......x",
    "x.......x",
    ".x.....x.",
    "..xxxxx..",
];
const PREVIOUS: [&str; 7] = [
    "xx....x", "xx...xx", "xx..xxx", "xx.xxxx", "xx..xxx", "xx...xx", "xx....x",
];
const NEXT: [&str; 7] = [
    "x....xx", "xx...xx", "xxx..xx", "xxxx.xx", "xxx..xx", "xx...xx", "x....xx",
];

/// A pixel icon in `currentColor`, `scale` screen pixels to an art pixel: a whole number keeps
/// every pixel the same size.
#[component]
fn Glyph(
    art: &'static [&'static str],
    scale: usize,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    let (w, h) = (art.iter().map(|r| r.len()).max().unwrap_or(0), art.len());
    view! {
        <svg
            class=class
            width=w * scale
            height=h * scale
            viewBox=format!("0 0 {w} {h}")
            shape-rendering="crispEdges"
            aria-hidden="true"
        >
            {runs(art)
                .into_iter()
                .map(|(x, y, w, _)| {
                    view! { <rect x=x y=y width=w height="1" fill="currentColor" /> }
                })
                .collect_view()}
        </svg>
    }
}

/// Play, pause, step, speed, layout and the timeline.
///
/// Laid out like a media player: the timeline across the top with the clock under its ends,
/// the transport in the middle, the speed and the layout under it (beside it from `sm` on).
#[component]
pub fn Controls(theatre: Theatre, #[prop(optional)] still: bool) -> impl IntoView {
    let i18n = use_i18n();
    let playing = move || theatre.playing.get();
    let at_end = Memo::new(move |_| theatre.now() >= theatre.end);
    let at_end = move || at_end.get() && theatre.end > 0;
    let play_label = move || {
        if playing() {
            t_string!(i18n, replay.controls.pause)
        } else if at_end() {
            t_string!(i18n, replay.controls.again)
        } else {
            t_string!(i18n, replay.controls.play)
        }
    };
    let step_button = move |forward: bool| {
        let label = move || {
            if forward {
                t_string!(i18n, replay.controls.next)
            } else {
                t_string!(i18n, replay.controls.previous)
            }
        };
        view! {
            <button
                type="button"
                class="grid size-11 place-items-center rounded-full text-foreground/70 transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"
                data-tooltip=label
                aria-label=label
                disabled=theatre.end == 0
                on:click=move |_| theatre.step(forward)
            >
                <Glyph art=if forward { &NEXT } else { &PREVIOUS } scale=2 />
            </button>
        }
    };
    view! {
        <div class="flex flex-col gap-3" data-testid="replay-controls">
            <div class="flex flex-col gap-1">
                <Timeline theatre />
                <p
                    dir="ltr"
                    class="flex justify-between px-1 font-pixel text-xs tabular-nums"
                    data-testid="replay-clock"
                >
                    <span class="text-foreground">
                        {move || format_duration(i64::from(theatre.now()))}
                    </span>
                    <span class="text-muted-foreground">
                        {format_duration(i64::from(theatre.end))}
                    </span>
                </p>
            </div>
            <div class="grid grid-cols-2 items-center gap-x-2 gap-y-3 sm:grid-cols-[1fr_auto_1fr]">
                // Back, play, on: the middle of the card on every width
                <div
                    dir="ltr"
                    class="col-span-2 flex items-center justify-center gap-3 sm:col-span-1 sm:col-start-2 sm:row-start-1"
                >
                    {step_button(false)}
                    <button
                        type="button"
                        class="grid size-14 place-items-center rounded-full bg-primary text-primary-foreground shadow-md transition-transform hover:scale-105 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring active:scale-95 disabled:opacity-40"
                        aria-label=play_label
                        data-tooltip=play_label
                        data-testid="replay-play"
                        data-playing=move || playing().to_string()
                        disabled=theatre.end == 0
                        on:click=move |_| theatre.toggle_play()
                    >
                        {move || {
                            if playing() {
                                view! { <Glyph art=&PAUSE scale=2 /> }.into_any()
                            } else if at_end() {
                                view! { <Glyph art=&AGAIN scale=3 /> }.into_any()
                            } else {
                                // A triangle's weight sits left of its box: nudged to look centred
                                view! { <Glyph art=&PLAY scale=2 class="ms-1" /> }
                                    .into_any()
                            }
                        }}
                    </button>
                    {step_button(true)}
                </div>
                <div
                    role="group"
                    aria-label=move || t_string!(i18n, replay.controls.speed)
                    class="flex w-fit gap-0.5 justify-self-start rounded-lg bg-muted p-[3px] sm:row-start-1"
                >
                    {SPEEDS
                        .into_iter()
                        .map(|speed| {
                            view! {
                                <Segment
                                    pressed=Signal::derive(move || theatre.speed.get() == speed)
                                    label=Signal::derive(move || {
                                        td_string!(
                                            i18n.get_locale(), replay.controls.speed_n, n = speed
                                        )
                                    })
                                    on_press=move || theatre.speed.set(speed)
                                >
                                    <span class="font-pixel text-xs">{format!("{speed}×")}</span>
                                </Segment>
                            }
                        })
                        .collect_view()}
                </div>
                <div
                    role="group"
                    aria-label=move || t_string!(i18n, replay.controls.layout)
                    class="flex w-fit gap-0.5 justify-self-end rounded-lg bg-muted p-[3px] sm:col-start-3 sm:row-start-1"
                >
                    <Segment
                        pressed=Signal::derive(move || theatre.layout.get() == Layout::Together)
                        label=Signal::derive(move || {
                            t_string!(i18n, replay.controls.together).to_string()
                        })
                        on_press=move || theatre.layout.set(Layout::Together)
                    >
                        <span class="font-pixel" aria-hidden="true">
                            "重"
                        </span>
                        <span class="hidden lg:inline">{t!(i18n, replay.controls.together)}</span>
                    </Segment>
                    <Segment
                        pressed=Signal::derive(move || theatre.layout.get() == Layout::Apart)
                        label=Signal::derive(move || {
                            t_string!(i18n, replay.controls.apart).to_string()
                        })
                        on_press=move || theatre.layout.set(Layout::Apart)
                    >
                        <span class="font-pixel" aria-hidden="true">
                            "並"
                        </span>
                        <span class="hidden lg:inline">{t!(i18n, replay.controls.apart)}</span>
                    </Segment>
                </div>
            </div>
            // The keys are no help on a touch screen; reduced motion's note is
            <p
                class="border-t pt-3 text-center text-xs text-muted-foreground"
                class=(["hidden", "pointer-fine:block"], !still)
            >
                {move || {
                    if still {
                        t_string!(i18n, replay.controls.still)
                    } else {
                        t_string!(i18n, replay.controls.keys)
                    }
                }}
            </p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Lane, LaneOutcome, MoveEvent, ReplayData};

    #[test]
    fn keys_play_step_and_jump() {
        Owner::new().with(|| {
            let lane = Lane {
                user_id: 1,
                name: "aiko".into(),
                guest: false,
                colour: 0,
                outcome: LaneOutcome::Solved {
                    time_ms: 1,
                    hints: 0,
                    checks: 0,
                    ranked: true,
                },
                log: [400, 30_000]
                    .into_iter()
                    .map(|t| MoveEvent {
                        t,
                        edge: 0,
                        count: 1,
                        tap: true,
                    })
                    .collect(),
                marks: Vec::new(),
            };
            let mut puzzle = common::Puzzle::empty(3, 1);
            puzzle.set(common::Pos::new(0, 0), 1);
            puzzle.set(common::Pos::new(2, 0), 1);
            let t = Theatre::new(
                ReplayData {
                    day: 1,
                    today: 2,
                    group: None,
                    me: 1,
                    locked: false,
                    puzzle: Some(puzzle),
                    lanes: vec![lane],
                    group_puzzle: None,
                },
                None,
            );
            assert!(on_key(t, " ", false));
            assert!(t.playing.get());
            assert!(on_key(t, "ArrowRight", false));
            assert!(!t.playing.get());
            assert_eq!(t.now(), 400);
            assert!(on_key(t, "ArrowRight", true));
            assert_eq!(t.now(), 10_400);
            assert!(on_key(t, "End", false));
            assert_eq!(t.now(), 30_000);
            assert!(on_key(t, "ArrowLeft", true));
            assert_eq!(t.now(), 20_000);
            assert!(on_key(t, "ArrowLeft", false));
            assert_eq!(t.now(), 400);
            assert!(on_key(t, "Home", false));
            assert_eq!(t.now(), 0);
            assert!(!on_key(t, "x", false));
            assert!(snap(-0.2).abs() < 1e-9 && snap(0.003).abs() < 1e-9);
            assert!((snap(0.5) - 0.5).abs() < 1e-9);
            assert!((snap(0.997) - 1.0).abs() < 1e-9 && (snap(1.4) - 1.0).abs() < 1e-9);
        });
    }

    /// The keys' note only where there's a mouse, reduced motion's everywhere; the clock under
    /// the timeline, the play button drawn as a pixel icon.
    #[tokio::test]
    async fn controls_render() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            let theatre = Theatre::new(super::super::tests::data(), None);
            let keys = view! { <Controls theatre /> }.to_html();
            let still = view! { <Controls theatre still=true /> }.to_html();
            assert!(keys.contains("hidden pointer-fine:block"), "{keys}");
            assert!(keys.contains("Space plays and pauses"));
            assert!(!still.contains("pointer-fine:block"), "{still}");
            assert!(still.contains("Reduced motion is on"));
            let clock = keys.find("replay-clock").unwrap();
            assert!(keys.find("replay-timeline").unwrap() < clock);
            assert!(keys[..keys.find("replay-play").unwrap()].contains("0:00.0"));
            assert!(!keys.contains('▶'));
        });
    }
}
