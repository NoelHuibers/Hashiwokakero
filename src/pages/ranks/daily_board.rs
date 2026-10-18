//! The daily ranking card of the ranks page: any day's leaderboard, and practicing a missed one.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use crate::api::archived_daily;
use crate::components::layout::use_localized;
use crate::components::leaderboard::LeaderboardTable;
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_long, difficulty_name, error_text};
use crate::model::{LAUNCH_DAY, Leaderboard, WEEKDAY_KANJI, daily_number, weekday};
use crate::state::{Game, Source, ToastKind, toast};

/// The leaderboard of the selected day (`None` for today) with buttons to go back and forth.
#[component]
pub(super) fn DailyBoard(
    day: RwSignal<Option<i64>>,
    today: RwSignal<Option<i64>>,
    board: RwSignal<Option<Leaderboard>>,
    loading: RwSignal<bool>,
) -> impl IntoView {
    let i18n = use_i18n();
    let practice_path = use_localized("/practice");
    let game = expect_context::<Game>();
    let navigate = use_navigate();

    let shown_day = move || board.with(|b| b.as_ref().map(|b| b.day));
    let go = move |delta: i64| {
        if let (Some(current), Some(t)) = (shown_day(), today.get_untracked()) {
            let next = (current + delta).clamp(LAUNCH_DAY, t);
            day.set(Some(next));
        }
    };

    let practice = move |_| {
        let Some(d) = shown_day() else { return };
        let navigate = navigate.clone();
        spawn_local(async move {
            match archived_daily(d).await {
                Ok(Some(puzzle)) => {
                    game.set_puzzle(
                        puzzle,
                        None,
                        Some(true),
                        Source::Imported(td_string!(
                            i18n.get_locale_untracked(),
                            ranks.archive_name,
                            number = daily_number(d)
                        )),
                    );
                    navigate(&practice_path(), NavigateOptions::default());
                }
                Ok(None) => toast(ToastKind::Info, |l| {
                    td_string!(l, ranks.no_puzzle).to_string()
                }),
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
    };

    view! {
        <Card class="gap-4">
            <CardHeader>
                <div class="flex w-full flex-wrap items-center justify-between gap-3">
                    <div class="flex items-center gap-3">
                        {move || {
                            shown_day()
                                .map(|d| {
                                    view! {
                                        <span class="hanko grid size-12 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-2xl">
                                            {WEEKDAY_KANJI[weekday(d)]}
                                        </span>
                                    }
                                })
                        }}
                        <div>
                            <CardTitle class="text-xl">
                                {move || {
                                    shown_day()
                                        .map(|d| t_string!(i18n, daily.title, number = daily_number(d)))
                                        .unwrap_or_default()
                                }}
                            </CardTitle>
                            <CardDescription>
                                {move || {
                                    board
                                        .get()
                                        .map(|b| {
                                            let locale = i18n.get_locale();
                                            format!(
                                                "{} · {} {} · {}",
                                                date_long(locale, b.day),
                                                b.difficulty.kanji(),
                                                difficulty_name(locale, b.difficulty),
                                                td_string!(locale, ranks.finished, count = b.finishers),
                                            )
                                        })
                                        .unwrap_or_default()
                                }}
                            </CardDescription>
                        </div>
                    </div>
                    <div class="flex gap-1.5">
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown_day().is_none_or(|d| d <= LAUNCH_DAY)
                            on:click=move |_| go(-1)
                        >
                            "← " {t!(i18n, ranks.earlier)}
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown_day() == today.get()
                            on:click=move |_| go(1)
                        >
                            {t!(i18n, ranks.later)} " →"
                        </Button>
                        <Button
                            variant=ButtonVariant::Ghost
                            size=ButtonSize::Sm
                            attr:disabled=move || shown_day() == today.get()
                            on:click=move |_| day.set(None)
                        >
                            {t!(i18n, ranks.today)}
                        </Button>
                    </div>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4" class:opacity-60=move || loading.get()>
                {move || board.get().map(|b| view! { <LeaderboardTable entries=b.entries me=b.me /> })}
                <Show when=move || { shown_day().zip(today.get()).is_some_and(|(d, t)| d < t) }>
                    <div class="flex items-center justify-between gap-3 rounded-lg border border-dashed p-3 text-sm">
                        <span class="text-muted-foreground">{t!(i18n, ranks.missed)}</span>
                        <Button size=ButtonSize::Sm variant=ButtonVariant::Secondary on:click=practice.clone()>
                            {t!(i18n, ranks.practice_it)} " 練習"
                        </Button>
                    </div>
                </Show>
            </CardContent>
        </Card>
    }
}
