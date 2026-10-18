//! A slider for the grid's width or height.

use common::{MAX_SIZE, MIN_SIZE};
use leptos::prelude::*;

use crate::components::ui::label::Label;

#[component]
pub(super) fn SizeInput(
    id: &'static str,
    #[prop(into)] label: Signal<String>,
    #[prop(into)] value: Signal<u8>,
    on_change: Callback<u8>,
) -> impl IntoView {
    let id = format!("size-{id}");
    view! {
        <div class="flex flex-col gap-2">
            <Label html_for=id.clone()>
                {move || label.get()} <span class="ms-auto font-pixel tabular-nums">{move || value.get()}</span>
            </Label>
            <input
                id=id
                type="range"
                min=MIN_SIZE
                max=MAX_SIZE
                prop:value=move || value.get()
                class="h-9 w-full cursor-pointer accent-[var(--primary)]"
                on:input=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                        on_change.run(v.clamp(MIN_SIZE, MAX_SIZE));
                    }
                }
            />
        </div>
    }
}
