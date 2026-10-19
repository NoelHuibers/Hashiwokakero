use leptos::prelude::*;
use leptos_router::components::A;

use super::stamps::StampBadge;
use super::ui::badge::{Badge, BadgeSize, BadgeVariant};
use crate::components::layout::use_localized;
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::prefix;
use crate::model::{LeaderboardEntry, Stamp, format_duration};
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

/// A player in a leaderboard: name (a link to their profile), most impressive stamp, "you" and
/// guest labels.
///
/// The guest label only shows where the table is wide enough (a container query on an
/// `@container` around the table, boards sit in narrow cards too): the name has priority.
#[component]
pub fn PlayerName(
    user_id: i64,
    name: String,
    guest: bool,
    stamp: Option<Stamp>,
    #[prop(optional)] me: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    let href = move || format!("{}/player/{user_id}", prefix(i18n.get_locale()));
    view! {
        <div class="flex items-center gap-2">
            <A href=href attr:class="min-w-0 truncate font-medium underline-offset-4 hover:text-primary hover:underline">
                {name}
            </A>
            {stamp.map(|stamp| view! { <StampBadge stamp=stamp /> })}
            {me.then(|| view! { <Badge variant=BadgeVariant::Default size=BadgeSize::Sm class="shrink-0">{t!(i18n, common.ranking.you)}</Badge> })}
            {guest.then(|| view! { <span class="hidden text-xs text-muted-foreground @sm:inline">{t!(i18n, common.ranking.guest)}</span> })}
        </div>
    }
}

/// A highlighted table row: your own result, the fastest solver.
///
/// A rounded pill, since a `<tr>` itself can't have rounded corners. Only colours, so the row
/// stays aligned with the others; give every row of the table [`ROW_INSET`] for the padding.
pub const HIGHLIGHT_ROW: &str =
    "[&>td]:bg-accent [&>td:first-child]:rounded-s-lg [&>td:last-child]:rounded-e-lg";

/// The same inner padding on every row, so a highlighted one doesn't shift its contents. Pull
/// the table out by as much (`-mx-2`) to keep its contents flush with the card's.
pub const ROW_INSET: &str = "[&>*:first-child]:ps-2 [&>*:last-child]:pe-2";

#[component]
fn Row(entry: LeaderboardEntry, me: bool) -> impl IntoView {
    let i18n = use_i18n();
    let penalties = entry.hints + entry.checks;
    view! {
        <tr class=format!(
            "border-b last:border-0 {ROW_INSET} {}",
            if me { HIGHLIGHT_ROW } else { "" },
        )>
            <td class="py-2 pe-2">
                <RankCell rank=entry.rank />
            </td>
            <td class="max-w-0 py-2 pe-2">
                <PlayerName user_id=entry.user_id name=entry.name guest=entry.guest stamp=entry.stamp me=me />
            </td>
            <td class="py-2 pe-2 text-end text-xs text-muted-foreground" data-tooltip=move || t_string!(i18n, common.ranking.penalties)>
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
        // A size container for the @sm: variants (tables themselves can't be one), pulled out by
        // the row inset
        <div class="@container -mx-2">
            <table class="w-full table-fixed text-sm">
                <colgroup>
                    // The first and last columns hold the row inset (ROW_INSET) too
                    <col class="w-14" />
                    <col />
                    <col class="w-16" />
                    <col class="w-22" />
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
        </div>
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
            attr:data-tooltip=move || t_string!(i18n, common.account)
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
