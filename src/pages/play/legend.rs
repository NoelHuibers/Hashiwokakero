//! The legend next to the practice board: the two kinds of bridge and the controls.

use common::{Board, Pos, Puzzle};
use leptos::prelude::*;

use crate::components::board::{BoardMode, BoardView};
use crate::components::ui::card::{Card, CardContent, CardHeader, CardTitle};
use crate::components::ui::kbd::Kbd;
use crate::i18n::{t, use_i18n};

/// A tiny board showing one kind of bridge, for the legend.
fn legend_board(required: u8, count: u8) -> (Board, Vec<u8>) {
    let mut puzzle = Puzzle::empty(4, 1);
    puzzle.set(Pos::new(0, 0), required);
    puzzle.set(Pos::new(3, 0), required);
    let board = Board::new(puzzle);
    (board, vec![count])
}

#[component]
pub(super) fn LegendCard() -> impl IntoView {
    let i18n = use_i18n();
    let (single, single_bridges) = legend_board(1, 1);
    let (double, double_bridges) = legend_board(2, 2);
    view! {
        <Card class="gap-3">
            <CardHeader>
                <CardTitle class="flex items-baseline gap-2 text-base">
                    {t!(i18n, practice.legend.title)} <span class="font-pixel text-sm text-primary">"操作"</span>
                </CardTitle>
            </CardHeader>
            <CardContent class="flex flex-col gap-3 text-sm">
                <div class="grid grid-cols-2 gap-3">
                    <div class="flex flex-col items-center gap-1">
                        <BoardView
                            board=Signal::stored(single)
                            bridges=Signal::stored(single_bridges)
                            mode=BoardMode::View
                            max_height="2.6rem"
                        />
                        <span class="text-xs text-muted-foreground">"1 · " {t!(i18n, practice.legend.single)}</span>
                    </div>
                    <div class="flex flex-col items-center gap-1">
                        <BoardView
                            board=Signal::stored(double)
                            bridges=Signal::stored(double_bridges)
                            mode=BoardMode::View
                            max_height="2.6rem"
                        />
                        <span class="text-xs text-muted-foreground">"2 · " {t!(i18n, practice.legend.double)}</span>
                    </div>
                </div>
                <ul class="flex flex-col gap-1.5 text-muted-foreground">
                    <li>{t!(i18n, practice.legend.build)}</li>
                    <li>{t!(i18n, practice.legend.again)}</li>
                    <li>{t!(i18n, practice.legend.remove, <kbd> = <Kbd />)}</li>
                    <li>
                        <Kbd>"Ctrl Z"</Kbd> " " {t!(i18n, common.game.undo)} " · " <Kbd>"Ctrl Y"</Kbd> " "
                        {t!(i18n, common.game.redo)} " · " <Kbd>"H"</Kbd> " " {t!(i18n, common.game.hint)}
                    </li>
                </ul>
            </CardContent>
        </Card>
    }
}
