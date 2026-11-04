//! The controls of a game timed on the server (a group puzzle, a race).
//!
//! Undo, redo, clear, the paid hint and check, and giving up. What a hint or check costs and how it reaches the server is
//! the page's; the board shows the answers the same way everywhere ([`show_hint`],
//! [`show_check`]).

use std::time::Duration;

use leptos::prelude::*;

use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::dialog::ConfirmDialog;
use crate::i18n::{t, td_string, use_i18n};
use crate::model::HelpPenalties;
use crate::state::{Game, ToastKind, toast};

/// Shows the bridge a hint fixed, marked for a few seconds, or says there was nothing to fix.
pub fn show_hint(game: Game, hint: Option<(usize, u8)>) {
    let Some((edge, value)) = hint else {
        toast(ToastKind::Info, |l| {
            td_string!(l, common.toast.nothing_to_hint).to_string()
        });
        return;
    };
    game.set_edges(&[(edge, value)]);
    game.hints.update(|h| *h += 1);
    game.hint.set(Some(edge));
    set_timeout(
        move || {
            if game.hint.try_get_untracked() == Some(Some(edge)) {
                game.hint.set(None);
            }
        },
        Duration::from_millis(3000),
    );
}

/// Shows what a check found: all right, or the wrong bridges marked for a moment.
pub fn show_check(game: Game, wrong: Vec<usize>) {
    if wrong.is_empty() {
        crate::audio::play(crate::audio::Sfx::Correct);
        toast(ToastKind::Success, |l| {
            td_string!(l, common.toast.all_correct).to_string()
        });
        return;
    }
    let count = wrong.len();
    toast(ToastKind::Error, move |l| {
        td_string!(l, common.toast.wrong_bridges, count)
    });
    game.wrong.set(wrong);
    set_timeout(
        move || {
            game.wrong.try_set(vec![]);
        },
        Duration::from_millis(2400),
    );
}

/// The row of controls under the board. `playing`: the buttons work; the hint and check go to
/// `on_hint` and `on_check`, giving up asks `give_up_question` first.
#[component]
pub fn PaidControls(
    game: Game,
    #[prop(into)] playing: Signal<bool>,
    #[prop(into)] busy: Signal<bool>,
    #[prop(into)] penalties: Signal<HelpPenalties>,
    on_hint: Callback<()>,
    on_check: Callback<()>,
    on_give_up: Callback<()>,
    #[prop(into)] give_up: Signal<String>,
    #[prop(into)] give_up_question: Signal<String>,
) -> impl IntoView {
    let i18n = use_i18n();
    let giving_up = RwSignal::new(false);
    let price = move |ms: i64| format!("+{}s", ms / 1000);
    view! {
        <div class="grid grid-cols-3 gap-2 sm:flex sm:flex-wrap" aria-busy=move || busy.get().to_string()>
            <Button
                variant=ButtonVariant::Outline
                attr:disabled=move || !playing.get() || game.undo_stack.with(Vec::is_empty)
                on:click=move |_| game.undo()
            >
                "↶ " {t!(i18n, common.game.undo)}
            </Button>
            <Button
                variant=ButtonVariant::Outline
                attr:disabled=move || !playing.get() || game.redo_stack.with(Vec::is_empty)
                on:click=move |_| game.redo()
            >
                {t!(i18n, common.game.redo)} " ↷"
            </Button>
            <Button variant=ButtonVariant::Ghost attr:disabled=move || !playing.get() on:click=move |_| game.restart()>
                {t!(i18n, common.game.clear)}
            </Button>
            <div class="contents sm:ms-auto sm:flex sm:gap-2">
                <Button
                    variant=ButtonVariant::Secondary
                    attr:disabled=move || !playing.get()
                    attr:data-tooltip=move || format!("+{} s", penalties.get().check_ms / 1000)
                    on:click=move |_| on_check.run(())
                >
                    {t!(i18n, common.game.check)} <span class="hidden sm:inline">" 確認"</span>
                    <span class="text-xs opacity-70">{move || price(penalties.get().check_ms)}</span>
                </Button>
                <Button
                    variant=ButtonVariant::Secondary
                    attr:disabled=move || !playing.get()
                    attr:data-tooltip=move || format!("+{} s", penalties.get().hint_ms / 1000)
                    on:click=move |_| on_hint.run(())
                >
                    {t!(i18n, common.game.hint)} <span class="hidden sm:inline">" 助言"</span>
                    <span class="text-xs opacity-70">{move || price(penalties.get().hint_ms)}</span>
                </Button>
                <Button
                    variant=ButtonVariant::Ghost
                    attr:disabled=move || !playing.get()
                    attr:aria-haspopup="dialog"
                    on:click=move |_| giving_up.set(true)
                >
                    {move || give_up.get()}
                </Button>
                <ConfirmDialog
                    open=giving_up
                    question=give_up_question
                    yes=give_up
                    on_yes=move || on_give_up.run(())
                />
            </div>
        </div>
    }
}
