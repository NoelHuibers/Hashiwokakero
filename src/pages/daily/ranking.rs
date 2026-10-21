//! Today's top ten next to the board, and a tab for each of the player's groups with the group's
//! board of today (spoiler safe until the player finished, see `pages::groups::today`).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::group_boards;
use crate::components::layout::use_localized;
use crate::components::leaderboard::LeaderboardTable;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::prefix;
use crate::model::GroupToday;
use crate::pages::groups::today::TodayBoard;
use crate::state::Auth;

use super::state::Daily;

/// The tabs: the world's ranking (`None`) and one per group.
#[component]
fn TabBar(boards: Vec<GroupToday>, tab: RwSignal<Option<i64>>) -> impl IntoView {
    let i18n = use_i18n();
    let button = move |value: Option<i64>, label: AnyView| {
        let selected = move || tab.get() == value;
        view! {
            <button
                type="button"
                role="tab"
                aria-selected=move || selected().to_string()
                class="relative flex shrink-0 items-center gap-1.5 rounded-md px-2.5 py-1 text-sm font-medium whitespace-nowrap transition-colors focus-visible:outline-2 focus-visible:outline-ring"
                class=(["bg-background", "text-foreground", "shadow-sm"], selected)
                class=(["text-foreground/60", "hover:text-foreground"], move || !selected())
                on:click=move |_| tab.set(value)
            >
                {label}
            </button>
        }
    };
    view! {
        <div
            role="tablist"
            aria-label=move || t_string!(i18n, groups.daily.tabs)
            class="flex max-w-full gap-1 overflow-x-auto rounded-lg bg-muted p-[3px]"
        >
            {button(None, view! { <span class="font-pixel">"世界"</span> {t!(i18n, groups.daily.world)} }.into_any())}
            {boards
                .into_iter()
                .map(|b| {
                    let done = b.done();
                    let total = b.entries.len();
                    button(
                        Some(b.group_id),
                        view! {
                            <span class="max-w-36 truncate">{b.group_name}</span>
                            <span class="font-pixel text-xs opacity-70">{format!("{done}/{total}")}</span>
                        }
                            .into_any(),
                    )
                })
                .collect_view()}
        </div>
    }
}

#[component]
pub(super) fn RankingCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let ranks = use_localized("/ranks");
    let groups = use_localized("/groups");
    // `None` until loaded, then the boards of the player's groups (none without one)
    let boards = RwSignal::new(None::<Vec<GroupToday>>);
    let tab = RwSignal::new(None::<i64>);

    // Along with the ranking: on load and after the finish, which opens the boards
    Effect::new(move |_| {
        daily.ranking.track();
        let _ = auth.user.with(|u| u.as_ref().map(|u| u.id));
        spawn_local(async move {
            if let Ok(list) = group_boards().await {
                boards.set(Some(list));
            }
        });
    });

    let board = move || {
        let id = tab.get()?;
        boards.with(|b| b.as_ref()?.iter().find(|b| b.group_id == id).cloned())
    };
    let me = move || auth.user.with(|u| u.as_ref().map(|u| u.id));
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
                    {move || match board() {
                        Some(b) => td_string!(i18n.get_locale(), groups.daily.done, done = b.done(), total = b.entries.len()),
                        None => {
                            daily
                                .ranking
                                .with(|lb| lb.as_ref().map(|lb| lb.finishers))
                                .map_or_else(|| t_string!(i18n, common.game.loading).to_string(), |count| t_string!(i18n, daily.ranking.crossed, count))
                        }
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || {
                    boards
                        .get()
                        .filter(|b| !b.is_empty())
                        .map(|b| view! { <TabBar boards=b tab /> })
                }}
                <div role="tabpanel">
                    {move || match board() {
                        Some(b) => {
                            let id = b.group_id;
                            let href = move || format!("{}/groups/{id}", prefix(i18n.get_locale()));
                            view! {
                                <div class="flex flex-col gap-2">
                                    <TodayBoard board=b me=me() />
                                    <A href=href attr:class="w-fit text-xs text-primary underline-offset-4 hover:underline">
                                        {t!(i18n, groups.daily.open)} " →"
                                    </A>
                                </div>
                            }
                                .into_any()
                        }
                        None => {
                            daily
                                .ranking
                                .get()
                                .map(|lb| view! { <LeaderboardTable entries=lb.entries me=lb.me limit=10 /> })
                                .into_any()
                        }
                    }}
                </div>
                {move || {
                    boards
                        .with(|b| b.as_ref().is_some_and(Vec::is_empty))
                        .then(|| {
                            view! {
                                <A
                                    href=groups
                                    attr:class="flex items-center gap-2 rounded-lg border border-dashed px-3 py-2 text-sm text-muted-foreground transition-colors hover:border-primary/60 hover:text-foreground"
                                    attr:data-testid="compare-with-friends"
                                >
                                    <span class="font-pixel text-primary">"仲間"</span>
                                    <span>{t!(i18n, groups.daily.compare, <b> = <b class="font-medium text-primary" />)}</span>
                                </A>
                            }
                        })
                }}
            </CardContent>
        </Card>
    }
}
