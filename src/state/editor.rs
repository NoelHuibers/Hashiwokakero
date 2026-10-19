//! The puzzle editor.

use common::{Puzzle, SolveReport};
use leptos::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Editor {
    pub puzzle: RwSignal<Puzzle>,
    /// Bridges of the last solver run, shown on top of the edited puzzle.
    pub solution: RwSignal<Option<Vec<u8>>>,
    pub report: RwSignal<Option<SolveReport>>,
    pub solving: RwSignal<bool>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            puzzle: RwSignal::new(Puzzle::empty(8, 8)),
            solution: RwSignal::new(None),
            report: RwSignal::new(None),
            solving: RwSignal::new(false),
        }
    }

    pub fn edit(&self, f: impl FnOnce(&mut Puzzle)) {
        self.puzzle.update(f);
        self.solution.set(None);
        self.report.set(None);
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Settings;

    /// A solution shown on top of the puzzle belongs to it: any edit takes it away.
    #[test]
    fn edits_drop_the_solution() {
        Owner::new().with(|| {
            let editor = Editor::default();
            editor.solution.set(Some(vec![1]));
            editor.edit(|p| p.set(common::Pos::new(1, 1), 3));
            assert_eq!(editor.solution.get_untracked(), None);
            assert_eq!(editor.report.get_untracked(), None);
            assert_eq!(editor.puzzle.with_untracked(|p| p.islands.len()), 1);
            assert!(Settings::default().sound.get_untracked());
        });
    }
}
