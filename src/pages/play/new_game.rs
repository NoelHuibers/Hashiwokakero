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
        <div class="grid grid-cols-5 gap-1 sm:gap-1.5" role="group" aria-label=move || t_string!(i18n, practice.new_puzzle)>
            {Difficulty::ALL
                .into_iter()
                .map(|d| {
                    let (w, h) = d.size();
                    view! {
                        <button
                            type="button"
                            aria-pressed=move || (settings.difficulty.get() == d).to_string()
                            class=move || {
                                format!(
                                    "flex min-w-0 flex-col items-center rounded-lg border px-0.5 py-1.5 transition-all hover:-translate-y-0.5 max-[359px]:px-0 sm:px-1 sm:py-2 {}",
                                    if settings.difficulty.get() == d {
                                        "border-primary bg-primary text-primary-foreground shadow-md"
                                    } else {
                                        "bg-background hover:border-primary/50"
                                    },
                                )
                            }
                            data-tooltip=move || format!("{} · {w}×{h}", difficulty_name(i18n.get_locale(), d))
                            on:click=move |_| settings.difficulty.set(d)
                        >
                            <span class="font-pixel text-lg leading-none" aria-hidden="true">{d.kanji()}</span>
                            // Phones: lower case in the body font, smaller below 360 px. A fifth of
                            // the card has no room for MEDIUM or Складний in the display capitals
                            <span class="mt-1 max-w-full truncate text-[10px] font-bold uppercase max-sm:font-sans max-sm:whitespace-normal max-sm:normal-case max-sm:leading-tight max-[359px]:text-[9px] sm:tracking-wider">
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
        <Card class="gap-3 py-4 sm:gap-4 sm:py-6">
            <CardHeader class="px-4 sm:px-6">
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, practice.card.title)} <span class="font-pixel text-sm text-primary">"練習"</span>
                </CardTitle>
                <CardDescription>
                    {t!(i18n, practice.card.text)}
                </CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 px-4 sm:px-6">
                <DifficultyPicker />
                <div class="flex gap-2">
                    <Button
                        class="h-11 flex-1"
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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::{Locale, td_string};

    #[tokio::test]
    async fn every_difficulty_exposes_exactly_one_pressed_choice() {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::ar);
            let settings = Settings::new();
            provide_context(settings);
            for difficulty in Difficulty::ALL {
                settings.difficulty.set(difficulty);
                let html = view! { <DifficultyPicker /> }.to_html();
                assert!(html.contains("role=\"group\""));
                assert!(html.contains(td_string!(Locale::ar, practice.new_puzzle)));
                assert_eq!(html.matches("aria-pressed=\"true\"").count(), 1);
                assert_eq!(html.matches("aria-pressed=\"false\"").count(), 4);
                let pressed = html
                    .split("<button ")
                    .find(|b| {
                        b.split('>')
                            .next()
                            .unwrap()
                            .contains("aria-pressed=\"true\"")
                    })
                    .unwrap()
                    .split("</button>")
                    .next()
                    .unwrap();
                assert!(pressed.contains(&difficulty_name(Locale::ar, difficulty)));
            }
        });
    }
}
