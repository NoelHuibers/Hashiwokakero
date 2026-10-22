//! The cover over a group puzzle before the viewer starts it: its size in the fog, the rules,
//! and the start button. The puzzle itself only comes with the start, like the daily's.

use common::{Board, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::start_group_puzzle;
use crate::components::board::{BoardMode, BoardView};
use crate::components::ui::button::{Button, ButtonSize};
use crate::components::ui::card::Card;
use crate::i18n::{t, t_string, use_i18n};
use crate::state::error_toast;

use super::puzzle_title;
use super::state::Play;

#[component]
pub fn Cover(play: Play) -> impl IntoView {
    let i18n = use_i18n();
    let size = Signal::derive(move || {
        play.page
            .with(|p| p.as_ref().map(|p| (p.summary.width, p.summary.height)))
            .unwrap_or((7, 7))
    });
    let board = Signal::derive(move || {
        let (w, h) = size.get();
        Board::new(Puzzle::empty(w, h))
    });
    let start = move |_| {
        let Some(id) = play.id() else { return };
        if play.busy.get_untracked() {
            return;
        }
        play.busy.set(true);
        spawn_local(async move {
            let reply = start_group_puzzle(id).await;
            if play.busy.try_set(false).is_some() || play.id() != Some(id) {
                return;
            }
            match reply {
                Ok(page) => play.apply(page),
                Err(e) => error_toast(e),
            }
        });
    };
    let rule = |label: String, cost: String, strong: bool| {
        view! {
            <li class="inline-flex items-center gap-1.5 rounded-full border bg-background/60 py-1 ps-2.5 pe-2 text-xs">
                <span class="text-muted-foreground">{label}</span>
                <span
                    class=if strong { "font-bold text-primary" } else { "font-pixel tabular-nums text-foreground" }
                    dir=(!strong).then_some("ltr")
                >
                    {cost}
                </span>
            </li>
        }
    };
    let name = move || {
        play.page
            .with(|p| {
                p.as_ref()
                    .map(|p| puzzle_title(i18n.get_locale(), &p.summary))
            })
            .unwrap_or_default()
    };
    let group = move || {
        play.page
            .with(|p| p.as_ref().map(|p| p.group_name.clone()))
            .unwrap_or_default()
    };
    let islands = move || {
        play.page
            .with(|p| p.as_ref().map_or(0, |p| p.summary.islands))
    };
    view! {
        <div class="grid place-items-center" data-testid="kadai-cover">
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
                            "挑"
                        </span>
                        <div class="min-w-0">
                            <p class="font-pixel text-xs tracking-[0.3em] text-primary">"挑戦状"</p>
                            <h2 class="break-words text-xl font-black leading-tight sm:text-2xl">{name}</h2>
                        </div>
                    </div>
                    <p class="text-sm leading-relaxed text-muted-foreground">
                        {move || t_string!(i18n, kadai.cover.lead, group = group(), count = islands())}
                    </p>
                    <ul class="flex flex-wrap gap-1.5">
                        {rule(t_string!(i18n, common.game.hint).to_string(), "+30s".into(), false)}
                        {rule(t_string!(i18n, common.game.check).to_string(), "+10s".into(), false)}
                        {rule(t_string!(i18n, kadai.page.give_up).to_string(), t_string!(i18n, kadai.cover.unranked).to_string(), true)}
                    </ul>
                    <Button
                        size=ButtonSize::Lg
                        class="h-12 w-full text-base font-bold shadow-lg shadow-primary/25"
                        attr:disabled=move || play.busy.get()
                        attr:data-testid="kadai-start"
                        on:click=start
                    >
                        {move || if play.busy.get() {
                            t_string!(i18n, kadai.cover.preparing).to_string()
                        } else {
                            format!("{} 始め", t_string!(i18n, kadai.cover.start))
                        }}
                        <svg viewBox="0 0 24 24" class="size-4 rtl:-scale-x-100" fill="currentColor" aria-hidden="true">
                            <path d="M8 5v14l11-7z" />
                        </svg>
                    </Button>
                    <p class="text-center text-xs text-muted-foreground">{t!(i18n, kadai.cover.once)}</p>
                </Card>
            </div>
        </div>
    }
}
