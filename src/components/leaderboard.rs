use leptos::prelude::*;
use leptos_router::components::A;

use super::ui::badge::{Badge, BadgeVariant};
use crate::components::layout::use_localized;
use crate::i18n::{t, t_string, use_i18n};
use crate::model::{LeaderboardEntry, format_duration};
use crate::state::Auth;

/// 🥇🥈🥉 as pixel-ish kanji medals: 金 gold, 銀 silver, 銅 bronze.
fn medal(rank: i64) -> Option<(&'static str, &'static str)> {
    match rank {
        1 => Some(("金", "bg-[var(--px-gold)] text-[#3a2a05]")),
        2 => Some(("銀", "bg-[#c9cdd4] text-[#2b2f36]")),
        3 => Some(("銅", "bg-[#d39a6a] text-[#3b2210]")),
        _ => None,
    }
}

#[component]
pub fn RankCell(rank: i64) -> impl IntoView {
    match medal(rank) {
        Some((kanji, class)) => view! {
            <span class=format!("grid size-7 place-items-center rounded-md font-pixel text-sm shadow-sm {class}")>
                {kanji}
            </span>
        }
        .into_any(),
        None => view! {
            <span class="grid size-7 place-items-center font-pixel text-sm tabular-nums text-muted-foreground">
                {rank}
            </span>
        }
        .into_any(),
    }
}

/// A highlighted table row (your own result, the fastest solver): a rounded, padded pill, since
/// a `<tr>` itself can't have rounded corners.
pub const HIGHLIGHT_ROW: &str = "[&>td]:bg-accent [&>td:first-child]:rounded-s-lg [&>td:first-child]:ps-2 [&>td:last-child]:rounded-e-lg [&>td:last-child]:pe-2";

#[component]
fn Row(entry: LeaderboardEntry, me: bool) -> impl IntoView {
    let i18n = use_i18n();
    let penalties = entry.hints + entry.checks;
    view! {
        <tr class=format!("border-b last:border-0 {}", if me { HIGHLIGHT_ROW } else { "" })>
            <td class="py-2 pe-2">
                <RankCell rank=entry.rank />
            </td>
            <td class="max-w-0 py-2 pe-2">
                <div class="flex items-center gap-2">
                    <span class="truncate font-medium">{entry.name}</span>
                    {me.then(|| view! { <Badge variant=BadgeVariant::Default size=crate::components::ui::badge::BadgeSize::Sm>{t!(i18n, common.ranking.you)}</Badge> })}
                    {entry.guest.then(|| view! { <span class="text-xs text-muted-foreground">{t!(i18n, common.ranking.guest)}</span> })}
                </div>
            </td>
            <td class="py-2 pe-2 text-end text-xs text-muted-foreground" title=move || t_string!(i18n, common.ranking.penalties)>
                {(penalties > 0).then(|| format!("{}h {}c", entry.hints, entry.checks))}
            </td>
            <td class="py-2 text-end font-pixel tabular-nums">{format_duration(entry.time_ms)}</td>
        </tr>
    }
}

/// Ranked finishers of a daily, with the current player's row added if they're further down.
#[component]
pub fn LeaderboardTable(
    entries: Vec<LeaderboardEntry>,
    me: Option<LeaderboardEntry>,
    /// Show at most this many rows (plus the player's own).
    #[prop(optional)]
    limit: Option<usize>,
) -> impl IntoView {
    let i18n = use_i18n();
    let my_id = me.as_ref().map(|m| m.user_id);
    let shown: Vec<LeaderboardEntry> = entries
        .into_iter()
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    let me_below = me.filter(|m| !shown.iter().any(|e| e.user_id == m.user_id));
    if shown.is_empty() {
        return view! {
            <div class="seigaiha grid h-32 place-items-center rounded-lg border border-dashed text-sm text-muted-foreground">
                <p class="rounded-md bg-card px-3 py-1">{t!(i18n, common.ranking.empty)} " 一番乗り"</p>
            </div>
        }
        .into_any();
    }
    view! {
        <table class="w-full table-fixed text-sm">
            <colgroup>
                <col class="w-10" />
                <col />
                <col class="w-16" />
                <col class="w-20" />
            </colgroup>
            <tbody>
                {shown
                    .into_iter()
                    .map(|e| {
                        let mine = Some(e.user_id) == my_id;
                        view! { <Row entry=e me=mine /> }
                    })
                    .collect_view()}
                {me_below
                    .map(|m| {
                        view! {
                            <tr aria-hidden="true">
                                <td colspan="4" class="py-0.5 text-center text-xs text-muted-foreground">"⋯"</td>
                            </tr>
                            <Row entry=m me=true />
                        }
                    })}
            </tbody>
        </table>
    }
    .into_any()
}

/// The signed in player in the header, or a sign in link.
#[component]
pub fn AccountButton() -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let to = use_localized("/account");
    view! {
        <A
            href=to
            attr:class="flex h-10 shrink-0 items-center gap-2 rounded-lg border bg-card px-2 transition-transform hover:-translate-y-0.5"
            attr:title=move || t_string!(i18n, common.account)
        >
            {move || match auth.user.get() {
                Some(user) => {
                    let initial = user.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                    view! {
                        <span class=format!(
                            "grid size-7 place-items-center rounded-md font-pixel text-sm {}",
                            if user.guest { "bg-secondary text-secondary-foreground" } else { "hanko" },
                        )>{initial}</span>
                        <span class="hidden max-w-28 truncate text-sm font-medium lg:inline">
                            {if user.guest { t_string!(i18n, common.ranking.guest_name).to_string() } else { user.name }}
                        </span>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        <span class="grid size-7 place-items-center rounded-md bg-secondary font-pixel text-sm">"入"</span>
                        <span class="hidden text-sm font-medium lg:inline">{t!(i18n, common.sign_in)}</span>
                    }
                        .into_any()
                }
            }}
        </A>
    }
}
