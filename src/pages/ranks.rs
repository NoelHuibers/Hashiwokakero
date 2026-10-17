use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use crate::api::{all_time_leaderboard, archived_daily, daily_leaderboard};
use crate::components::leaderboard::{LeaderboardTable, RankCell};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::model::{
    AllTimeEntry, LAUNCH_DAY, Leaderboard, WEEKDAY_KANJI, daily_number, weekday,
};
use crate::components::layout::use_localized;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_long, difficulty_name, error_text};
use crate::state::{Game, Source, ToastKind, toast};
use crate::seo::PageMeta;

#[component]
pub fn RanksPage() -> impl IntoView {
    let i18n = use_i18n();
    let practice_path = use_localized("/practice");
    let game = expect_context::<Game>();
    let navigate = use_navigate();

    let day = RwSignal::new(None::<i64>);
    let today = RwSignal::new(None::<i64>);
    let board = RwSignal::new(None::<Leaderboard>);
    let all_time = RwSignal::new(None::<Vec<AllTimeEntry>>);
    let loading = RwSignal::new(false);

    // Fetch whenever the selected day changes (browser only)
    Effect::new(move |_| {
        let selected = day.get();
        loading.set(true);
        spawn_local(async move {
            match daily_leaderboard(selected).await {
                Ok(lb) => {
                    if selected.is_none() {
                        today.set(Some(lb.day));
                    }
                    board.set(Some(lb));
                }
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
            loading.set(false);
        });
    });
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                if let Ok(list) = all_time_leaderboard().await {
                    all_time.set(Some(list));
                }
            });
        }
    });

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
                        Source::Imported(td_string!(i18n.get_locale_untracked(), ranks.archive_name, number = daily_number(d)).to_string()),
                    );
                    navigate(&practice_path(), Default::default());
                }
                Ok(None) => toast(ToastKind::Info, |l| td_string!(l, ranks.no_puzzle).to_string()),
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
    };

    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, ranks.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, ranks.meta.description).to_string())
            path="/ranks"
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="mb-5">
                <p class="font-pixel text-sm text-primary">"順位 · jun'i"</p>
                <h1 class="text-3xl font-black">{t!(i18n, ranks.title)}</h1>
                <p class="mt-1 max-w-2xl text-muted-foreground">
                    {t!(i18n, ranks.intro)}
                </p>
            </div>

            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
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
                                                .map(|d| t_string!(i18n, daily.title, number = daily_number(d)).to_string())
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

                <Card class="gap-3 self-start">
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-lg">
                            {t!(i18n, ranks.fame.title)} <span class="font-pixel text-sm text-primary">"殿堂"</span>
                        </CardTitle>
                        <CardDescription>{t!(i18n, ranks.fame.text)}</CardDescription>
                    </CardHeader>
                    <CardContent>
                        {move || match all_time.get() {
                            None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                            Some(list) if list.is_empty() => {
                                view! { <p class="text-sm text-muted-foreground">{t!(i18n, ranks.fame.empty)}</p> }.into_any()
                            }
                            Some(list) => {
                                view! {
                                    <table class="w-full table-fixed text-sm">
                                        <colgroup>
                                            <col class="w-10" />
                                            <col />
                                            <col class="w-16" />
                                            <col class="w-16" />
                                        </colgroup>
                                        <thead>
                                            <tr class="border-b text-start text-[10px] uppercase tracking-wider text-muted-foreground">
                                                <th></th>
                                                <th class="py-1.5 font-semibold">{t!(i18n, ranks.fame.player)}</th>
                                                <th class="py-1.5 text-end font-semibold">{t!(i18n, account.stats.solved)}</th>
                                                <th class="py-1.5 text-end font-semibold">{t!(i18n, account.stats.best_streak_short)}</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {list
                                                .into_iter()
                                                .map(|e| {
                                                    view! {
                                                        <tr class="border-b last:border-0">
                                                            <td class="py-2 pe-2"><RankCell rank=e.rank /></td>
                                                            <td class="max-w-0 py-2 pe-2">
                                                                <span class="truncate font-medium">{e.name}</span>
                                                                {e.guest.then(|| view! { <span class="ms-1.5 text-xs text-muted-foreground">{t!(i18n, common.ranking.guest)}</span> })}
                                                            </td>
                                                            <td class="py-2 text-end font-pixel tabular-nums">{e.solved}</td>
                                                            <td class="py-2 text-end font-pixel tabular-nums">
                                                                <Badge variant=BadgeVariant::Muted>{e.best_streak}</Badge>
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                }
                                    .into_any()
                            }
                        }}
                    </CardContent>
                </Card>
            </div>
        </section>
    }
}
