//use flame;
use std::collections::{HashMap, HashSet, VecDeque};

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
    decision_level: u32,
}

#[derive(Debug, Clone)]
struct ImplicationGraphNode {
    literal: BVar,
    decision_level: u32,
    reason: Option<usize>,
    predecessors: Vec<BVar>,
}

pub struct CDCL {
    branch_depth: u32,
    // Using HashMaps due to better get(i) / append complexity, see https://doc.rust-lang.org/std/collections/#sequences
    clauses: HashMap<usize, Clause>,
    clause_db: HashMap<usize, Clause>,
    free_vars: HashSet<i32>,
    history: Vec<Assignment>,
    // Memory allocation in the history is a bottle neck thus the initial unit prop and pure lit elim don't need to expand it
    history_enabled: bool,
    implication_graph: HashMap<BVar, ImplicationGraphNode>,
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
        let mx_clauses = 42; //TODO: use useful value here according to deletion strat
        let mut cdcl = CDCL {
            branch_depth: 0,
            clauses: HashMap::with_capacity(clause_count),
            clause_db: HashMap::with_capacity(mx_clauses),
            free_vars: HashSet::with_capacity(lit_count),
            history: Vec::new(),
            history_enabled: false,
            implication_graph: HashMap::new(),
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
            self.branch_depth = self.branch_depth + 1;
            self.set_var(true, forced, val, *var);

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
        self.set_var(true, true, !val, var);
        false
    }

    fn unit_prop(&mut self) -> bool {
        while !self.unit_queue.is_empty() {
            let forced_lit = self.unit_queue.pop_back().unwrap();
            // Try to set the literal. If this results in a conflict, `unsat` is true.
            let unsat = self.set_var(false, true, true, forced_lit);
            if unsat {
                return true; // Signalize that a conflict has occurred
            }
        }
        false // No conflict found
    }

    fn set_var(&mut self, branched: bool, forced: bool, val: bool, var: BVar) -> bool {
        let mut conflict = false;
        if self.history_enabled {
            let decision_level = self.history.len() as u32;
            self.history.push(Assignment {
                var,
                val,
                forced,
                decision_level,
            });
        }
        let lit = self.lit_val.get_mut(&(var.abs() as Atom)).unwrap();
        // For conflict graph
        if branched {
            self.implication_graph.insert(
                var,
                ImplicationGraphNode {
                    literal: var,
                    decision_level: self.branch_depth,
                    reason: None,
                    predecessors: vec![],
                },
            );
        };

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
                    // Add conflict clause to the database
                    self.implication_graph.insert(
                        0,
                        ImplicationGraphNode {
                            literal: 0,
                            decision_level: self.branch_depth,
                            reason: Some(*c_idx),
                            predecessors: clause.vars.clone(),
                        },
                    );
                }
                1 => {
                    // * if only one is found
                    self.unit_queue
                        .push_front(**new_watched_cands.iter().next().unwrap());
                    let vars = clause
                        .vars
                        .iter()
                        .filter(|&v| {
                            self.implication_graph.get(v).is_some()
                                || self.implication_graph.get(&-v).is_some()
                        })
                        .map(|v| *v)
                        .collect::<Vec<i32>>();
                    self.implication_graph
                        .entry(*new_watched_cands[0])
                        .and_modify(|node| node.predecessors.push(var))
                        .or_insert(ImplicationGraphNode {
                            literal: *new_watched_cands[0],
                            decision_level: self.branch_depth,
                            reason: Some(*c_idx),
                            predecessors: vars,
                        });
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

    // Function for deriving and adding a conflict clause
    fn derive_and_add_conflict_clause(&mut self) -> Result<(), String> {
        match self.analyze_conflict() {
            Ok((conflict_clause, backtrack_level)) => {
                if conflict_clause.is_empty() {
                    let error_message =
                        "No conflict clause found. Check the implementation of analyze_conflict."
                            .to_string();
                    println!("{}", &error_message);
                    return Err(error_message);
                }
    
                // Add conflict clause to the database
                let clause_id = self.clauses.len() + self.clause_db.len();
                self.clause_db.insert(clause_id, Clause {
                    watched_lhs: conflict_clause[0],
                    watched_rhs: conflict_clause.get(1).cloned().unwrap_or(conflict_clause[0]),
                    vars: conflict_clause,
                });
    
                // Perform non-chronological backtracking
                self.non_chronological_backtrack(backtrack_level);

                Ok(())
            }
            Err(e) => {
                // Handle the error case
                Err(e)
            }
        }
    }

    // Function for conflict analysis
    fn analyze_conflict(&mut self) -> Result<(Vec<BVar>, u32), String> {
        let conflict_var = self.history.last().ok_or("No history found for conflict")?.var;
        let mut seen = HashSet::new();
        let mut stack = vec![conflict_var];
        let mut involved_literals = Vec::new();
        let mut backtrack_level = 0;
        let current_decision_level = self.branch_depth;

        let mut last_common_ancestor = None; // Variable for saving the 1-UIP

        while let Some(var) = stack.pop() {
            if seen.contains(&var) {
                continue;
            }
            seen.insert(var);

            if let Some(node) = self.implication_graph.get(&var) {
                if node.reason.is_none() && var != conflict_var {
                    continue;
                }

                involved_literals.push(var); // Collecting the literals involved

                //  for identifying the 1-UIP
                if last_common_ancestor.is_none()
                    || self.implication_graph[&var].decision_level < current_decision_level
                {
                    last_common_ancestor = Some(var);
                }

                for &pred in &node.predecessors {
                    if self.implication_graph[&pred].decision_level < current_decision_level {
                        backtrack_level =
                            backtrack_level.max(self.implication_graph[&pred].decision_level);
                    } else {
                        stack.push(pred);
                    }
                }
            }
        }

        // Use last_common_ancestor and the literals involved to create the conflict clause
        let conflict_clause = todo!();

        Ok((conflict_clause, backtrack_level))
    }

    fn derive_conflict_clause(&self, involved_literals: &Vec<BVar>, last_common_ancestor: Option<BVar>) -> Result<Vec<BVar>, String> {
        if let Some(ancestor) = last_common_ancestor {
            let conflict_clause: Vec<BVar> = involved_literals.iter().filter(|&&lit| lit == ancestor || self.is_descendant_of(lit, ancestor)).map(|&lit| -lit).collect();
            if conflict_clause.is_empty() {
                return Err("No literals found for conflict clause".to_string());
            }
            Ok(conflict_clause)
        } else {
            Err("1-UIP not found".to_string())
        }
    }
    
    fn is_descendant_of(&self, lit: BVar, ancestor: BVar) -> bool {
        let mut stack = vec![lit]; // Stack for DFS, starting with `lit`
        let mut seen = HashSet::new(); // Set to note already visited nodes
    
        while let Some(current_lit) = stack.pop() {
            if current_lit == ancestor {
                return true; // `ancestor` found, `lit` is a descendant
            }
    
            // Avoid visiting the same node more than once
            if !seen.insert(current_lit) {
                continue;
            }
    
            // Access the current node in the implication graph
            if let Some(node) = self.implication_graph.get(&current_lit) {
                // Add all predecessors of the current node to the stack for the further search
                for &pred in &node.predecessors {
                    stack.push(pred);
                }
            }
        }  
        false // `ancestor` was not found, `lit` is not a descendant
    }
    

    fn current_decision_level(&self) -> u32 {
        self.branch_depth
    }

    fn non_chronological_backtrack(&mut self, assertion_level: u32) {
        // Reset assignments that were made after the assertion level
        while let Some(assignment) = self.history.last() {
            if assignment.decision_level < assertion_level {
                break;
            }

            // Undo assignment
            let var = assignment.var;
            let lit = self.lit_val.get_mut(&(var.abs() as Atom)).unwrap();
            lit.is_free = true; // Mark literal as free
            self.free_vars.insert(var); // Insert back into the set of free variables
            self.free_vars.insert(-var); // Also the negated literal

            // Remove last assignment from the history
            self.history.pop();
        }

        // Empty the unit queue, as all subsequent units are invalid
        self.unit_queue.clear();

        // Update the implication graph to reflect the undone assignments
        self.update_implication_graph(assertion_level);
    }

    fn update_implication_graph(&mut self, valid_level: u32) {
        self.implication_graph
            .retain(|_, node| node.decision_level <= valid_level);
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
    let c = cdcl.set_var(true, false, true, 1);
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
    let c = cdcl.set_var(true, false, false, -1);
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
    let c = cdcl.set_var(true, false, true, -1);
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
    let c = cdcl.set_var(true, false, true, -1);
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
    cdcl.set_var(true, false, false, -1);
    let c = cdcl.set_var(true, false, false, 2);
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
    cdcl.set_var(true, false, false, -1);
    let c_1 = cdcl.set_var(true, false, false, -2);
    let c_2 = cdcl.set_var(true, false, false, -3);
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
    cdcl.set_var(true, false, false, -1);
    assert_eq!(cdcl.clauses.get(&2).unwrap().watched_lhs, -2);
    cdcl.set_var(true, false, false, -2);
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
    let mut cdcl = CDCL::new(vec![vec![1, -2], vec![-1, 2], vec![-2, 3]], 3, 3, false);

    // Initialize the values and status for the variables in the Literal Values HashMap
    cdcl.lit_val.insert(
        1,
        Literal {
            val: true,
            is_free: false,
        },
    );
    cdcl.lit_val.insert(
        2,
        Literal {
            val: false,
            is_free: false,
        },
    );
    cdcl.lit_val.insert(
        3,
        Literal {
            val: false,
            is_free: true,
        },
    );

    // Add assignments to the history
    cdcl.history.push(Assignment {
        var: 1,
        val: true,
        forced: false,
        decision_level: 1,
    });
    cdcl.history.push(Assignment {
        var: -2,
        val: true,
        forced: true,
        decision_level: 2,
    });

    // Make sure that all required nodes are present in the implication_graph
    cdcl.implication_graph.insert(
        1,
        ImplicationGraphNode {
            literal: 1,
            decision_level: 1,
            reason: None, // No reason for decision knots
            predecessors: vec![],
        },
    );
    cdcl.implication_graph.insert(
        -2,
        ImplicationGraphNode {
            literal: -2,
            decision_level: 2,
            reason: Some(2), // Refers to an existing clause
            predecessors: vec![1],
        },
    );

    // Make sure that the clause to which the reason refers exists
    cdcl.clauses.insert(
        2,
        Clause {
            watched_lhs: -2,
            watched_rhs: 3,
            vars: vec![-2, 3],
        },
    );

    let result = cdcl.derive_and_add_conflict_clause();
    assert!(
        result.is_ok(),
        "Conflict clause should be successfully derived."
    );

    assert_eq!(
        cdcl.clauses.len(),
        4,
        "A new conflict clause should have been added."
    );
}

//TODO -> fails
#[test]
fn test_analyze_conflict() {
    let mut cdcl = CDCL::new(vec![vec![1, -2], vec![-1, 2]], 2, 2, false);
    cdcl.history.push(Assignment {
        var: 1,
        val: true,
        forced: false,
        decision_level: 1,
    });
    cdcl.history.push(Assignment {
        var: -2,
        val: true,
        forced: true,
        decision_level: 1,
    });

    // Simulate a conflict
    let result = cdcl.analyze_conflict();
    assert!(result.is_ok());
    let (conflict_clause, backtrack_level) = result.unwrap();
    assert!(!conflict_clause.is_empty());
    assert_eq!(backtrack_level, 1);
}

#[test]
fn test_lecture() {
    let mut cdcl = CDCL::new(
        vec![
            vec![-1, 2],
            vec![-1, 3, 9],
            vec![-2, -3, 4],
            vec![-4, 5, 10],
            vec![-4, 6, 11],
            vec![-5, -6],
            vec![1, 7, -12],
            vec![1, 8],
            vec![-7, -8, -13],
            vec![10, -11],
            vec![-12, 13],
        ],
        13,
        11,
        false,
    );
    cdcl.set_var(true, false, false, 9);
    cdcl.branch_depth += 1;
    cdcl.unit_prop();
    cdcl.set_var(true, false, false, 10);
    cdcl.branch_depth += 1;
    cdcl.unit_prop();
    cdcl.set_var(true, false, true, 12);
    cdcl.branch_depth += 1;
    cdcl.unit_prop();
    cdcl.set_var(true, false, true, 1);
    cdcl.branch_depth += 1;
    let conflict = cdcl.unit_prop();
    assert!(conflict);
    println!("{:?}", cdcl.implication_graph);
}

#[test]
fn test_derive_conflict_clause() {
    let mut cdcl = CDCL::new(vec![vec![1, -2], vec![-1, 2]], 2, 2, false);
    // Assuming the literals involved are 1 and -2, and 1 is the 1-UIP
    let involved_literals = vec![1, -2];
    let last_common_ancestor = Some(1);

    let result = cdcl.derive_conflict_clause(&involved_literals, last_common_ancestor);
    assert!(result.is_ok());

    let conflict_clause = result.unwrap();
    // Expect the conflict-indicating clause to contain -1, since 1 is the 1-UIP and is negated
    assert!(conflict_clause.contains(&-1));
}

#[test]
fn test_current_decision_level() {
    let mut cdcl = CDCL::new(vec![], 0, 0, false);
    assert_eq!(cdcl.current_decision_level(), 0); // No assignments, decision level should be 0

    cdcl.history.push(Assignment {
        var: 1,
        val: true,
        forced: false,
        decision_level: 1,
    });
    assert_eq!(cdcl.current_decision_level(), 1); // An assignment at level 1

    cdcl.history.push(Assignment {
        var: 2,
        val: false,
        forced: true,
        decision_level: 2,
    });
    assert_eq!(cdcl.current_decision_level(), 2); // Another assignment at level 2
}

#[test]
fn test_is_descendant_of() {
    let mut cdcl = CDCL::new(vec![], 0, 0, false);
    cdcl.implication_graph.insert(
        1,
        ImplicationGraphNode {
            literal: 1,
            decision_level: 1,
            reason: None,
            predecessors: vec![],
        },
    );
    cdcl.implication_graph.insert(
        -2,
        ImplicationGraphNode {
            literal: -2,
            decision_level: 2,
            reason: Some(1),
            predecessors: vec![1],
        },
    );

    assert!(cdcl.is_descendant_of(-2, 1));
    assert!(!cdcl.is_descendant_of(1, -2)); // 1 is not a descendant of -2
}

//TODO -> fails
#[test]
fn test_non_chronological_backtrack() {
    let mut cdcl = CDCL::new(vec![vec![1, -2], vec![-1, 2]], 2, 2, false);
    cdcl.history.push(Assignment {
        var: 1,
        val: true,
        forced: false,
        decision_level: 1,
    });
    cdcl.history.push(Assignment {
        var: 2,
        val: true,
        forced: true,
        decision_level: 2,
    });

    cdcl.non_chronological_backtrack(1);

    assert_eq!(cdcl.current_decision_level(), 1);
    assert!(cdcl.history.iter().all(|a| a.decision_level <= 1));
}
