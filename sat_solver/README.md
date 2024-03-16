# SAT-Solver

## Running the SAT-Solver

Once built via `cargo build --release` the solver can be executed the following:

`./target/release/sat [MODE] [FILEPATH] [FILEPATH] --heuristic [HEURISTIC] [FLAGS]`

### MODE

* dpll
* cdcl

### HEURISTIC

Supported Heuristics are:

- arbitrary
- dlis
- dlcs
- mom
- boehm
- jeroslawwang
- vsids
- custom

### Flags

Flags that can be set are: --depth or in short -d and --flamegraph or in short -f
Depth:
The depth flag can be set to true or false and determines whether the currently lowest backtracking depth is printed to the terminal. This gives some idea of the progress.
Flamegraph:
Makes the flamegraph.

## CPU Time Measurement

To measure the CPU time for each `.cnf` file in `src/inputs`, run the `cputime` binary:

`cargo run --bin cputime --time-limit [TIME-LIMIT] --heuristic [HEURISTIC]`

### TIME-LIMIT

Time-Limit arguments:

- true: 60sec execution time frame
- false: measures the duration of the execution without timeout

## Plotting

To create the plots for the existing `.csv` files in `src/cputime`, run the `plot` binary:
`cargo run --bin plot`

## Running the tests

When cloning make sure to also sync the submodule which includes testfiles:

`git submodule update --init --recursive`

Run the tests using cargo:

``

# Team Responsibilities

**Noel:**

- [x] Fix issue with Conflict analysis
- [x] Subsumed Clauses
- [ ] Restarts
- [ ] Drup logging
- [ ] Heuristics

**Laura:**

- [x] Implication Graph
- [x] Conflict analysis
- [ ] Clause deletion strategy
- [ ] Heuristics

**Flo:**

- [x] 2-Watched Literals
- [x] Fix/Update impl. graph while setting vars
- [x] Fix/Rewrite Conflict analysis
- [x] Test + Debug non-chronological backtracking
- [x] Check whether unit clauses can occur in conflict analysis
- [x] Check impl for set_var in respect to unit clauses
- [x] Pure literal elimination as preprocessing
- [x] Output valid DIMACS solution
