//use flame;
use std::{
    collections::{vec_deque, HashMap, HashSet, VecDeque},
    fmt::format,
    vec,
};

use crate::{
    dpll::DIMACSOutput,
    implication_graph::{ImplicationGraph, ImplicationGraphNode},
}; // Add this line to import the `cli` module from the crate root

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

#[derive(Clone)]
struct Assignment {
    var: BVar,
    val: bool,
    forced: bool,
    decision_level: u32,
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
    implication_graph: ImplicationGraph,
    lit_val: HashMap<Atom, Literal>,
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
            implication_graph: ImplicationGraph::new(),
            lit_val: HashMap::with_capacity(lit_count),
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
                let unsat = match self.backtrack_non_chron() {
                    Ok(b) => b,
                    Err(e) => panic!("{}", e),
                };
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
            let decision_level = self.branch_depth;
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
            self.implication_graph.insert_leaf(var, self.branch_depth)
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
            let mut clause = match self.clauses.get_mut(&c_idx) {
                Some(c) => c,
                None => self.clause_db.get_mut(c_idx).unwrap(),
            };
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
                    self.implication_graph.insert_conflict_node(
                        *c_idx,
                        clause.vars.clone(),
                        self.branch_depth,
                    )
                }
                1 => {
                    // * if only one is found
                    self.unit_queue
                        .push_front(**new_watched_cands.iter().next().unwrap());
                    self.implication_graph.insert_edge(
                        clause.vars.clone(),
                        *new_watched_cands[0],
                        val,
                        *c_idx,
                        self.branch_depth,
                    );
                }
                _ => (),
            };
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
        self.implication_graph.0.remove(&var.abs());
    }

    // Function for deriving and adding a conflict clause
    fn backtrack_non_chron(&mut self) -> Result<bool, String> {
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
                self.insert_clause(clause_id, conflict_clause.clone());

                // Perform non-chronological backtracking
                Ok(self.non_chronological_backtrack(backtrack_level, conflict_clause))
            }
            Err(e) => {
                // Handle the error case
                Err(e)
            }
        }
    }
    // 1-UIP Cut
    fn analyze_conflict(&mut self) -> Result<(Vec<BVar>, u32), String> {
        // TODO use iter instead of clone()
        let mut stack = self.history.clone();
        let mut seen = HashSet::new(); // Set to note already visited nodes
        let mut current_node: Option<&ImplicationGraphNode> = None;
        let mut current_vars = self
            .get_clause(self.implication_graph.get_conflict_node()?.reason.unwrap())?
            .vars
            .clone();
        let literals_of_max_branch_depth = self.literals_conflict_depth();
        while !is_asserting(&current_vars, &literals_of_max_branch_depth) {
            if let Some(next) = stack.pop() {
                current_node = self.implication_graph.0.get(&next.var);
                if let None = current_node {
                    continue;
                }
                if !seen.insert(current_node) {
                    continue;
                }
                if let None = current_node.unwrap().reason {
                    continue;
                }
                current_vars = resolution(
                    &current_vars,
                    &self.get_clause(current_node.unwrap().reason.unwrap())?.vars,
                );
            } else {
                return Err(
                    "Backtracked until the end without finding an asserting clause.".into(),
                );
            }
        }
        let backtrack_depth = current_vars
            .iter()
            .filter_map(|v| {
                self.implication_graph
                    .0
                    .get(&v.abs())
                    .and_then(|node: &ImplicationGraphNode| Some(node.decision_level))
            })
            .min()
            .unwrap();
        Ok((current_vars.into_iter().collect(), backtrack_depth))
    }

    // This could be tracked during construction of impl graph
    fn literals_conflict_depth(&self) -> Vec<i32> {
        let conflict = self.implication_graph.get_conflict_node().unwrap();
        let mut stack = vec![conflict.literal]; // Stack for DFS, starting with `lit`
        let mut seen = HashSet::new(); // Set to note already visited nodes
        let mut result = HashSet::new();
        while let Some(current_lit) = stack.pop() {
            if !seen.insert(current_lit) {
                continue;
            }
            if let Some(node) = self.implication_graph.0.get(&current_lit) {
                if node.decision_level != conflict.decision_level {
                    continue;
                }
                // Add all predecessors of the current node to the stack for the further search
                for &pred in &node.predecessors {
                    result.insert(node.literal);
                    stack.push(pred);
                }
            }
        }
        Vec::from_iter(result)
    }

    fn non_chronological_backtrack(
        &mut self,
        assertion_level: u32,
        conflict_clause: Vec<i32>,
    ) -> bool {
        // * undo all assignments of branching level > d
        self.implication_graph.clear_conflict();
        // Reset assignments that were made after the assertion level
        while let Some(assignment) = self.history.last() {
            if assignment.decision_level <= assertion_level {
                break;
            }

            // Undo assignment
            let var = assignment.var;
            self.unset_var(var);
            // Remove last assignment from the history
            self.history.pop();
        }
        if self.history.is_empty() {
            return true;
        }

        // * set branching depth to d
        self.branch_depth = assertion_level;

        // Empty the unit queue, as all subsequent units are invalid
        self.unit_queue.clear();

        // TODO: not so nice to filter the clause here. I'm sure this literal can be retrieved during conflict analysis
        let &unit_lit = conflict_clause
            .iter()
            .filter(|l| !self.lit_val[&(l.abs() as Atom)].is_free)
            .next()
            .unwrap();
        self.unit_queue.push_front(unit_lit);
        false
    }

    fn insert_clause(&mut self, clause_id: usize, conflict_clause: Vec<i32>) {
        self.clause_db.insert(
            clause_id,
            Clause {
                watched_lhs: conflict_clause[0],
                watched_rhs: conflict_clause
                    .get(1)
                    .cloned()
                    .unwrap_or(conflict_clause[0]),
                vars: conflict_clause.clone(),
            },
        );
        conflict_clause.iter().for_each(|lit| {
            self.pos_watched_occ
                .entry(*lit)
                .and_modify(|clause| clause.push(clause_id))
                .or_insert(vec![clause_id]);
        });
    }

    fn get_clause(&self, c_idx: CIdx) -> Result<&Clause, String> {
        match self.clauses.get(&c_idx).or(self.clause_db.get(&c_idx)) {
            Some(c) => Ok(c),
            None => Err(format!("Requested clause with index {} not found.", c_idx)),
        }
    }
}

fn is_asserting(clause: &Vec<i32>, literals_of_max_branch_depth: &Vec<i32>) -> bool {
    HashSet::<i32>::from_iter(clause.iter().map(|l| l.abs()))
        .intersection(&HashSet::<i32>::from_iter(
            literals_of_max_branch_depth.iter().map(|l| l.abs()),
        ))
        .count()
        == 1
}

// TODO: Is it safe to assume that resolution can happen? Double check in proof from lecture
fn resolution(clause1: &Vec<i32>, clause2: &Vec<i32>) -> Vec<i32> {
    let mut hs_1: HashSet<i32> = HashSet::from_iter(clause1.iter().cloned());
    let mut hs_2: HashSet<i32> = HashSet::from_iter(clause2.iter().cloned());
    if hs_1 == hs_2 {
        return clause1.clone();
    }
    for c_1 in clause1.iter() {
        if clause2.contains(&-c_1) {
            hs_1.remove(c_1);
            hs_2.remove(&-c_1);
        }
    }
    Vec::from_iter(hs_1.union(&hs_2).cloned())
}

#[test]
fn should_derive_1_UIP_from_lecture() {
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
    cdcl.history_enabled = true;
    cdcl.set_var(true, false, false, 9);
    cdcl.branch_depth += 1;
    cdcl.unit_prop();
    cdcl.set_var(true, false, false, 10);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.set_var(true, false, true, 12);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.set_var(true, false, true, 1);
    let conflict = cdcl.unit_prop();
    cdcl.branch_depth += 1;
    assert!(conflict);
    assert!(cdcl.implication_graph.0[&0].predecessors.contains(&5));
    assert!(cdcl.implication_graph.0[&0].predecessors.contains(&6));
    assert!(cdcl.implication_graph.0[&5].predecessors.contains(&10));
    assert!(cdcl.implication_graph.0[&5].predecessors.contains(&4));
    assert!(cdcl.implication_graph.0[&6].predecessors.contains(&4));
    assert!(cdcl.implication_graph.0[&6].predecessors.contains(&11));
    assert!(cdcl.implication_graph.0[&4].predecessors.contains(&2));
    assert!(cdcl.implication_graph.0[&4].predecessors.contains(&3));
    assert!(cdcl.implication_graph.0[&2].predecessors.contains(&1));
    assert!(cdcl.implication_graph.0[&3].predecessors.contains(&1));
    assert!(cdcl.implication_graph.0[&3].predecessors.contains(&9));
    assert!(cdcl.implication_graph.0[&11].predecessors.contains(&10));
    assert!(cdcl.implication_graph.0[&13].predecessors.contains(&12));
    assert_eq!(cdcl.literals_conflict_depth().len(), 6); // Inclduing 0 for the conflict node
    assert_eq!(cdcl.implication_graph.0[&9].decision_level, 0);
    assert_eq!(cdcl.implication_graph.0[&6].decision_level, 3);
    assert_eq!(cdcl.implication_graph.0[&6].reason, Some(4));
    let conflict = cdcl.analyze_conflict();
    assert_eq!(conflict.clone().unwrap().1, 1);
    assert!(conflict.clone().unwrap().0.contains(&-4));
    assert!(conflict.clone().unwrap().0.contains(&10));
    assert!(conflict.unwrap().0.contains(&11));
}

#[test]
fn should_set_var_1_true_watched_literals() {
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
fn should_set_var_neg_1_false_watched_literals() {
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
fn should_set_var_neg_1_true_watched_literals() {
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
fn should_set_var_1_false_watched_literals() {
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
fn should_detect_conflict_watched_literals() {
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
fn should_detect_conflict_watched_literals_2() {
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
fn should_unit_prop_watched_literals() {
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
    let mut cdcl = CDCL::new(vec![vec![1, -2], vec![-1, 2], vec![-2, -1]], 3, 3, false);

    // Initialize the values and status for the variables in the Literal Values HashMap

    cdcl.history_enabled = true;
    cdcl.branch_depth += 1;
    cdcl.set_var(true, false, true, 1);
    cdcl.unit_prop();

    let result = cdcl.backtrack_non_chron();
    assert!(
        result.is_ok(),
        "Conflict clause should be successfully derived."
    );

    assert_eq!(
        cdcl.clauses.len() + cdcl.clause_db.len(),
        4,
        "A new conflict clause should have been added."
    );
}

#[test]
fn test_analyze_conflict() {
    let mut cdcl = CDCL::new(vec![vec![-1, -2], vec![-1, 2]], 2, 2, false);
    cdcl.history_enabled = true;
    cdcl.branch_depth += 1;
    cdcl.set_var(true, false, true, 1);
    cdcl.unit_prop();

    // Simulate a conflict
    let result = cdcl.analyze_conflict();
    assert!(result.is_ok());
    let (conflict_clause, backtrack_level) = result.unwrap();
    assert!(!conflict_clause.is_empty());
    assert_eq!(backtrack_level, 1);
}

#[test]
fn test_current_decision_level() {
    let mut cdcl = CDCL::new(vec![vec![1, 2], vec![2, 3], vec![3, 1]], 0, 0, false);
    assert_eq!(cdcl.branch_depth, 0); // No assignments, decision level should be 0
    cdcl.solve();
    assert_eq!(cdcl.branch_depth, 2); // Assign 1 -> unit prop 2 -> Assign 2 => decision_level of 2
}
