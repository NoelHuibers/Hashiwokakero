//! A race that is over: the places with times and penalties, everyone's moves in the replay
//! theatre, and the rematch.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use crate::api::{load_race_replay, rematch_group_race};
use crate::components::leaderboard::{HIGHLIGHT_ROW, PlayerName, RANK_TABLE, ROW_INSET, RankCell};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::prefix;
use crate::model::{RaceOutcome, RacePlayer, ReplayData, format_duration};
use crate::pages::groups::today::ColourPip;
use crate::pages::replay::TheatreView;
use crate::state::error_toast;

use super::race_path;
use super::state::RaceState;

/// The order of the results: places first, then the finishes without one, then the rest.
fn order(players: &mut [RacePlayer]) {
    players.sort_by_key(|p| match p.outcome {
        RaceOutcome::Solved {
            place: Some(place), ..
        } => (0, i64::from(place)),
        RaceOutcome::Solved { time_ms, .. } => (1, time_ms),
        RaceOutcome::GaveUp => (2, 0),
        RaceOutcome::Out | RaceOutcome::Racing => (3, 0),
    });
}

#[component]
fn Row(player: RacePlayer, me: bool) -> impl IntoView {
    let i18n = use_i18n();
    let first = match player.outcome {
        RaceOutcome::Solved {
            place: Some(place), ..
        } => view! { <RankCell rank=i64::from(place) /> }.into_any(),
        _ => view! {
            <span class="grid size-7 place-items-center font-pixel text-muted-foreground" aria-hidden="true">"–"</span>
        }
        .into_any(),
    };
    let (hints, checks) = (player.hints, player.checks);
    let penalties = (hints + checks > 0).then(|| format!("{hints}h {checks}c"));
    let has_penalties = penalties.is_some();
    let end = match player.outcome {
        RaceOutcome::Solved { time_ms, place } => view! {
            <span class="flex flex-col items-end">
                <span class="font-pixel tabular-nums" class:text-primary=me>{format_duration(time_ms)}</span>
                {place.is_none().then(|| view! {
                    <span class="text-xs text-warning-dark">{t!(i18n, race.results.flagged)}</span>
                })}
            </span>
        }
        .into_any(),
        RaceOutcome::GaveUp => view! { <span class="text-xs text-muted-foreground">{t!(i18n, race.results.gave_up)}</span> }.into_any(),
        RaceOutcome::Out | RaceOutcome::Racing => view! {
            <span class="text-xs italic text-muted-foreground">{t!(i18n, race.results.out)}</span>
        }
        .into_any(),
    };
    let state = match player.outcome {
        RaceOutcome::Solved { place: Some(_), .. } => "placed",
        RaceOutcome::Solved { .. } => "flagged",
        RaceOutcome::GaveUp => "gave-up",
        RaceOutcome::Out | RaceOutcome::Racing => "out",
    };
    view! {
        <tr class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" }) data-user=player.user_id data-state=state>
            <td class="py-2 pe-2">{first}</td>
            <td class="max-w-0 py-2 pe-2">
                <div class="flex min-w-0 items-center gap-2 [&>div]:min-w-0">
                    <ColourPip colour=player.colour />
                    <PlayerName user_id=player.user_id name=player.name guest=player.guest stamp=None me />
                </div>
            </td>
            <td
                class="py-2 pe-2 text-end text-xs text-muted-foreground"
                data-tooltip=move || has_penalties.then(|| t_string!(i18n, common.ranking.penalties))
            >
                {penalties}
            </td>
            <td class="py-2 text-end">{end}</td>
        </tr>
    }
}

/// The places, next to the replay.
#[component]
pub fn Placings(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let rows = Memo::new(move |_| {
        race.view.with(|v| {
            v.as_ref().map(|v| {
                let mut players = v.players.clone();
                order(&mut players);
                (players, v.me)
            })
        })
    });
    view! {
        <Card class="gap-3" attr:data-testid="race-results">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, race.results.title)} <span class="font-pixel text-sm text-primary">"結果"</span>
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || rows.get().map(|(players, me)| view! {
                    <div class="@container -mx-2">
                        <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                            <colgroup>
                                <col class="w-10" />
                                <col />
                                <col class="w-12" />
                                <col class="w-20" />
                            </colgroup>
                            <tbody>
                                {players
                                    .into_iter()
                                    .map(|p| {
                                        let mine = p.user_id == me;
                                        view! { <Row player=p me=mine /> }
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    </div>
                })}
                <Rematch race />
            </CardContent>
        </Card>
    }
}

/// The rematch: opens a new lobby with the same settings, or joins the one somebody opened.
#[component]
fn Rematch(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let navigate = use_navigate();
    let next = Memo::new(move |_| {
        race.view
            .with(|v| v.as_ref().map(|v| (v.rematch, v.rematch_open, v.group_id)))
    });
    let go = move |_| {
        let Some((id, group)) = race
            .view
            .with_untracked(|v| v.as_ref().map(|v| (v.id, v.group_id)))
        else {
            return;
        };
        let navigate = navigate.clone();
        race.busy.set(true);
        spawn_local(async move {
            let reply = rematch_group_race(id).await;
            if race.busy.try_set(false).is_some() {
                return;
            }
            match reply {
                Ok(next) => navigate(
                    &format!(
                        "{}{}",
                        prefix(i18n.get_locale_untracked()),
                        race_path(group, next)
                    ),
                    NavigateOptions::default(),
                ),
                Err(e) => error_toast(e),
            }
        });
    };
    move || {
        let (rematch, open, group) = next.get()?;
        Some(match rematch {
            // Somebody opened it and it started: there it is
            Some(next) if !open => {
                let href = move || format!("{}{}", prefix(i18n.get_locale()), race_path(group, next));
                view! {
                    <leptos_router::components::A
                        href=href
                        attr:class="inline-flex h-11 items-center justify-center rounded-md border bg-background px-4 text-sm font-medium shadow-xs hover:bg-accent"
                        attr:data-testid="race-rematch-go"
                    >
                        {t!(i18n, race.results.rematch_go)} " →"
                    </leptos_router::components::A>
                }
                .into_any()
            }
            Some(_) => view! {
                <Button class="h-11 w-full" attr:disabled=move || race.busy.get() attr:data-testid="race-rematch-join" on:click=go.clone()>
                    <span class="font-pixel" aria-hidden="true">"再戦 "</span>{t!(i18n, race.results.rematch_join)}
                </Button>
            }
            .into_any(),
            None => view! {
                <Button
                    variant=ButtonVariant::Outline
                    class="h-11 w-full"
                    attr:disabled=move || race.busy.get()
                    attr:data-testid="race-rematch"
                    on:click=go.clone()
                >
                    <span class="font-pixel" aria-hidden="true">"再戦 "</span>{t!(i18n, race.results.rematch)}
                </Button>
            }
            .into_any(),
        })
    }
}

/// Everyone's moves in the replay theatre, loaded once the race is over.
#[component]
pub fn Results(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let replay = RwSignal::new(None::<ReplayData>);
    let id = Memo::new(move |_| race.view.with(|v| v.as_ref().map(|v| v.id)));
    Effect::new(move |_| {
        let Some(id) = id.get() else { return };
        spawn_local(async move {
            match load_race_replay(id).await {
                Ok(data) => {
                    replay.try_set(Some(data));
                }
                Err(e) => error_toast(e),
            }
        });
    });
    view! {
        <section class="flex flex-col gap-3" data-testid="race-replay">
            <h2 class="flex items-baseline gap-2 text-xl font-black">
                {t!(i18n, race.results.replay)} <span class="font-pixel text-sm text-primary">"再生"</span>
            </h2>
            {move || match replay.get() {
                None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                Some(data) => view! { <TheatreView data only=None /> }.into_any(),
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(user_id: i64, outcome: RaceOutcome) -> RacePlayer {
        RacePlayer {
            user_id,
            name: String::new(),
            guest: false,
            colour: 0,
            joined_at: 0,
            progress: 0,
            away: false,
            hints: 0,
            checks: 0,
            flag: None,
            outcome,
        }
    }

    #[test]
    fn places_first_then_flagged_then_the_rest() {
        let solved = |time_ms, place| RaceOutcome::Solved { time_ms, place };
        let mut players = vec![
            player(1, RaceOutcome::Out),
            player(2, solved(90, Some(2))),
            player(3, RaceOutcome::GaveUp),
            player(4, solved(10, None)),
            player(5, solved(60, Some(1))),
        ];
        order(&mut players);
        let ids: Vec<_> = players.iter().map(|p| p.user_id).collect();
        assert_eq!(ids, [5, 2, 4, 3, 1]);
    }
}
