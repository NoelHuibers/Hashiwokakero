use backend::solver::{solve};

#[test]
fn test_sat_problem() {
    // Example of a satisfiable SAT problem
    let clauses = vec![
        vec![1, -2],
        vec![-1, 2],
        vec![1, 2]
    ];

    let output = solve(clauses);

    assert!(output.contains("SATISFIABLE"));
}

#[test]
fn test_unsat_problem() {
    // Example of an unsatisfiable SAT problem
    let clauses = vec![
        vec![1],
        vec![-1]
    ];

    let output = solve(clauses);

    assert!(output.contains("UNSATISFIABLE"));
}