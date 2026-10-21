# Does fuel measure how long a solver takes?

The SAT Dojo scores solvers by *fuel*: the WebAssembly operators a solver executes, counted by
wasmtime, instead of seconds. Fuel makes every score exactly reproducible on any machine, at any
load. The price is that it must still mean something: a solver that uses less fuel should also be
faster on a real machine, and the ranking by fuel should be the ranking by time.

This document checks that with measurements, answers the published objection to instruction
counts, and explains the cost model the leaderboard uses. All data and the tool that produced
them are in the repository, see [Reproducing](#reproducing).

## The objection

Coutelier, Rath, Rawson, Biere and Kovács [1] argue that wall time is unsuitable for deterministic
progress measures because it depends on the machine and its load, and that "instructions easily
change when refactoring code". Kissat and CaDiCaL therefore count *ticks*, roughly the cache lines
a propagation touches; Knuth counts *mems*, 64-bit memory accesses [2]. A 2025 study found that
mems follow running time well within one program but poorly across programs [3]. As far as we
know, nobody has published how well instruction counts, ticks or mems predict the running time of
SAT solvers, and nothing at all for SAT solvers compiled to WebAssembly.

The objection has two parts, and they matter differently here:

1. *Instructions are not time*: a load that misses the cache costs a hundred times more than an
   addition. If solvers differed systematically in their mix of cheap and expensive operations,
   counting every operator as one would favour the memory-hungry ones. This is measurable, and a
   cost model that weights operators can fix it.
2. *Instructions change when code is refactored*: true, and intended. The Dojo measures the
   submitted program, not an algorithm; a refactoring that saves operators makes the program
   faster on a real machine too, as long as part 1 holds. What fuel must not do is reward
   changes that save operators but cost time, which is again part 1.

## Method

**Solvers.** Seven, from the same sources in both forms:

| solver | what it is | WebAssembly | native |
|---|---|---|---|
| MiniSat | last commit of its repository, DRAT added | wasi-sdk 34 (clang 23), `-O3` | gcc 14.2, `-O3` |
| CaDiCaL 3.0.1 | state of the art, LRAT | wasi-sdk 34, `-O3` | gcc 14.2, `-O3` |
| Kissat 4.0.4 | state of the art, DRAT | wasi-sdk 34, `-O3` | gcc 14.2, `-O3` |
| splr | a CDCL solver in Rust (house entry) | rustc, release | rustc, release |
| Hashi CDCL, Hashi DPLL | the website's solvers (house entries) | rustc, release | rustc, release |
| starter | the minimal DPLL of `dojo/starter` | rustc, release | rustc, release |

The reference solvers are the committed `dojo/reference/*.wasm` and `dojo/reference/build.sh
--native` (in the `rust:1.99-trixie` image). Every solver writes its proof in both forms (natively
to a file on disk), so both do the same work.

**Formulas.** 180: the dev set (36) and the hidden sets of the seeds `calibration-1`,
`calibration-2` and `calibration-3` (48 each, `dojo export --hidden`), so all eight dev families
at dev and larger sizes, the new families (factoring, miters, colouring) and each held-out family
once (Tseitin, Latin squares with holes, Hamiltonian cycles). There was no community pool to
add, and no GBD instances (see the limitations).

**Fuel** (`dojo calibrate fuel`). Every solver on every formula in the sandbox with the uniform
table (one unit per operator, wasmtime's default and the rules up to version 3), capped at 40
billion (four budgets of the time), proofs checked by drat-trim and cake_lpr. A run that finishes
runs again once per category with a table that charges only that category, and once with a table
of zeros: wasmtime charges one unit for every function entry whatever the table says, the zero
table counts those. That gives, for each run, how many loads, stores, integer operations,
multiplications and divisions, float operations, branches, calls, local/global/constant
operations, bulk-memory bytes, `memory.grow` pages and other operators it executed
(`dojo/src/fuel.rs` has the exact split). A test checks that the categories add up to the uniform
fuel exactly. Runs are deterministic, so they ran in parallel.

**Native time** (`dojo calibrate time`). On an Intel Core Ultra X7 358H (4 performance cores up to
4.7 GHz, 12 efficiency cores), Fedora 44, Linux 7.2.7: every run pinned to performance core 3
(`taskset -c 3`), one at a time, the median wall time of three runs (one if it ran into the 5 s
timeout), spawned through coreutils `timeout`, stdout and the proof to files. The same runs in the
sandbox once more for the overhead. The machine is shared with other work: each solver's block
ran while holding the machine-wide build lock (no compilation in parallel) and started only once
the load average was below 2. A first attempt was discarded because a stray single-threaded job
of another session was scheduled on core 3; it was moved off that core and everything was timed
again. Turbo stayed on (no root to disable it) and the governor was `powersave` with
`balance_performance`. The conditions of every block are in `calibration/conditions.log`. Noise
was low: of the 511 runs used below, the three timings differ by a median of 0.6 % (90th
percentile 1.6 %, 16 runs above 10 %).

**Analysis** (`dojo calibrate analyze`). The runs solved both natively and in the sandbox, with at
least 20 ms natively (shorter ones are dominated by starting the process): 511 of 1260.

- Correlation of fuel and native time within each solver and pooled: Spearman's ρ and Kendall's
  τ-b.
- Prediction across solvers: fuel times a scale (ms per fuel unit, the median ratio) predicts the
  time. *Held out* means the scale comes from the other six solvers: the root mean square of
  log(time / prediction) over all held-out runs, and per solver its median (*bias*). A bias of
  +0.35 means the solver is 1.4 times slower than its fuel says; the *spread* is the largest
  minus the smallest bias, the factor between the solver that fuel flatters most and least.
- Rankings by PAR-2: natively with a time limit of 566 ms, what the median run needs for the old
  budget of 10 billion uniform fuel; by fuel with that budget scaled into each model's units by
  the median ratio. Kendall's τ between the native ranking and each fuel ranking.
- Fitted models: non-negative least squares of the counts against native time (squared relative
  error), either one weight per category or one per group (memory accesses: loads, stores, bulk
  bytes; locals and constants; everything else). *Table* versions are rounded to the integer
  weights wasmtime takes (`OperatorCost` has one `u8` per operator). Leave-one-solver-out: for
  the held-out numbers the weights are fitted again without the solver, so a model can't
  memorise the seven solvers.

## Results

The full report is `calibration/report.md`; the essentials:

**Within a solver, fuel tracks time closely.** Uniform fuel against native time:

| | MiniSat | CaDiCaL | Kissat | splr | CDCL | DPLL | starter | pooled |
|---|---|---|---|---|---|---|---|---|
| Spearman ρ | 0.948 | 0.929 | 0.976 | 0.983 | 0.913 | 0.990 | 0.996 | 0.957 |
| Kendall τ | 0.849 | 0.823 | 0.896 | 0.906 | 0.788 | 0.933 | 0.963 | 0.820 |

No cost model changes these much (they range over 0.91 to 1.00 for every model): within one
program the mix of operators hardly changes from formula to formula, so any weighting gives
nearly the same order. That matches what was found for mems [3].

**Across solvers, fuel is off by up to a factor of 2.7, and weights help a little.**

| model | held out: RMS log error | held out: bias spread | pooled ρ |
|---|---|---|---|
| uniform (rules ≤ 3) | 0.411 | 0.999 | 0.957 |
| mems (loads, stores, bulk bytes) | 0.408 | 0.690 | 0.958 |
| **leaderboard (rules 4): memory 3, locals 0, rest 1** | **0.372** | **0.828** | **0.963** |
| fitted, 3 groups (refitted per fold) | 0.384 | 0.917 | 0.962 |
| fitted, per category (refitted per fold) | 0.376 | 0.608 | 0.972 |

Per-solver bias with the solver held out (log of time / predicted time):

| model | MiniSat | CaDiCaL | Kissat | splr | CDCL | DPLL | starter |
|---|---|---|---|---|---|---|---|
| uniform | +0.35 | +0.02 | +0.07 | +0.43 | −0.57 | −0.37 | −0.39 |
| leaderboard | +0.32 | −0.03 | −0.01 | +0.45 | −0.38 | −0.19 | −0.21 |

Under uniform fuel the simple Rust solvers (CDCL, DPLL, starter) look 1.5 to 1.8 times slower
than they are, and MiniSat and splr 1.4 to 1.5 times faster. A good part of it is how the code
uses memory: Hashi CDCL and DPLL execute about half as many loads per operator as the others (4.5 %
of their operators against 7 to 10 %), and every solver spends 54 to 60 % of its operators on
locals and constants, which a compiler keeps in registers. The fitted three-group model, which
generalises to the held-out solvers, puts 2.6 times the weight of an ordinary operator on a memory
access and none on locals. Its rounded form, loads and stores 3, locals 0, everything else 1,
lowers the held-out error by a tenth and narrows the spread between solvers from 2.7× to 2.3×.

Knuth's mems alone don't predict better overall (the held-out error is the same as uniform's);
they narrow the spread but break the DPLL solvers' within-solver order (ρ 0.971 against 0.990).

A weight per category fits the seven solvers best and still generalises a little better, but its
weights aren't plausible: 255 units for a float operation and for a page of `memory.grow`, 0 for a
load. With seven solvers there are effectively seven data points for eleven weights (each solver's
mix barely varies), so it explains which solver uses floats rather than what floats cost. We
don't use it.

**The ranking is the native one in every model.** By native PAR-2: MiniSat 0.158, CaDiCaL 0.195,
Kissat 0.210, splr 0.479, CDCL 1.168, starter 1.215, DPLL 1.370 (in units of the time limit).
Every fuel model gives the same order except that Kissat and CaDiCaL swap (0.212 and 0.219 under
the leaderboard's model), τ = 0.905 for all of them. On these formulas, small enough for the
Dojo's budget, MiniSat beats CaDiCaL and Kissat in both measures: their inprocessing pays off on
larger formulas.

**The sandbox costs 1.5 to 1.7 times native time** (median per solver: MiniSat 1.46, splr 1.54,
Kissat 1.62, DPLL 1.65, CaDiCaL 1.70, CDCL 1.74; the starter 2.35), fuel metering included,
somewhat more than the 45–55 % that Jangda et al. measured for SPEC without metering [4].

## Decision

The leaderboard uses the rounded three-group model since rules version 4 (`fuel::MODEL`):

| | loads, stores | locals, globals, constants | other operators | bulk memory | `memory.grow` | function entry |
|---|---|---|---|---|---|---|
| fuel | 3 | 0 | 1 | 1 per byte | 1 + 1 per page | 1 |

Bulk-memory bytes stay at 1: the group fit put them with the memory accesses, but a byte of
`memory.copy` is far cheaper than a load (the native copy moves 32 bytes at a time), and 1 per
byte predicts slightly better (0.372 against 0.375). The improvement over uniform is small, a
tenth of the error, and no ranking changes; we take it because it is consistent across the
held-out solvers and because it removes what a compiler hides anyway: locals and constants cost
nothing natively, and code with more of them is not slower.

The budget went from 10 to 7 billion: under the new model a run costs 0.68 times its uniform fuel
(the median over all runs), so the same runs finish within it as before. Rules version 4 makes
the website evaluate every entry again; the "too easy" filter for community benchmarks keeps a
tenth of the budget.

**Larger budgets.** Half a second per formula is a short race: the SAT Competition gives 5,000
seconds, and solvers with heavy inprocessing pay for it before it pays off. Since rules version 5
every run goes on past the budget, and the leaderboard also ranks by smaller and larger budgets
from the same runs. On nine hidden sets like a season's (`calibration-1` to `-3`, `hidden-4` to
`-9`), MiniSat leads Kissat and CaDiCaL at the budget on every one; Kissat overtakes MiniSat for
good between 2.4 and 3.9 times the budget on five of them and for a while on three more, CaDiCaL
only for a while on four. Beyond ten times nothing changes among the three: each solves all 48
formulas of every set by then.

Rules version 5 went on to ten times the budget, version 6 goes to four times
(`dojo::RUN_BUDGET`, 28 billion): that keeps all five crossovers for good, at less than half the
cost. Ten times turned out far too slow on the worker. Its four shared CPUs (Fly.io
`shared-cpu-4x`) may run 5 ms of every 80 ms each, a quarter of one core between the four, once a
small burst balance is used up, and a machine that starts on demand has little of it. On
2026-10-21 Hashi CDCL took 977.5 s there at ten times the budget and splr about 476 s; on four
fast desktop cores (`taskset -c 0-3`, four runs at a time, 4.3 to 8.5 billion fuel per second
and run depending on the solver, the same with one thread as with four) the two take 55 s and
22 s on `calibration-1`. So the worker gets through about 1.5 billion fuel per second for the
whole machine, 0.4 billion per run, 18 to 22 times less than the desktop, and more threads
don't help under a quota that all four CPUs share. (Rules version 5 assumed 3.4 billion per
run, as if the four CPUs were whole cores, and so five to ten minutes for a solver that solves
nothing.) Checking proofs is the smaller part: drat-trim and cake_lpr, one proof at a time, need
at most 20 s on a desktop core for all the proofs of a set (MiniSat's 25; the largest, 76 MB
from a run of 64 billion fuel, takes 11 s), up to two fifths of the time for the quick solvers,
next to nothing for the slow ones, whose time goes into runs that end out of fuel.

CPU seconds on the desktop for `calibration-1`, solving and checking, with the runs going to a
multiple of the budget; the worker takes about five seconds for each (977.5 s for Hashi CDCL's
211, 476 s for splr's 73, on the season's set, which is of the same kind):

| | 1× | 2× | 4× | 10× | worker at 4× | worker at 10× |
|---|---|---|---|---|---|---|
| Kissat | 12 | 14 | 23 | 23 | 2 min | 2 min |
| CaDiCaL | 16 | 19 | 24 | 33 | 2 min | 3 min |
| MiniSat | 17 | 20 | 27 | 45 | 2 min | 4 min |
| splr | 30 | 40 | 54 | 73 | 4½ min | 8 min (measured) |
| Hashi CDCL | 28 | 52 | 98 | 211 | 8 min | 16 min (measured) |
| Hashi DPLL | 46 | 86 | 148 | 307 | 12 min | 26 min |
| solves nothing | 40–65 | 80–130 | 160–250 | 400–630 | 13–21 min | 33–53 min |

A solver that solves nothing runs every formula to the end: 48 runs of 28 billion fuel, at the
fuel rates of the solvers above. At ten times the budget a run of a solver with MiniSat's rate
would also take about five and a half minutes on the worker, past the 300 s wall clock limit,
which would have decided its result instead of fuel; at four times it takes about two. On
dedicated CPUs (`performance`, a whole core each) the worker would be about eight times faster
with two of them: a minute or two for every house entry.

**The answer to the objection.** Instruction counts are not time, and refactoring changes them;
both are true. Measured on seven very different solvers, counting operators with weights for
memory and none for registers predicts native time within a solver with rank correlations of
0.91 to 1.00, ranks the solvers as their running time does, and is off across solvers by a
typical factor of 1.45 (e^0.372), at most 2.3. Ticks and mems are proxies of the same kind, and
mems did no better here. In return fuel is exactly reproducible on every machine, at any load,
which a public leaderboard needs more than a closer proxy for one particular processor. A
refactoring that saves fuel saves time too, up to these errors.

## Limitations

- One machine (a hybrid Intel laptop processor), one compiler per side. Weights for another
  processor would differ somewhat; the cache sizes matter.
- Small formulas: what a solver can do within the budget, so working sets mostly fit in the
  caches. On formulas with gigabytes of clauses memory would weigh more, but the Dojo's budget and
  memory limit keep entries away from that regime.
- Seven solvers, three of them close relatives (the Hashi solvers, the starter). More diverse
  entries would pin the weights down better; the study is easy to repeat with them.
- Native timings include starting a process (about a millisecond), so runs under 20 ms were left
  out of the fits; they count in the rankings.
- Native UNSAT answers were trusted when the label says UNSAT (the proofs weren't checked
  natively); the sandbox runs were checked by drat-trim and cake_lpr.
- No GBD or community benchmarks: the pool was empty, and GBD formulas big enough to matter would
  exceed the budget.
- The leaderboard's table was chosen after seeing all the data; its held-out numbers are
  therefore a little optimistic. The refitted three-group model (0.384) is the honest estimate.

## Reproducing

```sh
dojo/calibration/run.sh OUT [CPU]
```

builds the seven solvers, exports the formulas, measures and analyses (about an hour of timing,
pinned to `CPU`). The data of this study are in `dojo/calibration/`: `fuel.csv` (per run:
solved, uniform fuel and the count of every category and of function entries), `time.csv` (per
run: the native timings, whether the native run solved it, the sandbox time), `conditions.log`
and `report.md`, which `dojo calibrate analyze dojo/calibration/fuel.csv
dojo/calibration/time.csv` writes again.

## References

1. R. Coutelier, J. Rath, M. Rawson, A. Biere, L. Kovács, arXiv:2412.16058, 2024 (ticks as a
   deterministic measure of progress in Kissat and CaDiCaL).
2. D. E. Knuth, *The Art of Computer Programming*, Volume 4B, Part 2 (Satisfiability), 2022:
   mems.
3. arXiv:2505.07208, 2025 (mems against running time within and across programs).
4. A. Jangda, B. Powers, E. D. Berger, A. Guha, "Not So Fast: Analyzing the Performance of
   WebAssembly vs. Native Code", USENIX ATC 2019.
5. SAT Competition 2026, rules: <https://satcompetition.github.io/2026/rules.html> (the
   "bring your own benchmark" rule with MiniSat as the yardstick).
6. wasmtime 49, `Config::consume_fuel` and `Config::operator_cost`.
