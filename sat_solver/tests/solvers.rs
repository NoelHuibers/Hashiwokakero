//! Corners of the solvers' API the backend doesn't reach: empty clauses and deadlines set on
//! an existing solver.

use std::time::Instant;

use sat_solver::cdcl::CDCL;
use sat_solver::cli::Heuristic;
use sat_solver::dpll::{DIMACSOutput, DPLL};

fn cdcl(clauses: &[Vec<i32>]) -> CDCL {
    CDCL::new(
        clauses.to_vec(),
        2,
        clauses.len(),
        Heuristic::VSIDS,
        false,
        None,
        false,
        None,
        10,
        10,
        None,
    )
}

fn dpll(clauses: &[Vec<i32>]) -> DPLL {
    DPLL::new(clauses.to_vec(), 2, clauses.len(), Heuristic::DLIS, false)
}

#[test]
fn an_empty_clause_is_unsatisfiable() {
    let clauses = [vec![1, 2], vec![], vec![-1]];
    assert!(matches!(cdcl(&clauses).solve(), Ok(DIMACSOutput::Unsat)));
    assert!(matches!(dpll(&clauses).solve(), Ok(DIMACSOutput::Unsat)));
}

#[test]
fn a_solver_that_timed_out_can_go_on_without_deadline() {
    let clauses = [vec![1, 2], vec![-1, 2], vec![1, -2]];
    let mut cdcl = cdcl(&clauses);
    cdcl.set_deadline(Some(Instant::now()));
    assert!(matches!(cdcl.solve(), Ok(DIMACSOutput::Timeout)));
    cdcl.set_deadline(None);
    assert!(matches!(cdcl.solve(), Ok(DIMACSOutput::Sat(m)) if m == [1, 2, 0]));

    let mut dpll = dpll(&clauses);
    dpll.set_deadline(Some(Instant::now()));
    assert!(matches!(dpll.solve(), Ok(DIMACSOutput::Timeout)));
    dpll.set_deadline(None);
    assert!(matches!(dpll.solve(), Ok(DIMACSOutput::Sat(m)) if m == [1, 2, 0]));
}
