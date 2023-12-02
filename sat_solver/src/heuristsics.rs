use std::collections::{HashMap, HashSet};

use crate::dpll::{Clause, Literal};

pub(crate) fn arbitrary(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<i32, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    let arbitrary_unset_var = lit_val.iter().find(|v| v.1.is_free);
    (
        *arbitrary_unset_var.unwrap().0,
        arbitrary_unset_var.unwrap().1.val,
    )
}

pub(crate) fn dlis(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<i32, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn dlcs(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<i32, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn mom(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<i32, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn boehm(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<i32, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn jeroslaw_wang(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<i32, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}

pub(crate) fn custom(
    clauses: &HashMap<usize, Clause>,
    free_vars: &HashSet<i32>,
    lit_val: &HashMap<i32, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    todo!()
}