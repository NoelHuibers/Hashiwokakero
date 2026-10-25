use common::{Board, Pos, Puzzle};
use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::board::{BoardMode, BoardView};
use crate::components::layout::use_localized;
use crate::components::rules::Rules;
use crate::components::tips::{Tip, TipCards};
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::components::ui::kbd::Kbd;
use crate::components::ui::separator::Separator;
use crate::i18n::{t, t_string, use_i18n};
use crate::seo::{PageMeta, faq_ld, rules_faq};

/// A bridge between two cells (x, y) with its count.
type ExampleBridge = ((u8, u8), (u8, u8), u8);

/// Builds a solved example from its bridges, numbers follow from the bridge counts.
fn example(width: u8, height: u8, bridges: &[ExampleBridge]) -> (Board, Vec<u8>) {
    let mut puzzle = Puzzle::empty(width, height);
    for &((ax, ay), (bx, by), n) in bridges {
        for (x, y) in [(ax, ay), (bx, by)] {
            let pos = Pos::new(x, y);
            let current = puzzle
                .island_at(pos)
                .map_or(0, |i| puzzle.islands[i].required);
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
fn Clause(children: Children) -> impl IntoView {
    view! {
        <code dir="ltr" class="terminal block overflow-x-auto rounded-md px-3 py-2 font-pixel text-[13px]">{children()}</code>
    }
}

#[component]
pub fn RulesPage() -> impl IntoView {
    let i18n = use_i18n();
    let lab = use_localized("/lab");
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
            title=Signal::derive(move || t_string!(i18n, rules.meta.title).to_string())
            description=Signal::derive(move || t_string!(i18n, rules.meta.description).to_string())
            path="/rules"
            json_ld=faq_ld(i18n.get_locale_untracked())
        />
        <section class="mx-auto max-w-5xl px-4 py-8 sm:px-6">
            <div class="mb-8 flex items-end gap-4">
                <span class="vertical-jp font-pixel text-4xl leading-none text-primary">"橋をかけろ"</span>
                // Hashiwokakero, a long word, fits next to the vertical title on a phone
                <div class="min-w-0">
                    <p class="font-pixel text-sm text-primary">"遊び方 · asobikata"</p>
                    <h1 class="text-3xl font-black break-words sm:text-4xl">{t!(i18n, rules.title)}</h1>
                    <p class="mt-2 max-w-2xl text-muted-foreground">
                        {t!(i18n, rules.intro)}
                    </p>
                </div>
            </div>

            <div class="grid grid-cols-1 items-center gap-8 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
                <Rules class="flex flex-col gap-6" />
                <div class="flex flex-col items-center gap-3">
                    <BoardView
                        board=Signal::stored(board)
                        bridges=Signal::stored(bridges)
                        mode=BoardMode::View
                        max_height="22rem"
                        framed=true
                    />
                    <p class="text-sm text-muted-foreground">{t!(i18n, rules.example)}</p>
                </div>
            </div>

            <Separator class="my-10" />

            <section class="flex scroll-mt-20 flex-col gap-5" id="tips">
                <div>
                    <p class="font-pixel text-sm text-primary">"コツ · kotsu"</p>
                    <h2 class="text-2xl font-black">{t!(i18n, rules.tips.title)}</h2>
                    <p class="mt-2 max-w-2xl text-muted-foreground">{t!(i18n, rules.tips.intro)}</p>
                </div>
                <TipCards count=Tip::ALL.len() class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3" />
            </section>

            <Separator class="my-10" />

            <div class="grid grid-cols-1 gap-6 md:grid-cols-2">
                <Card>
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, rules.controls.title)} " " <span class="font-pixel text-sm text-primary">"操作"</span>
                        </CardTitle>
                    </CardHeader>
                    <CardContent class="flex flex-col gap-2 text-sm text-muted-foreground">
                        <p>{t!(i18n, rules.controls.tap)}</p>
                        <p>{t!(i18n, rules.controls.drag)}</p>
                        <p>{t!(i18n, rules.controls.remove, <right> = <Kbd />, <shift> = <Kbd />)}</p>
                        <p>
                            <Kbd>"Ctrl Z"</Kbd> " " {t!(i18n, common.game.undo)} " · " <Kbd>"Ctrl Y"</Kbd> " "
                            {t!(i18n, common.game.redo)} " · " <Kbd>"H"</Kbd> " " {t!(i18n, common.game.hint)}
                        </p>
                        <p>{t!(i18n, rules.controls.colors)}</p>
                    </CardContent>
                </Card>

                <Card>
                    <CardHeader>
                        <CardTitle class="flex items-baseline gap-2 text-xl">
                            {t!(i18n, rules.hood.title)} " " <span class="font-pixel text-sm text-primary">"仕組み"</span>
                        </CardTitle>
                    </CardHeader>
                    <CardContent class="flex flex-col gap-3 text-sm text-muted-foreground">
                        <p>
                            {t!(i18n, rules.hood.variables, <e> = <i />, <e1> = <i />, <e2> = <i />, <imp> = <i />)}
                        </p>
                        <Clause>"¬e₂ ∨ e₁"</Clause>
                        <p>{t!(i18n, rules.hood.islands)}</p>
                        <Clause>"¬h₁ ∨ ¬v₁"</Clause>
                        <p>
                            {t!(i18n, rules.hood.connectivity)}
                        </p>
                        <p>
                            {t!(
                                i18n,
                                rules.hood.generator,
                                <a> = move |children: ChildrenFn| view! {
                                    <A href=lab attr:class="text-primary underline underline-offset-4">{children()}</A>
                                }
                            )}
                        </p>
                    </CardContent>
                </Card>
            </div>

            <Separator class="my-10" />

            <section class="flex scroll-mt-20 flex-col gap-5" id="faq">
                <div>
                    <p class="font-pixel text-sm text-primary">"よくある質問 · FAQ"</p>
                    <h2 class="text-2xl font-black">{t!(i18n, rules.faq_title)}</h2>
                </div>
                <dl class="grid gap-x-8 gap-y-5 md:grid-cols-2">
                    {move || rules_faq(i18n.get_locale())
                        .into_iter()
                        .map(|(q, a)| {
                            view! {
                                <div class="flex flex-col gap-1.5">
                                    <dt class="font-bold">{q}</dt>
                                    <dd class="text-sm leading-relaxed text-muted-foreground">{a}</dd>
                                </div>
                            }
                        })
                        .collect_view()}
                </dl>
            </section>
        </section>
    }
}
