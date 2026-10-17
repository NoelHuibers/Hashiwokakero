//! The house entries of the SAT Dojo: the solvers behind the website, reading DIMACS on stdin
//! and answering in the SAT competition format.

use std::io::{Read, Write};

use backend::sat::{SatOutcome, solve_clauses};
use common::SolverConfig;

/// Clauses and the number of variables of a DIMACS formula.
pub fn parse(text: &str) -> (Vec<Vec<i32>>, usize) {
    let mut vars = 0;
    let mut clauses = Vec::new();
    let mut clause = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('c') || line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix("p cnf") {
            vars = header.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
            continue;
        }
        for lit in line.split_whitespace().filter_map(|t| t.parse::<i32>().ok()) {
            if lit == 0 {
                clauses.push(std::mem::take(&mut clause));
            } else {
                clause.push(lit);
            }
        }
    }
    (clauses, vars)
}

pub fn run(config: SolverConfig) {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).expect("could not read stdin");
    let (clauses, vars) = parse(&text);
    let mut out = std::io::stdout().lock();
    let _ = match solve_clauses(&clauses, vars, &config, None) {
        Ok((SatOutcome::Sat(model), _)) => {
            let mut line = String::from("s SATISFIABLE\nv");
            for (v, &value) in model.iter().enumerate().skip(1) {
                line.push_str(if value { " " } else { " -" });
                line.push_str(&v.to_string());
            }
            writeln!(out, "{line} 0")
        }
        Ok((SatOutcome::Unsat, _)) => writeln!(out, "s UNSATISFIABLE"),
        Ok((SatOutcome::Timeout, _)) => writeln!(out, "s UNKNOWN"),
        Err(e) => writeln!(out, "c {e}\ns UNKNOWN"),
    };
}
