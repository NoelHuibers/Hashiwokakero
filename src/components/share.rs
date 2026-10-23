//! Sharing a result: the phone's share sheet, `WhatsApp`, X, or the clipboard. In the app the
//! share sheet is Android's (`api::native`): its `WebView` has no `navigator.share`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};

use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::state::{ToastKind, toast};

fn copy(text: &str) {
    let written = window().navigator().clipboard().write_text(text);
    // Rejected where the page may not write to the clipboard; it said "copied" anyway
    spawn_local(async move {
        if wasm_bindgen_futures::JsFuture::from(written).await.is_ok() {
            toast(ToastKind::Success, |l| {
                td_string!(l, common.toast.copied).to_string()
            });
        } else {
            toast(ToastKind::Error, |l| {
                td_string!(l, common.toast.copy_failed).to_string()
            });
        }
    });
}

/// Opens `url` in a new tab, in the app in the phone's browser (or the app that takes it).
fn open(url: &str) {
    #[cfg(feature = "csr")]
    if crate::api::native::open_url(url) {
        return;
    }
    let _ = window().open_with_url_and_target(url, "_blank");
}

fn encode(text: &str) -> String {
    js_sys::encode_uri_component(text).into()
}

/// `navigator.share`, if the browser has it (phones and some desktops).
fn native_share() -> Option<js_sys::Function> {
    let navigator: JsValue = window().navigator().into();
    js_sys::Reflect::get(&navigator, &"share".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
}

/// Opens the system share sheet, or copies the text where there is none.
fn share(text: &str) {
    #[cfg(feature = "csr")]
    if crate::api::native::share(text, None) {
        return;
    }
    let Some(share) = native_share() else {
        copy(text);
        return;
    };
    let data = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&data, &"text".into(), &text.into());
    let navigator: JsValue = window().navigator().into();
    // Rejects when the user closes the sheet, nothing to do then
    let _ = share.call1(&navigator, &data);
}

const WHATSAPP: &str = "M17.5 14.4c-.3-.1-1.8-.9-2-1-.3-.1-.5-.1-.7.1-.2.3-.8 1-.9 1.2-.2.2-.3.2-.6.1-.3-.1-1.3-.5-2.4-1.5-.9-.8-1.5-1.8-1.7-2.1-.2-.3 0-.5.1-.6l.4-.5c.2-.2.2-.3.3-.5.1-.2 0-.4 0-.5l-.9-2.2c-.2-.6-.5-.5-.7-.5h-.6c-.2 0-.5.1-.8.4-.3.3-1 1-1 2.5s1.1 2.9 1.2 3.1c.1.2 2.1 3.2 5.1 4.5.7.3 1.3.5 1.7.6.7.2 1.4.2 1.9.1.6-.1 1.8-.7 2-1.4.2-.7.2-1.3.2-1.4-.1-.1-.3-.2-.6-.3zM12 21.8c-1.8 0-3.5-.5-5-1.4l-.4-.2-3.7 1 1-3.6-.2-.4c-1-1.6-1.5-3.4-1.5-5.2 0-5.4 4.4-9.8 9.8-9.8 2.6 0 5.1 1 6.9 2.9 1.9 1.9 2.9 4.3 2.9 6.9 0 5.4-4.4 9.8-9.8 9.8zm8.4-18.2C18.1 1.3 15.2.1 12 .1 5.5.1.2 5.4.2 11.9c0 2.1.5 4.1 1.6 5.9L.1 24l6.3-1.7c1.7.9 3.7 1.4 5.6 1.4 6.5 0 11.8-5.3 11.8-11.8 0-3.2-1.2-6.1-3.4-8.3z";
const X_LOGO: &str = "M18.2 2.3h3.4l-7.3 8.4 8.6 11.4h-6.7l-5.3-6.9-6 6.9H1.5l7.8-9L1.1 2.3h6.9l4.8 6.3 5.4-6.3zm-1.2 17.8h1.9L7.1 4.2h-2l11.9 15.9z";

/// The share button and, below it, WhatsApp, X and copy. `text` is built when clicked.
#[component]
pub fn ShareBar(
    text: impl Fn() -> String + Copy + Send + Sync + 'static,
    #[prop(optional)] outline: bool,
    /// The big button's label, "Share result" if none.
    #[prop(optional, into)]
    label: Option<Signal<String>>,
) -> impl IntoView {
    let i18n = use_i18n();
    let small = move |label: AnyView,
                      path: Option<&'static str>,
                      on: Box<dyn Fn() + Send + Sync>| {
        view! {
            <Button
                variant=ButtonVariant::Ghost
                size=ButtonSize::Sm
                class="flex-1 gap-1.5 text-muted-foreground"
                on:click=move |_| on()
            >
                {path
                    .map(|d| {
                        view! {
                            <svg viewBox="0 0 24 24" class="size-4" fill="currentColor" aria-hidden="true">
                                <path d=d />
                            </svg>
                        }
                    })}
                {label}
            </Button>
        }
    };
    view! {
        <div class="flex w-full flex-col gap-1">
            <Button
                variant=if outline { ButtonVariant::Outline } else { ButtonVariant::Default }
                class="w-full"
                on:click=move |_| share(&text())
            >
                {move || match label {
                    Some(label) => label.get(),
                    None => t_string!(i18n, common.game.share).to_string(),
                }}
            </Button>
            <div class="flex w-full gap-1">
                {small(
                    "WhatsApp".into_any(),
                    Some(WHATSAPP),
                    Box::new(move || open(&format!("https://wa.me/?text={}", encode(&text())))),
                )}
                {small(
                    view! { <span class="sr-only">"X"</span> }.into_any(),
                    Some(X_LOGO),
                    Box::new(move || open(&format!("https://x.com/intent/post?text={}", encode(&text())))),
                )}
                {small(t!(i18n, common.copy).into_any(), None, Box::new(move || copy(&text())))}
            </div>
        </div>
    }
}
