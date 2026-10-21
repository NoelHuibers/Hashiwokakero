//! The result card once the daily is solved or given up: time, rank and sharing.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::celebration::Stamp;
use crate::components::layout::use_localized;
use crate::components::share::ShareBar;
use crate::components::ui::card::Card;
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::{difficulty_name, localized, prefix};
use crate::model::{Flag, WEEKDAY_KANJI, daily_number, format_duration, points, weekday};
use crate::pages::groups::hanko::ReceivedHanko;
use crate::state::{Auth, Status};

use super::recovery::Phase;
use super::state::Daily;
use crate::components::ui::button::{Button, ButtonVariant};

/// What the share buttons post about the finished daily: its number, weekday and difficulty,
/// the time, the rank once it's known, and the link.
pub(super) fn share_text(daily: &Daily, locale: Locale) -> String {
    let Some(info) = daily.info.get_untracked() else {
        return String::new();
    };
    let rank = daily
        .result
        .get_untracked()
        .map(|r| (r.rank, r.finishers))
        .or_else(|| {
            daily.ranking.with_untracked(|lb| {
                lb.as_ref()
                    .and_then(|lb| lb.me.as_ref().map(|m| (m.rank, lb.finishers)))
            })
        });
    let origin = window().location().origin().unwrap_or_default();
    let headline = td_string!(
        locale,
        daily.share,
        number = daily_number(info.day),
        weekday = WEEKDAY_KANJI[weekday(info.day)],
        difficulty = difficulty_name(locale, info.difficulty)
    );
    let rank = rank
        .map(|(rank, total)| format!(" · {}", td_string!(locale, daily.share_rank, rank, total)))
        .unwrap_or_default();
    format!(
        "{headline}\n⏱ {}{rank}\n{origin}{}",
        format_duration(daily.elapsed_ms()),
        localized(locale, "/"),
    )
}

#[component]
pub(super) fn ResultCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let practice = use_localized("/practice");
    let account = use_localized("/account");
    let share_text = move || share_text(&daily, i18n.get_locale_untracked());
    let revealed = move || daily.game.status.get() == Status::Revealed;
    let day = Memo::new(move |_| daily.info.with(|i| i.as_ref().map(|i| i.day)));
    // (rank, finishers): from the submit, later from the ranking, which keeps up with the others
    let place = move || {
        daily
            .result
            .get()
            .map(|r| (r.rank, r.finishers))
            .filter(|&(rank, _)| rank > 0)
            .or_else(|| {
                daily.ranking.with(|lb| {
                    lb.as_ref()
                        .and_then(|lb| lb.me.as_ref().map(|m| (m.rank, lb.finishers)))
                })
            })
    };
    view! {
        <Card class="items-center gap-3 overflow-hidden px-6 text-center">
            {move || {
                if revealed() {
                    view! {
                        <span class="hanko grid size-16 rotate-[-6deg] place-items-center rounded-lg font-pixel text-4xl">
                            "解"
                        </span>
                        <h2 class="text-xl font-black">{t!(i18n, daily.result.revealed_title)}</h2>
                        <p class="text-sm text-muted-foreground">{t!(i18n, daily.result.revealed_text)}</p>
                    }
                        .into_any()
                } else {
                    view! {
                        <Stamp kanji="完" class="size-20 text-5xl" />
                        <h2 class="text-2xl font-black">"見事！ " {t!(i18n, common.game.well_done)}</h2>
                        <p class="font-pixel text-3xl tabular-nums">{move || format_duration(daily.elapsed_ms())}</p>
                        {move || {
                            daily
                                .flag()
                                .map(|flag| {
                                    let locale = i18n.get_locale();
                                    let reason = match flag {
                                        Flag::Log => td_string!(locale, daily.flag.log),
                                        Flag::TooFast => td_string!(locale, daily.flag.too_fast),
                                        Flag::Pace => td_string!(locale, daily.flag.pace),
                                        Flag::SecondAccount => td_string!(locale, daily.flag.second_account),
                                        // A banned player isn't signed in to see it
                                        Flag::Review | Flag::Banned => td_string!(locale, daily.flag.review),
                                    };
                                    view! {
                                        <p class="rounded-md border border-warning/40 bg-warning-light px-3 py-2 text-start text-sm text-warning-dark">
                                            {reason}
                                        </p>
                                    }
                                })
                        }}
                        <p class="text-sm text-muted-foreground">
                            {move || match (daily.flag(), place()) {
                                (Some(_), _) => t_string!(i18n, daily.flag.title).to_string(),
                                (None, Some((rank, total))) => t_string!(i18n, daily.result.rank, rank, total),
                                (None, None) if daily.finished() && daily.ranking_load.phase.get() != Phase::Error => t_string!(i18n, daily.result.ranking).to_string(),
                                (None, None) => String::new(),
                            }}
                        </p>
                        {move || {
                            place()
                                .filter(|_| daily.flag().is_none())
                                .map(|(rank, total)| {
                                    view! {
                                        <p class="font-pixel text-sm text-primary">
                                            {t_string!(i18n, daily.result.points, points = points(rank, total))} " 点"
                                        </p>
                                    }
                                })
                        }}
                    }
                        .into_any()
                }
            }}
            <div role="status" aria-live="polite" data-testid="daily-submit-status"
                class:hidden=move || daily.submission.get() == Phase::Idle>
                <Show when=move || daily.submission.get() == Phase::Pending>
                    <p>{t!(i18n, daily.recovery.submit_pending)}</p>
                </Show>
                <Show when=move || daily.submission.get() == Phase::Error>
                    <p>{t!(i18n, daily.recovery.submit_error)}</p>
                    <Button variant=ButtonVariant::Outline on:click=move |_| daily.submit(true)>
                        {t!(i18n, daily.recovery.submit_retry)}
                    </Button>
                </Show>
            </div>
            // The hanko the group members pressed on this result
            {move || day.get().map(|today| view! { <ReceivedHanko today /> })}
            <div class="flex w-full flex-col gap-2">
                <Show when=move || daily.finished()><ShareBar text=share_text /></Show>
                {move || {
                    let day = daily.info.with(|i| i.as_ref().map(|i| i.day))?;
                    (!revealed()).then(|| {
                        let href = move || format!("{}/replay/{day}", prefix(i18n.get_locale()));
                        view! {
                            <A
                                href=href
                                attr:class="inline-flex min-h-10 items-center justify-center gap-2 rounded-md border bg-card text-sm font-medium shadow-xs hover:bg-accent"
                                attr:data-testid="watch-mine"
                            >
                                <span class="font-pixel text-primary" aria-hidden="true">"再"</span>
                                {t!(i18n, replay.watch_mine)}
                            </A>
                        }
                    })
                }}
                <A
                    href=practice
                    attr:class="inline-flex min-h-10 items-center justify-center text-sm text-primary underline-offset-4 hover:underline"
                >
                    {t!(i18n, daily.result.more)}
                </A>
            </div>
            {move || {
                auth.user
                    .get()
                    .filter(|_| daily.finished())
                    .filter(|u| u.guest)
                    .map(|u| {
                        view! {
                            <div class="w-full rounded-lg border border-dashed bg-muted/50 p-3 text-start text-sm">
                                <p>{t!(i18n, daily.result.on_board, name = u.name, <b> = <b />)}</p>
                                <A href=account attr:class="font-medium text-primary underline-offset-4 hover:underline">
                                    {t!(i18n, daily.result.claim)}
                                </A>
                            </div>
                        }
                    })
            }}
        </Card>
    }
}
