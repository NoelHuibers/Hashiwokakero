//! Hanko stamps: the small one next to a name on a leaderboard, and the stamp book.

use leptos::prelude::*;

use crate::i18n::{td_string, use_i18n};
use crate::lang::{date_short, stamp_name, stamp_text};
use crate::model::{MS_PER_DAY, Stamp, StampProgress};
use crate::state::{ToastKind, toast};

/// A player's most impressive stamp, next to their name.
#[component]
pub fn StampBadge(stamp: Stamp) -> impl IntoView {
    let i18n = use_i18n();
    let title = move || stamp_name(i18n.get_locale(), stamp);
    view! {
        <span
            class="hanko inline-grid size-6 shrink-0 rotate-[-6deg] place-items-center rounded-[5px] font-pixel text-sm leading-none"
            data-tooltip=title
            aria-label=title
        >
            {stamp.kanji()}
        </span>
    }
}

/// One stamp of the stamp book, greyed out with a progress bar while it's still to get.
#[component]
fn StampTile(progress: StampProgress) -> impl IntoView {
    let i18n = use_i18n();
    let StampProgress {
        stamp,
        earned_at,
        progress,
        target,
    } = progress;
    let earned = earned_at.is_some();
    view! {
        <li
            class="flex flex-col items-center gap-1.5 rounded-lg border bg-background p-3 text-center"
            class:opacity-60=!earned
            data-tooltip=move || stamp_text(i18n.get_locale(), stamp)
        >
            <span class=format!(
                "relative grid size-12 place-items-center rounded-lg font-pixel text-2xl {}",
                if earned { "hanko rotate-[-6deg]" } else { "border-2 border-dashed text-muted-foreground" },
            )>
                {stamp.kanji()}
                {stamp
                    .tier()
                    .map(|n| {
                        view! {
                            <span class="absolute -bottom-1.5 -end-2 rounded-sm bg-card px-1 font-pixel text-[10px] leading-tight text-foreground shadow-sm">
                                {n}
                            </span>
                        }
                    })}
            </span>
            <span class="text-xs font-bold leading-tight">{move || stamp_name(i18n.get_locale(), stamp)}</span>
            {match earned_at {
                Some(at) => {
                    view! {
                        <span class="text-[10px] leading-tight text-muted-foreground">
                            {move || date_short(i18n.get_locale(), at.div_euclid(MS_PER_DAY))}
                        </span>
                    }
                        .into_any()
                }
                None if target > 1 => {
                    view! {
                        <span class="flex w-full flex-col gap-0.5">
                            <span class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                                <span
                                    class="block h-full rounded-full bg-primary"
                                    style=format!("width: {}%", progress * 100 / target.max(1))
                                ></span>
                            </span>
                            <span class="font-pixel text-[10px] tabular-nums text-muted-foreground">
                                {format!("{progress} / {target}")}
                            </span>
                        </span>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        <span class="text-[10px] leading-tight text-muted-foreground">
                            {move || stamp_text(i18n.get_locale(), stamp)}
                        </span>
                    }
                        .into_any()
                }
            }}
        </li>
    }
}

/// Every stamp, earned or not.
#[component]
pub fn StampBook(book: Vec<StampProgress>) -> impl IntoView {
    let i18n = use_i18n();
    let earned = book.iter().filter(|p| p.earned_at.is_some()).count();
    let total = book.len();
    view! {
        <div class="flex flex-col gap-3">
            <p class="text-sm text-muted-foreground">
                {move || td_string!(i18n.get_locale(), stamps.collected, earned, total)}
            </p>
            <ul class="grid grid-cols-3 gap-2 sm:grid-cols-4 lg:grid-cols-6">
                {book.into_iter().map(|p| view! { <StampTile progress=p /> }).collect_view()}
            </ul>
        </div>
    }
}

/// Toasts for stamps just earned, the first few by name.
pub fn announce(stamps: &[Stamp]) {
    const NAMED: usize = 3;
    for &stamp in stamps.iter().take(NAMED) {
        toast(ToastKind::Success, move |l| {
            format!(
                "{} {}",
                stamp.kanji(),
                td_string!(l, stamps.new, name = stamp_name(l, stamp))
            )
        });
    }
    if stamps.len() > NAMED {
        let count = stamps.len() - NAMED;
        toast(ToastKind::Success, move |l| {
            td_string!(l, stamps.more, count)
        });
    }
}
