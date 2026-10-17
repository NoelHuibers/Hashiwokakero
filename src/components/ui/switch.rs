use leptos::prelude::*;
use leptos_ui::clx;
use tw_merge::tw_merge;

// From rust-ui (switch.rs), changed to be controlled by a signal instead of keeping its own state.

mod components {
    use super::{
        Children, ClassAttribute, CustomAttribute, ElementChild, IntoView, clx, component, view,
    };
    clx! {SwitchLabel, span, "text-sm font-medium"}
}

pub use components::*;

#[component]
pub fn Switch(
    #[prop(optional, into)] id: String,
    checked: RwSignal<bool>,
    #[prop(into, optional, default = Signal::stored("Toggle switch".to_string()))]
    aria_label: Signal<String>,
    #[prop(into, optional)] disabled: Signal<bool>,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    let state = move || {
        if checked.get() {
            "checked"
        } else {
            "unchecked"
        }
    };

    let track_class = tw_merge!(
        "relative inline-flex h-6 w-11 shrink-0 cursor-pointer items-center after:absolute after:-inset-2.5 after:content-[''] rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary data-[state=unchecked]:bg-input",
        class
    );

    view! {
        <button
            data-name="Switch"
            id=id
            type="button"
            role="switch"
            aria-checked=move || checked.get().to_string()
            aria-label=move || aria_label.get()
            data-state=state
            class=track_class
            disabled=move || disabled.get()
            on:click=move |_| checked.update(|c| *c = !*c)
        >
            <span
                data-state=state
                class="block rounded-full ring-0 shadow-lg transition-transform pointer-events-none size-5 bg-background data-[state=checked]:translate-x-5 rtl:data-[state=checked]:-translate-x-5 data-[state=unchecked]:translate-x-0"
            />
        </button>
    }
}
