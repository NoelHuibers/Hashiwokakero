//! A new puzzle on Play: the difficulty chips above the board, the "new puzzle" button next to
//! the clock (and `N`), and the way back to the puzzle left for it.

use common::{Difficulty, GenerateParams};
use leptos::prelude::*;

use crate::i18n::{t_string, td_string, use_i18n};
use crate::lang::difficulty_name;
use crate::state::{self, Game, Settings, Snapshot, Source, ToastKind, Toasts, random_seed};

/// The key of the toast that offers the puzzle left back, one offer at a time.
const BACK: &str = "play.back";

/// A new puzzle of `difficulty`. Without asking: when the puzzle on the board was begun and not
/// finished, a toast offers it back once the new one is there.
pub(super) fn new_random(game: &Game, difficulty: Difficulty) {
    let (width, height) = difficulty.size();
    let params = GenerateParams {
        width,
        height,
        difficulty,
        seed: random_seed(),
    };
    let game = *game;
    let left = game.has_progress().then(|| game.snapshot());
    let toasts = use_context::<Toasts>();
    state::new_puzzle_then(game, params, Source::Generated(params), move || {
        if let (Some(left), Some(toasts)) = (left, toasts) {
            offer_back(&game, toasts, left);
        }
    });
}

/// The toast with "Back to previous puzzle", which brings `left` back as it was.
fn offer_back(game: &Game, toasts: Toasts, left: Snapshot) {
    let game = *game;
    toasts.push_action(
        ToastKind::Info,
        BACK,
        |l| td_string!(l, play.left).to_string(),
        |l| td_string!(l, play.back).to_string(),
        move || go_back(&game, toasts, left.clone()),
    );
}

/// Brings the puzzle left back. The one on the board then is offered back in turn if it was
/// begun, so going back loses nothing either.
fn go_back(game: &Game, toasts: Toasts, left: Snapshot) {
    let now = game.has_progress().then(|| game.snapshot());
    game.restore_snapshot(left);
    if let Some(now) = now {
        offer_back(game, toasts, now);
    }
}

/// The five difficulties above the board: a tap makes a new puzzle of it at once and keeps it for
/// the next ones (`N`, "next puzzle"). The one kept is pressed.
#[component]
pub(super) fn DifficultyChips(
    /// Where the row goes in the page's grid.
    #[prop(optional)]
    class: &'static str,
) -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    view! {
        <div
            class=format!("grid grid-cols-5 gap-1 sm:gap-1.5 {class}")
            role="group"
            aria-label=move || t_string!(i18n, play.new_puzzle)
        >
            {Difficulty::ALL
                .into_iter()
                .map(|d| {
                    let (w, h) = d.size();
                    let pressed = move || settings.difficulty.get() == d;
                    view! {
                        <button
                            type="button"
                            aria-pressed=move || pressed().to_string()
                            disabled=move || game.loading.get()
                            class=move || {
                                format!(
                                    "flex min-h-11 min-w-0 flex-col items-center justify-center rounded-lg border px-0.5 py-1 transition-colors disabled:opacity-60 max-[359px]:px-0 sm:px-1 sm:py-1.5 {}",
                                    if pressed() {
                                        "border-primary bg-primary text-primary-foreground shadow-sm"
                                    } else {
                                        "bg-background hover:border-primary/50 hover:bg-accent"
                                    },
                                )
                            }
                            data-tooltip=move || {
                                format!("{} · {w}×{h}", difficulty_name(i18n.get_locale(), d))
                            }
                            on:click=move |_| {
                                settings.difficulty.set(d);
                                new_random(&game, d);
                            }
                        >
                            <span class="font-pixel text-sm leading-none sm:text-base" aria-hidden="true">{d.kanji()}</span>
                            // Phones: lower case in the body font, smaller below 360 px. A fifth of
                            // the row has no room for MEDIUM or Складний in the display capitals
                            <span class="mt-0.5 max-w-full truncate text-[10px] font-bold uppercase max-sm:font-sans max-sm:whitespace-normal max-sm:normal-case max-sm:leading-tight max-[359px]:text-[9px] sm:mt-1 sm:tracking-wider">
                                {move || difficulty_name(i18n.get_locale(), d)}
                            </span>
                            <span class="hidden text-[10px] opacity-70 sm:block" aria-hidden="true">{format!("{w}×{h}")}</span>
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// Lucide's "refresh-cw".
const REFRESH: &str = "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8M21 3v5h-5M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16M8 16H3v5";

/// A new puzzle of the difficulty kept, next to the clock. Its name shows from `sm` on.
#[component]
pub(super) fn NewPuzzleButton() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    view! {
        <button
            type="button"
            class="inline-flex h-11 min-w-11 shrink-0 items-center justify-center gap-2 rounded-lg border bg-background px-0 text-sm font-medium shadow-xs transition-colors hover:bg-accent focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 sm:h-auto sm:min-w-13 sm:px-4 dark:border-input dark:bg-input/30"
            aria-label=move || t_string!(i18n, play.new_puzzle)
            data-tooltip=move || {
                let d = settings.difficulty.get();
                format!(
                    "{} · {} {} · N",
                    t_string!(i18n, play.new_puzzle),
                    d.kanji(),
                    difficulty_name(i18n.get_locale(), d),
                )
            }
            disabled=move || game.loading.get()
            on:click=move |_| new_random(&game, settings.difficulty.get_untracked())
        >
            <svg viewBox="0 0 24 24" class="size-5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d=REFRESH />
            </svg>
            <span class="hidden sm:inline">{move || t_string!(i18n, play.new_puzzle)}</span>
        </button>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::{Locale, td_string};
    use crate::state::{Status, ToastAction};

    #[tokio::test]
    async fn every_difficulty_exposes_exactly_one_pressed_choice() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::ar);
            let settings = Settings::new();
            provide_context(settings);
            provide_context(Game::pausable("test.play.chips"));
            for difficulty in Difficulty::ALL {
                settings.difficulty.set(difficulty);
                let html = view! { <DifficultyChips /> }.to_html();
                assert!(html.contains("role=\"group\""));
                assert!(html.contains(td_string!(Locale::ar, play.new_puzzle)));
                assert_eq!(html.matches("aria-pressed=\"true\"").count(), 1);
                assert_eq!(html.matches("aria-pressed=\"false\"").count(), 4);
                assert!(!html.contains(" title="));
                let pressed = html
                    .split("<button ")
                    .find(|b| {
                        b.split('>')
                            .next()
                            .unwrap()
                            .contains("aria-pressed=\"true\"")
                    })
                    .unwrap()
                    .split("</button>")
                    .next()
                    .unwrap();
                assert!(pressed.contains(&difficulty_name(Locale::ar, difficulty)));
                assert!(pressed.contains(difficulty.kanji()));
            }
        });
    }

    /// A game begun on Play: a row of three islands, two moves, a hint, a minute on the clock.
    fn begun(key: &'static str) -> Game {
        let game = Game::pausable(key);
        let mut puzzle = common::Puzzle::empty(3, 1);
        for x in 0..3 {
            puzzle.set(common::Pos::new(x, 0), 2);
        }
        game.set_puzzle(puzzle, None, None, Source::Custom);
        game.cycle(0, 1);
        game.cycle(1, 1);
        game.hints.set(1);
        game.elapsed.set(61);
        game
    }

    /// The button of the one toast shown, run as a click would.
    fn click_back(toasts: Toasts) {
        let shown = toasts.0.get_untracked();
        assert_eq!(shown.len(), 1);
        assert_eq!(
            (shown[0].text)(Locale::de),
            td_string!(Locale::de, play.left)
        );
        let Some(ToastAction::Run { label, run, .. }) = &shown[0].action else {
            panic!("a button: {:?}", shown[0].action);
        };
        assert_eq!(label(Locale::de), td_string!(Locale::de, play.back));
        toasts.dismiss(shown[0].id);
        run();
    }

    /// "Back to previous puzzle" brings the puzzle left back as it was; the new one, begun
    /// meanwhile, is offered back in turn, and a new one not begun is just dropped.
    #[test]
    fn the_puzzle_left_comes_back_and_nothing_is_lost() {
        Owner::new().with(|| {
            let game = begun("test.play.back");
            let toasts = Toasts::new();
            let left = game.snapshot();
            let before = (game.puzzle(), game.bridges.get_untracked());

            // The new puzzle is on the board, two moves into it
            let mut other = common::Puzzle::empty(1, 3);
            for y in 0..3 {
                other.set(common::Pos::new(0, y), 1);
            }
            game.set_puzzle(other.clone(), None, None, Source::Custom);
            offer_back(&game, toasts, left);
            game.cycle(0, 1);
            game.cycle(0, 1);

            click_back(toasts);
            assert_eq!((game.puzzle(), game.bridges.get_untracked()), before);
            assert_eq!(
                (
                    game.moves.get_untracked(),
                    game.hints.get_untracked(),
                    game.elapsed.get_untracked()
                ),
                (2, 1, 61)
            );
            assert_eq!(game.undo_stack.with_untracked(Vec::len), 2);
            assert_eq!(game.status.get_untracked(), Status::Playing);

            // And back to the other one, as it was
            click_back(toasts);
            assert_eq!(game.puzzle(), other);
            assert_eq!(game.bridges.get_untracked(), vec![2, 0]);
            assert_eq!(game.moves.get_untracked(), 2);

            // Once more, now without moves on the puzzle left behind: no new offer
            click_back(toasts);
            assert_eq!(game.puzzle(), before.0);
            game.restart();
            game.moves.set(0);
            let left = game.snapshot();
            offer_back(&game, toasts, left);
            click_back(toasts);
            assert!(toasts.0.with_untracked(Vec::is_empty));
        });
    }
}
