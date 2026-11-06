//! A race (勝負 shōbu) of a group, `/groups/:id/races/:race`: the lobby, the countdown, the race
//! with everyone's progress, and the results with the replay and the rematch.
//!
//! Everything comes from the server, which the page asks every second or two
//! (`poll_group_race`, see [`state::RaceState`] and `model::poll_delay_ms`), not while the tab is
//! hidden. The answers carry the server's clock: the countdown and the race's clock follow it, not
//! the browser's.

pub mod countdown;
pub mod lobby;
pub mod results;
pub mod state;
pub mod track;

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Meta, Title};
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::{
    give_up_group_race, group_race_check, group_race_hint, poll_group_race, submit_group_race,
};
use crate::components::board::{BoardMode, BoardView};
use crate::components::celebration::{Petals, Stamp};
use crate::components::layout::use_localized;
use crate::components::paid_controls::{PaidControls, show_check, show_hint};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{difficulty_name, prefix};
use crate::model::{RaceOutcome, RacePhase, RaceShare, RaceView, format_duration, is_blocked_name};
use crate::pages::daily::Stat;
use crate::seo::title_text;
use crate::state::{Auth, Game, Status, error_toast};

use countdown::{Countdown, CountdownStage};
use lobby::Lobby;
use results::Results;
use state::{GAME_KEY, RaceState};
use track::Tracks;

/// The path of a race's page (without the language).
pub fn race_path(group: i64, race: i64) -> String {
    format!("/groups/{group}/races/{race}")
}

/// Where a race's shared puzzle comes from: "Shared puzzle · by aiko".
pub fn shared_by(locale: Locale, share: &RaceShare) -> String {
    match &share.sharer {
        Some(s) if is_blocked_name(&s.name) => {
            td_string!(locale, race.shared.by_blocked).to_string()
        }
        Some(s) => td_string!(locale, race.shared.by, name = s.name.clone()),
        None => td_string!(locale, race.shared.by_former).to_string(),
    }
}

/// The marker of a player who solved a race's shared puzzle before the race: they know it.
#[component]
pub fn SolvedBefore() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <span
            class="inline-flex shrink-0 items-center gap-1 rounded border px-1 text-[10px] leading-4 text-muted-foreground"
            data-tooltip=move || t_string!(i18n, race.shared.before_hint)
            data-testid="race-solved-before"
        >
            <span class="font-pixel" aria-hidden="true">"既"</span>
            <span class="sr-only sm:not-sr-only">{t!(i18n, race.shared.before)}</span>
        </span>
    }
}

/// The browser clock, unix time in ms.
#[expect(
    clippy::cast_possible_truncation,
    reason = "`Date.now()` is a whole number of ms, far inside the i64 range"
)]
pub(crate) fn browser_now() -> i64 {
    js_sys::Date::now() as i64
}

/// The page's requests to the server, one after the other: a chain of timeouts that ends when
/// another one starts (`run` moved on), the page is gone, or the race is.
#[derive(Clone, Copy, Debug)]
struct Poller {
    race: RaceState,
    /// The chain that runs; a new one makes the others stop.
    run: StoredValue<u32>,
    /// `Some(false)`: no such race, or not of the visitor's groups.
    found: RwSignal<Option<bool>>,
}

impl Poller {
    /// Starts a new chain for race `id` of group `group`, at once.
    fn restart(self, group: Option<i64>, id: i64) {
        let Some(run) = self.run.try_update_value(|r| {
            *r += 1;
            *r
        }) else {
            return;
        };
        self.poll(group, id, run);
    }

    fn schedule(self, group: Option<i64>, id: i64, run: u32, ms: u32) {
        set_timeout(
            move || self.poll(group, id, run),
            Duration::from_millis(u64::from(ms)),
        );
    }

    fn poll(self, group: Option<i64>, id: i64, run: u32) {
        if self.run.try_get_value() != Some(run) {
            return;
        }
        // Paused while the tab is hidden, it asks again when it's back (`visibilitychange`)
        if document().visibility_state() != web_sys::VisibilityState::Visible {
            return;
        }
        let race = self.race;
        let progress = race.take_progress(browser_now());
        let puzzle = race.view.with_untracked(Option::is_none) || race.wants_puzzle();
        let sent = browser_now();
        spawn_local(async move {
            let reply = poll_group_race(id, progress, puzzle).await;
            if self.run.try_get_value() != Some(run) {
                return;
            }
            match reply {
                Ok(Some(view)) if Some(view.group_id) == group && view.id == id => {
                    race.apply(view, sent, browser_now());
                    self.found.set(Some(true));
                }
                Ok(_) => {
                    self.found.set(Some(false));
                    return;
                }
                Err(e) => {
                    if progress.is_some() {
                        race.progress_failed();
                    }
                    // The first answer: the page has nothing to show; later, the next one may work
                    if self.found.get_untracked().is_none() {
                        error_toast(e);
                    }
                }
            }
            self.schedule(group, id, run, race.next_poll_ms());
        });
    }
}

impl RaceState {
    /// Sends the solved board with the move log.
    fn submit(self) {
        let Some(id) = self.id() else { return };
        // The last click may not be logged yet
        self.record_moves();
        self.busy.set(true);
        self.failed.set(false);
        let bridges = self.game.bridges.get_untracked();
        let moves = self.game.moves.get_untracked();
        let log = self.log();
        spawn_local(async move {
            let sent = browser_now();
            let reply = submit_group_race(id, bridges, moves, log).await;
            if self.busy.try_set(false).is_some() || self.id() != Some(id) {
                return;
            }
            match reply {
                Ok(view) => {
                    self.won_here.set(true);
                    self.apply(view, sent, browser_now());
                }
                Err(e) => {
                    self.failed.set(true);
                    error_toast(e);
                }
            }
        });
    }

    /// Counts a help of the viewer on the page, before the next answer says so.
    fn count_help(self, hint: bool) {
        self.view.update(|v| {
            if let Some(v) = v {
                let me = v.me;
                if let Some(p) = v.players.iter_mut().find(|p| p.user_id == me) {
                    if hint {
                        p.hints += 1;
                    } else {
                        p.checks += 1;
                    }
                }
            }
        });
    }
}

/// A race that isn't there (any more), or of a group the visitor isn't in.
#[component]
fn Missing() -> impl IntoView {
    let i18n = use_i18n();
    let groups = use_localized("/groups");
    view! {
        <div class="mx-auto flex max-w-xl flex-col items-center gap-4 py-16 text-center" data-testid="race-missing">
            <Stamp kanji="迷" class="size-20 text-5xl" />
            <h1 class="text-2xl font-black">{t!(i18n, race.page.missing)}</h1>
            <p class="text-muted-foreground">{t!(i18n, race.page.missing_hint)}</p>
            <A href=groups attr:class="rounded-md bg-primary px-4 py-2 font-medium text-primary-foreground">
                {t!(i18n, groups.page.back)}
            </A>
        </div>
    }
}

/// The page of a race.
#[component]
pub fn RacePage() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let params = use_params_map();
    let group_id =
        Memo::new(move |_| params.with(|p| p.get("id").and_then(|id| id.parse::<i64>().ok())));
    let race_id =
        Memo::new(move |_| params.with(|p| p.get("race").and_then(|id| id.parse::<i64>().ok())));
    let game = Game::new(GAME_KEY);
    let race = RaceState::new(game);
    let poller = Poller {
        race,
        run: StoredValue::new(0),
        found: RwSignal::new(None),
    };
    // The poll sets `found` on every answer; a memo keeps the page's body (and the board in it,
    // with the island the player just tapped) from being built anew each time
    let shown = Memo::new(move |_| poller.found.get());
    let found = poller.found;

    // Ask for the race, again when the visitor or the race changes (a rematch)
    Effect::new(move |_| {
        let (group, id) = (group_id.get(), race_id.get());
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        if !auth.loaded.get() {
            return;
        }
        match id {
            Some(id) => untrack(|| poller.restart(group, id)),
            None => found.set(Some(false)),
        }
    });
    // The clock, and asking again as soon as the tab is back (browser only)
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        race.now.set(browser_now());
        if let Ok(handle) = set_interval_with_handle(
            move || {
                race.now.try_set(browser_now());
            },
            Duration::from_millis(100),
        ) {
            on_cleanup(move || handle.clear());
        }
        let visible = window_event_listener_untyped("visibilitychange", move |_| {
            if document().visibility_state() == web_sys::VisibilityState::Visible
                && let Some(id) = race_id.try_get_untracked().flatten()
                && found.get_untracked() != Some(false)
            {
                poller.restart(group_id.get_untracked(), id);
            }
        });
        on_cleanup(move || visible.remove());
    });
    Effect::new(move |_| {
        if race.racing() && race.has_board() {
            game.save();
        }
    });
    // Solved, the sounds of hints and wrong bridges, like every game
    Effect::new(move |_| {
        let solved = game
            .board
            .with(|b| game.bridges.with(|br| b.check(br).solved));
        if solved && game.status.get_untracked() == Status::Playing {
            game.status.set(Status::Won);
            crate::audio::play(crate::audio::Sfx::Win);
        }
    });
    Effect::new(move |_| {
        if game.hint.get().is_some() {
            crate::audio::play(crate::audio::Sfx::Hint);
        }
    });
    Effect::new(move |_| {
        if game.wrong.with(|w| !w.is_empty()) {
            crate::audio::play(crate::audio::Sfx::Wrong);
        }
    });
    Effect::new(move |_| race.record_moves());
    Effect::new(move |_| {
        if race.ready_to_submit() {
            untrack(|| race.submit());
        }
    });

    let back = move || {
        let p = prefix(i18n.get_locale());
        match group_id.get() {
            Some(id) => format!("{p}/groups/{id}"),
            None => format!("{p}/groups"),
        }
    };
    let back_text = move || {
        race.view
            .with(|v| v.as_ref().map(|v| v.group_name.clone()))
            .unwrap_or_else(|| t_string!(i18n, groups.page.back).to_string())
    };
    view! {
        <Title text=title_text(move || format!("{} · Hashi", t_string!(i18n, race.title))) />
        <Meta name="robots" content="noindex" />
        <section class="mx-auto flex max-w-7xl flex-col gap-5 px-4 py-6 sm:px-6">
            <A href=back exact=true attr:class="w-fit text-sm text-primary hover:underline">
                "← " {back_text}
            </A>
            {move || match shown.get() {
                None => view! { <p class="text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Some(false) => view! { <Missing /> }.into_any(),
                Some(true) => view! { <RaceBody race /> }.into_any(),
            }}
        </section>
    }
}

/// The header: 勝負, the group and the puzzle's difficulty and size, or the shared puzzle's
/// sharer and size.
#[component]
fn Header(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let difficulty = Memo::new(move |_| {
        race.view.with(|v| {
            v.as_ref()
                .filter(|v| v.shared.is_none())
                .map(|v| v.difficulty)
        })
    });
    let shared = Memo::new(move |_| {
        race.view
            .with(|v| v.as_ref().and_then(|v| v.shared.clone()))
    });
    let left = Memo::new(move |_| {
        let deadline = race
            .view
            .with(|v| v.as_ref().and_then(RaceView::deadline))?;
        (race.phase() == Some(RacePhase::Running))
            .then(|| (deadline - race.server_now() + 59_999) / 60_000)
    });
    view! {
        <div class="flex min-w-0 items-center gap-3">
            <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl">
                "勝"
            </span>
            <div class="min-w-0">
                <p class="font-pixel text-sm text-primary">"勝負 · shōbu"</p>
                <h1 class="break-words text-2xl font-black sm:text-3xl">{t!(i18n, race.title)}</h1>
                <div class="mt-1 flex flex-wrap items-center gap-1.5 text-sm text-muted-foreground">
                    {move || difficulty.get().map(|d| {
                        let (w, h) = d.size();
                        view! {
                            <Badge variant=BadgeVariant::Muted>
                                <span class="font-pixel" aria-hidden="true">{d.kanji()}</span>
                                " " {move || difficulty_name(i18n.get_locale(), d)}
                            </Badge>
                            <Badge variant=BadgeVariant::Muted>{format!("{w}×{h}")}</Badge>
                        }
                    })}
                    {move || shared.get().map(|share| {
                        let size = format!("{}×{}", share.width, share.height);
                        view! {
                            <Badge variant=BadgeVariant::Muted attr:data-testid="race-shared">
                                <span class="font-pixel me-1" aria-hidden="true">"共"</span>
                                {move || shared_by(i18n.get_locale(), &share)}
                            </Badge>
                            <Badge variant=BadgeVariant::Muted>{size}</Badge>
                        }
                    })}
                    {move || left.get().map(|minutes| view! {
                        <Badge variant=BadgeVariant::Default>
                            {move || t_string!(i18n, race.play.time_left, minutes)}
                        </Badge>
                    })}
                </div>
            </div>
        </div>
    }
}

/// The player's board: empty with the countdown before the start, the game then.
#[component]
fn Board(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let game = race.game;
    let has_board = Memo::new(move |_| race.has_board());
    let size = Memo::new(move |_| {
        race.view
            .with(|v| v.as_ref().map_or((7, 7), RaceView::size))
    });
    move || {
        if has_board.get() {
            view! {
                <div class="relative" data-testid="race-board">
                    <BoardView
                        board=game.board
                        bridges=game.bridges
                        mode=BoardMode::Play
                        on_cycle=Callback::new(move |(edge, delta)| game.cycle(edge, delta))
                        wrong=game.wrong
                        hint=game.hint
                        framed=true
                    />
                    <Countdown race />
                    <Show when=move || race.won_here.get() && game.status.get() == Status::Won>
                        <Petals />
                    </Show>
                </div>
            }
            .into_any()
        } else {
            let (w, h) = size.get();
            view! {
                <div
                    class="relative grid w-full place-items-center rounded-xl border-2 border-dashed bg-muted/40"
                    style=format!("aspect-ratio: {w} / {h}; max-height: 70vh")
                    data-testid="race-waiting"
                >
                    <Show
                        when=move || race.phase() == Some(RacePhase::Countdown)
                        fallback=move || view! {
                            <p class="text-sm text-muted-foreground">{t!(i18n, race.play.loading)}</p>
                        }
                    >
                        <CountdownStage race />
                    </Show>
                </div>
            }
            .into_any()
        }
    }
}

/// The controls while the player races.
#[component]
fn Controls(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let game = race.game;
    let playing = Signal::derive(move || {
        race.racing()
            && race.has_board()
            && game.status.get() == Status::Playing
            && !race.busy.get()
    });
    let hint = move |()| {
        let Some(id) = race.id() else { return };
        race.busy.set(true);
        let bridges = game.bridges.get_untracked();
        let epoch = game.epoch.get_untracked();
        spawn_local(async move {
            let reply = group_race_hint(id, bridges).await;
            if race.busy.try_set(false).is_some() || game.epoch.try_get_untracked() != Some(epoch) {
                return;
            }
            match reply {
                Ok(hint) => {
                    if hint.is_some() {
                        race.count_help(true);
                    }
                    show_hint(game, hint);
                }
                Err(e) => error_toast(e),
            }
        });
    };
    let check = move |()| {
        let Some(id) = race.id() else { return };
        race.busy.set(true);
        let bridges = game.bridges.get_untracked();
        let epoch = game.epoch.get_untracked();
        spawn_local(async move {
            let reply = group_race_check(id, bridges).await;
            if race.busy.try_set(false).is_some() || game.epoch.try_get_untracked() != Some(epoch) {
                return;
            }
            match reply {
                Ok(wrong) => {
                    race.count_help(false);
                    show_check(game, wrong);
                }
                Err(e) => error_toast(e),
            }
        });
    };
    let give_up = move |()| {
        let Some(id) = race.id() else { return };
        race.busy.set(true);
        spawn_local(async move {
            let sent = browser_now();
            let reply = give_up_group_race(id).await;
            if race.busy.try_set(false).is_some() || race.id() != Some(id) {
                return;
            }
            match reply {
                Ok(view) => race.apply(view, sent, browser_now()),
                Err(e) => error_toast(e),
            }
        });
    };
    view! {
        <PaidControls
            game
            playing
            busy=race.busy
            penalties=Signal::derive(move || race.view.with(|v| v.as_ref().map(|v| v.penalties).unwrap_or_default()))
            on_hint=Callback::new(hint)
            on_check=Callback::new(check)
            on_give_up=Callback::new(give_up)
            give_up=Signal::derive(move || t_string!(i18n, race.play.give_up).to_string())
            give_up_question=Signal::derive(move || t_string!(i18n, race.play.give_up_confirm).to_string())
        />
    }
}

/// What the player sees once they're done and the others still race.
#[component]
fn Done(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        let solved = matches!(race.mine(), Some(RaceOutcome::Solved { .. }));
        view! {
            <div class="flex flex-col gap-1 rounded-xl border-2 border-primary/40 bg-primary/5 p-4" role="status" data-testid="race-done">
                <p class="text-lg font-black">
                    {if solved {
                        view! { <span class="font-pixel text-primary" aria-hidden="true">"完 "</span> {t!(i18n, race.done.solved)} }.into_any()
                    } else {
                        t!(i18n, race.done.gave_up).into_any()
                    }}
                </p>
                <p class="text-sm text-muted-foreground">{t!(i18n, race.done.waiting)}</p>
            </div>
        }
    }
}

/// The race loaded: what the phase and the viewer's part in it show.
#[component]
fn RaceBody(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let phase = Memo::new(move |_| race.phase());
    let joined = Memo::new(move |_| race.view.with(|v| v.as_ref().is_some_and(RaceView::joined)));
    let mine = Memo::new(move |_| race.mine());
    let playing_view = Memo::new(move |_| {
        matches!(phase.get(), Some(RacePhase::Countdown | RacePhase::Running)) && joined.get()
    });
    let stats = move || {
        (phase.get() == Some(RacePhase::Running) && joined.get()).then(|| {
            view! {
                <div class="flex gap-2">
                    <Stat
                        label=Signal::derive(move || t_string!(i18n, common.game.time).to_string())
                        jp="時間"
                        value=Signal::derive(move || format_duration(race.elapsed_ms()))
                    />
                    <Stat
                        label=Signal::derive(move || t_string!(i18n, daily.penalty).to_string())
                        jp="罰"
                        value=Signal::derive(move || format!("+{}s", race.penalty_ms() / 1000))
                    />
                </div>
            }
        })
    };
    view! {
        <div
            class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]"
            data-testid="race-page"
            data-phase=move || phase.get().map(|p| format!("{p:?}")).unwrap_or_default()
        >
            <div class="flex min-w-0 flex-col gap-4">
                <div class="flex min-h-16 flex-wrap items-end justify-between gap-3">
                    <Header race />
                    {stats}
                </div>
                {move || match phase.get() {
                    Some(RacePhase::Lobby) => view! { <Lobby race /> }.into_any(),
                    Some(RacePhase::Ended) => view! { <Results race /> }.into_any(),
                    _ if playing_view.get() => view! {
                        <Board race />
                        <Show when=move || mine.get() == Some(RaceOutcome::Racing) && phase.get() == Some(RacePhase::Running)>
                            <Show when=move || race.failed.get()>
                                <div role="status" class="flex flex-wrap items-center gap-3 rounded-lg border border-warning/40 bg-warning-light px-3 py-2 text-sm text-warning-dark">
                                    <span>{t!(i18n, kadai.page.submit_failed)}</span>
                                    <Button variant=ButtonVariant::Outline attr:disabled=move || race.busy.get() on:click=move |_| race.submit()>
                                        {t!(i18n, kadai.page.submit_retry)}
                                    </Button>
                                </div>
                            </Show>
                            <Controls race />
                        </Show>
                        <Show when=move || mine.get().is_some_and(RaceOutcome::done)>
                            <Done race />
                        </Show>
                    }.into_any(),
                    _ => view! {
                        <div class="flex flex-col items-center gap-3 rounded-xl border bg-card p-6 text-center" data-testid="race-watching">
                            <Show when=move || phase.get() == Some(RacePhase::Countdown)>
                                <CountdownStage race />
                            </Show>
                            <p class="text-sm text-muted-foreground">{t!(i18n, race.watch.text)}</p>
                        </div>
                    }.into_any(),
                }}
            </div>
            // On phones the results come before the replay
            <aside
                class="flex min-w-0 flex-col gap-4"
                class=("max-lg:order-first", move || phase.get() == Some(RacePhase::Ended))
            >
                <Show when=move || matches!(phase.get(), Some(RacePhase::Countdown | RacePhase::Running))>
                    <Tracks race />
                </Show>
                <Show when=move || phase.get() == Some(RacePhase::Ended)>
                    <results::Placings race />
                    <results::SharePuzzleCard race />
                </Show>
            </aside>
        </div>
    }
}

// Rendering on the server needs its features
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use common::Difficulty;

    use super::*;
    use crate::i18n::Locale;
    use crate::model::{HelpPenalties, RacePlayer};

    fn player(user_id: i64, name: &str, outcome: RaceOutcome) -> RacePlayer {
        RacePlayer {
            user_id,
            name: name.into(),
            guest: false,
            colour: u8::try_from(user_id).unwrap(),
            joined_at: user_id,
            progress: 40,
            away: user_id == 3,
            hints: 1,
            checks: 0,
            flag: None,
            outcome,
            solved_before: false,
        }
    }

    /// A race seen by aiko (1), the host: in the lobby with bram, or running with the time it
    /// started, or over.
    fn view(starts_at: Option<i64>, ended_at: Option<i64>) -> RaceView {
        let over = ended_at.is_some();
        RaceView {
            id: 5,
            group_id: 1,
            group_name: "Bridge Club".into(),
            me: 1,
            host: Some(1),
            difficulty: Difficulty::Medium,
            penalties: HelpPenalties::of(Difficulty::Medium),
            created_at: 0,
            starts_at,
            ended_at,
            players: vec![
                player(
                    1,
                    "aiko",
                    if over {
                        RaceOutcome::Solved {
                            time_ms: 75_000,
                            place: Some(1),
                        }
                    } else {
                        RaceOutcome::Racing
                    },
                ),
                player(
                    2,
                    "bram",
                    if over {
                        RaceOutcome::GaveUp
                    } else {
                        RaceOutcome::Racing
                    },
                ),
                player(
                    3,
                    "chen",
                    if over {
                        RaceOutcome::Out
                    } else {
                        RaceOutcome::Racing
                    },
                ),
            ],
            puzzle: None,
            solution: None,
            rematch: None,
            rematch_open: false,
            server_now: 10_000,
            shared: None,
        }
    }

    fn render(view: RaceView) -> String {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            provide_context(leptos_router::location::RequestUrl::new(
                "/groups/1/races/5",
            ));
            provide_context(crate::state::Rivals::new());
            let race = RaceState::new(Game::new("test.race"));
            race.now.set(10_000);
            race.show(view);
            view! {
                <leptos_router::components::Router>
                    <RaceBody race />
                </leptos_router::components::Router>
            }
            .to_html()
        });
        drop(owner);
        html
    }

    #[tokio::test]
    async fn every_phase_of_a_race_renders() {
        let lobby = render(view(None, None));
        for text in [
            "race-lobby",
            "Lobby",
            "aiko",
            "bram",
            "host",
            "away",
            "race-start",
            "Call it off",
            "Share the lobby link",
            "中級",
            "10×10",
        ] {
            assert!(lobby.contains(text), "{text}: {lobby}");
        }
        let counting = render(view(Some(15_000), None));
        for text in ["race-waiting", "race-countdown", "race-tracks", "用意"] {
            assert!(counting.contains(text), "{text}: {counting}");
        }
        let running = render(view(Some(5_000), None));
        for text in ["race-tracks", "Progress", "data-user=\"2\""] {
            assert!(running.contains(text), "{text}: {running}");
        }
        let over = render(view(Some(5_000), Some(9_000)));
        for text in ["race-results", "1:15.0", "gave up", "t finish", "Rematch"] {
            assert!(over.contains(text), "{text}: {over}");
        }
    }

    /// [`view`] on zoe's shared 9×9 puzzle, which bram solved before.
    fn shared(starts_at: Option<i64>, ended_at: Option<i64>) -> RaceView {
        let mut race = view(starts_at, ended_at);
        race.shared = Some(RaceShare {
            code: "k7m2xq9a".into(),
            sharer: Some(crate::model::SharedBy {
                user_id: 9,
                name: "zoe".into(),
                guest: false,
                stamp: None,
                result: None,
            }),
            width: 9,
            height: 9,
        });
        race.players[1].solved_before = true;
        race
    }

    #[tokio::test]
    async fn a_race_on_a_shared_puzzle_says_whose_it_is() {
        let lobby = render(shared(None, None));
        for text in [
            "Shared puzzle · by zoe",
            "9×9",
            "this shared puzzle",
            "race-solved-before",
            "solved before",
        ] {
            assert!(lobby.contains(text), "{text}: {lobby}");
        }
        // Its size, not the difficulty's preset
        assert!(
            !lobby.contains("10×10") && !lobby.contains("中級"),
            "{lobby}"
        );
        let over = render(shared(Some(5_000), Some(9_000)));
        for text in [
            "race-results",
            "race-solved-before",
            "race-share",
            "Share this puzzle",
        ] {
            assert!(over.contains(text), "{text}: {over}");
        }
        // A race of the generator: nobody solved it before; its puzzle can be shared too
        let plain = render(view(Some(5_000), Some(9_000)));
        assert!(!plain.contains("race-solved-before"), "{plain}");
        assert!(plain.contains("race-share"), "{plain}");
    }

    #[test]
    fn paths_and_texts() {
        assert_eq!(race_path(3, 9), "/groups/3/races/9");
        let mut share = RaceShare {
            code: "k7m2xq9a".into(),
            sharer: None,
            width: 7,
            height: 7,
        };
        assert_eq!(
            shared_by(Locale::en, &share),
            "Shared puzzle · by a former player"
        );
        share.sharer = Some(crate::model::SharedBy {
            user_id: 9,
            name: crate::model::BLOCKED_NAME.into(),
            guest: false,
            stamp: None,
            result: None,
        });
        assert_eq!(
            shared_by(Locale::en, &share),
            "Shared puzzle · by a blocked player"
        );
    }
}
