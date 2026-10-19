//! The number palette: what a tap on a cell of the workshop's grid places.

use leptos::prelude::*;

use crate::i18n::{t_string, use_i18n};

#[component]
pub(super) fn BrushPalette(brush: RwSignal<Option<u8>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="grid grid-cols-5 gap-1.5 sm:grid-cols-10 sm:gap-1" role="radiogroup" aria-label=move || t_string!(i18n, create.brush_label)>
            {std::iter::once(None)
                .chain((1..=8).map(Some))
                .chain(std::iter::once(Some(0)))
                .map(|b| {
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
