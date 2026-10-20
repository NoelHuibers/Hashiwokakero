//! The Dojo's leaderboard, the banzuke: every ranked solver by PAR-2, by answer and by what it
//! adds to the field, and the season's cactus plot.

use leptos::prelude::*;

use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::model::DojoBoard;

use super::views::BoardViews;

#[component]
pub(super) fn Leaderboard(board: RwSignal<Option<DojoBoard>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Card class="gap-4" attr:id="leaderboard">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, dojo.board.title)} " " <span class="font-pixel text-sm text-primary">"番付 · banzuke"</span>
                </CardTitle>
            </CardHeader>
            <CardContent>
                {move || match board.get() {
                    None => view! { <p class="py-8 text-center text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(b) if b.entries.is_empty() => {
                        view! {
                            <div class="seigaiha grid h-40 place-items-center rounded-lg border border-dashed text-muted-foreground">
                                <p class="rounded-md bg-card px-3 py-1">{t!(i18n, dojo.board.empty)}</p>
                            </div>
                        }
                            .into_any()
                    }
                    Some(b) => view! { <BoardViews entries=b.entries portfolio=b.portfolio families=true /> }.into_any(),
                }}
            </CardContent>
        </Card>
    }
}
