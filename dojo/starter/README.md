# Hashi SAT Dojo starter

A minimal DPLL solver that speaks the [SAT Dojo](https://hashi.rs/dojo) protocol. Copy this folder,
make the solver faster, and submit it.

```sh
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
# the file to submit:
ls target/wasm32-wasip1/release/dojo-starter.wasm
```

## Protocol

- Input: a DIMACS CNF formula on **stdin**. There is no network or environment, and no files
  but the proof file below.
- Output on **stdout**, as in the SAT competition:
  - `s SATISFIABLE` followed by the model on `v` lines, ending with `0`
  - `s UNSATISFIABLE`, backed by a proof (see below)
  - `s UNKNOWN` if you give up
- Any language that compiles to WebAssembly with WASI preview 1 works: Rust, C/C++ (wasi-sdk), Zig, Go...

### Proofs

Like in the main track of the [SAT Competition](https://satcompetition.github.io/2026/rules.html),
an UNSAT answer only counts with a proof. Your program gets a file path as its first argument
(`/proof/proof.out`); write the proof there, in one of these formats:

- **DRAT** (the default), text or binary (told apart automatically, like drat-trim does): each
  line a clause that follows from the formula and the clauses before it, ending with `0`; `d` in
  front deletes a clause. The last clause is the empty one, `0`.
- **LRAT**, text or binary, with hints: print the line `c proof lrat` on stdout too.

This starter writes DRAT: every time a branch fails it adds the clause "not all of these
decisions", which follows by unit propagation (RUP). For the formula `(1 ∨ 2) (¬1 ∨ 2) (1 ∨ ¬2)
(¬1 ∨ ¬2)`, deciding `1` and then `-1` both fail at once, so the proof is:

```text
-1 0
1 0
0
```

The leaderboard checks proofs like the competition: drat-trim turns DRAT into LRAT, and
cake_lpr, a checker proven correct in HOL4 and compiled by CakeML, has to accept it. Writing the
proof costs fuel like everything else, checking it doesn't. No proof, or more than 256 MiB of it,
or one the checkers can't finish within 5 minutes and 1 GiB, counts as **unsolved**; a proof they
reject counts as a **wrong answer**.

## Scoring

Every formula gets a budget of 7 billion *fuel*. Fuel counts the WebAssembly operators your solver
executes, weighted like a real processor pays for them: loads and stores cost 3, locals, globals
and constants 0 (they are registers once compiled), every other operator 1, bulk memory operations
1 per byte. The weights come from a calibration against native running time with MiniSat,
CaDiCaL, Kissat and four other solvers ([dojo/CALIBRATION.md](../CALIBRATION.md)). Time and
machine load don't matter, so scores are exactly reproducible. Clocks and random numbers are deterministic too: `random_get` is seeded per formula
and set.

Score = PAR-2: the mean fuel per formula, where unsolved formulas count twice the budget.
**A single wrong answer disqualifies the submission**: a wrong model, UNSAT on a satisfiable
formula, or a rejected proof. So never guess UNSAT.

PAR-2 alone decides the rank. The leaderboard also splits every score into SAT, UNSAT and ALL
(`dojo run` prints the split too), draws a cactus plot of all solvers with the virtual best solver,
and shows what your solver adds to the others: its marginal contribution, its state-of-the-art
contribution and its Shapley value. A solver that is fast where everyone else is slow counts
there, even if it ranks low.

## The formulas

The public development set has 36 formulas, half of them satisfiable, in eight families: Hashi
puzzles, Hashi parity (unsatisfiable by a parity argument), random 3-SAT, pigeonhole, queens,
factoring (a multiplier circuit whose output is a semiprime or a prime), miters (two different
multipliers that agree, or one with a rare bug) and graph colouring (planted colourings, or
Mycielski graphs that need one colour more). Every formula is scrambled and reaches your solver
without a name, under the same `c` line.

A season's hidden set has 48: the same families from a secret seed, one larger formula of each,
and one **mystery family** that the dev set doesn't have, picked by the seed from a reserve and
revealed when the season ends. Tuning your solver to the dev set's families won't be enough, it
has to generalise (the SAT Competition 2026 raised exactly that after an AI-built solver won with
over a hundred family-specific sub-solvers). A few **community benchmarks** join it too: formulas
players brought along (https://hashi.rs/dojo/benchmarks), drawn by the season's seed.

## Testing locally

The `dojo` tool runs your solver exactly like the leaderboard, on the public development set:

```sh
cargo install --git https://github.com/NoelHuibers/Hashiwokakero dojo --features worker
dojo run target/wasm32-wasip1/release/dojo-starter.wasm          # all families
dojo run my.wasm --family pigeonhole --stderr                      # one family, with your logs
dojo export dev-set/                                               # the formulas as .cnf files
dojo run my.wasm --hidden --seed practice                          # a hidden-style set
dojo run my.wasm --set dojo-season-1.tar.gz                        # a past season's hidden set
```

`dojo run` checks proofs with drat-trim and cake_lpr if they are on your `PATH` (or named by
`DOJO_DRAT_TRIM` and `DOJO_CAKE_LPR`), exactly like the leaderboard, and with its built-in checker
otherwise. The built-in one checks every lemma, so it may reject a proof with a wrong but unused
lemma that drat-trim would skip. To install the two (needs a C compiler and curl):

```sh
git clone https://github.com/NoelHuibers/Hashiwokakero && Hashiwokakero/dojo/checkers/build.sh ~/.local/bin
dojo run my.wasm --checker verified                                # fail if they're missing
dojo run my.wasm --proofs proofs/                                  # keep every proof file
```

Finished seasons are archived at https://hashi.rs/dojo/seasons: the seed of the hidden set (its
SHA-256 was published when the season started), the set itself and every result per formula.
