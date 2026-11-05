//! Pausing Play: the button next to the clock (and `P`), and what covers the board meanwhile.
//! Only Play's own game pauses (`Game::pausable`): the daily, group puzzles and races are ranked.

use leptos::prelude::*;

use crate::components::ui::button::Button;
use crate::i18n::{t, t_string, use_i18n};
use crate::state::Game;

/// Lucide's "pause" and "play".
const PAUSE: &str = "M14 4h4v16h-4zM6 4h4v16H6z";
const PLAY: &str = "M6 3l14 9-14 9z";

/// A toggle: pressed while paused, then it shows the triangle that resumes.
#[component]
pub(super) fn PauseButton() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let paused = game.paused;
    view! {
        <button
            type="button"
            class="grid h-11 min-w-11 shrink-0 place-items-center rounded-lg border bg-background shadow-xs transition-colors hover:bg-accent focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground sm:h-auto sm:min-w-13 dark:border-input dark:bg-input/30"
            aria-label=move || t_string!(i18n, play.pause)
            aria-pressed=move || paused.get().to_string()
            data-tooltip=move || {
                if paused.get() {
                    format!("{} · P", t_string!(i18n, play.resume))
                } else {
                    format!("{} · P", t_string!(i18n, play.pause))
                }
            }
            disabled=move || !paused.get() && !game.can_pause()
            on:click=move |_| game.toggle_pause()
        >
            <svg viewBox="0 0 24 24" class="size-5" fill="currentColor" aria-hidden="true">
                <path d=move || if paused.get() { PLAY } else { PAUSE } />
            </svg>
        </button>
    }
}

/// Covers the board while paused, which is hidden under it (not just dimmed: a pause is no time
/// to study it). Resuming goes on with the same clock.
#[component]
pub(super) fn PausedCover() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    view! {
        <Show when=move || game.paused.get()>
            <div
                class="absolute inset-0 z-20 grid place-items-center rounded-2xl border bg-card p-4 shadow-inner animate-in fade-in"
                data-testid="paused"
            >
                <div class="flex max-w-xs flex-col items-center gap-3 text-center">
                    <span class="hanko grid size-14 rotate-[-6deg] place-items-center rounded-md font-pixel text-3xl" aria-hidden="true">
                        "休"
                    </span>
                    <h2 class="text-2xl font-black">{t!(i18n, play.paused)} " · 休憩"</h2>
                    <p class="text-sm text-muted-foreground">{t!(i18n, play.paused_text)}</p>
                    <Button class="h-11 px-6" on:click=move |_| game.resume()>
                        {t!(i18n, play.resume)} " 再開"
                    </Button>
                </div>
            </div>
        </Show>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::{Locale, td_string};
    use crate::state::Source;

    /// Whether the first tag has the `disabled` attribute (not the class's `disabled:` variant).
    fn disabled(html: &str) -> bool {
        let tag = &html[..=html.find('>').unwrap()];
        [" disabled ", " disabled>", " disabled="]
            .iter()
            .any(|a| tag.contains(a))
    }

    #[tokio::test]
    async fn the_pause_button_is_a_named_toggle_and_the_cover_shows_only_while_paused() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::uk);
            let game = Game::pausable("test.play.pause");
            provide_context(game);
            let button = view! { <PauseButton /> }.to_html();
            assert!(button.contains(&format!(
                "aria-label=\"{}\"",
                td_string!(Locale::uk, play.pause)
            )));
            assert!(button.contains("aria-pressed=\"false\""));
            assert!(disabled(&button), "nothing to pause yet: {button}");
            assert!(!button.contains(" title="));
            assert_eq!(
                view! { <PausedCover /> }
                    .to_html()
                    .matches("data-testid")
                    .count(),
                0
            );

            game.set_puzzle(crate::golden::puzzle(), None, None, Source::Custom);
            game.pause();
            let button = view! { <PauseButton /> }.to_html();
            assert!(button.contains("aria-pressed=\"true\""));
            assert!(!disabled(&button), "{button}");
            assert!(button.contains(td_string!(Locale::uk, play.resume)));
            let cover = view! { <PausedCover /> }.to_html();
            assert!(cover.contains("data-testid=\"paused\""));
            assert!(cover.contains(td_string!(Locale::uk, play.paused)));
            assert!(cover.contains(td_string!(Locale::uk, play.resume)));
        });
    }
}
