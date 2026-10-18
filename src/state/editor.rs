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
