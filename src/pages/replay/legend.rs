//! Who's in the replay: a toggle per member with their progress, and the finish line, which
//! fills in as they cross it ("1 aiko 3:12").

use leptos::prelude::*;

use crate::components::leaderboard::RankCell;
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::model::{LaneOutcome, colour_var, format_duration, progress};

use super::state::Theatre;

/// A member's row: the toggle, how far they are, where they finished.
#[component]
fn Row(theatre: Theatre, lane: usize) -> impl IntoView {
    let i18n = use_i18n();
    let Some(info) = theatre.lane(lane) else {
        return ().into_any();
    };
    let me = info.user_id == theatre.data.with_value(|d| d.me);
    let colour = colour_var(info.colour);
    let playable = theatre.playable(lane);
    let shown = move || theatre.shown(lane);
    let percent = Memo::new(move |_| {
        theatre.boards.with(|b| {
            b.get(lane)
                .map_or(0, |b| progress(&b.bridges, theatre.total))
        })
    });
    let place = Memo::new(move |_| {
        theatre
            .finished
            .with(|f| f.iter().find(|f| f.lane == lane).map(|f| (f.place, f.at)))
    });
    let place = move || place.get();
    let outcome = info.outcome;
    let status = move || {
        match outcome {
        LaneOutcome::GaveUp => view! { <span class="text-xs text-muted-foreground">{t!(i18n, replay.legend.gave_up)}</span> }.into_any(),
        LaneOutcome::Missing => view! { <span class="text-xs italic text-muted-foreground">{t!(i18n, replay.legend.missing)}</span> }.into_any(),
        LaneOutcome::Solved { .. } if !playable => {
            view! { <span class="text-xs text-muted-foreground">{t!(i18n, replay.legend.no_log)}</span> }.into_any()
        }
        LaneOutcome::Solved { .. } => match place() {
            Some((_, at)) => view! {
                <span class="font-pixel text-sm tabular-nums">{format_duration(i64::from(at))}</span>
            }
                .into_any(),
            None => view! {
                <span class="font-pixel text-xs tabular-nums text-muted-foreground">{move || format!("{}%", percent.get())}</span>
            }
                .into_any(),
        },
    }
    };
    let penalties = match outcome {
        LaneOutcome::Solved { hints, checks, .. } if hints + checks > 0 => {
            Some(format!("{hints}h {checks}c"))
        }
        _ => None,
    };
    let unranked = matches!(outcome, LaneOutcome::Solved { ranked: false, .. });
    let name = info.name.clone();
    let toggle_label = move || {
        let l = i18n.get_locale();
        if shown() {
            td_string!(l, replay.legend.hide, name = name.clone())
        } else {
            td_string!(l, replay.legend.show, name = name.clone())
        }
    };
    let pip_style = {
        let colour = colour.clone();
        move || {
            if shown() {
                format!("background: {colour}")
            } else {
                format!("box-shadow: inset 0 0 0 2px {colour}")
            }
        }
    };
    view! {
        <li class="flex flex-col" data-lane=info.user_id data-shown=move || shown().to_string()>
            <button
                type="button"
                class="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-start text-sm transition-colors enabled:hover:bg-muted disabled:cursor-default"
                class:opacity-55=move || playable && !shown()
                aria-pressed=move || playable.then(|| shown().to_string())
                aria-label=move || playable.then(toggle_label.clone())
                disabled=!playable
                on:click=move |_| theatre.toggle(lane)
            >
                <span class="grid w-7 shrink-0 place-items-center">
                    {move || match place() {
                        Some((place, _)) => view! { <RankCell rank=i64::try_from(place).unwrap_or(0) /> }.into_any(),
                        None => view! { <span class="size-3 rounded-[4px] transition-all" style=pip_style.clone()></span> }.into_any(),
                    }}
                </span>
                <span class="min-w-0 flex-1 truncate" class:font-bold=me>{info.name.clone()}</span>
                {me.then(|| view! { <span class="shrink-0 rounded bg-primary px-1.5 text-[10px] font-medium text-primary-foreground">{t!(i18n, common.ranking.you)}</span> })}
                {unranked.then(|| view! { <span class="shrink-0 text-[10px] text-muted-foreground" data-tooltip=move || t_string!(i18n, replay.legend.unranked_hint)>{t!(i18n, replay.legend.unranked)}</span> })}
                {penalties.map(|p| view! { <span class="shrink-0 text-xs text-muted-foreground" data-tooltip=move || t_string!(i18n, common.ranking.penalties)>{p}</span> })}
                <span class="shrink-0 text-end">{status}</span>
            </button>
            {playable.then(|| view! {
                <div class="mx-2 mb-1 h-1 overflow-hidden rounded-full bg-muted" aria-hidden="true">
                    <div
                        class="h-full rounded-full"
                        style=move || format!("width: {}%; background: {colour}", percent.get())
                    ></div>
                </div>
            })}
        </li>
    }
    .into_any()
}

/// The members of the replay, a toggle each, and "everyone" / "just me".
#[component]
pub fn Legend(theatre: Theatre) -> impl IntoView {
    let i18n = use_i18n();
    let me = theatre.data.with_value(|d| d.me);
    let several = (0..theatre.lanes())
        .filter(|&i| theatre.playable(i))
        .count()
        > 1;
    let all_shown = move || (0..theatre.lanes()).all(|i| !theatre.playable(i) || theatre.shown(i));
    let only_me = move || {
        theatre
            .shown_lanes()
            .iter()
            .all(|&i| theatre.lane(i).is_some_and(|l| l.user_id == me))
    };
    let chip = "rounded-full border px-2.5 py-0.5 text-xs font-medium transition-colors hover:bg-accent aria-pressed:border-primary aria-pressed:bg-primary/10 aria-pressed:text-primary";
    view! {
        <Card class="gap-3" attr:data-testid="replay-legend">
            <CardHeader>
                <div class="flex w-full flex-wrap items-center justify-between gap-2">
                    <CardTitle class="flex items-baseline gap-2 text-lg">
                        {t!(i18n, replay.legend.title)} <span class="font-pixel text-sm text-primary">"仲間"</span>
                    </CardTitle>
                    {several.then(|| view! {
                        <div class="flex gap-1.5">
                            <button type="button" class=chip aria-pressed=move || all_shown().to_string() on:click=move |_| theatre.show_only(None)>
                                {t!(i18n, replay.legend.everyone)}
                            </button>
                            {theatre.data.with_value(|d| d.lane(me).is_some_and(|l| !l.log.is_empty())).then(|| view! {
                                <button type="button" class=chip aria-pressed=move || only_me().to_string() on:click=move |_| theatre.show_only(Some(&[me]))>
                                    {t!(i18n, replay.legend.just_me)}
                                </button>
                            })}
                        </div>
                    })}
                </div>
            </CardHeader>
            <CardContent>
                <ul class="-mx-2 flex flex-col">
                    {(0..theatre.lanes()).map(|lane| view! { <Row theatre lane /> }).collect_view()}
                </ul>
            </CardContent>
        </Card>
    }
}

/// The finish line: who's across, in the order they crossed, with the time they did.
#[component]
pub fn FinishLine(theatre: Theatre) -> impl IntoView {
    let i18n = use_i18n();
    let finishers = theatre.finished;
    let me = theatre.data.with_value(|d| d.me);
    view! {
        <Card class="gap-3" attr:data-testid="replay-finish">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-lg">
                    {t!(i18n, replay.finish.title)} <span class="font-pixel text-sm text-primary">"決勝"</span>
                </CardTitle>
            </CardHeader>
            <CardContent>
                {move || {
                    let list = finishers.get();
                    if list.is_empty() {
                        return view! {
                            <p class="flex items-center gap-2 text-sm text-muted-foreground">
                                <span class="font-pixel text-sea" aria-hidden="true">"〜"</span>
                                {t!(i18n, replay.finish.none)}
                            </p>
                        }
                            .into_any();
                    }
                    view! {
                        <ol class="flex flex-col gap-1" aria-live="polite">
                            {list
                                .into_iter()
                                .filter_map(|f| theatre.lane(f.lane).map(|l| (f, l)))
                                .map(|(f, lane)| {
                                    let official = match lane.outcome {
                                        LaneOutcome::Solved { time_ms, hints, checks, .. } if hints + checks > 0 => Some(time_ms),
                                        _ => None,
                                    };
                                    let mine = lane.user_id == me;
                                    view! {
                                        <li
                                            class="flex min-w-0 items-center gap-2 rounded-md px-1 py-1 text-sm"
                                            class=(["bg-primary/10"], mine)
                                            data-place=f.place
                                        >
                                            <RankCell rank=i64::try_from(f.place).unwrap_or(0) />
                                            <span class="size-2.5 shrink-0 rounded-[3px]" style=format!("background: {}", colour_var(lane.colour)) aria-hidden="true"></span>
                                            <span class="min-w-0 flex-1 truncate" class:font-bold=mine>{lane.name}</span>
                                            {official.map(|ms| view! {
                                                <span class="shrink-0 text-xs text-muted-foreground">
                                                    {move || td_string!(i18n.get_locale(), replay.finish.official, time = format_duration(ms))}
                                                </span>
                                            })}
                                            <span class="shrink-0 font-pixel tabular-nums">{format_duration(i64::from(f.at))}</span>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ol>
                    }
                        .into_any()
                }}
            </CardContent>
        </Card>
    }
}
