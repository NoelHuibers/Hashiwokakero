//! The hall of fame card of the ranks page: the all time rankings, most solved, most medals and
//! longest streaks.

use leptos::prelude::*;

use crate::components::leaderboard::{PlayerName, RankCell};
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::tabs::{Tabs, TabsContent, TabsList, TabsTrigger};
use crate::i18n::{t, use_i18n};
use crate::model::{AllTimeEntry, HallOfFame as Fame};

/// The tabs, each one ranking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Solved,
    Medals,
    Streaks,
}

/// A medal count, faded when there are none.
#[component]
fn Medals(count: i64, class: &'static str) -> impl IntoView {
    view! {
        <td class="py-2 text-end">
            <span
                class=format!("inline-grid h-6 min-w-6 place-items-center rounded-md px-1 font-pixel text-xs tabular-nums {class}")
                class:opacity-30=count == 0
            >
                {count}
            </span>
        </td>
    }
}

#[component]
fn Ranking(list: Vec<AllTimeEntry>, tab: Tab) -> impl IntoView {
    let i18n = use_i18n();
    if list.is_empty() {
        return view! { <p class="text-sm text-muted-foreground">{t!(i18n, ranks.fame.empty)}</p> }
            .into_any();
    }
    let columns = match tab {
        Tab::Medals => 3,
        _ => 2,
    };
    view! {
        // A size container for PlayerName
        <div class="@container">
        <table class="w-full table-fixed text-sm">
            <colgroup>
                <col class="w-10" />
                <col />
                {(0..columns)
                    .map(|_| view! { <col class=if columns == 3 { "w-10" } else { "w-16" } /> })
                    .collect_view()}
            </colgroup>
            <thead>
                <tr class="border-b text-start text-[10px] uppercase tracking-wider text-muted-foreground">
                    <th></th>
                    <th class="py-1.5 text-start font-semibold">{t!(i18n, ranks.fame.player)}</th>
                    {match tab {
                        Tab::Solved => {
                            view! {
                                <th class="py-1.5 text-end font-semibold">{t!(i18n, account.stats.solved)}</th>
                                <th class="py-1.5 text-end font-semibold">{t!(i18n, account.stats.best_streak_short)}</th>
                            }
                                .into_any()
                        }
                        Tab::Medals => {
                            view! {
                                <th class="py-1.5 text-end font-pixel text-xs">"金"</th>
                                <th class="py-1.5 text-end font-pixel text-xs">"銀"</th>
                                <th class="py-1.5 text-end font-pixel text-xs">"銅"</th>
                            }
                                .into_any()
                        }
                        Tab::Streaks => {
                            view! {
                                <th class="py-1.5 text-end font-semibold">{t!(i18n, ranks.fame.now)}</th>
                                <th class="py-1.5 text-end font-semibold">{t!(i18n, ranks.fame.best)}</th>
                            }
                                .into_any()
                        }
                    }}
                </tr>
            </thead>
            <tbody>
                {list
                    .into_iter()
                    .map(|e| {
                        let numbers = match tab {
                            Tab::Solved => {
                                view! {
                                    <td class="py-2 text-end font-pixel tabular-nums">{e.solved}</td>
                                    <td class="py-2 text-end font-pixel tabular-nums">
                                        <Badge variant=BadgeVariant::Muted>{e.best_streak}</Badge>
                                    </td>
                                }
                                    .into_any()
                            }
                            Tab::Medals => {
                                view! {
                                    <Medals count=e.gold class="bg-[var(--px-gold)] text-[#3a2a05]" />
                                    <Medals count=e.silver class="bg-[#c9cdd4] text-[#2b2f36]" />
                                    <Medals count=e.bronze class="bg-[#d39a6a] text-[#3b2210]" />
                                }
                                    .into_any()
                            }
                            Tab::Streaks => {
                                view! {
                                    <td class="py-2 text-end font-pixel tabular-nums" class:text-muted-foreground=e.current_streak == 0>
                                        {e.current_streak}
                                    </td>
                                    <td class="py-2 text-end font-pixel tabular-nums">
                                        <Badge variant=BadgeVariant::Muted>{e.best_streak}</Badge>
                                    </td>
                                }
                                    .into_any()
                            }
                        };
                        view! {
                            <tr class="border-b last:border-0">
                                <td class="py-2 pe-2"><RankCell rank=e.rank /></td>
                                <td class="max-w-0 py-2 pe-2">
                                    <PlayerName user_id=e.user_id name=e.name guest=e.guest stamp=e.stamp />
                                </td>
                                {numbers}
                            </tr>
                        }
                    })
                    .collect_view()}
            </tbody>
        </table>
        </div>
    }
    .into_any()
}

/// The all time rankings, `None` while they load.
#[component]
pub(super) fn HallOfFame(fame: RwSignal<Option<Fame>>) -> impl IntoView {
    let i18n = use_i18n();
    let tab = move |tab: Tab| {
        move || {
            match fame.get() {
            None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
            Some(f) => {
                let list = match tab {
                    Tab::Solved => f.solved,
                    Tab::Medals => f.medals,
                    Tab::Streaks => f.streaks,
                };
                view! { <Ranking list=list tab=tab /> }.into_any()
            }
        }
        }
    };
    view! {
        <Card class="gap-3 self-start">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-lg">
                    {t!(i18n, ranks.fame.title)} <span class="font-pixel text-sm text-primary">"殿堂"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, ranks.fame.text)}</CardDescription>
            </CardHeader>
            <CardContent>
                <Tabs default_value="solved" class="w-full">
                    <TabsList class="w-full">
                        <TabsTrigger value="solved">{t!(i18n, ranks.fame.solved)}</TabsTrigger>
                        <TabsTrigger value="medals">{t!(i18n, ranks.fame.medals)}</TabsTrigger>
                        <TabsTrigger value="streaks">{t!(i18n, ranks.fame.streaks)}</TabsTrigger>
                    </TabsList>
                    <TabsContent value="solved" class="pt-2">{tab(Tab::Solved)}</TabsContent>
                    <TabsContent value="medals" class="flex flex-col gap-2 pt-2">
                        {tab(Tab::Medals)}
                        <p class="text-xs text-muted-foreground">{t!(i18n, ranks.fame.medals_text)}</p>
                    </TabsContent>
                    <TabsContent value="streaks" class="pt-2">{tab(Tab::Streaks)}</TabsContent>
                </Tabs>
            </CardContent>
        </Card>
    }
}
