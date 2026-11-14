//! Sharing a puzzle as a link (`/p/<code>`, see `server::shares`): after solving it on Play or
//! on a shared puzzle's page, once the workshop proved it unique, or after a race.
//!
//! Two steps: the first button asks the server for the link, then the share bar shares a text
//! with it. The phone's share sheet only opens right after a tap, the server's answer may come
//! too late for the first one.

use common::Puzzle;
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::share::ShareBar;
use crate::api::{share_group_race, share_puzzle};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{error_text, localized};
use crate::model::{SharedSolve, share_clock, share_path};
use crate::state::{Auth, note_error};

/// What a link is made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShareRequest {
    /// A puzzle, and how the player solved it (`None`: not played).
    Puzzle(Puzzle, Option<SharedSolve>),
    /// The puzzle of a race that is over (`server::groups::race::share`).
    Race(i64),
}

/// The text a link is shared with: a title, what happened, then the link.
pub fn share_lines(title: &str, line: &str, url: &str) -> String {
    format!("🌉 Hashi · {title} · 橋をかけろ\n{line}\n{url}")
}

/// The text a link to a puzzle is shared with: what the player did, then the link. `solve`:
/// their time on it, `None` from the workshop.
pub fn share_text(locale: Locale, title: &str, solve: Option<(i64, u32)>, url: &str) -> String {
    let line = match solve {
        Some((time_ms, 0)) => td_string!(locale, shared.share.time, time = share_clock(time_ms)),
        Some((time_ms, hints)) => td_string!(
            locale,
            shared.share.time_hints,
            time = share_clock(time_ms),
            hints
        ),
        None => td_string!(locale, shared.share.untimed).to_string(),
    };
    share_lines(title, &line, url)
}

/// Where the link of `code` points, in the player's language: the website, also from the app.
pub fn share_url(locale: Locale, code: &str) -> String {
    crate::seo::public_origin() + &localized(locale, &share_path(code))
}

/// The kanji of the sharing stamps, 道 帯 師 (`model::stamps`).
const SHARING_KANJI: &str = "道 帯 師";

/// What the button has done.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Link {
    None,
    Making,
    Made(String),
    /// Why there is none: the error, in the player's language.
    Failed(String),
}

/// "Share this puzzle", then the link, and the share bar that shares it.
///
/// `request`: what the link is made of ([`ShareRequest`]). `text`: the text
/// shared with the link's URL. `fallback`: what the share bar shares until there is a link (or
/// where none can be made: offline, a puzzle with more than one solution), the result alone;
/// `None`: no share bar without a link.
#[component]
pub fn SharePuzzle(
    request: impl Fn() -> ShareRequest + Copy + Send + Sync + 'static,
    text: impl Fn(Locale, &str) -> String + Copy + Send + Sync + 'static,
    #[prop(optional)] fallback: Option<Callback<(), String>>,
    #[prop(optional)] outline: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    let auth = use_context::<Auth>();
    let link = RwSignal::new(Link::None);
    let make = move |_| {
        if link.get_untracked() == Link::Making {
            return;
        }
        link.set(Link::Making);
        let request = request();
        spawn_local(async move {
            let made = match request {
                ShareRequest::Puzzle(puzzle, solve) => share_puzzle(puzzle, solve).await,
                ShareRequest::Race(id) => share_group_race(id).await,
            };
            match made {
                Ok(made) => {
                    // A guest made for the link: the page shows them signed in
                    if let Some(auth) = auth
                        && auth.user.with_untracked(|u| u.as_ref().map(|u| u.id))
                            != Some(made.user.id)
                    {
                        auth.user.try_set(Some(made.user));
                    }
                    link.try_set(Link::Made(made.code));
                }
                Err(e) => {
                    note_error(&e.to_string());
                    let why = error_text(i18n.get_locale_untracked(), &e);
                    link.try_set(Link::Failed(why));
                }
            }
        });
    };
    let url = move || match link.get() {
        Link::Made(code) => Some(share_url(i18n.get_locale(), &code)),
        _ => None,
    };
    let shared_text = move || {
        let locale = i18n.get_locale_untracked();
        match link.get_untracked() {
            Link::Made(code) => text(locale, &share_url(locale, &code)),
            _ => fallback.map_or_default(|f| f.run(())),
        }
    };
    let has_link = move || matches!(link.get(), Link::Made(_));
    view! {
        <div class="flex w-full flex-col gap-2" data-testid="share-puzzle">
            {move || match link.get() {
                Link::None | Link::Making => view! {
                    <Button
                        variant=ButtonVariant::Default
                        class="w-full"
                        attr:disabled=move || link.get() == Link::Making
                        on:click=make
                    >
                        {move || if link.get() == Link::Making {
                            t_string!(i18n, shared.share.making).to_string()
                        } else {
                            format!("{} 共有", t_string!(i18n, shared.share.button))
                        }}
                    </Button>
                }
                    .into_any(),
                Link::Made(_) => view! {
                    <div class="flex flex-col gap-1 text-start">
                        <span class="text-xs font-bold uppercase tracking-wider text-muted-foreground">
                            {t!(i18n, shared.share.link)}
                        </span>
                        <a
                            href=move || url().unwrap_or_default()
                            class="truncate rounded-md border bg-muted/40 px-2 py-1.5 font-mono text-sm text-primary"
                            dir="ltr"
                            data-testid="share-link"
                        >
                            {move || url().unwrap_or_default()}
                        </a>
                        // Who solves it through the link counts for the sharing stamps
                        <p class="text-xs text-muted-foreground" data-testid="share-stamps">
                            {t!(i18n, shared.share.stamps)} " "
                            <span class="font-pixel whitespace-nowrap text-primary">{SHARING_KANJI}</span>
                        </p>
                    </div>
                }
                    .into_any(),
                Link::Failed(why) => view! {
                    <p class="text-sm text-muted-foreground" role="status">
                        {t_string!(i18n, shared.share.failed, error = why)}
                    </p>
                }
                    .into_any(),
            }}
            // The result until there is a link, then the link
            <Show when=move || fallback.is_some() || has_link()>
                <ShareBar
                    text=shared_text
                    outline=outline
                    label=Signal::derive(move || {
                        if has_link() {
                            t_string!(i18n, shared.share.send).to_string()
                        } else {
                            t_string!(i18n, common.game.share).to_string()
                        }
                    })
                />
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Stamp;

    #[test]
    fn the_text_says_what_the_player_did() {
        let url = "https://hashi.rs/p/k7m2xq9a";
        assert_eq!(
            share_text(Locale::en, "Hard 14×14", Some((133_000, 0)), url),
            "🌉 Hashi · Hard 14×14 · 橋をかけろ\nI solved this Hashi puzzle in 02:13. Can you beat me?\nhttps://hashi.rs/p/k7m2xq9a"
        );
        assert!(
            share_text(Locale::en, "x", Some((133_000, 2)), url)
                .contains("in 02:13 with 2 hints (+30 s each)")
        );
        assert!(share_text(Locale::en, "x", None, url).contains("I built this Hashi puzzle"));
    }

    #[test]
    fn the_hint_names_the_sharing_stamps() {
        let kanji = [Stamp::Dojo, Stamp::BlackBelt, Stamp::Sensei].map(Stamp::kanji);
        assert_eq!(SHARING_KANJI, kanji.join(" "));
    }
}
