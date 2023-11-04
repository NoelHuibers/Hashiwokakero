use minisat::Solver;

fn solve(clauses: Vec<Vec<i32>>) {
    let mut solver = Solver::new();
    for clause in &clauses {
        solver.add_clause(clause);
    }
    let result = solver.solve();

    match result {
        Ok(true) => {
            let model = solver.get_model();
            println!("Solution: {:?}", model);
        }
        Ok(false) => {
            println!("No solution found. The puzzle is unsolvable.");
        }
        Err(err) => {
            eprintln!("An error occurred: {:?}", err);
        }
    }
}
