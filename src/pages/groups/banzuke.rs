//! The banzuke 番付 card: the group's week as the ranking list of a sumo tournament.
//!
//! A hanging scroll from the yokozuna down to the maegashira, with the week's awards beside it.
//! Earlier weeks page back to the group's first; last week's yokozuna wear a crown on the
//! archipelago.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::load_banzuke;
use crate::components::layout::use_localized;
use crate::components::leaderboard::PlayerName;
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{Locale, t, t_string, td_string, use_i18n};
use crate::lang::date_short;
use crate::model::{
    Banzuke, BanzukeRow, Movement, Title, WEEKDAY_KANJI, colour_var, day_points, kanji_number,
};
use crate::state::error_toast;

use super::GroupContext;
use super::awards::Awards;
use super::today::ColourPip;

/// A crown over the island of last week's yokozuna. 9×6.
pub const CROWN: [&str; 6] = [
    "y...y...y",
    "yy.yyy.yy",
    "yyyyyyyyy",
    "yryyryyry",
    "yyyyyyyyy",
    "ooooooooo",
];

/// The name of a title: "Yokozuna", "Maegashira 3".
pub fn title_name(l: Locale, title: Title) -> String {
    match title {
        Title::Yokozuna => td_string!(l, banzuke.titles.yokozuna).to_string(),
        Title::Ozeki => td_string!(l, banzuke.titles.ozeki).to_string(),
        Title::Sekiwake => td_string!(l, banzuke.titles.sekiwake).to_string(),
        Title::Komusubi => td_string!(l, banzuke.titles.komusubi).to_string(),
        Title::Maegashira(number) => td_string!(l, banzuke.titles.maegashira, number),
    }
}

/// What the plate of a rank says, top to bottom: 横綱, 前頭三.
pub fn plate_text(title: Option<Title>) -> String {
    match title {
        Some(Title::Maegashira(n)) => {
            format!("{}{}", Title::Maegashira(n).kanji(), kanji_number(n))
        }
        Some(t) => t.kanji().to_string(),
        // 休場, sitting the tournament out
        None => "休場".to_string(),
    }
}

/// The rank's plate, written top to bottom.
#[component]
fn RankPlate(title: Option<Title>, place: Option<i64>) -> impl IntoView {
    let i18n = use_i18n();
    let label = move || {
        let l = i18n.get_locale();
        match (title, place) {
            (Some(title), Some(place)) => {
                td_string!(l, banzuke.plate, title = title_name(l, title), place)
            }
            _ => td_string!(l, banzuke.kyujo).to_string(),
        }
    };
    let big = title == Some(Title::Yokozuna);
    view! {
        <span
            class="rank-plate grid min-h-14 w-8 shrink-0 place-items-center rounded-[4px] py-1.5 text-sm"
            class=(["w-9", "min-h-16", "text-base"], big)
            data-title=title.map_or("kyujo", Title::id)
            data-tooltip=label
            role="img"
            aria-label=label
        >
            {plate_text(title)}
        </span>
    }
}

/// The seven days of a member's week: a square in their colour for each daily played, gold for
/// the days they won.
#[component]
fn Days(
    row: BanzukeRow,
    monday: i64,
    today: i64,
    today_hidden: bool,
    members: usize,
) -> impl IntoView {
    let i18n = use_i18n();
    let colour = colour_var(row.colour);
    let played = row.played();
    let wins = row.wins;
    let summary = move || td_string!(i18n.get_locale(), banzuke.days, played, wins);
    view! {
        <span class="flex items-center gap-1" role="img" aria-label=summary>
            {row
                .days
                .iter()
                .enumerate()
                .map(|(i, &place)| {
                    let day = monday + i64::try_from(i).unwrap_or(0);
                    let pending = day > today || (day == today && today_hidden);
                    let tip = move || {
                        let l = i18n.get_locale();
                        let date = format!("{} {}", WEEKDAY_KANJI[i], date_short(l, day));
                        match place {
                            Some(place) => td_string!(
                                l,
                                banzuke.day,
                                date,
                                place,
                                points = day_points(place, members)
                            ),
                            None if pending => td_string!(l, banzuke.day_open, date),
                            None => td_string!(l, banzuke.day_missed, date),
                        }
                    };
                    let look = match place {
                        Some(1) => "bg-[var(--px-gold)] ring-1 ring-[#3a2a05]/60",
                        Some(_) => "",
                        None if pending => "border border-dashed border-current opacity-20",
                        None => "border border-dashed border-current opacity-45",
                    };
                    view! {
                        <span
                            class=format!("size-3.5 rounded-[3px] sm:size-4 {look}")
                            style=place
                                .filter(|&p| p > 1)
                                .map(|_| format!("background: {colour}"))
                            data-tooltip=tip
                            data-place=place.map(|p| p.to_string())
                        ></span>
                    }
                })
                .collect_view()}
        </span>
    }
}

/// ▲2, ▼1, – or 新 since the week before.
#[component]
fn Move(movement: Option<Movement>) -> impl IntoView {
    let i18n = use_i18n();
    let Some(movement) = movement else {
        return ().into_any();
    };
    let label = move || {
        let l = i18n.get_locale();
        match movement {
            Movement::Up(n) => td_string!(l, banzuke.movement.up, n),
            Movement::Down(n) => td_string!(l, banzuke.movement.down, n),
            Movement::Stayed => td_string!(l, banzuke.movement.same).to_string(),
            Movement::New => td_string!(l, banzuke.movement.new).to_string(),
        }
    };
    let (text, class) = match movement {
        Movement::Up(n) => (format!("▲{n}"), "text-success-dark"),
        Movement::Down(n) => (format!("▼{n}"), "text-primary"),
        Movement::Stayed => ("–".to_string(), "ink-soft"),
        Movement::New => ("新".to_string(), "hanko-ink rounded-[3px] px-1"),
    };
    view! {
        <span
            class=format!("font-pixel text-xs leading-none tabular-nums {class}")
            data-tooltip=label
            data-movement=format!("{movement:?}").to_lowercase()
            role="img"
            aria-label=label
        >
            {text}
        </span>
    }
    .into_any()
}

/// A member's line on the scroll.
#[component]
fn Line(
    row: BanzukeRow,
    me: bool,
    monday: i64,
    today: i64,
    today_hidden: bool,
    members: usize,
) -> impl IntoView {
    let i18n = use_i18n();
    let placed = row.place.is_some();
    let points = row.points;
    let title = row.title;
    let subtitle = move || {
        let l = i18n.get_locale();
        match title {
            Some(title) => title_name(l, title),
            None => td_string!(l, banzuke.kyujo).to_string(),
        }
    };
    let top = title == Some(Title::Yokozuna);
    view! {
        <li
            class="banzuke-row flex items-center gap-3 px-3 py-2.5"
            class=(["bg-[var(--px-gold)]/10"], top)
            data-member=row.user_id
            data-title=title.map_or("kyujo", Title::id)
            data-place=row.place.map(|p| p.to_string())
        >
            <RankPlate title place=row.place />
            <div class="flex min-w-0 flex-1 flex-col gap-1">
                <div class="flex min-w-0 items-center gap-1.5 [&>div]:min-w-0" class:opacity-70=!placed>
                    <ColourPip colour=row.colour />
                    <PlayerName user_id=row.user_id name=row.name.clone() guest=row.guest stamp=None me />
                </div>
                <span class="ink-soft text-xs" class:font-bold=top>{subtitle}</span>
                {placed
                    .then(|| {
                        view! { <Days row=row.clone() monday today today_hidden members /> }
                    })}
            </div>
            <div class="flex shrink-0 flex-col items-end gap-1.5">
                {placed
                    .then(|| {
                        view! {
                            <span
                                class="font-pixel text-xl leading-none tabular-nums"
                                class:text-2xl=top
                                data-tooltip=move || td_string!(i18n.get_locale(), banzuke.points, points)
                                data-points=points
                            >
                                {points}
                                <span class="ink-soft ms-0.5 text-xs">"点"</span>
                            </span>
                        }
                    })}
                <Move movement=row.movement />
            </div>
        </li>
    }
}

/// The scroll: its heading and a line per member.
#[component]
fn Scroll(banzuke: Banzuke, me: i64) -> impl IntoView {
    let i18n = use_i18n();
    let daily = use_localized("/");
    let Banzuke {
        monday,
        today,
        today_hidden,
        ..
    } = banzuke;
    let over = banzuke.over();
    let members = banzuke.rows.len();
    let anyone = banzuke.rows.iter().any(|r| r.place.is_some());
    let lines = banzuke
        .rows
        .into_iter()
        .map(|row| {
            let mine = row.user_id == me;
            view! { <Line row me=mine monday today today_hidden members /> }
        })
        .collect_view();
    view! {
        <div class="banzuke-scroll">
            <div class="banzuke-paper flex flex-col">
                <div class="flex items-center justify-between gap-3 px-3 pb-2 pt-3">
                    <span class="font-pixel text-2xl leading-none tracking-[0.3em]">"番付"</span>
                    // 千秋楽: the last day, the tournament is decided. 本場所: under way
                    <span
                        class="hanko-ink rotate-[-4deg] rounded-[4px] px-1.5 py-1 font-pixel text-xs leading-none"
                        data-tooltip=move || {
                            if over {
                                td_string!(i18n.get_locale(), banzuke.final_state)
                            } else {
                                td_string!(i18n.get_locale(), banzuke.live)
                            }
                        }
                        data-testid="banzuke-state"
                        data-over=over.to_string()
                    >
                        {if over { "千秋楽" } else { "本場所" }}
                    </span>
                </div>
                {(!anyone)
                    .then(|| {
                        view! {
                            <p class="ink-soft px-3 pb-3 text-sm italic">{t!(i18n, banzuke.empty)}</p>
                        }
                    })}
                <ol class="flex flex-col pb-1">{lines}</ol>
                {today_hidden
                    .then(|| {
                        view! {
                            <p class="ink-soft mx-3 mb-3 mt-1 rounded-md border border-dashed border-current/30 px-3 py-2 text-xs" data-testid="banzuke-today-hidden">
                                {t!(i18n, banzuke.today_hidden)} " "
                                <A
                                    href=daily
                                    exact=true
                                    attr:class="font-medium text-primary underline-offset-4 hover:underline"
                                >
                                    {t!(i18n, banzuke.play)} " →"
                                </A>
                            </p>
                        }
                    })}
            </div>
        </div>
    }
}

/// How the points and the awards work, folded away.
#[component]
fn Rules() -> impl IntoView {
    let i18n = use_i18n();
    view! {
        <details class="group rounded-lg border bg-muted/30 px-3 py-2 text-sm">
            <summary class="cursor-pointer select-none font-medium text-muted-foreground hover:text-foreground">
                {t!(i18n, banzuke.rules.title)}
            </summary>
            <div class="mt-2 flex flex-col gap-1.5 text-xs text-muted-foreground">
                <p>{t!(i18n, banzuke.rules.points)}</p>
                <p>{t!(i18n, banzuke.rules.titles)}</p>
                <p>{t!(i18n, banzuke.rules.pace)}</p>
                <p>{t!(i18n, banzuke.rules.members)}</p>
            </div>
        </details>
    }
}

/// The group page's banzuke card. Tells `crowns` who was last week's yokozuna.
#[component]
pub fn BanzukeCard(group: GroupContext, crowns: RwSignal<Vec<i64>>) -> impl IntoView {
    let i18n = use_i18n();
    // The Monday asked for, `None`: this week
    let monday = RwSignal::new(None::<i64>);
    let banzuke = RwSignal::new(None::<Banzuke>);
    let id = Memo::new(move |_| group.page.with(|p| p.as_ref().map(|p| p.id)));

    // Again when the page reloads (a member more or less, today's daily finished)
    Effect::new(move |_| {
        let Some(id) = id.get() else { return };
        group.page.track();
        let asked = monday.get();
        spawn_local(async move {
            match load_banzuke(id, asked).await {
                Ok(b) => {
                    crowns.set(b.champions.clone());
                    banzuke.set(Some(b));
                }
                Err(e) => error_toast(e),
            }
        });
    });

    let shown =
        move || banzuke.with(|b| b.as_ref().map(|b| (b.monday, b.first, b.today, b.over())));
    let go = move |delta: i64| {
        if let Some((m, ..)) = shown() {
            monday.set(Some(m + delta));
        }
    };
    let me = move || group.page.with(|p| p.as_ref().map_or(0, |p| p.me));
    view! {
        <Card class="gap-3" attr:data-testid="banzuke">
            <CardHeader>
                <div class="flex w-full flex-wrap items-center justify-between gap-3">
                    <div>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, banzuke.title)} <span class="font-pixel text-sm text-primary">"番付"</span>
                        </CardTitle>
                        <CardDescription>
                            {move || {
                                shown()
                                    .map(|(m, ..)| {
                                        let l = i18n.get_locale();
                                        format!("{} – {}", date_short(l, m), date_short(l, m + 6))
                                    })
                            }}
                        </CardDescription>
                    </div>
                    <div class="flex gap-1.5">
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown().is_none_or(|(m, first, ..)| m <= first)
                            attr:aria-label=move || t_string!(i18n, banzuke.earlier)
                            on:click=move |_| go(-7)
                        >
                            "← " <span class="hidden sm:inline">{t!(i18n, banzuke.earlier)}</span>
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            size=ButtonSize::Sm
                            attr:disabled=move || shown().is_none_or(|(m, _, today, _)| m + 6 >= today)
                            attr:aria-label=move || t_string!(i18n, banzuke.later)
                            on:click=move |_| go(7)
                        >
                            <span class="hidden sm:inline">{t!(i18n, banzuke.later)}</span> " →"
                        </Button>
                    </div>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                {move || match banzuke.get() {
                    Some(b) => {
                        let me = me();
                        view! {
                            <div class="grid grid-cols-1 gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
                                <Scroll banzuke=b.clone() me />
                                <Awards banzuke=b me />
                            </div>
                        }
                            .into_any()
                    }
                    None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                }}
                <Rules />
            </CardContent>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plates_read_top_to_bottom() {
        assert_eq!(plate_text(Some(Title::Yokozuna)), "横綱");
        assert_eq!(plate_text(Some(Title::Komusubi)), "小結");
        assert_eq!(plate_text(Some(Title::Maegashira(3))), "前頭三");
        assert_eq!(plate_text(Some(Title::Maegashira(12))), "前頭十二");
        assert_eq!(plate_text(None), "休場");
    }
}
