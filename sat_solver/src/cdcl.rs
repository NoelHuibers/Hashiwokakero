use flame;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::stdout,
};

use crate::dpll::DIMACSOutput; // Add this line to import the `cli` module from the crate root

type Atom = u16;
type BVar = i32;
type CIdx = usize;

// Using structs instead of tuples for readability
// Same mem layout as for tuples:
// - https://doc.rust-lang.org/reference/type-layout.html#tuple-layout
// - https://doc.rust-lang.org/reference/types/struct.html#structtype
pub(crate) struct Clause {
    pub(crate) watched_lhs: BVar,
    pub(crate) watched_rhs: BVar,
    pub(crate) vars: Vec<BVar>,
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

pub struct CDCL {
    // Using HashMaps due to better get(i) / append complexity, see https://doc.rust-lang.org/std/collections/#sequences
    clauses: HashMap<usize, Clause>,
    free_vars: HashSet<i32>,
    history: Vec<Assignment>,
    // Memory allocation in the history is a bottle neck thus the initial unit prop and pure lit elim don't need to expand it
    history_enabled: bool,
    lit_val: HashMap<Atom, Literal>,
    min_depth: u16,
    // Keys don't contain the sign as abs is cheaper than calculating the sign every time
    pos_watched_occ: HashMap<BVar, Vec<CIdx>>,
    // Using VecDaque for better push_front complexity
    unit_queue: VecDeque<BVar>,
}

impl CDCL {
    pub fn new(
        input: Vec<Vec<i32>>,
        lit_count: usize,
        clause_count: usize,
        show_depth: bool,
    ) -> Self {
        let mut cdcl = CDCL {
            clauses: HashMap::with_capacity(clause_count),
            free_vars: HashSet::with_capacity(lit_count),
            history: Vec::new(),
            history_enabled: false,
            lit_val: HashMap::with_capacity(lit_count),
            min_depth: match show_depth {
                true => u16::MAX,
                false => 0,
            },
            pos_watched_occ: HashMap::with_capacity(clause_count * 2),
            unit_queue: VecDeque::new(),
        };
        // * read formula
        // Using iterators where possible for better performance
        input.into_iter().enumerate().for_each(|(c, vars)| {
            let clause = Clause {
                watched_lhs: *vars.first().unwrap(),
                watched_rhs: *vars.last().unwrap(),
                vars: vars.clone(),
            };
            let fist_var = &clause.vars.clone()[0];
            cdcl.clauses.insert(c, clause);
            if vars.len() == 1 && !cdcl.unit_queue.contains(fist_var) {
                cdcl.unit_queue.push_front(*fist_var);
            }
            // Only set watched lits in watched_occ
            vec![vars.first().unwrap(), vars.last().unwrap()]
                .iter()
                .for_each(|literal| {
                    cdcl.pos_watched_occ
                        .entry(**literal)
                        .and_modify(|clauses: &mut Vec<CIdx>| clauses.push(c))
                        .or_insert(vec![c]);
                });
            // All vars will be initialized and set as free
            vars.iter().for_each(|literal| {
                let lit = literal;
                cdcl.free_vars.insert(*lit);
                cdcl.lit_val.insert(
                    literal.abs() as Atom,
                    Literal {
                        val: false,
                        is_free: true,
                    },
                );
            });
        });
        cdcl
    }

    pub fn solve(&mut self) -> DIMACSOutput {
        // * unit propagation
        let conflict = self.unit_prop();
        if conflict {
            return DIMACSOutput::Unsat;
        }
        self.history_enabled = true;
        if self.clauses.values().fold(true, |i, c| {
            let lhs = self.lit_val.get(&(c.watched_lhs.abs() as Atom));
            let rhs = self.lit_val.get(&(c.watched_rhs.abs() as Atom));
            i && (lhs.is_some_and(|l| !l.is_free) || rhs.is_some_and(|l| !l.is_free))
        }) {
            let res: Vec<i32> = self
                .lit_val
                .iter()
                .map(|(atom, lit)| match lit.val {
                    true => *atom as BVar,
                    false => -(*atom as BVar),
                })
                .collect();
            return DIMACSOutput::Sat(res);
        }

        // * pure lit elim

        loop {
            // * choose literal var
            // TODO is currently very inefficient and needs to be replaced by picking conflict literals.
            let var = self.free_vars.iter().next().unwrap();
            let (var, val, forced) = (var, var.is_positive(), false);
            // * set value var
            self.set_var(forced, val, *var);

            // * unit propagation
            loop {
                let conflict = self.unit_prop();
                if !conflict {
                    break;
                };

                // * if conflict detected
                let unsat = self.backtrack();
                if unsat {
                    return DIMACSOutput::Unsat;
                }
            }

            // * if all clauses satisfied
            if self.clauses.values().fold(true, |i, c| {
                let lhs = self.lit_val.get(&(c.watched_lhs.abs() as Atom));
                let rhs = self.lit_val.get(&(c.watched_rhs.abs() as Atom));
                i && (lhs.is_some_and(|l| !l.is_free) || rhs.is_some_and(|l| !l.is_free))
            }) {
                let res: Vec<i32> = self
                    .lit_val
                    .iter()
                    .map(|(atom, lit)| match lit.val {
                        true => *atom as BVar,
                        false => -(*atom as BVar),
                    })
                    .collect();
                return DIMACSOutput::Sat(res);
            }
        }
    }

    fn backtrack(&mut self) -> bool {
        let mut last_step = self.history.pop();
        while last_step.as_ref().is_some_and(|step| step.forced) {
            self.unset_var(last_step.unwrap().var);
            last_step = self.history.pop();
        }
        if self.history.len() as u16 <= self.min_depth {
            self.min_depth = self.history.len() as u16;
            println!("backtracked to depth {}", self.history.len());
        }
        if last_step.is_none() {
            return true;
        }
        let var = last_step.as_ref().unwrap().var;
        let val = last_step.unwrap().val;
        self.unset_var(var);
        self.unit_queue.clear();
        self.set_var(true, !val, var);
        false
    }

    fn unit_prop(&mut self) -> bool {
        while !self.unit_queue.is_empty() {
            let forced_lit = self.unit_queue.pop_back().unwrap();
            // Try to set the literal. If this results in a conflict, `unsat` is true.
            let unsat = self.set_var(true, true, forced_lit);
            if unsat {
                // A conflict has been detected, so derive and add a conflict clause.
                self.derive_and_add_conflict_clause();
                return true; // Signalize that a conflict has occurred
            }
        }
        false // No conflict found
    } 

    fn set_var(&mut self, forced: bool, val: bool, var: BVar) -> bool {
        let mut conflict = false;
        if self.history_enabled {
            self.history.push(Assignment { var, val, forced });
        }
        let lit = self.lit_val.get_mut(&(var.abs() as Atom)).unwrap();
        lit.is_free = false;
        self.free_vars.remove(&var);
        self.free_vars.remove(&(var * -1));
        // -1 true => 1 false
        // -1 false => 1 true
        // 1 true => 1 true
        // 1 false => 1 false
        // <=> var.is_pos == value
        let new_val = val == var.is_positive();
        lit.val = new_val;
        let conflict_literal = match val {
            true => var * -1,
            false => var,
        };
        let conflict_clauses = self.pos_watched_occ.get(&conflict_literal);
        if let None = conflict_clauses {
            return conflict;
        }
        for c_idx in conflict_clauses.unwrap().clone().iter() {
            let mut clause = self.clauses.get_mut(c_idx).unwrap();
            // * if satisfying literal encountert
            if clause.vars.iter().any(|&v| {
                self.lit_val
                    .get(&(v.abs() as Atom))
                    .is_some_and(|a: &Literal| !a.is_free && (a.val == v.is_positive()))
            }) {
                // Because this clause is already sat
                continue;
            }
            let new_watched_cands = clause
                .vars
                .iter()
                .filter(|&v| self.lit_val.get(&(v.abs() as Atom)).unwrap().is_free)
                .collect::<Vec<&BVar>>();
            match new_watched_cands.len() {
                // * if no unassigned literal found
                0 => {
                    conflict = true;
                }
                1 => {
                    // * if only one is found
                    self.unit_queue
                        .push_front(**new_watched_cands.iter().next().unwrap());
                }
                _ => (),
            }
            let other_watched = match conflict_literal == clause.watched_rhs {
                true => clause.watched_lhs,
                false => clause.watched_rhs,
            };
            // At least one of them is already implicitly removed but we don't know which one at this point.
            let new_watched_cands: Vec<_> = new_watched_cands
                .iter()
                .filter(|&v| **v != other_watched)
                .collect();
            if new_watched_cands.len() == 0 {
                continue;
            }
            let new = *new_watched_cands.first().unwrap();
            // * mark new var as watched
            match conflict_literal == clause.watched_rhs {
                true => clause.watched_rhs = **new,
                false => clause.watched_lhs = **new,
            };
            let old = conflict_literal;
            let vars = self.pos_watched_occ.get_mut(&old).unwrap();
            let idx = vars.iter().position(|x| x == c_idx).unwrap();
            vars.remove(idx);
            // * add clause to watch list
            self.pos_watched_occ
                .entry(**new)
                .and_modify(|vec| vec.push(*c_idx))
                .or_insert(vec![*c_idx]);
        }
        conflict
    }

    fn unset_var(&mut self, var: BVar) {
        self.free_vars.insert(var);
        self.free_vars.insert(var * -1);
        let lit_var: &mut Literal = self.lit_val.get_mut(&(var.abs() as Atom)).unwrap();
        lit_var.is_free = true;
    }
    
    fn derive_and_add_conflict_clause(&mut self) {
        if let Some(last_assignment) = self.history.last() {
            let conflict_var = last_assignment.var;
            let conflict_clause = vec![-conflict_var]; //  simple conflict clause containing only the negated last literal

            // Create a new Clause instance
            let new_clause = Clause {
                watched_lhs: conflict_clause[0],
                watched_rhs: conflict_clause[0], // if the clause contains only one literal, this is used as both observed literals
                vars: conflict_clause,
            };

            // Add the new clause to the clause database
            let new_clause_id = self.clauses.len();
            self.clauses.insert(new_clause_id, new_clause);
        }
    }
}

#[test]
fn set_var_1_true() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, true, 1);
    assert!(!c);
    assert_eq!(cdcl.free_vars.len(), 4);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, false);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().val, true);
    assert_eq!(cdcl.unit_queue.len(), 1);
    assert_eq!(cdcl.clauses.get(&2).unwrap().watched_lhs, -2)
}

#[test]
fn set_var_neg_1_false() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, false, -1);
    assert!(!c);
    assert_eq!(cdcl.free_vars.len(), 4);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, false);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().val, true);
    assert_eq!(cdcl.unit_queue.len(), 1);
    assert_eq!(cdcl.clauses.get(&2).unwrap().watched_lhs, -2)
}

#[test]
fn set_var_neg_1_true() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, true, -1);
    assert!(!c);
    assert_eq!(cdcl.free_vars.len(), 4);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, false);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().val, false);
    assert_eq!(cdcl.unit_queue.len(), 0);
    assert_eq!(cdcl.clauses.get(&0).unwrap().watched_lhs, -2)
}

#[test]
fn set_var_1_false() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, true, -1);
    assert!(!c);
    assert_eq!(cdcl.free_vars.len(), 4);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, false);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().val, false);
    assert_eq!(cdcl.unit_queue.len(), 0);
    assert_eq!(cdcl.clauses.get(&0).unwrap().watched_lhs, -2)
}

#[test]
fn conflict_2() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    );
    cdcl.set_var(false, false, -1);
    let c = cdcl.set_var(false, false, 2);
    assert!(c)
}

#[test]
fn conflict_3() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    );
    cdcl.set_var(false, false, -1);
    let c_1 = cdcl.set_var(false, false, -2);
    let c_2 = cdcl.set_var(false, false, -3);
    assert!(!c_1);
    assert!(c_2);
}

#[test]
fn unit_prop() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    );
    cdcl.set_var(false, false, -1);
    assert_eq!(cdcl.clauses.get(&2).unwrap().watched_lhs, -2);
    cdcl.set_var(false, false, -2);
    assert_eq!(cdcl.unit_queue.len(), 2);
    cdcl.unit_prop();
    assert_eq!(cdcl.lit_val.get(&3).unwrap().is_free, false);
    assert_eq!(cdcl.lit_val.get(&3).unwrap().val, false);
}

#[test]
fn should_solve_sat_small() {
    let res = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        true,
    )
    .solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_solve_sat() {
    let res = CDCL::new(
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
        8,
        35,
        true,
    )
    .solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_solve_unsat_small() {
    let res = CDCL::new(
        vec![
            vec![1, -2, 3],
            vec![-1, 2],
            vec![-1, -2, -3],
            vec![1],
            vec![2],
            vec![3],
        ],
        3,
        6,
        true,
    )
    .solve();
    if let DIMACSOutput::Sat(_) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}

#[test]
fn should_solve_unsat() {
    let res = CDCL::new(
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
        12,
        68,
        true,
    )
    .solve();
    if let DIMACSOutput::Sat(_) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}

#[test]
fn should_parse_and_solve_sat() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, true).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_parse_and_solve_unsat() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/unsat/aim-50-1_6-no-1.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, true).solve();
    if let DIMACSOutput::Sat(_) = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_elim_pure_lit() {
    let res = CDCL::new(
        vec![vec![1, -2, 3], vec![1, 2], vec![1, -2, -3]],
        3,
        6,
        true,
    )
    .solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn bug_jan_2nd_should_be_sat() {
    let (mut input, mut v_c, mut c_c) =
        crate::parse::parse("./src/inputs/sat/ssa7552-038.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, true).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }

    (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/uf50-06.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, true).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn test_derive_and_add_conflict_clause() {
    let mut cdcl = CDCL::new(vec![vec![1, -2], vec![-1, 2]], 2, 2, false);
    
    // Set an assignment that would lead to a conflict
    cdcl.history.push(Assignment { var: 1, val: true, forced: false });
    
    // Simulate the unit propagation that recognizes a conflict
    cdcl.derive_and_add_conflict_clause();

    // Check whether a new conflict clause has been added
    assert_eq!(cdcl.clauses.len(), 3); // Expect that there are now 3 clauses in the database
    let new_clause = cdcl.clauses.get(&2).unwrap(); // The new conflict clause should have the ID 2
    assert_eq!(new_clause.vars, vec![-1]); // The new conflict clause should contain the negated literal
}
