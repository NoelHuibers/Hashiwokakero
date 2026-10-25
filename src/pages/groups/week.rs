//! The week grid: members × Monday to Sunday of a UTC week.
//!
//! A hanko in the member's colour for every daily solved, empty paper for the ones missed, today
//! marked. Earlier weeks page back to the group's first.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::load_group_week;
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::date_short;
use crate::model::{GroupWeek, WEEKDAY_KANJI, colour_var};
use crate::state::error_toast;

use super::GroupContext;
use super::archipelago::initial;

/// What a day of a member is in the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayCell {
    Solved,
    Missed,
    /// Today, not solved yet: there's still time.
    Open,
    /// After today.
    Future,
}

/// The cell of `day` for a member who solved it or not, on `today`.
pub const fn day_cell(day: i64, today: i64, solved: bool) -> DayCell {
    if solved {
        DayCell::Solved
    } else if day < today {
        DayCell::Missed
    } else if day == today {
        DayCell::Open
    } else {
        DayCell::Future
    }
}

#[component]
fn Grid(week: GroupWeek, me: i64) -> impl IntoView {
    let i18n = use_i18n();
    let GroupWeek {
        monday,
        today,
        rows,
        ..
    } = week;
    view! {
        // relative: the hidden captions (sr-only, absolute) stay in the scroller, not past the screen
        <div class="relative -mx-2 overflow-x-auto px-2">
            <table class="w-full min-w-[19rem] table-fixed border-separate border-spacing-y-1 text-sm">
                <colgroup>
                    <col class="w-[34%] sm:w-[30%]" />
                    {(0..7).map(|_| view! { <col /> }).collect_view()}
                </colgroup>
                <thead>
                    <tr>
                        <th class="sr-only">{t!(i18n, groups.week.member)}</th>
                        {(0..7i64)
                            .map(|i| {
                                let day = monday + i;
                                let is_today = day == today;
                                view! {
                                    <th
                                        class="pb-1 text-center font-pixel text-sm font-normal"
                                        class=(["text-primary"], is_today)
                                        class=(["text-muted-foreground"], !is_today)
                                        data-tooltip=move || date_short(i18n.get_locale(), day)
                                    >
                                        <span class="sr-only">{move || date_short(i18n.get_locale(), day)}</span>
                                        <span aria-hidden="true">{WEEKDAY_KANJI[usize::try_from(i).unwrap_or(0)]}</span>
                                    </th>
                                }
                            })
                            .collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {rows
                        .into_iter()
                        .map(|row| {
                            let colour = colour_var(row.colour);
                            let letter = initial(&row.name);
                            let mine = row.user_id == me;
                            let name = row.name.clone();
                            view! {
                                <tr data-member=row.user_id>
                                    <th scope="row" class="max-w-0 pe-2 text-start font-normal">
                                        <span class="flex min-w-0 items-center gap-1.5">
                                            <span class="size-2.5 shrink-0 rounded-[3px]" style=format!("background: {colour}") aria-hidden="true"></span>
                                            <span class="truncate" class:font-bold=mine>{row.name.clone()}</span>
                                        </span>
                                    </th>
                                    {(0..7usize)
                                        .map(|i| {
                                            let day = monday + i64::try_from(i).unwrap_or(0);
                                            let cell = day_cell(day, today, row.solved[i]);
                                            let name = name.clone();
                                            let label = move || {
                                                let l = i18n.get_locale();
                                                let date = date_short(l, day);
                                                match cell {
                                                    DayCell::Solved => td_string!(l, groups.week.solved, name = name.clone(), date),
                                                    DayCell::Missed => td_string!(l, groups.week.missed, name = name.clone(), date),
                                                    DayCell::Open | DayCell::Future => td_string!(l, groups.week.open, name = name.clone(), date),
                                                }
                                            };
                                            let today_col = day == today;
                                            view! {
                                                <td
                                                    class="px-0.5 text-center"
                                                    class=(["bg-primary/10"], today_col)
                                                    data-day=day
                                                    data-cell=format!("{cell:?}").to_lowercase()
                                                >
                                                    <span
                                                        class="mx-auto grid aspect-square w-full max-w-8 place-items-center rounded-md font-pixel text-xs leading-none"
                                                        class=(["nakama-stamp", "rotate-[-6deg]"], cell == DayCell::Solved)
                                                        class=(["border", "border-dashed", "border-border", "bg-card"], cell == DayCell::Missed)
                                                        class=(["border", "border-dashed", "border-primary/60"], cell == DayCell::Open)
                                                        class=(["opacity-30", "border", "border-border/60"], cell == DayCell::Future)
                                                        style=format!("--member: {colour}")
                                                        data-tooltip=label.clone()
                                                        role="img"
                                                        aria-label=label
                                                    >
                                                        {(cell == DayCell::Solved).then(|| letter.clone())}
                                                    </span>
                                                </td>
                                            }
                                        })
                                        .collect_view()}
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// The group page's week card.
#[component]
pub fn WeekCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    // The Monday asked for, `None`: this week
    let monday = RwSignal::new(None::<i64>);
    let week = RwSignal::new(None::<GroupWeek>);
    let id = Memo::new(move |_| group.page.with(|p| p.as_ref().map(|p| p.id)));

    // Again when the page reloads (a member more or less)
    Effect::new(move |_| {
        let Some(id) = id.get() else { return };
        group.page.track();
        let asked = monday.get();
        spawn_local(async move {
            match load_group_week(id, asked).await {
                Ok(w) => week.set(Some(w)),
                Err(e) => error_toast(e),
            }
        });
    });

    let shown = move || week.with(|w| w.as_ref().map(|w| (w.monday, w.first, w.today)));
    let go = move |delta: i64| {
        if let Some((m, ..)) = shown() {
            monday.set(Some(m + delta));
        }
    };
    let me = move || group.page.with(|p| p.as_ref().map_or(0, |p| p.me));
    view! {
        <Card class="gap-3">
            <CardHeader>
                <div class="flex w-full flex-wrap items-center justify-between gap-3">
                    <div>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, groups.week.title)} <span class="font-pixel text-sm text-primary">"週"</span>
                        </CardTitle>
                        <CardDescription>
                            {move || {
                                shown()
                                    .map(|(m, ..)| {
                                        let l = i18n.get_locale();
                                        format!("{} – {}", date_short(l, m), date_short(l, m + 6))
                                    })
                            }}
                        </CardDescription>
                    </div>
                    <div class="flex gap-1.5">
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown().is_none_or(|(m, first, _)| m <= first)
                            attr:aria-label=move || t_string!(i18n, groups.week.earlier)
                            on:click=move |_| go(-7)
                        >
                            "← " <span class="hidden sm:inline">{t!(i18n, groups.week.earlier)}</span>
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown().is_none_or(|(m, _, today)| m + 6 >= today)
                            attr:aria-label=move || t_string!(i18n, groups.week.later)
                            on:click=move |_| go(7)
                        >
                            <span class="hidden sm:inline">{t!(i18n, groups.week.later)}</span> " →"
                        </Button>
                    </div>
                </div>
            </CardHeader>
            <CardContent>
                {move || match week.get() {
                    Some(w) => view! { <Grid week=w me=me() /> }.into_any(),
                    None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                }}
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_solved_missed_open_or_still_to_come() {
        assert_eq!(day_cell(10, 12, true), DayCell::Solved);
        assert_eq!(day_cell(12, 12, true), DayCell::Solved);
        assert_eq!(day_cell(10, 12, false), DayCell::Missed);
        assert_eq!(day_cell(12, 12, false), DayCell::Open);
        assert_eq!(day_cell(13, 12, false), DayCell::Future);
    }
}
