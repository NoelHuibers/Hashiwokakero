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
    view! {
        <div class="relative">
            <div class="opacity-60 blur-[2px]">
                <BoardView
                    board=board
                    bridges=Signal::derive(move || board.with(Board::empty_bridges))
                    mode=BoardMode::View
                    framed=true
                />
            </div>
            <div class="absolute inset-0 grid place-items-center p-4">
                <Card class="w-full max-w-sm items-center gap-3 px-6 text-center shadow-2xl">
                    <span class="hanko grid size-16 rotate-[-6deg] place-items-center rounded-lg font-pixel text-4xl">
                        "橋"
                    </span>
                    <h2 class="text-2xl font-black">{t!(i18n, daily.cover.title)} " 今日の橋"</h2>
                    <p class="text-sm text-muted-foreground">
                        {t!(i18n, daily.cover.text)}
                    </p>
                    <Button
                        size=ButtonSize::Lg
                        class="w-full text-base"
                        attr:disabled=move || daily.busy.get() || daily.info.with(Option::is_none)
                        on:click=start
                    >
                        {move || if daily.busy.get() { t_string!(i18n, daily.cover.preparing).to_string() } else { format!("{} 始め", t_string!(i18n, daily.cover.start)) }}
                    </Button>
                    <p class="text-xs text-muted-foreground">
                        {move || match auth.user.get() {
                            Some(u) if !u.guest => t_string!(i18n, daily.cover.playing_as, name = u.name),
                            Some(u) => t_string!(i18n, daily.cover.playing_as_guest, name = u.name),
                            None => t_string!(i18n, daily.cover.no_account).to_string(),
                        }}
                    </p>
                </Card>
            </div>
        </div>
    }
}
