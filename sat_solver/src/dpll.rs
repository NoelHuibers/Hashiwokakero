use crate::heuristsics::{arbitrary, boehm, custom, dlcs, dlis, jeroslaw_wang, mom};
use std::collections::{HashMap, HashSet, VecDeque};

type BVar = i32;
type CIdx = usize;

#[derive(Debug, Clone)]
pub enum DIMACSOutput {
    Sat(Vec<i32>),
    Unsat,
}

// Using structs instead of tuples for readability
// Same mem layout as for tuples:
// - https://doc.rust-lang.org/reference/type-layout.html#tuple-layout
// - https://doc.rust-lang.org/reference/types/struct.html#structtype
pub(crate) struct Clause<'a> {
    pub(crate) sat_by_var: BVar,
    pub(crate) vars: &'a Vec<BVar>,
    pub(crate) unassign_vars: u8,
}

pub(crate) struct Literal {
    pub(crate) val: bool,
    pub(crate) is_free: bool,
}

struct Assignment {
    var: BVar,
    val: bool,
    forced: bool,
}

pub fn solve(input: Vec<Vec<i32>>, heuristic: &str) -> DIMACSOutput {
    let mut assignment_stack: Vec<Assignment> = Vec::new();
    // Using HashMaps due to better get(i) / append complexity, see https://doc.rust-lang.org/std/collections/#sequences
    let mut clauses = HashMap::new();
    let mut lit_val = HashMap::new();
    let mut neg_occ = HashMap::new();
    let mut pos_occ = HashMap::new();
    // Using VecDaque for better push_front complexity
    let mut unit_queue = VecDeque::new();

    // Those are for the heuristics:
    let mut free_lits = HashSet::new();
    let mut unsat_clauses = HashSet::new();

    // * read formula
    // Using iterators where possible for better performance
    input.iter().enumerate().for_each(|(c, vars)| {
        unsat_clauses.insert((vars.clone(), vars.len() as u8));
        let clause = Clause {
            sat_by_var: 0,
            vars: &vars,
            unassign_vars: vars.len() as u8,
        };
        let fist_var = &clause.vars[0];
        clauses.insert(c, clause);
        if vars.len() == 1 && !unit_queue.contains(fist_var) {
            unit_queue.push_front(*fist_var);
        }
        vars.iter().for_each(|literal| {
            let lit = literal.abs();
            lit_val.insert(
                lit,
                Literal {
                    val: false,
                    is_free: true,
                },
            );
            free_lits.insert(*literal);
            match literal.signum() {
                // TODO: Is it neccessary to check whether a variable occurs twice in same pol. in a clause?
                // When not checking clauses like (a \/ -a \/ ...) would not be taken into account.
                1 => pos_occ
                    .entry(lit)
                    .and_modify(|clauses: &mut Vec<CIdx>| clauses.push(c))
                    .or_insert(vec![c]),
                -1 => neg_occ
                    .entry(lit)
                    .and_modify(|clauses: &mut Vec<CIdx>| clauses.push(c))
                    .or_insert(vec![c]),
                _ => panic!("0 is not a valid Variable in the DIMACS format"),
            };
        })
    });

    // * unit propagation
    let conflict = unit_prop(
        &mut assignment_stack,
        &mut clauses,
        &mut free_lits,
        &mut lit_val,
        &mut neg_occ,
        &mut pos_occ,
        &mut unit_queue,
        &mut unsat_clauses,
    );
    if conflict {
        return DIMACSOutput::Unsat
    }
    if clauses.values().fold(true, |i, c| i && c.sat_by_var != 0) {
        let res: Vec<i32> = lit_val
            .iter()
            .map(|v| match v.1.val {
                true => *v.0,
                false => -v.0,
            })
            .collect();
        return DIMACSOutput::Sat(res);
    }

    loop {
        // * choose literal var
        let (var, val) = pick_literal(&clauses, &free_lits, heuristic, &lit_val, &unsat_clauses);
        // * set value var
        set_var(
            &mut assignment_stack,
            &mut clauses,
            false,
            &mut free_lits,
            &mut lit_val,
            &mut neg_occ,
            &mut pos_occ,
            &mut unit_queue,
            &mut unsat_clauses,
            val,
            var,
        );

        // * unit propagation
        loop {
            let conflict = unit_prop(
                &mut assignment_stack,
                &mut clauses,
                &mut free_lits,
                &mut lit_val,
                &mut neg_occ,
                &mut pos_occ,
                &mut unit_queue,
                &mut unsat_clauses,
            );
            if !conflict {
                break;
            };

            // * if conflict detected
            let unsat = backtrack(
                &mut assignment_stack,
                &mut clauses,
                &mut free_lits,
                &mut lit_val,
                &mut neg_occ,
                &mut pos_occ,
                &mut unit_queue,
                &mut unsat_clauses,
            );
            if unsat {
                return DIMACSOutput::Unsat;
            }
        }

        // * if all clauses satisfied
        if clauses.values().fold(true, |i, c| i && c.sat_by_var != 0) {
            let res: Vec<i32> = lit_val
                .iter()
                .map(|v| match v.1.val {
                    true => *v.0,
                    false => -v.0,
                })
                .collect();
            return DIMACSOutput::Sat(res);
        }
    }
}

fn backtrack(
    assignment_stack: &mut Vec<Assignment>,
    clauses: &mut HashMap<CIdx, Clause>,
    free_lits: &mut HashSet<BVar>,
    lit_val: &mut HashMap<BVar, Literal>,
    neg_occ: &mut HashMap<BVar, Vec<CIdx>>,
    pos_occ: &mut HashMap<BVar, Vec<CIdx>>,
    unit_queue: &mut VecDeque<BVar>,
    unsat_clauses: &mut HashSet<(Vec<BVar>, u8)>,
) -> bool {
    let mut last_step = assignment_stack.pop();
    while last_step.as_ref().is_some_and(|step| step.forced) {
        unset_var(
            clauses,
            free_lits,
            lit_val,
            neg_occ,
            pos_occ,
            unsat_clauses,
            last_step.unwrap().var,
        );
        last_step = assignment_stack.pop();
    }
    if last_step.is_none() {
        return true;
    }
    let var = last_step.as_ref().unwrap().var;
    let val = last_step.unwrap().val;
    unset_var(
        clauses,
        free_lits,
        lit_val,
        neg_occ,
        pos_occ,
        unsat_clauses,
        var,
    );
    unit_queue.clear();
    set_var(
        assignment_stack,
        clauses,
        true,
        free_lits,
        lit_val,
        neg_occ,
        pos_occ,
        unit_queue,
        unsat_clauses,
        !val,
        var,
    );
    false
}

// TODO: @Laura hier können die Heuristiken rein. Gerne auch mit enum flag welche an/aus sind.
fn pick_literal(
    clauses: &HashMap<CIdx, Clause>,
    free_vars: &HashSet<BVar>,
    heuristic: &str,
    lit_val: &HashMap<BVar, Literal>,
    unsat_clauses: &HashSet<(Vec<BVar>, u8)>,
) -> (BVar, bool) {
    match heuristic {
        "arbitrary" => arbitrary(clauses, free_vars, lit_val, unsat_clauses),
        "DLIS" => dlis(clauses, free_vars, lit_val, unsat_clauses),
        "DLCS" => dlcs(clauses, free_vars, lit_val, unsat_clauses),
        "MOM" => mom(clauses, free_vars, lit_val, unsat_clauses),
        "Boehm" => boehm(clauses, free_vars, lit_val, unsat_clauses),
        "Jeroslaw-Wang" => jeroslaw_wang(clauses, free_vars, lit_val, unsat_clauses),
        "Custom" => custom(clauses, free_vars, lit_val, unsat_clauses),
        _ => panic!("Unsupported heuristic"),
    }
}

fn unit_prop(
    assignment_stack: &mut Vec<Assignment>,
    clauses: &mut HashMap<CIdx, Clause>,
    free_lits: &mut HashSet<BVar>,
    lit_val: &mut HashMap<BVar, Literal>,
    neg_occ: &mut HashMap<BVar, Vec<CIdx>>,
    pos_occ: &mut HashMap<BVar, Vec<CIdx>>,
    unit_queue: &mut VecDeque<BVar>,
    unsat_clauses: &mut HashSet<(Vec<BVar>, u8)>,
) -> bool {
    while !unit_queue.is_empty() {
        let forced_lit = unit_queue.pop_back().unwrap();
        // mark all clauses with pos_occ as sat
        let unsat = set_var(
            assignment_stack,
            clauses,
            true,
            free_lits,
            lit_val,
            neg_occ,
            pos_occ,
            unit_queue,
            unsat_clauses,
            true,
            forced_lit,
        );
        if unsat {
            return true;
        }
    }
    false
}

fn set_var(
    assignment_stack: &mut Vec<Assignment>,
    clauses: &mut HashMap<CIdx, Clause>,
    forced: bool,
    free_lits: &mut HashSet<BVar>,
    lit_val: &mut HashMap<BVar, Literal>,
    neg_occ: &mut HashMap<BVar, Vec<CIdx>>,
    pos_occ: &mut HashMap<BVar, Vec<CIdx>>,
    unit_queue: &mut VecDeque<BVar>,
    unsat_clauses: &mut HashSet<(Vec<BVar>, u8)>,
    val: bool,
    var: BVar,
) -> bool {
    assignment_stack.push(Assignment { var, val, forced });
    let mut conflict = false;
    let mut lit = lit_val.get_mut(&var.abs()).unwrap();
    lit.is_free = false;
    free_lits.remove(&var);
    free_lits.remove(&-var);
    // -1 true => 1 false
    // -1 false => 1 true
    // 1 true => 1 true
    // 1 false => 1 false
    // <=> var.is_pos == value
    let new_val = val == var.is_positive();
    lit.val = new_val;
    // mark all clauses with pos_occ as sat
    // -1 true => neg_occ is sat
    // -1 false => pos_occ is sat
    // 1 true => pos_occ is sat
    // 1 false => neg_occ is sat
    // <=> var.is_pos == value
    let (mark_sat, mark_unsat) = match new_val {
        true => (pos_occ, neg_occ),
        false => (neg_occ, pos_occ),
    };
    if let Some(occ) = mark_sat.get(&var.abs()) {
        occ.iter().for_each(|c: &CIdx| {
            clauses.entry(*c).and_modify(|sat_clause| {
                if sat_clause.sat_by_var == 0 {
                    sat_clause.sat_by_var = var;
                    unsat_clauses.remove(&(sat_clause.vars.to_vec(), sat_clause.unassign_vars));
                }
            });
        });
    }
    if let Some(occ) = mark_unsat.get(&var.abs()) {
        occ.iter().for_each(|c| {
            let unsat_clause = clauses.get_mut(c).unwrap();
            unsat_clauses.remove(&(unsat_clause.vars.to_vec(), unsat_clause.unassign_vars));
            unsat_clause.unassign_vars = unsat_clause.unassign_vars - 1;
            unsat_clauses.insert((unsat_clause.vars.to_vec(), unsat_clause.unassign_vars));
            match unsat_clause.unassign_vars {
                0 => conflict = true,
                1 => {
                    if let Some(free_lit) = unsat_clause.vars.iter().find(|&v| {
                        lit_val.get(&v.abs()).unwrap().is_free && !unit_queue.contains(v)
                    }) {
                        unit_queue.push_front(*free_lit);
                    }
                }
                _ => (),
            };
        })
    }
    conflict
}

fn unset_var(
    clauses: &mut HashMap<CIdx, Clause>,
    free_lits: &mut HashSet<BVar>,
    lit_val: &mut HashMap<BVar, Literal>,
    neg_occ: &mut HashMap<BVar, Vec<CIdx>>,
    pos_occ: &mut HashMap<BVar, Vec<CIdx>>,
    unsat_clauses: &mut HashSet<(Vec<BVar>, u8)>,
    var: BVar,
) {
    let mut lit_var = lit_val.get_mut(&var.abs()).unwrap();
    lit_var.is_free = true;
    free_lits.insert(var);
    free_lits.insert(-var);
    // Value of lit_var actually doesn't matter
    let (mark_sat, mark_unsat) = match lit_var.val {
        true => (pos_occ, neg_occ),
        false => (neg_occ, pos_occ),
    };
    if let Some(occ) = mark_sat.get(&var.abs()) {
        occ.iter().for_each(|c| {
            clauses.entry(*c).and_modify(|sat_clause| {
                if sat_clause.sat_by_var == var {
                    sat_clause.sat_by_var = 0;
                    unsat_clauses.insert((sat_clause.vars.to_vec(), sat_clause.unassign_vars));
                }
            });
        })
    };
    if let Some(occ) = mark_unsat.get(&var.abs()) {
        occ.iter().for_each(|c| {
            let unsat_clause = clauses.get_mut(c).unwrap();
            unsat_clauses.remove(&(unsat_clause.vars.to_vec(), unsat_clause.unassign_vars));
            unsat_clause.unassign_vars = unsat_clause.unassign_vars + 1;
            unsat_clauses.insert((unsat_clause.vars.to_vec(), unsat_clause.unassign_vars));
        })
    }
}

#[test]
fn should_solve_sat_small() {
    let res = solve(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        "arbitrary",
    );
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_solve_sat() {
    let res = solve(
        vec![
            vec![1, 2, 3],
            vec![1, 2, 4],
            vec![1, 3, 4],
            vec![2, 3, 4],
            vec![-1, -2, -3],
            vec![-1, -2, -4],
            vec![-1, -3, -4],
            vec![-2, -3, -4],
            vec![1, 2, 5],
            vec![1, 2, 6],
            vec![1, 5, 6],
            vec![2, 5, 6],
            vec![-1, -2, -5],
            vec![-1, -2, -6],
            vec![-1, -5, -6],
            vec![-2, -5, -6],
            vec![3, 4, 7],
            vec![3, 4, 8],
            vec![3, 7, 8],
            vec![4, 7, 8],
            vec![-3, -4, -7],
            vec![-3, -4, -8],
            vec![-3, -7, -8],
            vec![-4, -7, -8],
            vec![5, 6, 7],
            vec![5, 6, 8],
            vec![5, 7, 8],
            vec![6, 7, 8],
            vec![-5, -6, -7],
            vec![-5, -6, -8],
            vec![-5, -7, -8],
            vec![-6, -7, -8],
            vec![3, 4],
            vec![1, 2],
            vec![5, 6],
        ],
        "arbitrary",
    );
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_solve_unsat_small() {
    let res = solve(
        vec![
            vec![1, -2, 3],
            vec![-1, 2],
            vec![-1, -2, -3],
            vec![1],
            vec![2],
            vec![3],
        ],
        "arbitrary",
    );
    if let DIMACSOutput::Sat(_) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}

#[test]
fn should_solve_unsat() {
    let res = solve(
        vec![
            vec![1, 2],
            vec![-1, -2],
            vec![3, 4, 5],
            vec![3, 4, 6],
            vec![3, 5, 6],
            vec![4, 5, 6],
            vec![-3, -4, -5],
            vec![-3, -4, -6],
            vec![-3, -5, -6],
            vec![-4, -5, -6],
            vec![3, 4, 7],
            vec![3, 4, 8],
            vec![3, 7, 8],
            vec![4, 7, 8],
            vec![-3, -4, -7],
            vec![-3, -4, -8],
            vec![-3, -7, -8],
            vec![-4, -7, -8],
            vec![5, 6, 9],
            vec![5, 6, 10],
            vec![5, 9, 10],
            vec![6, 9, 10],
            vec![-5, -6, -9],
            vec![-5, -6, -10],
            vec![-5, -9, -10],
            vec![-6, -9, -10],
            vec![1, 2, 9, 10],
            vec![1, 2, 9, 11],
            vec![1, 2, 9, 12],
            vec![1, 2, 10, 11],
            vec![1, 2, 10, 12],
            vec![1, 2, 11, 12],
            vec![1, 9, 10, 11],
            vec![1, 9, 10, 12],
            vec![1, 9, 11, 12],
            vec![1, 10, 11, 12],
            vec![2, 9, 10, 11],
            vec![2, 9, 10, 12],
            vec![2, 9, 11, 12],
            vec![2, 10, 11, 12],
            vec![9, 10, 11, 12],
            vec![-1, -2, -9, -10],
            vec![-1, -2, -9, -11],
            vec![-1, -2, -9, -12],
            vec![-1, -2, -10, -11],
            vec![-1, -2, -10, -12],
            vec![-1, -2, -11, -12],
            vec![-1, -9, -10, -11],
            vec![-1, -9, -10, -12],
            vec![-1, -9, -11, -12],
            vec![-1, -10, -11, -12],
            vec![-2, -9, -10, -11],
            vec![-2, -9, -10, -12],
            vec![-2, -9, -11, -12],
            vec![-2, -10, -11, -12],
            vec![-9, -10, -11, -12],
            vec![7, 8, 11],
            vec![7, 8, 12],
            vec![7, 11, 12],
            vec![8, 11, 12],
            vec![-7, -8, -11],
            vec![-7, -8, -12],
            vec![-7, -11, -12],
            vec![-8, -11, -12],
            vec![-1, -3],
            vec![-1, -4],
            vec![-2, -3],
            vec![-2, -4],
            vec![1, 2],
        ],
        "arbitrary",
    );
    if let DIMACSOutput::Sat(_) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}

#[test]
fn should_parse_and_solve() {
    let input = crate::parse::parse("/home/florian/Git/gruppe-m/backend/output/test2.cnf").unwrap();
    let res = solve(input, "arbitrary");
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_parse_and_solve2() {
    let input = crate::parse::parse("/home/florian/Git/gruppe-m/backend/output/test3.cnf").unwrap();
    let res = solve(input, "arbitrary");
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}
