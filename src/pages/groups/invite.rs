//! The invite link of a group: shown to copy, and shared like a daily result.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::share::ShareBar;
use crate::components::ui::button::{Button, ButtonVariant};
use crate::i18n::{t, td_string, use_i18n};
use crate::state::{ToastKind, toast};

/// The link to join a group with `code`, on this site.
pub fn invite_url(code: &str) -> String {
    let origin = window().location().origin().unwrap_or_default();
    format!("{origin}/g/{code}")
}

/// Copies the link, and says so.
fn copy_link(url: &str) {
    let written = window().navigator().clipboard().write_text(url);
    spawn_local(async move {
        if wasm_bindgen_futures::JsFuture::from(written).await.is_ok() {
            toast(ToastKind::Success, |l| {
                td_string!(l, groups.invite.copied).to_string()
            });
        } else {
            toast(ToastKind::Error, |l| {
                td_string!(l, common.toast.copy_failed).to_string()
            });
        }
    });
}

/// The invite link of a group named `name`, with a button to copy it and the share bar.
#[component]
pub fn InviteLink(
    #[prop(into)] name: Signal<String>,
    #[prop(into)] code: Signal<String>,
) -> impl IntoView {
    let i18n = use_i18n();
    // The page's address is only known in the browser
    let shown = RwSignal::new(String::new());
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| shown.set(invite_url(&code.get())));
    let text = move || {
        format!(
            "{} 仲間\n{}",
            td_string!(
                i18n.get_locale_untracked(),
                groups.invite.share_text,
                name = name.get_untracked()
            ),
            invite_url(&code.get_untracked()),
        )
    };
    view! {
        <div class="flex flex-col gap-2">
            <span class="text-sm font-medium">{t!(i18n, groups.invite.title)}</span>
            <div class="flex gap-2">
            <input
                class="h-10 w-full min-w-0 rounded-md border border-dashed border-primary/60 bg-muted/50 px-3 font-pixel text-sm tracking-wide text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                readonly=true
                aria-label=move || td_string!(i18n.get_locale(), groups.invite.title)
                prop:value=move || shown.get()
                data-testid="invite-link"
                node_ref=input
                // All of it at once, to copy by hand
                on:focus=move |_| {
                    if let Some(input) = input.get_untracked() {
                        input.select();
                    }
                }
            />
            <Button
                variant=ButtonVariant::Default
                class="h-10 shrink-0"
                on:click=move |_| copy_link(&invite_url(&code.get_untracked()))
            >
                {t!(i18n, groups.invite.copy)}
            </Button>
            </div>
            <ShareBar
                text=text
                outline=true
                label=Signal::derive(move || td_string!(i18n.get_locale(), groups.invite.share).to_string())
            />
        </div>
    }
}
