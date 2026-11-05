//! The Lab, for solver writers: race our SAT solvers on a puzzle, and the SAT Dojo.
//!
//! The Dojo (`pages::dojo`) scores people's own solvers. One section with a tab per page
//! (`section`); the Dojo's old addresses redirect to its tabs (`seo::MOVED_PAGES`).

mod presets;
pub mod race;
pub mod results;
pub mod section;

use leptos::prelude::*;

use crate::pages::dojo::{BenchmarksTab, LeaderboardTab, SeasonsTab, SolverDetail, SubmitTab};

use race::Race;
pub use race::RunState;
pub use section::{LabSection, LabTab};

/// `/lab`: the race.
#[component]
pub fn LabPage() -> impl IntoView {
    view! {
        <LabSection tab=LabTab::Race>
            <Race />
        </LabSection>
    }
}

/// `/lab/leaderboard`: the Dojo's leaderboard (the Dojo's page, `/dojo`, before).
#[component]
pub fn LabLeaderboardPage() -> impl IntoView {
    view! {
        <LabSection tab=LabTab::Leaderboard>
            <LeaderboardTab />
        </LabSection>
    }
}

/// `/lab/submit`: how to submit a solver to the Dojo, and the form.
#[component]
pub fn LabSubmitPage() -> impl IntoView {
    view! {
        <LabSection tab=LabTab::Submit>
            <SubmitTab />
        </LabSection>
    }
}

/// `/lab/benchmarks`: the community benchmarks.
#[component]
pub fn LabBenchmarksPage() -> impl IntoView {
    view! {
        <LabSection tab=LabTab::Benchmarks>
            <BenchmarksTab />
        </LabSection>
    }
}

/// `/lab/seasons`: the season archive.
#[component]
pub fn LabSeasonsPage() -> impl IntoView {
    view! {
        <LabSection tab=LabTab::Seasons>
            <SeasonsTab />
        </LabSection>
    }
}

/// `/lab/solver/<id>`: a submitted solver, under the leaderboard's tab, with a title of its own
/// and not indexed.
#[component]
pub fn LabSolverPage() -> impl IntoView {
    view! {
        <LabSection tab=LabTab::Leaderboard meta=false>
            <SolverDetail />
        </LabSection>
    }
}
