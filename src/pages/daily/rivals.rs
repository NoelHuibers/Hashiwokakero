//! Today's duels against the rivals, once the daily is finished: "You beat Keiko by 12 s",
//! "Tobias was 40 s faster", "Lena hasn't played yet", with the head-to-head record of each.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::todays_rivals;
use crate::components::layout::use_localized;
use crate::components::rivals::{Score, gap_text};
use crate::components::sprites::{FLAME, PixelSprite, SWORDS};
use crate::components::stamps::StampBadge;
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::i18n::{t, use_i18n};
use crate::model::{HeadToHead, RivalDay, RivalsToday, Verdict, format_duration};
use crate::state::Rivals;

use super::state::Daily;

/// The rivals in the order the card lists them, each with what their line says: duels first,
/// the closest at the top of each kind (see [`Verdict::order`]).
pub fn duels(today: RivalsToday) -> Vec<(Verdict, RivalDay)> {
    let mine = today.mine;
    let mut list: Vec<_> = today
        .rivals
        .into_iter()
        .map(|r| (Verdict::of(mine, r.time_ms), r))
        .collect();
    list.sort_by_key(|(v, _)| v.order());
    list
}

/// The little picture at the start of a line: a flame for a win, the swords for a duel lost or
/// no duel, 分 for a draw, 待 while the rival hasn't played.
#[component]
fn Mark(verdict: Verdict) -> impl IntoView {
    let frame = "grid size-8 shrink-0 place-items-center rounded-md";
    match verdict {
        Verdict::Beat { .. } => view! {
            <span class=format!("{frame} bg-gold/20")>
                <PixelSprite art=&FLAME class="h-5" />
            </span>
        }
        .into_any(),
        Verdict::Lost { .. } | Verdict::Solved { .. } => view! {
            <span class=format!("{frame} bg-muted")>
                <PixelSprite art=&SWORDS class="size-4 opacity-80" />
            </span>
        }
        .into_any(),
        Verdict::Tie => view! { <span class=format!("{frame} bg-muted font-pixel text-muted-foreground")>"分"</span> }
            .into_any(),
        Verdict::Waiting => view! {
            <span class=format!("{frame} border border-dashed font-pixel text-muted-foreground")>"待"</span>
        }
        .into_any(),
    }
}

#[component]
fn Line(verdict: Verdict, rival: RivalDay) -> impl IntoView {
    let i18n = use_i18n();
    let RivalDay {
        name,
        stamp,
        time_ms,
        record:
            HeadToHead {
                wins,
                losses,
                draws,
                ..
            },
        ..
    } = rival;
    let who = move || name.clone();
    let gap = move |ms: i64| move || gap_text(i18n.get_locale(), ms);
    let text = match verdict {
        Verdict::Beat { gap_ms } => {
            t!(i18n, rivals.today.beat, name = who, gap = gap(gap_ms), <b> = <b class="font-bold text-foreground" />).into_any()
        }
        Verdict::Lost { gap_ms } => {
            t!(i18n, rivals.today.lost, name = who, gap = gap(gap_ms), <b> = <b class="font-bold text-foreground" />).into_any()
        }
        Verdict::Tie => t!(i18n, rivals.today.tie, name = who, <b> = <b class="font-bold text-foreground" />).into_any(),
        Verdict::Waiting => {
            t!(i18n, rivals.today.waiting, name = who, <b> = <b class="font-bold text-foreground" />).into_any()
        }
        Verdict::Solved { time_ms } => {
            t!(
                i18n,
                rivals.today.solved,
                name = who,
                time = format_duration(time_ms),
                <b> = <b class="font-bold text-foreground" />
            )
                .into_any()
        }
    };
    let beat = matches!(verdict, Verdict::Beat { .. });
    view! {
        <li class="flex items-center gap-3 py-2" data-verdict=format!("{verdict:?}")>
            <Mark verdict />
            <div class="flex min-w-0 flex-1 flex-col">
                <p class="text-sm leading-snug" class:text-muted-foreground=!beat>
                    {text}
                    {stamp.map(|stamp| view! { " " <span class="inline-block align-middle"><StampBadge stamp=stamp /></span> })}
                </p>
                {time_ms
                    .map(|t| {
                        view! { <span class="font-pixel text-xs tabular-nums text-muted-foreground">{format_duration(t)}</span> }
                    })}
            </div>
            <Score wins losses draws />
        </li>
    }
}

/// The rivals card under the result, once today's daily is finished.
#[component]
pub(super) fn RivalsCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let rivals = expect_context::<Rivals>();
    let ranks = use_localized("/ranks");
    let today = RwSignal::new(None::<RivalsToday>);

    // Loaded once the daily is finished, again when the ranking was refreshed (after the
    // submit) or the rivals changed. The server sends nothing before the finish anyway.
    Effect::new(move |_| {
        let finished = daily.finished();
        daily.ranking.track();
        rivals.changes.track();
        if !finished {
            today.set(None);
            return;
        }
        spawn_local(async move {
            if let Ok(t) = todays_rivals().await {
                today.set(t);
            }
        });
    });

    view! {
        <Show when=move || today.with(Option::is_some)>
            <Card class="gap-3" attr:data-rivals="">
                <CardHeader>
                    <CardTitle class="flex items-baseline gap-2 text-lg">
                        {t!(i18n, rivals.today.title)} <span class="font-pixel text-sm text-primary">"競"</span>
                    </CardTitle>
                </CardHeader>
                <CardContent>
                    {move || {
                        let list = today.get().map(duels).unwrap_or_default();
                        if list.is_empty() {
                            view! {
                                <div class="seigaiha flex flex-col items-center gap-2 rounded-lg border border-dashed p-4 text-center text-sm text-muted-foreground">
                                    <span class="grid size-10 place-items-center rounded-lg bg-card shadow-xs">
                                        <PixelSprite art=&SWORDS class="size-6" />
                                    </span>
                                    <p class="rounded-md bg-card px-2 py-0.5">{t!(i18n, rivals.today.empty)}</p>
                                    <A href=ranks attr:class="rounded-md bg-card px-2 font-medium text-primary underline-offset-4 hover:underline">
                                        {t!(i18n, rivals.today.find)}
                                    </A>
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {
                                <ul class="-my-2 divide-y">
                                    {list.into_iter().map(|(verdict, rival)| view! { <Line verdict rival /> }).collect_view()}
                                </ul>
                            }
                                .into_any()
                        }
                    }}
                </CardContent>
            </Card>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rival(user_id: i64, time_ms: Option<i64>) -> RivalDay {
        RivalDay {
            user_id,
            name: format!("rival {user_id}"),
            guest: false,
            stamp: None,
            time_ms,
            record: HeadToHead::default(),
        }
    }

    #[test]
    fn the_closest_duels_come_first() {
        let today = RivalsToday {
            mine: Some(60_000),
            rivals: vec![
                rival(1, None),
                rival(2, Some(100_000)),
                rival(3, Some(20_000)),
                rival(4, Some(61_000)),
                rival(5, Some(59_000)),
            ],
        };
        let order: Vec<(i64, Verdict)> = duels(today)
            .into_iter()
            .map(|(v, r)| (r.user_id, v))
            .collect();
        assert_eq!(
            order,
            [
                (4, Verdict::Beat { gap_ms: 1_000 }),
                (2, Verdict::Beat { gap_ms: 40_000 }),
                (5, Verdict::Lost { gap_ms: 1_000 }),
                (3, Verdict::Lost { gap_ms: 40_000 }),
                (1, Verdict::Waiting),
            ]
        );
        // Given up: their times, no duels
        let order: Vec<Verdict> = duels(RivalsToday {
            mine: None,
            rivals: vec![rival(1, None), rival(2, Some(9))],
        })
        .into_iter()
        .map(|(v, _)| v)
        .collect();
        assert_eq!(order, [Verdict::Solved { time_ms: 9 }, Verdict::Waiting]);
    }
}
