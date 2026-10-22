//! Tooltips: one floating label for the whole app, shown next to any element with a
//! `data-tooltip` attribute, in place of the browser's plain `title` box.
//!
//! rust-ui's Tooltip (`ui add tooltip`) is CSS only: it wraps the trigger and shows the label next
//! to it on `group-hover`. Inside a scrolling container (the leaderboard table, the board) that
//! label is cut off, it can't flip at the edge of the screen and never shows for the keyboard or a
//! finger. So this keeps its look (ink on paper, a small arrow) with another mechanism: a single
//! `role="tooltip"` element, fixed above or below whatever is hovered or focused.
//!
//! ```rust
//! use leptos::prelude::*;
//!
//! let button = view! { <button data-tooltip="Zoom in" aria-label="Zoom in">"＋"</button> };
//! let html = button.to_html();
//! assert!(html.contains(r#"data-tooltip="Zoom in""#));
//! assert!(html.contains(r#"aria-label="Zoom in""#));
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, FocusEvent, HtmlElement, KeyboardEvent, PointerEvent};

const SELECTOR: &str = "[data-tooltip]";
const ID: &str = "tooltip";
/// No `duration-*` here: without a `transition-property` that makes every property a transition,
/// `left`/`top` too, and the tooltip would slide over from the corner it was put in before
/// `place`. `animate-in` already lasts 150ms.
const CLASS: &str = "group pixel-shadow pointer-events-none fixed top-0 left-0 z-[70] w-max max-w-[min(18rem,calc(100vw-1rem))] bg-foreground px-2.5 py-1.5 text-center text-xs leading-snug font-medium text-balance wrap-anywhere text-background animate-in fade-in-0 zoom-in-95 data-[side=bottom]:slide-in-from-top-1 data-[side=top]:slide-in-from-bottom-1 motion-reduce:animate-none";
/// Resting on an element this long shows its tooltip...
const DELAY: Duration = Duration::from_millis(400);
/// ...but right after one closed, the next one opens at once, for skimming along a row
const SKIP_DELAY_MS: f64 = 300.0;
/// A tapped tooltip goes away by itself
const TOUCH_MS: Duration = Duration::from_millis(2500);
/// Space between the trigger and the arrow tip, and kept free at the edges of the screen
const GAP: f64 = 6.0;
const EDGE: f64 = 8.0;
/// The stepped pixel outline (`pixel-shadow`) and the arrow stick out of the box this far
const OUTLINE: f64 = 3.0;
const ARROW: f64 = 6.0;

#[derive(Default)]
struct State {
    /// The element the tooltip is shown (or about to be shown) for
    trigger: Option<Element>,
    open: bool,
    timer: Option<TimeoutHandle>,
    /// `Date.now()` when the last tooltip closed
    closed_at: f64,
    /// Whether `aria-describedby` was set on the trigger, to take it away again
    described: bool,
}

type Shared = Rc<RefCell<State>>;

/// The tooltip element itself, put once at the root of the app. It listens on the window, so
/// every `data-tooltip` works, including ones rendered later.
#[component]
pub fn Tooltips() -> impl IntoView {
    let root = NodeRef::<leptos::html::Div>::new();
    let text = NodeRef::<leptos::html::Span>::new();

    Effect::new(move |_| {
        let state = Shared::default();
        let tip = move || -> Option<(HtmlElement, HtmlElement)> {
            Some((root.get_untracked()?.into(), text.get_untracked()?.into()))
        };

        let hover = {
            let state = state.clone();
            window_event_listener(leptos::ev::pointerover, move |ev: PointerEvent| {
                if ev.pointer_type() == "touch" {
                    return;
                }
                let Some((root, text)) = tip() else { return };
                let target = ev.target().and_then(|t| t.dyn_into::<Element>().ok());
                let over = target.as_ref().and_then(trigger_of);
                let current = state.borrow().trigger.clone();
                match (over, current) {
                    (Some(over), Some(current)) if over == current => {}
                    (Some(over), _) => {
                        let skip = state.borrow().open
                            || js_sys::Date::now() - state.borrow().closed_at < SKIP_DELAY_MS;
                        if skip {
                            show(&state, &over, &root, &text);
                        } else {
                            schedule(&state, over, root, text, DELAY);
                        }
                    }
                    // Over something else, also when the trigger was taken out of the page
                    (None, Some(current)) => {
                        if !target.is_some_and(|t| current.contains(Some(&t))) {
                            hide(&state, &root);
                        }
                    }
                    (None, None) => {}
                }
            })
        };
        let leave = {
            let state = state.clone();
            window_event_listener(leptos::ev::pointerout, move |ev: PointerEvent| {
                if ev.pointer_type() == "touch" {
                    return;
                }
                let Some((root, _)) = tip() else { return };
                let to = ev
                    .related_target()
                    .and_then(|t| t.dyn_into::<web_sys::Node>().ok());
                let current = state.borrow().trigger.clone();
                if current.is_some_and(|c| !to.is_some_and(|to| c.contains(Some(&to)))) {
                    hide(&state, &root);
                }
            })
        };
        // Clicks close it, like the browser's. A tap on something that only has a tooltip (a
        // stamp, a day of the calendar) shows it, buttons and links (also around or inside the
        // element with the tooltip) just do their thing.
        let press = {
            let state = state.clone();
            window_event_listener(leptos::ev::pointerdown, move |ev: PointerEvent| {
                let Some((root, text)) = tip() else { return };
                let tapped = (ev.pointer_type() == "touch")
                    .then(|| ev.target().and_then(|t| t.dyn_into::<Element>().ok()))
                    .flatten()
                    .filter(|t| !is_interactive(t))
                    .as_ref()
                    .and_then(trigger_of);
                let again = tapped.is_some() && tapped == state.borrow().trigger;
                hide(&state, &root);
                if let Some(el) = tapped.filter(|_| !again) {
                    show(&state, &el, &root, &text);
                    let shared = state.clone();
                    let timer =
                        set_timeout_with_handle(move || hide(&shared, &root), TOUCH_MS).ok();
                    state.borrow_mut().timer = timer;
                }
            })
        };
        // The keyboard: shown on focus by Tab (not after a click), Escape closes it
        let focus = {
            let state = state.clone();
            window_event_listener(leptos::ev::focusin, move |ev: FocusEvent| {
                let Some((root, text)) = tip() else { return };
                let Some(el) = ev.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                    return;
                };
                if el.matches(":focus-visible").unwrap_or(false)
                    && let Some(trigger) = trigger_of(&el)
                {
                    show(&state, &trigger, &root, &text);
                }
            })
        };
        let blur = {
            let state = state.clone();
            window_event_listener(leptos::ev::focusout, move |ev: FocusEvent| {
                let Some((root, _)) = tip() else { return };
                let from = ev.target().and_then(|t| t.dyn_into::<Element>().ok());
                if from
                    .as_ref()
                    .and_then(trigger_of)
                    .is_some_and(|el| state.borrow().trigger == Some(el))
                {
                    hide(&state, &root);
                }
            })
        };
        let escape = {
            let state = state.clone();
            window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
                if ev.key() == "Escape"
                    && let Some((root, _)) = tip()
                {
                    hide(&state, &root);
                }
            })
        };
        let resize = {
            let state = state.clone();
            window_event_listener(leptos::ev::resize, move |_| {
                if let Some((root, _)) = tip() {
                    hide(&state, &root);
                }
            })
        };
        // Scrolling (also into view, on focus) moves the trigger: the tooltip follows it, until it
        // leaves the screen. Scroll doesn't bubble, so in the capture phase.
        let scroll = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
            let Some((root, _)) = tip() else { return };
            let trigger = state
                .borrow()
                .trigger
                .clone()
                .filter(|_| state.borrow().open);
            if let Some(trigger) = trigger {
                let rect = trigger.get_bounding_client_rect();
                let height = window()
                    .inner_height()
                    .ok()
                    .and_then(|h| h.as_f64())
                    .unwrap_or(0.0);
                if rect.bottom() < 0.0 || rect.top() > height {
                    hide(&state, &root);
                } else {
                    place(&trigger, &root);
                }
            }
        });
        let _ = window().add_event_listener_with_callback_and_bool(
            "scroll",
            scroll.as_ref().unchecked_ref(),
            true,
        );
        // The tooltip lives as long as the page
        scroll.forget();

        on_cleanup(move || {
            for handle in [hover, leave, press, focus, blur, escape, resize] {
                handle.remove();
            }
        });
    });

    view! {
        <div
            node_ref=root
            id=ID
            role="tooltip"
            hidden=true
            data-side="top"
            class=CLASS
        >
            <span node_ref=text></span>
            // A stepped pixel triangle, pointing at the trigger
            <svg
                viewBox="0 0 6 3"
                class="absolute left-[var(--arrow-x)] h-[6px] w-[12px] -translate-x-1/2 fill-foreground group-data-[side=bottom]:bottom-[calc(100%+3px)] group-data-[side=bottom]:rotate-180 group-data-[side=top]:top-[calc(100%+3px)]"
                shape-rendering="crispEdges"
                aria-hidden="true"
            >
                <path d="M0 0h6v1H5v1H4v1H2V2H1V1H0z" />
            </svg>
        </div>
    }
}

/// The element with the tooltip `el` is part of, if any. One marked `data-tooltip-clipped` (a
/// name that may be cut off with an ellipsis) only has it while its text doesn't fit.
fn trigger_of(el: &Element) -> Option<Element> {
    el.closest(SELECTOR).ok().flatten().filter(|t| {
        t.get_attribute("data-tooltip")
            .is_some_and(|s| !s.is_empty())
            && (!t.has_attribute("data-tooltip-clipped") || t.scroll_width() > t.client_width())
    })
}

/// Whether `el` is (in) something a tap does something with.
fn is_interactive(el: &Element) -> bool {
    el.closest(
        "a[href], button, input, select, textarea, label, [role=button], [role=radio], [role=tab]",
    )
    .ok()
    .flatten()
    .is_some()
}

fn schedule(state: &Shared, el: Element, root: HtmlElement, text: HtmlElement, delay: Duration) {
    cancel(state);
    state.borrow_mut().trigger = Some(el.clone());
    let shared = state.clone();
    let timer = set_timeout_with_handle(
        move || {
            if shared.borrow().trigger.as_ref() == Some(&el) {
                show(&shared, &el, &root, &text);
            }
        },
        delay,
    )
    .ok();
    state.borrow_mut().timer = timer;
}

fn cancel(state: &Shared) {
    if let Some(timer) = state.borrow_mut().timer.take() {
        timer.clear();
    }
}

fn show(state: &Shared, el: &Element, root: &HtmlElement, text: &HtmlElement) {
    cancel(state);
    let Some(label) = el.get_attribute("data-tooltip").filter(|l| !l.is_empty()) else {
        return;
    };
    if !el.is_connected() {
        return;
    }
    let previous = state.borrow().trigger.clone();
    if let Some(previous) = previous.filter(|p| p != el) {
        undescribe(state, &previous);
    }

    text.set_text_content(Some(&label));
    // Coming out of `hidden` plays the `animate-in` animation, when it's already open (moving
    // along a row) it just follows
    root.set_hidden(false);
    place(el, root);

    // Screen readers: the tooltip describes the trigger, unless it already says the same (a cut
    // off name is read whole anyway)
    let described = el.get_attribute("aria-label").as_deref() != Some(label.as_str())
        && !el.has_attribute("data-tooltip-clipped")
        && el.get_attribute("aria-describedby").is_none();
    if described {
        let _ = el.set_attribute("aria-describedby", ID);
    }
    let mut s = state.borrow_mut();
    s.trigger = Some(el.clone());
    s.open = true;
    s.described = described;
}

/// Above the trigger, below when there's no room; centred on it but kept on the screen.
fn place(el: &Element, root: &HtmlElement) {
    let style = root.style();
    let _ = style.set_property("left", "0px");
    let _ = style.set_property("top", "0px");
    let anchor = el.get_bounding_client_rect();
    // The layout size, the bounding box is still scaled down by the animation
    let (width, height) = (
        f64::from(root.offset_width()),
        f64::from(root.offset_height()),
    );
    let viewport = document()
        .document_element()
        .map_or(0.0, |d| f64::from(d.client_width()));
    let Placement {
        side,
        left,
        top,
        arrow,
    } = Placement::new(
        Anchor {
            top: anchor.top(),
            bottom: anchor.bottom(),
            left: anchor.left(),
            width: anchor.width(),
        },
        (width, height),
        viewport,
    );

    let _ = style.set_property("left", &format!("{left:.0}px"));
    let _ = style.set_property("top", &format!("{top:.0}px"));
    let _ = style.set_property("--arrow-x", &format!("{arrow:.0}px"));
    let _ = root.set_attribute("data-side", side);
}

/// Where the trigger is on the screen, in CSS pixels.
#[derive(Clone, Copy, Debug)]
struct Anchor {
    top: f64,
    bottom: f64,
    left: f64,
    width: f64,
}

/// Where the tooltip goes: its side of the trigger, its top left corner and the arrow's offset
/// from its left edge.
#[derive(Debug, PartialEq)]
struct Placement {
    side: &'static str,
    left: f64,
    top: f64,
    arrow: f64,
}

impl Placement {
    /// A tooltip of `size` (width, height) for `anchor`, on a screen `viewport` wide.
    fn new(anchor: Anchor, (width, height): (f64, f64), viewport: f64) -> Self {
        let outset = OUTLINE + ARROW + GAP;
        let above = anchor.top - outset - height;
        let (side, top) = if above >= EDGE + OUTLINE {
            ("top", above)
        } else {
            ("bottom", anchor.bottom + outset)
        };
        let centre = anchor.left + anchor.width / 2.0;
        let left = (centre - width / 2.0).clamp(
            EDGE + OUTLINE,
            (viewport - EDGE - OUTLINE - width).max(EDGE + OUTLINE),
        );
        let arrow = (centre - left).clamp(
            ARROW + OUTLINE,
            (width - ARROW - OUTLINE).max(ARROW + OUTLINE),
        );
        Self {
            side,
            left,
            top,
            arrow,
        }
    }
}

fn hide(state: &Shared, root: &HtmlElement) {
    cancel(state);
    let trigger = state.borrow_mut().trigger.take();
    if let Some(trigger) = trigger {
        undescribe(state, &trigger);
    }
    let mut s = state.borrow_mut();
    if s.open {
        s.open = false;
        s.closed_at = js_sys::Date::now();
        root.set_hidden(true);
    }
}

fn undescribe(state: &Shared, el: &Element) {
    if std::mem::take(&mut state.borrow_mut().described) {
        let _ = el.remove_attribute("aria-describedby");
    }
}

#[cfg(test)]
mod tests {
    use super::{ARROW, Anchor, CLASS, EDGE, GAP, OUTLINE, Placement};

    /// It appears where it belongs, it doesn't glide there from the top left.
    #[test]
    fn position_is_not_animated() {
        for class in CLASS.split_whitespace() {
            let utility = class.rsplit(':').next().unwrap_or(class);
            assert!(
                !utility.starts_with("duration-")
                    && !utility.starts_with("transition")
                    && !utility.starts_with("ease-"),
                "{class} would animate the tooltip's position",
            );
        }
    }

    /// A 40 × 20 button with its top left corner at (`x`, `y`).
    fn button(x: f64, y: f64) -> Anchor {
        Anchor {
            top: y,
            bottom: y + 20.0,
            left: x,
            width: 40.0,
        }
    }

    /// A tooltip 100 × 30 on a screen 400 wide.
    fn place(anchor: Anchor) -> Placement {
        Placement::new(anchor, (100.0, 30.0), 400.0)
    }

    /// Room enough above the trigger: centred on it, its arrow in the middle.
    #[test]
    fn a_tooltip_sits_centred_above_its_trigger() {
        let p = place(button(180.0, 200.0));
        assert_eq!(p.side, "top");
        assert!((p.top - (200.0 - OUTLINE - ARROW - GAP - 30.0)).abs() < 1e-9);
        assert!((p.left - 150.0).abs() < 1e-9);
        assert!((p.arrow - 50.0).abs() < 1e-9);
    }

    #[test]
    fn without_room_above_it_goes_below() {
        let p = place(button(180.0, 30.0));
        assert_eq!(p.side, "bottom");
        assert!((p.top - (50.0 + OUTLINE + ARROW + GAP)).abs() < 1e-9);
    }

    /// Next to the edge of the screen it moves inwards, and its arrow still points at the
    /// trigger, as far as the arrow can go.
    #[test]
    fn at_the_edges_of_the_screen_it_stays_on_it() {
        let p = place(button(0.0, 200.0));
        assert!((p.left - (EDGE + OUTLINE)).abs() < 1e-9);
        assert!((p.arrow - (20.0 - EDGE - OUTLINE)).abs() < 1e-9);

        let p = place(button(360.0, 200.0));
        assert!((p.left + 100.0 - (400.0 - EDGE - OUTLINE)).abs() < 1e-9);
        assert!((p.arrow - (380.0 - p.left)).abs() < 1e-9);

        // A trigger off to the side: the arrow stops at the corner
        let p = place(button(-200.0, 200.0));
        assert!((p.arrow - (ARROW + OUTLINE)).abs() < 1e-9);
    }

    /// On a screen narrower than the tooltip its left edge stays visible.
    #[test]
    fn on_a_narrow_screen_the_start_stays_visible() {
        let p = Placement::new(button(10.0, 200.0), (300.0, 30.0), 200.0);
        assert!((p.left - (EDGE + OUTLINE)).abs() < 1e-9);
    }
}
