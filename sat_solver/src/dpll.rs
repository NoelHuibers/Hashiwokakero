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
            backtrack();
        }

        // * if all clauses satisfied
        if clauses.values().fold(true, |i, c| i && c.sat_by_var != 0) {
            return todo!();
        }
    }
}

fn backtrack() {
    todo!()
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
    var_value.value = value;
    // mark all clauses with pos_occ as sat
    pos_occ.get(&var).unwrap().iter().for_each(|c| {
        clauses.entry(*c).and_modify(|sat_clause| {
            sat_clause.sat_by_var = var;
            sat_clause.unassign_vars = sat_clause.unassign_vars - 1;
        });
    });
    for c in neg_occ.get(&var).unwrap() {
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

#[test]
fn should_solve_sat() {
    solve(vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]]);
}

fn should_solve_unsat() {
    solve(vec![
        vec![1, -2, 3],
        vec![-1, 2],
        vec![-1, -2, -3],
        vec![1],
        vec![2],
        vec![3],
    ]);
}
