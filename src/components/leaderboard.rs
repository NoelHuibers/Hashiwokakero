use leptos::prelude::*;
use leptos_router::components::A;

use super::rivals::RivalMark;
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

/// The medal of the top three, the plain rank below. `large` for the podium.
#[component]
pub fn RankCell(rank: i64, #[prop(optional)] large: bool) -> impl IntoView {
    let size = if large {
        "size-9 text-base"
    } else {
        "size-7 text-sm"
    };
    match medal(rank) {
        Some((kanji, class)) => view! {
            <span class=format!("grid place-items-center rounded-md font-pixel shadow-sm {size} {class}")>
                {kanji}
            </span>
        }
        .into_any(),
        None => view! {
            <span class=format!("grid place-items-center font-pixel tabular-nums text-muted-foreground {size}")>
                {rank}
            </span>
        }
        .into_any(),
    }
}

/// A player in a leaderboard: name (a link to their profile), the stamp they show, "you" and
/// guest labels. Deleted players have no name and no profile, their times stay ranked.
///
/// The guest label only shows where the table is wide enough (a container query on an
/// `@container` around the table, boards sit in narrow cards too): the name has priority.
///
/// `rival` puts the small crossed swords after a player you marked as your rival, only on the
/// daily and monthly boards, where you race them.
#[component]
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "component props are always named where they're set"
)]
pub fn PlayerName(
    user_id: i64,
    name: String,
    guest: bool,
    stamp: Option<Stamp>,
    #[prop(optional)] me: bool,
    #[prop(optional)] rival: bool,
    /// In a row marked as yours ([`HIGHLIGHT_ROW`]), which already says it: the "you" badge is
    /// then only for screen readers and leaves its room to the name.
    #[prop(optional)]
    marked_row: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    if name.is_empty() {
        return view! {
            <div class="flex items-center gap-2">
                <span class="min-w-0 truncate italic text-muted-foreground">{t!(i18n, common.ranking.deleted)}</span>
            </div>
        }
        .into_any();
    }
    let href = move || format!("{}/player/{user_id}", prefix(i18n.get_locale()));
    let full = name.clone();
    view! {
        <div class="flex items-center gap-2">
            // The whole name in a tooltip, while it's cut off
            <A
                href=href
                attr:class="min-w-0 truncate font-medium underline-offset-4 hover:text-primary hover:underline"
                attr:data-tooltip=full
                attr:data-tooltip-clipped=""
            >
                {name}
            </A>
            {rival.then(|| view! { <RivalMark user_id /> })}
            {stamp.map(|stamp| view! { <StampBadge stamp=stamp /> })}
            {me.then(|| {
                if marked_row {
                    view! { <span class="sr-only">{t!(i18n, common.ranking.you)}</span> }.into_any()
                } else {
                    view! { <Badge variant=BadgeVariant::Default size=BadgeSize::Sm class="shrink-0">{t!(i18n, common.ranking.you)}</Badge> }
                        .into_any()
                }
            })}
            {guest.then(|| view! { <span class="hidden text-xs text-muted-foreground @sm:inline">{t!(i18n, common.ranking.guest)}</span> })}
        </div>
    }
    .into_any()
}

/// The marked row of a ranking table: your own result, the fastest run.
///
/// Pressed like a hanko, a rounded 朱 outline. Needs [`RANK_TABLE`] on the `<table>`, which
/// draws the lines between the rows (see `.rank-table` in tailwind.css).
pub const HIGHLIGHT_ROW: &str = "rank-mark";

/// Classes of a ranking `<table>`, see [`HIGHLIGHT_ROW`].
pub const RANK_TABLE: &str = "rank-table";

/// The same inner padding on every row, so a highlighted one doesn't shift its contents. Pull
/// the table out by as much (`-mx-2`) to keep its contents flush with the card's.
pub const ROW_INSET: &str = "[&>*:first-child]:ps-2 [&>*:last-child]:pe-2";

/// What a [`Podium`] step shows under the big number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceDetail {
    /// Nothing.
    None,
    /// Dailies solved (the monthly race).
    Solved(i64),
    /// Hints and checks used, shown when there were any.
    Penalties { hints: i64, checks: i64 },
}

/// One step of a [`Podium`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub rank: i64,
    pub user_id: i64,
    pub name: String,
    pub guest: bool,
    pub stamp: Option<Stamp>,
    /// The signed in player, marked like [`HIGHLIGHT_ROW`].
    pub me: bool,
    /// The big number: a time, points.
    pub value: String,
    pub detail: PlaceDetail,
}

/// Where the n-th place stands on the podium: 2 · 1 · 3, the winner in the middle and highest.
/// Grid columns are logical, so right to left languages mirror it.
const fn step_layout(index: usize) -> (&'static str, &'static str) {
    match index {
        0 => ("col-start-2", "pt-6"),
        1 => ("col-start-1", "pt-4"),
        _ => ("col-start-3", "pt-3"),
    }
}

/// How many of a ranking go on the podium, the rest stay in the table below.
pub const PODIUM: usize = 3;

/// The top three of a ranking as a podium. Pass at most [`PODIUM`] places, in order.
#[component]
pub fn Podium(places: Vec<Place>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <ol class="grid grid-cols-3 items-end gap-2">
            {places
                .into_iter()
                .take(PODIUM)
                .enumerate()
                .map(|(index, place)| {
                    let (column, top) = step_layout(index);
                    let me = place.me;
                    let detail = match place.detail {
                        PlaceDetail::None => None,
                        PlaceDetail::Solved(solved) => {
                            Some(
                                view! {
                                    <span class="text-[10px] uppercase tracking-wider text-muted-foreground">
                                        <span class="font-pixel">{solved}</span>
                                        " "
                                        {t!(i18n, account.stats.solved)}
                                    </span>
                                }
                                    .into_any(),
                            )
                        }
                        PlaceDetail::Penalties { hints, checks } => {
                            (hints + checks > 0)
                                .then(|| {
                                    view! {
                                        <span
                                            class="text-xs text-muted-foreground"
                                            data-tooltip=move || t_string!(i18n, common.ranking.penalties)
                                        >
                                            {format!("{hints}h {checks}c")}
                                        </span>
                                    }
                                        .into_any()
                                })
                        }
                    };
                    view! {
                        <li
                            data-place=index + 1
                            class=format!(
                                "podium-step relative row-start-1 flex min-w-0 flex-col items-center gap-1 rounded-xl px-2 pb-3 text-center text-sm {column} {top}",
                            )
                            class:rank-mark=me
                        >
                            {me
                                .then(|| {
                                    view! {
                                        <Badge
                                            variant=BadgeVariant::Default
                                            size=BadgeSize::Sm
                                            class="absolute inset-x-0 -top-2 mx-auto"
                                        >
                                            {t!(i18n, common.ranking.you)}
                                        </Badge>
                                    }
                                })}
                            <RankCell rank=place.rank large=true />
                            // PlayerName is a flex row that has to shrink for its name to truncate
                            <div class="flex w-full min-w-0 justify-center [&>div]:min-w-0">
                                <PlayerName user_id=place.user_id name=place.name guest=place.guest stamp=place.stamp rival=true />
                            </div>
                            <span
                                class=format!("font-pixel tabular-nums {}", if index == 0 { "text-2xl" } else { "text-xl" })
                                class:text-primary=me
                            >
                                {place.value}
                            </span>
                            // Always the line, so only the place decides how high a step is
                            <span class="flex h-4 items-center">{detail}</span>
                        </li>
                    }
                })
                .collect_view()}
        </ol>
    }
}

#[component]
fn Row(entry: LeaderboardEntry, me: bool) -> impl IntoView {
    let i18n = use_i18n();
    let penalties = entry.hints + entry.checks;
    view! {
        <tr class=format!("{ROW_INSET} {}", if me { HIGHLIGHT_ROW } else { "" })>
            <td class="py-2 pe-2">
                <RankCell rank=entry.rank />
            </td>
            <td class="max-w-0 py-2 pe-2">
                <PlayerName user_id=entry.user_id name=entry.name guest=entry.guest stamp=entry.stamp me=me marked_row=true rival=true />
            </td>
            <td class="py-2 pe-2 text-end text-xs text-muted-foreground" data-tooltip=move || t_string!(i18n, common.ranking.penalties)>
                {(penalties > 0).then(|| format!("{}h {}c", entry.hints, entry.checks))}
            </td>
            <td class="py-2 text-end font-pixel tabular-nums" class:text-primary=me>{format_duration(entry.time_ms)}</td>
        </tr>
    }
}

/// The places on the podium and the rest of a ranking, if it gets one.
pub fn split_podium<T>(mut entries: Vec<T>, podium: bool) -> (Vec<T>, Vec<T>) {
    if !podium {
        return (Vec::new(), entries);
    }
    let rest = entries.split_off(entries.len().min(PODIUM));
    (entries, rest)
}

/// Ranked finishers of a daily, with the current player's row added if they're further down.
#[component]
pub fn LeaderboardTable(
    entries: Vec<LeaderboardEntry>,
    me: Option<LeaderboardEntry>,
    /// Show at most this many rows (plus the player's own).
    #[prop(optional)]
    limit: Option<usize>,
    /// The top three on a [`Podium`] above the table, for wide cards.
    #[prop(optional)]
    podium: bool,
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
    let (top, rest) = split_podium(shown, podium);
    let places: Vec<Place> = top
        .into_iter()
        .map(|e| Place {
            rank: e.rank,
            me: Some(e.user_id) == my_id,
            user_id: e.user_id,
            value: format_duration(e.time_ms),
            detail: PlaceDetail::Penalties {
                hints: e.hints,
                checks: e.checks,
            },
            name: e.name,
            guest: e.guest,
            stamp: e.stamp,
        })
        .collect();
    let table = (!rest.is_empty() || me_below.is_some()).then(|| {
        view! {
            // A size container for the @sm: variants (tables themselves can't be one), pulled out
            // by the row inset
            <div class="@container -mx-2">
                <table class=format!("{RANK_TABLE} w-full table-fixed text-sm")>
                    <colgroup>
                        // The first and last columns hold the row inset (ROW_INSET) too
                        <col class="w-14" />
                        <col />
                        <col class="w-16" />
                        <col class="w-22" />
                    </colgroup>
                    <tbody>
                        {rest
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
    });
    view! {
        <div class="flex flex-col gap-3">
            {(!places.is_empty()).then(|| view! { <Podium places=places /> })}
            {table}
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
            attr:aria-label=move || match auth.user.get() {
                Some(user) => format!("{} · {}", t_string!(i18n, common.account),
                    if user.guest { t_string!(i18n, common.ranking.guest_name).to_string() } else { user.name }),
                None => t_string!(i18n, common.sign_in).to_string(),
            }
        >
            {move || match auth.user.get() {
                Some(user) => {
                    let initial = user.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                    view! {
                        <span aria-hidden="true" class=format!(
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
                        <span aria-hidden="true" class="grid size-7 place-items-center rounded-md bg-secondary font-pixel text-sm">"入"</span>
                        <span class="hidden text-sm font-medium lg:inline">{t!(i18n, common.sign_in)}</span>
                    }
                        .into_any()
                }
            }}
        </A>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_three_go_on_the_podium() {
        assert_eq!(
            split_podium(vec![1, 2, 3, 4, 5], true),
            (vec![1, 2, 3], vec![4, 5])
        );
        assert_eq!(split_podium(vec![1, 2], true), (vec![1, 2], vec![]));
        assert_eq!(split_podium(Vec::<i32>::new(), true), (vec![], vec![]));
        assert_eq!(
            split_podium(vec![1, 2, 3, 4], false),
            (vec![], vec![1, 2, 3, 4])
        );
    }

    #[test]
    fn the_winner_stands_in_the_middle_and_highest() {
        let columns: Vec<_> = (0..3).map(|i| step_layout(i).0).collect();
        assert_eq!(columns, ["col-start-2", "col-start-1", "col-start-3"]);
        let tops: Vec<_> = (0..3).map(|i| step_layout(i).1).collect();
        assert_eq!(tops, ["pt-6", "pt-4", "pt-3"]);
    }
}
