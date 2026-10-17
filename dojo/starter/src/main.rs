//! A small DPLL solver for the Hashi SAT Dojo. Make it faster!
//!
//! The Dojo protocol:
//! - the formula comes as DIMACS CNF on stdin
//! - answer on stdout: `s SATISFIABLE` with the model on `v` lines ending in 0,
//!   `s UNSATISFIABLE`, or `s UNKNOWN`
//!
//! Build: `cargo build --release --target wasm32-wasip1`
//! Score it locally: `dojo run target/wasm32-wasip1/release/dojo-starter.wasm`
//!
//! Scoring counts executed WebAssembly instructions ("fuel"), not seconds, so every
//! instruction you save shows up in your score, on every machine the same.

use std::io::{BufWriter, Read, Write};

/// A literal: variable `v` (from 1) is `2v` when true and `2v + 1` when false.
type Lit = u32;

fn lit(dimacs: i32) -> Lit {
    let v = dimacs.unsigned_abs() * 2;
    if dimacs > 0 { v } else { v + 1 }
}

fn neg(l: Lit) -> Lit {
    l ^ 1
}

struct Solver {
    clauses: Vec<Vec<Lit>>,
    /// Clauses containing each literal.
    occurs: Vec<Vec<usize>>,
    /// Per variable: 0 unassigned, 1 true, 2 false.
    value: Vec<u8>,
    /// Assigned literals in order, to undo them when backtracking.
    trail: Vec<Lit>,
}

impl Solver {
    fn new(vars: usize, clauses: Vec<Vec<Lit>>) -> Self {
        let mut occurs = vec![Vec::new(); 2 * vars + 2];
        for (i, c) in clauses.iter().enumerate() {
            for &l in c {
                occurs[l as usize].push(i);
            }
        }
        Solver { clauses, occurs, value: vec![0; vars + 1], trail: Vec::new() }
    }

    fn is_true(&self, l: Lit) -> bool {
        self.value[(l >> 1) as usize] == 1 + (l & 1) as u8
    }

    fn is_false(&self, l: Lit) -> bool {
        self.value[(l >> 1) as usize] == 2 - (l & 1) as u8
    }

    fn assign(&mut self, l: Lit) {
        self.value[(l >> 1) as usize] = 1 + (l & 1) as u8;
        self.trail.push(l);
    }

    fn undo(&mut self, to: usize) {
        for l in self.trail.drain(to..) {
            self.value[(l >> 1) as usize] = 0;
        }
    }

    /// Unit propagation from trail position `from`. False on a conflict.
    fn propagate(&mut self, mut from: usize) -> bool {
        while from < self.trail.len() {
            let falsified = neg(self.trail[from]);
            from += 1;
            for i in 0..self.occurs[falsified as usize].len() {
                let c = self.occurs[falsified as usize][i];
                let mut unassigned = None;
                let mut open = 0;
                let mut satisfied = false;
                for &l in &self.clauses[c] {
                    if self.is_true(l) {
                        satisfied = true;
                        break;
                    }
                    if !self.is_false(l) {
                        open += 1;
                        unassigned = Some(l);
                    }
                }
                if satisfied {
                    continue;
                }
                match (open, unassigned) {
                    (0, _) => return false,
                    (1, Some(l)) => self.assign(l),
                    _ => {}
                }
            }
        }
        true
    }

    /// The first unassigned variable. Your first improvement: a better heuristic!
    fn pick(&self) -> Option<Lit> {
        (1..self.value.len()).find(|&v| self.value[v] == 0).map(|v| 2 * v as u32)
    }

    fn solve(&mut self) -> bool {
        // Unit clauses first
        for c in 0..self.clauses.len() {
            match self.clauses[c].len() {
                0 => return false,
                1 => {
                    let l = self.clauses[c][0];
                    if self.is_false(l) {
                        return false;
                    }
                    if !self.is_true(l) {
                        self.assign(l);
                    }
                }
                _ => {}
            }
        }
        self.propagate(0) && self.search()
    }

    fn search(&mut self) -> bool {
        let Some(l) = self.pick() else { return true };
        for choice in [l, neg(l)] {
            let mark = self.trail.len();
            self.assign(choice);
            if self.propagate(mark) && self.search() {
                return true;
            }
            self.undo(mark);
        }
        false
    }
}

fn main() {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).expect("could not read stdin");
    let mut vars = 0;
    let mut clauses = Vec::new();
    let mut clause = Vec::new();
    for line in text.lines() {
        if line.starts_with('c') {
            continue;
        }
        if let Some(header) = line.strip_prefix("p cnf") {
            vars = header.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
            continue;
        }
        for n in line.split_whitespace().filter_map(|t| t.parse::<i32>().ok()) {
            if n == 0 {
                clauses.push(std::mem::take(&mut clause));
            } else {
                vars = vars.max(n.unsigned_abs() as usize);
                clause.push(lit(n));
            }
        }
    }

    let mut solver = Solver::new(vars, clauses);
    let mut out = BufWriter::new(std::io::stdout().lock());
    if solver.solve() {
        write!(out, "s SATISFIABLE\nv").unwrap();
        for v in 1..=vars {
            let sign = if solver.value[v] == 2 { "-" } else { "" };
            write!(out, " {sign}{v}").unwrap();
        }
        writeln!(out, " 0").unwrap();
    } else {
        writeln!(out, "s UNSATISFIABLE").unwrap();
    }
}
