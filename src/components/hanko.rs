//! Hanko 判子 seals as pixel art, and the dot that tells about new ones on one's results.

use leptos::prelude::*;

use super::sprites::PixelSprite;
use crate::i18n::{td_string, use_i18n};
use crate::model::HankoKind;
use crate::state::HankoInbox;

/// A round seal pressed in vermilion ink, its outline only. A bit worn on the left, like a real
/// one. 12×12, so it stays crisp at 24 and 36 px.
const RING: [&str; 12] = [
    "...rrrrrr...",
    "..r......r..",
    ".r........r.",
    "r..........r",
    "...........r",
    "r..........R",
    "r..........r",
    "R..........r",
    "r..........r",
    ".r........r.",
    "..rR.....r..",
    "...rrrrrr...",
];

/// The same seal inked all over: the ones the viewer pressed.
const SOLID: [&str; 12] = [
    "...RRRRRR...",
    "..RrrrrrrR..",
    ".RrrrrrrrrR.",
    "RrrrrrrrrrrR",
    "rrrrrrrrrrrR",
    "RrrrrrrrrrrR",
    "RrrrrrrrrrrR",
    "RrrrrrrrrrrR",
    "RrrrrrrrrrrR",
    ".RrrrrrrrrR.",
    "..RRrrrrrR..",
    "...RRRRRR...",
];

/// A hanko seal of `kind`: its kanji in a round pixel outline, or inked all over (`solid`),
/// turned by `tilt` degrees. Sized by `class` (`size-6 text-[12px]`, `size-9 text-lg`);
/// `hanko-press` brings it down with a thud, `delay` ms late. Decorative: say what it is next
/// to it.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn Seal(
    kind: HankoKind,
    #[prop(optional)] solid: bool,
    #[prop(optional)] tilt: i64,
    #[prop(optional)] delay: u32,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    view! {
        <span
            class=format!(
                "hanko-seal relative inline-grid shrink-0 place-items-center select-none {class}",
            )
            data-solid=solid.to_string()
            data-kind=kind.id()
            style=format!("--tilt: {tilt}deg; --delay: {delay}ms")
            aria-hidden="true"
        >
            <PixelSprite
                art=if solid { &SOLID } else { &RING }
                class="absolute inset-0 size-full"
            />
            <span class="relative font-pixel leading-none">{kind.kanji()}</span>
        </span>
    }
}

/// A small vermilion dot while hanko on the player's results wait to be seen, for the groups'
/// entry of the navigation. Says how many to screen readers.
#[component]
pub fn HankoDot(#[prop(into, optional)] class: String) -> impl IntoView {
    let i18n = use_i18n();
    let inbox = expect_context::<HankoInbox>();
    move || {
        let count = inbox.total();
        (count > 0).then(|| {
            view! {
                <span
                    class=format!("hanko-dot pointer-events-none {class}")
                    data-testid="hanko-dot"
                >
                    <span class="sr-only">
                        {move || td_string!(i18n.get_locale(), hanko.unread, count)}
                    </span>
                </span>
            }
        })
    }
}
