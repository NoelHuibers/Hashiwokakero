//! The Dojo's leaderboard, the banzuke: every ranked solver with its score per family.

use dojo::Score;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::lang::prefix;
use crate::model::DojoBoard;

use super::names::{family_kanji, family_name, par2};

/// One small square per family, filled by the share of solved formulas.
#[component]
fn FamilyChips(score: Score) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <div class="flex gap-1">
            {score
                .families
                .into_iter()
                .map(|f| {
                    let share = if f.total == 0 { 0.0 } else { f64::from(f.solved) / f64::from(f.total) };
                    let title = move || format!("{}: {}/{}", family_name(i18n.get_locale(), f.family), f.solved, f.total);
                    view! {
                        <span
                            class="relative grid size-7 place-items-center overflow-hidden rounded-sm border bg-muted font-pixel text-xs"
                            title=title
                        >
                            <span
                                class="absolute inset-x-0 bottom-0 bg-primary/70"
                                style=format!("height: {:.0}%", share * 100.0)
                            ></span>
                            <span class="relative">{family_kanji(f.family)}</span>
                        </span>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
pub(super) fn Leaderboard(board: RwSignal<Option<DojoBoard>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <Card class="gap-4">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-xl">
                    {t!(i18n, dojo.board.title)} " " <span class="font-pixel text-sm text-primary">"番付 · banzuke"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, dojo.board.par2_hint)}</CardDescription>
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
                    Some(b) => {
                        view! {
                            <div class="overflow-x-auto">
                                <table class="w-full text-sm">
                                    <thead>
                                        <tr class="border-b text-start text-xs uppercase tracking-wider text-muted-foreground">
                                            <th class="py-2 pe-2 text-start font-semibold">"#"</th>
                                            <th class="py-2 pe-3 text-start font-semibold">{t!(i18n, dojo.board.solver)}</th>
                                            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.solved)}</th>
                                            <th class="py-2 pe-3 text-end font-semibold">{t!(i18n, dojo.board.par2)}</th>
                                            <th class="hidden py-2 text-start font-semibold sm:table-cell">{t!(i18n, dojo.board.families)}</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {b
                                            .entries
                                            .into_iter()
                                            .map(|e| {
                                                let href = move || format!("{}/dojo/{}", prefix(i18n.get_locale()), e.id);
                                                let medal = match e.rank {
                                                    1 => "金",
                                                    2 => "銀",
                                                    3 => "銅",
                                                    _ => "",
                                                };
                                                view! {
                                                    <tr class="border-b last:border-0">
                                                        <td class="py-2.5 pe-2 font-pixel tabular-nums">
                                                            {if medal.is_empty() { e.rank.to_string() } else { medal.to_string() }}
                                                        </td>
                                                        <td class="py-2.5 pe-3">
                                                            <A href=href attr:class="font-medium hover:text-primary hover:underline">
                                                                {e.name.clone()}
                                                            </A>
                                                            {e
                                                                .house
                                                                .then(|| {
                                                                    view! {
                                                                        <Badge variant=BadgeVariant::Secondary size=BadgeSize::Sm class="ms-2">
                                                                            {t!(i18n, dojo.board.house)}
                                                                        </Badge>
                                                                    }
                                                                })}
                                                            <span class="block text-xs text-muted-foreground">{e.author.clone()}</span>
                                                        </td>
                                                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">
                                                            {format!("{}/{}", e.score.solved, e.score.total)}
                                                        </td>
                                                        <td class="py-2.5 pe-3 text-end font-pixel tabular-nums">{par2(&e.score)}</td>
                                                        <td class="hidden py-2.5 sm:table-cell">
                                                            <FamilyChips score=e.score />
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }
                            .into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}
