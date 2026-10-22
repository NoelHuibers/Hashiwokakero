//! Toasts: short messages in a corner of the screen.

use leptos::prelude::*;

use crate::api::update_url;
use crate::i18n::{t, use_i18n};
use crate::state::{ToastAction, ToastKind, Toasts};

const ACTION: &str = "shrink-0 cursor-pointer rounded-md border border-current px-2 py-0.5 font-medium hover:bg-destructive/10";

#[component]
pub fn Toaster() -> impl IntoView {
    let i18n = use_i18n();
    let toasts = expect_context::<Toasts>();
    view! {
        // Keep the live region mounted before messages arrive. Announce additions politely,
        // without moving focus or reading every earlier toast again when one disappears.
        <div
            class="pointer-events-none fixed bottom-[calc(4.5rem+env(safe-area-inset-bottom))] end-4 z-50 flex w-[min(92vw,22rem)] flex-col gap-2 md:bottom-4"
            role="log"
            aria-live="polite"
            aria-relevant="additions text"
            aria-atomic="false"
        >
            <For
                each=move || toasts.0.get()
                key=|t| t.id
                children=move |t| {
                    let (mark, class) = match t.kind {
                        ToastKind::Success => ("◎", "border-success/40 bg-success-light text-success-dark"),
                        ToastKind::Error => ("✕", "border-destructive/40 bg-card text-destructive"),
                        ToastKind::Info => ("◇", "border-border bg-card text-foreground"),
                    };
                    let text = t.text.clone();
                    let action = t.action.map(|action| match action {
                        // A button, not an automatic reload: that would throw away a board in
                        // progress
                        ToastAction::Reload => view! {
                            <button
                                type="button"
                                class=ACTION
                                on:click=|_| {
                                    _ = window().location().reload();
                                }
                            >
                                {t!(i18n, common.toast.reload)}
                            </button>
                        }
                        .into_any(),
                        // The store's page. TODO(step 10 of docs/mobile.md): the WebView opens
                        // no new windows, open it with Tauri's opener plugin.
                        ToastAction::Update => view! {
                            <a class=ACTION href=update_url() target="_blank" rel="noopener">
                                {t!(i18n, common.toast.update)}
                            </a>
                        }
                        .into_any(),
                    });
                    view! {
                        <div class=format!(
                            "pointer-events-auto flex items-start gap-3 rounded-lg border px-4 py-3 text-sm shadow-lg animate-in fade-in slide-in-from-bottom-2 {class}",
                        )>
                            <span class="font-pixel" aria-hidden="true">{mark}</span>
                            <span class="flex-1">{move || text(i18n.get_locale())}</span>
                            {action}
                        </div>
                    }
                }
            />
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::{Locale, td_string};
    use crate::state::Toast;

    #[tokio::test]
    async fn an_empty_live_region_precedes_localized_feedback() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::ja);
            let toasts = Toasts(RwSignal::new(vec![]));
            provide_context(toasts);
            let empty = view! { <Toaster /> }.to_html();
            let host = empty.split('>').next().unwrap();
            assert!(host.contains("role=\"log\""));
            assert!(host.contains("aria-live=\"polite\""));
            assert!(host.contains("aria-relevant=\"additions text\""));
            assert!(host.contains("aria-atomic=\"false\""));
            toasts.0.set(vec![Toast {
                id: 0,
                kind: ToastKind::Error,
                text: std::sync::Arc::new(|l| td_string!(l, common.errors.outdated).to_string()),
                action: Some(ToastAction::Reload),
            }]);
            let filled = view! { <Toaster /> }.to_html();
            assert_eq!(filled.split('>').next().unwrap(), host);
            assert!(filled.contains(td_string!(Locale::ja, common.errors.outdated)));
            assert!(filled.contains(td_string!(Locale::ja, common.toast.reload)));
            assert!(!filled.contains("role=\"alert\""));
            // An outdated app: the store, not a reload
            toasts.0.set(vec![Toast {
                id: 1,
                kind: ToastKind::Error,
                text: std::sync::Arc::new(|l| {
                    td_string!(l, common.errors.app_outdated).to_string()
                }),
                action: Some(ToastAction::Update),
            }]);
            let update = view! { <Toaster /> }.to_html();
            assert!(update.contains(td_string!(Locale::ja, common.errors.app_outdated)));
            assert!(
                update.contains("href=\"https://hashi.rs\" target=\"_blank\" rel=\"noopener\"")
            );
            assert!(update.contains(td_string!(Locale::ja, common.toast.update)));
            assert!(!update.contains(td_string!(Locale::ja, common.toast.reload)));
        });
    }
}
