use leptos::prelude::*;
use leptos_ui::clx;

// From rust-ui (select_native.rs), with the chevron inlined instead of the icons crate and the
// value/on_change props generalised to signals and plain closures.

#[component]
pub fn SelectNative(
    children: Children,
    #[prop(into)] id: String,
    #[prop(optional, into)] class: String,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] on_change: Callback<String>,
    #[prop(into, optional)] disabled: Signal<bool>,
) -> impl IntoView {
    clx! {
        SelectNativeRoot, select,
        "peer inline-flex w-full cursor-pointer appearance-none items-center rounded-lg h-9 pe-8 ps-3 border border-input bg-background shadow-sm shadow-black/5 transition-shadow text-sm text-foreground focus-visible:border-ring focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/20 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 has-[option[disabled]:checked]:text-muted-foreground",
    }

    view! {
        <div class="relative">
            <SelectNativeRoot
                attr:id=id
                class=class
                prop:value=move || value.get()
                attr:disabled=move || disabled.get()
                on:change=move |ev| on_change.run(event_target_value(&ev))
            >
                {children()}
            </SelectNativeRoot>

            <span class="flex absolute inset-y-0 justify-center items-center w-9 h-full pointer-events-none end-0 text-muted-foreground/80 peer-disabled:opacity-50">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="size-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="m6 9 6 6 6-6" />
                </svg>
            </span>
        </div>
    }
}
