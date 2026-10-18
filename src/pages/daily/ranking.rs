//! Today's top ten next to the board.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::layout::use_localized;
use crate::components::leaderboard::LeaderboardTable;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};

use super::state::Daily;

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
                            .with(|lb| lb.as_ref().map(|lb| lb.finishers)).map_or_else(|| t_string!(i18n, common.game.loading).to_string(), |count| t_string!(i18n, daily.ranking.crossed, count))
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent>
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
