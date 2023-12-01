use std::{
    collections::{HashMap, HashSet, VecDeque},
    u8::MAX,
};

type BVar = i32;

#[derive(Debug, Clone)]
enum DIMACSOutput {
    Sat(Vec<i32>),
    Unsat,
}

// Using structs instead of tuples for readability
// Same mem layout as for tuples:
// - https://doc.rust-lang.org/reference/type-layout.html#tuple-layout
// - https://doc.rust-lang.org/reference/types/struct.html#structtype
struct Clause<'a> {
    sat_by_var: BVar,
    vars: &'a Vec<BVar>,
    unassign_vars: u8,
}

struct Variable {
    value: bool,
    is_free: bool,
}

struct Assignment {
    var: BVar,
    value: bool,
    forced: bool,
}

fn solve(input: Vec<Vec<i32>>) -> DIMACSOutput {
    // Using HashMaps due to better get(i) / append complexity, see https://doc.rust-lang.org/std/collections/#sequences
    // The first element in (i32, &Vec<i32>, u8) represents whether a clause was satisfied and if yes by which var:
    // 0 -> not satisfied
    // n -> satisfied by n
    let mut clauses = HashMap::new();
    let mut neg_occ = HashMap::new();
    let mut pos_occ = HashMap::new();
    // variable maps to 0|1, free|set
    let mut var_value = HashMap::new();
    // Using VecDaque for better push_front complexity
    let mut unit_queue = VecDeque::new();
    // (i32, bool, bool) is the variable (incl. polarity), the value 0|1 and whether branched|forced
    let mut assignment_stack: Vec<Assignment> = Vec::new();

    // * read formula
    // Using iterators where possible for better performance
    input.iter().enumerate().for_each(|(c, vars)| {
        let clause = Clause {
            sat_by_var: 0,
            vars: &vars,
            unassign_vars: vars.len() as u8,
        };
        clauses.insert(c, clause);
        if vars.len() == 1 {
            unit_queue.push_front(c);
        }
        vars.iter().for_each(|literal| {
            let lit = literal.abs();
            var_value.insert(
                lit,
                Variable {
                    value: false,
                    is_free: true,
                },
            );
            match literal.signum() {
                // TODO: Is it neccessary to check whether a variable occurs twice in same pol. in a clause?
                // When not checking clauses like (a \/ -a \/ ...) would not be taken into account.
                1 => pos_occ
                    .entry(lit)
                    .and_modify(|clauses: &mut Vec<usize>| clauses.push(c))
                    .or_insert(vec![c]),
                -1 => neg_occ
                    .entry(lit)
                    .and_modify(|clauses: &mut Vec<usize>| clauses.push(c))
                    .or_insert(vec![c]),
                _ => panic!("0 is not a valid Variable in the DIMACS format"),
            };
        })
    });

    // * unit propagation
    let unsat = unit_prop(
        &mut unit_queue,
        &mut clauses,
        &mut assignment_stack,
        &mut var_value,
        &mut pos_occ,
        &mut neg_occ,
    );
    if unsat {
        return DIMACSOutput::Unsat;
    }

    loop {
        // * choose literal var
        let (var, val) = choose_literal(&mut var_value);
        // * set value var
        set_var(
            var,
            false,
            val,
            &mut unit_queue,
            &mut clauses,
            &mut assignment_stack,
            &mut var_value,
            &mut pos_occ,
            &mut neg_occ,
        );

        // * unit propagation
        let unsat = unit_prop(
            &mut unit_queue,
            &mut clauses,
            &mut assignment_stack,
            &mut var_value,
            &mut pos_occ,
            &mut neg_occ,
        );

        // * if conflict detected
        if unsat {
            let unsat = backtrack(
                &mut clauses,
                &mut assignment_stack,
                &mut var_value,
                &mut unit_queue,
                &mut pos_occ,
                &mut neg_occ,
            );
            if unsat {
                return DIMACSOutput::Unsat;
            }
            let _ = unit_prop(
                &mut unit_queue,
                &mut clauses,
                &mut assignment_stack,
                &mut var_value,
                &mut pos_occ,
                &mut neg_occ,
            );
        }

        // * if all clauses satisfied
        if clauses.values().fold(true, |i, c| i && c.sat_by_var != 0) {
            return todo!();
        }
    }
}

fn backtrack(
    clauses: &mut HashMap<usize, Clause>,
    assignment_stack: &mut Vec<Assignment>,
    var_value: &mut HashMap<i32, Variable>,
    unit_queue: &mut VecDeque<usize>,
    pos_occ: &mut HashMap<i32, Vec<usize>>,
    neg_occ: &mut HashMap<i32, Vec<usize>>,
) -> bool {
    let mut last_step = assignment_stack.pop();
    while last_step.as_ref().is_some_and(|step| step.forced) {
        unset_var(last_step.unwrap().var, clauses, var_value, pos_occ, neg_occ);
        last_step = assignment_stack.pop();
    }
    if assignment_stack.is_empty() {
        return true;
    }
    let var = last_step.as_ref().unwrap().var;
    let val = last_step.unwrap().value;
    unset_var(var, clauses, var_value, pos_occ, neg_occ);
    unit_queue.clear();
    set_var(
        var,
        true,
        !val,
        unit_queue,
        clauses,
        assignment_stack,
        var_value,
        pos_occ,
        neg_occ,
    );
    false
}

// TODO: @Laura hier können die Heuristiken rein. Gerne auch mit enum flag welche an/aus sind.
fn choose_literal(var_value: &mut HashMap<i32, Variable>) -> (i32, bool) {
    let arbitrary_unset_var = var_value.iter().find(|v| v.1.is_free).unwrap();
    (*arbitrary_unset_var.0, arbitrary_unset_var.1.value)
}

fn unit_prop(
    unit_queue: &mut VecDeque<usize>,
    clauses: &mut HashMap<usize, Clause>,
    assignment_stack: &mut Vec<Assignment>,
    var_value: &mut HashMap<i32, Variable>,
    pos_occ: &mut HashMap<i32, Vec<usize>>,
    neg_occ: &mut HashMap<i32, Vec<usize>>,
) -> bool {
    while !unit_queue.is_empty() {
        let unit_clause = unit_queue.pop_back().unwrap();
        let clause = clauses.get(&unit_clause).unwrap();
        let forced_var = clause
            .vars
            .iter()
            .find(|v| var_value.get(&v.abs()).unwrap().is_free)
            .unwrap();
        assignment_stack.push(Assignment {
            var: *forced_var,
            value: forced_var.is_positive(),
            forced: true,
        });
        // mark all clauses with pos_occ as sat
        let unsat = set_var(
            *forced_var,
            true,
            forced_var.is_positive(),
            unit_queue,
            clauses,
            assignment_stack,
            var_value,
            pos_occ,
            neg_occ,
        );
        if unsat {
            return true;
        }
    }
    false
}

fn set_var(
    var: BVar,
    forced: bool,
    value: bool,
    unit_queue: &mut VecDeque<usize>,
    clauses: &mut HashMap<usize, Clause>,
    assignment_stack: &mut Vec<Assignment>,
    var_value: &mut HashMap<i32, Variable>,
    pos_occ: &mut HashMap<i32, Vec<usize>>,
    neg_occ: &mut HashMap<i32, Vec<usize>>,
) -> bool {
    assignment_stack.push(Assignment { var, value, forced });
    let mut var_value = var_value.get_mut(&var.abs()).unwrap();
    var_value.is_free = false;
    // -1 true => 1 false
    // -1 false => 1 true
    // 1 true => 1 true
    // 1 false => 1 false
    // <=> var.is_pos == value
    var_value.value = value == var.is_positive();
    // mark all clauses with pos_occ as sat
    // -1 true => pos 1 sat
    // -1 false => neg 1 sat
    // 1 true => pos 1 sat
    // 1 false => neg 1 sat
    let (mark_sat, mark_unsat) = match var.is_positive() & value {
        true => (pos_occ, neg_occ),
        false => (neg_occ, pos_occ),
    };
    mark_sat.get(&var.abs()).unwrap().iter().for_each(|c: &usize| {
        clauses.entry(*c).and_modify(|sat_clause| {
            sat_clause.sat_by_var = var;
        });
    });
    for c in mark_unsat.get(&var.abs()).unwrap() {
        let unsat_clause = clauses.get_mut(c).unwrap();
        unsat_clause.unassign_vars = unsat_clause.unassign_vars - 1;
        match unsat_clause.unassign_vars {
            0 => return true,
            1 => unit_queue.push_front(*c),
            _ => continue,
        };
    }
    false
}

fn unset_var(
    var: BVar,
    clauses: &mut HashMap<usize, Clause>,
    var_value: &mut HashMap<i32, Variable>,
    pos_occ: &mut HashMap<i32, Vec<usize>>,
    neg_occ: &mut HashMap<i32, Vec<usize>>,
) {
    let mut var_value = var_value.get_mut(&var.abs()).unwrap();
    var_value.is_free = true;
    // Value actually doesn't matter
    var_value.value = false;
    let (mark_sat, mark_unsat) = match var.is_positive() & var_value.value {
        true => (pos_occ, neg_occ),
        false => (neg_occ, pos_occ),
    };
    mark_sat.get(&var).unwrap().iter().for_each(|c| {
        clauses.entry(*c).and_modify(|sat_clause| {
            if sat_clause.sat_by_var == var {
                sat_clause.sat_by_var = 0;
            }
        });
    });
    for c in mark_unsat.get(&var).unwrap() {
        let unsat_clause = clauses.get_mut(c).unwrap();
        unsat_clause.unassign_vars = unsat_clause.unassign_vars - 1;
    }
}

#[test]
fn should_solve_sat() {
    let res = solve(vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]]);
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

fn should_solve_unsat() {
    let res = solve(vec![
        vec![1, -2, 3],
        vec![-1, 2],
        vec![-1, -2, -3],
        vec![1],
        vec![2],
        vec![3],
    ]);
    if let DIMACSOutput::Sat(_) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}
