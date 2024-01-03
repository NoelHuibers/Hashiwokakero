# DPLL Solver

## Running the SAT-Solver

Once built via `cargo build --release` the solver can be executed the following:

`./target/release/dpll_solver -- bin dpll --mode [MODE] --input [FILEPATH] --heuristic [HEURISTIC] --depth [DEPTH]`


### MODE

Currently, only the DPLL mode is available.

### DEPTH

The depth flag can be set to true or false and determines whether the currently lowest backtracking depth is printed to the terminal. This gives some idea of the progress.

### HEURISTIC

Supported Heuristics are:

- arbitrary (in progress)
- dlis (in progress)
- dlcs (in progress)
- mom (TODO)
- boehm (TODO)
- jeroslaw_wang (TODO)

### CPU Time Measurement
To measure the CPU time for each `.cnf` file in `dimacs-files/input`, run the `cputime` binary:

`cargo run --bin cputime`

## Running the tests

When cloning make sure to also sync the submodule which includes testfiles:

`git submodule update --init --recursive`

Run the tests using cargo:

``

# Team Responsibilities

**Noel:**

- [x] Init
- [x] Parser
- [ ] Edge cases for parser (e.G. newlines before 0 was found as in Zulip question)

**Laura:**

- [x] Testing / Profiling
- [x] Benchmarks / CPU Time
- [ ] Cactus plots
- [ ] Heuristics

**Flo:**

- [x] DPLL Algorithm
- [x] Pure literal Elimination
- [x] Better mem allocations
