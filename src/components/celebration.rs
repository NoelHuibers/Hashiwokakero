use leptos::prelude::*;

use super::sprites::{PETAL, PixelSprite};

/// 桜吹雪 a shower of pixel cherry blossom petals.
#[component]
pub fn Petals(#[prop(default = 42)] count: usize) -> impl IntoView {
    // Deterministic pseudo random spread, so server and client render the same
    let mut seed = 0x9e37_79b9u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f64::from(seed % 10_000) / 10_000.0
    };
    (0..count)
        .map(|_| {
            let left = next() * 100.0;
            let delay = next() * 2.5;
            let fall = 4.5 + next() * 4.0;
            let sway = 1.2 + next() * 1.6;
            let size = 10.0 + next() * 10.0;
            let style = format!(
                "left: {left:.1}vw; width: {size:.0}px; height: {size:.0}px; animation-duration: {fall:.2}s, {sway:.2}s; animation-delay: {delay:.2}s, 0s;"
            );
            view! {
                <div class="petal" style=style aria-hidden="true">
                    <PixelSprite art=&PETAL class="size-full" />
                </div>
            }
        })
        .collect_view()
}

/// The stamp shown on top of the board when a puzzle is finished.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn Stamp(#[prop(into)] kanji: String, #[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <span
            class=format!(
                "stamp-in inline-grid place-items-center rounded-xl border-4 border-primary px-3 py-1 font-pixel text-primary {class}",
            )
            style="box-shadow: inset 0 0 0 2px var(--card), inset 0 0 0 4px var(--primary);"
        >
            {kanji}
        </span>
    }
}
