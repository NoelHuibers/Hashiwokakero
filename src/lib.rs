// Leptos views are deeply nested types, release builds need more room
#![recursion_limit = "256"]

pub mod api;
pub mod app;
pub mod audio;
pub mod components;
pub mod lang;
pub mod model;
pub mod pages;
pub mod seo;
#[cfg(feature = "ssr")]
pub mod server;
pub mod state;

#[cfg(test)]
mod golden;

/// The translations (`t!`, `use_i18n`, `Locale`, ...), generated in their own crate
pub use hashi_i18n::i18n;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
    // Only reached when hydration didn't panic. The browser tests (e2e/) wait for this mark
    // before they click anything.
    if let Some(el) = leptos::prelude::document().document_element() {
        let _ = el.set_attribute("data-hydrated", "");
    }
}
