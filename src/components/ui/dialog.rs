//! A dialog over the page, and the question asked before something that can't be undone.
//!
//! In place of the browser's `confirm()`, which looks different in every browser (on phones a
//! bare system box with the site's address over it) and can't be styled.

use leptos::portal::Portal;
use leptos::prelude::*;

use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::Card;
use crate::i18n::{t, use_i18n};

/// A dialog over the page: Escape and a click beside it close it (`open`), it takes the focus.
/// In a portal at the end of the body: the page's cards would cover it.
#[component]
pub fn Dialog(
    open: RwSignal<bool>,
    labelled_by: &'static str,
    children: ChildrenFn,
) -> impl IntoView {
    let dialog = NodeRef::<leptos::html::Div>::new();
    // The portal wants children it can call more than once
    let children = StoredValue::new(children);
    Effect::new(move |_| {
        let Some(dialog) = dialog.get() else { return };
        let _ = dialog.focus();
        let escape = window_event_listener(leptos::ev::keydown, move |ev| {
            if ev.key() == "Escape" {
                open.set(false);
            }
        });
        on_cleanup(move || escape.remove());
    });
    view! {
        <Portal>
        <div
            class="fixed inset-0 z-50 grid place-items-center overflow-y-auto bg-black/40 p-4"
            on:click=move |ev| {
                // Beside the dialog, not in it
                if ev.target() == ev.current_target() {
                    open.set(false);
                }
            }
        >
            <div
                node_ref=dialog
                role="dialog"
                aria-modal="true"
                aria-labelledby=labelled_by
                tabindex="-1"
                class="w-full max-w-md rounded-xl outline-none"
            >
                <Card class="gap-4 px-5 py-5 shadow-2xl animate-in zoom-in-95 fade-in">{children.with_value(|c| c())}</Card>
            </div>
        </div>
        </Portal>
    }
}

/// Asks `question` while `open`: "Cancel" closes it, `yes` (the action's own name) closes it
/// and runs `on_yes`. `destructive` paints that button red, for what's lost for good.
#[component]
pub fn ConfirmDialog(
    open: RwSignal<bool>,
    #[prop(into)] question: Signal<String>,
    #[prop(into)] yes: Signal<String>,
    #[prop(into)] on_yes: Callback<()>,
    #[prop(optional)] destructive: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    let variant = if destructive {
        ButtonVariant::Destructive
    } else {
        ButtonVariant::Default
    };
    view! {
        <Show when=move || open.get()>
            <Dialog open labelled_by="confirm-question">
                <p id="confirm-question" class="font-medium leading-relaxed">{move || question.get()}</p>
                <div class="flex flex-wrap justify-end gap-2">
                    <Button variant=ButtonVariant::Ghost on:click=move |_| open.set(false) attr:data-testid="confirm-no">
                        {t!(i18n, common.cancel)}
                    </Button>
                    <Button
                        variant
                        on:click=move |_| {
                            open.set(false);
                            on_yes.run(());
                        }
                        attr:data-testid="confirm-yes"
                    >
                        {move || yes.get()}
                    </Button>
                </div>
            </Dialog>
        </Show>
    }
}
