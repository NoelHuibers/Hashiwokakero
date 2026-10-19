# SAT-Solver

DPLL/CDCL SAT solver from the SAT solving lab (`gruppe-m-3-abgabe`, commit `1f464e7`), used by the Hashi backend as an alternative to splr (see `backend/src/sat.rs`). The library can bound the runtime of `CDCL`/`DPLL` with `set_deadline`/`with_deadline` (`DIMACSOutput::Timeout`) and reports statistics via `decisions()` and `restarts()`.

Only the solver library, the `sat` binary and the inputs needed by the unit tests are included here. The benchmark tooling (`cputime`, `results`, `plot`) and the full set of benchmark inputs remain in the original repository.

## Running the SAT-Solver

Once built via `cargo build --release` the solver can be executed the following:

`../target/release/sat [MODE] [FILEPATH] [FILEPATH] --heuristic [HEURISTIC] [FLAGS]`

### MODE

- dpll
- cdcl

### HEURISTIC

Supported heuristics are:

- arbitrary
- dlis
- dlcs
- mom
- boehm
- jeroslaw_wang
- vsids
- custom

### Flags

| short | long               | description                                  |
| ----- | ------------------ | -------------------------------------------- |
| -H|--heuristic |Specify the heuristic to use [default: arbitrary]|
| -d|--depth|Shows the depth of the search tree|
| -k|--k |Argument for k-bounded learning [default: 10]|
| -m|--m |Argument for m-size relevance based learning [default: 10]|
| -s|--subsumed_clauses|Eliminates subsumed clauses|
| -f|--flamegraph|Specify whether to create a flamegraph|
| -r|--restarts_threshold |Specify the conflict threshold for a restart|
| -l|--luby|Use luby sequence for restarts|
| -F|--restart_factor |Specify the factor for restarts|
| -D|--DRUP|Log the learned and deleted clauses as a DRUP proof to `<output>.log` (needs the output file)|
| -t|--timeout |Give up after this many seconds and print `UNKNOWN (timeout)`|
| -h|--help|Print help|
| -V|--version|Print version|

## Running the tests

`cargo test --package sat_solver`
