// Leptos views are deeply nested types, release builds need more room
#![recursion_limit = "256"]

pub mod api;
pub mod app;
pub mod components;
pub mod pages;
pub mod state;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
