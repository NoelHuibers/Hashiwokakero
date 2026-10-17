use std::time::Duration;

use common::{Board, Difficulty, GenerateParams, Puzzle};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::{
    daily_check, daily_hint, daily_leaderboard, daily_reveal, daily_today, start_daily, submit_daily,
};
use crate::components::board::{BoardMode, BoardView};
use crate::components::celebration::{Petals, Stamp};
use crate::components::leaderboard::LeaderboardTable;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::ui::progress::Progress;
use crate::model::{
    CHECK_PENALTY_MS, DailyInfo, HINT_PENALTY_MS, Leaderboard, MS_PER_DAY, SubmitResult,
    WEEKDAY_KANJI, daily_difficulty, daily_number, date_string, format_duration, pretty_date,
    weekday,
};
use crate::state::{Auth, DailyGame, Game, Source, Status, ToastKind, toast};
use crate::pages::play::SeoIntro;
use crate::seo::{PageMeta, site_ld};

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
        self.info.with(|i| i.as_ref().is_some_and(|i| i.attempt.is_some()))
    }

    fn finished(&self) -> bool {
        self.info
            .with(|i| i.as_ref().and_then(|i| i.attempt.as_ref()).is_some_and(|a| a.finished()))
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
        self.skew.set(info.server_now - js_sys::Date::now() as i64);
        if let Some(puzzle) = &info.puzzle {
            let loaded = self.game.status.get_untracked() != Status::Empty
                && self.game.board.with_untracked(|b| b.puzzle == *puzzle);
            if !loaded {
                self.load_puzzle(&info, puzzle.clone());
            }
        }
        self.info.set(Some(info));
    }

    fn load_puzzle(&self, info: &DailyInfo, puzzle: Puzzle) {
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
                game.status.set(if a.revealed { Status::Revealed } else { Status::Won });
            }
            _ => {
                game.restore_progress(&puzzle);
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
                Err(e) => toast(ToastKind::Error, format!("Could not load the daily puzzle: {e}")),
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
fn Stat(label: &'static str, jp: &'static str, #[prop(into)] value: Signal<String>) -> impl IntoView {
    view! {
        <div class="flex min-w-[4.5rem] flex-col items-center rounded-lg border bg-card px-3 py-1.5 shadow-xs">
            <span class="font-pixel text-xl tabular-nums leading-tight">{move || value.get()}</span>
            <span class="text-[10px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
                {label} " · " {jp}
            </span>
        </div>
    }
}

/// The board before the start: an empty sea in the fog.
#[component]
fn Cover(daily: Daily) -> impl IntoView {
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
                Err(e) => toast(ToastKind::Error, format!("Could not start: {e}")),
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
                    <h2 class="text-2xl font-black">"Today's bridge 今日の橋"</h2>
                    <p class="text-sm text-muted-foreground">
                        "The puzzle appears when you press start, the clock runs on the server. Hints add 30 s, checks 10 s, revealing the solution leaves you unranked."
                    </p>
                    <Button
                        size=ButtonSize::Lg
                        class="w-full text-base"
                        attr:disabled=move || daily.busy.get() || daily.info.with(Option::is_none)
                        on:click=start
                    >
                        {move || if daily.busy.get() { "Preparing…" } else { "Start 始め" }}
                    </Button>
                    <p class="text-xs text-muted-foreground">
                        {move || match auth.user.get() {
                            Some(u) if !u.guest => format!("Playing as {}", u.name),
                            Some(u) => format!("Playing as {} (guest)", u.name),
                            None => "No account needed, you'll play as a guest.".to_string(),
                        }}
                    </p>
                </Card>
            </div>
        </div>
    }
}

#[component]
fn ResultCard(daily: Daily) -> impl IntoView {
    let auth = expect_context::<Auth>();
    let share = move |_| {
        let Some(info) = daily.info.get_untracked() else { return };
        let rank = daily
            .result
            .get_untracked()
            .map(|r| (r.rank, r.finishers))
            .or_else(|| daily.ranking.with_untracked(|lb| lb.as_ref().and_then(|lb| lb.me.as_ref().map(|m| (m.rank, lb.finishers)))));
        let origin = window().location().origin().unwrap_or_default();
        let text = format!(
            "Hashi 橋 Daily #{} · {} {}\n⏱ {}{}\n{origin}",
            daily_number(info.day),
            WEEKDAY_KANJI[weekday(info.day)],
            info.difficulty.label(),
            format_duration(daily.elapsed_ms()),
            rank.map(|(r, n)| format!(" · 🏅 #{r} of {n}")).unwrap_or_default(),
        );
        let _ = window().navigator().clipboard().write_text(&text);
        toast(ToastKind::Success, "Result copied to the clipboard.");
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
                        <h2 class="text-xl font-black">"Solved by the SAT solver"</h2>
                        <p class="text-sm text-muted-foreground">
                            "This one doesn't count for the ranking. See you tomorrow!"
                        </p>
                    }
                        .into_any()
                } else {
                    view! {
                        <Stamp kanji="完" class="size-20 text-5xl" />
                        <h2 class="text-2xl font-black">"見事！ Well done!"</h2>
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
                                    Some((r, n)) => format!("Rank {r} of {n} today"),
                                    None => "Ranking…".to_string(),
                                }
                            }}
                        </p>
                    }
                        .into_any()
                }
            }}
            <div class="flex w-full flex-col gap-2">
                <Button on:click=share class="w-full">"Share result"</Button>
                <A
                    href="/practice"
                    attr:class="inline-flex min-h-10 items-center justify-center text-sm text-primary underline-offset-4 hover:underline"
                >
                    "More puzzles in practice mode →"
                </A>
            </div>
            {move || {
                auth.user
                    .get()
                    .filter(|u| u.guest)
                    .map(|u| {
                        view! {
                            <div class="w-full rounded-lg border border-dashed bg-muted/50 p-3 text-left text-sm">
                                <p>
                                    "You're on the board as " <b>{u.name}</b> "."
                                </p>
                                <A href="/account" attr:class="font-medium text-primary underline-offset-4 hover:underline">
                                    "Claim your name to keep your streak on every device →"
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
    view! {
        <Card class="gap-3">
            <CardHeader>
                <div class="flex w-full items-baseline justify-between">
                    <CardTitle class="flex items-baseline gap-2 text-lg">
                        "Today's ranking" <span class="font-pixel text-sm text-primary">"順位"</span>
                    </CardTitle>
                    <A
                        href="/ranks"
                        attr:class="-mr-2 inline-flex min-h-9 items-center px-2 text-xs text-primary underline-offset-4 hover:underline"
                    >
                        "All →"
                    </A>
                </div>
                <CardDescription>
                    {move || {
                        daily
                            .ranking
                            .with(|lb| lb.as_ref().map(|lb| lb.finishers))
                            .map(|n| format!("{n} {} crossed today", if n == 1 { "player has" } else { "players have" }))
                            .unwrap_or_else(|| "Loading…".to_string())
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
    let today = Signal::derive(move || daily.info.with(|i| i.as_ref().map(|i| i.day)));
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-lg">
                    "This week" <span class="font-pixel text-sm text-primary">"週"</span>
                </CardTitle>
                <CardDescription>"Like a newspaper puzzle, the bridges get harder towards Sunday."</CardDescription>
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
                                    title=d.label()
                                >
                                    <span class="font-pixel text-base leading-none">{WEEKDAY_KANJI[wd]}</span>
                                    <span class="mt-1 font-pixel text-[11px] opacity-80">{d.kanji()}</span>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
                <p class="flex items-center justify-between text-sm">
                    <span class="text-muted-foreground">"Next bridge in"</span>
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
        daily.now.set(js_sys::Date::now() as i64);
        daily.reload();
        if let Ok(handle) = set_interval_with_handle(
            move || daily.now.set(js_sys::Date::now() as i64),
            Duration::from_millis(100),
        ) {
            on_cleanup(move || handle.clear());
        }
    });

    // A new day started while the page was open
    Effect::new(move |_| {
        let Some(day) = daily.info.with(|i| i.as_ref().map(|i| i.day)) else { return };
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
                    let end = js_sys::Date::now() as i64 + daily.skew.get_untracked();
                    daily.update_attempt(|a| {
                        a.finished_at = Some(end);
                        a.time_ms = Some(result.time_ms);
                    });
                    daily.result.set(Some(result));
                    daily.refresh_ranking();
                }
                Err(e) => {
                    toast(ToastKind::Error, format!("Could not submit: {e}"));
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
                Ok(None) => toast(ToastKind::Info, "Nothing to hint, every bridge is in place."),
                Err(e) => toast(ToastKind::Error, e.to_string()),
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
                        toast(ToastKind::Success, "So far so good: every bridge belongs to the solution.");
                    } else {
                        let n = wrong.len();
                        toast(
                            ToastKind::Error,
                            format!("{n} bridge{} not part of the solution.", if n == 1 { " is" } else { "s are" }),
                        );
                        game.wrong.set(wrong);
                        set_timeout(move || game.wrong.set(vec![]), Duration::from_millis(2400));
                    }
                }
                Err(e) => toast(ToastKind::Error, e.to_string()),
            }
        });
    };

    let give_up = move |_| {
        let Some(day) = daily.day() else { return };
        let sure = window()
            .confirm_with_message("Let the SAT solver show the solution? Today's run won't be ranked.")
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
                    let missing: Vec<usize> = (0..current.len()).filter(|&e| current[e] != solution[e]).collect();
                    let step = (2000 / missing.len().max(1) as u64).clamp(10, 80);
                    for (k, edge) in missing.into_iter().enumerate() {
                        let value = solution[edge];
                        set_timeout(
                            move || game.bridges.update(|b| b[edge] = value),
                            Duration::from_millis(step * k as u64),
                        );
                    }
                }
                Err(e) => toast(ToastKind::Error, e.to_string()),
            }
        });
    };

    let playing = Signal::derive(move || daily.started() && !daily.finished() && game.status.get() == Status::Playing);
    let check_state = Memo::new(move |_| game.board.with(|b| game.bridges.with(|br| b.check(br))));
    let islands = Memo::new(move |_| game.board.with(|b| b.puzzle.islands.len()));
    let islands_done = Memo::new(move |_| {
        game.board.with(|b| {
            check_state.with(|c| b.puzzle.islands.iter().zip(&c.degrees).filter(|(i, d)| i.required == **d).count())
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
                            {pretty_date(info.day)}
                        </p>
                        <h1 class="text-2xl font-black sm:text-3xl">
                            {format!("Daily bridge #{}", daily_number(info.day))}
                        </h1>
                        <div class="mt-1 flex gap-1.5">
                            <Badge variant=BadgeVariant::Default>
                                {format!("{} {}", info.difficulty.kanji(), info.difficulty.label())}
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
            title="Hashi · Daily Hashiwokakero (Bridges) puzzle, free online"
            description="A new Hashiwokakero (Bridges) puzzle every day, the same for everyone: cute Japanese pixel art, a leaderboard and streaks, and every puzzle has exactly one solution."
            path="/"
            json_ld=site_ld()
        />
        <section class="mx-auto max-w-7xl px-4 py-6 sm:px-6">
            <div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_380px]">
                <div class="flex min-w-0 flex-col gap-4">
                    <div class="flex min-h-16 flex-wrap items-end justify-between gap-3">
                        {header}
                        <div class="flex gap-2">
                            <Stat
                                label="Time"
                                jp="時間"
                                value=Signal::derive(move || {
                                    if daily.started() { format_duration(daily.elapsed_ms()) } else { "–".into() }
                                })
                            />
                            <Stat label="Moves" jp="手" value=Signal::derive(move || game.moves.get().to_string()) />
                            <Stat
                                label="Penalty"
                                jp="罰"
                                value=Signal::derive(move || {
                                    let p = daily
                                        .info
                                        .with(|i| i.as_ref().and_then(|i| i.attempt.as_ref()).map(|a| a.penalty_ms()))
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
                                    <p class="font-pixel">"読み込み中 · loading"</p>
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
                                    " islands complete"
                                </span>
                                <span>
                                    {move || {
                                        let c = check_state.with(|c| c.components);
                                        if c > 1 { format!("{c} separate groups") } else { "all connected".to_string() }
                                    }}
                                </span>
                            </div>
                            <Progress
                                value=Signal::derive(move || {
                                    let n = islands.get();
                                    if n == 0 { 0.0 } else { islands_done.get() as f64 / n as f64 * 100.0 }
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
                                "↶ Undo"
                            </Button>
                            <Button
                                variant=ButtonVariant::Outline
                                attr:disabled=move || !playing.get() || game.redo_stack.with(Vec::is_empty)
                                on:click=move |_| game.redo()
                            >
                                "Redo ↷"
                            </Button>
                            <Button
                                variant=ButtonVariant::Ghost
                                attr:disabled=move || !playing.get()
                                on:click=move |_| game.restart()
                            >
                                "Clear"
                            </Button>
                            <div class="contents sm:ml-auto sm:flex sm:gap-2">
                                <Button
                                    variant=ButtonVariant::Secondary
                                    attr:disabled=move || !playing.get()
                                    attr:title=format!("+{} s", CHECK_PENALTY_MS / 1000)
                                    on:click=check
                                >
                                    "Check" <span class="hidden sm:inline">" 確認"</span> <span class="text-xs opacity-70">"+10s"</span>
                                </Button>
                                <Button
                                    variant=ButtonVariant::Secondary
                                    attr:disabled=move || !playing.get()
                                    attr:title=format!("+{} s", HINT_PENALTY_MS / 1000)
                                    on:click=hint
                                >
                                    "Hint" <span class="hidden sm:inline">" 助言"</span> <span class="text-xs opacity-70">"+30s"</span>
                                </Button>
                                <Button
                                    variant=ButtonVariant::Ghost
                                    attr:disabled=move || !playing.get()
                                    on:click=give_up
                                >
                                    "Give up"
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
