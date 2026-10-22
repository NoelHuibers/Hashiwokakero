//! The group page's card of group puzzles (課題 kadai).
//!
//! The open challenges with who solved them and how long they stay open, the applause for the
//! viewer's closed ones, and the archive of closed ones with their final boards.

use common::Board;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::{load_group_puzzles, seen_group_puzzle_kudos};
use crate::components::board::{BoardMode, BoardView};
use crate::components::leaderboard::RankCell;
use crate::components::ui::button::{Button, ButtonSize};
use crate::components::ui::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::i18n::{t, t_string, use_i18n};
use crate::lang::prefix;
use crate::model::{
    GroupPuzzles, Kudos, MAX_OPEN, MyPuzzle, PuzzleSummary, colour_var, format_duration,
};
use crate::pages::kadai::author::{Applause, StatsGrid};
use crate::pages::kadai::{byline, left_text, puzzle_path, puzzle_title};
use crate::state::error_toast;

use super::GroupContext;

/// Who solved it, a pip per player in their colour, filled once they did.
#[component]
fn Pips(summary: PuzzleSummary) -> impl IntoView {
    view! {
        <div class="flex flex-wrap gap-1" aria-hidden="true">
            {summary
                .pips
                .iter()
                .map(|p| {
                    let style = if p.solved {
                        format!("background: {}", colour_var(p.colour))
                    } else {
                        format!("box-shadow: inset 0 0 0 1.5px {}", colour_var(p.colour))
                    };
                    view! { <span class="h-2 w-3 rounded-full opacity-90" style=style></span> }
                })
                .collect_view()}
        </div>
    }
}

/// The viewer's state of a puzzle, at the end of its row.
#[component]
fn MineChip(mine: MyPuzzle) -> impl IntoView {
    let i18n = use_i18n();
    let chip =
        "inline-flex shrink-0 items-center gap-1 rounded-full px-2.5 py-1 text-xs font-medium";
    match mine {
        MyPuzzle::Author => view! {
            <span class=format!("{chip} border border-primary/40 text-primary")>
                <span class="font-pixel" aria-hidden="true">"作"</span>
                {t!(i18n, kadai.card.yours)}
            </span>
        }
        .into_any(),
        MyPuzzle::Fresh => view! {
            <span class=format!("{chip} bg-primary text-primary-foreground shadow-sm")>
                {t!(i18n, kadai.card.play)} " →"
            </span>
        }
        .into_any(),
        MyPuzzle::Playing => view! {
            <span class=format!("{chip} bg-secondary text-secondary-foreground")>
                {t!(i18n, kadai.card.continue_playing)} " →"
            </span>
        }
        .into_any(),
        MyPuzzle::Solved { time_ms, .. } => view! {
            <span class=format!("{chip} bg-success-light text-success-dark")>
                "✓ " <span class="font-pixel tabular-nums">{format_duration(time_ms)}</span>
            </span>
        }
        .into_any(),
        MyPuzzle::GaveUp => view! {
            <span class=format!("{chip} bg-muted text-muted-foreground")>{t!(i18n, kadai.card.gave_up)}</span>
        }
        .into_any(),
    }
}

/// An author's hanko in their colour, or a plain one for a former member.
fn author_style(summary: &PuzzleSummary) -> String {
    summary.author.as_ref().map_or_else(String::new, |a| {
        format!("--member: {}", colour_var(a.colour))
    })
}

/// An open puzzle: its name, author and size, who solved it, the time left and the way in.
#[component]
fn OpenRow(summary: PuzzleSummary, now: i64) -> impl IntoView {
    let i18n = use_i18n();
    let path = puzzle_path(summary.group_id, summary.id);
    let href = move || format!("{}{path}", prefix(i18n.get_locale()));
    let solved = summary.solved();
    let players = summary.players();
    let closes_at = summary.closes_at;
    let id = summary.id;
    let style = author_style(&summary);
    let mine = summary.mine;
    let shown = summary.clone();
    let row = summary.clone();
    view! {
        <li>
            <A
                href=href
                attr:class="group flex items-center gap-3 rounded-lg border bg-card p-3 shadow-xs transition-colors hover:border-primary/60 hover:bg-accent/40"
                attr:data-puzzle=id
            >
                <span
                    class="nakama-stamp grid size-10 shrink-0 rotate-[-5deg] place-items-center rounded-md font-pixel text-lg"
                    style=style
                    aria-hidden="true"
                >
                    "課"
                </span>
                <div class="flex min-w-0 flex-1 flex-col gap-1">
                    <p class="truncate font-bold leading-tight" data-testid="kadai-row-title">
                        {move || puzzle_title(i18n.get_locale(), &shown)}
                    </p>
                    <p class="truncate text-xs text-muted-foreground">
                        {move || {
                            let l = i18n.get_locale();
                            format!(
                                "{} · {}×{} · {}",
                                byline(l, &row),
                                row.width,
                                row.height,
                                left_text(l, now, closes_at),
                            )
                        }}
                    </p>
                    <div class="flex flex-wrap items-center gap-2">
                        <Pips summary=summary />
                        <span class="text-xs text-muted-foreground" data-testid="kadai-row-solved">
                            {move || t_string!(i18n, kadai.solved_of, solved, players)}
                        </span>
                    </div>
                </div>
                <MineChip mine />
            </A>
        </li>
    }
}

/// A closed puzzle: its final board, small, and who won.
#[component]
fn ClosedRow(summary: PuzzleSummary) -> impl IntoView {
    let i18n = use_i18n();
    let path = puzzle_path(summary.group_id, summary.id);
    let href = move || format!("{}{path}", prefix(i18n.get_locale()));
    let solved = summary.solved();
    let players = summary.players();
    let picture = summary.board.clone().map(|b| {
        let board = Signal::stored(Board::new(b.puzzle));
        let bridges = Signal::stored(b.solution);
        view! {
            <div class="w-16 shrink-0 sm:w-20" aria-hidden="true">
                <BoardView board bridges mode=BoardMode::View max_height="5rem" />
            </div>
        }
    });
    let winner = summary.winner.clone().map(|(name, time)| {
        view! {
            <span class="flex min-w-0 items-center gap-1.5 text-xs">
                <RankCell rank=1 />
                <span class="truncate font-medium">{name}</span>
                <span class="font-pixel tabular-nums text-muted-foreground">{format_duration(time)}</span>
            </span>
        }
    });
    let closed_id = summary.id;
    let shown = summary.clone();
    let row = summary;
    view! {
        <li>
            <A
                href=href
                attr:class="flex items-center gap-3 rounded-lg p-2 transition-colors hover:bg-accent/40"
                attr:data-closed=closed_id
            >
                {picture}
                <div class="flex min-w-0 flex-1 flex-col gap-1">
                    <p class="truncate text-sm font-bold leading-tight">{move || puzzle_title(i18n.get_locale(), &shown)}</p>
                    <p class="truncate text-xs text-muted-foreground">
                        {move || format!("{} · {}", byline(i18n.get_locale(), &row), t_string!(i18n, kadai.solved_of, solved, players))}
                    </p>
                    {winner}
                </div>
            </A>
        </li>
    }
}

/// The applause for one of the viewer's puzzles that closed, until they say thanks.
#[component]
fn KudosPanel(kudos: Kudos, on_seen: Callback<i64>) -> impl IntoView {
    let i18n = use_i18n();
    let id = kudos.puzzle.id;
    let path = puzzle_path(kudos.puzzle.group_id, id);
    let href = move || format!("{}{path}", prefix(i18n.get_locale()));
    let name = kudos.puzzle.clone();
    let busy = RwSignal::new(false);
    let seen = move |_| {
        busy.set(true);
        spawn_local(async move {
            match seen_group_puzzle_kudos(id).await {
                Ok(()) => on_seen.run(id),
                Err(e) => error_toast(e),
            }
            busy.try_set(false);
        });
    };
    view! {
        <div class="flex flex-col gap-3 rounded-xl border-2 border-primary/50 bg-primary/5 p-3" data-testid="kadai-kudos">
            <p class="text-sm">
                {move || t_string!(i18n, kadai.kudos.title, name = puzzle_title(i18n.get_locale(), &name))}
            </p>
            <Applause stats=kudos.stats.clone() />
            <StatsGrid stats=kudos.stats open=false />
            <div class="flex flex-wrap items-center gap-2">
                <Button size=ButtonSize::Sm attr:disabled=move || busy.get() on:click=seen>
                    {t!(i18n, kadai.kudos.thanks)} " ありがとう"
                </Button>
                <A href=href attr:class="text-sm text-primary underline-offset-4 hover:underline">
                    {t!(i18n, kadai.kudos.see)} " →"
                </A>
            </div>
        </div>
    }
}

/// The card on the group page.
#[component]
pub fn KadaiCard(group: GroupContext) -> impl IntoView {
    let i18n = use_i18n();
    let puzzles = RwSignal::new(None::<GroupPuzzles>);
    let id = Memo::new(move |_| group.page.with(|p| p.as_ref().map(|p| p.id)));
    Effect::new(move |_| {
        let Some(id) = id.get() else { return };
        spawn_local(async move {
            match load_group_puzzles(id).await {
                Ok(list) => {
                    puzzles.try_set(Some(list));
                }
                Err(e) => error_toast(e),
            }
        });
    });
    let on_seen = Callback::new(move |seen: i64| {
        puzzles.update(|p| {
            if let Some(p) = p {
                p.kudos.retain(|k| k.puzzle.id != seen);
            }
        });
    });
    let create = move || {
        let group = id.get().unwrap_or(0);
        format!("{}/create?group={group}", prefix(i18n.get_locale()))
    };
    view! {
        <Card class="gap-3" attr:data-testid="kadai-card">
            <CardHeader>
                <div class="flex flex-wrap items-start justify-between gap-2">
                    <div class="min-w-0">
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, kadai.title)} <span class="font-pixel text-sm text-primary">"課題"</span>
                        </CardTitle>
                        <CardDescription>{t!(i18n, kadai.card.text)}</CardDescription>
                    </div>
                    <Show when=move || puzzles.with(|p| p.as_ref().is_some_and(|p| p.can_post > 0))>
                        <A
                            href=create
                            attr:class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md border bg-background px-3 text-sm font-medium shadow-xs hover:bg-accent"
                            attr:data-testid="kadai-new"
                        >
                            <span class="font-pixel text-primary" aria-hidden="true">"作"</span>
                            {t!(i18n, kadai.card.new)}
                        </A>
                    </Show>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                {move || match puzzles.get() {
                    None => view! { <p class="text-sm text-muted-foreground">{t!(i18n, common.game.loading)}</p> }.into_any(),
                    Some(list) => {
                        let now = list.server_now;
                        let kudos = list
                            .kudos
                            .into_iter()
                            .map(|k| view! { <KudosPanel kudos=k on_seen /> })
                            .collect_view();
                        let open = if list.open.is_empty() {
                            view! {
                                <div class="flex flex-col items-start gap-2 rounded-lg border border-dashed bg-muted/40 p-4" data-testid="kadai-empty">
                                    <p class="text-sm font-medium">{t!(i18n, kadai.card.none)}</p>
                                    <A href=create attr:class="text-sm font-medium text-primary underline-offset-4 hover:underline">
                                        {t!(i18n, kadai.card.make)} " →"
                                    </A>
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {
                                <ul class="flex flex-col gap-2" data-testid="kadai-open">
                                    {list.open.into_iter().map(|s| view! { <OpenRow summary=s now /> }).collect_view()}
                                </ul>
                            }
                                .into_any()
                        };
                        let full = (list.can_post == 0).then(|| view! {
                            <p class="text-xs text-muted-foreground">{move || t_string!(i18n, kadai.card.full, max = MAX_OPEN)}</p>
                        });
                        let closed = (!list.closed.is_empty()).then(|| view! {
                            <details class="group/archive" data-testid="kadai-archive">
                                <summary class="flex cursor-pointer list-none items-center gap-2 text-sm font-medium text-muted-foreground hover:text-foreground">
                                    <span class="font-pixel text-primary" aria-hidden="true">"済"</span>
                                    {t!(i18n, kadai.card.closed)}
                                    <span class="transition-transform group-open/archive:rotate-90" aria-hidden="true">"›"</span>
                                </summary>
                                <ul class="mt-2 flex flex-col gap-1">
                                    {list.closed.into_iter().map(|s| view! { <ClosedRow summary=s /> }).collect_view()}
                                </ul>
                            </details>
                        });
                        view! { {kudos} {open} {full} {closed} }.into_any()
                    }
                }}
            </CardContent>
        </Card>
    }
}

// Rendering on the server needs its features
#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::i18n::{Locale, use_i18n};
    use crate::model::{FinalBoard, Pip, PuzzleAuthor, PuzzleStats};

    fn summary(id: i64, mine: MyPuzzle, open: bool) -> PuzzleSummary {
        PuzzleSummary {
            id,
            group_id: 1,
            title: (id != 2).then(|| format!("Puzzle {id}")),
            author: Some(PuzzleAuthor {
                user_id: 1,
                name: "aiko".into(),
                guest: false,
                colour: 4,
            }),
            width: 9,
            height: 7,
            islands: 12,
            created_at: 0,
            closes_at: 3 * crate::model::MS_PER_DAY,
            open,
            pips: vec![
                Pip {
                    colour: 1,
                    solved: true,
                },
                Pip {
                    colour: 2,
                    solved: false,
                },
            ],
            mine,
            board: (!open).then(|| FinalBoard {
                puzzle: common::Puzzle::from_grid(&[vec![1, 0, 1], vec![0, 0, 0]]),
                solution: vec![1],
            }),
            winner: (!open).then(|| ("chen".into(), 61_000)),
        }
    }

    fn render(view: fn() -> AnyView) -> String {
        _ = any_spawner::Executor::init_tokio();
        let owner = Owner::new_root(None);
        let html = owner.with(|| {
            crate::app::provide_server_i18n();
            use_i18n().set_locale(Locale::en);
            provide_context(leptos_router::location::RequestUrl::new("/groups/1"));
            view! { <leptos_router::components::Router>{view()}</leptos_router::components::Router> }
                .to_html()
        });
        drop(owner);
        html
    }

    #[tokio::test]
    async fn the_rows_of_the_card_render() {
        let rows = render(|| {
            view! {
                <ul>
                    <OpenRow summary=summary(1, MyPuzzle::Fresh, true) now=0 />
                    <OpenRow summary=summary(2, MyPuzzle::Author, true) now=0 />
                    <OpenRow summary=summary(3, MyPuzzle::Playing, true) now=0 />
                    <OpenRow summary=summary(4, MyPuzzle::Solved { time_ms: 61_000, rank: Some(1) }, true) now=0 />
                    <OpenRow summary=summary(5, MyPuzzle::GaveUp, true) now=0 />
                    <ClosedRow summary=summary(6, MyPuzzle::Fresh, false) />
                </ul>
            }
            .into_any()
        });
        for text in [
            "Puzzle 1",
            "s puzzle",
            "by aiko · 9×7 · 3d left",
            "1 of 2 solved",
            "Play",
            "yours",
            "Continue",
            "1:01.0",
            "gave up",
            "data-closed=\"6\"",
            "chen",
            "href=\"/groups/1/puzzles/1\"",
        ] {
            assert!(rows.contains(text), "{text}: {rows}");
        }
        let kudos = render(|| {
            let stats = PuzzleStats {
                players: 2,
                solved: 2,
                fastest_ms: Some(61_000),
                ..PuzzleStats::default()
            };
            view! {
                <KudosPanel
                    kudos=Kudos { puzzle: summary(7, MyPuzzle::Author, false), stats }
                    on_seen=Callback::new(|_| ())
                />
            }
            .into_any()
        });
        for text in [
            "Your puzzle “Puzzle 7” has closed",
            "Applause from 2 solvers!",
            "Thanks!",
            "2 / 2",
        ] {
            assert!(kudos.contains(text), "{text}: {kudos}");
        }
    }
}
