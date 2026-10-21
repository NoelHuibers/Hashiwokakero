//! Today's daily in a group: who's done (before you finished it yourself), the ranking (after).
//! The board is shared with the daily page's ranking card, a tab per group.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::layout::use_localized;
use crate::components::leaderboard::{HIGHLIGHT_ROW, PlayerName, RANK_TABLE, ROW_INSET, RankCell};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::{GroupToday, TodayEntry, TodayState, colour_var, daily_number, format_duration};
use crate::pages::replay::replay_path;

use super::GroupContext;

/// A member's colour, as a little square next to their name.
#[component]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Leptos components take their props by value"
)]
pub fn ColourPip(colour: u8, #[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <span
            class=format!("inline-block size-2.5 shrink-0 rounded-[3px] {class}")
            style=format!("background: {}", colour_var(colour))
            aria-hidden="true"
        ></span>
    }
}

/// How many of the group are done, a pip per member in their colour, filled when they are.
#[component]
fn Progress(board: GroupToday) -> impl IntoView {
    view! {
        <div class="flex flex-wrap gap-1" aria-hidden="true">
            {board
                .entries
                .iter()
                .map(|e| {
                    let style = if e.state.done() {
                        format!("background: {}", colour_var(e.colour))
                    } else {
                        format!("box-shadow: inset 0 0 0 1.5px {}", colour_var(e.colour))
                    };
                    view! { <span class="h-2 w-4 rounded-full opacity-90" style=style></span> }
                })
                .collect_view()}
        </div>
    }
}

/// What a row shows, for the browser tests.
const fn state_id(state: TodayState) -> &'static str {
    match state {
        TodayState::Waiting => "waiting",
        TodayState::Done => "done",
        TodayState::Ranked { .. } => "ranked",
        TodayState::Unranked => "unranked",
    }
}

/// A small ▶ next to a member who finished, once the board is open: the viewer's solve against
/// theirs in the replay theatre.
#[component]
fn DuelLink(group: i64, day: i64, user_id: i64, name: String) -> impl IntoView {
    let i18n = use_i18n();
    let href = move || {
        format!(
            "{}{}",
            prefix(i18n.get_locale()),
            replay_path(group, day, Some(user_id))
        )
    };
    let label = move || td_string!(i18n.get_locale(), replay.duel, name = name.clone());
    view! {
        <A
            href=href
            attr:class="grid size-6 shrink-0 place-items-center rounded-full border bg-card text-[10px] text-primary shadow-xs transition-colors hover:border-primary hover:bg-primary hover:text-primary-foreground"
            attr:aria-label=label.clone()
            attr:data-tooltip=label
            attr:data-duel=user_id
        >
            "▶"
        </A>
    }
}

#[component]
fn Row(entry: TodayEntry, me: bool, duel: Option<(i64, i64)>) -> impl IntoView {
    let i18n = use_i18n();
    let TodayEntry {
        user_id,
        name,
        guest,
        colour,
        stamp,
        state,
    } = entry;
    let first = match state {
        TodayState::Ranked { rank, .. } => view! { <RankCell rank=rank /> }.into_any(),
        TodayState::Done => view! {
            <span
                class="nakama-stamp grid size-7 rotate-[-6deg] place-items-center rounded-md font-pixel text-sm"
                style=format!("--member: {}", colour_var(colour))
                aria-hidden="true"
            >
                "済"
            </span>
        }
        .into_any(),
        TodayState::Unranked => view! {
            <span class="grid size-7 place-items-center font-pixel text-muted-foreground" aria-hidden="true">"–"</span>
        }
        .into_any(),
        TodayState::Waiting => view! {
            <span class="grid size-7 place-items-center font-pixel text-sea" aria-hidden="true">"〜"</span>
        }
        .into_any(),
    };
    let (penalties, end) = match state {
        TodayState::Ranked {
            time_ms,
            hints,
            checks,
            ..
        } => (
            (hints + checks > 0).then(|| format!("{hints}h {checks}c")),
            view! { <span class="font-pixel tabular-nums" class:text-primary=me>{format_duration(time_ms)}</span> }
                .into_any(),
        ),
        TodayState::Done => (
            None,
            view! { <span class="text-xs font-medium text-success-dark">"✓ " {t!(i18n, groups.today.done)}</span> }
                .into_any(),
        ),
        TodayState::Unranked => (
            None,
            view! { <span class="text-xs text-muted-foreground">{t!(i18n, groups.today.unranked)}</span> }
                .into_any(),
        ),
        TodayState::Waiting => (
            None,
            view! { <span class="text-xs italic text-muted-foreground">{t!(i18n, groups.today.waiting)}</span> }
                .into_any(),
        ),
    };
    let has_penalties = penalties.is_some();
    // Their solve can be watched against one's own: finished and not given up
    let duel = duel
        .filter(|_| !me && matches!(state, TodayState::Ranked { .. } | TodayState::Unranked))
        .map(|(group, day)| view! { <DuelLink group day user_id name=name.clone() /> });
    view! {
        <tr class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" }) data-state=state_id(state)>
            <td class="py-2 pe-2">{first}</td>
            <td class="max-w-0 py-2 pe-2">
                <div class="flex min-w-0 items-center gap-2 [&>div]:min-w-0">
                    <ColourPip colour=colour />
                    <PlayerName user_id name guest stamp me />
                    {duel}
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

/// A group's board of today. Before the viewer (`me`) finished, it says who's done and nudges;
/// after, the ranking with times like the global board.
#[component]
pub fn TodayBoard(board: GroupToday, me: Option<i64>) -> impl IntoView {
    let i18n = use_i18n();
    let daily = use_localized("/");
    let done = board.done();
    let total = board.entries.len();
    // The group and the day, for the rows' duel links
    let place = (board.group_id, board.day);
    let open = board.open;
    let summary = if open {
        None
    } else {
        Some(view! {
            <div class="flex flex-col gap-2 rounded-lg border border-dashed bg-muted/40 p-3">
                <p class="text-sm font-medium">
                    {move || {
                        if done == 0 {
                            t_string!(i18n, groups.today.all_waiting).to_string()
                        } else {
                            t_string!(i18n, groups.today.your_turn, done, total)
                        }
                    }}
                </p>
                <Progress board=board.clone() />
                <p class="text-xs text-muted-foreground">{t!(i18n, groups.today.spoiler)}</p>
                <A
                    href=daily
                    exact=true
                    attr:class="w-fit text-sm font-medium text-primary underline-offset-4 hover:underline"
                >
                    {t!(i18n, groups.today.play)} " →"
                </A>
            </div>
        })
    };
    view! {
        <div class="flex flex-col gap-3" data-board=board.group_id data-open=open.to_string()>
            {summary}
            <div class="@container -mx-2">
                <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                    <colgroup>
                        <col class="w-12" />
                        <col />
                        <col class="w-16" />
                        <col class="w-24" />
                    </colgroup>
                    <tbody>
                        {board
                            .entries
                            .into_iter()
                            .map(|e| {
                                let mine = Some(e.user_id) == me;
                                let duel = open.then_some(place);
                                view! { <Row entry=e me=mine duel /> }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

/// The group page's card of today's daily.
#[component]
pub fn TodayCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, groups.today.title)} <span class="font-pixel text-sm text-primary">"今日"</span>
                </CardTitle>
                <CardDescription>
                    {move || {
                        group
                            .page
                            .with(|p| p.as_ref().map(|p| p.today.day))
                            .map(|day| t_string!(i18n, groups.today.daily, number = daily_number(day)))
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent>
                {move || {
                    group
                        .page
                        .get()
                        .map(|p| view! { <TodayBoard board=p.today me=Some(p.me) /> })
                }}
            </CardContent>
        </Card>
    }
}
