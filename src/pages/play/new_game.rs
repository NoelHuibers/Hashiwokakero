//! Picking the difficulty of the next practice puzzle and generating it.

use common::{Difficulty, GenerateParams};
use leptos::prelude::*;

use crate::components::ui::button::Button;
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::difficulty_name;
use crate::state::{self, Game, Settings, Source, random_seed};

pub(super) fn new_random(game: &Game, difficulty: Difficulty) {
    let (width, height) = difficulty.size();
    let params = GenerateParams {
        width,
        height,
        difficulty,
        seed: random_seed(),
    };
    state::new_puzzle(*game, params, Source::Generated(params));
}

#[component]
fn DifficultyPicker() -> impl IntoView {
    let i18n = use_i18n();
    let settings = expect_context::<Settings>();
    view! {
        <div class="grid grid-cols-5 gap-1.5">
            {Difficulty::ALL
                .into_iter()
                .map(|d| {
                    let (w, h) = d.size();
                    view! {
                        <button
                            class=move || {
                                format!(
                                    "flex flex-col items-center rounded-lg border px-1 py-2 transition-all hover:-translate-y-0.5 {}",
                                    if settings.difficulty.get() == d {
                                        "border-primary bg-primary text-primary-foreground shadow-md"
                                    } else {
                                        "bg-background hover:border-primary/50"
                                    },
                                )
                            }
                            title=move || format!("{} · {w}×{h}", difficulty_name(i18n.get_locale(), d))
                            on:click=move |_| settings.difficulty.set(d)
                        >
                            <span class="font-pixel text-lg leading-none">{d.kanji()}</span>
                            <span class="mt-1 max-w-full truncate text-[10px] font-bold uppercase tracking-wider">
                                {move || difficulty_name(i18n.get_locale(), d)}
                            </span>
                            <span class="text-[10px] opacity-70">{format!("{w}×{h}")}</span>
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
pub(super) fn NewGameCard() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<Game>();
    let settings = expect_context::<Settings>();
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, practice.card.title)} <span class="font-pixel text-sm text-primary">"練習"</span>
                </CardTitle>
                <CardDescription>
                    {t!(i18n, practice.card.text)}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                <DifficultyPicker />
                <div class="flex gap-2">
                    <Button
                        class="flex-1"
                        attr:disabled=move || game.loading.get()
                        on:click=move |_| new_random(&game, settings.difficulty.get_untracked())
                    >
                        {move || if game.loading.get() { t_string!(i18n, practice.generating).to_string() } else { format!("{} 生成", t_string!(i18n, practice.generate)) }}
                    </Button>
                </div>
            </CardContent>
        </Card>
    }
}
