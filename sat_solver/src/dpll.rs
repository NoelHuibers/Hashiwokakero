use crate::{
    cli::Heuristic,
    hash::{self, HashMap, HashSet},
    heuristics::{arbitrary, boehm, custom, dlcs, dlis, jeroslaw_wang, mom, vsids_dpll},
    logger::ProofLog,
    profiling,
};
use std::{collections::VecDeque, fmt, time::Instant};

// Absolute value of a literal. This used to be u16, which silently merged variables above 65535.
type Atom = u32;
type BVar = i32;
type CIdx = usize;

#[derive(Debug, Clone)]
pub enum DIMACSOutput {
    Sat(Vec<i32>),
    Unsat,
    /// The solver gave up because its deadline passed (see `set_deadline`).
    Timeout,
}

impl fmt::Display for DIMACSOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sat(variables) => {
                writeln!(f, "SAT")?;
                if !variables.is_empty() {
                    for (index, var) in variables.iter().enumerate() {
                        if index > 0 {
                            write!(f, " ")?;
                        }
                        write!(f, "{var}")?;
                    }
                }
                Ok(())
            }
            Self::Unsat => write!(f, "UNSAT"),
            Self::Timeout => write!(f, "UNKNOWN (timeout)"),
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub branch_depth: i64,
    pub history_len: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Error: {} (branch depth: {}, history length: {})",
            self.message, self.branch_depth, self.history_len
        )
    }
}

// Using structs instead of tuples for readability
// Same mem layout as for tuples:
// - https://doc.rust-lang.org/reference/type-layout.html#tuple-layout
// - https://doc.rust-lang.org/reference/types/struct.html#structtype
#[derive(Debug)]
pub(crate) struct Clause {
    pub(crate) sat_by_var: BVar,
    pub(crate) vars: Vec<BVar>,
    pub(crate) unassign_vars: u32,
}

#[derive(Debug)]
pub(crate) struct Literal {
    pub(crate) val: bool,
    pub(crate) is_free: bool,
}

#[derive(Debug)]
struct Assignment {
    var: BVar,
    val: bool,
    forced: bool,
}

#[derive(Debug)]
pub struct DPLL {
    // Using HashMaps due to better get(i) / append complexity, see https://doc.rust-lang.org/std/collections/#sequences
    clauses: HashMap<usize, Clause>,
    heuristic: Heuristic,
    history: Vec<Assignment>,
    // Memory allocation in the history is a bottle neck thus the initial unit prop and pure lit elim don't need to expand it
    history_enabled: bool,
    lit_val: HashMap<Atom, Literal>,
    min_depth: usize,
    // Keys don't contain the sign as abs is cheaper than calculating the sign every time
    neg_occ: HashMap<Atom, Vec<CIdx>>,
    pos_occ: HashMap<Atom, Vec<CIdx>>,
    // Using VecDaque for better push_front complexity
    unit_queue: VecDeque<BVar>,
    // Those are for the heuristics:
    unsat_clauses: HashSet<(Vec<BVar>, u32)>,
    free_lits: HashSet<BVar>,
    show_depth: bool,
    decisions: u64,
    // Give up (DIMACSOutput::Timeout) once this point in time has passed
    deadline: Option<Instant>,
    // The input contained an empty clause, so the formula is trivially UNSAT
    has_empty_clause: bool,
    // The DRAT proof being written, if asked for: at every conflict the negation of the
    // decisions, which is RUP (see `conflict_lemma`)
    proof: Option<ProofLog>,
}

impl DPLL {
    pub fn new(
        mut input: Vec<Vec<i32>>,
        lit_count: usize,
        clause_count: usize,
        heuristic: Heuristic,
        show_depth: bool,
    ) -> Self {
        profiling::start("DPLL::new");
        let mut dpll = Self {
            clauses: hash::map(clause_count),
            heuristic,
            history: Vec::new(),
            history_enabled: false,
            lit_val: hash::map(lit_count),
            min_depth: if show_depth { usize::MAX } else { 0 },
            neg_occ: hash::map(lit_count),
            pos_occ: hash::map(lit_count),
            unit_queue: VecDeque::new(),
            free_lits: hash::set(lit_count),
            unsat_clauses: hash::set(clause_count),
            show_depth,
            decisions: 0,
            deadline: None,
            has_empty_clause: false,
            proof: None,
        };
        if input.iter().any(Vec::is_empty) {
            dpll.has_empty_clause = true;
            input.retain(|c| !c.is_empty());
        }
        // A literal twice in a clause was counted twice as unassigned: such a clause never
        // became unit, a conflict on it went unnoticed and the search ended in a panic
        for clause in &mut input {
            let mut seen = hash::set(clause.len());
            clause.retain(|&l| seen.insert(l));
        }
        // * read formula
        // Using iterators where possible for better performance
        input.into_iter().enumerate().for_each(|(c, vars)| {
            let len = u32::try_from(vars.len()).expect("a clause has fewer than 2^32 literals");
            dpll.unsat_clauses.insert((vars.clone(), len));
            let clause = Clause {
                sat_by_var: 0,
                vars: vars.clone(),
                unassign_vars: len,
            };
            let fist_var = &clause.vars.clone()[0];
            dpll.clauses.insert(c, clause);
            if vars.len() == 1 && !dpll.unit_queue.contains(fist_var) {
                dpll.unit_queue.push_front(*fist_var);
            }
            for literal in &vars {
                let lit = as_atom(*literal);
                dpll.lit_val.insert(
                    lit,
                    Literal {
                        val: false,
                        is_free: true,
                    },
                );
                // Just for heuristics
                dpll.free_lits.insert(*literal);
                match literal.signum() {
                    // TODO: Is it neccessary to check whether a variable occurs twice in same pol. in a clause?
                    // When not checking clauses like (a \/ -a \/ ...) would not be taken into account.
                    1 => dpll
                        .pos_occ
                        .entry(lit)
                        .and_modify(|clauses: &mut Vec<CIdx>| clauses.push(c))
                        .or_insert_with(|| vec![c]),
                    -1 => dpll
                        .neg_occ
                        .entry(lit)
                        .and_modify(|clauses: &mut Vec<CIdx>| clauses.push(c))
                        .or_insert_with(|| vec![c]),
                    _ => panic!("0 is not a valid Variable in the DIMACS format"),
                };
            }
        });
        profiling::end("DPLL::new");
        dpll
    }

    /// Stop searching once `deadline` has passed, `solve` then returns [`DIMACSOutput::Timeout`].
    pub fn set_deadline(&mut self, deadline: Option<Instant>) {
        self.deadline = deadline;
    }

    /// Builder style variant of [`DPLL::set_deadline`].
    #[must_use]
    pub fn with_deadline(mut self, deadline: Option<Instant>) -> Self {
        self.deadline = deadline;
        self
    }

    /// Writes a DRAT proof to `proof` when the formula turns out unsatisfiable.
    #[must_use]
    pub fn with_proof(mut self, proof: Option<ProofLog>) -> Self {
        self.proof = proof;
        self
    }

    /// At a conflict: the decisions so far can't all hold, the negation of the decision
    /// literals is a lemma. It is RUP: every flipped decision on the trail was made unit by the
    /// lemma of the conflict that flipped it (which only names decisions below it), so unit
    /// propagation from the decisions reproduces the trail and its conflict. With no decisions
    /// left it is the empty clause, the end of the refutation.
    fn conflict_lemma(&mut self) {
        if let Some(log) = &mut self.proof {
            let lemma: Vec<BVar> = self
                .history
                .iter()
                .filter(|step| !step.forced)
                .map(|step| if step.val { -step.var } else { step.var })
                .collect();
            if lemma.is_empty() {
                log.refuted();
            } else {
                log.add(&lemma);
            }
        }
    }

    /// The formula is refuted without a search: unit propagation alone does it.
    fn refuted(&mut self) -> DIMACSOutput {
        if let Some(log) = &mut self.proof {
            log.refuted();
        }
        DIMACSOutput::Unsat
    }

    /// Number of branching decisions (excluding pure literals and forced assignments) so far.
    pub fn decisions(&self) -> u64 {
        self.decisions
    }

    fn timed_out(&self, step: u64) -> bool {
        step.is_multiple_of(16) && self.deadline.is_some_and(|d| Instant::now() >= d)
    }

    pub fn solve(&mut self) -> Result<DIMACSOutput, Error> {
        if self.has_empty_clause {
            return Ok(self.refuted());
        }
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(DIMACSOutput::Timeout);
        }
        let mut steps: u64 = 0;
        // * unit propagation
        profiling::start("DPLL::solve");
        let conflict = self.unit_prop();
        if conflict {
            profiling::end("DPLL::solve");
            return Ok(self.refuted());
        }
        if self.clauses.values().all(|c| c.sat_by_var != 0) {
            let mut res: Vec<i32> = self
                .lit_val
                .iter()
                .map(|(atom, lit)| {
                    if lit.val {
                        as_lit(*atom)
                    } else {
                        -as_lit(*atom)
                    }
                })
                .collect();
            res.sort_by_key(|k| k.abs());
            res.push(0);
            profiling::end("DPLL::solve");
            return Ok(DIMACSOutput::Sat(res));
        }

        // * pure lit elim
        let mut pure_lits = self.get_pure_lits();

        loop {
            steps += 1;
            if self.timed_out(steps) {
                profiling::end("DPLL::solve");
                return Ok(DIMACSOutput::Timeout);
            }
            // * choose literal var
            let (var, val, forced) = if pure_lits.is_empty() {
                self.history_enabled = true;
                let (var, val) = self.pick_literal();
                self.decisions += 1;
                (var, val, false)
            } else {
                let lit = pure_lits.pop().unwrap();
                // A pure literal is RAT: no clause has its negation (and no lemma yet either,
                // pure literals come before any decision). The literal `set_var` makes true:
                if let Some(log) = &mut self.proof {
                    let val = lit.is_positive();
                    log.add(&[if val { lit } else { -lit }]);
                }
                // As the lit occurs only in one polarity it doesn't make sense to try both assignments
                (lit, lit.is_positive(), true)
            };
            // * set value var
            self.set_var(forced, val, var);

            // * unit propagation
            loop {
                let conflict = self.unit_prop();
                if !conflict {
                    break;
                }

                steps += 1;
                if self.timed_out(steps) {
                    profiling::end("DPLL::solve");
                    return Ok(DIMACSOutput::Timeout);
                }
                // * if conflict detected
                self.conflict_lemma();
                let unsat = self.backtrack();
                if unsat {
                    profiling::end("DPLL::solve");
                    return Ok(DIMACSOutput::Unsat);
                }
            }

            // * if all clauses satisfied
            if self.clauses.values().all(|c| c.sat_by_var != 0) {
                let mut res: Vec<i32> = self
                    .lit_val
                    .iter()
                    .map(|(atom, lit)| {
                        if lit.val {
                            as_lit(*atom)
                        } else {
                            -as_lit(*atom)
                        }
                    })
                    .collect();
                res.sort_by_key(|k| k.abs());
                // Terminated like the other models (see above and CDCL)
                res.push(0);
                profiling::end("DPLL::solve");
                return Ok(DIMACSOutput::Sat(res));
            }
        }
    }

    fn get_pure_lits(&self) -> Vec<BVar> {
        profiling::start("get_pure_lits");
        let neg_hs = self.neg_occ.keys().collect::<HashSet<&Atom>>();
        let pos_hs = self.pos_occ.keys().collect::<HashSet<&Atom>>();
        // neg_occ \ pos_occ
        let pure = neg_hs.symmetric_difference(&pos_hs);
        let result = pure
            .into_iter()
            .map(|&literal| {
                // TODO: is the sign in the unitque important?
                let sign = match pos_hs.get(literal) {
                    Some(_) => 1,
                    None => -1,
                };
                as_lit(*literal) * sign
            })
            .filter(|p| !self.lit_val.contains_key(&as_atom(*p)))
            .collect::<Vec<BVar>>();
        profiling::end("get_pure_lits");
        result
    }

    fn backtrack(&mut self) -> bool {
        profiling::start("backtrack");
        let mut last_step = self.history.pop();
        while last_step.as_ref().is_some_and(|step| step.forced) {
            self.unset_var(last_step.unwrap().var);
            last_step = self.history.pop();
        }
        if self.show_depth && self.history.len() <= self.min_depth {
            self.min_depth = self.history.len();
            println!("backtracked to depth {}", self.history.len());
        }
        if last_step.is_none() {
            profiling::end("backtrack");
            return true;
        }
        let var = last_step.as_ref().unwrap().var;
        let val = last_step.unwrap().val;
        self.unset_var(var);
        self.unit_queue.clear();
        self.set_var(true, !val, var);
        profiling::end("backtrack");
        false
    }

    // TODO: @Laura hier können die Heuristiken rein. Gerne auch mit enum flag welche an/aus sind.
    fn pick_literal(&self) -> (BVar, bool) {
        profiling::start("pick literal");
        let (var, val) = match self.heuristic {
            Heuristic::DLIS => dlis(&self.free_lits, &self.unsat_clauses),
            Heuristic::DLCS => dlcs(&self.free_lits, &self.lit_val, &self.unsat_clauses),
            Heuristic::MOM => mom(&self.free_lits, &self.lit_val, &self.unsat_clauses),
            Heuristic::Boehm => boehm(&self.free_lits, &self.lit_val, &self.unsat_clauses),
            Heuristic::JeroslawWang => {
                jeroslaw_wang(&self.free_lits, &self.lit_val, &self.unsat_clauses)
            }
            Heuristic::VSIDS => vsids_dpll(&self.free_lits, &self.unsat_clauses),
            Heuristic::Custom => custom(&self.free_lits, &self.unsat_clauses),
            _ => arbitrary(&self.free_lits),
        };
        profiling::end("pick literal");
        (var, val)
    }

    fn unit_prop(&mut self) -> bool {
        profiling::start("unit propagation");
        while !self.unit_queue.is_empty() {
            let forced_lit = self.unit_queue.pop_back().unwrap();
            // mark all clauses with pos_occ as sat
            let unsat = self.set_var(true, true, forced_lit);
            if unsat {
                profiling::end("unit propagation");
                return true;
            }
        }
        profiling::end("unit propagation");
        false
    }

    fn set_var(&mut self, forced: bool, val: bool, var: BVar) -> bool {
        profiling::start("set var");
        if self.history_enabled {
            self.history.push(Assignment { var, val, forced });
        }
        let mut conflict = false;
        let lit = self.lit_val.get_mut(&as_atom(var)).unwrap();
        lit.is_free = false;
        // Just for heuristics
        self.free_lits.remove(&var);
        self.free_lits.remove(&-var);
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
        let (mark_sat, mark_unsat) = if new_val {
            (&self.pos_occ, &self.neg_occ)
        } else {
            (&self.neg_occ, &self.pos_occ)
        };
        if let Some(occ) = mark_sat.get(&as_atom(var)) {
            for c in occ {
                self.clauses.entry(*c).and_modify(|sat_clause| {
                    if sat_clause.sat_by_var == 0 {
                        sat_clause.sat_by_var = var;
                        self.unsat_clauses
                            .remove(&(sat_clause.vars.clone(), sat_clause.unassign_vars));
                    }
                });
            }
        }
        if let Some(occ) = mark_unsat.get(&as_atom(var)) {
            for c in occ {
                let unsat_clause = self.clauses.get_mut(c).unwrap();
                self.unsat_clauses
                    .remove(&(unsat_clause.vars.clone(), unsat_clause.unassign_vars));
                unsat_clause.unassign_vars -= 1;
                self.unsat_clauses
                    .insert((unsat_clause.vars.clone(), unsat_clause.unassign_vars));
                match unsat_clause.unassign_vars {
                    0 => conflict = true,
                    1 => {
                        if let Some(free_lit) = unsat_clause.vars.iter().find(|&v| {
                            self.lit_val.get(&as_atom(*v)).unwrap().is_free
                                && !self.unit_queue.contains(v)
                        }) {
                            self.unit_queue.push_front(*free_lit);
                        }
                    }
                    _ => (),
                }
            }
        }
        profiling::end("set var");
        conflict
    }

    fn unset_var(&mut self, var: BVar) {
        profiling::start("unset var");
        let lit_var = self.lit_val.get_mut(&as_atom(var)).unwrap();
        lit_var.is_free = true;
        // Just for heuristics
        self.free_lits.insert(var);
        self.free_lits.insert(-var);
        // Value of lit_var actually doesn't matter
        let (mark_sat, mark_unsat) = if lit_var.val {
            (&self.pos_occ, &self.neg_occ)
        } else {
            (&self.neg_occ, &self.pos_occ)
        };
        if let Some(occ) = mark_sat.get(&as_atom(var)) {
            for c in occ {
                self.clauses.entry(*c).and_modify(|sat_clause| {
                    if sat_clause.sat_by_var == var {
                        sat_clause.sat_by_var = 0;
                        self.unsat_clauses
                            .insert((sat_clause.vars.clone(), sat_clause.unassign_vars));
                    }
                });
            }
        }
        if let Some(occ) = mark_unsat.get(&as_atom(var)) {
            for c in occ {
                let unsat_clause = self.clauses.get_mut(c).unwrap();
                self.unsat_clauses
                    .remove(&(unsat_clause.vars.clone(), unsat_clause.unassign_vars));
                unsat_clause.unassign_vars += 1;
                self.unsat_clauses
                    .insert((unsat_clause.vars.clone(), unsat_clause.unassign_vars));
            }
        }
        profiling::end("unset var");
    }
}

fn as_atom(lit: BVar) -> Atom {
    lit.unsigned_abs()
}

// Atoms are absolute values of BVar literals, so they convert back without wrapping
const fn as_lit(atom: Atom) -> BVar {
    atom.cast_signed()
}

#[test]
fn should_solve_sat_small() {
    let res = DPLL::new(
        vec![vec![1, -2, 3], vec![-1, 2], vec![-1, -2, -3]],
        3,
        3,
        Heuristic::Arbitrary,
        true,
    )
    .solve();
    if matches!(res, Ok(DIMACSOutput::Unsat)) {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_solve_sat() {
    let res = DPLL::new(
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
        true,
    )
    .solve();
    if matches!(res, Ok(DIMACSOutput::Unsat)) {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_solve_unsat_small() {
    let res = DPLL::new(
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
        true,
    )
    .solve();
    if let Ok(DIMACSOutput::Sat(_)) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}

#[test]
fn should_solve_unsat() {
    let res = DPLL::new(
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
        true,
    )
    .solve();
    if let Ok(DIMACSOutput::Sat(_)) = res {
        panic!("Was SAT but expected UNSAT.")
    }
}

/// Found by the property tests (tests/props.rs): repeated literals made the solver panic.
#[test]
fn repeated_literals_count_once() {
    let solve =
        |clauses: Vec<Vec<i32>>| DPLL::new(clauses, 2, 2, Heuristic::Arbitrary, false).solve();
    assert!(matches!(
        solve(vec![vec![1, 1], vec![-1, -1]]),
        Ok(DIMACSOutput::Unsat)
    ));
    assert!(matches!(
        solve(vec![vec![1, 2, 1], vec![-1], vec![-2, -2]]),
        Ok(DIMACSOutput::Unsat)
    ));
    let Ok(DIMACSOutput::Sat(model)) = solve(vec![vec![2, 2], vec![-1, 2, -1]]) else {
        panic!("Expected SAT");
    };
    assert!(model.contains(&2));
}

/// Found by the property tests (backend/tests/props.rs): the custom heuristic found no
/// variable to branch on when the free ones only occur negated, and DPLL panicked.
#[test]
fn custom_heuristic_branches_on_negated_variables() {
    let res = DPLL::new(vec![vec![-2, -1]], 2, 1, Heuristic::Custom, false).solve();
    let Ok(DIMACSOutput::Sat(model)) = res else {
        panic!("Expected SAT, got {res:?}");
    };
    assert!(model.contains(&-1) || model.contains(&-2));
}

#[test]
fn should_parse_and_solve_sat() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/aim-50-1_6-yes1-1.cnf").unwrap();
    let res = DPLL::new(input, v_c, c_c, Heuristic::Arbitrary, true).solve();
    if matches!(res, Ok(DIMACSOutput::Unsat)) {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_parse_and_solve_unsat() {
    let (input, v_c, c_c) = crate::parse::parse("./src/inputs/unsat/aim-50-1_6-no-1.cnf").unwrap();
    let res = DPLL::new(input, v_c, c_c, Heuristic::Arbitrary, true).solve();
    if let Ok(DIMACSOutput::Sat(_)) = res {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
fn should_elim_pure_lit() {
    let res = DPLL::new(
        vec![vec![1, -2, 3], vec![1, 2], vec![1, -2, -3]],
        3,
        6,
        Heuristic::Arbitrary,
        true,
    )
    .solve();
    if matches!(res, Ok(DIMACSOutput::Unsat)) {
        panic!("Was UNSAT but expected SAT.")
    }
}

#[test]
#[ignore = "over five minutes: the arbitrary heuristic is a poor fit for ssa7552-038"]
fn bug_jan_2nd_should_be_sat() {
    let (mut input, mut v_c, mut c_c) =
        crate::parse::parse("./src/inputs/sat/ssa7552-038.cnf").unwrap();
    let res = DPLL::new(input, v_c, c_c, Heuristic::Arbitrary, true).solve();
    if matches!(res, Ok(DIMACSOutput::Unsat)) {
        panic!("Was UNSAT but expected SAT.")
    }

    (input, v_c, c_c) = crate::parse::parse("./src/inputs/sat/uf50-06.cnf").unwrap();
    let res = DPLL::new(input, v_c, c_c, Heuristic::Arbitrary, true).solve();
    if matches!(res, Ok(DIMACSOutput::Unsat)) {
        panic!("Was UNSAT but expected SAT.")
    }
}

/// The DPLL proof: the negated decisions at every conflict, the empty clause at the end.
#[test]
fn writes_a_proof_of_negated_decisions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proof.drat");
    let clauses = vec![vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2]];
    let log = ProofLog::create(&path).unwrap();
    let result = DPLL::new(clauses, 2, 4, Heuristic::Arbitrary, false)
        .with_proof(Some(log))
        .solve()
        .unwrap();
    assert!(matches!(result, DIMACSOutput::Unsat));
    let proof = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = proof.lines().collect();
    // One decision fails, then the flipped one: no decisions are left
    assert_eq!(lines.len(), 2, "{proof}");
    assert_eq!(lines[1], "0");
    assert!(
        ["-1 0", "1 0", "-2 0", "2 0"].contains(&lines[0]),
        "{proof}"
    );
    // Refuted by unit propagation
    let log = ProofLog::create(&path).unwrap();
    let result = DPLL::new(vec![vec![1], vec![-1]], 1, 2, Heuristic::Arbitrary, false)
        .with_proof(Some(log))
        .solve()
        .unwrap();
    assert!(matches!(result, DIMACSOutput::Unsat));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "0\n");
}
