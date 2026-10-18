//! What covers the practice board once it is solved or revealed.

use leptos::prelude::*;

use crate::components::celebration::{Petals, Stamp};
use crate::components::share::ShareBar;
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::Card;
use crate::i18n::{t, td_string, use_i18n};
use crate::lang::localized;
use crate::state::{Game, Settings, Status, format_time};

use super::new_game::new_random;

#[component]
pub(super) fn WinOverlay() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    let dismissed = RwSignal::new(false);
    Effect::new(move |_| {
        game.epoch.track();
        game.status.track();
        dismissed.set(false);
    });

    let share_text = move || {
        let locale = i18n.get_locale_untracked();
        format!(
            "🌉 Hashi · {} · {} · 橋をかけろ\n{}",
            game.source.get_untracked().title(locale),
            td_string!(
                locale,
                practice.share,
                time = format_time(game.elapsed.get_untracked()),
                moves = game.moves.get_untracked(),
                hints = game.hints.get_untracked()
            ),
            window().location().origin().unwrap_or_default() + &localized(locale, "/practice"),
        )
    };

    view! {
        <Show when=move || game.status.get() == Status::Won && !dismissed.get()>
            <Petals />
            <div class="absolute inset-0 z-10 grid place-items-center p-4">
                <Card class="w-full max-w-sm items-center gap-3 px-6 text-center shadow-2xl animate-in zoom-in-95 fade-in">
                    <Stamp kanji="完" class="size-20 text-5xl" />
                    <h2 class="text-2xl font-black">"見事！ " {t!(i18n, common.game.well_done)}</h2>
                    <p class="text-muted-foreground">
                        {t!(
                            i18n,
                            practice.won,
                            time = move || format_time(game.elapsed.get()),
                            moves = move || game.moves.get(),
                            hints = move || game.hints.get(),
                            <b> = <span class="font-pixel text-foreground" />
                        )}
                    </p>
                    <div class="flex w-full flex-wrap justify-center gap-2 pt-1">
                        <Button on:click=move |_| new_random(&game, settings.difficulty.get_untracked())>
                            {t!(i18n, practice.next)} " 次へ"
                        </Button>
                        <Button variant=ButtonVariant::Ghost on:click=move |_| dismissed.set(true)>
                            {t!(i18n, practice.admire)}
                        </Button>
                        <ShareBar text=share_text outline=true />
                    </div>
                </Card>
            </div>
        </Show>
        <Show when=move || game.status.get() == Status::Revealed && !dismissed.get()>
            // Below the board on phones, where it would cover the bottom rows
            <div class="relative z-10 mt-3 flex justify-center sm:absolute sm:inset-x-0 sm:bottom-3 sm:mt-0 sm:px-4">
                <div class="flex flex-wrap items-center justify-center gap-3 rounded-xl border bg-card/95 px-4 py-2 text-sm shadow-xl backdrop-blur animate-in slide-in-from-bottom-4 fade-in">
                    <span class="hanko grid size-9 shrink-0 rotate-[-6deg] place-items-center rounded-md font-pixel text-xl">
                        "解"
                    </span>
                    <span>{t!(i18n, practice.revealed)}</span>
                    <Button size=ButtonSize::Sm on:click=move |_| game.restart()>
                        {t!(i18n, practice.play_myself)}
                    </Button>
                    <Button
                        size=ButtonSize::Sm
                        variant=ButtonVariant::Outline
                        on:click=move |_| new_random(&game, settings.difficulty.get_untracked())
                    >
                        {t!(i18n, practice.new_puzzle)}
                    </Button>
                    <Button size=ButtonSize::Sm variant=ButtonVariant::Ghost on:click=move |_| dismissed.set(true)>
                        "✕"
                    </Button>
                </div>
            </div>
        </Show>
    }
}
