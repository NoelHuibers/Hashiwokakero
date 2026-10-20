//! The daily puzzle page, the home page: one puzzle a day for everyone, timed and ranked.

pub mod about;
mod actions;
pub mod cover;
pub mod ranking;
pub mod result;
pub mod rivals;
pub mod state;
pub mod week;
pub mod win;

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{
    PaidAction, PaidOutcome, daily_paid_action, daily_recover_actions, daily_reveal, daily_today,
    submit_daily,
};
use crate::components::board::{BoardMode, BoardView};
use crate::components::celebration::Petals;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::ui::progress::Progress;
use crate::i18n::{t, t_string, td_string, use_i18n};
use crate::lang::{date_long, difficulty_name};
use crate::model::{
    AttemptInfo, CHECK_PENALTY_MS, HINT_PENALTY_MS, MS_PER_DAY, WEEKDAY_KANJI, daily_difficulty,
    daily_number, format_duration, weekday,
};
use crate::pages::play::islands_percent;
use crate::seo::{PageMeta, site_ld};
use crate::state::{Auth, DailyGame, Status, ToastKind, error_toast, load, toast};

use about::About;
use cover::Cover;
use ranking::RankingCard;
use result::ResultCard;
use rivals::RivalsCard;
use state::{Daily, LOG_KEY, SavedLog, browser_now, browser_utc_offset};
use week::WeekCard;
use win::WinCard;

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

/// Today on the server's clock, the day of `daily_today`. `None` in the browser, which doesn't
/// know the server's day before it loads the daily.
#[cfg_attr(
    feature = "ssr",
    expect(clippy::unnecessary_wraps, reason = "`None` in the browser")
)]
fn server_today() -> Option<i64> {
    #[cfg(feature = "ssr")]
    {
        Some(crate::server::daily::today())
    }
    #[cfg(not(feature = "ssr"))]
    {
        None
    }
}

#[component]
pub fn DailyPage() -> impl IntoView {
    let i18n = use_i18n();
    let game = expect_context::<DailyGame>().0;
    let auth = expect_context::<Auth>();
    let daily = Daily {
        game,
        actions: actions::Actions::new(auth),
        info: RwSignal::new(None),
        skew: RwSignal::new(0),
        now: RwSignal::new(0),
        result: RwSignal::new(None),
        admired: RwSignal::new(false),
        unsent: RwSignal::new(false),
        ranking: RwSignal::new(None),
        busy: RwSignal::new(false),
        refreshing: StoredValue::new(false),
        logged: StoredValue::new(None),
    };

    Effect::new(move |_| daily.actions.sync(&daily));

    // Load today's daily and keep a clock running (browser only)
    Effect::new(move |first: Option<()>| {
        if first.is_some() {
            return;
        }
        daily.now.set(browser_now());
        if let Ok(handle) = set_interval_with_handle(
            move || daily.now.set(browser_now()),
            Duration::from_millis(100),
        ) {
            on_cleanup(move || handle.clear());
        }
        // Back from another tab or window, which may have started or finished the daily
        let focus = window_event_listener(leptos::ev::focus, move |_| daily.refresh());
        let visible = window_event_listener_untyped("visibilitychange", move |_| {
            if document().visibility_state() == web_sys::VisibilityState::Visible {
                daily.refresh();
            }
        });
        on_cleanup(move || {
            focus.remove();
            visible.remove();
        });
    });

    // Wait for the known identity before fetching its attempt, and refetch after a local
    // registration/sign-in refresh. An old request cannot install another identity's info.
    Effect::new(move |_| {
        if auth.loaded.get() {
            auth.user.with(|u| u.as_ref().map(|u| u.id));
            untrack(|| daily.reload());
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

    // Every bridge change while the clock runs, for the server to check the result: a click
    // changes one bridge and counts a move, hints, undo and restart don't
    Effect::new(move |_| daily.record_moves());

    // Submit as soon as the board is solved
    Effect::new(move |_| {
        if !daily.ready_to_submit() {
            return;
        }
        let Some(day) = daily.day() else { return };
        let bridges = game.bridges.get_untracked();
        let moves = game.moves.get_untracked();
        let Some(guard) = daily.actions.capture(&daily) else {
            return;
        };
        daily.busy.set(true);
        // Flush synchronously: effect scheduling must not decide whether the final move is in
        // the log. The observer can run afterward without appending it twice.
        untrack(|| daily.record_moves());
        spawn_local(async move {
            // The final move has already been logged
            let log = load::<SavedLog>(LOG_KEY)
                .filter(|l| l.day == day)
                .map(|l| l.events)
                .unwrap_or_default();
            let response = submit_daily(day, bridges, moves, log, Some(browser_utc_offset())).await;
            if !guard.current(&daily) {
                daily.busy.try_set(false);
                return;
            }
            match response {
                Ok(result) => {
                    // The win card shows the stamps, no toasts on top of it
                    daily.submitted(result, browser_now() + daily.skew.get_untracked());
                    daily.refresh_ranking();
                }
                Err(e) => {
                    error_toast(e);
                    let info = daily_today().await.ok();
                    if !guard.current(&daily) {
                        daily.busy.try_set(false);
                        return;
                    }
                    daily.submit_failed(info);
                }
            }
            daily.busy.set(false);
        });
    });

    let paid = move |action| {
        let Some(guard) = daily.actions.begin(&daily, Some(action)) else {
            return;
        };
        spawn_local(async move {
            if !guard.current(&daily) {
                daily.actions.finish(&guard);
                return;
            }
            let candidates = daily.actions.recovery_intents();
            let mut recovered_reply = None;
            if !candidates.is_empty() {
                let recovery = daily_recover_actions(
                    guard.day(),
                    guard.started_at(),
                    candidates.clone(),
                    guard.user(),
                )
                .await;
                if !guard.current(&daily) {
                    daily.actions.finish(&guard);
                    return;
                }
                match recovery {
                    Ok(recovery) => {
                        daily.actions.recovered(recovery.as_ref(), &candidates);
                        let Some(recovery) = recovery else {
                            daily.actions.finish(&guard);
                            return;
                        };
                        if recovery.intent.action != action {
                            daily.actions.finish(&guard);
                            return;
                        }
                        recovered_reply = Some(recovery.reply);
                    }
                    Err(e) => {
                        // Unknown recovery stays locked; retrying cannot fall through to a charge.
                        daily.actions.finish(&guard);
                        error_toast(e);
                        return;
                    }
                }
            }
            let Some(intent) = daily.actions.purchase(&guard, action, || {
                format!(
                    "{:016x}{:016x}",
                    crate::state::random_seed(),
                    crate::state::random_seed()
                )
            }) else {
                daily.actions.finish(&guard);
                toast(ToastKind::Error, |l| {
                    td_string!(l, daily.action_storage).to_string()
                });
                return;
            };
            let reply = if let Some(reply) = recovered_reply {
                Ok(reply)
            } else {
                daily_paid_action(
                    guard.day(),
                    intent.bridges.clone(),
                    intent.action,
                    intent.id.clone(),
                    intent.user,
                    intent.attempt_id.clone(),
                )
                .await
            };
            if !guard.current(&daily) {
                daily.actions.finish(&guard);
                return;
            }
            match reply {
                Ok(reply) => {
                    if let Some(outcome) = guard.accept(&daily, &intent.bridges, reply) {
                        match outcome {
                            PaidOutcome::Hint(Some((edge, value))) => {
                                game.set_edges(&[(edge, value)]);
                                game.hints.update(|h| *h += 1);
                                game.hint.set(Some(edge));
                                let highlight = daily.actions.capture(&daily).expect("active page");
                                set_timeout(
                                    move || {
                                        highlight.clear_hint(&daily, edge);
                                    },
                                    Duration::from_millis(3000),
                                );
                            }
                            PaidOutcome::Hint(None) => toast(ToastKind::Info, |l| {
                                td_string!(l, common.toast.nothing_to_hint).to_string()
                            }),
                            PaidOutcome::Check(wrong) => {
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
                                    let feedback = guard.clone();
                                    set_timeout(
                                        move || {
                                            feedback.clear_wrong(&daily);
                                        },
                                        Duration::from_millis(2400),
                                    );
                                }
                            }
                        }
                    }
                    daily.actions.resolved();
                }
                Err(e) => {
                    daily.actions.failed(&daily, &guard, &e);
                    error_toast(e);
                }
            }
            daily.actions.finish(&guard);
        });
    };
    let hint = move |_| paid(PaidAction::Hint);
    let check = move |_| paid(PaidAction::Check);

    let give_up = move |_| {
        if !untrack(|| daily.actions.can_start(&daily, None)) {
            return;
        }
        let sure = window()
            .confirm_with_message(t_string!(i18n, daily.give_up_confirm))
            .unwrap_or(false);
        if !sure {
            return;
        }
        let Some(guard) = daily.actions.begin(&daily, None) else {
            return;
        };
        spawn_local(async move {
            let reply = daily_reveal(guard.day()).await;
            if !guard.current(&daily) {
                daily.actions.finish(&guard);
                return;
            }
            match reply {
                Ok(solution) if solution.len() == guard.bridges.len() => {
                    game.status.set(Status::Revealed);
                    daily.update_attempt(|a| a.revealed = true);
                    let missing: Vec<usize> = (0..guard.bridges.len())
                        .filter(|&e| guard.bridges[e] != solution[e])
                        .collect();
                    let step = (2000 / missing.len().max(1) as u64).clamp(10, 80);
                    let mut frame = guard.clone();
                    for (k, edge) in missing.into_iter().enumerate() {
                        let value = solution[edge];
                        let before = frame.clone();
                        set_timeout(
                            move || {
                                before.reveal_step(&daily, edge, value);
                            },
                            Duration::from_millis(step * k as u64),
                        );
                        frame.bridges[edge] = value;
                    }
                }
                Ok(_) => {}
                Err(e) => error_toast(e),
            }
            daily.actions.finish(&guard);
        });
    };

    let playing = Signal::derive(move || daily.actions.can_start(&daily, None));
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

    // The browser reads the server's day back from the page while it hydrates (its own clock
    // may be on another day); opened in the browser, the page has none and waits for the daily
    let rendered = SharedValue::new(server_today).into_inner();
    let shown_day = daily.header_day(rendered);

    let header = move || {
        shown_day.get().map(|day| {
            let wd = weekday(day);
            let difficulty = daily_difficulty(day);
            view! {
                <div class="flex items-center gap-3">
                    <span class="hanko grid size-14 shrink-0 rotate-[-4deg] place-items-center rounded-lg font-pixel text-3xl">
                        {WEEKDAY_KANJI[wd]}
                    </span>
                    <div>
                        <p class="text-xs font-bold uppercase tracking-[0.2em] text-muted-foreground">
                            {move || date_long(i18n.get_locale(), day)}
                        </p>
                        <h1 class="text-2xl font-black sm:text-3xl">
                            {t!(i18n, daily.title, number = daily_number(day))}
                        </h1>
                        <div class="mt-1 flex gap-1.5">
                            <Badge variant=BadgeVariant::Default>
                                {difficulty.kanji()} " " {move || difficulty_name(i18n.get_locale(), difficulty)}
                            </Badge>
                            <Badge variant=BadgeVariant::Muted>
                                {let (w, h) = difficulty.size();
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
                                        .with(|i| i.as_ref().and_then(|i| i.attempt.as_ref()).map(AttemptInfo::penalty_ms))
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
                                        on_cycle=Callback::new(move |(edge, delta)| daily.actions.edit(&daily, || game.cycle(edge, delta)))
                                        wrong=game.wrong
                                        hint=game.hint
                                        framed=true
                                    />
                                    <Show when=move || game.status.get() == Status::Won>
                                        <Petals />
                                    </Show>
                                    <Show when=move || daily.celebrating()>
                                        <WinCard daily=daily />
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
                        <div role="status" aria-live="polite" data-testid="daily-action-status" class="text-sm text-muted-foreground">
                            {move || if daily.actions.pending.get() {
                                t_string!(i18n, daily.action_pending).to_string()
                            } else if daily.actions.recovery.get() {
                                t_string!(i18n, daily.action_recover).to_string()
                            } else if daily.actions.intent.with(Option::is_some) {
                                t_string!(i18n, daily.action_retry).to_string()
                            } else { String::new() }}
                        </div>
                        <Show when=move || daily.unsent.get() && !daily.finished()>
                            <div class="flex flex-wrap items-center gap-2 rounded-md border border-warning/40 bg-warning-light px-3 py-2 text-sm text-warning-dark">
                                <span class="flex-1">{t!(i18n, daily.submit_failed)}</span>
                                <Button
                                    size=ButtonSize::Sm
                                    attr:disabled=move || daily.busy.get()
                                    on:click=move |_| daily.submit_again()
                                >
                                    {t!(i18n, daily.submit_again)}
                                </Button>
                            </div>
                        </Show>
                        <div class="grid grid-cols-3 gap-2 sm:flex sm:flex-wrap" aria-busy=move || daily.actions.pending.get().to_string()>
                            <Button
                                variant=ButtonVariant::Outline
                                attr:disabled=move || !playing.get() || game.undo_stack.with(Vec::is_empty)
                                on:click=move |_| daily.actions.edit(&daily, || game.undo())
                            >
                                "↶ " {t!(i18n, common.game.undo)}
                            </Button>
                            <Button
                                variant=ButtonVariant::Outline
                                attr:disabled=move || !playing.get() || game.redo_stack.with(Vec::is_empty)
                                on:click=move |_| daily.actions.edit(&daily, || game.redo())
                            >
                                {t!(i18n, common.game.redo)} " ↷"
                            </Button>
                            <Button
                                variant=ButtonVariant::Ghost
                                attr:disabled=move || !playing.get()
                                on:click=move |_| daily.actions.edit(&daily, || game.restart())
                            >
                                {t!(i18n, common.game.clear)}
                            </Button>
                            <div class="contents sm:ms-auto sm:flex sm:gap-2">
                                <Button
                                    variant=ButtonVariant::Secondary
                                    attr:disabled=move || !daily.actions.can_start(&daily, Some(PaidAction::Check))
                                    attr:data-tooltip=format!("+{} s", CHECK_PENALTY_MS / 1000)
                                    on:click=check
                                >
                                    {t!(i18n, common.game.check)} <span class="hidden sm:inline">" 確認"</span> <span class="text-xs opacity-70">"+10s"</span>
                                </Button>
                                <Button
                                    variant=ButtonVariant::Secondary
                                    attr:disabled=move || !daily.actions.can_start(&daily, Some(PaidAction::Hint))
                                    attr:data-tooltip=format!("+{} s", HINT_PENALTY_MS / 1000)
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
                    <RivalsCard daily=daily />
                    <RankingCard daily=daily />
                    <WeekCard daily=daily />
                </aside>
            </div>
        </section>
        <About />
    }
}
