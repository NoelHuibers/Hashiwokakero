use crate::cdcl::CDCL;
use crate::cli::Heuristic;

pub fn luby_sequence(step: usize) -> usize {
    let mut k: usize = 1;
    while (1 << (k + 1)) <= step + 1 {
        k += 1;
    }
    if (1 << k) == step + 1 {
        return 1 << (k - 1);
    }
    luby_sequence(step - (1 << (k - 1)) + 1)
}

#[test]
fn test_restart_resets_solver_state() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let mut solver = CDCL::new(
        input,
        v_c,
        c_c,
        Heuristic::Arbitrary,
        false,
        None,
        false,
        None,
    );

    solver
        .solve()
        .unwrap_or_else(|_| panic!("Initial solving failed"));

    solver.restart();
    assert_eq!(solver.branch_depth, 0, "Branch depth was not reset");
    assert!(solver.unit_queue.is_empty(), "Unit queue was not cleared");
}

#[test]
fn test_solve_without_restart_threshold() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let mut solver = CDCL::new(
        input,
        v_c,
        c_c,
        Heuristic::Arbitrary,
        false,
        None,
        false,
        None,
    );

    solver.solve().unwrap();

    assert_eq!(solver.restart_count, 0, "Solver should not have restarted");
}

#[test]
fn test_solve_with_restart_threshold() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let mut solver = CDCL::new(
        input,
        v_c,
        c_c,
        Heuristic::Arbitrary,
        false,
        Some(1),
        false,
        None,
    );

    solver.solve().unwrap();

    assert!(
        solver.restart_count > 0,
        "Solver should have restarted at least once"
    );
}

#[test]
fn test_solver_uses_luby_sequence_for_restarts() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let base_conflict_threshold = Some(1);
    let mut solver = CDCL::new(
        input,
        v_c,
        c_c,
        Heuristic::Arbitrary,
        false,
        base_conflict_threshold,
        true,
        None,
    );

    solver.solve().unwrap();

    assert!(
        solver.restart_count > 0,
        "Solver should have restarted at least once"
    );
}
