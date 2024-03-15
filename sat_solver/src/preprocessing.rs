use std::collections::HashSet;

fn delete_subsumed_clauses(clauses: &mut Vec<Vec<i32>>) {
    let clause_sets: Vec<HashSet<i32>> = clauses
        .iter()
        .map(|clause| clause.iter().copied().collect())
        .collect();
    let mut to_remove = vec![false; clauses.len()];
    for i in 0..clause_sets.len() {
        if to_remove[i] {
            continue;
        }
        for j in i + 1..clause_sets.len() {
            if to_remove[j] {
                continue;
            }
            if clause_sets[i].is_subset(&clause_sets[j]) {
                to_remove[j] = true;
            } else if clause_sets[j].is_subset(&clause_sets[i]) {
                to_remove[i] = true;
                break;
            }
        }
    }
    *clauses = clauses
        .iter()
        .enumerate()
        .filter_map(|(i, clause)| {
            if !to_remove[i] {
                Some(clause.clone())
            } else {
                None
            }
        })
        .collect();
}

#[test]
fn basic_subsumption() {
    let mut clauses = vec![vec![1, 2, 3], vec![1, 2]];
    delete_subsumed_clauses(&mut clauses);
    assert_eq!(clauses, vec![vec![1, 2]]);
}

#[test]
fn no_subsumption() {
    let mut clauses = vec![vec![1, 2], vec![2, 3]];
    delete_subsumed_clauses(&mut clauses);
    assert_eq!(clauses, vec![vec![1, 2], vec![2, 3]]);
}

#[test]
fn multiple_subsumptions() {
    let mut clauses = vec![vec![1, 2], vec![1, 2, 3], vec![1, 2, 3, 4], vec![1]];
    delete_subsumed_clauses(&mut clauses);
    assert_eq!(clauses, vec![vec![1]]);
}

#[test]
fn self_subsumption() {
    let mut clauses = vec![vec![1, 2], vec![1, 2]];
    delete_subsumed_clauses(&mut clauses);
    assert_eq!(clauses, vec![vec![1, 2]]);
}
