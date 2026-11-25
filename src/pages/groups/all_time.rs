//! The all time card 通算: a line per member with their daily wins, race wins, weeks as
//! yokozuna and current streak, sortable by each (`model::all_time` has the rules).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::load_group_all_time;
use crate::components::leaderboard::{HIGHLIGHT_ROW, PlayerName, RANK_TABLE, ROW_INSET};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{Locale, t, td_string, use_i18n};
use crate::lang::date_short;
use crate::model::{AllTimeColumn, GroupAllTime, GroupAllTimeRow, sort_all_time};
use crate::state::error_toast;

use super::GroupContext;
use super::today::ColourPip;

/// A column's heading: `(short, full, tooltip)`, the short one for narrow cards.
fn heading(column: AllTimeColumn, l: Locale) -> (String, String, String) {
    let (short, full) = match column {
        AllTimeColumn::DailyWins => (
            td_string!(l, groups.all_time.daily_short).to_string(),
            td_string!(l, groups.all_time.daily).to_string(),
        ),
        AllTimeColumn::RaceWins => (
            td_string!(l, groups.all_time.races_short).to_string(),
            td_string!(l, groups.all_time.races).to_string(),
        ),
        AllTimeColumn::Yokozuna => (
            td_string!(l, groups.all_time.yokozuna_short).to_string(),
            td_string!(l, groups.all_time.yokozuna).to_string(),
        ),
        AllTimeColumn::Streak => (
            td_string!(l, groups.all_time.streak_short).to_string(),
            td_string!(l, groups.all_time.streak).to_string(),
        ),
    };
    let tip = if column == AllTimeColumn::Streak {
        td_string!(l, groups.all_time.streak_tip).to_string()
    } else {
        full.clone()
    };
    (short, full, tip)
}

/// A heading that sorts by its column: the most first, again for the fewest first.
#[component]
fn Heading(column: AllTimeColumn, sort: RwSignal<(AllTimeColumn, bool)>) -> impl IntoView {
    let i18n = use_i18n();
    let state = move || match sort.get() {
        (by, _) if by != column => "none",
        (_, false) => "descending",
        (_, true) => "ascending",
    };
    let names = move || heading(column, i18n.get_locale());
    view! {
        <th class="py-2 text-end align-top font-medium" aria-sort=state data-column=column.id()>
            <button
                type="button"
                class="inline-flex max-w-full flex-col items-end leading-tight hover:text-foreground"
                class=("text-primary", move || state() != "none")
                data-tooltip=move || names().2
                aria-label=move || names().2
                on:click=move |_| {
                    sort.update(|(by, reversed)| {
                        if *by == column {
                            *reversed = !*reversed;
                        } else {
                            (*by, *reversed) = (column, false);
                        }
                    });
                }
            >
                // The short heading on a phone, the whole one where there's room
                <span class="block max-w-full truncate @xl:hidden">{move || names().0}</span>
                <span class="hidden @xl:block">{move || names().1}</span>
                <span aria-hidden="true" class="h-3 text-[10px]">
                    {move || match state() {
                        "descending" => "▼",
                        "ascending" => "▲",
                        _ => "",
                    }}
                </span>
            </button>
        </th>
    }
}

/// A member's line.
#[component]
fn Line(row: GroupAllTimeRow, me: bool) -> impl IntoView {
    let numbers = AllTimeColumn::ALL
        .iter()
        .map(|&c| {
            let n = c.of(&row);
            view! {
                <td
                    class="py-2 text-end font-pixel tabular-nums"
                    class=("text-muted-foreground", n == 0)
                    class=("text-primary", me && n > 0)
                    data-column=c.id()
                >
                    {n}
                </td>
            }
        })
        .collect_view();
    view! {
        <tr
            class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" })
            data-member=row.user_id
        >
            <td class="max-w-0 py-2 pe-2">
                <div class="flex min-w-0 items-center gap-2 [&>div]:min-w-0">
                    <ColourPip colour=row.colour />
                    <PlayerName
                        user_id=row.user_id
                        name=row.name
                        guest=row.guest
                        stamp=None
                        me
                        marked_row=true
                    />
                </div>
            </td>
            {numbers}
        </tr>
    }
}

/// The table, sorted as asked.
#[component]
fn Table(table: GroupAllTime, me: i64, sort: RwSignal<(AllTimeColumn, bool)>) -> impl IntoView {
    let rows = move || {
        let (column, reversed) = sort.get();
        let mut rows = table.rows.clone();
        sort_all_time(&mut rows, column, reversed);
        rows.into_iter()
            .map(|row| {
                let mine = row.user_id == me;
                view! { <Line row me=mine /> }
            })
            .collect_view()
    };
    let i18n = use_i18n();
    view! {
        // A size container for the headings (tables themselves can't be one), pulled out by the
        // row inset
        <div class="@container -mx-2">
            <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                <colgroup>
                    <col />
                    <col class="w-11 @xl:w-28" />
                    <col class="w-11 @xl:w-28" />
                    <col class="w-11 @xl:w-28" />
                    <col class="w-12 @xl:w-24" />
                </colgroup>
                <thead class="text-[11px] text-muted-foreground @xl:text-xs">
                    <tr class=ROW_INSET>
                        <th class="py-2 text-start align-top font-medium">
                            {t!(i18n, groups.all_time.member)}
                        </th>
                        {AllTimeColumn::ALL
                            .iter()
                            .map(|&column| view! { <Heading column sort /> })
                            .collect_view()}
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}

/// How each column is counted, folded away.
#[component]
fn Rules() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <details class="group rounded-lg border bg-muted/30 px-3 py-2 text-sm">
            <summary class="cursor-pointer select-none font-medium text-muted-foreground hover:text-foreground">
                {t!(i18n, groups.all_time.rules.title)}
            </summary>
            <div class="mt-2 flex flex-col gap-1.5 text-xs text-muted-foreground">
                <p>{t!(i18n, groups.all_time.rules.daily)}</p>
                <p>{t!(i18n, groups.all_time.rules.races)}</p>
                <p>{t!(i18n, groups.all_time.rules.yokozuna)}</p>
                <p>{t!(i18n, groups.all_time.rules.members)}</p>
            </div>
        </details>
    }
}

/// The group page's all time card.
#[component]
pub fn AllTimeCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    let table = RwSignal::new(None::<GroupAllTime>);
    // By daily wins (then race wins) at first
    let sort = RwSignal::new((AllTimeColumn::DailyWins, false));
    let id = Memo::new(move |_| group.page.with(|p| p.as_ref().map(|p| p.id)));

    // Again when the page reloads (a member more or less, today's daily finished)
    Effect::new(move |_| {
        let Some(id) = id.get() else { return };
        group.page.track();
        spawn_local(async move {
            match load_group_all_time(id).await {
                Ok(t) => table.set(Some(t)),
                Err(e) => error_toast(e),
            }
        });
    });

    let me = move || group.page.with(|p| p.as_ref().map_or(0, |p| p.me));
    view! {
        <Card class="gap-3" attr:data-testid="all-time">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, groups.all_time.title)}
                    <span class="font-pixel text-sm text-primary">"通算"</span>
                </CardTitle>
                <CardDescription>
                    {move || {
                        table
                            .with(|t| t.as_ref().map(|t| t.first))
                            .map(|first| {
                                let l = i18n.get_locale();
                                td_string!(l, groups.all_time.since, date = date_short(l, first))
                            })
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || match table.get() {
                    Some(t) => {
                        let empty = t
                            .empty()
                            .then(|| {
                                view! {
                                    <p
                                        class="text-sm text-muted-foreground"
                                        data-testid="all-time-empty"
                                    >
                                        {t!(i18n, groups.all_time.empty)}
                                    </p>
                                }
                            });
                        let hidden = t
                            .today_hidden
                            .then(|| {
                                view! {
                                    <p class="text-xs text-muted-foreground">
                                        {t!(i18n, banzuke.today_hidden)}
                                    </p>
                                }
                            });
                        view! {
                            {empty}
                            <Table table=t me=me() sort />
                            {hidden}
                        }
                            .into_any()
                    }
                    None => {
                        view! {
                            <p class="text-sm text-muted-foreground">
                                {t!(i18n, common.game.loading)}
                            </p>
                        }
                            .into_any()
                    }
                }} <Rules />
            </CardContent>
        </Card>
    }
}
