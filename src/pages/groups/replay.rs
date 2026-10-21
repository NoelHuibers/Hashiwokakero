//! The group page's way into the replay theatre (`pages::replay`): today's replay once the
//! viewer finished the daily, and the days before, back to the group's first.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::layout::use_localized;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::{date_short, prefix};
use crate::model::{LAUNCH_DAY, WEEKDAY_KANJI, daily_number, group_day, weekday};
use crate::pages::replay::replay_path;

use super::GroupContext;

/// Days before today with a link of their own.
pub const PAST_DAYS: i64 = 7;

/// The days before `today` to offer, the latest first, not before the group's first day.
pub fn past_days(today: i64, first: i64) -> Vec<i64> {
    (1..=PAST_DAYS)
        .map(|back| today - back)
        .filter(|&d| d >= first)
        .collect()
}

#[component]
pub fn ReplayCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    let daily = use_localized("/");
    let href = move |day: i64| {
        let id = group.page.with(|p| p.as_ref().map_or(0, |p| p.id));
        format!(
            "{}{}",
            prefix(i18n.get_locale()),
            replay_path(id, day, None)
        )
    };
    view! {
        <Card class="gap-3" attr:data-testid="replay-card">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, replay.card.title)} <span class="font-pixel text-sm text-primary">"再生"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, replay.card.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || {
                    let p = group.page.get()?;
                    let today = p.today.day;
                    let open = p.today.open;
                    // Not before the group, nor before the first daily
                    let first = group_day(p.created_at).max(LAUNCH_DAY);
                    let watch = if open {
                        view! {
                            <A
                                href=move || href(today)
                                attr:class="inline-flex min-h-11 w-fit items-center gap-2 rounded-full bg-primary px-5 font-medium text-primary-foreground shadow-md transition-transform hover:scale-[1.02] active:scale-[0.98]"
                                attr:data-testid="watch-today"
                            >
                                <span aria-hidden="true">"▶"</span>
                                {t!(i18n, replay.watch)}
                            </A>
                        }
                            .into_any()
                    } else {
                        view! {
                            <div class="flex flex-col gap-1 rounded-lg border border-dashed bg-muted/40 p-3 text-sm">
                                <p class="flex items-center gap-2 font-medium">
                                    <span class="font-pixel text-primary" aria-hidden="true">"待"</span>
                                    {t!(i18n, replay.card.locked)}
                                </p>
                                <A href=daily exact=true attr:class="w-fit text-primary underline-offset-4 hover:underline">
                                    {t!(i18n, replay.locked.play)} " →"
                                </A>
                            </div>
                        }
                            .into_any()
                    };
                    let days = past_days(today, first);
                    let earlier = (!days.is_empty()).then(|| {
                        view! {
                            <div class="flex flex-col gap-1.5">
                                <p class="text-xs font-medium text-muted-foreground">{t!(i18n, replay.card.earlier)}</p>
                                <ul class="flex flex-wrap gap-1.5">
                                    {days
                                        .into_iter()
                                        .map(|day| {
                                            let label = move || date_short(i18n.get_locale(), day);
                                            view! {
                                                <li>
                                                    <A
                                                        href=move || href(day)
                                                        attr:class="flex items-center gap-1.5 rounded-md border bg-card px-2.5 py-1 text-sm shadow-xs transition-colors hover:border-primary/60 hover:bg-accent"
                                                        attr:data-day=day
                                                        attr:aria-label=move || format!("{} · {}", t_string!(i18n, replay.watch), label())
                                                        attr:data-tooltip=label
                                                    >
                                                        <span class="font-pixel text-primary" aria-hidden="true">{WEEKDAY_KANJI[weekday(day)]}</span>
                                                        <span class="tabular-nums" aria-hidden="true">{format!("#{}", daily_number(day))}</span>
                                                    </A>
                                                </li>
                                            }
                                        })
                                        .collect_view()}
                                </ul>
                            </div>
                        }
                    });
                    Some(view! { {watch} {earlier} })
                }}
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn past_days_go_back_a_week_and_not_before_the_group() {
        assert_eq!(past_days(100, 0), [99, 98, 97, 96, 95, 94, 93]);
        assert_eq!(past_days(100, 98), [99, 98]);
        assert_eq!(past_days(100, 100), Vec::<i64>::new());
    }
}
