//! The monthly race card of the ranks page: points of this (or an earlier) month.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::month_leaderboard;
use crate::components::leaderboard::{
    HIGHLIGHT_ROW, Place, PlaceDetail, PlaceValue, PlayerName, Podium, RANK_TABLE, ROW_INSET,
    RankCell,
};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::month_long;
use crate::model::{LAUNCH_DAY, MonthBoard, MonthEntry, month_of};
use crate::state::error_toast;

#[component]
fn Row(entry: MonthEntry, me: bool) -> impl IntoView {
    view! {
        <tr class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" })>
            <td class="py-2 pe-2">
                <RankCell rank=entry.rank />
            </td>
            <td class="max-w-0 py-2 pe-2">
                <PlayerName user_id=entry.user_id name=entry.name guest=entry.guest stamp=entry.stamp me=me marked_row=true rival=true />
            </td>
            <td class="py-2 text-end font-pixel tabular-nums text-muted-foreground">{entry.solved}</td>
            <td class="py-2 text-end font-pixel text-base tabular-nums" class:text-primary=me>{entry.points}</td>
        </tr>
    }
}

/// The points race of a month: 10 for every daily solved, up to 10 more for the placement.
#[component]
pub(super) fn MonthRace() -> impl IntoView {
    let i18n = use_i18n();
    let month = RwSignal::new(None::<i64>);
    let current = RwSignal::new(None::<i64>);
    let board = RwSignal::new(None::<MonthBoard>);
    let loading = RwSignal::new(false);

    // Fetch whenever the selected month changes (browser only)
    Effect::new(move |_| {
        let selected = month.get();
        loading.set(true);
        spawn_local(async move {
            match month_leaderboard(selected).await {
                Ok(b) => {
                    if selected.is_none() {
                        current.set(Some(b.month));
                    }
                    board.set(Some(b));
                }
                Err(e) => error_toast(e),
            }
            loading.set(false);
        });
    });

    let shown = move || board.with(|b| b.as_ref().map(|b| b.month));
    let go = move |delta: i64| {
        if let (Some(m), Some(c)) = (shown(), current.get_untracked()) {
            month.set(Some((m + delta).clamp(month_of(LAUNCH_DAY), c)));
        }
    };

    view! {
        <Card class="gap-4">
            <CardHeader>
                <div class="flex w-full flex-wrap items-center justify-between gap-3">
                    <div class="flex items-center gap-3">
                        <span class="hanko grid size-12 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-2xl">
                            "月"
                        </span>
                        <div>
                            <CardTitle class="text-xl">
                                {t!(i18n, ranks.month.title)} " · "
                                {move || shown().map(|m| month_long(i18n.get_locale(), m)).unwrap_or_default()}
                            </CardTitle>
                            <CardDescription>{t!(i18n, ranks.month.text)}</CardDescription>
                        </div>
                    </div>
                    <div class="flex gap-1.5">
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown().is_none_or(|m| m <= month_of(LAUNCH_DAY))
                            on:click=move |_| go(-1)
                        >
                            "← "
                            {t!(i18n, ranks.earlier)}
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown() == current.get()
                            on:click=move |_| go(1)
                        >
                            {t!(i18n, ranks.later)}
                            " →"
                        </Button>
                    </div>
                </div>
            </CardHeader>
            <CardContent class:opacity-60=move || loading.get()>
                {move || match board.get() {
                    None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(b) if b.entries.is_empty() => {
                        view! { <p class="text-sm text-muted-foreground">{t!(i18n, ranks.month.empty)}</p> }.into_any()
                    }
                    Some(b) => {
                        let me = b.me.as_ref().map(|m| m.user_id);
                        let outside = b.me.clone().filter(|m| !b.entries.iter().any(|e| e.user_id == m.user_id));
                        let mut top = b.entries;
                        let rest = top.split_off(top.partition_point(|entry| entry.rank <= 3));
                        let places: Vec<Place> = top
                            .into_iter()
                            .map(|e| Place {
                                rank: e.rank,
                                me: Some(e.user_id) == me,
                                user_id: e.user_id,
                                value: PlaceValue::Points(e.points),
                                detail: PlaceDetail::Solved(e.solved),
                                name: e.name,
                                guest: e.guest,
                                stamp: e.stamp,
                            })
                            .collect();
                        let table = (!rest.is_empty() || outside.is_some()).then(|| view! {
                            // A size container for PlayerName, pulled out by the row inset
                            <div class="@container -mx-2">
                            <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                                <colgroup>
                                    // The first and last columns hold the row inset (ROW_INSET) too
                                    <col class="w-14" />
                                    <col />
                                    <col class="w-16" />
                                    <col class="w-20" />
                                </colgroup>
                                <thead>
                                    <tr class=format!("text-start text-[10px] uppercase tracking-wider text-muted-foreground {ROW_INSET}")>
                                        <th></th>
                                        <th class="py-1.5 text-start font-semibold">{t!(i18n, ranks.fame.player)}</th>
                                        <th class="py-1.5 text-end font-semibold">{t!(i18n, account.stats.solved)}</th>
                                        <th class="py-1.5 text-end font-semibold">{t!(i18n, ranks.month.points)}</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rest
                                        .into_iter()
                                        .map(|e| {
                                            let mine = Some(e.user_id) == me;
                                            view! { <Row entry=e me=mine /> }
                                        })
                                        .collect_view()}
                                    {outside
                                        .map(|e| {
                                            view! {
                                                <tr aria-hidden="true">
                                                    <td colspan="4" class="py-0.5 text-center text-xs text-muted-foreground">"⋯"</td>
                                                </tr>
                                                <Row entry=e me=true />
                                            }
                                        })}
                                </tbody>
                            </table>
                            </div>
                        });
                        view! {
                            <div class="flex flex-col gap-3">
                                <Podium places=places />
                                {table}
                            </div>
                            <p class="mt-2 text-xs text-muted-foreground">
                                {move || t_string!(i18n, ranks.month.players, count = b.players)}
                            </p>
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}
