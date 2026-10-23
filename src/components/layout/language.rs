//! The language picker in the header.

use leptos::prelude::*;
use leptos_i18n::Locale as _;
use leptos_router::hooks::use_location;

use super::settings::use_dismiss;
use crate::i18n::{Locale, t, t_string, use_i18n};
use crate::lang::{localized, native_name, unlocalized};

/// The language picker: a button with the language code that opens a panel with every language
/// in its own script. The entries are links to the same page in that language, so they work
/// before the app is loaded and crawlers can follow them.
#[component]
pub fn LanguagePicker() -> impl IntoView {
    let i18n = use_i18n();
    let location = use_location();
    let open = RwSignal::new(false);
    let root = NodeRef::<leptos::html::Div>::new();
    let button = NodeRef::<leptos::html::Button>::new();

    use_dismiss(open, root, button);

    // The current page in another language, keeping the query
    let href = move |l: Locale| {
        format!(
            "{}{}",
            localized(l, &unlocalized(&location.pathname.get())),
            location
                .search
                .get()
                .is_empty()
                .then(String::new)
                .unwrap_or_else(|| format!("?{}", location.search.get()))
        )
    };
    // Short label: the language code, the two Chinese scripts by their own name
    let code = |l: Locale| match l.as_str() {
        "zh" => "简".to_string(),
        "zh-Hant" => "繁".to_string(),
        other => other.to_string(),
    };

    view! {
        <div node_ref=root class="relative shrink-0">
            <button
                node_ref=button
                class="grid h-10 min-w-10 place-items-center rounded-lg border bg-card px-2 text-xs font-bold uppercase transition-transform hover:-translate-y-0.5"
                class:ring-2=move || open.get()
                class:ring-primary=move || open.get()
                aria-haspopup="true"
                aria-expanded=move || open.get().to_string()
                aria-label=move || t_string!(i18n, common.language)
                data-tooltip=move || t_string!(i18n, common.language)
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="flex items-center gap-1" aria-hidden="true">
                    <svg viewBox="0 0 24 24" class="size-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="10" />
                        <path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                    </svg>
                    <span class="hidden sm:inline">{move || code(i18n.get_locale())}</span>
                </span>
            </button>
            <Show when=move || open.get()>
                // Phones: across the screen below the header, the button isn't at the edge
                <div class="fixed inset-x-3 top-[calc(4rem+var(--safe-top))] z-50 rounded-xl border bg-card p-2 shadow-2xl animate-in fade-in zoom-in-95 sm:absolute sm:inset-x-auto sm:end-0 sm:top-full sm:mt-2 sm:w-[26rem]">
                    <p class="px-2 pb-2 pt-1 font-pixel text-xs text-muted-foreground">
                        "言語 · " {t!(i18n, common.language)}
                    </p>
                    <ul class="grid max-h-[min(70vh,28rem)] grid-cols-2 gap-1 overflow-y-auto">
                        {Locale::get_all()
                            .iter()
                            .map(|l| {
                                let l = *l;
                                let current = move || i18n.get_locale() == l;
                                view! {
                                    <li>
                                        <a
                                            href=move || href(l)
                                            lang=l.as_str()
                                            hreflang=l.as_str()
                                            aria-current=move || current().then_some("true")
                                            class=move || {
                                                format!(
                                                    "flex min-h-10 items-center justify-between gap-2 rounded-lg px-3 py-2 text-sm transition-colors {}",
                                                    if current() {
                                                        "bg-primary text-primary-foreground"
                                                    } else {
                                                        "hover:bg-secondary"
                                                    },
                                                )
                                            }
                                            // English has no prefix, and <I18nRoute> sends an
                                            // unprefixed path back to the remembered language
                                            on:click=move |_| {
                                                i18n.set_locale(l);
                                                open.set(false);
                                            }
                                        >
                                            <span class="truncate">{native_name(l)}</span>
                                            <span class="shrink-0 font-pixel text-[11px] uppercase opacity-70">{code(l)}</span>
                                        </a>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                </div>
            </Show>
        </div>
    }
}
