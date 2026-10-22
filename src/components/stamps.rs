//! Hanko stamps: the small one next to a name on a leaderboard, and the stamp book.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::show_stamp;
use crate::i18n::{td_string, use_i18n};
use crate::lang::{date_short, stamp_name, stamp_text};
use crate::model::{MS_PER_DAY, Stamp, StampProgress, known_stamps, share_percent};
use crate::state::{ToastKind, error_toast, toast};

/// The stamp a player chose to show next to their name, pressed in red ink so it doesn't
/// compete with the medals. Nothing for a newer server's stamp this build doesn't know.
#[component]
pub fn StampBadge(stamp: Stamp) -> impl IntoView {
    let i18n = use_i18n();
    let title = move || stamp_name(i18n.get_locale(), stamp);
    (stamp != Stamp::Unknown).then(|| {
        view! {
            <span
                class="hanko-ink inline-grid size-5 shrink-0 rotate-[-8deg] place-items-center rounded-[4px] font-pixel text-xs leading-none"
                data-tooltip=title
                aria-label=title
            >
                {stamp.kanji()}
            </span>
        }
    })
}

/// One stamp of the stamp book, greyed out with a progress bar while it's still to get, and how
/// many of the `players` have it. A secret stamp shows a question mark until it's earned. With
/// `pick` an earned stamp can be chosen to show next to the player's name (`shown`).
#[component]
fn StampTile(
    progress: StampProgress,
    players: i64,
    shown: RwSignal<Option<Stamp>>,
    pick: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    let StampProgress {
        stamp,
        earned_at,
        progress,
        target,
        holders,
    } = progress;
    let share = share_percent(holders, players);
    let earned = earned_at.is_some();
    let hidden = stamp.secret() && !earned;
    let name = move || {
        let l = i18n.get_locale();
        if hidden {
            td_string!(l, stamps.secret.name)
        } else {
            stamp_name(l, stamp)
        }
    };
    let text = move || {
        let l = i18n.get_locale();
        if hidden {
            td_string!(l, stamps.secret.text)
        } else {
            stamp_text(l, stamp)
        }
    };
    let is_shown = move || shown.get() == Some(stamp);
    view! {
        <li
            class="relative flex flex-col items-center gap-1.5 rounded-lg border bg-background p-3 text-center"
            class=("border-primary", is_shown)
            class=("ring-1", is_shown)
            class=("ring-primary", is_shown)
            class:opacity-60=!earned
            data-tooltip=text
        >
            {(pick && earned)
                .then(|| {
                    view! {
                        <button
                            type="button"
                            class="absolute inset-0 z-10 cursor-pointer rounded-lg transition-colors hover:bg-primary/5 focus-visible:outline-2 focus-visible:outline-ring"
                            aria-label=name
                            aria-pressed=move || is_shown().to_string()
                            on:click=move |_| toggle(shown, stamp)
                        ></button>
                    }
                })}
            {move || {
                is_shown()
                    .then(|| {
                        view! {
                            <span
                                class="absolute -end-1.5 -top-1.5 grid size-5 place-items-center rounded-full bg-primary text-[11px] font-bold text-primary-foreground shadow-sm"
                                data-tooltip=move || td_string!(i18n.get_locale(), stamps.shown_label)
                            >
                                "✓"
                            </span>
                        }
                    })
            }}
            <span class=format!(
                "relative grid size-12 place-items-center rounded-lg font-pixel text-2xl {}",
                if earned { "hanko rotate-[-6deg]" } else { "border-2 border-dashed text-muted-foreground" },
            )>
                {if hidden { "?" } else { stamp.kanji() }}
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
            <span class="text-xs font-bold leading-tight">{name}</span>
            {match earned_at {
                Some(at) => {
                    view! {
                        <span class="text-[10px] leading-tight text-muted-foreground">
                            {move || date_short(i18n.get_locale(), at.div_euclid(MS_PER_DAY))}
                        </span>
                    }
                        .into_any()
                }
                None if target > 1 && !hidden => {
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
                            {text}
                        </span>
                    }
                        .into_any()
                }
            }}
            {share
                .map(|percent| {
                    view! {
                        <span class="mt-auto text-[10px] leading-tight text-muted-foreground/80">
                            {move || td_string!(i18n.get_locale(), stamps.share, percent = percent.clone())}
                        </span>
                    }
                })}
        </li>
    }
}

/// Shows `stamp` next to the player's name, or no stamp if it's the one shown already.
fn toggle(shown: RwSignal<Option<Stamp>>, stamp: Stamp) {
    let before = shown.get_untracked();
    let after = (before != Some(stamp)).then_some(stamp);
    shown.set(after);
    spawn_local(async move {
        match show_stamp(after).await {
            Ok(()) => toast(ToastKind::Success, move |l| match after {
                Some(stamp) => format!(
                    "{} {}",
                    stamp.kanji(),
                    td_string!(l, stamps.shown, name = stamp_name(l, stamp))
                ),
                None => td_string!(l, stamps.hidden).to_string(),
            }),
            Err(e) => {
                shown.set(before);
                error_toast(e);
            }
        }
    });
}

/// Every stamp, earned or not. `players` counts everyone with a stamp, for the share of players
/// who have each one. `shown` is the stamp next to the player's name, which they can change with
/// `pick` (their own book). A newer server's stamps this build doesn't know are left out.
#[component]
pub fn StampBook(
    mut book: Vec<StampProgress>,
    players: i64,
    shown: RwSignal<Option<Stamp>>,
    #[prop(optional)] pick: bool,
) -> impl IntoView {
    let i18n = use_i18n();
    book.retain(|p| p.stamp != Stamp::Unknown);
    let earned = book.iter().filter(|p| p.earned_at.is_some()).count();
    let total = book.len();
    view! {
        <div class="flex flex-col gap-3">
            <p class="text-sm text-muted-foreground">
                {move || td_string!(i18n.get_locale(), stamps.collected, earned, total)}
                {(pick && earned > 0)
                    .then(|| view! { " " {move || td_string!(i18n.get_locale(), stamps.pick)} })}
            </p>
            <ul class="grid grid-cols-3 gap-2 sm:grid-cols-4 lg:grid-cols-6">
                {book
                    .into_iter()
                    .map(|p| view! { <StampTile progress=p players shown pick /> })
                    .collect_view()}
            </ul>
        </div>
    }
}

/// Toasts for stamps just earned, the first few by name.
pub fn announce(stamps: &[Stamp]) {
    const NAMED: usize = 3;
    let stamps = known_stamps(stamps);
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

// Rendering on the server needs its features
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::Locale;

    fn render(view: impl FnOnce() -> AnyView) -> String {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            view().to_html()
        });
        drop(owner);
        html
    }

    /// A newer server's stamps (an app that wasn't updated yet) show nowhere: not next to a
    /// name, not in the book.
    #[tokio::test]
    async fn unknown_stamps_are_left_out() {
        let badge = render(|| view! { <StampBadge stamp=Stamp::Unknown /> }.into_any());
        assert!(!badge.contains("hanko-ink"), "{badge}");
        let badge = render(|| view! { <StampBadge stamp=Stamp::Gold /> }.into_any());
        assert!(badge.contains("hanko-ink"), "{badge}");
        let tile = |stamp| StampProgress {
            stamp,
            earned_at: Some(0),
            progress: 1,
            target: 1,
            holders: 1,
        };
        let book = render(|| {
            let shown = RwSignal::new(None);
            view! {
                <StampBook book=vec![tile(Stamp::Gold), tile(Stamp::Unknown)] players=2 shown />
            }
            .into_any()
        });
        assert_eq!(book.matches("<li").count(), 1, "{book}");
        assert!(book.contains("1 of 1"), "{book}");
    }
}
