//! Everyone who solved a shared puzzle, the fastest first: the leaderboard's rows (medals, names
//! with their stamps, "you"), the sharer marked.

use leptos::prelude::*;

use crate::components::leaderboard::{HIGHLIGHT_ROW, PlayerName, RANK_TABLE, ROW_INSET, RankCell};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::model::{SharedEntry, SharedPuzzlePage, share_clock};

#[component]
fn Row(entry: SharedEntry, me: bool, sharer: bool) -> impl IntoView {
    let i18n = use_i18n();
    let SharedEntry {
        rank,
        user_id,
        name,
        guest,
        stamp,
        hints,
        score_ms,
        ..
    } = entry;
    view! {
        <tr
            class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" })
            data-user=user_id
            data-sharer=sharer.then_some("")
        >
            <td class="py-2 pe-2"><RankCell rank=rank /></td>
            <td class="max-w-0 py-2 pe-2">
                <div class="flex min-w-0 items-center gap-2 [&>div]:min-w-0">
                    <PlayerName user_id name guest stamp me marked_row=true />
                    {sharer.then(|| view! {
                        <span class="shrink-0 rounded-sm bg-primary/10 px-1 font-pixel text-[10px] text-primary" data-tooltip=move || t_string!(i18n, shared.board.sharer)>
                            "共"
                            <span class="sr-only">" " {t!(i18n, shared.board.sharer)}</span>
                        </span>
                    })}
                </div>
            </td>
            <td
                class="py-2 pe-2 text-end text-xs text-muted-foreground"
                data-tooltip=move || (hints > 0).then(|| t_string!(i18n, common.ranking.penalties))
            >
                {(hints > 0).then(|| format!("{hints}h"))}
            </td>
            <td class="py-2 text-end font-pixel tabular-nums" class:text-primary=me>{share_clock(score_ms)}</td>
        </tr>
    }
}

/// The board card next to the puzzle.
#[component]
pub fn BoardCard(page: RwSignal<SharedPuzzlePage>) -> impl IntoView {
    let i18n = use_i18n();
    let rows = Memo::new(move |_| {
        page.with(|p| {
            let me = p.mine.as_ref().map(|m| m.user_id);
            let sharer = p.sharer.as_ref().map(|s| s.user_id);
            let mut entries = p.board.clone();
            // The viewer's own place, also further down
            if let Some(mine) = &p.mine
                && !entries.iter().any(|e| e.user_id == mine.user_id)
            {
                entries.push(mine.clone());
            }
            (entries, me, sharer, p.solvers)
        })
    });
    view! {
        <Card class="gap-3" attr:data-testid="shared-board">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, shared.board.title)} <span class="font-pixel text-sm text-primary">"順位"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, shared.board.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                {move || {
                    let (entries, me, sharer, solvers) = rows.get();
                    if entries.is_empty() {
                        return view! {
                            <div class="seigaiha grid h-24 place-items-center rounded-lg border border-dashed text-sm text-muted-foreground">
                                <p class="rounded-md bg-card px-3 py-1">{t!(i18n, shared.board.empty)} " 一番乗り"</p>
                            </div>
                        }
                            .into_any();
                    }
                    view! {
                        <p class="text-xs text-muted-foreground" data-testid="shared-solvers">
                            {t_string!(i18n, shared.board.solvers, count = solvers)}
                        </p>
                        <div class="@container -mx-2">
                            <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                                <colgroup>
                                    <col class="w-12" />
                                    <col />
                                    <col class="w-12" />
                                    <col class="w-20" />
                                </colgroup>
                                <tbody>
                                    {entries
                                        .into_iter()
                                        .map(|e| {
                                            let mine = Some(e.user_id) == me;
                                            let shared_it = Some(e.user_id) == sharer;
                                            view! { <Row entry=e me=mine sharer=shared_it /> }
                                        })
                                        .collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }
                        .into_any()
                }}
            </CardContent>
        </Card>
    }
}
