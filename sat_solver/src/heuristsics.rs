use std::collections::{HashMap, HashSet};

use crate::dpll::{Clause, Literal};

pub(crate) fn arbitrary(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    (*free_vars.iter().next().unwrap(), true)
}

pub(crate) fn dlis(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn dlcs(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn mom(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn boehm(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn jeroslaw_wang(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn custom(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}