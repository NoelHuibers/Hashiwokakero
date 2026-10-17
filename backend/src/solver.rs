//! Solving DIMACS CNF files (used by the CLI and the tests). The web server uses [`crate::api`].

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::time::{Duration, Instant};

use common::SolverConfig;
pub use common::SolverKind;
use splr::Certificate;

use crate::sat::{self, SatOutcome};

pub fn solve(filepath: &str) -> io::Result<Certificate> {
    solve_with(filepath, SolverKind::Splr)
}

/// Solves a CNF file with the default configuration of the given solver, without time limit.
pub fn solve_with(filepath: &str, kind: SolverKind) -> io::Result<Certificate> {
    let config = SolverConfig {
        kind,
        timeout_ms: 0,
        ..Default::default()
    };
    solve_file(filepath, &config)
}

/// Solves a CNF file. `timeout_ms == 0` means no time limit, a timeout is reported as
/// `io::ErrorKind::TimedOut`.
pub fn solve_file(filepath: &str, config: &SolverConfig) -> io::Result<Certificate> {
    let text = fs::read_to_string(filepath)?;
    let (clauses, num_vars) =
        parse_dimacs(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let deadline = (config.timeout_ms > 0)
        .then(|| Instant::now() + Duration::from_millis(config.timeout_ms as u64));
    let (outcome, _) =
        sat::solve_clauses(&clauses, num_vars, config, deadline).map_err(io::Error::other)?;
    match outcome {
        SatOutcome::Sat(model) => Ok(Certificate::SAT(
            (1..model.len())
                .map(|v| if model[v] { v as i32 } else { -(v as i32) })
                .collect(),
        )),
        SatOutcome::Unsat => Ok(Certificate::UNSAT),
        SatOutcome::Timeout => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{} timed out", config.kind.label()),
        )),
    }
}

/// Parses DIMACS CNF: comment lines (`c ...`), the problem line (`p cnf VARS CLAUSES`) and
/// clauses terminated by `0` (possibly spanning several lines).
pub fn parse_dimacs(text: &str) -> Result<(Vec<Vec<i32>>, usize), String> {
    let mut num_vars = None;
    let mut clauses = Vec::new();
    let mut clause = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('c') || line.starts_with('%') {
            continue;
        }
        if line.starts_with('p') {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() != 4 || parts[1] != "cnf" {
                return Err(format!("Invalid problem line '{line}'"));
            }
            num_vars = Some(
                parts[2]
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid problem line '{line}'"))?,
            );
            continue;
        }
        if num_vars.is_none() {
            return Err("Missing 'p cnf' line".into());
        }
        for lit in line.split_whitespace() {
            let lit = lit
                .parse::<i32>()
                .map_err(|_| format!("Invalid literal '{lit}'"))?;
            if lit == 0 {
                clauses.push(std::mem::take(&mut clause));
            } else {
                clause.push(lit);
            }
        }
    }
    if !clause.is_empty() {
        clauses.push(clause);
    }
    let num_vars = num_vars.ok_or("Missing 'p cnf' line")?;
    Ok((clauses, num_vars))
}

pub fn write_solution(certificate: Certificate, output_file: &str) -> io::Result<()> {
    if !output_file.ends_with(".txt") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only .txt files are allowed as output from a solver.",
        ));
    }
    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(get_content(certificate).as_bytes())
}

pub fn get_content(certificate: Certificate) -> String {
    match certificate {
        Certificate::SAT(model) => {
            let mut res = String::from("SAT\n");
            for literal in model {
                res.push_str(&format!("{literal} "));
            }
            res
        }
        Certificate::UNSAT => String::from("UNSAT"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dimacs() {
        let (clauses, vars) = parse_dimacs("c x\np cnf 3 2\n1 -2\n 0 3 0\n").unwrap();
        assert_eq!(vars, 3);
        assert_eq!(clauses, vec![vec![1, -2], vec![3]]);
        assert!(parse_dimacs("1 2 0\n").is_err());
        assert!(parse_dimacs("p cnf x 2\n").is_err());
    }
}
