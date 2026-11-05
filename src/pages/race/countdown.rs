//! The countdown on the server's clock: 用意 (ready), 3, 2, 1, 始め (go).

use leptos::prelude::*;

use crate::i18n::{t_string, use_i18n};
use crate::model::{Countdown as Stage, countdown};

use super::state::RaceState;

/// What the countdown shows now, `Hidden` without a start.
fn stage(race: &RaceState) -> Stage {
    let start = race.view.with(|v| v.as_ref().and_then(|v| v.starts_at));
    start.map_or(Stage::Hidden, |s| countdown(race.server_now(), s))
}

/// The countdown's big sign, and what it says to screen readers.
#[component]
fn Sign(stage: Memo<Stage>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        let (big, small) = match stage.get() {
            Stage::Ready => (
                "用意".to_string(),
                t_string!(i18n, race.countdown.ready).to_string(),
            ),
            Stage::Count(n) => (n.to_string(), String::new()),
            Stage::Go => (
                "始め".to_string(),
                t_string!(i18n, race.countdown.go).to_string(),
            ),
            Stage::Hidden => return None,
        };
        Some(view! {
            <div class="flex flex-col items-center gap-1" role="timer" aria-live="assertive" data-testid="race-countdown">
                <span class="font-pixel text-7xl leading-none text-primary sm:text-8xl">{big}</span>
                <span class="text-sm font-bold uppercase tracking-wider text-muted-foreground">{small}</span>
            </div>
        })
    }
}

/// The countdown in place of the board before the start.
#[component]
pub fn CountdownStage(race: RaceState) -> impl IntoView {
    let stage = Memo::new(move |_| stage(&race));
    view! { <Sign stage /> }
}

/// 始め over the board, the first second of the race.
#[component]
pub fn Countdown(race: RaceState) -> impl IntoView {
    let stage = Memo::new(move |_| stage(&race));
    view! {
        <Show when=move || stage.get() != Stage::Hidden>
            <div class="pointer-events-none absolute inset-0 grid place-items-center rounded-xl bg-background/60">
                <Sign stage />
            </div>
        </Show>
    }
}
