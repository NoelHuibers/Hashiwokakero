//! The head-to-head record against a player who is one of your rivals: wins, losses and draws
//! over every day you both solved, and the last duels.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::rival_record;
use crate::components::rivals::{RecentDuels, outcome_class};
use crate::components::sprites::{PixelSprite, SWORDS};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, td_string, use_i18n};
use crate::model::{HeadToHead, Outcome};
use crate::state::{Rivals, error_toast};

/// One of the three numbers: 勝 wins, 負 losses, 分 draws.
#[component]
fn Tally(outcome: Outcome, count: i64, #[prop(into)] label: Signal<String>) -> impl IntoView {
    view! {
        <div class=format!(
            "flex flex-col items-center gap-0.5 rounded-lg border px-3 py-2 {}",
            outcome_class(outcome),
        )>
            <span class="font-pixel text-3xl tabular-nums leading-tight">{count}</span>
            <span class="text-[10px] font-bold uppercase tracking-[0.16em]">
                {move || label.get()} " · " <span class="font-pixel">{outcome.kanji()}</span>
            </span>
        </div>
    }
}

#[component]
fn Record(record: HeadToHead) -> impl IntoView {
    let i18n = use_i18n();
    if record.played() == 0 {
        return view! {
            <div class="seigaiha grid h-28 place-items-center rounded-lg border border-dashed p-3 text-center text-sm text-muted-foreground">
                <p class="rounded-md bg-card px-3 py-1">{t!(i18n, rivals.record.none)}</p>
            </div>
        }
        .into_any();
    }
    view! {
        <div class="flex flex-col gap-4">
            <div class="grid grid-cols-3 gap-2 sm:max-w-sm">
                <Tally outcome=Outcome::Win count=record.wins label=Signal::derive(move || td_string!(i18n.get_locale(), rivals.record.wins).to_string()) />
                <Tally outcome=Outcome::Loss count=record.losses label=Signal::derive(move || td_string!(i18n.get_locale(), rivals.record.losses).to_string()) />
                <Tally outcome=Outcome::Draw count=record.draws label=Signal::derive(move || td_string!(i18n.get_locale(), rivals.record.draws).to_string()) />
            </div>
            <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                <span class="text-xs font-bold uppercase tracking-wider text-muted-foreground">
                    {t!(i18n, rivals.record.recent)}
                </span>
                <RecentDuels record large=true />
            </div>
        </div>
    }
    .into_any()
}

/// The record against `player`, only while they are one of your rivals.
#[component]
pub(super) fn HeadToHeadCard(player: i64) -> impl IntoView {
    let i18n = use_i18n();
    let rivals = expect_context::<Rivals>();
    let record = RwSignal::new(None::<HeadToHead>);

    // Fetched when they become a rival (also on the first load of the rivals), forgotten when
    // they stop being one
    Effect::new(move |_| {
        if !rivals.is(player) {
            record.set(None);
            return;
        }
        rivals.changes.track();
        spawn_local(async move {
            match rival_record(player).await {
                Ok(r) => record.set(r),
                Err(e) => error_toast(e),
            }
        });
    });

    view! {
        {move || {
            record
                .get()
                .map(|r| {
                    view! {
                        <Card class="gap-4" attr:data-head-to-head="">
                            <CardHeader>
                                <CardTitle class="flex items-center gap-2 text-xl">
                                    <PixelSprite art=&SWORDS class="size-5" />
                                    {t!(i18n, rivals.record.title)}
                                    <span class="font-pixel text-sm text-primary">"対戦"</span>
                                </CardTitle>
                                <CardDescription>
                                    {t!(i18n, rivals.record.text)}
                                </CardDescription>
                            </CardHeader>
                            <CardContent>
                                <Record record=r />
                            </CardContent>
                        </Card>
                    }
                })
        }}
    }
}
