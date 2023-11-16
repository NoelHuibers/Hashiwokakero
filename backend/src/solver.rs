use splr::*;
use std::fs;
use std::io;

pub fn solve(clauses: Vec<Vec<i32>>) -> String {
    let result = match Certificate::try_from(clauses) {
        Ok(Certificate::SAT(ans)) => format!("SATISFIABLE: {:?}", ans),
        Ok(Certificate::UNSAT) => "UNSATISFIABLE".to_string(),
        Err(e) => panic!("UNKNOWN; {}", e),
    };

    println!("{}", result);

    result
}

pub fn parse(input_file: &str) -> io::Result<Vec<Vec<i32>>> {
    if !input_file.ends_with(".cnf") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only .cnf files are allowed.",
        ));
    }

    let input = fs::read_to_string(input_file)?;
    let lines: Vec<&str> = input.lines().collect();
    let mut clauses = Vec::new();

    for line in lines {
        {
            if line.starts_with('c') || line.starts_with('p') {
                continue;
            }

            let clause = line
                .split_whitespace()
                .filter_map(|lit| {
                    let num = lit.parse::<i32>().ok()?;
                    if num == 0 {
                        None
                    } else {
                        Some(num)
                    }
                })
                .collect::<Vec<i32>>();
            clauses.push(clause);
        }
    }
    Ok(clauses)
}
