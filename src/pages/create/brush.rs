//! The number palette: what a tap on a cell of the workshop's grid places.

use leptos::prelude::*;

use crate::components::ui::toggle_group::{radio_group_keydown, radio_tab_stop};
use crate::i18n::{t_string, use_i18n};

/// Count up, the numbers 1 to 8, erase.
const BRUSHES: [Option<u8>; 10] = [
    None,
    Some(1),
    Some(2),
    Some(3),
    Some(4),
    Some(5),
    Some(6),
    Some(7),
    Some(8),
    Some(0),
];

/// A radio group: one Tab stop, the arrow keys pick the brush.
#[component]
pub(super) fn BrushPalette(brush: RwSignal<Option<u8>>) -> impl IntoView {
    let i18n = use_i18n();
    let tab_stop = Memo::new(move |_| {
        let checked = BRUSHES.iter().position(|&b| b == brush.get());
        radio_tab_stop(checked, &[true; BRUSHES.len()])
    });
    view! {
        <div
            class="grid grid-cols-5 gap-1.5 sm:grid-cols-10 sm:gap-1"
            role="radiogroup"
            aria-label=move || t_string!(i18n, create.brush_label)
            on:keydown=move |ev| radio_group_keydown(&ev)
        >
            {BRUSHES
                .into_iter()
                .enumerate()
                .map(|(i, b)| {
                    let label = match b {
                        None => "+1".to_string(),
                        Some(0) => "消".to_string(),
                        Some(n) => n.to_string(),
                    };
                    let title = move || match b {
                        None => t_string!(i18n, create.brush.count_up).to_string(),
                        Some(0) => t_string!(i18n, create.brush.erase).to_string(),
                        Some(n) => t_string!(i18n, create.brush.place, number = n),
                    };
                    view! {
                        <button
                            type="button"
                            role="radio"
                            data-tooltip=title
                            aria-label=title
                            aria-checked=move || (brush.get() == b).to_string()
                            tabindex=move || if tab_stop.get() == Some(i) { 0 } else { -1 }
                            class=move || {
                                format!(
                                    "grid h-10 place-items-center rounded-md border font-pixel text-base transition-colors {}",
                                    if brush.get() == b {
                                        "border-primary bg-primary text-primary-foreground shadow-sm"
                                    } else {
                                        "bg-card hover:border-primary/50"
                                    },
                                )
                            }
                            on:click=move |_| brush.set(b)
                        >
                            {label}
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    /// One Tab stop, the chosen brush: every brush used to be one, ten stops in a row.
    #[tokio::test]
    async fn the_chosen_brush_is_the_only_tab_stop() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            for (i, b) in BRUSHES.into_iter().enumerate() {
                let brush = RwSignal::new(b);
                let html = view! { <BrushPalette brush /> }.to_html();
                assert!(html.contains("role=\"radiogroup\""));
                let radios: Vec<&str> = html
                    .split("<button ")
                    .skip(1)
                    .map(|b| b.split('>').next().unwrap())
                    .collect();
                assert_eq!(radios.len(), BRUSHES.len());
                for (j, radio) in radios.into_iter().enumerate() {
                    let (checked, tabindex) = if i == j { (true, 0) } else { (false, -1) };
                    assert!(
                        radio.contains(&format!("aria-checked=\"{checked}\"")),
                        "{radio}"
                    );
                    assert!(
                        radio.contains(&format!("tabindex=\"{tabindex}\"")),
                        "{radio}"
                    );
                }
            }
        });
    }
}
