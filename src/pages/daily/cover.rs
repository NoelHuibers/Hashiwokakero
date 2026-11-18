//! The cover over the board before the daily is started.

use common::{Board, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{current_user, start_daily, start_warm_up};
use crate::components::board::{BoardMode, BoardView};
use crate::components::ui::button::{Button, ButtonSize};
use crate::components::ui::card::Card;
use crate::i18n::{t, t_string, use_i18n};
use crate::model::daily_difficulty;
use crate::state::{Auth, device_id, error_toast};

use super::state::{Daily, RefreshGuard, browser_now};

/// How often hovering, focusing or touching Start warms the server up, at most: a connection
/// the browser opened stays open about this long without traffic.
const WARM_UP_EVERY_MS: i64 = 20_000;

/// Whether a warm-up at `now` is due after the last one at `last` (browser ms). A clock that went
/// back (the computer's time was set) doesn't keep it from ever being due again.
fn warm_up_due(last: Option<i64>, now: i64) -> bool {
    last.is_none_or(|last| now < last || now - last >= WARM_UP_EVERY_MS)
}

/// The board before the start: an empty sea in the fog.
#[component]
pub(super) fn Cover(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let size = Signal::derive(move || {
        let day = daily.info.with(|i| i.as_ref().map(|i| i.day)).unwrap_or(0);
        daily_difficulty(day).size()
    });
    let board = Signal::derive(move || {
        let (w, h) = size.get();
        Board::new(Puzzle::empty(w, h))
    });
    // The player is about to press Start: the server wakes up and the connections open while the
    // pointer moves or the finger comes down, not after the press (`api::start_warm_up`). The
    // clock starts with the start alone, this changes nothing of it.
    let warmed = StoredValue::new(None::<i64>);
    let warm_up = move || {
        let now = browser_now();
        if daily.busy.get_untracked() || !warm_up_due(warmed.get_value(), now) {
            return;
        }
        warmed.set_value(Some(now));
        spawn_local(async move {
            // Only a head start: a failure shows with the start, if it lasts
            let _ = start_warm_up().await;
        });
    };
    let start = move |_| {
        if daily.busy.get_untracked() {
            return;
        }
        let Some(request) = daily.starting.begin(false) else {
            return;
        };
        daily.busy.set(true);
        let guard = RefreshGuard::new(&daily);
        spawn_local(async move {
            let response = start_daily(Some(device_id())).await;
            if !daily.starting.current(request) {
                return;
            }
            // Without an account the start made a guest, whom the reply names (`take_start`).
            // An older server's reply doesn't: then the board waits until the page knows them.
            // Until then it can't take their clicks, and the new identity reloads the daily (the
            // identity effect in `DailyPage`), which would swap a board shown now for the loading
            // placeholder under the first clicks. That reload shows the started daily.
            let guest = match &response {
                Ok(start) if start.user.is_none() && auth.user.with_untracked(Option::is_none) => {
                    current_user().await.ok().flatten()
                }
                _ => None,
            };
            if !daily.starting.current(request) {
                return;
            }
            daily.starting.finish(request, false);
            if let Some(guest) = guest {
                auth.user.set(Some(guest));
                return;
            }
            daily.busy.set(false);
            if !guard.current(&daily) {
                return;
            }
            match response {
                Ok(start) => daily.take_start(start),
                Err(e) => error_toast(e),
            }
        });
    };
    // A penalty as a small tag: what you do and what it costs.
    let rule = |label: String, cost: String, strong: bool| {
        view! {
            <li class="inline-flex items-center gap-1.5 rounded-full border bg-background/60 py-1 ps-2.5 pe-2 text-xs">
                <span class="text-muted-foreground">{label}</span>
                // dir keeps "+30s" from turning into "30s+" in right-to-left languages
                <span
                    class=if strong { "font-bold text-primary" } else { "font-pixel tabular-nums text-foreground" }
                    dir=(!strong).then_some("ltr")
                >
                    {cost}
                </span>
            </li>
        }
    };
    view! {
        // Board and card share one grid cell, so a card taller than the board
        // (narrow screens) grows the cell instead of spilling over what follows.
        <div class="grid place-items-center">
            // The fog lifts the moment Start is pressed, while the start's request is on its way
            <div
                class=move || {
                    if daily.busy.get() {
                        "col-start-1 row-start-1 w-full opacity-90 blur-none transition-[opacity,filter] duration-500"
                    } else {
                        "col-start-1 row-start-1 w-full opacity-50 blur-[3px] transition-[opacity,filter] duration-500"
                    }
                }
                data-testid="daily-fog"
                aria-hidden="true"
            >
                <BoardView
                    board=board
                    bridges=Signal::derive(move || board.with(Board::empty_bridges))
                    mode=BoardMode::View
                    framed=true
                />
            </div>
            <div class="relative col-start-1 row-start-1 grid w-full place-items-center p-4 sm:p-8">
                <Card class="w-full max-w-sm gap-4 border-border/80 bg-card/90 px-5 py-5 shadow-2xl backdrop-blur-md sm:gap-5 sm:px-6 sm:py-6">
                    <div class="flex items-center gap-3">
                        <span class="hanko grid size-11 shrink-0 rotate-[-6deg] place-items-center rounded-lg font-pixel text-2xl">
                            "橋"
                        </span>
                        <div class="min-w-0">
                            <p class="font-pixel text-xs tracking-[0.3em] text-primary">"今日の橋"</p>
                            <h2 class="text-xl font-black leading-tight sm:text-2xl">{t!(i18n, daily.cover.title)}</h2>
                        </div>
                    </div>
                    <p class="text-sm leading-relaxed text-muted-foreground">{t!(i18n, daily.cover.lead)}</p>
                    <ul class="flex flex-wrap gap-1.5">
                        {move || rule(t_string!(i18n, common.game.hint).to_string(), format!("+{}s", daily.penalties().hint_ms / 1000), false)}
                        {move || rule(t_string!(i18n, common.game.check).to_string(), format!("+{}s", daily.penalties().check_ms / 1000), false)}
                        {rule(t_string!(i18n, daily.give_up).to_string(), t_string!(i18n, daily.cover.unranked).to_string(), true)}
                    </ul>
                    <div class="flex flex-col gap-2.5">
                        <Button
                            size=ButtonSize::Lg
                            class="h-12 w-full text-base font-bold shadow-lg shadow-primary/25"
                            attr:disabled=move || daily.busy.get() || daily.info.with(Option::is_none)
                            on:click=start
                            on:pointerenter=move |_| warm_up()
                            on:pointerdown=move |_| warm_up()
                            on:focus=move |_| warm_up()
                        >
                            {move || if daily.busy.get() {
                                t_string!(i18n, daily.cover.preparing).to_string()
                            } else {
                                format!("{} 始め", t_string!(i18n, daily.cover.start))
                            }}
                            <svg viewBox="0 0 24 24" class="size-4 rtl:-scale-x-100" fill="currentColor" aria-hidden="true">
                                <path d="M8 5v14l11-7z" />
                            </svg>
                        </Button>
                        <p class="text-center text-xs text-muted-foreground">
                            {move || match auth.user.get() {
                                Some(u) if !u.guest => t_string!(i18n, daily.cover.playing_as, name = u.name),
                                Some(u) => t_string!(i18n, daily.cover.playing_as_guest, name = u.name),
                                None => t_string!(i18n, daily.cover.no_account).to_string(),
                            }}
                        </p>
                    </div>
                </Card>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warm_up_goes_out_at_most_every_twenty_seconds() {
        assert!(warm_up_due(None, 0));
        assert!(!warm_up_due(Some(1_000), 1_000));
        assert!(!warm_up_due(Some(1_000), 20_999));
        assert!(warm_up_due(Some(1_000), 21_000));
        // The computer's clock was set back
        assert!(warm_up_due(Some(50_000), 1_000));
    }
}
