//! Our DPLL solver with its default branching heuristic.
fn main() {
    house::run(
        common::SolverConfig {
            kind: common::SolverKind::Dpll,
            ..Default::default()
        }
        .normalized(),
    );
}
