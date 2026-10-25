//! The settings pop-up for sound, music and (on phones) day or night.

use leptos::prelude::*;

use crate::components::ui::switch::Switch;
use crate::i18n::{t, t_string, use_i18n};
use crate::state::Settings;

/// Closes a pop-up on a click outside of `root` and on Escape (focus goes back to `button`).
pub(crate) fn use_dismiss(
    open: RwSignal<bool>,
    root: NodeRef<leptos::html::Div>,
    button: NodeRef<leptos::html::Button>,
) {
    let outside = window_event_listener(leptos::ev::pointerdown, move |ev| {
        if !open.get_untracked() {
            return;
        }
        let inside = root.get_untracked().is_some_and(|root| {
            ev.target()
                .and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Node>(t).ok())
                .is_some_and(|t| root.contains(Some(&t)))
        });
        if !inside {
            open.set(false);
        }
    });
    let escape = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" && open.get_untracked() {
            open.set(false);
            if let Some(button) = button.get_untracked() {
                let _ = button.focus();
            }
        }
    });
    on_cleanup(move || {
        outside.remove();
        escape.remove();
    });
}

const SPEAKER: &str = "M11 5 6 9H3v6h3l5 4V5z";

/// Lucide's "settings" cog.
const COG: &str = "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z";

/// Settings: sound effects and music, each with a switch; saved in the browser. On computers it
/// is a speaker next to the day/night button, on phones, where the header has no room for that
/// button, a cog that also holds day or night.
#[component]
pub fn SettingsButton() -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    let open = RwSignal::new(false);
    let root = NodeRef::<leptos::html::Div>::new();
    let button = NodeRef::<leptos::html::Button>::new();
    use_dismiss(open, root, button);
    let on = move || settings.sound.get() || settings.music.get();
    view! {
        <div node_ref=root class="relative shrink-0">
            <button
                node_ref=button
                class="grid size-10 place-items-center rounded-lg border bg-card transition-transform hover:-translate-y-0.5"
                class:ring-2=move || open.get()
                class:ring-primary=move || open.get()
                aria-haspopup="true"
                aria-expanded=move || open.get().to_string()
                aria-label=move || t_string!(i18n, common.settings)
                data-tooltip=move || t_string!(i18n, common.settings)
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <svg viewBox="0 0 24 24" class="size-5 sm:hidden" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d=COG />
                    <circle cx="12" cy="12" r="3" />
                </svg>
                <svg viewBox="0 0 24 24" class="hidden size-5 sm:block" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d=SPEAKER fill="currentColor" />
                    {move || {
                        if on() {
                            view! { <path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" /> }.into_any()
                        } else {
                            view! { <path d="m16 9 6 6M22 9l-6 6" /> }.into_any()
                        }
                    }}
                </svg>
            </button>
            <Show when=move || open.get()>
                <div class="absolute end-0 top-full z-50 mt-2 flex w-64 flex-col gap-1 rounded-xl border bg-card p-3 shadow-2xl animate-in fade-in zoom-in-95">
                    <p class="pb-1 font-pixel text-xs text-muted-foreground">"設定 · " {t!(i18n, common.settings)}</p>
                    <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 text-sm font-medium sm:hidden">
                        <span>{t!(i18n, common.theme.day)} " / " {t!(i18n, common.theme.night)}</span>
                        <Switch
                            checked=settings.dark
                            thumb=Signal::derive(move || if settings.dark.get() { "夜" } else { "昼" })
                            aria_label=Signal::derive(move || t_string!(i18n, common.theme.toggle).to_string())
                        />
                    </label>
                    <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 text-sm font-medium">
                        {t!(i18n, common.audio.sound)}
                        <Switch
                            checked=settings.sound
                            aria_label=Signal::derive(move || t_string!(i18n, common.audio.sound).to_string())
                        />
                    </label>
                    <label class="flex min-h-10 cursor-pointer items-center justify-between gap-3 text-sm font-medium">
                        <span>
                            {t!(i18n, common.audio.music)}
                            <span class="block text-xs font-normal text-muted-foreground">{t!(i18n, common.audio.music_hint)}</span>
                        </span>
                        <Switch
                            checked=settings.music
                            aria_label=Signal::derive(move || t_string!(i18n, common.audio.music).to_string())
                        />
                    </label>
                </div>
            </Show>
        </div>
    }
}
