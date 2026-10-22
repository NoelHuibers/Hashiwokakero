//! A group puzzle's ranking within the group. Spoiler safe like the daily's: until the viewer is
//! done with it (or it closed), it only says who is done.

use leptos::prelude::*;

use crate::components::leaderboard::{HIGHLIGHT_ROW, PlayerName, RANK_TABLE, ROW_INSET, RankCell};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::model::{TodayEntry, TodayState, colour_var, format_duration};
use crate::pages::groups::today::ColourPip;

use super::state::Play;

/// What a row shows, for the browser tests.
const fn state_id(state: TodayState) -> &'static str {
    match state {
        TodayState::Waiting => "waiting",
        TodayState::Done => "done",
        TodayState::Ranked { .. } => "ranked",
        TodayState::Unranked => "unranked",
    }
}

#[component]
fn Row(entry: TodayEntry, me: bool, closed: bool) -> impl IntoView {
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
            view! {
                <span class="text-xs italic text-muted-foreground">
                    {move || if closed { t_string!(i18n, kadai.ranking.missed) } else { t_string!(i18n, groups.today.waiting) }}
                </span>
            }
            .into_any(),
        ),
    };
    let has_penalties = penalties.is_some();
    view! {
        <tr class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" }) data-state=state_id(state) data-user=user_id>
            <td class="py-2 pe-2">{first}</td>
            <td class="max-w-0 py-2 pe-2">
                <div class="flex min-w-0 items-center gap-2 [&>div]:min-w-0">
                    <ColourPip colour=colour />
                    <PlayerName user_id name guest stamp me />
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

/// The ranking card next to the board.
#[component]
pub fn RankingCard(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let board = Memo::new(move |_| {
        play.page.with(|p| {
            p.as_ref()
                .map(|p| (p.board.clone(), p.board_open, p.me, p.summary.open))
        })
    });
    view! {
        <Card class="gap-3" attr:data-testid="kadai-ranking">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, kadai.ranking.title)} <span class="font-pixel text-sm text-primary">"順位"</span>
                </CardTitle>
                <CardDescription>
                    {move || {
                        board.with(|b| b.as_ref().map(|(_, _, _, open)| *open)).map(|open| {
                            if open {
                                t_string!(i18n, kadai.ranking.open).to_string()
                            } else {
                                t_string!(i18n, kadai.ranking.final_board).to_string()
                            }
                        })
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || {
                    board.get().map(|(entries, open, me, still_open)| {
                        let closed = !still_open;
                        let done = entries.iter().filter(|e| e.state.done()).count();
                        let total = entries.len();
                        let summary = (!open).then(|| view! {
                            <p class="rounded-lg border border-dashed bg-muted/40 p-3 text-sm">
                                {move || t_string!(i18n, kadai.ranking.spoiler, done, total)}
                            </p>
                        });
                        let empty = entries.is_empty().then(|| view! {
                            <p class="text-sm text-muted-foreground">{t!(i18n, kadai.ranking.alone)}</p>
                        });
                        view! {
                            {summary}
                            {empty}
                            <div class="@container -mx-2">
                                <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                                    <colgroup>
                                        <col class="w-12" />
                                        <col />
                                        <col class="w-16" />
                                        <col class="w-24" />
                                    </colgroup>
                                    <tbody>
                                        {entries
                                            .into_iter()
                                            .map(|e| {
                                                let mine = e.user_id == me;
                                                view! { <Row entry=e me=mine closed /> }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }
                    })
                }}
            </CardContent>
        </Card>
    }
}
