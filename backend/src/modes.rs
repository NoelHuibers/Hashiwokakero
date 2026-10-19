//! The modes of the CLI (see `main.rs`). Errors are returned, `main` prints them and exits
//! with a failure status.

use std::fs;

use common::{Board, GenerateParams, Puzzle, SolveOutcome, SolverConfig};

use crate::{api, reconstruct, solver};

fn read_puzzle(input_file: &str) -> Result<Puzzle, String> {
    let text = fs::read_to_string(input_file).map_err(|e| format!("{input_file}: {e}"))?;
    let puzzle = Puzzle::parse_txt(&text)?;
    api::validate(&puzzle)?;
    Ok(puzzle)
}

/// Writes the static encoding of a puzzle to `<input_file>.cnf`.
pub fn encode_mode(input_file: &str) -> Result<(), String> {
    let dimacs = read_puzzle(input_file).and_then(|p| api::to_dimacs(&p))?;
    let out_file = format!("{input_file}.cnf");
    fs::write(&out_file, dimacs).map_err(|e| format!("{out_file}: {e}"))?;
    println!("Successfully generated {out_file}");
    Ok(())
}

/// Solves a DIMACS CNF file.
pub fn solve_mode(
    input_file: &str,
    output_file: Option<String>,
    config: SolverConfig,
) -> Result<(), String> {
    let certificate =
        solver::solve_file(input_file, &config).map_err(|e| format!("{input_file}: {e}"))?;
    if let Some(output_file) = output_file {
        solver::write_solution(certificate, &output_file)
            .map_err(|e| format!("{output_file}: {e}"))?;
        println!("Solution written to {output_file}");
    } else {
        println!("{}", solver::get_content(certificate));
    }
    Ok(())
}

/// Encodes, solves (with connectivity and uniqueness check) and draws the solution of a puzzle.
pub fn esr_mode(
    input_file: &str,
    output_file: Option<String>,
    config: SolverConfig,
) -> Result<(), String> {
    let puzzle = read_puzzle(input_file)?;
    let report = api::solve_unique(&puzzle, &config)?;
    let text = match &report.outcome {
        SolveOutcome::Solved { bridges, unique } => {
            let board = Board::new(puzzle);
            let uniqueness = match unique {
                Some(true) => "unique solution",
                Some(false) => "NOT unique, there are other solutions",
                None => "uniqueness unknown (timeout)",
            };
            format!("{}({uniqueness})\n", reconstruct::render(&board, bridges))
        }
        SolveOutcome::Unsatisfiable => "Problem is UNSAT\n".to_string(),
        SolveOutcome::Timeout => "Timeout\n".to_string(),
    };
    let s = &report.stats;
    eprintln!(
        "{} | {} variables, {} clauses | encode {:.2} ms, solve {:.2} ms | {} SAT calls, {} connectivity cuts{}{}",
        s.config.describe(),
        s.variables,
        s.clauses,
        s.encode_ms,
        s.solve_ms,
        s.sat_calls,
        s.connectivity_cuts,
        s.decisions
            .map_or(String::new(), |d| format!(", {d} decisions")),
        s.restarts
            .map_or(String::new(), |r| format!(", {r} restarts")),
    );
    match output_file {
        Some(output_file) => {
            fs::write(&output_file, &text).map_err(|e| format!("{output_file}: {e}"))?;
            println!("Solution written to {output_file}");
        }
        None => print!("{text}"),
    }
    Ok(())
}

/// Generates a puzzle and writes it in the input format (to stdout without output file).
pub fn generate_mode(params: GenerateParams, output_file: Option<String>) -> Result<(), String> {
    let generated = api::generate(&params);
    let p = &generated.params;
    eprintln!(
        "{}x{} {} (seed {}): {} islands, unique: {}, {} attempts, {:.1} ms",
        p.width,
        p.height,
        p.difficulty.label(),
        p.seed,
        generated.puzzle.islands.len(),
        generated.unique,
        generated.attempts,
        generated.generate_ms
    );
    let text = generated.puzzle.to_txt();
    if let Some(output_file) = output_file {
        fs::write(&output_file, &text).map_err(|e| format!("{output_file}: {e}"))?;
        println!("Puzzle written to {output_file}");
    } else {
        print!("{text}");
        println!();
        print!(
            "{}",
            reconstruct::render(&Board::new(generated.puzzle.clone()), &generated.solution)
        );
    }
    Ok(())
}
