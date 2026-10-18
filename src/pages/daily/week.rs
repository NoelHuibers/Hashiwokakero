//! The week's difficulties at a glance and the countdown to the next daily.

use common::Difficulty;
use leptos::prelude::*;

use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::lang::difficulty_name;
use crate::model::{MS_PER_DAY, WEEKDAY_KANJI, weekday};

use super::state::Daily;

/// `05:12:09` until the next daily.
fn countdown(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

#[component]
pub(super) fn WeekCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let today = Signal::derive(move || daily.info.with(|i| i.as_ref().map(|i| i.day)));
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-lg">
                    {t!(i18n, daily.week.title)} <span class="font-pixel text-sm text-primary">"週"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, daily.week.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                <div class="grid grid-cols-7 gap-1">
                    {(0..7usize)
                        .map(|wd| {
                            let d = match wd {
                                0 => Difficulty::Easy,
                                1 | 2 => Difficulty::Medium,
                                3 | 4 => Difficulty::Hard,
                                5 => Difficulty::Expert,
                                _ => Difficulty::Oni,
                            };
                            let is_today = move || today.get().is_some_and(|t| weekday(t) == wd);
                            view! {
                                <div
                                    class="flex flex-col items-center rounded-md border py-1.5"
                                    class=(["border-primary", "bg-primary", "text-primary-foreground"], is_today)
                                    title=move || difficulty_name(i18n.get_locale(), d)
                                >
                                    <span class="font-pixel text-base leading-none">{WEEKDAY_KANJI[wd]}</span>
                                    <span class="mt-1 font-pixel text-[11px] opacity-80">{d.kanji()}</span>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
                <p class="flex items-center justify-between text-sm">
                    <span class="text-muted-foreground">{t!(i18n, daily.week.next)}</span>
                    <span class="font-pixel tabular-nums">
                        {move || {
                            today
                                .get()
                                .map(|t| countdown((t + 1) * MS_PER_DAY - daily.server_now()))
                                .unwrap_or_default()
                        }}
                    </span>
                </p>
            </CardContent>
        </Card>
    }
}
