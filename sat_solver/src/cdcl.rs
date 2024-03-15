//use flame;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::Write,
    vec,
};

use crate::{
    cli::Heuristic,
    dpll::DIMACSOutput,
    heuristics::{self, berkmin, BerkMinData, VsidsData},
}; // Add this line to import the `cli` module from the crate root

type Atom = u16;
type BVar = i32;
type CIdx = usize;

// Using structs instead of tuples for readability
// Same mem layout as for tuples:
// - https://doc.rust-lang.org/reference/type-layout.html#tuple-layout
// - https://doc.rust-lang.org/reference/types/struct.html#structtype
#[derive(Clone)]
pub(crate) struct Clause {
    watched_lhs: BVar,
    watched_rhs: BVar,
    vars: Vec<BVar>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Literal {
    val: bool,
    is_free: bool,
    reason: Option<CIdx>,
    decision_level: u32,
}

#[derive(Clone)]
struct Assignment {
    var: BVar,
    val: bool,
    forced: bool,
}

pub struct CDCL {
    branch_depth: u32,
    // Using HashMaps due to better get(i) / append complexity, see https://doc.rust-lang.org/std/collections/#sequences
    clauses: HashMap<usize, Clause>,
    clause_db: HashMap<usize, Clause>,
    free_vars: HashSet<i32>,
    heuristic: Heuristic,
    history: Vec<Assignment>,
    // Memory allocation in the history is a bottle neck thus the initial unit prop and pure lit elim don't need to expand it
    history_enabled: bool,
    lit_val: HashMap<Atom, Literal>,
    // Keys don't contain the sign as abs is cheaper than calculating the sign every time
    pos_watched_occ: HashMap<BVar, Vec<CIdx>>,
    // Using VecDaque for better push_front complexity
    unit_queue: VecDeque<(BVar, Option<CIdx>)>,
    literals_at_current_depth: HashSet<u16>,

    vsids_data: VsidsData,
    berkmin_data: BerkMinData,
}

impl CDCL {
    pub fn new(
        input: Vec<Vec<i32>>,
        lit_count: usize,
        clause_count: usize,
        heuristic: Heuristic,
    ) -> Self {
        let mx_clauses = 42; //TODO: use useful value here according to deletion strat
        let mut cdcl = CDCL {
            branch_depth: 0,
            clauses: HashMap::with_capacity(clause_count),
            clause_db: HashMap::with_capacity(mx_clauses),
            free_vars: HashSet::with_capacity(lit_count),
            heuristic,
            history: Vec::new(),
            history_enabled: false,
            lit_val: HashMap::with_capacity(lit_count),
            pos_watched_occ: HashMap::with_capacity(clause_count * 2),
            unit_queue: VecDeque::new(),
            literals_at_current_depth: HashSet::new(),
            vsids_data: VsidsData::new(),
            berkmin_data: BerkMinData::new(),
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
            if vars.len() == 1 && !cdcl.unit_queue.contains(&(*fist_var, Some(c))) {
                cdcl.unit_queue.push_front((*fist_var, Some(c)));
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
                    as_atom(*literal),
                    Literal {
                        val: false,
                        is_free: true,
                        reason: None,
                        decision_level: cdcl.branch_depth,
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
        if let Some(res) = self.is_sat() {
            return res;
        }

        loop {
            // * choose literal var
            // TODO is currently very inefficient and needs to be replaced by picking conflict literals.
            let (var, val) = self.pick_branching_literal();
            // * set value var
            self.branch_depth = self.branch_depth + 1;
            self.literals_at_current_depth.clear();
            println!("Branching d {}:", self.branch_depth);
            self.set_var(false, val, var, None);

            // * unit propagation
            loop {
                let conflict = self.unit_prop();
                if !conflict {
                    break;
                };
                // * if conflict detected
                let (conflict_clause, backtrack_level) = self.analyze_conflict().unwrap();

                if self.branch_depth == 0 {
                    return DIMACSOutput::Unsat;
                }

                // Add conflict clause to the database
                let clause_id = self.clauses.len() + self.clause_db.len();
                self.insert_clause(clause_id, conflict_clause.clone());
                    
                // Perform non-chronological backtracking
                self.non_chronological_backtrack(backtrack_level, clause_id, conflict_clause);
            }

            // * if all clauses satisfied
            if let Some(res) = self.is_sat() {
                return res;
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
        self.set_var(true, !val, var, None);
        false
    }

    fn unit_prop(&mut self) -> bool {
        println!("Unitprop:");
        let mut unsat = !self.unit_queue.is_empty();
        while !self.unit_queue.is_empty() {
            let forced_lit = self.unit_queue.pop_back().unwrap();
            // Try to set the literal. If this results in a conflict, `unsat` is true.
            if !self.lit_val[&as_atom(forced_lit.0)].is_free {
                println!("Warning attemted to set {} again.", forced_lit.0);
                continue;
            }
            unsat = self.set_var(true, true, forced_lit.0, forced_lit.1);
            if unsat {
                break; // Signalize that a conflict has occurred
            }
        }
        unsat // No conflict found
    }

    fn set_var(&mut self, forced: bool, val: bool, var: BVar, reason: Option<CIdx>) -> bool {
        println!("\tset {} to {}", var, val);
        let mut conflict = false;
        if self.history_enabled {
            self.history.push(Assignment { var, val, forced });
        }
        let lit = self.lit_val.get_mut(&as_atom(var)).unwrap();
        // For conflict graph
        lit.is_free = false;
        lit.reason = reason;
        lit.decision_level = self.branch_depth;
        self.literals_at_current_depth.insert(as_atom(var));
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
            let clause = match self.clauses.get_mut(&c_idx) {
                Some(c) => c,
                None => self.clause_db.get_mut(c_idx).unwrap(),
            };
            // * if satisfying literal encountert
            if clause.vars.iter().any(|&v| {
                self.lit_val
                    .get(&as_atom(v))
                    .is_some_and(|a: &Literal| !a.is_free && (a.val == v.is_positive()))
            }) {
                // Because this clause is already sat
                continue;
            }
            let new_watched_cands = clause
                .vars
                .iter()
                .filter(|&v| self.lit_val.get(&as_atom(*v)).unwrap().is_free)
                .collect::<Vec<&BVar>>();
            match new_watched_cands.len() {
                // * if no unassigned literal found
                0 => {
                    conflict = true;
                    // Pseudo literal representing the root of the conflict graph
                    self.lit_val.insert(
                        0,
                        Literal {
                            val: false,
                            is_free: false,
                            reason: Some(*c_idx),
                            decision_level: self.branch_depth,
                        },
                    );
                }
                1 => {
                    // * if only one is found
                    self.unit_queue
                        .push_front((*new_watched_cands[0], Some(*c_idx)));
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
        println!("\tunset {}", var);
        self.free_vars.insert(var);
        self.free_vars.insert(var * -1);
        let lit_var: &mut Literal = self.lit_val.get_mut(&as_atom(var)).unwrap();
        lit_var.is_free = true;
        lit_var.reason = None;
        lit_var.decision_level = 0;
        self.literals_at_current_depth.remove(&as_atom(var));
    }

    // 1-UIP Cut
    fn analyze_conflict(&self) -> Result<(Clause, u32), String> {
        // TODO use iter instead of clone()
        let mut stack = self.history.clone();
        let mut new_vars = vec![];
        let mut current_node: &Literal;
        let mut current_vars = &self
            .get_clause(
                self.lit_val[&0]
                    .reason
                    .expect("Conflict has no reason clause"),
            )?
            .vars;
        while !is_asserting(current_vars, &self.literals_at_current_depth) {
            if let Some(next) = stack.pop() {
                current_node = &self.lit_val[&as_atom(next.var)];
                if let None = current_node.clone().reason {
                    continue;
                }
                let reason_clause = &self.get_clause(current_node.reason.unwrap())?.vars;
                // quote lecure: We go backwards on the stack and if we find an assignment that is of a literal (next.var) in our clause then we resolve with the reason clause (current_node) and we continue to do that until we have an asserting clause.
                if current_vars.contains(&next.var) || current_vars.contains(&-next.var) {
                    new_vars = resolution(current_vars, reason_clause)?;
                }
                current_vars = &new_vars;
            } else {
                return Err("Resolved until the end without finding an asserting clause.".into());
            }
        }
        let mut vars = current_vars.to_vec();
        vars.sort_by_key(|&v| self.lit_val[&as_atom(v)].decision_level);
        vars.reverse();
        // Second largest (largest excluding conflict literal) decision level
        let watched_lhs = vars[0];
        let watched_rhs = *vars.get(1).unwrap_or(&watched_lhs);
        let conflict_clause = Clause {
            watched_lhs,
            watched_rhs,
            vars,
        };
        let assetion_level = self.lit_val[&as_atom(watched_rhs)].decision_level;
        Ok((conflict_clause, assetion_level))
    }

    fn non_chronological_backtrack(
        &mut self,
        assertion_level: u32,
        conflict_c_idx: CIdx,
        conflict_clause: Clause,
    ) {
        // * undo all assignments of branching level > d
        // Reset assignments that were made after the assertion level
        while let Some(assignment) = self.history.last() {
            if self.lit_val[&as_atom(assignment.var)].decision_level <= assertion_level {
                break;
            }
            // Undo assignment
            let var = assignment.var;
            self.unset_var(var);
            // Remove last assignment from the history
            self.history.pop();
        }
        // * set branching depth to d
        self.branch_depth = assertion_level;
        // As we're tracking literals_at_current_depth during setting/unsetting vars we
        // need to collect "leftovers" when setting the decision_level to a lover val
        self.lit_val
            .iter()
            .filter(|l| l.1.decision_level == self.branch_depth)
            .for_each(|l| {
                self.literals_at_current_depth.insert(*l.0);
            });
        // Empty the unit queue, as all subsequent units are invalid
        self.unit_queue.clear();
        if !self.lit_val[&as_atom(conflict_clause.watched_lhs)].is_free {
            println!(
                "watched_lhs {} is already set at {} and thus won't be queued.",
                conflict_clause.watched_lhs, assertion_level
            );
            return;
        }
        self.unit_queue
            .push_front((conflict_clause.watched_lhs, Some(conflict_c_idx)));
    }

    fn insert_clause(&mut self, clause_id: usize, conflict_clause: Clause) {
        self.clause_db.insert(clause_id, conflict_clause.clone());
        [conflict_clause.watched_lhs, conflict_clause.watched_rhs]
            .iter()
            .for_each(|lit| {
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

    fn print_graph_as_dot(&self) {
        let nodes = self
            .lit_val
            .iter()
            .map(|n| {
                format!(
                    "{} [label=\"{{{{{}|{}|{}}}|{:?}}}\"];\n",
                    n.0,
                    n.0,
                    n.1.val as i32,
                    n.1.decision_level,
                    match n.1.reason {
                        Some(c) => self.get_clause(c).unwrap().vars.clone(),
                        None => vec![],
                    },
                )
            })
            .collect::<Vec<String>>()
            .concat();
        let edges = self
            .lit_val
            .iter()
            .flat_map(|n| match n.1.reason {
                Some(reason) => self
                    .get_clause(reason)
                    .unwrap()
                    .vars
                    .iter()
                    .filter(|&&e| as_atom(e) != *n.0)
                    .map(move |p| format!("{} -> {};\n", n.0, p.abs()))
                    .collect(),
                None => vec![],
            })
            .collect::<Vec<String>>()
            .concat();
        let graph = format!("digraph G {{ node [shape=record];\n{}\n{}}}", nodes, edges);
        let file_path = "imp_graph.dot";

        // Open the file in write mode, creating it if it doesn't exist
        let mut file = match File::create(file_path) {
            Ok(file) => file,
            Err(why) => panic!("Couldn't create file: {}", why),
        };

        // Write the content to the file
        let _ignored = file.write_all(graph.as_bytes());
    }

    fn is_sat(&self) -> Option<DIMACSOutput> {
        if self.clauses.values().fold(true, |i, c| {
            let lhs = self.lit_val.get(&as_atom(c.watched_lhs)).unwrap();
            let rhs = self.lit_val.get(&as_atom(c.watched_rhs)).unwrap();
            i && !(lhs.is_free && rhs.is_free && self.pos_watched_occ.contains_key(&c.watched_lhs) && self.pos_watched_occ.contains_key(&c.watched_rhs))
        }) {
            let res: Vec<i32> = self
                .lit_val
                .iter()
                .map(|(atom, lit)| match lit.val {
                    true => *atom as BVar,
                    false => -(*atom as BVar),
                })
                .collect();
            return Some(DIMACSOutput::Sat(res));
        }
        None
    }

    // Deletion strategies
    // Implementation of the k-bounded Learning Strategy
    pub fn k_bounded_learning(cdcl: &mut CDCL, k: usize) {
        let mut deleted_clauses = vec![];

        // Check and process learned clauses
        for (clause_id, clause) in &cdcl.clause_db {
            // Check if the width of the clause exceeds the limit
            if clause.vars.len() > k {
                // Check if two literals in the clause are unassigned
                if clause
                    .vars
                    .iter()
                    .filter(|&lit| cdcl.lit_val[&(lit.abs() as Atom)].is_free)
                    .count()
                    >= 2
                {
                    // Delete the clause
                    deleted_clauses.push(*clause_id);
                }
            }
        }

        // Delete identified clauses
        for clause_id in deleted_clauses {
            let clause = cdcl.clause_db.remove(&clause_id).unwrap();
            for lit in &clause.vars {
                if let Some(clauses) = cdcl.pos_watched_occ.get_mut(&lit) {
                    if let Some(index) = clauses.iter().position(|&x| x == clause_id) {
                        clauses.remove(index);
                    }
                }
            }
        }
    }

    // Implementation of the m-size relevance based learning Strategy as a free function
    pub fn m_size_relevance_learning(cdcl: &mut CDCL, m: usize) {
        let mut deleted_clauses = vec![];

        // Check and process learned clauses
        for (clause_id, clause) in &cdcl.clause_db {
            // Check if more than m literals in the clause are unassigned
            if clause
                .vars
                .iter()
                .filter(|&lit| cdcl.lit_val[&(lit.abs() as Atom)].is_free)
                .count()
                > m
            {
                // Delete the clause
                deleted_clauses.push(*clause_id);
            }
        }

        // Delete identified clauses
        for clause_id in deleted_clauses {
            let clause = cdcl.clause_db.remove(&clause_id).unwrap();
            for lit in &clause.vars {
                if let Some(clauses) = cdcl.pos_watched_occ.get_mut(&lit) {
                    if let Some(index) = clauses.iter().position(|&x| x == clause_id) {
                        clauses.remove(index);
                    }
                }
            }
        }
    }

    // Combined strategy: k-bounded learning and m-size relevance based learning
    pub fn combined_learning_strategy(&mut self, k: usize, m: usize) {
        CDCL::k_bounded_learning(self, k);
        CDCL::m_size_relevance_learning(self, m);
    }
    fn pick_branching_literal(&mut self) -> (i32, bool) {
        // TODO: track clauses
        let unsat_clauses = self
            .clause_db
            .iter()
            .filter(|c| {
                let lhs = self.lit_val.get(&as_atom(c.1.watched_lhs));
                let rhs = self.lit_val.get(&as_atom(c.1.watched_rhs));
                !(lhs.is_some_and(|l| !l.is_free) || rhs.is_some_and(|l| !l.is_free))
            })
            .map(|c| (c.1.vars.clone(), 0))
            .collect::<Vec<(Vec<i32>, u8)>>();
        match self.heuristic {
            Heuristic::VSIDS => {
                heuristics::vsids(&self.free_vars, &unsat_clauses, &mut self.vsids_data)
            }
            Heuristic::VMTF => heuristics::vmtf(
                &self.free_vars,
                &self.clauses.iter().map(|c| c.1.vars.clone()).collect(),
                &self.clause_db.iter().map(|c| c.1.vars.clone()).collect(),
                &mut VecDeque::new(),
                self.clause_db.len(),
            ),
            Heuristic::BerkMin => berkmin(&self.free_vars, &unsat_clauses, &mut self.berkmin_data),
            _ => (*self.free_vars.iter().next().unwrap(), true),
        }
    }
}

fn is_asserting(clause: &Vec<i32>, literals_of_max_branch_depth: &HashSet<u16>) -> bool {
    let res = HashSet::<u16>::from_iter(clause.iter().map(|l| as_atom(*l)))
        .intersection(literals_of_max_branch_depth)
        .count();
    res == 1
}

fn as_atom(lit: BVar) -> Atom {
    lit.abs() as Atom
}

fn resolution(clause1: &Vec<i32>, clause2: &Vec<i32>) -> Result<Vec<i32>, String> {
    let mut hs_1: HashSet<i32> = HashSet::from_iter(clause1.iter().cloned());
    let mut hs_2: HashSet<i32> = HashSet::from_iter(clause2.iter().cloned());
    if hs_1 == hs_2 {
        // TODO: throw error here and fix history
        return Ok(clause1.clone());
    }
    for c_1 in clause1.iter() {
        if clause2.contains(&-c_1) {
            hs_1.remove(c_1);
            hs_2.remove(&-c_1);
            let res = Vec::from_iter(hs_1.union(&hs_2).cloned());
            //println!("\t resolve {:?} with {:?} to {:?}", clause1, clause2, res.clone());
            return Ok(res);
        }
    }
    Err(format!(
        "Could not apply resolution to {:?} and {:?}",
        clause1, clause2
    ))
}

#[test]
fn should_be_sat_bug_mar_14th_1() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/ssa7552-159.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, Heuristic::Arbitrary).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_be_sat_bug_mar_14th_2() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/ssa7552-038.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, Heuristic::Arbitrary).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_be_sat_bug_mar_14th_3() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/ssa7552-158.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, Heuristic::Arbitrary).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_derive_1_UIP_from_wikipedia() {
    let mut cdcl = CDCL::new(
        vec![
            vec![1, 4],
            vec![1, -3, -8],
            vec![1, 8, 12],
            vec![2, 11],
            vec![-7, -3, 9],
            vec![-7, 8, -9],
            vec![7, 8, -10],
            vec![7, 10, -12],
        ],
        9,
        5,
        Heuristic::Arbitrary,
    );
    cdcl.history_enabled = true;
    cdcl.set_var(false, false, 1, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, true, 4, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, true, 3, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, false, 2, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, true, 7, None);
    let conflict = cdcl.unit_prop();
    assert!(conflict);
    let conflict = cdcl.analyze_conflict();
    assert!(conflict.is_ok());
    assert!(conflict.clone().unwrap().0.vars.contains(&-3));
    assert!(conflict.clone().unwrap().0.vars.contains(&-7));
    assert!(conflict.unwrap().0.vars.contains(&8));
    cdcl.print_graph_as_dot();
    assert!(cdcl.unit_queue.contains(&(-7, Some(8))));
    assert_eq!(cdcl.unit_queue.len(), 1);
    cdcl.unit_prop();
}

#[test]
fn should_derive_1_UIP_from_princeton_paper() {
    let mut cdcl = CDCL::new(
        vec![
            vec![1, 2],
            vec![1, 3, 7],
            vec![-2, -3, 4],
            vec![-4, 5, 8],
            vec![-4, 6, 9],
            vec![-5, -6],
        ],
        9,
        5,
        Heuristic::Arbitrary,
    );
    cdcl.history_enabled = true;
    cdcl.set_var(false, false, 7, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, false, 8, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, false, 9, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, false, 1, None);
    let conflict = cdcl.unit_prop();
    let conflict = cdcl.analyze_conflict();
    assert_eq!(conflict.clone().unwrap().1, 2);
    assert!(conflict.clone().unwrap().0.vars.contains(&-4));
    assert!(conflict.clone().unwrap().0.vars.contains(&9));
    assert_eq!(conflict.clone().unwrap().0.vars.len(), 3);
    assert!(conflict.unwrap().0.vars.contains(&8));
    cdcl.print_graph_as_dot();
    assert_eq!(cdcl.history.len(), 3);
    assert!(cdcl.unit_queue.contains(&(-4, Some(6))));
    cdcl.unit_prop();
    assert!(!cdcl.lit_val[&4].val);
}

#[test]
fn should_derive_1_uip_from_lecture() {
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
        Heuristic::Arbitrary,
    );
    cdcl.history_enabled = true;
    cdcl.set_var(false, false, 9, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, false, 10, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, true, 12, None);
    cdcl.unit_prop();
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, true, 1, None);
    let conflict = cdcl.unit_prop();
    assert!(conflict);
    let conflict = cdcl.analyze_conflict();
    assert_eq!(conflict.clone().unwrap().1, 1);
    assert!(conflict.clone().unwrap().0.vars.contains(&-4));
    assert!(conflict.clone().unwrap().0.vars.contains(&10));
    assert!(conflict.unwrap().0.vars.contains(&11));
    cdcl.print_graph_as_dot();
    assert_eq!(cdcl.history.len(), 3);
    assert!(cdcl.unit_queue.contains(&(-4, Some(11))))
}

#[test]
fn should_set_var_1_true_watched_literals() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        Heuristic::Arbitrary,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, true, 1, None);
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
        Heuristic::Arbitrary,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, false, -1, None);
    assert!(!c);
    assert_eq!(cdcl.free_vars.len(), 4);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, false);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().val, true);
    assert_eq!(cdcl.unit_queue.len(), 1);
    assert_eq!(cdcl.clauses.get(&2).unwrap().watched_lhs, -2);
}

#[test]
fn should_set_var_neg_1_true_watched_literals() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        Heuristic::Arbitrary,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, true, -1, None);
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
        Heuristic::Arbitrary,
    );
    assert_eq!(cdcl.free_vars.len(), 6);
    assert_eq!(cdcl.lit_val.get(&1).unwrap().is_free, true);
    assert_eq!(cdcl.pos_watched_occ.get(&1).unwrap(), &vec![0]);
    let c = cdcl.set_var(false, true, -1, None);
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
        Heuristic::Arbitrary,
    );
    cdcl.set_var(false, false, -1, None);
    let c = cdcl.set_var(false, false, 2, None);
    assert!(c)
}

#[test]
fn should_detect_conflict_watched_literals_2() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        Heuristic::Arbitrary,
    );
    cdcl.set_var(false, false, -1, None);
    let c_1 = cdcl.set_var(false, false, -2, None);
    let c_2 = cdcl.set_var(false, false, -3, None);
    assert!(!c_1);
    assert!(c_2);
}

#[test]
fn should_unit_prop_watched_literals() {
    let mut cdcl: CDCL = CDCL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        Heuristic::Arbitrary,
    );
    cdcl.set_var(false, false, -1, None);
    assert_eq!(cdcl.clauses.get(&2).unwrap().watched_lhs, -2);
    cdcl.set_var(false, false, -2, None);
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
        Heuristic::Arbitrary,
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
        Heuristic::Arbitrary,
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
        Heuristic::Arbitrary,
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
        Heuristic::Arbitrary,
    )
    .solve();
    if let DIMACSOutput::Sat(_) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}

#[test]
fn should_parse_and_solve_sat() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let mut cdcl = CDCL::new(input, v_c, c_c, Heuristic::Arbitrary);
    let res = cdcl.solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_parse_and_solve_unsat() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/unsat/aim-50-1_6-no-1.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, Heuristic::Arbitrary).solve();
    if let DIMACSOutput::Sat(vars) = res {
        println!("{:?}", vars);
        panic!("Was SAT but expected UNSAT.")
    }
}

#[test]
fn should_elim_pure_lit() {
    let res = CDCL::new(
        vec![vec![1, -2, 3], vec![1, 2], vec![1, -2, -3]],
        3,
        6,
        Heuristic::Arbitrary,
    )
    .solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_be_sat_bug_jan_2nd_1() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/ssa7552-038.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, Heuristic::Arbitrary).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_be_sat_bug_jan_2nd_2() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/uf50-06.cnf").unwrap();
    let res = CDCL::new(input, v_c, c_c, Heuristic::Arbitrary).solve();
    if let DIMACSOutput::Unsat = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn test_derive_and_add_conflict_clause() {
    let mut cdcl = CDCL::new(
        vec![vec![1, -2], vec![-1, 2], vec![-2, -1]],
        3,
        3,
        Heuristic::Arbitrary,
    );

    // Initialize the values and status for the variables in the Literal Values HashMap

    cdcl.history_enabled = true;
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, true, 1, None);
    cdcl.unit_prop();

}

#[test]
fn test_analyze_conflict() {
    let mut cdcl = CDCL::new(vec![vec![-1, -2], vec![-1, 2]], 2, 2, Heuristic::Arbitrary);
    cdcl.history_enabled = true;
    cdcl.branch_depth += 1;
    cdcl.literals_at_current_depth.clear();
    cdcl.set_var(false, true, 1, None);
    cdcl.unit_prop();

    // Simulate a conflict
    let result = cdcl.analyze_conflict();
    assert!(result.is_ok());
    let (conflict_clause, backtrack_level) = result.unwrap();
    assert!(!conflict_clause.vars.is_empty());
    assert_eq!(backtrack_level, 0);
    cdcl.unit_prop();
}

#[test]
fn test_current_decision_level() {
    let mut cdcl = CDCL::new(
        vec![vec![1, 2], vec![2, 3], vec![3, 1]],
        0,
        0,
        Heuristic::Arbitrary,
    );
    assert_eq!(cdcl.branch_depth, 0); // No assignments, decision level should be 0
    cdcl.solve();
    assert_eq!(cdcl.branch_depth, 2); // Assign 1 -> unit prop 2 -> Assign 2 => decision_level of 2
}
