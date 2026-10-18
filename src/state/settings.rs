//! Settings: solver configuration, difficulty, theme, sound and music.

use common::{Difficulty, SolverConfig};
use leptos::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub solver: RwSignal<SolverConfig>,
    pub difficulty: RwSignal<Difficulty>,
    pub dark: RwSignal<bool>,
    /// Sound effects, on unless turned off.
    pub sound: RwSignal<bool>,
    /// Background music, off unless turned on.
    pub music: RwSignal<bool>,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            solver: RwSignal::new(SolverConfig::default()),
            difficulty: RwSignal::new(Difficulty::Medium),
            dark: RwSignal::new(false),
            sound: RwSignal::new(true),
            music: RwSignal::new(false),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
