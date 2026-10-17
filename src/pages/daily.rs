use std::time::Duration;

use common::{Board, Difficulty, GenerateParams, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::{
    daily_check, daily_hint, daily_leaderboard, daily_reveal, daily_today, start_daily,
    submit_daily,
};
use crate::components::board::{BoardMode, BoardView};
use crate::components::celebration::{Petals, Stamp};
use crate::components::layout::use_localized;
use crate::components::leaderboard::LeaderboardTable;
use crate::components::share::ShareBar;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::progress::Progress;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_long, difficulty_name, error_text, localized};
use crate::model::{
    CHECK_PENALTY_MS, DailyInfo, HINT_PENALTY_MS, Leaderboard, MS_PER_DAY, SubmitResult,
    WEEKDAY_KANJI, daily_difficulty, daily_number, date_string, format_duration, weekday,
};
use crate::pages::play::{SeoIntro, islands_percent};
use crate::seo::{PageMeta, site_ld};
use crate::state::{Auth, DailyGame, Game, Source, Status, ToastKind, toast};

fn params_for(day: i64) -> GenerateParams {
    let difficulty = daily_difficulty(day);
    let (width, height) = difficulty.size();
    GenerateParams {
        width,
        height,
        difficulty,
        seed: 0,
    }
}

/// The browser clock, unix time in ms.
#[expect(
    clippy::cast_possible_truncation,
    reason = "`Date.now()` is a whole number of ms, far inside the i64 range"
)]
fn browser_now() -> i64 {
    js_sys::Date::now() as i64
}

/// `05:12:09` until the next daily.
fn countdown(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

#[derive(Clone, Copy)]
struct Daily {
    game: Game,
    info: RwSignal<Option<DailyInfo>>,
    /// Server clock minus client clock.
    skew: RwSignal<i64>,
    /// Client clock, ticking while the page is open.
    now: RwSignal<i64>,
    result: RwSignal<Option<SubmitResult>>,
    ranking: RwSignal<Option<Leaderboard>>,
    busy: RwSignal<bool>,
}

impl Daily {
    fn server_now(&self) -> i64 {
        self.now.get() + self.skew.get()
    }

    fn day(&self) -> Option<i64> {
        self.info.with_untracked(|i| i.as_ref().map(|i| i.day))
    }

    fn started(&self) -> bool {
        self.info
            .with(|i| i.as_ref().is_some_and(|i| i.attempt.is_some()))
    }

    fn finished(&self) -> bool {
        self.info.with(|i| {
            i.as_ref()
                .and_then(|i| i.attempt.as_ref())
                .is_some_and(super::super::model::AttemptInfo::finished)
        })
    }

    /// Running time including penalties, or the final time.
    fn elapsed_ms(&self) -> i64 {
        let now = self.server_now();
        self.info.with(|i| {
            let Some(a) = i.as_ref().and_then(|i| i.attempt.as_ref()) else {
                return 0;
            };
            match (a.time_ms, a.finished_at) {
                (Some(t), _) => t,
                (None, Some(end)) => end - a.started_at + a.penalty_ms(),
                _ => now - a.started_at + a.penalty_ms(),
            }
        })
    }

    fn apply(&self, info: DailyInfo) {
        self.skew.set(info.server_now - browser_now());
        if let Some(puzzle) = &info.puzzle {
            let loaded = self.game.status.get_untracked() != Status::Empty
                && self.game.board.with_untracked(|b| b.puzzle == *puzzle);
            if !loaded {
                self.load_puzzle(&info, puzzle);
            }
        }
        self.info.set(Some(info));
    }

    fn load_puzzle(&self, info: &DailyInfo, puzzle: &Puzzle) {
        let game = self.game;
        let source = Source::Daily {
            date: date_string(info.day),
            params: params_for(info.day),
        };
        game.set_puzzle(puzzle.clone(), None, Some(true), source);
        match &info.attempt {
            Some(a) if a.finished() => {
                if let Some(bridges) = &a.bridges {
                    game.bridges.set(bridges.clone());
                }
                game.status.set(if a.revealed {
                    Status::Revealed
                } else {
                    Status::Won
                });
            }
            _ => {
                game.restore_progress(puzzle);
            }
        }
    }

    fn refresh_ranking(&self) {
        let ranking = self.ranking;
        spawn_local(async move {
            if let Ok(lb) = daily_leaderboard(None).await {
                ranking.set(Some(lb));
            }
        });
    }

    fn reload(&self) {
        let this = *self;
        spawn_local(async move {
            match daily_today().await {
                Ok(info) => this.apply(info),
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
        self.refresh_ranking();
    }

    fn update_attempt(&self, f: impl FnOnce(&mut crate::model::AttemptInfo)) {
        self.info.update(|i| {
            if let Some(a) = i.as_mut().and_then(|i| i.attempt.as_mut()) {
                f(a);
            }
        });
    }
}

#[component]
fn Stat(
    #[prop(into)] label: Signal<String>,
    jp: &'static str,
    #[prop(into)] value: Signal<String>,
) -> impl IntoView {
    view! {
        <div class="flex min-w-[4.5rem] flex-col items-center rounded-lg border bg-card px-3 py-1.5 shadow-xs">
            <span class="font-pixel text-xl tabular-nums leading-tight">{move || value.get()}</span>
            <span class="text-[10px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
                {move || label.get()} " · " {jp}
            </span>
        </div>
    }
}

/// The board before the start: an empty sea in the fog.
#[component]
fn Cover(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let size = Signal::derive(move || {
        let day = daily.info.with(|i| i.as_ref().map(|i| i.day)).unwrap_or(0);
        daily_difficulty(day).size()
    });
    let board = Signal::derive(move || {
        let (w, h) = size.get();
        Board::new(Puzzle::empty(w, h))
    });
    let start = move |_| {
        if daily.busy.get_untracked() {
            return;
        }
        daily.busy.set(true);
        spawn_local(async move {
            match start_daily().await {
                Ok(info) => {
                    daily.apply(info);
                    auth.refresh();
                }
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
            daily.busy.set(false);
        });
    };
    view! {
        <div class="relative">
            <div class="opacity-60 blur-[2px]">
                <BoardView
                    board=board
                    bridges=Signal::derive(move || board.with(Board::empty_bridges))
                    mode=BoardMode::View
                    framed=true
                />
            </div>
            <div class="absolute inset-0 grid place-items-center p-4">
                <Card class="w-full max-w-sm items-center gap-3 px-6 text-center shadow-2xl">
                    <span class="hanko grid size-16 rotate-[-6deg] place-items-center rounded-lg font-pixel text-4xl">
                        "橋"
                    </span>
                    <h2 class="text-2xl font-black">{t!(i18n, daily.cover.title)} " 今日の橋"</h2>
                    <p class="text-sm text-muted-foreground">
                        {t!(i18n, daily.cover.text)}
                    </p>
                    <Button
                        size=ButtonSize::Lg
                        class="w-full text-base"
                        attr:disabled=move || daily.busy.get() || daily.info.with(Option::is_none)
                        on:click=start
                    >
                        {move || if daily.busy.get() { t_string!(i18n, daily.cover.preparing).to_string() } else { format!("{} 始め", t_string!(i18n, daily.cover.start)) }}
                    </Button>
                    <p class="text-xs text-muted-foreground">
                        {move || match auth.user.get() {
                            Some(u) if !u.guest => t_string!(i18n, daily.cover.playing_as, name = u.name),
                            Some(u) => t_string!(i18n, daily.cover.playing_as_guest, name = u.name),
                            None => t_string!(i18n, daily.cover.no_account).to_string(),
                        }}
                    </p>
                </Card>
            </div>
        </div>
    }
}

#[component]
fn ResultCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let auth = expect_context::<Auth>();
    let practice = use_localized("/practice");
    let account = use_localized("/account");
    let share_text = move || {
        let Some(info) = daily.info.get_untracked() else {
            return String::new();
        };
        let rank = daily
            .result
            .get_untracked()
            .map(|r| (r.rank, r.finishers))
            .or_else(|| {
                daily.ranking.with_untracked(|lb| {
                    lb.as_ref()
                        .and_then(|lb| lb.me.as_ref().map(|m| (m.rank, lb.finishers)))
                })
            });
        let locale = i18n.get_locale_untracked();
        let origin = window().location().origin().unwrap_or_default();
        let headline = td_string!(
            locale,
            daily.share,
            number = daily_number(info.day),
            weekday = WEEKDAY_KANJI[weekday(info.day)],
            difficulty = difficulty_name(locale, info.difficulty)
        );
        let rank = rank
            .map(|(rank, total)| {
                format!(" · {}", td_string!(locale, daily.share_rank, rank, total))
            })
            .unwrap_or_default();
        format!(
            "{headline}\n⏱ {}{rank}\n{origin}{}",
            format_duration(daily.elapsed_ms()),
            localized(locale, "/"),
        )
    };
    let revealed = move || daily.game.status.get() == Status::Revealed;
    view! {
        <Card class="items-center gap-3 overflow-hidden px-6 text-center">
            {move || {
                if revealed() {
                    view! {
                        <span class="hanko grid size-16 rotate-[-6deg] place-items-center rounded-lg font-pixel text-4xl">
                            "解"
                        </span>
                        <h2 class="text-xl font-black">{t!(i18n, daily.result.revealed_title)}</h2>
                        <p class="text-sm text-muted-foreground">{t!(i18n, daily.result.revealed_text)}</p>
                    }
                        .into_any()
                } else {
                    view! {
                        <Stamp kanji="完" class="size-20 text-5xl" />
                        <h2 class="text-2xl font-black">"見事！ " {t!(i18n, common.game.well_done)}</h2>
                        <p class="font-pixel text-3xl tabular-nums">{move || format_duration(daily.elapsed_ms())}</p>
                        <p class="text-sm text-muted-foreground">
                            {move || {
                                let rank = daily
                                    .result
                                    .get()
                                    .map(|r| (r.rank, r.finishers))
                                    .or_else(|| {
                                        daily
                                            .ranking
                                            .with(|lb| {
                                                lb.as_ref().and_then(|lb| lb.me.as_ref().map(|m| (m.rank, lb.finishers)))
                                            })
                                    });
                                match rank {
                                    Some((rank, total)) => t_string!(i18n, daily.result.rank, rank, total),
                                    None => t_string!(i18n, daily.result.ranking).to_string(),
                                }
                            }}
                        </p>
                    }
                        .into_any()
                }
            }}
            <div class="flex w-full flex-col gap-2">
                <ShareBar text=share_text />
                <A
                    href=practice
                    attr:class="inline-flex min-h-10 items-center justify-center text-sm text-primary underline-offset-4 hover:underline"
                >
                    {t!(i18n, daily.result.more)}
                </A>
            </div>
            {move || {
                auth.user
                    .get()
                    .filter(|u| u.guest)
                    .map(|u| {
                        view! {
                            <div class="w-full rounded-lg border border-dashed bg-muted/50 p-3 text-start text-sm">
                                <p>{t!(i18n, daily.result.on_board, name = u.name, <b> = <b />)}</p>
                                <A href=account attr:class="font-medium text-primary underline-offset-4 hover:underline">
                                    {t!(i18n, daily.result.claim)}
                                </A>
                            </div>
                        }
                    })
            }}
        </Card>
    }
}

#[component]
fn RankingCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let ranks = use_localized("/ranks");
    view! {
        <Card class="gap-3">
            <CardHeader>
                <div class="flex w-full items-baseline justify-between">
                    <CardTitle class="flex items-baseline gap-2 text-lg">
                        {t!(i18n, daily.ranking.title)} <span class="font-pixel text-sm text-primary">"順位"</span>
                    </CardTitle>
                    <A
                        href=ranks
                        attr:class="-me-2 inline-flex min-h-9 items-center px-2 text-xs text-primary underline-offset-4 hover:underline"
                    >
                        {t!(i18n, daily.ranking.all)}
                    </A>
                </div>
                <CardDescription>
                    {move || {
                        daily
                            .ranking
                            .with(|lb| lb.as_ref().map(|lb| lb.finishers)).map_or_else(|| t_string!(i18n, common.game.loading).to_string(), |count| t_string!(i18n, daily.ranking.crossed, count))
                    }}
                </CardDescription>
            </CardHeader>
            <CardContent>
                {move || {
                    daily
                        .ranking
                        .get()
                        .map(|lb| view! { <LeaderboardTable entries=lb.entries me=lb.me limit=10 /> })
                }}
            </CardContent>
        </Card>
    }
}

#[component]
fn WeekCard(daily: Daily) -> impl IntoView {
    let i18n = use_i18n();
    let today = Signal::derive(move || daily.info.with(|i| i.as_ref().map(|i| i.day)));
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-lg">
                    {t!(i18n, daily.week.title)} <span class="font-pixel text-sm text-primary">"週"</span>
                </CardTitle>
                <CardDescription>{t!(i18n, daily.week.text)}</CardDescription>
            </CardHeader>
            <CardContent class="flex flex-col gap-3">
                <div class="grid grid-cols-7 gap-1">
                    {(0..7usize)
                        .map(|wd| {
                            let d = match wd {
                                0 => Difficulty::Easy,
                                1 | 2 => Difficulty::Medium,
                                3 | 4 => Difficulty::Hard,
                                5 => Difficulty::Expert,
                                _ => Difficulty::Oni,
                            };
                            let is_today = move || today.get().is_some_and(|t| weekday(t) == wd);
                            view! {
                                <div
                                    class="flex flex-col items-center rounded-md border py-1.5"
                                    class=(["border-primary", "bg-primary", "text-primary-foreground"], is_today)
                                    title=move || difficulty_name(i18n.get_locale(), d)
                                >
                                    <span class="font-pixel text-base leading-none">{WEEKDAY_KANJI[wd]}</span>
                                    <span class="mt-1 font-pixel text-[11px] opacity-80">{d.kanji()}</span>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
                <p class="flex items-center justify-between text-sm">
                    <span class="text-muted-foreground">{t!(i18n, daily.week.next)}</span>
                    <span class="font-pixel tabular-nums">
                        {move || {
                            today
                                .get()
                                .map(|t| countdown((t + 1) * MS_PER_DAY - daily.server_now()))
                                .unwrap_or_default()
                        }}
                    </span>
                </p>
            </CardContent>
        </Card>
    }
}

#[component]
pub fn DailyPage() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<DailyGame>().0;
    let daily = Daily {
        game,
        info: RwSignal::new(None),
        skew: RwSignal::new(0),
        now: RwSignal::new(0),
        result: RwSignal::new(None),
        ranking: RwSignal::new(None),
        busy: RwSignal::new(false),
    };

    // Load today's daily and keep a clock running (browser only)
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        daily.now.set(browser_now());
        daily.reload();
        if let Ok(handle) = set_interval_with_handle(
            move || daily.now.set(browser_now()),
            Duration::from_millis(100),
        ) {
            on_cleanup(move || handle.clear());
        }
    });

    // A new day started while the page was open
    Effect::new(move |_| {
        let Some(day) = daily.info.with(|i| i.as_ref().map(|i| i.day)) else {
            return;
        };
        if daily.server_now() >= (day + 1) * MS_PER_DAY && daily.finished() {
            daily.info.set(None);
            daily.reload();
        }
    });

    // Submit as soon as the board is solved
    Effect::new(move |_| {
        if game.status.get() != Status::Won || daily.finished() {
            return;
        }
        let Some(day) = daily.day() else { return };
        let bridges = game.bridges.get_untracked();
        let moves = game.moves.get_untracked();
        daily.busy.set(true);
        spawn_local(async move {
            match submit_daily(day, bridges, moves).await {
                Ok(result) => {
                    let end = browser_now() + daily.skew.get_untracked();
                    daily.update_attempt(|a| {
                        a.finished_at = Some(end);
                        a.time_ms = Some(result.time_ms);
                    });
                    daily.result.set(Some(result));
                    daily.refresh_ranking();
                }
                Err(e) => {
                    toast(ToastKind::Error, move |l| error_text(l, &e));
                    game.status.set(Status::Playing);
                }
            }
            daily.busy.set(false);
        });
    });

    let hint = move |_| {
        let Some(day) = daily.day() else { return };
        let bridges = game.bridges.get_untracked();
        spawn_local(async move {
            match daily_hint(day, bridges).await {
                Ok(Some((edge, value))) => {
                    daily.update_attempt(|a| a.hints += 1);
                    game.set_edges(&[(edge, value)]);
                    game.hints.update(|h| *h += 1);
                    game.hint.set(Some(edge));
                    set_timeout(
                        move || {
                            if game.hint.get_untracked() == Some(edge) {
                                game.hint.set(None);
                            }
                        },
                        Duration::from_millis(3000),
                    );
                }
                Ok(None) => toast(ToastKind::Info, |l| {
                    td_string!(l, common.toast.nothing_to_hint).to_string()
                }),
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
    };

    let check = move |_| {
        let Some(day) = daily.day() else { return };
        let bridges = game.bridges.get_untracked();
        spawn_local(async move {
            match daily_check(day, bridges).await {
                Ok(wrong) => {
                    daily.update_attempt(|a| a.checks += 1);
                    if wrong.is_empty() {
                        crate::audio::play(crate::audio::Sfx::Correct);
                        toast(ToastKind::Success, |l| {
                            td_string!(l, common.toast.all_correct).to_string()
                        });
                    } else {
                        let count = wrong.len();
                        toast(ToastKind::Error, move |l| {
                            td_string!(l, common.toast.wrong_bridges, count)
                        });
                        game.wrong.set(wrong);
                        set_timeout(move || game.wrong.set(vec![]), Duration::from_millis(2400));
                    }
                }
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
    };

    let give_up = move |_| {
        let Some(day) = daily.day() else { return };
        let sure = window()
            .confirm_with_message(t_string!(i18n, daily.give_up_confirm))
            .unwrap_or(false);
        if !sure {
            return;
        }
        spawn_local(async move {
            match daily_reveal(day).await {
                Ok(solution) => {
                    game.status.set(Status::Revealed);
                    daily.update_attempt(|a| a.revealed = true);
                    let current = game.bridges.get_untracked();
                    let missing: Vec<usize> = (0..current.len())
                        .filter(|&e| current[e] != solution[e])
                        .collect();
                    let step = (2000 / missing.len().max(1) as u64).clamp(10, 80);
                    for (k, edge) in missing.into_iter().enumerate() {
                        let value = solution[edge];
                        set_timeout(
                            move || game.bridges.update(|b| b[edge] = value),
                            Duration::from_millis(step * k as u64),
                        );
                    }
                }
                Err(e) => toast(ToastKind::Error, move |l| error_text(l, &e)),
            }
        });
    };

    let playing = Signal::derive(move || {
        daily.started() && !daily.finished() && game.status.get() == Status::Playing
    });
    let check_state = Memo::new(move |_| game.board.with(|b| game.bridges.with(|br| b.check(br))));
    let islands = Memo::new(move |_| game.board.with(|b| b.puzzle.islands.len()));
    let islands_done = Memo::new(move |_| {
        game.board.with(|b| {
            check_state.with(|c| {
                b.puzzle
                    .islands
                    .iter()
                    .zip(&c.degrees)
                    .filter(|(i, d)| i.required == **d)
                    .count()
            })
        })
    });

    let header = move || {
        daily.info.get().map(|info| {
            let wd = weekday(info.day);
            view! {
                <div class="flex items-center gap-3">
                    <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl">
                        {WEEKDAY_KANJI[wd]}
                    </span>
                    <div>
                        <p class="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground">
                            {move || date_long(i18n.get_locale(), info.day)}
                        </p>
                        <h1 class="text-2xl font-black sm:text-3xl">
                            {t!(i18n, daily.title, number = daily_number(info.day))}
                        </h1>
                        <div class="mt-1 flex gap-1.5">
                            <Badge variant=BadgeVariant::Default>
                                {info.difficulty.kanji()} " " {move || difficulty_name(i18n.get_locale(), info.difficulty)}
                            </Badge>
                            <Badge variant=BadgeVariant::Muted>
                                {let (w, h) = info.difficulty.size();
                                format!("{w}×{h}")}
                            </Badge>
                        </div>
                    </div>
                </div>
            }
        })
    };

    view! {
        <PageMeta
            title=Signal::derive(move || t_string!(i18n, daily.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, daily.meta.description).to_string())
            path="/"
            json_ld=site_ld(i18n.get_locale_untracked())
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-4">
                    <div class="flex min-h-16 flex-wrap items-end justify-between gap-3">
                        {header}
                        <div class="flex gap-2">
                            <Stat
                                label=Signal::derive(move || t_string!(i18n, common.game.time).to_string())
                                jp="時間"
                                value=Signal::derive(move || {
                                    if daily.started() { format_duration(daily.elapsed_ms()) } else { "–".into() }
                                })
                            />
                            <Stat label=Signal::derive(move || t_string!(i18n, common.game.moves).to_string()) jp="手" value=Signal::derive(move || game.moves.get().to_string()) />
                            <Stat
                                label=Signal::derive(move || t_string!(i18n, daily.penalty).to_string())
                                jp="罰"
                                value=Signal::derive(move || {
                                    let p = daily
                                        .info
                                        .with(|i| i.as_ref().and_then(|i| i.attempt.as_ref()).map(super::super::model::AttemptInfo::penalty_ms))
                                        .unwrap_or(0);
                                    format!("+{}s", p / 1000)
                                })
                            />
                        </div>
                    </div>

                    {move || {
                        if daily.info.with(Option::is_none) {
                            view! {
                                <div class="grid aspect-square w-full place-items-center rounded-2xl border border-dashed text-muted-foreground">
                                    <p class="font-pixel">"読み込み中 · " {t!(i18n, common.game.loading)}</p>
                                </div>
                            }
                                .into_any()
                        } else if !daily.started() {
                            view! { <Cover daily=daily /> }.into_any()
                        } else {
                            view! {
                                <div class="relative">
                                    <BoardView
                                        board=game.board
                                        bridges=game.bridges
                                        mode=BoardMode::Play
                                        on_cycle=Callback::new(move |(edge, delta)| game.cycle(edge, delta))
                                        wrong=game.wrong
                                        hint=game.hint
                                        framed=true
                                    />
                                    <Show when=move || game.status.get() == Status::Won>
                                        <Petals />
                                    </Show>
                                </div>
                            }
                                .into_any()
                        }
                    }}

                    <Show when=move || daily.started()>
                        <div class="flex flex-col gap-2">
                            <div class="flex items-center justify-between text-xs text-muted-foreground">
                                <span>
                                    <span class="font-pixel text-foreground">
                                        {move || format!("{} / {}", islands_done.get(), islands.get())}
                                    </span>
                                    " " {t!(i18n, common.game.islands_complete)}
                                </span>
                                <span>
                                    {move || {
                                        let c = check_state.with(|c| c.components);
                                        if c > 1 { t_string!(i18n, common.game.groups, count = c) } else { t_string!(i18n, common.game.all_connected).to_string() }
                                    }}
                                </span>
                            </div>
                            <Progress
                                value=Signal::derive(move || {
                                    islands_percent(islands_done.get(), islands.get())
                                })
                                class="h-2.5"
                            />
                        </div>
                        <div class="grid grid-cols-3 gap-2 sm:flex sm:flex-wrap">
                            <Button
                                variant=ButtonVariant::Outline
                                attr:disabled=move || !playing.get() || game.undo_stack.with(Vec::is_empty)
                                on:click=move |_| game.undo()
                            >
                                "↶ " {t!(i18n, common.game.undo)}
                            </Button>
                            <Button
                                variant=ButtonVariant::Outline
                                attr:disabled=move || !playing.get() || game.redo_stack.with(Vec::is_empty)
                                on:click=move |_| game.redo()
                            >
                                {t!(i18n, common.game.redo)} " ↷"
                            </Button>
                            <Button
                                variant=ButtonVariant::Ghost
                                attr:disabled=move || !playing.get()
                                on:click=move |_| game.restart()
                            >
                                {t!(i18n, common.game.clear)}
                            </Button>
                            <div class="contents sm:ms-auto sm:flex sm:gap-2">
                                <Button
                                    variant=ButtonVariant::Secondary
                                    attr:disabled=move || !playing.get()
                                    attr:title=format!("+{} s", CHECK_PENALTY_MS / 1000)
                                    on:click=check
                                >
                                    {t!(i18n, common.game.check)} <span class="hidden sm:inline">" 確認"</span> <span class="text-xs opacity-70">"+10s"</span>
                                </Button>
                                <Button
                                    variant=ButtonVariant::Secondary
                                    attr:disabled=move || !playing.get()
                                    attr:title=format!("+{} s", HINT_PENALTY_MS / 1000)
                                    on:click=hint
                                >
                                    {t!(i18n, common.game.hint)} <span class="hidden sm:inline">" 助言"</span> <span class="text-xs opacity-70">"+30s"</span>
                                </Button>
                                <Button
                                    variant=ButtonVariant::Ghost
                                    attr:disabled=move || !playing.get()
                                    on:click=give_up
                                >
                                    {t!(i18n, daily.give_up)}
                                </Button>
                            </div>
                        </div>
                    </Show>
                </div>

                <aside class="flex flex-col gap-4">
                    <Show when=move || daily.finished() || matches!(game.status.get(), Status::Won | Status::Revealed)>
                        <ResultCard daily=daily />
                    </Show>
                    <RankingCard daily=daily />
                    <WeekCard daily=daily />
                </aside>
            </div>
        </section>
        <SeoIntro />
    }
}
