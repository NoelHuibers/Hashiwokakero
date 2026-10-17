use backend::generate_clauses::generate;
use backend::parse_input::parse_input;
use backend::solver::{SolverKind, solve, solve_with};
use std::{io::Write, vec};

#[test]
fn test_sat_problem() {
    // Example of a satisfiable SAT problem
    let clauses = vec![vec![1, -2], vec![-1, 2], vec![1, 2]];
    let mut temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");

    writeln!(&mut temp_file, "c This is a test SAT problem").expect("Failed to write to temp file");
    writeln!(&mut temp_file, "p cnf 2 3").expect("Failed to write to temp file");

    for clause in &clauses {
        writeln!(
            &mut temp_file,
            "{} 0",
            clause
                .iter()
                .map(|&lit| lit.to_string())
                .collect::<Vec<String>>()
                .join(" ")
        )
        .expect("Failed to write to temp file");
    }

    temp_file
        .as_file()
        .sync_all()
        .expect("Failed to sync temp file");
    let output = solve(
        temp_file
            .path()
            .to_str()
            .expect("Failed to convert path to str"),
    );

    assert!(matches!(output, Ok(splr::Certificate::SAT(_))));
}

#[test]
fn test_unsat_problem() {
    // Example of an unsatisfiable SAT problem
    let clauses = vec![vec![1, 2], vec![-1], vec![-2]];
    let mut temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");

    writeln!(&mut temp_file, "c This is a test SAT problem").expect("Failed to write to temp file");
    writeln!(&mut temp_file, "p cnf 2 3").expect("Failed to write to temp file");

    for clause in &clauses {
        writeln!(
            &mut temp_file,
            "{} 0",
            clause
                .iter()
                .map(|&lit| lit.to_string())
                .collect::<Vec<String>>()
                .join(" ")
        )
        .expect("Failed to write to temp file");
    }

    temp_file
        .as_file()
        .sync_all()
        .expect("Failed to sync temp file");
    let output = solve(
        temp_file
            .path()
            .to_str()
            .expect("Failed to convert path to str"),
    );

    assert!(match output {
        Ok(cert) => matches!(cert, splr::Certificate::UNSAT),
        Err(err) => {
            println!("Error: {err:?}");
            false
        }
    });
}

fn write_cnf(clauses: &[Vec<i32>], variables: usize) -> tempfile::NamedTempFile {
    let temp_file = tempfile::Builder::new()
        .suffix(".cnf")
        .tempfile()
        .expect("Failed to create temp file");
    backend::writer::generate_dimacs(
        &clauses.to_vec(),
        variables,
        temp_file.path().to_str().unwrap(),
    )
    .expect("Failed to write CNF");
    temp_file
}

fn satisfies(model: &[i32], clauses: &[Vec<i32>]) -> bool {
    clauses
        .iter()
        .all(|clause| clause.iter().any(|lit| model.contains(lit)))
}

#[test]
fn test_cdcl_small_problems() {
    let sat = write_cnf(&[vec![1, -2], vec![-1, 2], vec![1, 2]], 2);
    let unsat = write_cnf(&[vec![1, 2], vec![-1], vec![-2]], 2);
    match solve_with(sat.path().to_str().unwrap(), SolverKind::Cdcl) {
        Ok(splr::Certificate::SAT(model)) => {
            assert!(satisfies(&model, &[vec![1, -2], vec![-1, 2], vec![1, 2]]));
        }
        other => panic!("Expected SAT, got {other:?}"),
    }
    assert!(matches!(
        solve_with(unsat.path().to_str().unwrap(), SolverKind::Cdcl),
        Ok(splr::Certificate::UNSAT)
    ));
}

// CDCL has to agree with splr on all example puzzles and return a valid model
#[test]
fn test_cdcl_on_puzzles() {
    let mut inputs: Vec<_> = std::fs::read_dir("./input")
        .expect("Failed to read input dir")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "txt"))
        .collect();
    inputs.sort();

    for input in inputs {
        // Some inputs are intentionally invalid
        let Ok(game_board) = parse_input(input.to_str().unwrap()) else {
            continue;
        };
        let (clauses, var_map) = generate(&game_board);
        let cnf = write_cnf(&clauses, var_map.keys().len());
        let cnf_path = cnf.path().to_str().unwrap();

        let expected_sat = matches!(
            solve_with(cnf_path, SolverKind::Splr),
            Ok(splr::Certificate::SAT(_))
        );
        match solve_with(cnf_path, SolverKind::Cdcl) {
            Ok(splr::Certificate::SAT(model)) => {
                assert!(expected_sat, "{input:?} is UNSAT but got SAT");
                assert!(satisfies(&model, &clauses), "Invalid model for {input:?}");
            }
            Ok(splr::Certificate::UNSAT) => {
                assert!(!expected_sat, "{input:?} is SAT but got UNSAT");
            }
            Err(err) => panic!("{input:?} failed: {err}"),
        }
    }
}

#[test]
fn test_solver_kind_from_id() {
    for kind in SolverKind::ALL {
        assert_eq!(SolverKind::from_id(kind.id()), Some(kind));
    }
    assert_eq!(SolverKind::from_id("CDCL"), Some(SolverKind::Cdcl));
    assert_eq!(SolverKind::from_id("minisat"), None);
}
