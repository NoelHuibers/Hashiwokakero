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

// One build runs in one place: hydrating what the server rendered, rendering in the browser
// alone (the mobile app), or on the server
#[cfg(all(feature = "hydrate", feature = "csr"))]
compile_error!("the features `hydrate` and `csr` exclude each other");
#[cfg(all(feature = "ssr", feature = "csr"))]
compile_error!("the features `ssr` and `csr` exclude each other");

/// The website's entry point: hydrates the page the server rendered (`app::shell`).
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
    mark_ready();
}

/// The mobile app's entry point: renders the whole app into the `<body>` of a static page.
/// That page brings what `app::shell` adds on the website, the stylesheet, icons and the theme
/// script.
///
/// `version` is the app's (`major.minor.patch`), sent with every call (`api::APP_HEADER`) and
/// compared with the server's minimum. Before the first call, the server functions are pointed
/// at the website's server when the page runs in the app's `WebView` (see `api::app`).
#[cfg(feature = "csr")]
pub fn mount(version: &'static str) {
    use leptos::prelude::Owner;
    console_error_panic_hook::set_once();
    api::start_app(version);
    // Before the router's listener, see there
    api::native::open_links_outside();
    let app = std::rc::Rc::new(std::cell::Cell::new(None));
    leptos::mount::mount_to_body({
        let app = app.clone();
        move || {
            app.set(Owner::current());
            app::App()
        }
    });
    // The app's owner stays the current one, as `hydrate_body` leaves it on the website: tasks
    // without an owner of their own (`spawn_local`) find the app's contexts after an `.await`,
    // the toasts of a call's answer for one. `mount_to_body` would leave none.
    if let Some(owner) = app.take() {
        owner.set();
    }
    // Game Center or Play Games, silently and in the background: the platforms want it early
    leptos::task::spawn_local(async {
        api::native::games_sign_in().await;
    });
    mark_ready();
}

/// Only reached when hydrating or mounting didn't panic. The browser tests (e2e/) wait for this
/// mark before they click anything.
#[cfg(any(feature = "hydrate", feature = "csr"))]
fn mark_ready() {
    if let Some(el) = leptos::prelude::document().document_element() {
        let _ = el.set_attribute("data-hydrated", "");
    }
}
