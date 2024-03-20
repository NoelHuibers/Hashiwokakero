use crate::cdcl::CDCL;
use crate::cli::Heuristic;

pub fn luby_sequence(mut u: u32, mut v: u32) -> (u32, u32) {
    if (u & u.wrapping_neg()) == v {
        u += 1;
        v = 1;
    } else {
        v <<= 1;
    }
    (u, v)
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
        10,
        10,
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
        10,
        10,
        None,
    );

    solver.solve().unwrap();

    assert_eq!(solver.restart.count, 0, "Solver should not have restarted");
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
        10,
        10,
        None,
    );

    solver.solve().unwrap();

    assert!(
        solver.restart.count > 0,
        "Solver should have restarted at least once"
    );
}

#[test]
fn test_solver_uses_luby_sequence_for_restarts() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let base_conflict_threshold = Some(10);
    let mut solver = CDCL::new(
        input,
        v_c,
        c_c,
        Heuristic::Arbitrary,
        false,
        base_conflict_threshold,
        true,
        None,
        10,
        10,
        None,
    );

    solver.solve().unwrap();

    assert!(
        solver.restart.count > 0,
        "Solver should have restarted at least once"
    );
}

#[test]
fn test_luby_sequence() {
    let expected = vec![
        (1, 1),
        (2, 1),
        (2, 2),
        (3, 1),
        (4, 1),
        (4, 2),
        (4, 4),
        (5, 1),
    ];
    let mut actual = vec![(1, 1)];
    for i in 0..7 {
        let first = actual.last().unwrap();
        let res = luby_sequence(first.0, first.1);
        actual.push(res);
        assert_eq!(res, expected[i + 1])
    }
}
