use common::{Board, Pos, Puzzle};
use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::board::{BoardMode, BoardView};
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::components::ui::kbd::Kbd;
use crate::components::ui::separator::Separator;
use crate::seo::{PageMeta, RULES_FAQ, faq_ld};

/// A bridge between two cells (x, y) with its count.
type ExampleBridge = ((u8, u8), (u8, u8), u8);

/// Builds a solved example from its bridges, numbers follow from the bridge counts.
fn example(width: u8, height: u8, bridges: &[ExampleBridge]) -> (Board, Vec<u8>) {
    let mut puzzle = Puzzle::empty(width, height);
    for &((ax, ay), (bx, by), n) in bridges {
        for (x, y) in [(ax, ay), (bx, by)] {
            let pos = Pos::new(x, y);
            let current = puzzle.island_at(pos).map_or(0, |i| puzzle.islands[i].required);
            puzzle.set(pos, current + n);
        }
    }
    let board = Board::new(puzzle);
    let mut counts = board.empty_bridges();
    for &((ax, ay), (bx, by), n) in bridges {
        let a = board.puzzle.island_at(Pos::new(ax, ay));
        let b = board.puzzle.island_at(Pos::new(bx, by));
        if let Some(e) = a.zip(b).and_then(|(a, b)| board.edge_between(a, b)) {
            counts[e] = n;
        }
    }
    (board, counts)
}

#[component]
fn Rule(number: &'static str, title: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class="flex gap-4">
            <span class="grid size-12 shrink-0 place-items-center rounded-lg bg-primary font-pixel text-2xl text-primary-foreground shadow-md">
                {number}
            </span>
            <div>
                <h3 class="text-lg font-bold">{title}</h3>
                <p class="text-muted-foreground">{children()}</p>
            </div>
        </div>
    }
}

#[component]
fn Clause(children: Children) -> impl IntoView {
    view! {
        <code class="terminal block overflow-x-auto rounded-md px-3 py-2 font-pixel text-[13px]">{children()}</code>
    }
}

#[component]
pub fn RulesPage() -> impl IntoView {
    let (board, bridges) = example(
        5,
        5,
        &[
            ((0, 0), (4, 0), 2),
            ((0, 0), (0, 4), 1),
            ((4, 0), (4, 2), 2),
            ((2, 2), (4, 2), 1),
            ((2, 2), (2, 4), 2),
            ((0, 4), (2, 4), 1),
        ],
    );

    view! {
        <PageMeta
            title="How to play Hashiwokakero · Rules and solving tips · Hashi"
            description="The rules of Hashiwokakero (Bridges) with a solved example, the deductions that crack most puzzles without guessing, and how a SAT solver proves a solution unique."
            path="/rules"
            json_ld=faq_ld(&RULES_FAQ)
        />
        <section class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
            <div class="mb-8 flex items-end gap-4">
                <span class="vertical-jp font-pixel text-4xl leading-none text-primary">"橋をかけろ"</span>
                <div>
                    <p class="font-pixel text-sm text-primary">"遊び方 · asobikata"</p>
                    <h1 class="text-4xl font-black">"How to play"</h1>
                    <p class="mt-2 max-w-2xl text-muted-foreground">
                        "Hashiwokakero (橋をかけろ, “build bridges!”) is a logic puzzle published by Nikoli. Connect all islands with bridges so that every island has exactly as many bridges as its number says."
                    </p>
                </div>
            </div>

            <div class="grid grid-cols-1 items-center gap-8 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
                <div class="flex flex-col gap-6">
                    <Rule number="一" title="Count the bridges">
                        "The number on an island is how many bridges end there."
                    </Rule>
                    <Rule number="二" title="One or two">
                        "Islands can be joined by a single wooden bridge or a double vermilion bridge, never more."
                    </Rule>
                    <Rule number="三" title="Straight, no crossings">
                        "Bridges run horizontally or vertically and must not cross other bridges or islands."
                    </Rule>
                    <Rule number="四" title="One archipelago">
                        "In the end every island has to be reachable from every other one."
                    </Rule>
                </div>
                <div class="flex flex-col items-center gap-3">
                    <BoardView
                        board=Signal::stored(board)
                        bridges=Signal::stored(bridges)
                        mode=BoardMode::View
                        max_height="22rem"
                        framed=true
                    />
                    <p class="text-sm text-muted-foreground">"A solved puzzle"</p>
                </div>
            </div>

            <Separator class="my-10" />

            <div class="grid grid-cols-1 gap-6 md:grid-cols-2">
                <Card>
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            "Controls" <span class="font-pixel text-sm text-primary">"操作"</span>
                        </CardTitle>
                    </CardHeader>
                    <CardContent class="flex flex-col gap-2 text-sm text-muted-foreground">
                        <p>"Click the water between two islands to build a bridge, again for a second one, a third click removes them."</p>
                        <p>"Or drag from an island towards its neighbour, or click two islands one after the other."</p>
                        <p><Kbd>"Right click"</Kbd> " / " <Kbd>"Shift"</Kbd> " + click removes a bridge."</p>
                        <p>
                            <Kbd>"Ctrl Z"</Kbd> " undo · " <Kbd>"Ctrl Y"</Kbd> " redo · " <Kbd>"H"</Kbd>
                            " hint"
                        </p>
                        <p>"Finished islands turn green, overfull ones red. Stuck? Ask the SAT solver for a hint or to check your bridges."</p>
                    </CardContent>
                </Card>

                <Card>
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            "Under the hood" <span class="font-pixel text-sm text-primary">"仕組み"</span>
                        </CardTitle>
                    </CardHeader>
                    <CardContent class="flex flex-col gap-3 text-sm text-muted-foreground">
                        <p>
                            "Every possible bridge " <i>"e"</i> " between two neighbouring islands gets two boolean variables: "
                            <i>"e₁"</i> " (at least one bridge) and " <i>"e₂"</i> " (a second bridge), with "
                            <i>"e₂ → e₁"</i> " so that every arrangement has exactly one model."
                        </p>
                        <Clause>"¬e₂ ∨ e₁"</Clause>
                        <p>"An island with number n needs exactly n of its variables to be true, written as clauses over all combinations. Crossing bridges exclude each other:"</p>
                        <Clause>"¬h₁ ∨ ¬v₁"</Clause>
                        <p>
                            "Connectivity is hard to express in CNF, so it is added lazily: whenever the solver returns separate groups of islands, a clause demanding a bridge out of such a group is added and the solver runs again."
                        </p>
                        <p>
                            "The generator grows a random archipelago and proves uniqueness by blocking the found solution and asking the solver for another one. Try the different solver modes in the "
                            <A href="/lab" attr:class="text-primary underline underline-offset-4">"lab"</A>
                            "."
                        </p>
                    </CardContent>
                </Card>
            </div>

            <Separator class="my-10" />

            <section class="flex flex-col gap-5" id="faq">
                <div>
                    <p class="font-pixel text-sm text-primary">"よくある質問 · FAQ"</p>
                    <h2 class="text-2xl font-black">"Questions about Hashiwokakero"</h2>
                </div>
                <dl class="grid gap-x-8 gap-y-5 md:grid-cols-2">
                    {RULES_FAQ
                        .iter()
                        .map(|(q, a)| {
                            view! {
                                <div class="flex flex-col gap-1.5">
                                    <dt class="font-bold">{*q}</dt>
                                    <dd class="text-sm leading-relaxed text-muted-foreground">{*a}</dd>
                                </div>
                            }
                        })
                        .collect_view()}
                </dl>
            </section>
        </section>
    }
}
