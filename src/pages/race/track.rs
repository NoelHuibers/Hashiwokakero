//! Everyone's progress during the race: a bar per player in their colour, the share of the
//! solution's bridges on their board. Never the bridges themselves.

use leptos::prelude::*;

use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::lang::player_name;
use crate::model::{RaceOutcome, RacePlayer, colour_var, format_duration};
use crate::pages::groups::today::ColourPip;

use super::state::RaceState;

/// What a bar says at its end.
fn state_id(player: &RacePlayer) -> &'static str {
    match player.outcome {
        RaceOutcome::Solved { .. } => "solved",
        RaceOutcome::GaveUp => "gave-up",
        RaceOutcome::Out => "out",
        RaceOutcome::Racing if player.away => "away",
        RaceOutcome::Racing => "racing",
    }
}

#[component]
fn Track(player: RacePlayer, me: bool, #[prop(into)] progress: Signal<u8>) -> impl IntoView {
    let i18n = use_i18n();
    let colour = colour_var(player.colour);
    let name = player.name.clone();
    let shown = player.name.clone();
    let state = state_id(&player);
    let end = match player.outcome {
        RaceOutcome::Solved { time_ms, .. } => view! {
            <span class="font-pixel text-xs tabular-nums text-success-dark">
                <span aria-hidden="true">"完 "</span>{format_duration(time_ms)}
            </span>
        }
        .into_any(),
        RaceOutcome::GaveUp => view! { <span class="text-xs text-muted-foreground">{t!(i18n, race.results.gave_up)}</span> }.into_any(),
        RaceOutcome::Racing if player.away => view! {
            <span class="text-xs italic text-muted-foreground">{t!(i18n, race.lobby.away)}</span>
        }
        .into_any(),
        RaceOutcome::Racing | RaceOutcome::Out => view! {
            <span class="font-pixel text-xs tabular-nums">{move || format!("{}%", progress.get())}</span>
        }
        .into_any(),
    };
    let label = move || {
        format!(
            "{}: {}%",
            player_name(i18n.get_locale(), &name),
            progress.get()
        )
    };
    view! {
        <li class="flex flex-col gap-1" data-user=player.user_id data-state=state class:opacity-60=player.away>
            <div class="flex items-center justify-between gap-2 text-sm">
                <span class="flex min-w-0 items-center gap-2">
                    <ColourPip colour=player.colour />
                    <span class="truncate font-medium">{move || player_name(i18n.get_locale(), &shown)}</span>
                    {me.then(|| view! { <span class="shrink-0 text-xs text-muted-foreground">"(" {t!(i18n, race.track.you)} ")"</span> })}
                </span>
                {end}
            </div>
            <div
                class="h-2.5 w-full overflow-hidden rounded-full bg-secondary"
                role="progressbar"
                aria-valuemin="0"
                aria-valuemax="100"
                aria-valuenow=move || progress.get().to_string()
                aria-label=label
            >
                <div
                    class="h-full rounded-full transition-[width] duration-500 ease-out"
                    style=move || format!("width: {}%; background: {colour}", progress.get())
                ></div>
            </div>
        </li>
    }
}

/// The progress card next to the board.
#[component]
pub fn Tracks(race: RaceState) -> impl IntoView {
    let i18n = use_i18n();
    let players = Memo::new(move |_| {
        race.view
            .with(|v| v.as_ref().map(|v| (v.players.clone(), v.me)))
    });
    view! {
        <Card class="gap-3" attr:data-testid="race-tracks">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, race.track.title)} <span class="font-pixel text-sm text-primary">"進度"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, race.track.text)}</CardDescription>
            </CardHeader>
            <CardContent>
                <ul class="flex flex-col gap-3">
                    {move || players.get().map(|(players, me)| {
                        players
                            .into_iter()
                            .map(|p| {
                                let mine = p.user_id == me;
                                let reported = p.progress;
                                // The player's own bar follows their board, not the last request
                                let progress = Signal::derive(move || {
                                    if mine && race.racing() {
                                        race.progress().max(reported)
                                    } else {
                                        reported
                                    }
                                });
                                view! { <Track player=p me=mine progress /> }
                            })
                            .collect_view()
                    })}
                </ul>
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_says_how_its_player_stands() {
        let mut p = RacePlayer {
            user_id: 1,
            name: "aiko".into(),
            guest: false,
            colour: 0,
            joined_at: 0,
            progress: 0,
            away: false,
            hints: 0,
            checks: 0,
            flag: None,
            outcome: RaceOutcome::Racing,
        };
        assert_eq!(state_id(&p), "racing");
        p.away = true;
        assert_eq!(state_id(&p), "away");
        p.outcome = RaceOutcome::GaveUp;
        assert_eq!(state_id(&p), "gave-up");
        p.outcome = RaceOutcome::Solved {
            time_ms: 1,
            place: None,
        };
        assert_eq!(state_id(&p), "solved");
        p.outcome = RaceOutcome::Out;
        assert_eq!(state_id(&p), "out");
    }
}
