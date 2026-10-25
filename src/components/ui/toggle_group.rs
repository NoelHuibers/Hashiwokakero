use leptos::prelude::*;
use leptos_ui::clx;
use tw_merge::tw_merge;
use wasm_bindgen::JsCast;

// From rust-ui (toggle_group.rs), changed into a radio group: one Tab stop, arrow keys pick.

/* ========================================================== */
/*                       Enums                                */
/* ========================================================== */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToggleGroupVariant {
    #[default]
    Default,
    Outline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToggleGroupOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/* ========================================================== */
/*                       Context                              */
/* ========================================================== */

/// Each item's pressed and disabled state, in order, so the group knows its Tab stop.
type Radios = Vec<(Signal<bool>, Signal<bool>)>;

#[derive(Clone, Copy)]
struct ToggleGroupCtx {
    variant: ToggleGroupVariant,
    orientation: ToggleGroupOrientation,
    spacing: i32,
    radios: StoredValue<Radios>,
    tab_stop: RwSignal<Option<usize>>,
}

/* ========================================================== */
/*                       Keyboard                             */
/* ========================================================== */

/// The radio that takes the group's one Tab stop: the checked one, or the first enabled one
/// while none is (or the checked one is disabled).
pub fn radio_tab_stop(checked: Option<usize>, enabled: &[bool]) -> Option<usize> {
    checked
        .filter(|&i| enabled.get(i) == Some(&true))
        .or_else(|| enabled.iter().position(|&e| e))
}

/// Where a key moves the focus, and the choice, in a radio group. `None` for other keys.
///
/// The arrows go to the next or previous enabled radio, wrapping around, Home and End to the
/// first and last one. In a right-to-left page the next one is on the left.
pub fn radio_key_target(key: &str, rtl: bool, current: usize, enabled: &[bool]) -> Option<usize> {
    let count = enabled.len();
    let forward = match key {
        "ArrowDown" => true,
        "ArrowUp" => false,
        "ArrowRight" | "ArrowLeft" => (key == "ArrowRight") != rtl,
        "Home" => return enabled.iter().position(|&e| e),
        "End" => return enabled.iter().rposition(|&e| e),
        _ => return None,
    };
    let current = current.min(count.checked_sub(1)?);
    (1..=count)
        .map(|i| {
            if forward {
                (current + i) % count
            } else {
                (current + count - i) % count
            }
        })
        .find(|&i| enabled[i])
}

/// Arrow keys, Home and End on a radio of the group: focus and check the radio they go to.
/// Space and Enter check the focused one already, the radios are buttons.
pub fn radio_group_keydown(ev: &web_sys::KeyboardEvent) {
    if ev.ctrl_key() || ev.meta_key() || ev.alt_key() {
        return;
    }
    let Some(group) = ev
        .current_target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return;
    };
    let children = group.children();
    let radios: Vec<web_sys::HtmlElement> = (0..children.length())
        .filter_map(|i| children.item(i))
        .filter(|el| el.get_attribute("role").as_deref() == Some("radio"))
        .filter_map(|el| el.dyn_into().ok())
        .collect();
    let target = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
    let Some(current) = radios
        .iter()
        .position(|r| target.as_ref() == Some(r.unchecked_ref()))
    else {
        return;
    };
    let enabled: Vec<bool> = radios
        .iter()
        .map(|r| {
            !r.has_attribute("disabled")
                && r.get_attribute("aria-disabled").as_deref() != Some("true")
        })
        .collect();
    // In the reading direction: right is back in Arabic
    let rtl = group.closest("[dir=rtl]").ok().flatten().is_some();
    let Some(next) = radio_key_target(&ev.key(), rtl, current, &enabled) else {
        return;
    };
    ev.prevent_default();
    ev.stop_propagation();
    let radio = &radios[next];
    let _ = radio.focus();
    if radio.get_attribute("aria-checked").as_deref() != Some("true") {
        radio.click();
    }
}

/* ========================================================== */
/*                     Components (clx!)                      */
/* ========================================================== */

mod components {
    use super::{
        Children, ClassAttribute, CustomAttribute, ElementChild, IntoView, clx, component, view,
    };
    clx! {ToggleGroupAction, a, "inline-flex gap-2 justify-center items-center p-0 text-sm font-medium whitespace-nowrap rounded-sm transition-all outline-none disabled:opacity-50 disabled:pointer-events-none [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 aria-invalid:ring-destructive/20 aria-invalid:border-destructive size-6 dark:aria-invalid:ring-destructive/40 dark:hover:bg-accent/50 hover:bg-accent hover:text-accent-foreground focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]"}
}

pub use components::*;

/* ========================================================== */
/*                     ✨ FUNCTIONS ✨                        */
/* ========================================================== */

#[component]
pub fn ToggleGroup(
    children: Children,
    #[prop(optional, into)] class: String,
    /// The group's accessible name.
    #[prop(into)]
    aria_label: Signal<String>,
    #[prop(optional)] variant: ToggleGroupVariant,
    #[prop(optional)] orientation: ToggleGroupOrientation,
    #[prop(optional, default = 1i32)] spacing: i32,
) -> impl IntoView {
    let radios = StoredValue::new(Radios::new());
    let tab_stop = RwSignal::new(None);
    provide_context(ToggleGroupCtx {
        variant,
        orientation,
        spacing,
        radios,
        tab_stop,
    });
    // Once all items are there. Until then (and on the server) the pressed one is the Tab stop.
    Effect::new(move |_| {
        let stop = radios.with_value(|radios| {
            let checked = radios.iter().position(|(pressed, _)| pressed.get());
            let enabled: Vec<bool> = radios.iter().map(|(_, disabled)| !disabled.get()).collect();
            radio_tab_stop(checked, &enabled)
        });
        tab_stop.set(stop);
    });

    let is_vertical = orientation == ToggleGroupOrientation::Vertical;

    let gap_style = if spacing == 0 {
        "gap: 0px".to_string()
    } else {
        format!("gap: {}rem", f64::from(spacing) * 0.25)
    };

    let class = tw_merge!(
        "flex items-center rounded-md group/toggle-group w-fit",
        if is_vertical { "flex-col" } else { "" },
        if variant == ToggleGroupVariant::Outline {
            "shadow-xs"
        } else {
            ""
        },
        class
    );

    view! {
        <div
            class=class
            style=gap_style
            role="radiogroup"
            aria-label=move || aria_label.get()
            on:keydown=move |ev| radio_group_keydown(&ev)
            data-variant=if variant == ToggleGroupVariant::Outline { "Outline" } else { "Default" }
            data-orientation=if is_vertical { "Vertical" } else { "Horizontal" }
            data-spacing=spacing.to_string()
        >
            {children()}
        </div>
    }
}

#[component]
pub fn ToggleGroupItem(
    children: Children,
    #[prop(optional, into)] class: String,
    #[prop(into)] title: Signal<String>,
    #[prop(optional, into)] pressed: Signal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    let ctx = expect_context::<ToggleGroupCtx>();
    let index = ctx
        .radios
        .try_update_value(|radios| {
            radios.push((pressed, disabled));
            radios.len() - 1
        })
        .unwrap_or_default();
    let tab_stop = move || (pressed.get() && !disabled.get()) || ctx.tab_stop.get() == Some(index);

    let is_vertical = ctx.orientation == ToggleGroupOrientation::Vertical;
    let is_grouped = ctx.spacing == 0;
    let is_outline = ctx.variant == ToggleGroupVariant::Outline;

    let rounded = match (is_grouped, is_vertical) {
        (true, true) => "rounded-none first:rounded-t-md last:rounded-b-md",
        (true, false) => "rounded-none first:rounded-s-md last:rounded-e-md",
        (false, _) => "rounded-md",
    };

    let border = if is_outline && is_grouped {
        if is_vertical {
            "border border-t-0 first:border-t"
        } else {
            "border border-s-0 first:border-s"
        }
    } else if is_outline {
        "border"
    } else {
        ""
    };

    let width = if is_vertical { "w-full" } else { "" };

    let merged_class = tw_merge!(
        "inline-flex flex-1 gap-2 justify-center items-center px-2 min-w-0 h-9 text-sm font-medium whitespace-nowrap bg-transparent shadow-none outline-none focus:z-10 focus-visible:z-10 disabled:opacity-50 disabled:pointer-events-none data-[state=on]:bg-accent data-[state=on]:text-accent-foreground [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 [&_svg]:shrink-0 transition-[color,box-shadow] aria-invalid:ring-destructive/20 aria-invalid:border-destructive shrink-0 dark:aria-invalid:ring-destructive/40 hover:bg-muted hover:text-muted-foreground focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
        rounded,
        border,
        width,
        class
    );

    view! {
        <button
            type="button"
            data-name="ToggleGroupItem"
            class=merged_class
            role="radio"
            aria-checked=move || pressed.get().to_string()
            tabindex=move || if tab_stop() { 0 } else { -1 }
            disabled=move || disabled.get()
            data-tooltip=move || title.get()
            data-state=move || if pressed.get() { "on" } else { "off" }
        >
            {children()}
        </button>
    }
}

#[cfg(test)]
mod tests {
    use super::{radio_key_target, radio_tab_stop};

    const ALL: [bool; 4] = [true; 4];

    #[test]
    fn arrows_go_to_the_next_and_previous_radio_and_wrap_around() {
        assert_eq!(radio_key_target("ArrowRight", false, 1, &ALL), Some(2));
        assert_eq!(radio_key_target("ArrowDown", false, 1, &ALL), Some(2));
        assert_eq!(radio_key_target("ArrowLeft", false, 1, &ALL), Some(0));
        assert_eq!(radio_key_target("ArrowUp", false, 1, &ALL), Some(0));
        assert_eq!(radio_key_target("ArrowRight", false, 3, &ALL), Some(0));
        assert_eq!(radio_key_target("ArrowDown", false, 3, &ALL), Some(0));
        assert_eq!(radio_key_target("ArrowLeft", false, 0, &ALL), Some(3));
        assert_eq!(radio_key_target("ArrowUp", false, 0, &ALL), Some(3));
    }

    #[test]
    fn home_and_end_go_to_the_first_and_last_radio() {
        assert_eq!(radio_key_target("Home", false, 2, &ALL), Some(0));
        assert_eq!(radio_key_target("End", false, 1, &ALL), Some(3));
        let ends_disabled = [false, true, true, false];
        assert_eq!(radio_key_target("Home", false, 2, &ends_disabled), Some(1));
        assert_eq!(radio_key_target("End", false, 1, &ends_disabled), Some(2));
    }

    #[test]
    fn disabled_radios_are_skipped() {
        let enabled = [true, false, false, true];
        assert_eq!(radio_key_target("ArrowRight", false, 0, &enabled), Some(3));
        assert_eq!(radio_key_target("ArrowLeft", false, 3, &enabled), Some(0));
        // Over the end too
        assert_eq!(radio_key_target("ArrowRight", false, 3, &enabled), Some(0));
        assert_eq!(radio_key_target("ArrowLeft", false, 0, &enabled), Some(3));
        // The only enabled one stays
        let alone = [false, true, false];
        assert_eq!(radio_key_target("ArrowRight", false, 1, &alone), Some(1));
        assert_eq!(radio_key_target("ArrowUp", false, 1, &alone), Some(1));
    }

    #[test]
    fn right_to_left_swaps_left_and_right_but_not_up_and_down() {
        assert_eq!(radio_key_target("ArrowLeft", true, 1, &ALL), Some(2));
        assert_eq!(radio_key_target("ArrowRight", true, 1, &ALL), Some(0));
        assert_eq!(radio_key_target("ArrowLeft", true, 3, &ALL), Some(0));
        assert_eq!(radio_key_target("ArrowRight", true, 0, &ALL), Some(3));
        assert_eq!(radio_key_target("ArrowDown", true, 1, &ALL), Some(2));
        assert_eq!(radio_key_target("ArrowUp", true, 1, &ALL), Some(0));
        assert_eq!(radio_key_target("Home", true, 2, &ALL), Some(0));
        assert_eq!(radio_key_target("End", true, 1, &ALL), Some(3));
    }

    #[test]
    fn other_keys_and_empty_groups_move_nothing() {
        for key in ["Enter", " ", "Tab", "a", "PageDown"] {
            assert_eq!(radio_key_target(key, false, 1, &ALL), None);
        }
        assert_eq!(radio_key_target("ArrowRight", false, 0, &[]), None);
        assert_eq!(radio_key_target("Home", false, 0, &[false, false]), None);
        assert_eq!(
            radio_key_target("ArrowRight", false, 0, &[false, false]),
            None
        );
    }

    #[test]
    fn the_tab_stop_is_the_checked_radio_or_else_the_first_enabled_one() {
        assert_eq!(radio_tab_stop(Some(2), &ALL), Some(2));
        assert_eq!(radio_tab_stop(None, &ALL), Some(0));
        assert_eq!(radio_tab_stop(None, &[false, false, true]), Some(2));
        // A disabled radio can't take the focus, checked or not
        assert_eq!(radio_tab_stop(Some(0), &[false, true, true]), Some(1));
        assert_eq!(radio_tab_stop(None, &[false, false]), None);
    }
}
