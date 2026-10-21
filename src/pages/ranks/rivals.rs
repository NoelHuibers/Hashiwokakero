//! The rivals card of the ranks page: you and your rivals side by side, solved dailies, medals,
//! streaks and the head-to-head record against each.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::load_rivals_board;
use crate::components::leaderboard::{HIGHLIGHT_ROW, PlayerName, RANK_TABLE, ROW_INSET};
use crate::components::rivals::{RecentDuels, Score};
use crate::components::sprites::{PixelSprite, SWORDS};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::model::{RivalRow, RivalsBoard};
use crate::state::{Auth, Rivals, error_toast};

/// The gold, silver and bronze medals of a row, faded when there are none.
#[component]
fn Medals(gold: i64, silver: i64, bronze: i64) -> impl IntoView {
    let chip = |count: i64, kanji: &'static str, class: &'static str| {
        view! {
            <span
                class=format!(
                    "inline-flex h-6 min-w-8 items-center justify-center gap-0.5 rounded-md px-1 font-pixel text-xs tabular-nums {class}",
                )
                class:opacity-30=count == 0
                aria-label=format!("{kanji} {count}")
            >
                <span class="text-[10px]">{kanji}</span>
                {count}
            </span>
        }
    };
    view! {
        <span class="inline-flex justify-end gap-1">
            {chip(gold, "金", "bg-[var(--px-gold)] text-[#3a2a05]")}
            {chip(silver, "銀", "bg-[#c9cdd4] text-[#2b2f36]")}
            {chip(bronze, "銅", "bg-[#d39a6a] text-[#3b2210]")}
        </span>
    }
}

#[component]
fn Row(row: RivalRow) -> impl IntoView {
    let RivalRow { entry, me, record } = row;
    view! {
        <tr class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" })>
            <td class="py-2 pe-2 text-center font-pixel text-sm tabular-nums text-muted-foreground">{entry.rank}</td>
            <td class="max-w-0 py-2 pe-2">
                <PlayerName user_id=entry.user_id name=entry.name guest=entry.guest stamp=entry.stamp me=me />
            </td>
            <td class="py-2 pe-2 text-end font-pixel tabular-nums">{entry.solved}</td>
            <td class="hidden py-2 pe-2 text-end sm:table-cell">
                <Medals gold=entry.gold silver=entry.silver bronze=entry.bronze />
            </td>
            <td class="py-2 pe-2 text-end font-pixel tabular-nums" class:text-muted-foreground=entry.current_streak == 0>
                {entry.current_streak}
            </td>
            <td class="py-2 text-end">
                {match record {
                    Some(record) => {
                        let (wins, losses, draws) = (record.wins, record.losses, record.draws);
                        view! {
                            <div class="flex items-center justify-end gap-2">
                                <span class="hidden @3xl:block">
                                    <RecentDuels record />
                                </span>
                                <Score wins losses draws />
                            </div>
                        }
                            .into_any()
                    }
                    None => view! { <span class="text-muted-foreground">"–"</span> }.into_any(),
                }}
            </td>
        </tr>
    }
}

#[component]
fn Board(board: RivalsBoard) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        // A size container for PlayerName and the last duels, pulled out by the row inset
        <div class="@container -mx-2">
            <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                // Widths on the head cells, not a colgroup: the medals column hides on phones
                <thead>
                    <tr class=format!("text-start text-[10px] uppercase tracking-wider text-muted-foreground {ROW_INSET}")>
                        <th class="w-10"></th>
                        <th class="py-1.5 text-start font-semibold">{t!(i18n, ranks.fame.player)}</th>
                        <th class="w-16 py-1.5 pe-2 text-end font-semibold">{t!(i18n, account.stats.solved)}</th>
                        <th class="hidden w-36 py-1.5 pe-2 text-end font-semibold sm:table-cell">{t!(i18n, ranks.fame.medals)}</th>
                        <th class="w-16 py-1.5 pe-2 text-end font-semibold">{t!(i18n, rivals.board.streak)}</th>
                        <th class="w-20 py-1.5 text-end font-semibold @3xl:w-64">{t!(i18n, rivals.record.short)}</th>
                    </tr>
                </thead>
                <tbody>{board.rows.into_iter().map(|row| view! { <Row row /> }).collect_view()}</tbody>
            </table>
        </div>
    }
}

/// You and your rivals, or how to get some.
#[component]
pub(super) fn RivalsCard() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let rivals = expect_context::<Rivals>();
    // `None` while it loads, `Some(None)` without a session
    let board = RwSignal::new(None::<Option<RivalsBoard>>);

    // Again whenever the rivals or the player change (browser only)
    Effect::new(move |_| {
        rivals.ids.track();
        auth.user.track();
        if !auth.loaded.get() {
            return;
        }
        spawn_local(async move {
            match load_rivals_board().await {
                Ok(b) => board.set(Some(b)),
                Err(e) => error_toast(e),
            }
        });
    });

    view! {
        <Card class="gap-4" attr:data-rivals-board="">
            <CardHeader>
                <div class="flex items-center gap-3">
                    <span class="hanko grid size-12 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-2xl">
                        "競"
                    </span>
                    <div>
                        <CardTitle class="text-xl">{t!(i18n, rivals.title)} <span class="font-pixel text-sm text-primary">" ライバル"</span></CardTitle>
                        <CardDescription>{t!(i18n, rivals.board.text)}</CardDescription>
                    </div>
                </div>
            </CardHeader>
            <CardContent>
                {move || match board.get() {
                    None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(Some(b)) if b.rows.len() > 1 => view! { <Board board=b /> }.into_any(),
                    Some(_) => {
                        view! {
                            <div class="seigaiha flex items-center gap-3 rounded-lg border border-dashed p-3 text-sm text-muted-foreground">
                                <span class="grid size-10 shrink-0 place-items-center rounded-lg bg-card shadow-xs">
                                    <PixelSprite art=&SWORDS class="size-6" />
                                </span>
                                <p class="rounded-md bg-card px-2 py-1">{t!(i18n, rivals.board.empty)}</p>
                            </div>
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}
