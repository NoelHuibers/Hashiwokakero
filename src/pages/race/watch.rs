//! Watching the others (観戦 kansen): the boards of the ones still racing, live.
//!
//! A player who is done (solved or gave up) sees a little board of everyone else in their colour
//! while the race runs. The server sends the boards only to them (`RaceView::boards`).

use common::Board;
use leptos::prelude::*;

use crate::components::sprites::BoardDefs;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::player_name;
use crate::model::{RaceOutcome, RacePlayer, RaceView, format_duration};
use crate::pages::replay::grid::MiniCard;

use super::state::RaceState;
use super::track::state_id;

/// Where a player comes among the watched: the ones racing first, then the ones away, the
/// finishes by place and time, and the ones who gave up; the first to join first.
fn rank(player: &RacePlayer) -> (u8, i64, i64) {
    let (group, by) = match player.outcome {
        RaceOutcome::Racing if player.away => (1, 0),
        RaceOutcome::Racing => (0, 0),
        RaceOutcome::Solved {
            place: Some(place), ..
        } => (2, i64::from(place)),
        RaceOutcome::Solved { time_ms, .. } => (3, time_ms),
        RaceOutcome::GaveUp => (4, 0),
        RaceOutcome::Out => (5, 0),
    };
    (group, by, player.joined_at)
}

/// The players the viewer watches, in the order of [`rank`]: everyone else who joined.
fn watched(view: &RaceView) -> Vec<i64> {
    let mut others: Vec<&RacePlayer> = view
        .players
        .iter()
        .filter(|p| p.user_id != view.me)
        .collect();
    others.sort_by_key(|p| rank(p));
    others.iter().map(|p| p.user_id).collect()
}

/// A player's board as they last reported it, all empty before they did (or if it doesn't fit
/// the puzzle's `edges`).
fn board_of(view: &RaceView, user: i64, edges: usize) -> Vec<u8> {
    view.boards
        .iter()
        .find(|b| b.user_id == user)
        .map(|b| b.bridges.clone())
        .filter(|b| b.len() == edges)
        .unwrap_or_else(|| vec![0; edges])
}

/// What a card says at the end of the name: how far they are, their time and place, or that they
/// gave up or are away.
#[component]
fn Standing(player: Memo<Option<RacePlayer>>) -> impl IntoView {
    let i18n = use_i18n();
    move || {
        let p = player.get()?;
        Some(match p.outcome {
            RaceOutcome::Solved { time_ms, place } => view! {
                <span class="inline-flex items-center gap-1 text-success-dark">
                    <span aria-hidden="true">"完"</span>
                    {format_duration(time_ms)}
                    {place.map(|place| view! {
                        <span class="font-sans text-foreground">{move || td_string!(i18n.get_locale(), race.watch.place, place)}</span>
                    })}
                </span>
            }
            .into_any(),
            RaceOutcome::GaveUp => view! { <span class="font-sans">{t!(i18n, race.results.gave_up)}</span> }.into_any(),
            RaceOutcome::Out => view! { <span class="font-sans">{t!(i18n, race.results.out)}</span> }.into_any(),
            RaceOutcome::Racing if p.away => view! { <span class="font-sans italic">{t!(i18n, race.lobby.away)}</span> }.into_any(),
            RaceOutcome::Racing => view! { <span class="text-foreground">{format!("{}%", p.progress)}</span> }.into_any(),
        })
    }
}

/// One watched player's little board.
#[component]
fn Watched(race: RaceState, board: StoredValue<Board>, user: i64) -> impl IntoView {
    let i18n = use_i18n();
    // Memos: the poll sets the view every second, the card only changes with what it shows
    let player = Memo::new(move |_| {
        race.view.with(|v| {
            v.as_ref()
                .and_then(|v| v.players.iter().find(|p| p.user_id == user).cloned())
        })
    });
    let edges = board.with_value(|b| b.edges.len());
    let bridges = Memo::new(move |_| {
        race.view.with(|v| {
            v.as_ref()
                .map_or_else(|| vec![0; edges], |v| board_of(v, user, edges))
        })
    });
    let Some(first) = player.get_untracked() else {
        return ().into_any();
    };
    let percent =
        Signal::derive(move || player.with(|p| p.as_ref().map_or(0, |p| u32::from(p.progress))));
    let state = move || player.with(|p| p.as_ref().map_or("racing", state_id));
    let name = first.name.clone();
    let label = Signal::derive(move || {
        td_string!(
            i18n.get_locale(),
            replay.legend.progress,
            name = player_name(i18n.get_locale(), &name),
            percent = percent.get()
        )
    });
    view! {
        <MiniCard
            board
            bridges
            colour=first.colour
            name=player_name(i18n.get_locale_untracked(), &first.name)
            label
            percent
            attr:data-user=user
            attr:data-state=state
        >
            <Standing player />
        </MiniCard>
    }
    .into_any()
}

/// The others' boards for a player who is done, while the race runs. Before their own board,
/// which is no longer what they look at.
#[component]
pub fn Watch(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let puzzle = Memo::new(move |_| race.puzzle());
    let others = Memo::new(move |_| race.view.with(|v| v.as_ref().map_or_default(watched)));
    // One other player: their board as big as the player's own
    let grid = move || {
        if others.with(Vec::len) == 1 {
            "grid grid-cols-1 gap-2 sm:grid-cols-2 sm:gap-3"
        } else {
            "grid grid-cols-2 gap-2 sm:grid-cols-3 sm:gap-3"
        }
    };
    move || {
        let board = StoredValue::new(Board::new(puzzle.get()?));
        Some(view! {
            <section class="-order-1 flex flex-col gap-3" data-testid="race-watch" aria-labelledby="race-watch-title">
                // The sea, islands and bridges the little boards use
                <svg class="absolute size-0" aria-hidden="true">
                    <BoardDefs />
                </svg>
                <div>
                    <h2 id="race-watch-title" class="flex items-baseline gap-2 text-xl font-black">
                        {t!(i18n, race.watch.others)} <span class="font-pixel text-sm text-primary" aria-hidden="true">"観戦"</span>
                    </h2>
                    <p class="text-sm text-muted-foreground">{t!(i18n, race.watch.others_text)}</p>
                </div>
                <ul class=grid aria-label=move || t_string!(i18n, race.watch.others)>
                    <For each=move || others.get() key=|user| *user let:user>
                        <Watched race board user />
                    </For>
                </ul>
            </section>
        })
    }
}

#[cfg(test)]
mod tests {
    use common::Difficulty;

    use super::*;
    use crate::model::{HelpPenalties, RaceBoard};

    fn player(user_id: i64, outcome: RaceOutcome, away: bool) -> RacePlayer {
        RacePlayer {
            user_id,
            name: format!("p{user_id}"),
            guest: false,
            colour: 0,
            joined_at: user_id,
            progress: 0,
            away,
            hints: 0,
            checks: 0,
            flag: None,
            outcome,
            solved_before: false,
        }
    }

    fn view(players: Vec<RacePlayer>) -> RaceView {
        RaceView {
            id: 1,
            group_id: 1,
            group_name: "Club".into(),
            me: 1,
            host: Some(1),
            difficulty: Difficulty::Easy,
            penalties: HelpPenalties::default(),
            created_at: 0,
            starts_at: Some(0),
            ended_at: None,
            players,
            puzzle: None,
            solution: None,
            rematch: None,
            rematch_open: false,
            server_now: 1,
            shared: None,
            boards: vec![
                RaceBoard {
                    user_id: 2,
                    bridges: vec![1, 2],
                },
                RaceBoard {
                    user_id: 3,
                    bridges: vec![1],
                },
            ],
        }
    }

    #[test]
    fn the_ones_racing_come_first_then_the_finishes() {
        let solved = |time_ms, place| RaceOutcome::Solved { time_ms, place };
        let race = view(vec![
            player(1, solved(9_000, Some(1)), false),
            player(2, RaceOutcome::GaveUp, false),
            player(3, solved(20_000, Some(3)), false),
            player(4, solved(1_000, None), false),
            player(5, RaceOutcome::Racing, true),
            player(6, solved(12_000, Some(2)), false),
            player(7, RaceOutcome::Racing, false),
            player(8, RaceOutcome::Racing, false),
        ]);
        // Without the viewer (1)
        assert_eq!(watched(&race), [7, 8, 5, 6, 3, 4, 2]);
    }

    #[test]
    fn a_board_that_does_not_fit_shows_empty() {
        let race = view(vec![]);
        assert_eq!(board_of(&race, 2, 2), [1, 2]);
        // Of another length than the puzzle's edges
        assert_eq!(board_of(&race, 3, 2), [0, 0]);
        // Nothing reported yet
        assert_eq!(board_of(&race, 4, 3), [0, 0, 0]);
    }
}
