//! Client state shared by all pages: the current game, the editor, solver settings and toasts.

mod auth;
mod browser;
mod editor;
mod game;
mod hanko;
mod rivals;
mod settings;
mod solver;
mod toasts;

use std::time::Duration;

use common::{Puzzle, SolverConfig};
use leptos::prelude::*;

pub use auth::Auth;
use browser::store_raw;
pub use browser::{device_id, download, format_time, load, random_seed, store};
pub use editor::Editor;
pub use game::{DailyGame, Game, Source, Status};
pub use hanko::HankoInbox;
pub use rivals::Rivals;
pub use settings::Settings;
pub use solver::{check, check_unique, hint, new_puzzle, reveal, run_solver};
pub use toasts::{Toast, ToastKind, ToastText, Toasts, error_toast, toast};

const GAME_KEY: &str = "hashi.game.v2";
const DAILY_KEY: &str = "hashi.daily.v1";
const EDITOR_KEY: &str = "hashi.editor.v2";
const SOLVER_KEY: &str = "hashi.solver.v2";
const THEME_KEY: &str = "hashi.theme";
const SOUND_KEY: &str = "hashi.sound";
const MUSIC_KEY: &str = "hashi.music";

/// Creates all state and wires up persistence, the timer and win detection.
pub fn provide_state() {
    let game = Game::new(GAME_KEY);
    let daily = Game::new(DAILY_KEY);
    let editor = Editor::new();
    let settings = Settings::new();
    let auth = Auth {
        user: RwSignal::new(None),
        loaded: RwSignal::new(false),
    };
    provide_context(game);
    provide_context(DailyGame(daily));
    provide_context(auth);
    let rivals = Rivals::new();
    provide_context(rivals);
    let hanko = HankoInbox::new();
    provide_context(hanko);
    provide_context(editor);
    provide_context(settings);
    provide_context(Toasts(RwSignal::new(vec![])));

    // Everything below only runs in the browser
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        if let Some(puzzle) = load::<Puzzle>(EDITOR_KEY).filter(|p| p.validate().is_ok()) {
            editor.puzzle.set(puzzle);
        }
        if let Some(config) = load::<SolverConfig>(SOLVER_KEY) {
            settings.solver.set(config.normalized());
        }
        settings.dark.set(
            document()
                .document_element()
                .is_some_and(|el| el.class_list().contains("dark")),
        );
        if let Some(sound) = load::<bool>(SOUND_KEY) {
            settings.sound.set(sound);
        }
        if let Some(music) = load::<bool>(MUSIC_KEY) {
            settings.music.set(music);
        }
        // Browsers allow audio only after a gesture
        let unlock = window_event_listener(leptos::ev::pointerdown, |_| crate::audio::unlock());
        let unlock_keys = window_event_listener(leptos::ev::keydown, |_| crate::audio::unlock());
        on_cleanup(move || {
            unlock.remove();
            unlock_keys.remove();
        });
        auth.refresh();
    });

    // The rivals and unread hanko of whoever is signed in, again when that changes
    Effect::new(move |prev: Option<Option<i64>>| {
        let user = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if prev.is_some_and(|p| p != user) || (prev.is_none() && user.is_some()) {
            rivals.load(user.is_some());
            hanko.load(user.is_some());
        }
        user
    });

    // Sound settings, saved like the theme
    Effect::new(move |prev: Option<()>| {
        let sound = settings.sound.get();
        crate::audio::set_sound(sound);
        if prev.is_some() {
            store(SOUND_KEY, &sound);
        }
    });
    Effect::new(move |prev: Option<()>| {
        let music = settings.music.get();
        crate::audio::set_music(music);
        if prev.is_some() {
            store(MUSIC_KEY, &music);
            if music {
                crate::audio::unlock();
            }
        }
    });

    Effect::new(move |_| game.save());
    Effect::new(move |_| daily.save());
    Effect::new(move |_| store(EDITOR_KEY, &editor.puzzle.get()));
    Effect::new(move |_| store(SOLVER_KEY, &settings.solver.get()));
    Effect::new(move |prev: Option<bool>| {
        let dark = settings.dark.get();
        if prev.is_some() {
            if let Some(el) = document().document_element() {
                let _ = if dark {
                    el.class_list().add_1("dark")
                } else {
                    el.class_list().remove_1("dark")
                };
            }
            store_raw(THEME_KEY, if dark { "dark" } else { "light" });
        }
        dark
    });

    // Win detection
    for g in [game, daily] {
        Effect::new(move |_| {
            let solved = g.board.with(|b| g.bridges.with(|br| b.check(br).solved));
            if solved && g.status.get_untracked() == Status::Playing {
                g.status.set(Status::Won);
                crate::audio::play(crate::audio::Sfx::Win);
            }
        });
        // Hints and checks sound the same everywhere
        Effect::new(move |_| {
            if g.hint.get().is_some() {
                crate::audio::play(crate::audio::Sfx::Hint);
            }
        });
        Effect::new(move |_| {
            if g.wrong.with(|w| !w.is_empty()) {
                crate::audio::play(crate::audio::Sfx::Wrong);
            }
        });
    }

    // Timer, paused while the tab is hidden
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        set_interval(
            move || {
                let visible = document().visibility_state() == web_sys::VisibilityState::Visible;
                if visible
                    && game.status.get_untracked() == Status::Playing
                    && !game.loading.get_untracked()
                {
                    game.elapsed.update(|e| *e += 1);
                }
            },
            Duration::from_secs(1),
        );
    });
}

/// The daily this device saved (`DAILY_KEY`): its puzzle, its day (`Source::Daily`) and how far
/// it got. For the app's daily lock, see `api::practice`.
#[cfg(feature = "csr")]
pub fn saved_daily() -> Option<(Puzzle, Source, Status)> {
    game::saved(DAILY_KEY)
}
