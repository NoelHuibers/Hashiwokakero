use sat_solver::{cdcl::CDCL, cli::Heuristic, dpll::DIMACSOutput};
use splr::*;
use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::str::FromStr;

/// The SAT solvers available for solving the generated CNF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SolverKind {
    /// The external splr crate.
    #[default]
    Splr,
    /// Our own CDCL solver from the sat_solver crate (VSIDS, no restarts).
    Cdcl,
}

impl SolverKind {
    pub const ALL: [SolverKind; 2] = [SolverKind::Splr, SolverKind::Cdcl];
}

impl FromStr for SolverKind {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "splr" => Ok(SolverKind::Splr),
            "cdcl" => Ok(SolverKind::Cdcl),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Unknown solver '{}'. Use 'splr' or 'cdcl'.", other),
            )),
        }
    }
}

impl fmt::Display for SolverKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SolverKind::Splr => write!(f, "splr"),
            SolverKind::Cdcl => write!(f, "cdcl"),
        }
    }
}

pub fn solve(filepath: &str) -> io::Result<Certificate> {
    solve_with(filepath, SolverKind::Splr)
}

pub fn solve_with(filepath: &str, kind: SolverKind) -> io::Result<Certificate> {
    match kind {
        SolverKind::Splr => solve_splr(filepath),
        SolverKind::Cdcl => solve_cdcl(filepath),
    }
}

fn solve_splr(filepath: &str) -> io::Result<Certificate> {
    let config = Config::from(filepath);
    match Solver::build(&config) {
        Ok(mut s) => match s.solve() {
            Ok(ans) => Ok(ans),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidInput, e.to_string())),
        },
        Err(e) => Err(io::Error::new(io::ErrorKind::InvalidInput, e.to_string())),
    }
}

fn solve_cdcl(filepath: &str) -> io::Result<Certificate> {
    let (clauses, var_count, clause_count) = sat_solver::parse::parse(filepath)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("{:?}", e)))?;
    // The CDCL solver expects at least one clause and no empty clauses
    if clauses.is_empty() {
        return Ok(Certificate::SAT(vec![]));
    }
    if clauses.iter().any(|c| c.is_empty()) {
        return Ok(Certificate::UNSAT);
    }

    let result = CDCL::new(
        clauses,
        var_count,
        clause_count,
        Heuristic::VSIDS,
        false,
        None,
        false,
        None,
        10,
        10,
        None,
    )
    .solve();

    match result {
        // The model may be terminated by a 0, which is not a literal
        Ok(DIMACSOutput::Sat(model)) => Ok(Certificate::SAT(
            model.into_iter().filter(|&l| l != 0).collect(),
        )),
        Ok(DIMACSOutput::Unsat) => Ok(Certificate::UNSAT),
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
    }
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

    match certificate {
        Certificate::SAT(model) => {
            writeln!(writer, "SAT")?;
            for literal in model {
                write!(writer, "{} ", literal)?;
            }
        }
        Certificate::UNSAT => {
            write!(writer, "UNSAT")?;
        }
    }
    Ok(())
}

pub fn get_content(certificate: Certificate) -> String {
    let contents = match certificate {
        Certificate::SAT(model) => {
            let mut res = String::from("SAT\n");
            for literal in model {
                res.push_str(&format!("{} ", literal));
            }
            res
        }
        Certificate::UNSAT => String::from("UNSAT"),
    };
    contents
}
