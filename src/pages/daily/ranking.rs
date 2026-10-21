//! Today's top ten next to the board.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::layout::use_localized;
use crate::components::leaderboard::LeaderboardTable;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};

use super::recovery::Phase;
use super::state::Daily;
use crate::components::ui::button::{Button, ButtonVariant};

#[component]
pub(super) fn RankingCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let ranks = use_localized("/ranks");
    view! {
        <Card class="gap-3">
            <CardHeader>
                <div class="flex w-full items-baseline justify-between">
                    <CardTitle class="flex items-baseline gap-2 text-lg">
                        {t!(i18n, daily.ranking.title)} <span class="font-pixel text-sm text-primary">"順位"</span>
                    </CardTitle>
                    <A
                        href=ranks
                        attr:class="-me-2 inline-flex min-h-9 items-center px-2 text-xs text-primary underline-offset-4 hover:underline"
                    >
                        {t!(i18n, daily.ranking.all)}
                    </A>
                </div>
                <CardDescription>
                    {move || {
                        daily
                            .ranking
                            .with(|lb| lb.as_ref().map(|lb| lb.finishers)).map_or_else(String::new, |count| t_string!(i18n, daily.ranking.crossed, count))
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent>
                <div role="status" aria-live="polite" data-testid="daily-ranking-status">
                    <Show when=move || daily.ranking_load.phase.get() == Phase::Pending || (daily.ranking.with(Option::is_none) && daily.ranking_load.phase.get() != Phase::Error)>
                        <p>{t!(i18n, common.game.loading)}</p>
                    </Show>
                    <Show when=move || daily.ranking_load.phase.get() == Phase::Error>
                        <p>{t!(i18n, daily.recovery.ranking_error)}</p>
                        <Button variant=ButtonVariant::Outline on:click=move |_| daily.retry_ranking()>
                            {t!(i18n, daily.recovery.ranking_retry)}
                        </Button>
                    </Show>
                </div>
                {move || {
                    daily
                        .ranking
                        .get()
                        .map(|lb| view! { <LeaderboardTable entries=lb.entries me=lb.me limit=10 /> })
                }}
            </CardContent>
        </Card>
    }
}
