use std::collections::{HashMap, HashSet, VecDeque};

use crate::dpll::Literal;

pub(crate) fn arbitrary(free_lits: &HashSet<i32>) -> (i32, bool) {
    match free_lits.iter().next() {
        Some(&lit) => (lit, true),
        None => (0, true),
    }
}

// DLIS:  picks the variable that appears in the largest number of unsatisfied clauses,
// aiming to quickly reduce the search space
pub(crate) fn dlis(
    free_lits: &HashSet<i32>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    // Create a map to count the occurrences of each variable in unsatisfied clauses
    let mut var_occurrences: HashMap<i32, usize> = HashMap::new();

    // Iterate over all unsatisfied clauses
    for (clause, _) in unsat_clauses {
        for &var in clause {
            let abs_var = var.abs();
            if free_lits.contains(&abs_var) || free_lits.contains(&-abs_var) {
                // Increment the count for this variable
                *var_occurrences.entry(var).or_insert(0) += 1;
            }
        }
    }

    // Find the variable with the highest occurrence count
    let mut max_var = 0;
    let mut max_count = 0;
    for (&var, &count) in var_occurrences.iter() {
        if count > max_count {
            max_var = var;
            max_count = count;
        }
    }

    // If no variable was found, return a default value
    if max_var == 0 {
        return (*free_lits.iter().next().unwrap(), true);
    }

    // The value to assign is true if the variable appears more often positively
    let val = max_var > 0;
    (max_var.abs(), val)
}

// DLCS: chooses the variable that appears the most in the unsatisfied clauses,
// counting both its positive and negative occurrences.
// It's similar to DLIS but considers both polarities of the variable.
pub(crate) fn dlcs(
    free_lits: &HashSet<i32>,
    _lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    let mut var_occurrences: HashMap<i32, usize> = HashMap::new();

    for (clause, _) in unsat_clauses {
        for &var in clause {
            let abs_var = var.abs();
            if free_lits.contains(&abs_var) {
                *var_occurrences.entry(abs_var).or_insert(0) += 1;
            }
        }
    }

    let &max_var = var_occurrences
        .iter()
        .max_by_key(|entry| entry.1)
        .map(|(var, _)| var)
        .unwrap_or(&0);
    if max_var == 0 {
        return (*free_lits.iter().next().unwrap(), true);
    }

    let val = true; // Default assignment
    (max_var, val)
}

// MOM:  selects a variable from the smallest unsatisfied clauses.
// Within these clauses, it picks the variable that occurs the most frequently.
// This heuristic focuses on resolving smaller clauses first, as they are closer to causing conflicts.
pub(crate) fn mom(
    free_lits: &HashSet<i32>,
    _lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    let min_clause_size = unsat_clauses
        .iter()
        .map(|(_, size)| *size)
        .min()
        .unwrap_or(0);
    let mut var_occurrences: HashMap<i32, usize> = HashMap::new();

    for (clause, size) in unsat_clauses {
        if *size == min_clause_size {
            for &var in clause {
                if free_lits.contains(&var.abs()) {
                    *var_occurrences.entry(var.abs()).or_insert(0) += 1;
                }
            }
        }
    }

    let &max_var = var_occurrences
        .iter()
        .max_by_key(|entry| entry.1)
        .map(|(var, _)| var)
        .unwrap_or(&0);
    if max_var == 0 {
        return (*free_lits.iter().next().unwrap(), true);
    }

    let val = true; // Default assignment
    (max_var, val)
}

// Boehm: is a variation of MOM.
// It includes a bias term to favor variables appearing in smaller clauses.
// This term is usually a power of the number of occurrences of the variable in small clauses.
pub(crate) fn boehm(
    free_lits: &HashSet<i32>,
    _lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    let min_clause_size = unsat_clauses
        .iter()
        .map(|(_, size)| *size)
        .min()
        .unwrap_or(0);
    let mut var_occurrences: HashMap<i32, usize> = HashMap::new();

    for (clause, size) in unsat_clauses {
        if *size == min_clause_size {
            for &var in clause {
                if free_lits.contains(&var.abs()) {
                    *var_occurrences.entry(var.abs()).or_insert(0) +=
                        2usize.pow(min_clause_size as u32); // Bias term
                }
            }
        }
    }

    let &max_var = var_occurrences
        .iter()
        .max_by_key(|entry| entry.1)
        .map(|(var, _)| var)
        .unwrap_or(&0);
    if max_var == 0 {
        return (*free_lits.iter().next().unwrap(), true);
    }

    let val = true; // Default assignment
    (max_var, val)
}

// Jeroslaw-Wang: calculates a score for each variable based on its appearances in the clauses.
// The score of a literal in a clause is 2^(-clause_size).
// The total score for a variable is the sum of the scores of its literals.
pub(crate) fn jeroslaw_wang(
    free_lits: &HashSet<i32>,
    _lit_val: &HashMap<u16, Literal>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    let mut scores: HashMap<i32, f64> = HashMap::new();

    for (clause, size) in unsat_clauses {
        let score = 2f64.powf(-(*size as f64));
        for &var in clause {
            if free_lits.contains(&var.abs()) {
                *scores.entry(var.abs()).or_insert(0.0) += score;
            }
        }
    }

    let &max_var = scores
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(var, _)| var)
        .unwrap_or(&0);
    if max_var == 0 {
        return (*free_lits.iter().next().unwrap(), true);
    }

    let val = true; // Default assignment
    (max_var, val)
}

// VSIDS
pub(crate) fn vsids(
    free_lits: &HashSet<i32>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    let mut scores: HashMap<i32, f64> = HashMap::new();

    for (clause, _) in unsat_clauses {
        for &var in clause {
            if free_lits.contains(&var.abs()) {
                *scores.entry(var.abs()).or_insert(0.0) += 1.0;
            }
        }
    }

    let &max_var = scores
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(var, _)| var)
        .unwrap_or(&0);
    if max_var == 0 {
        return (*free_lits.iter().next().unwrap(), true);
    }

    let val = true; // Default assignment
    (max_var, val)
}

// VMTF: Selects the last variable involved in a conflict
pub(crate) fn vmtf(
    free_lits: &HashSet<i32>,
    all_clauses: &Vec<Vec<i32>>, 
    learned_clauses: &Vec<Vec<i32>>,
    vmtf_order: &mut VecDeque<i32>,
    num_conflicts: usize,
) -> (i32, bool) {
    // Initialization of n(a) based on the initial heuristic h(a)
    let mut initial_weights = HashMap::new();
    for clause in all_clauses {
        for &lit in clause {
            let abs_lit = lit.abs();
            *initial_weights.entry(abs_lit).or_insert(0) += 1;
        }
    }

    // Using the initial weights as a starting point
    let mut lit_weights = initial_weights.clone();

    // Dynamic adjustment of the number of clauses last viewed
    let num_recent_clauses = std::cmp::min(10 + num_conflicts / 100, learned_clauses.len());

    // Updating the weights based on the last learned clauses
    for clause in learned_clauses.iter().rev().take(num_recent_clauses) {
        for &lit in clause {
            let abs_lit = lit.abs();
            if free_lits.contains(&abs_lit) {
                *lit_weights.entry(abs_lit).or_insert(0) += 1;
            }
        }
    }

    // Sorting of literals based on their updated weights
    let mut sorted_lits: Vec<(i32, usize)> = lit_weights.iter().map(|(&lit, &weight)| (lit, weight)).collect();
    sorted_lits.sort_by(|a, b| b.1.cmp(&a.1));

    // Moving the literals with the highest weights to the beginning of vmtf_order
    for (lit, _) in sorted_lits.iter().take(std::cmp::min(sorted_lits.len(), 8)) {
        vmtf_order.retain(|&x| x != *lit);
        vmtf_order.push_front(*lit);
    }

    // Selection of the first literal in the list
    if let Some(&lit) = vmtf_order.front() {
        return (lit, true);
    }

    // Fallback
    if let Some(&fallback_lit) = free_lits.iter().next() {
        return (fallback_lit, true);
    }

    // Error Handling
    (0, false)
}


// BerkMin: Selects a variable from the most recent unsatisfied clause
pub struct BerkMinData {
    priorities: HashMap<i32, f64>,
    decay_factor: f64, // Factor for periodic updating
}

impl BerkMinData {
    pub(crate) fn new() -> Self {
        BerkMinData {
            priorities: HashMap::new(),
            decay_factor: 0.25, 
        }
    }

    // Function for updating the priorities for literals in conflict clauses
    pub(crate) fn update_priorities(&mut self, clause: &Vec<i32>) {
        for &lit in clause {
            *self.priorities.entry(lit.abs()).or_insert(0.0) += 1.0;
        }
    }

    // Function for periodically updating the priorities
    pub(crate) fn decay_priorities(&mut self) {
        for priority in self.priorities.values_mut() {
            *priority *= self.decay_factor; // Updates the priorities by multiplying by the decay factor
        }
    }
}

pub(crate) fn berkmin(
    free_lits: &HashSet<i32>,
    unsat_clauses: &[(Vec<i32>, u8)], 
    berkmin_data: &mut BerkMinData,
) -> (i32, bool) {
    if let Some((most_recent_clause, _)) = unsat_clauses.last() { // Access to the latest unfulfilled clause
        berkmin_data.update_priorities(most_recent_clause);

        // Selects the literal with the highest priority from this clause
        let &selected_lit = most_recent_clause
            .iter()
            .filter(|&lit| free_lits.contains(&lit.abs()))
            .max_by(|&a, &b| {
                berkmin_data.priorities.get(&a.abs())
                .unwrap_or(&0.0) 
                .partial_cmp(berkmin_data.priorities.get(&b.abs()).unwrap_or(&0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(&0);

        if selected_lit != 0 {
            return (selected_lit.abs(), selected_lit > 0);
        }
    }

    // Selects the literal with the highest priority from this clause
    if let Some(&fallback_lit) = free_lits.iter().next() {
        return (fallback_lit, true);
    }

    // Error handling if no literals are available
    (0, false) // or panic!("BerkMin: No free literal available");
}


// Custom: Least number of clauses: selects the variable that appears in the smallest number of clauses.
pub(crate) fn custom(
    free_lits: &HashSet<i32>,
    unsat_clauses: &HashSet<(Vec<i32>, u8)>,
) -> (i32, bool) {
    let mut scores: HashMap<i32, usize> = HashMap::new();

    for (clause, _) in unsat_clauses {
        for &var in clause {
            if free_lits.contains(&var.abs()) {
                *scores.entry(var.abs()).or_insert(0) += 1;
            }
        }
    }

    let &min_var = scores
        .iter()
        .min_by(|a, b| a.1.cmp(b.1))
        .map(|(var, _)| var)
        .unwrap_or(&0);
    let val = true; // Default assignment
    (min_var, val)
}
