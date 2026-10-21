//! Practice-only solver disclosure. The editor keeps its full solver panel visible.

use leptos::prelude::*;

use crate::i18n::{t, use_i18n};
use crate::state::{load, store};

const OPEN_KEY: &str = "hashi.practice.solver.open";

#[component]
pub(super) fn SolverTools(children: Children) -> impl IntoView {
    let i18n = use_i18n();
    let details = NodeRef::<leptos::html::Details>::new();
    let restored = StoredValue::new(false);

    // Read after hydration: SSR and the first client render both start closed. Only a
    // native toggle after restoration may persist, so mounting cannot erase a saved choice.
    Effect::new(move |_| {
        if let Some(element) = details.get()
            && !restored.get_value()
        {
            if load::<bool>(OPEN_KEY).unwrap_or(false) {
                let _ = element.set_attribute("open", "");
            }
            restored.set_value(true);
        }
    });

    view! {
        <details
            node_ref=details
            class="group min-w-0"
            on:toggle=move |_| {
                if restored.get_value()
                    && let Some(element) = details.get_untracked()
                {
                    store(OPEN_KEY, &element.has_attribute("open"));
                }
            }
        >
            <summary class="flex min-h-11 cursor-pointer list-none items-center justify-between gap-3 rounded-xl border bg-card px-4 py-3 shadow-xs focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden">
                <span class="font-bold">{t!(i18n, common.solver.title)}</span>
                <span class="ms-auto font-pixel text-xs text-primary" aria-hidden="true">"充足可能性"</span>
                <svg class="size-4 shrink-0 group-open:rotate-180" viewBox="0 0 20 20" fill="none" stroke="currentColor" aria-hidden="true">
                    <path d="m5 7.5 5 5 5-5" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
            </summary>
            <div class="mt-3">{children()}</div>
        </details>
    }
}
