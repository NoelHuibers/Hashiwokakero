//! Quick sanity checks on a puzzle being built, and the alert that lists what they found.

use common::{Board, Puzzle};
use leptos::prelude::*;

use crate::components::ui::alert::{Alert, AlertDescription, AlertTitle};
use crate::i18n::{Locale, t, td_string, use_i18n};

/// Quick sanity checks that don't need a SAT solver.
pub(super) fn warnings(locale: Locale, puzzle: &Puzzle) -> Vec<String> {
    let mut out = Vec::new();
    if puzzle.islands.is_empty() {
        return out;
    }
    let board = Board::new(puzzle.clone());
    for (i, island) in puzzle.islands.iter().enumerate() {
        let max = board.incident[i].len() * 2;
        if max < usize::from(island.required) {
            out.push(
                td_string!(
                    locale,
                    create.warning_capacity,
                    number = island.required,
                    x = island.pos.x + 1,
                    y = island.pos.y + 1,
                    max
                )
                .to_string(),
            );
        }
    }
    let total: u32 = puzzle.islands.iter().map(|i| u32::from(i.required)).sum();
    if total % 2 == 1 {
        out.push(td_string!(locale, create.warning_odd, total));
    }
    out.truncate(4);
    out
}

#[component]
pub(super) fn Warnings(issues: Memo<Vec<String>>) -> impl IntoView {
    let i18n = use_i18n();
    view! {
        {move || {
            let list = issues.get();
            (!list.is_empty())
                .then(|| {
                    view! {
                        <Alert class="border-warning/50 bg-warning-light text-warning-dark">
                            <AlertTitle>"注意 · " {t!(i18n, create.heads_up)}</AlertTitle>
                            <AlertDescription>
                                <ul class="list-disc ps-5">
                                    {list.into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}
                                </ul>
                            </AlertDescription>
                        </Alert>
                    }
                })
        }}
    }
}
