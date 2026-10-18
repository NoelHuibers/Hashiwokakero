//! The frame around every page: header, navigation, footer and toasts.

pub mod footer;
pub mod header;
pub mod language;
pub mod nav;
pub mod settings;
pub mod toaster;

use leptos::prelude::*;

use crate::i18n::use_i18n;
use crate::lang::localized;

pub use footer::Footer;
pub use header::Header;
pub use nav::MobileNav;
pub use toaster::Toaster;

/// The hanko 判子 seal with 橋 (bridge), used as logo.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn Hanko(#[prop(into, optional)] class: String) -> impl IntoView {
    view! {
        <span
            class=format!(
                "hanko inline-grid place-items-center rounded-md font-pixel select-none rotate-[-6deg] {class}",
            )
            aria-hidden="true"
        >
            "橋"
        </span>
    }
}

/// A link to `path` in the current language.
pub fn use_localized(path: &'static str) -> impl Fn() -> String + Copy + Send + Sync + 'static {
    let i18n = use_i18n();
    move || localized(i18n.get_locale(), path)
}
