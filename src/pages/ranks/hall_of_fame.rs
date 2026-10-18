//! The hall of fame card of the ranks page: the all time ranking of daily players.

use leptos::prelude::*;

use crate::components::leaderboard::RankCell;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::model::AllTimeEntry;

/// The all time ranking, `None` while it loads.
#[component]
pub(super) fn HallOfFame(all_time: RwSignal<Option<Vec<AllTimeEntry>>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
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
    }
}
