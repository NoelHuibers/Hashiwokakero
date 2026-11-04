//! The group page's card of races (勝負 shōbu).
//!
//! The open one to join or watch, a new one to open with a difficulty, and the last ones with their winners. Asks again every few seconds while
//! the tab is shown, so a lobby someone opens shows up.

use std::time::Duration;

use common::Difficulty;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::{join_group_race, load_group_races, open_group_race};
use crate::components::leaderboard::RankCell;
use crate::components::ui::button::{Button, ButtonSize};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::{date_short, difficulty_name, player_name, prefix};
use crate::model::{
    CARD_POLL_MS, GroupRaces, RacePhase, RaceSummary, colour_var, day_of, format_duration,
};
use crate::pages::race::race_path;
use crate::state::error_toast;

use super::GroupContext;

/// The players of a race as pips in their colours.
#[component]
fn Pips(race: RaceSummary) -> impl IntoView {
    view! {
        <div class="flex flex-wrap gap-1" aria-hidden="true">
            {race
                .entrants
                .iter()
                .map(|e| view! {
                    <span class="h-2 w-3 rounded-full" style=format!("background: {}", colour_var(e.colour))></span>
                })
                .collect_view()}
        </div>
    }
}

/// The open race: join it, go to its lobby, or watch it.
#[component]
fn OpenRace(race: RaceSummary, group: i64, now: i64) -> impl IntoView {
    let i18n = use_i18n();
    let navigate = use_navigate();
    let busy = RwSignal::new(false);
    let id = race.id;
    let href = move || format!("{}{}", prefix(i18n.get_locale()), race_path(group, id));
    let count = race.entrants.len();
    let lobby = race.phase(now) == RacePhase::Lobby;
    let join = move |_| {
        busy.set(true);
        let navigate = navigate.clone();
        spawn_local(async move {
            match join_group_race(id).await {
                Ok(_) => navigate(&href(), NavigateOptions::default()),
                Err(e) => error_toast(e),
            }
            busy.try_set(false);
        });
    };
    let action = match (lobby, race.joined) {
        (true, false) => view! {
            <Button size=ButtonSize::Sm attr:disabled=move || busy.get() attr:data-testid="race-card-join" on:click=join>
                {t!(i18n, race.card.join)} " →"
            </Button>
        }
        .into_any(),
        (true, true) => view! {
            <A href=href attr:class="inline-flex h-8 shrink-0 items-center rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground shadow-xs hover:bg-primary/90">
                {t!(i18n, race.card.to_lobby)} " →"
            </A>
        }
        .into_any(),
        (false, _) => view! {
            <A href=href attr:class="inline-flex h-8 shrink-0 items-center rounded-md border bg-background px-3 text-sm font-medium shadow-xs hover:bg-accent">
                {t!(i18n, race.card.watch)} " →"
            </A>
        }
        .into_any(),
    };
    let difficulty = race.difficulty;
    view! {
        <div class="flex items-center gap-3 rounded-xl border-2 border-primary/50 bg-primary/5 p-3" data-testid="race-open" data-race=id>
            <span class="hanko grid size-10 shrink-0 rotate-[-5deg] place-items-center rounded-md font-pixel text-lg" aria-hidden="true">
                "勝"
            </span>
            <div class="flex min-w-0 flex-1 flex-col gap-1">
                <p class="font-bold leading-tight">
                    {move || if lobby {
                        t_string!(i18n, race.card.lobby, count)
                    } else {
                        t_string!(i18n, race.card.running, count)
                    }}
                </p>
                <p class="truncate text-xs text-muted-foreground">
                    <span class="font-pixel" aria-hidden="true">{difficulty.kanji()} " "</span>
                    {move || difficulty_name(i18n.get_locale(), difficulty)}
                </p>
                <Pips race />
            </div>
            {action}
        </div>
    }
}

/// Opening a race: the difficulty, and the button.
#[component]
fn NewRace(group: i64) -> impl IntoView {
    let i18n = use_i18n();
    let navigate = use_navigate();
    let difficulty = RwSignal::new(Difficulty::Easy);
    let busy = RwSignal::new(false);
    let open = move |_| {
        busy.set(true);
        let navigate = navigate.clone();
        spawn_local(async move {
            match open_group_race(group, difficulty.get_untracked()).await {
                Ok(id) => navigate(
                    &format!(
                        "{}{}",
                        prefix(i18n.get_locale_untracked()),
                        race_path(group, id)
                    ),
                    NavigateOptions::default(),
                ),
                Err(e) => error_toast(e),
            }
            busy.try_set(false);
        });
    };
    view! {
        <div class="flex flex-col gap-2" data-testid="race-new">
            <div class="grid grid-cols-5 gap-1" role="group" aria-label=move || t_string!(i18n, race.card.difficulty)>
                {Difficulty::ALL
                    .into_iter()
                    .map(|d| {
                        let (w, h) = d.size();
                        view! {
                            <button
                                type="button"
                                aria-pressed=move || (difficulty.get() == d).to_string()
                                aria-label=move || difficulty_name(i18n.get_locale(), d)
                                data-tooltip=move || format!("{} · {w}×{h}", difficulty_name(i18n.get_locale(), d))
                                class=move || format!(
                                    "flex min-w-0 flex-col items-center rounded-lg border px-0.5 py-1.5 text-center {}",
                                    if difficulty.get() == d {
                                        "border-primary bg-primary text-primary-foreground shadow-md"
                                    } else {
                                        "bg-background hover:border-primary/50"
                                    },
                                )
                                on:click=move |_| difficulty.set(d)
                            >
                                <span class="font-pixel text-base leading-none" aria-hidden="true">{d.kanji()}</span>
                                <span class="text-[10px] opacity-70" aria-hidden="true">{format!("{w}×{h}")}</span>
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <Button class="h-10" attr:disabled=move || busy.get() attr:data-testid="race-open-new" on:click=open>
                {move || if busy.get() {
                    t_string!(i18n, race.card.opening).to_string()
                } else {
                    format!("{} 勝負", t_string!(i18n, race.card.open))
                }}
            </Button>
        </div>
    }
}

/// A race that is over: when, how many, and who won.
#[component]
fn PastRace(race: RaceSummary, group: i64) -> impl IntoView {
    let i18n = use_i18n();
    let id = race.id;
    let href = move || format!("{}{}", prefix(i18n.get_locale()), race_path(group, id));
    let day = day_of(race.starts_at.unwrap_or(race.created_at));
    let count = race.entrants.len();
    let difficulty = race.difficulty;
    let winner = race.winner().cloned();
    view! {
        <li>
            <A href=href attr:class="flex items-center gap-3 rounded-lg p-2 transition-colors hover:bg-accent/40" attr:data-past=id>
                <span class="font-pixel w-8 shrink-0 text-center text-primary" aria-hidden="true">{difficulty.kanji()}</span>
                <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                    <p class="truncate text-xs text-muted-foreground">
                        {move || format!(
                            "{} · {} · {}",
                            date_short(i18n.get_locale(), day),
                            difficulty_name(i18n.get_locale(), difficulty),
                            t_string!(i18n, race.card.players, count),
                        )}
                    </p>
                    {match winner {
                        Some(w) => {
                            let name = w.name.clone();
                            view! {
                                <span class="flex min-w-0 items-center gap-1.5 text-sm">
                                    <RankCell rank=1 />
                                    <span class="truncate font-medium">{move || player_name(i18n.get_locale(), &name)}</span>
                                    <span class="font-pixel tabular-nums text-muted-foreground">
                                        {w.time_ms.map(format_duration)}
                                    </span>
                                </span>
                            }
                            .into_any()
                        }
                        None => view! { <span class="text-sm text-muted-foreground">{t!(i18n, race.card.no_winner)}</span> }.into_any(),
                    }}
                </div>
            </A>
        </li>
    }
}

/// The card on the group page.
#[component]
pub fn RaceCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    let races = RwSignal::new(None::<GroupRaces>);
    let id = Memo::new(move |_| group.page.with(|p| p.as_ref().map(|p| p.id)));
    let fetch = move || {
        let Some(id) = id.get_untracked() else { return };
        spawn_local(async move {
            match load_group_races(id).await {
                Ok(list) => {
                    races.try_set(Some(list));
                }
                Err(e) => error_toast(e),
            }
        });
    };
    Effect::new(move |_| {
        id.track();
        untrack(fetch);
    });
    // Again every few seconds while the tab is shown (browser only)
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        if let Ok(handle) = set_interval_with_handle(
            move || {
                if document().visibility_state() == web_sys::VisibilityState::Visible {
                    fetch();
                }
            },
            Duration::from_millis(u64::from(CARD_POLL_MS)),
        ) {
            on_cleanup(move || handle.clear());
        }
    });
    view! {
        <Card class="gap-3" attr:data-testid="race-card">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, race.title)} <span class="font-pixel text-sm text-primary">"勝負"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, race.card.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                {move || match (races.get(), id.get()) {
                    (Some(list), Some(group)) => {
                        let now = list.server_now;
                        let open = match list.open {
                            Some(race) => view! { <OpenRace race group now /> }.into_any(),
                            None => view! { <NewRace group /> }.into_any(),
                        };
                        let history = if list.recent.is_empty() {
                            view! { <p class="text-sm text-muted-foreground">{t!(i18n, race.card.none)}</p> }.into_any()
                        } else {
                            view! {
                                <div class="flex flex-col gap-1" data-testid="race-history">
                                    <p class="flex items-center gap-2 text-sm font-medium text-muted-foreground">
                                        <span class="font-pixel text-primary" aria-hidden="true">"歴"</span>
                                        {t!(i18n, race.card.history)}
                                    </p>
                                    <ul class="flex flex-col gap-1">
                                        {list.recent.into_iter().map(|r| view! { <PastRace race=r group /> }).collect_view()}
                                    </ul>
                                </div>
                            }
                            .into_any()
                        };
                        view! { {open} {history} }.into_any()
                    }
                    _ => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                }}
            </CardContent>
        </Card>
    }
}
