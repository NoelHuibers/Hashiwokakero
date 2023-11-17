use backend::solver::solve;
use std::{io::Write, vec};
use tempfile;

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

    assert!(match output {
        Ok(cert) => match cert {
            splr::Certificate::SAT(_) => true,
            _ => false,
        },
        Err(_) => false,
    });
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
        Ok(cert) => match cert {
            splr::Certificate::UNSAT => true,
            _ => false,
        },
        Err(err) => {
            println!("Error: {:?}", err);
            false
        }
    });
}
