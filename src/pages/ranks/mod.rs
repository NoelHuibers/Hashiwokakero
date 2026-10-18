//! The ranks page: the daily leaderboard of any day, the monthly race and the hall of fame.

pub mod daily_board;
pub mod hall_of_fame;
pub mod month;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{daily_leaderboard, load_hall_of_fame};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::error_text;
use crate::model::{HallOfFame as Fame, Leaderboard};
use crate::seo::PageMeta;
use crate::state::{ToastKind, toast};

use daily_board::DailyBoard;
use hall_of_fame::HallOfFame;
use month::MonthRace;

#[component]
pub fn RanksPage() -> impl IntoView {
    let i18n = use_i18n();

    let day = RwSignal::new(None::<i64>);
    let today = RwSignal::new(None::<i64>);
    let board = RwSignal::new(None::<Leaderboard>);
    let fame = RwSignal::new(None::<Fame>);
    let loading = RwSignal::new(false);

    // Fetch whenever the selected day changes (browser only)
    Effect::new(move |_| {
        let selected = day.get();
        loading.set(true);
        spawn_local(async move {
            match daily_leaderboard(selected).await {
                Ok(lb) => {
                    if selected.is_none() {
                        today.set(Some(lb.day));
                    }
                    board.set(Some(lb));
                }
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
            loading.set(false);
        });
    });
    Effect::new(move |first: Option<()>| {
        if first.is_none() {
            spawn_local(async move {
                if let Ok(f) = load_hall_of_fame().await {
                    fame.set(Some(f));
                }
            });
        }
    });

    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, ranks.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, ranks.meta.description).to_string())
            path="/ranks"
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="mb-5">
                <p class="font-pixel text-sm text-primary">"順位 · jun'i"</p>
                <h1 class="text-3xl font-black">{t!(i18n, ranks.title)}</h1>
                <p class="mt-1 max-w-2xl text-muted-foreground">
                    {t!(i18n, ranks.intro)}
                </p>
            </div>

            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-6">
                    <DailyBoard day=day today=today board=board loading=loading />
                    <MonthRace />
                </div>
                <HallOfFame fame=fame />
            </div>
        </section>
    }
}
