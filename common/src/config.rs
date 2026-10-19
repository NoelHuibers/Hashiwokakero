//! Solver and generator options plus their results, as exchanged with the server.

use serde::{Deserialize, Serialize};

use crate::Puzzle;

/// Which SAT solver to run on the CNF encoding of a puzzle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SolverKind {
    /// The external splr crate.
    Splr,
    /// Our own CDCL solver from the `sat_solver` crate.
    #[default]
    Cdcl,
    /// Our own DPLL solver from the `sat_solver` crate.
    Dpll,
}

impl SolverKind {
    pub const ALL: [Self; 3] = [Self::Cdcl, Self::Dpll, Self::Splr];

    pub fn id(self) -> &'static str {
        match self {
            Self::Splr => "splr",
            Self::Cdcl => "cdcl",
            Self::Dpll => "dpll",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Splr => "splr",
            Self::Cdcl => "Own CDCL",
            Self::Dpll => "Own DPLL",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.id().eq_ignore_ascii_case(id))
    }

    /// Branching heuristics the solver implements.
    pub fn heuristics(self) -> &'static [Heuristic] {
        match self {
            Self::Splr => &[],
            Self::Cdcl => &[Heuristic::Vsids, Heuristic::Arbitrary],
            Self::Dpll => &Heuristic::ALL,
        }
    }

    pub fn supports_restarts(self) -> bool {
        self == Self::Cdcl
    }

    pub fn supports_subsumption(self) -> bool {
        self == Self::Cdcl
    }
}

/// Branching heuristics of our own solvers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Heuristic {
    Arbitrary,
    Dlis,
    Dlcs,
    Mom,
    Boehm,
    JeroslawWang,
    #[default]
    Vsids,
    Custom,
}

impl Heuristic {
    pub const ALL: [Self; 8] = [
        Self::Vsids,
        Self::Arbitrary,
        Self::Dlis,
        Self::Dlcs,
        Self::Mom,
        Self::Boehm,
        Self::JeroslawWang,
        Self::Custom,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Arbitrary => "arbitrary",
            Self::Dlis => "dlis",
            Self::Dlcs => "dlcs",
            Self::Mom => "mom",
            Self::Boehm => "boehm",
            Self::JeroslawWang => "jeroslaw_wang",
            Self::Vsids => "vsids",
            Self::Custom => "custom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Arbitrary => "Arbitrary",
            Self::Dlis => "DLIS",
            Self::Dlcs => "DLCS",
            Self::Mom => "MOM",
            Self::Boehm => "Böhm",
            Self::JeroslawWang => "Jeroslow-Wang",
            Self::Vsids => "VSIDS",
            Self::Custom => "Custom",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|h| h.id().eq_ignore_ascii_case(id))
    }
}

/// Restart policy of the CDCL solver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Restarts {
    #[default]
    None,
    /// Restart every `threshold` conflicts.
    Fixed { threshold: u32 },
    /// Restart after `threshold` conflicts, growing the threshold by `factor_percent` each time.
    Geometric { threshold: u32, factor_percent: u32 },
    /// Restart after `unit * luby(i)` conflicts.
    Luby { unit: u32 },
}

impl Restarts {
    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fixed { .. } => "fixed",
            Self::Geometric { .. } => "geometric",
            Self::Luby { .. } => "luby",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No restarts",
            Self::Fixed { .. } => "Fixed interval",
            Self::Geometric { .. } => "Geometric",
            Self::Luby { .. } => "Luby sequence",
        }
    }

    /// The policy with sensible default parameters.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "none" => Some(Self::None),
            "fixed" => Some(Self::Fixed { threshold: 100 }),
            "geometric" => Some(Self::Geometric {
                threshold: 100,
                factor_percent: 150,
            }),
            "luby" => Some(Self::Luby { unit: 32 }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SolverConfig {
    pub kind: SolverKind,
    /// Ignored by splr and replaced by the default if the solver doesn't support it.
    pub heuristic: Heuristic,
    /// Only used by the CDCL solver.
    pub restarts: Restarts,
    /// Eliminate subsumed clauses before solving (CDCL only).
    pub subsumption: bool,
    /// Give up after this many milliseconds.
    pub timeout_ms: u32,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            kind: SolverKind::Cdcl,
            heuristic: Heuristic::Vsids,
            restarts: Restarts::Luby { unit: 32 },
            subsumption: false,
            timeout_ms: 10_000,
        }
    }
}

impl SolverConfig {
    pub fn splr() -> Self {
        Self {
            kind: SolverKind::Splr,
            ..Default::default()
        }
    }

    /// The configuration with options the chosen solver doesn't support reset to defaults.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let heuristics = self.kind.heuristics();
        if !heuristics.is_empty() && !heuristics.contains(&self.heuristic) {
            self.heuristic = heuristics[0];
        }
        if !self.kind.supports_restarts() {
            self.restarts = Restarts::None;
        }
        if !self.kind.supports_subsumption() {
            self.subsumption = false;
        }
        self
    }

    /// Short human readable description, e.g. "Own CDCL · VSIDS · Luby sequence".
    pub fn describe(&self) -> String {
        let mut parts = vec![self.kind.label().to_string()];
        if !self.kind.heuristics().is_empty() {
            parts.push(self.heuristic.label().to_string());
        }
        if self.kind.supports_restarts() {
            parts.push(self.restarts.label().to_string());
        }
        if self.subsumption {
            parts.push("subsumption".to_string());
        }
        parts.join(" · ")
    }
}

/// Numbers about one solver run, shown in the solver panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SolveStats {
    /// Normalized, see [`SolverConfig::normalized`].
    pub config: SolverConfig,
    pub variables: usize,
    pub clauses: usize,
    pub encode_ms: f64,
    pub solve_ms: f64,
    /// Branching decisions.
    pub decisions: Option<u64>,
    pub restarts: Option<u64>,
    /// How often the SAT solver was called. Disconnected solutions are refuted with an
    /// additional clause and solved again (lazy connectivity).
    pub sat_calls: u32,
    /// Clauses added to cut off disconnected solutions.
    pub connectivity_cuts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolveOutcome {
    /// Bridge counts per [`crate::Board::edges`] index.
    Solved {
        bridges: Vec<u8>,
        unique: Option<bool>,
    },
    Unsatisfiable,
    Timeout,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolveReport {
    pub outcome: SolveOutcome,
    pub stats: SolveStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    #[default]
    Medium,
    Hard,
    Expert,
    Oni,
}

impl Difficulty {
    pub const ALL: [Self; 5] = [
        Self::Easy,
        Self::Medium,
        Self::Hard,
        Self::Expert,
        Self::Oni,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Easy => "easy",
            Self::Medium => "medium",
            Self::Hard => "hard",
            Self::Expert => "expert",
            Self::Oni => "oni",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
            Self::Expert => "Expert",
            Self::Oni => "Oni",
        }
    }

    /// Japanese name shown next to the label.
    pub fn kanji(self) -> &'static str {
        match self {
            Self::Easy => "初級",
            Self::Medium => "中級",
            Self::Hard => "上級",
            Self::Expert => "達人",
            Self::Oni => "鬼",
        }
    }

    /// Default grid size (width, height).
    pub fn size(self) -> (u8, u8) {
        match self {
            Self::Easy => (7, 7),
            Self::Medium => (10, 10),
            Self::Hard => (14, 14),
            Self::Expert => (18, 18),
            Self::Oni => (24, 24),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.id() == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GenerateParams {
    pub width: u8,
    pub height: u8,
    pub difficulty: Difficulty,
    /// Same seed and parameters give the same puzzle.
    pub seed: u64,
}

/// Grid sizes accepted by the generator and the editor.
pub const MIN_SIZE: u8 = 3;
pub const MAX_SIZE: u8 = 30;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratedPuzzle {
    pub puzzle: Puzzle,
    /// The unique solution, bridge counts per [`crate::Board::edges`] index.
    pub solution: Vec<u8>,
    pub params: GenerateParams,
    /// Whether the solution was proven unique by the SAT solver.
    pub unique: bool,
    /// Candidates thrown away or repaired before this one was accepted.
    pub attempts: u32,
    pub generate_ms: f64,
}
