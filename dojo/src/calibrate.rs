//! The calibration study: does fuel predict how long a solver really takes?
//!
//! The main objection to counting instructions (Coutelier, Rath, Rawson, Biere and Kovács,
//! 2024) is that they don't follow the cost of memory accesses, which is why Kissat and `CaDiCaL`
//! count "ticks" (about the cache lines touched) and Knuth counts "mems". This module measures
//! it, see dojo/CALIBRATION.md for the method and the results. Three steps, each a
//! `dojo calibrate` subcommand:
//!
//! - [`measure_fuel`]: every solver (WebAssembly) on every formula with the uniform cost table,
//!   capped; the runs that finish run once more per [`Category`] with a table that only charges
//!   that category, which counts how often the run executed it. Deterministic, in parallel.
//! - [`measure_time`]: the same solvers built natively, the median wall time of a few runs
//!   (run it pinned to one core, `taskset -c N`, on a quiet machine), and the WebAssembly
//!   modules once more in the sandbox for the overhead.
//! - [`analyze`]: correlations of fuel and time within and across solvers, how well fuel keeps
//!   the ranking by PAR-2, and cost weights per category fitted to the native times, checked by
//!   leaving one solver out.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "statistics on counts and times far below 2^52, rounded on purpose"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::bench::Instance;
use crate::eval::{self, Options};
use crate::fuel::{Category, Counts, Weights};
use crate::output::{Claim, parse};
use crate::proof::TempDir;
use crate::sandbox::{Limits, Sandbox};
use crate::{Answer, Outcome};

const CATEGORIES: usize = Category::ALL.len();

/// The set seed the random numbers of every run in the study come from (see
/// [`eval::rng_seed`]), the same in both measurements.
pub const SEED: &str = "calibration";

/// The leaderboard's budget in uniform fuel (what it was before the calibrated model, rules
/// version 3): the rankings of the analysis use it, since [`measure_fuel`] measures with the
/// uniform table.
pub const UNIFORM_BUDGET: u64 = 10_000_000_000;

/// The cap of [`measure_fuel`] in uniform fuel: four budgets, so a model may scale fuel by up
/// to four before a run counts as unsolved in every model.
pub const CAP: u64 = 4 * UNIFORM_BUDGET;

/// One solver of the study: its WebAssembly module and, for [`measure_time`], the same
/// solver built natively.
#[derive(Clone, Debug)]
pub struct Solver {
    pub name: String,
    pub wasm: Vec<u8>,
    pub native: Option<PathBuf>,
}

/// A run in the sandbox: how it ended under the uniform table, and how often it executed each
/// category (`None` if it didn't finish within the cap, then there is nothing to count).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuelRow {
    pub solver: String,
    pub instance: String,
    pub family: String,
    pub expected: Answer,
    /// Solved under the uniform table, within the cap, with a model or a verified proof.
    pub solved: bool,
    /// Uniform fuel, at most the cap.
    pub fuel: u64,
    pub counts: Option<Counts>,
}

/// A native run: the median wall time of its runs and whether it solved the formula, and the
/// wall time of the WebAssembly module in the sandbox.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeRow {
    pub solver: String,
    pub instance: String,
    /// Milliseconds of each native run (one if the first ran into the timeout).
    pub native_ms: Vec<f64>,
    /// Answered correctly (a model that checks out, or UNSAT on an UNSAT formula).
    pub native_solved: bool,
    /// Milliseconds in the sandbox (uniform table, no proof check), `None` if not measured.
    pub wasm_ms: Option<f64>,
}

impl TimeRow {
    pub fn median_ms(&self) -> f64 {
        median(&self.native_ms)
    }
}

/// Runs `f` on every item with `threads` threads, results in order.
fn parallel<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, items.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let result = f(item);
                    *slots[i]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
                }
            });
        }
    });
    slots
        .into_iter()
        .filter_map(|s| {
            s.into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
        .collect()
}

/// Measures fuel: see the module docs. `options` gives the cap (`limits.fuel`), memory, the
/// proof checker and the threads; `progress` gets every finished row.
pub fn measure_fuel(
    solvers: &[Solver],
    set: &[Instance],
    options: &Options,
    progress: impl Fn(&FuelRow) + Sync,
) -> Result<Vec<FuelRow>, String> {
    let uniform = Sandbox::with_weights(&Weights::UNIFORM)?;
    // One per category, then one that only counts the function entries
    let counting: Vec<Sandbox> = Category::ALL
        .iter()
        .map(|&c| Weights::only(c))
        .chain([Weights::ZERO])
        .map(|weights| Sandbox::with_weights(&weights))
        .collect::<Result<_, _>>()?;
    let mut work = Vec::new();
    for (s, solver) in solvers.iter().enumerate() {
        // Compiled once per sandbox (a module belongs to the engine it was compiled with)
        let modules = std::iter::once(&uniform)
            .chain(&counting)
            .map(|sandbox| {
                sandbox
                    .load(&solver.wasm)
                    .map_err(|e| format!("{}: {e:?}", solver.name))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let modules = std::sync::Arc::new(modules);
        for instance in set {
            work.push((s, std::sync::Arc::clone(&modules), instance));
        }
    }
    let rows = parallel(&work, options.threads, |(s, modules, instance)| {
        let result = eval::run_one(&uniform, &modules[0], instance, options);
        let finished = result.fuel < options.limits.fuel
            && !matches!(result.outcome, Outcome::Timeout | Outcome::Crashed);
        let counts = finished.then(|| {
            let mut fuel = [0; CATEGORIES + 1];
            let limits = Limits {
                fuel: u64::MAX / 2,
                ..options.limits.clone()
            };
            for (c, sandbox) in counting.iter().enumerate() {
                let dir = TempDir::new(&options.proof_root, "dojo-calibrate").ok();
                let run = sandbox.run(
                    &modules[c + 1],
                    &eval::input(instance),
                    eval::rng_seed(&options.set_seed, &instance.name),
                    &limits,
                    false,
                    dir.as_ref().map(TempDir::path),
                );
                fuel[c] = run.fuel;
            }
            let entries = fuel[CATEGORIES];
            let mut counts = Counts {
                entries,
                ..Counts::default()
            };
            for (op, fuel) in counts.ops.iter_mut().zip(fuel) {
                *op = fuel.saturating_sub(entries);
            }
            counts
        });
        let row = FuelRow {
            solver: solvers[*s].name.clone(),
            instance: instance.name.clone(),
            family: instance.family.id().to_string(),
            expected: instance.expected,
            solved: matches!(result.outcome, Outcome::Solved(_)),
            fuel: result.fuel,
            counts,
        };
        progress(&row);
        row
    });
    Ok(rows)
}

/// How long the native runs and the sandbox may take.
#[derive(Clone, Debug)]
pub struct TimeLimits {
    pub timeout: Duration,
    /// Limits of the sandbox runs (the fuel cap of [`measure_fuel`]).
    pub sandbox: Limits,
}

/// What to measure of one solver on one formula: native runs (the median counts; a run that
/// hits the timeout isn't repeated) and whether to time the sandbox too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub runs: usize,
    pub wasm: bool,
}

/// Whether `stdout` answers `instance` correctly (models are checked, UNSAT is trusted when
/// the label says so: the native runs aren't proof-checked, the sandbox runs were).
fn correct(instance: &Instance, stdout: &str) -> bool {
    match parse(stdout) {
        Claim::Sat(model) => instance.cnf.check(&model) == Some(true),
        Claim::Unsat => instance.expected == Answer::Unsat,
        Claim::Unknown | Claim::Malformed => false,
    }
}

/// One native run of `binary` on `input` (a DIMACS file) with a proof file in `scratch`: wall
/// time and stdout. The coreutils `timeout` ends it after `limit`; it waits without using the
/// CPU the solver runs on.
fn native_run(
    binary: &Path,
    input: &Path,
    scratch: &Path,
    limit: Duration,
) -> Result<(Duration, bool, String), String> {
    let out = scratch.join("stdout");
    let proof = scratch.join("proof.out");
    let stdin = std::fs::File::open(input).map_err(|e| e.to_string())?;
    let stdout = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let status = Command::new("timeout")
        .arg("-s")
        .arg("KILL")
        .arg(format!("{}", limit.as_secs_f64()))
        .arg(binary)
        .arg(&proof)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("timeout {}: {e}", binary.display()))?;
    let wall = started.elapsed();
    let _ = std::fs::remove_file(&proof);
    // 124: timed out (TERM), 137: killed
    let timed_out = matches!(status.code(), Some(124 | 137)) || status.code().is_none();
    let text = std::fs::read_to_string(&out).unwrap_or_default();
    Ok((wall, timed_out, text))
}

/// Measures time: see the module docs.
///
/// Runs one thing at a time, so run it pinned to a core. `plan` says what to measure of a
/// solver on a formula (`None`: nothing). `scratch` holds the formula, the output and the proof
/// of the current run.
pub fn measure_time(
    solvers: &[Solver],
    set: &[Instance],
    limits: &TimeLimits,
    plan: &dyn Fn(&Solver, &Instance) -> Option<Plan>,
    scratch: &Path,
    progress: impl Fn(&TimeRow),
) -> Result<Vec<TimeRow>, String> {
    let sandbox = Sandbox::with_weights(&Weights::UNIFORM)?;
    std::fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
    let cnf = scratch.join("formula.cnf");
    let mut rows = Vec::new();
    for solver in solvers {
        let module = sandbox
            .load(&solver.wasm)
            .map_err(|e| format!("{}: {e:?}", solver.name))?;
        for instance in set {
            let Some(plan) = plan(solver, instance) else {
                continue;
            };
            let input = eval::input(instance);
            std::fs::write(&cnf, &input).map_err(|e| e.to_string())?;
            let mut native_ms = Vec::new();
            let mut native_solved = false;
            if let Some(binary) = &solver.native {
                for _ in 0..plan.runs.max(1) {
                    let (wall, timed_out, stdout) =
                        native_run(binary, &cnf, scratch, limits.timeout)?;
                    native_ms.push(wall.as_secs_f64() * 1e3);
                    native_solved = !timed_out && correct(instance, &stdout);
                    if timed_out {
                        break;
                    }
                }
            }
            let wasm_ms = plan.wasm.then(|| {
                let dir = TempDir::new(scratch, "dojo-calibrate").ok();
                let run = sandbox.run(
                    &module,
                    &input,
                    eval::rng_seed(SEED, &instance.name),
                    &limits.sandbox,
                    false,
                    dir.as_ref().map(TempDir::path),
                );
                run.wall.as_secs_f64() * 1e3
            });
            let row = TimeRow {
                solver: solver.name.clone(),
                instance: instance.name.clone(),
                native_ms,
                native_solved,
                wasm_ms,
            };
            progress(&row);
            rows.push(row);
        }
    }
    Ok(rows)
}

// ----- CSV ------------------------------------------------------------------------------------

/// The header of fuel.csv.
pub fn fuel_header() -> String {
    let mut header = String::from("solver,instance,family,expected,solved,fuel");
    for c in Category::ALL {
        let _ = write!(header, ",{}", c.id());
    }
    header.push_str(",entries");
    header
}

impl FuelRow {
    pub fn csv(&self) -> String {
        let mut line = format!(
            "{},{},{},{},{},{}",
            self.solver,
            self.instance,
            self.family,
            self.expected.id(),
            u8::from(self.solved),
            self.fuel
        );
        match &self.counts {
            Some(counts) => {
                for n in counts.ops.iter().chain([&counts.entries]) {
                    let _ = write!(line, ",{n}");
                }
            }
            None => line.push_str(&",".repeat(CATEGORIES + 1)),
        }
        line
    }

    pub fn parse(line: &str) -> Result<Self, String> {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != 7 + CATEGORIES {
            return Err(format!("expected {} fields: {line}", 7 + CATEGORIES));
        }
        let number = |s: &str| s.parse::<u64>().map_err(|_| format!("not a number: {s}"));
        let counts = if fields[6].is_empty() {
            None
        } else {
            let mut counts = Counts::default();
            for (c, field) in fields[6..6 + CATEGORIES].iter().enumerate() {
                counts.ops[c] = number(field)?;
            }
            counts.entries = number(fields[6 + CATEGORIES])?;
            Some(counts)
        };
        Ok(Self {
            solver: fields[0].to_string(),
            instance: fields[1].to_string(),
            family: fields[2].to_string(),
            expected: Answer::from_id(fields[3]).ok_or_else(|| format!("answer: {line}"))?,
            solved: fields[4] == "1",
            fuel: number(fields[5])?,
            counts,
        })
    }
}

/// The header of time.csv.
pub const TIME_HEADER: &str = "solver,instance,native_ms,native_runs,native_solved,wasm_ms";

impl TimeRow {
    pub fn csv(&self) -> String {
        let runs: Vec<String> = self.native_ms.iter().map(|ms| format!("{ms:.3}")).collect();
        format!(
            "{},{},{:.3},{},{},{}",
            self.solver,
            self.instance,
            self.median_ms(),
            runs.join(";"),
            u8::from(self.native_solved),
            self.wasm_ms.map_or(String::new(), |ms| format!("{ms:.3}"))
        )
    }

    pub fn parse(line: &str) -> Result<Self, String> {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != 6 {
            return Err(format!("expected 6 fields: {line}"));
        }
        let ms = |s: &str| s.parse::<f64>().map_err(|_| format!("not a number: {s}"));
        Ok(Self {
            solver: fields[0].to_string(),
            instance: fields[1].to_string(),
            native_ms: fields[3]
                .split(';')
                .filter(|s| !s.is_empty())
                .map(ms)
                .collect::<Result<_, _>>()?,
            native_solved: fields[4] == "1",
            wasm_ms: if fields[5].is_empty() {
                None
            } else {
                Some(ms(fields[5])?)
            },
        })
    }
}

/// Reads a CSV file of `parse`d rows after its header.
pub fn read_csv<T>(
    text: &str,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(parse)
        .collect()
}

// ----- Statistics -----------------------------------------------------------------------------

pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        f64::midpoint(sorted[n / 2 - 1], sorted[n / 2])
    }
}

/// The `q` quantile (0..=1), linearly interpolated.
pub fn quantile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = q.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let (low, high) = (position.floor() as usize, position.ceil() as usize);
    sorted[low] + (sorted[high] - sorted[low]) * (position - low as f64)
}

/// Ranks from 1, ties get the mean of their ranks.
fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut ranks = vec![0.0; values.len()];
    let mut i = 0;
    while i < order.len() {
        let mut j = i;
        while j + 1 < order.len() && values[order[j + 1]].total_cmp(&values[order[i]]).is_eq() {
            j += 1;
        }
        let rank = (i + j) as f64 / 2.0 + 1.0;
        for &k in &order[i..=j] {
            ranks[k] = rank;
        }
        i = j + 1;
    }
    ranks
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx) * (a - mx);
        syy += (b - my) * (b - my);
    }
    sxy / (sxx * syy).sqrt()
}

/// Spearman's rank correlation, with ties.
pub fn spearman(x: &[f64], y: &[f64]) -> f64 {
    if x.len() < 2 {
        return f64::NAN;
    }
    pearson(&ranks(x), &ranks(y))
}

/// Kendall's τ-b, with ties.
pub fn kendall(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len();
    let (mut concordant, mut discordant, mut ties_x, mut ties_y) = (0i64, 0i64, 0i64, 0i64);
    for i in 0..n {
        for j in i + 1..n {
            let dx = x[i].total_cmp(&x[j]) as i64;
            let dy = y[i].total_cmp(&y[j]) as i64;
            match (dx, dy) {
                (0, 0) => {}
                (0, _) => ties_x += 1,
                (_, 0) => ties_y += 1,
                _ if dx == dy => concordant += 1,
                _ => discordant += 1,
            }
        }
    }
    let n0 = concordant + discordant;
    let denominator = (((n0 + ties_x) * (n0 + ties_y)) as f64).sqrt();
    if denominator == 0.0 {
        f64::NAN
    } else {
        (concordant - discordant) as f64 / denominator
    }
}

/// Non-negative least squares, `min |A w - b|²` with `w ≥ 0`, by coordinate descent on the
/// normal equations (columns scaled to unit length first). `a` has one row per observation.
pub fn nnls(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let k = a.first().map_or(0, Vec::len);
    let norms: Vec<f64> = (0..k)
        .map(|j| a.iter().map(|row| row[j] * row[j]).sum::<f64>().sqrt())
        .collect();
    let mut gram = vec![vec![0.0; k]; k];
    let mut atb = vec![0.0; k];
    for (row, &target) in a.iter().zip(b) {
        for i in 0..k {
            if norms[i] == 0.0 {
                continue;
            }
            let ri = row[i] / norms[i];
            atb[i] += ri * target;
            for j in 0..k {
                if norms[j] > 0.0 {
                    gram[i][j] += ri * row[j] / norms[j];
                }
            }
        }
    }
    let mut w = vec![0.0; k];
    for _ in 0..20_000 {
        let mut change: f64 = 0.0;
        for i in 0..k {
            if gram[i][i] <= 0.0 {
                continue;
            }
            let gradient: f64 = (0..k).map(|j| gram[i][j] * w[j]).sum::<f64>() - atb[i];
            let next = (w[i] - gradient / gram[i][i]).max(0.0);
            change = change.max((next - w[i]).abs());
            w[i] = next;
        }
        if change < 1e-13 {
            break;
        }
    }
    w.iter()
        .zip(&norms)
        .map(|(w, n)| if *n > 0.0 { w / n } else { 0.0 })
        .collect()
}

// ----- Analysis -------------------------------------------------------------------------------

/// A run with both measurements.
#[derive(Clone, Debug)]
struct Joined {
    solver: String,
    /// Uniform fuel.
    fuel: u64,
    counts: Option<Counts>,
    wasm_solved: bool,
    native_ms: f64,
    native_solved: bool,
    wasm_ms: Option<f64>,
}

/// A cost model: weights per category and per function entry, continuous (fitted) or as
/// wasmtime takes them (a table, function entries always cost one).
#[derive(Clone, Debug)]
struct Model {
    name: String,
    weights: [f64; CATEGORIES + 1],
}

impl Model {
    fn of(name: &str, weights: &Weights) -> Self {
        let mut w = [1.0; CATEGORIES + 1];
        for (c, &category) in Category::ALL.iter().enumerate() {
            w[c] = f64::from(weights.get(category));
        }
        Self {
            name: name.to_string(),
            weights: w,
        }
    }

    fn fuel(&self, counts: &Counts) -> f64 {
        counts
            .ops
            .iter()
            .chain([&counts.entries])
            .zip(&self.weights)
            .map(|(&n, w)| n as f64 * w)
            .sum()
    }
}

/// The "mems" of Knuth: loads, stores and the bytes of bulk operations.
pub fn mems() -> Weights {
    let mut weights = Weights([0; CATEGORIES]);
    for c in [Category::Load, Category::Store, Category::Bulk] {
        weights.set(c, 1);
    }
    weights
}

/// The groups of the small fitted model, a weight each.
///
/// Memory accesses (loads, stores, bulk bytes), locals and constants (registers, once compiled),
/// and everything else (with the function entries). With a handful of solvers, a weight per category can't be told apart (see
/// dojo/CALIBRATION.md).
pub const GROUPS: [&[Category]; 3] = [
    &[Category::Load, Category::Store, Category::Bulk],
    &[Category::Local],
    &[
        Category::Alu,
        Category::MulDiv,
        Category::Float,
        Category::Branch,
        Category::Call,
        Category::Grow,
        Category::Other,
    ],
];

/// The group of [`GROUPS`] a column (a category, or the function entries last) belongs to.
fn group_of(column: usize) -> usize {
    Category::ALL
        .get(column)
        .and_then(|c| GROUPS.iter().position(|g| g.contains(c)))
        .unwrap_or(GROUPS.len() - 1)
}

/// How a model is fitted: a weight per [`GROUPS`] group or per category, and whether it is
/// rounded to a table wasmtime takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fit {
    grouped: bool,
    integer: bool,
}

/// Fits weights to `runs`: `w · counts ≈ time` with the smallest squared relative error,
/// non-negative. Returns a weight per category and the function entries (last).
fn fit(runs: &[&Joined], grouped: bool) -> [f64; CATEGORIES + 1] {
    let column = |c: usize| if grouped { group_of(c) } else { c };
    let width = if grouped {
        GROUPS.len()
    } else {
        CATEGORIES + 1
    };
    let a: Vec<Vec<f64>> = runs
        .iter()
        .map(|r| {
            let counts = r.counts.unwrap_or_default();
            let mut row = vec![0.0; width];
            for (c, &n) in counts.ops.iter().chain([&counts.entries]).enumerate() {
                row[column(c)] += n as f64 / r.native_ms;
            }
            row
        })
        .collect();
    let w = nnls(&a, &vec![1.0; runs.len()]);
    let mut weights = [0.0; CATEGORIES + 1];
    for (c, weight) in weights.iter_mut().enumerate() {
        *weight = w[column(c)];
    }
    weights
}

/// The table (what wasmtime takes) closest to continuous `weights`: each divided by a unit and
/// rounded. The unit is the mean time of an operation on `runs` divided by 1 to 16 (so an
/// average operation costs that many units), whichever fits `runs` best; the smallest wins a
/// tie. Function entries always cost one unit.
fn integer_weights(weights: &[f64; CATEGORIES + 1], runs: &[&Joined]) -> Weights {
    let (mut time, mut ops) = (0.0, 0.0);
    for r in runs {
        let counts = r.counts.unwrap_or_default();
        for (c, &n) in counts.ops.iter().chain([&counts.entries]).enumerate() {
            time += weights[c] * n as f64;
            ops += n as f64;
        }
    }
    let mean = time / ops.max(1.0);
    let mut best = (f64::INFINITY, Weights::UNIFORM);
    for k in 1..=16 {
        let unit = mean / f64::from(k);
        let mut table = Weights::ZERO;
        for (weight, fitted) in table.0.iter_mut().zip(weights) {
            *weight = (fitted / unit).round().clamp(0.0, 255.0) as u8;
        }
        let error = log_error(&Model::of("", &table), runs).1;
        if error < best.0 - 1e-9 {
            best = (error, table);
        }
    }
    best.1
}

/// A model fitted to `runs` the way `kind` says.
fn fitted_model(name: &str, runs: &[&Joined], kind: Fit) -> (Model, Option<Weights>) {
    let weights = fit(runs, kind.grouped);
    if kind.integer {
        let table = integer_weights(&weights, runs);
        (Model::of(name, &table), Some(table))
    } else {
        (
            Model {
                name: name.to_string(),
                weights,
            },
            None,
        )
    }
}

/// The scale (ms per fuel unit, the median ratio) and the root mean square log error of a
/// model on `runs` after that scale.
fn log_error(model: &Model, runs: &[&Joined]) -> (f64, f64) {
    let ratios: Vec<f64> = runs
        .iter()
        .filter_map(|r| {
            let fuel = model.fuel(&r.counts?);
            (fuel > 0.0).then(|| (r.native_ms / fuel).ln())
        })
        .collect();
    let scale = median(&ratios);
    let rms =
        (ratios.iter().map(|r| (r - scale).powi(2)).sum::<f64>() / ratios.len() as f64).sqrt();
    (scale.exp(), rms)
}

/// What the analysis found, as numbers (the report is [`Analysis::markdown`]).
#[derive(Clone, Debug)]
pub struct Analysis {
    /// Solvers in the order given.
    pub solvers: Vec<String>,
    /// Runs with both measurements, solved both ways, at least `min_ms` natively.
    pub runs: usize,
    pub min_ms: f64,
    /// Per model: its name, weights (`None` for continuous ones) and per solver the Spearman ρ
    /// and Kendall τ of fuel against native time, pooled ρ and τ, RMS log error (fitted on all
    /// runs) and the RMS log error and per-solver bias when the scale is fitted without the
    /// solver.
    pub models: Vec<ModelResult>,
    /// Per solver: median WebAssembly / native time.
    pub overhead: Vec<(String, f64, f64, f64)>,
    /// PAR-2 rankings: by native time, by fuel per model; Kendall τ against the native one.
    pub rankings: Vec<Ranking>,
    /// The time limit of the native ranking, ms.
    pub limit_ms: f64,
    /// Instances by family in the study.
    pub families: BTreeMap<String, usize>,
    pub total_runs: usize,
}

/// A ranking of the solvers by PAR-2 (in units of the limit), and its Kendall τ against the
/// ranking by native time.
#[derive(Clone, Debug)]
pub struct Ranking {
    pub name: String,
    pub par2: Vec<(String, f64)>,
    pub tau: f64,
}

#[derive(Clone, Debug)]
pub struct ModelResult {
    pub name: String,
    pub table: Option<Weights>,
    /// The weights on all runs: in ms per operation for continuous models, per category and
    /// the function entries last.
    pub weights: [f64; CATEGORIES + 1],
    pub per_solver: Vec<(String, usize, f64, f64)>,
    pub pooled_rho: f64,
    pub pooled_tau: f64,
    pub rms: f64,
    /// Leave one solver out: the weights (for fitted models) and the scale come from the
    /// others; RMS log error over all held-out runs, and each solver's median log ratio.
    pub loso_rms: f64,
    pub loso_bias: Vec<(String, f64)>,
}

/// Analyzes the measurements: `budget` is the fuel budget of the leaderboard (the native
/// ranking gets the time the median run needs for that much uniform fuel).
pub fn analyze(
    fuel: &[FuelRow],
    time: &[TimeRow],
    budget: u64,
    min_ms: f64,
) -> Result<Analysis, String> {
    let times: BTreeMap<(&str, &str), &TimeRow> = time
        .iter()
        .map(|t| ((t.solver.as_str(), t.instance.as_str()), t))
        .collect();
    let mut solvers: Vec<String> = Vec::new();
    let mut families: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut joined = Vec::new();
    for f in fuel {
        if !solvers.contains(&f.solver) {
            solvers.push(f.solver.clone());
        }
        families
            .entry(f.family.clone())
            .or_default()
            .insert(f.instance.as_str());
        let Some(t) = times.get(&(f.solver.as_str(), f.instance.as_str())) else {
            return Err(format!("no time for {} on {}", f.solver, f.instance));
        };
        joined.push(Joined {
            solver: f.solver.clone(),
            fuel: f.fuel,
            counts: f.counts,
            wasm_solved: f.solved,
            native_ms: t.median_ms(),
            native_solved: t.native_solved,
            wasm_ms: t.wasm_ms,
        });
    }
    let usable: Vec<&Joined> = joined
        .iter()
        .filter(|r| r.wasm_solved && r.native_solved && r.counts.is_some() && r.native_ms >= min_ms)
        .collect();
    if usable.len() < 10 {
        return Err(format!("only {} usable runs", usable.len()));
    }

    let mut models: Vec<(Model, Option<Weights>, Option<Fit>)> = vec![
        (
            Model::of("uniform", &Weights::UNIFORM),
            Some(Weights::UNIFORM),
            None,
        ),
        (Model::of("mems", &mems()), Some(mems()), None),
    ];
    if crate::fuel::MODEL != Weights::UNIFORM {
        models.push((
            Model::of("leaderboard", &crate::fuel::MODEL),
            Some(crate::fuel::MODEL),
            None,
        ));
    }
    for (name, grouped, integer) in [
        ("fitted, 3 groups", true, false),
        ("fitted, 3 groups (table)", true, true),
        ("fitted, per category", false, false),
        ("fitted, per category (table)", false, true),
    ] {
        let kind = Fit { grouped, integer };
        let (model, table) = fitted_model(name, &usable, kind);
        models.push((model, table, Some(kind)));
    }
    let mut results = Vec::new();
    for (model, table, refit) in &models {
        let mut per_solver = Vec::new();
        for solver in &solvers {
            let runs: Vec<&&Joined> = usable.iter().filter(|r| &r.solver == solver).collect();
            let x: Vec<f64> = runs
                .iter()
                .map(|r| model.fuel(&r.counts.unwrap_or_default()))
                .collect();
            let y: Vec<f64> = runs.iter().map(|r| r.native_ms).collect();
            per_solver.push((
                solver.clone(),
                runs.len(),
                spearman(&x, &y),
                kendall(&x, &y),
            ));
        }
        let x: Vec<f64> = usable
            .iter()
            .map(|r| model.fuel(&r.counts.unwrap_or_default()))
            .collect();
        let y: Vec<f64> = usable.iter().map(|r| r.native_ms).collect();
        let rms = log_error(model, &usable).1;
        // Leave one solver out
        let mut held_out_errors = Vec::new();
        let mut loso_bias = Vec::new();
        for solver in &solvers {
            let training: Vec<&Joined> = usable
                .iter()
                .copied()
                .filter(|r| &r.solver != solver)
                .collect();
            let testing: Vec<&Joined> = usable
                .iter()
                .copied()
                .filter(|r| &r.solver == solver)
                .collect();
            if training.len() < 5 || testing.is_empty() {
                continue;
            }
            let trained = match refit {
                Some(kind) => fitted_model("", &training, *kind).0,
                None => model.clone(),
            };
            let scale = log_error(&trained, &training).0.ln();
            let errors: Vec<f64> = testing
                .iter()
                .filter_map(|r| {
                    let fuel = trained.fuel(&r.counts?);
                    (fuel > 0.0).then(|| (r.native_ms / fuel).ln() - scale)
                })
                .collect();
            loso_bias.push((solver.clone(), median(&errors)));
            held_out_errors.extend(errors);
        }
        let loso_rms = (held_out_errors.iter().map(|e| e * e).sum::<f64>()
            / held_out_errors.len().max(1) as f64)
            .sqrt();
        results.push(ModelResult {
            name: model.name.clone(),
            table: *table,
            weights: model.weights,
            per_solver,
            pooled_rho: spearman(&x, &y),
            pooled_tau: kendall(&x, &y),
            rms,
            loso_rms,
            loso_bias,
        });
    }

    // Overhead of the sandbox
    let mut overhead = Vec::new();
    for solver in &solvers {
        let ratios: Vec<f64> = usable
            .iter()
            .filter(|r| &r.solver == solver)
            .filter_map(|r| Some(r.wasm_ms? / r.native_ms))
            .collect();
        if !ratios.is_empty() {
            overhead.push((
                solver.clone(),
                median(&ratios),
                quantile(&ratios, 0.25),
                quantile(&ratios, 0.75),
            ));
        }
    }

    // Rankings by PAR-2: native time within the time the median run needs for `budget`
    let uniform = Model::of("uniform", &Weights::UNIFORM);
    let ms_per_fuel = log_error(&uniform, &usable).0;
    let limit_ms = budget as f64 * ms_per_fuel;
    let par2 = |score: &dyn Fn(&Joined) -> Option<f64>, limit: f64| -> Vec<(String, f64)> {
        let mut out: Vec<(String, f64)> = solvers
            .iter()
            .map(|s| {
                let runs: Vec<&Joined> = joined.iter().filter(|r| &r.solver == s).collect();
                let total: f64 = runs
                    .iter()
                    .map(|r| score(r).filter(|&v| v <= limit).unwrap_or(2.0 * limit))
                    .sum();
                (s.clone(), total / runs.len().max(1) as f64 / limit)
            })
            .collect();
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out
    };
    let native = par2(&|r| r.native_solved.then_some(r.native_ms), limit_ms);
    let position = |ranking: &[(String, f64)]| -> Vec<f64> {
        solvers
            .iter()
            .map(|s| {
                ranking
                    .iter()
                    .find(|(n, _)| n == s)
                    .map_or(f64::NAN, |(_, v)| *v)
            })
            .collect()
    };
    let native_scores = position(&native);
    let mut rankings = vec![Ranking {
        name: "native time".into(),
        par2: native,
        tau: 1.0,
    }];
    for (model, _, _) in &models {
        // The budget in this model's units: as much as the uniform budget, by the median ratio
        let ratios: Vec<f64> = usable
            .iter()
            .filter_map(|r| Some(model.fuel(&r.counts?) / r.fuel as f64))
            .collect();
        let model_budget = budget as f64 * median(&ratios);
        let ranking = par2(
            &|r| {
                if !r.wasm_solved {
                    return None;
                }
                // Runs without counts didn't finish within the cap: unsolved
                r.counts.map(|c| model.fuel(&c))
            },
            model_budget,
        );
        let tau = kendall(&native_scores, &position(&ranking));
        rankings.push(Ranking {
            name: format!("fuel, {}", model.name),
            par2: ranking,
            tau,
        });
    }

    Ok(Analysis {
        solvers,
        runs: usable.len(),
        min_ms,
        models: results,
        overhead,
        rankings,
        limit_ms,
        families: families.into_iter().map(|(f, i)| (f, i.len())).collect(),
        total_runs: joined.len(),
    })
}

fn weights_text(table: &Weights) -> String {
    Category::ALL
        .iter()
        .map(|&c| format!("{} {}", c.id(), table.get(c)))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Analysis {
    /// The report, as Markdown tables.
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{} runs of {} solvers on {} formulas; {} solved both natively and in the sandbox \
             with at least {} ms natively are used for the correlations and the fits.\n",
            self.total_runs,
            self.solvers.len(),
            self.families.values().sum::<usize>(),
            self.runs,
            self.min_ms
        );
        let _ = writeln!(out, "### Fuel against native time\n");
        let _ = write!(out, "| model |");
        for s in &self.solvers {
            let _ = write!(out, " {s} |");
        }
        let _ = writeln!(
            out,
            " pooled | RMS log error | held out: RMS | held out: bias spread |"
        );
        let _ = write!(out, "|---|");
        for _ in &self.solvers {
            let _ = write!(out, "---|");
        }
        let _ = writeln!(out, "---|---|---|---|");
        for m in &self.models {
            let _ = write!(out, "| {} |", m.name);
            for (_, n, rho, tau) in &m.per_solver {
                let _ = write!(out, " ρ {rho:.3} / τ {tau:.3} (n={n}) |");
            }
            let biases: Vec<f64> = m.loso_bias.iter().map(|(_, b)| *b).collect();
            let spread = biases.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - biases.iter().copied().fold(f64::INFINITY, f64::min);
            let _ = writeln!(
                out,
                " ρ {:.3} / τ {:.3} | {:.3} | {:.3} | {:.3} |",
                m.pooled_rho, m.pooled_tau, m.rms, m.loso_rms, spread
            );
        }
        let _ = writeln!(
            out,
            "\nPer-solver bias when the solver is left out (median log of time / predicted time; 0 is perfect, 0.69 is twice as slow as predicted):\n"
        );
        let _ = write!(out, "| model |");
        for s in &self.solvers {
            let _ = write!(out, " {s} |");
        }
        let _ = writeln!(out);
        let _ = write!(out, "|---|");
        for _ in &self.solvers {
            let _ = write!(out, "---|");
        }
        let _ = writeln!(out);
        for m in &self.models {
            let _ = write!(out, "| {} |", m.name);
            for s in &self.solvers {
                match m.loso_bias.iter().find(|(n, _)| n == s) {
                    Some((_, b)) => {
                        let _ = write!(out, " {b:+.3} |");
                    }
                    None => out.push_str(" – |"),
                }
            }
            let _ = writeln!(out);
        }
        let _ = writeln!(out, "\n### Weights\n");
        for m in &self.models {
            if let Some(table) = &m.table {
                let _ = writeln!(out, "- {}: {}", m.name, weights_text(table));
            } else {
                let weights: Vec<String> = Category::ALL
                    .iter()
                    .map(|c| c.id())
                    .chain(["function entry"])
                    .zip(&m.weights)
                    .map(|(name, w)| format!("{name} {:.3}", w * 1e6))
                    .collect();
                let _ = writeln!(
                    out,
                    "- {}, ns per operation: {}",
                    m.name,
                    weights.join(", ")
                );
            }
        }
        let _ = writeln!(
            out,
            "\n### Rankings by PAR-2\n\nNative time limit {:.0} ms (what the median run needs for the budget's uniform fuel); fuel budgets scaled the same way per model.\n",
            self.limit_ms
        );
        for Ranking { name, par2, tau } in &self.rankings {
            let order: Vec<String> = par2.iter().map(|(s, v)| format!("{s} {v:.3}")).collect();
            let _ = writeln!(out, "- {name} (τ {tau:.3}): {}", order.join(", "));
        }
        if !self.overhead.is_empty() {
            let _ = writeln!(
                out,
                "\n### Sandbox against native\n\n| solver | median | quartiles |\n|---|---|---|"
            );
            for (s, m, q1, q3) in &self.overhead {
                let _ = writeln!(out, "| {s} | {m:.2}× | {q1:.2}–{q3:.2}× |");
            }
        }
        let _ = writeln!(out, "\n### Formulas\n");
        for (family, n) in &self.families {
            let _ = writeln!(out, "- {family}: {n}");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::{Spec, generate_with};
    use crate::{Answer, Family};

    fn tiny_set() -> Vec<Instance> {
        generate_with(
            "calibration test",
            &[
                Spec {
                    family: Family::Factoring,
                    size: 12,
                    want: Some(Answer::Sat),
                },
                Spec {
                    family: Family::Pigeonhole,
                    size: 5,
                    want: Some(Answer::Unsat),
                },
            ],
        )
    }

    fn minisat(native: Option<PathBuf>) -> Solver {
        Solver {
            name: "minisat".into(),
            wasm: crate::reference::MINISAT.to_vec(),
            native,
        }
    }

    #[test]
    fn statistics() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
        assert!(median(&[]).is_nan());
        assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.25), 2.0);
        assert_eq!(quantile(&[1.0, 3.0], 0.5), 2.0);
        assert!(quantile(&[], 0.5).is_nan());
        assert_eq!(ranks(&[10.0, 20.0, 20.0, 5.0]), [2.0, 3.5, 3.5, 1.0]);
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((spearman(&x, &[2.0, 4.0, 9.0, 16.0, 100.0]) - 1.0).abs() < 1e-12);
        assert!((spearman(&x, &[5.0, 4.0, 3.0, 2.0, 1.0]) + 1.0).abs() < 1e-12);
        assert!(spearman(&[1.0], &[1.0]).is_nan());
        assert!((kendall(&x, &[1.0, 3.0, 2.0, 4.0, 5.0]) - 0.8).abs() < 1e-12);
        // τ-b with ties: one tie in y
        let tau = kendall(&[1.0, 2.0, 3.0], &[1.0, 1.0, 2.0]);
        assert!((tau - 2.0 / 6.0f64.sqrt()).abs() < 1e-12, "{tau}");
        assert!(kendall(&[1.0, 1.0], &[2.0, 3.0]).is_nan());
    }

    /// The least squares fit finds weights that generated the data, never negative ones.
    #[test]
    fn nnls_recovers_weights() {
        let a: Vec<Vec<f64>> = (0..30)
            .map(|i| {
                let i = f64::from(i);
                vec![
                    i + 1.0,
                    (i * 7.0) % 11.0 + 1.0,
                    1e6 * ((i * 3.0) % 5.0 + 1.0),
                    0.0,
                ]
            })
            .collect();
        let b: Vec<f64> = a
            .iter()
            .map(|r| 2.0 * r[0] + 0.5 * r[1] + 3e-6 * r[2])
            .collect();
        let w = nnls(&a, &b);
        for (got, want) in w.iter().zip([2.0, 0.5, 3e-6, 0.0]) {
            assert!((got - want).abs() <= 1e-6 * want.max(1e-9) + 1e-12, "{w:?}");
        }
        // A negative relation gets weight 0
        let b: Vec<f64> = a.iter().map(|r| r[0] - 0.1 * r[1]).collect();
        assert!(nnls(&a, &b).iter().all(|&w| w >= 0.0));
    }

    #[test]
    fn csv_round_trips() {
        let mut counts = Counts {
            entries: 3,
            ..Counts::default()
        };
        counts.ops[0] = 7;
        let row = FuelRow {
            solver: "s".into(),
            instance: "dev/hashi-01".into(),
            family: "hashi".into(),
            expected: Answer::Sat,
            solved: true,
            fuel: 42,
            counts: Some(counts),
        };
        assert_eq!(FuelRow::parse(&row.csv()), Ok(row.clone()));
        let censored = FuelRow {
            counts: None,
            solved: false,
            ..row
        };
        assert_eq!(FuelRow::parse(&censored.csv()), Ok(censored));
        assert_eq!(fuel_header().split(',').count(), 7 + CATEGORIES);
        assert!(FuelRow::parse("a,b").is_err());
        assert!(FuelRow::parse(&"x,".repeat(16)[..31]).is_err());
        let time = TimeRow {
            solver: "s".into(),
            instance: "i".into(),
            native_ms: vec![1.5, 2.25, 2.0],
            native_solved: true,
            wasm_ms: Some(3.0),
        };
        assert_eq!(time.median_ms(), 2.0);
        assert_eq!(TimeRow::parse(&time.csv()), Ok(time.clone()));
        let none = TimeRow {
            native_ms: vec![],
            wasm_ms: None,
            ..time
        };
        assert_eq!(TimeRow::parse(&none.csv()), Ok(none));
        assert!(TimeRow::parse("a,b,c").is_err());
        let text = format!("{TIME_HEADER}\n{}\n\n", time_row().csv());
        assert_eq!(read_csv(&text, TimeRow::parse).unwrap().len(), 1);
    }

    fn time_row() -> TimeRow {
        TimeRow {
            solver: "a".into(),
            instance: "b".into(),
            native_ms: vec![1.0],
            native_solved: false,
            wasm_ms: None,
        }
    }

    /// Counting each category separately adds up to the uniform fuel exactly: the categories
    /// split the operators, nothing is counted twice or left out.
    #[test]
    fn categories_add_up_to_uniform_fuel() {
        let set = tiny_set();
        let options = Options {
            threads: 2,
            set_seed: SEED.into(),
            ..Options::default()
        };
        let rows = measure_fuel(&[minisat(None)], &set, &options, |_| {}).unwrap();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert!(row.solved, "{row:?}");
            let counts = row.counts.unwrap();
            assert_eq!(
                counts.ops.iter().sum::<u64>() + counts.entries,
                row.fuel,
                "{row:?}"
            );
            assert_eq!(Weights::UNIFORM.fuel(&counts), row.fuel);
            assert!(counts.get(Category::Load) > 0 && counts.get(Category::Branch) > 0);
            assert!(counts.entries > 0 && counts.get(Category::Other) < counts.entries);
        }
        // Capped: nothing to count
        let capped = Options {
            limits: Limits {
                fuel: 1000,
                ..Limits::default()
            },
            ..options
        };
        let rows = measure_fuel(&[minisat(None)], &set, &capped, |_| {}).unwrap();
        assert!(rows.iter().all(|r| !r.solved && r.counts.is_none()));
    }

    /// Native runs: a script that answers like a solver, one that answers wrong and one that
    /// runs into the timeout.
    #[cfg(unix)]
    #[test]
    fn times_native_runs() {
        let scratch = TempDir::new(&std::env::temp_dir(), "dojo-calibrate-test").unwrap();
        let script = |name: &str, body: &str| {
            let path = scratch.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\ncat >/dev/null\n{body}\n")).unwrap();
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
            path
        };
        let set = tiny_set();
        let unsat = script("unsat", "echo 's UNSATISFIABLE'");
        let slow = script("slow", "sleep 5");
        let limits = TimeLimits {
            timeout: Duration::from_millis(300),
            sandbox: Limits::default(),
        };
        let all = |_: &Solver, _: &Instance| {
            Some(Plan {
                runs: 2,
                wasm: true,
            })
        };
        let rows = measure_time(
            &[minisat(Some(unsat))],
            &set,
            &limits,
            &all,
            &scratch.path().join("runs"),
            |_| {},
        )
        .unwrap();
        // UNSAT is right for the pigeonhole formula only
        assert_eq!(
            rows.iter().map(|r| r.native_solved).collect::<Vec<_>>(),
            [false, true]
        );
        assert!(
            rows.iter()
                .all(|r| r.native_ms.len() == 2 && r.wasm_ms.is_some())
        );
        let only_first = |_: &Solver, i: &Instance| {
            (i.expected == Answer::Sat).then_some(Plan {
                runs: 3,
                wasm: false,
            })
        };
        let rows = measure_time(
            &[minisat(Some(slow))],
            &set,
            &limits,
            &only_first,
            &scratch.path().join("runs"),
            |_| {},
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        // Timed out: not repeated, not solved
        assert_eq!(rows[0].native_ms.len(), 1);
        assert!(!rows[0].native_solved && rows[0].wasm_ms.is_none());
        assert!(rows[0].native_ms[0] < 3000.0);
    }

    /// Synthetic solvers whose native time follows loads at 2 ns and everything else at 0.5 ns,
    /// with solver-specific mixes: fitting recovers the shape, the fitted model beats the uniform
    /// one, and the report has every section.
    #[test]
    fn analysis_of_synthetic_runs() {
        let mut fuel = Vec::new();
        let mut time = Vec::new();
        for (s, loads_per_op) in [("a", 0.1), ("b", 0.3), ("c", 0.5), ("d", 0.2)] {
            for i in 0..40 {
                let ops = 1_000_000.0 * f64::from(i + 1) * if s == "c" { 3.0 } else { 1.0 };
                let mut counts = Counts::default();
                counts.ops[Category::Load as usize] = (ops * loads_per_op) as u64;
                counts.ops[Category::Alu as usize] = (ops * (1.0 - loads_per_op)) as u64;
                let ms = (2e-6 * counts.get(Category::Load) as f64
                    + 0.5e-6 * counts.get(Category::Alu) as f64)
                    * (1.0 + 0.02 * f64::from(i % 3));
                let instance = format!("set/f{i}");
                let solved = i < 38;
                fuel.push(FuelRow {
                    solver: s.into(),
                    instance: instance.clone(),
                    family: if i % 2 == 0 { "x" } else { "y" }.into(),
                    expected: Answer::Sat,
                    solved,
                    fuel: Weights::UNIFORM.fuel(&counts),
                    counts: solved.then_some(counts),
                });
                time.push(TimeRow {
                    solver: s.into(),
                    instance,
                    native_ms: vec![ms],
                    native_solved: solved,
                    wasm_ms: Some(ms * 2.0),
                });
            }
        }
        let analysis = analyze(&fuel, &time, 20_000_000, 0.0).unwrap();
        assert_eq!(analysis.solvers, ["a", "b", "c", "d"]);
        assert_eq!(analysis.runs, 4 * 38);
        let model = |name: &str| analysis.models.iter().find(|m| m.name == name).unwrap();
        assert!(model("fitted, per category").loso_rms < model("uniform").loso_rms / 3.0);
        assert!(model("fitted, 3 groups").loso_rms < model("uniform").loso_rms / 3.0);
        assert!(model("fitted, per category (table)").loso_rms < model("uniform").loso_rms);
        let table = model("fitted, 3 groups (table)").table.unwrap();
        assert!(table.get(Category::Load) >= 3 * table.get(Category::Alu).max(1));
        assert_eq!(table.get(Category::Load), table.get(Category::Store));
        assert!((analysis.overhead[0].1 - 2.0).abs() < 1e-9);
        assert_eq!(analysis.families.len(), 2);
        for ranking in &analysis.rankings {
            assert_eq!(ranking.par2.len(), 4);
        }
        let report = analysis.markdown();
        for section in [
            "### Fuel against native time",
            "### Weights",
            "### Rankings by PAR-2",
            "### Sandbox against native",
            "| a |",
        ] {
            assert!(report.contains(section), "{section}\n{report}");
        }
        // Missing times or too few runs
        assert!(analyze(&fuel, &time[1..], 1, 0.0).is_err());
        assert!(analyze(&fuel[..5], &time, 1, 0.0).is_err());
    }
}
