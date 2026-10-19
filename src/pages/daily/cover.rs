//! The cover over the board before the daily is started.

use common::{Board, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::start_daily;
use crate::components::board::{BoardMode, BoardView};
use crate::components::ui::button::{Button, ButtonSize};
use crate::components::ui::card::Card;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::error_text;
use crate::model::daily_difficulty;
use crate::state::{Auth, ToastKind, device_id, toast};

use super::state::Daily;

/// The board before the start: an empty sea in the fog.
#[component]
pub(super) fn Cover(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let size = Signal::derive(move || {
        let day = daily.info.with(|i| i.as_ref().map(|i| i.day)).unwrap_or(0);
        daily_difficulty(day).size()
    });
    let board = Signal::derive(move || {
        let (w, h) = size.get();
        Board::new(Puzzle::empty(w, h))
    });
    let start = move |_| {
        if daily.busy.get_untracked() {
            return;
        }
        daily.busy.set(true);
        spawn_local(async move {
            match start_daily(Some(device_id())).await {
                Ok(info) => {
                    daily.apply(info);
                    auth.refresh();
                }
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
            daily.busy.set(false);
        });
    };
    // A penalty as a small tag: what you do and what it costs.
    let rule = |label: String, cost: String, strong: bool| {
        view! {
            <li class="inline-flex items-center gap-1.5 rounded-full border bg-background/60 py-1 ps-2.5 pe-2 text-xs">
                <span class="text-muted-foreground">{label}</span>
                // dir keeps "+30s" from turning into "30s+" in right-to-left languages
                <span
                    class=if strong { "font-bold text-primary" } else { "font-pixel tabular-nums text-foreground" }
                    dir=(!strong).then_some("ltr")
                >
                    {cost}
                </span>
            </li>
        }
    };
    view! {
        // Board and card share one grid cell, so a card taller than the board
        // (narrow screens) grows the cell instead of spilling over what follows.
        <div class="grid place-items-center">
            <div class="col-start-1 row-start-1 w-full opacity-50 blur-[3px]" aria-hidden="true">
                <BoardView
                    board=board
                    bridges=Signal::derive(move || board.with(Board::empty_bridges))
                    mode=BoardMode::View
                    framed=true
                />
            </div>
            <div class="relative col-start-1 row-start-1 grid w-full place-items-center p-4 sm:p-8">
                <Card class="w-full max-w-sm gap-4 border-border/80 bg-card/90 px-5 py-5 shadow-2xl backdrop-blur-md sm:gap-5 sm:px-6 sm:py-6">
                    <div class="flex items-center gap-3">
                        <span class="hanko grid size-11 shrink-0 rotate-[-6deg] place-items-center rounded-lg font-pixel text-2xl">
                            "橋"
                        </span>
                        <div class="min-w-0">
                            <p class="font-pixel text-xs tracking-[0.3em] text-primary">"今日の橋"</p>
                            <h2 class="text-xl font-black leading-tight sm:text-2xl">{t!(i18n, daily.cover.title)}</h2>
                        </div>
                    </div>
                    <p class="text-sm leading-relaxed text-muted-foreground">{t!(i18n, daily.cover.lead)}</p>
                    <ul class="flex flex-wrap gap-1.5">
                        {rule(t_string!(i18n, common.game.hint).to_string(), "+30s".into(), false)}
                        {rule(t_string!(i18n, common.game.check).to_string(), "+10s".into(), false)}
                        {rule(t_string!(i18n, daily.give_up).to_string(), t_string!(i18n, daily.cover.unranked).to_string(), true)}
                    </ul>
                    <div class="flex flex-col gap-2.5">
                        <Button
                            size=ButtonSize::Lg
                            class="h-12 w-full text-base font-bold shadow-lg shadow-primary/25"
                            attr:disabled=move || daily.busy.get() || daily.info.with(Option::is_none)
                            on:click=start
                        >
                            {move || if daily.busy.get() {
                                t_string!(i18n, daily.cover.preparing).to_string()
                            } else {
                                format!("{} 始め", t_string!(i18n, daily.cover.start))
                            }}
                            <svg viewBox="0 0 24 24" class="size-4 rtl:-scale-x-100" fill="currentColor" aria-hidden="true">
                                <path d="M8 5v14l11-7z" />
                            </svg>
                        </Button>
                        <p class="text-center text-xs text-muted-foreground">
                            {move || match auth.user.get() {
                                Some(u) if !u.guest => t_string!(i18n, daily.cover.playing_as, name = u.name),
                                Some(u) => t_string!(i18n, daily.cover.playing_as_guest, name = u.name),
                                None => t_string!(i18n, daily.cover.no_account).to_string(),
                            }}
                        </p>
                    </div>
                </Card>
            </div>
        </div>
    }
}
