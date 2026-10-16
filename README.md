# Hashi - Bridges Puzzle

## Folder Structure

The project is organized into three main folders:

- **backend**: Contains the backend code for the Hashi - Bridges Puzzle game.
- **benches**: Contains the code required for benchmarking.
- **common**: Shared code and resources that are used by both the backend and frontend.
- **frontend**: Houses the frontend code, responsible for the user interface and interactions.
- **sat_solver**: Our own DPLL/CDCL SAT solver; its CDCL solver can be used instead of splr to solve the puzzles.
- **solutions**: Contains the .cnf and solution .txt files for all initial test problems.

## How to Run

To run the project, follow these steps:

To run the project, open a terminal in the project root directory and execute the following command for frontend:

`cargo run --package frontend`

or for backend:

`cargo run --package backend -- --mode [MODE] --input [FILEPATH] --output [FILEPATH] --solver [SOLVER]`

### MODE

We currently support three modes:

|Flag|Alias|Functionality|
|-|-|-|
|encode||Reads a .txt file containing an ASCII hashi puzzle and generates an according .cnf in that directory|
|solve||Reads a CNF file and solves it using minisat|
|encodesolvereconstruct|esr|Reads a .txt file containing an ASCII hashi puzzle, generates a .cnf, solves the CNF file using minisat, reconstructs the solution and prints the reconstructed solved hasi puzzle. 

### SOLVER

The `solve` and `esr` modes can use different SAT solvers (default: `splr`):

|Solver|Functionality|
|-|-|
|splr|The [splr](https://crates.io/crates/splr) crate|
|cdcl|Our own CDCL solver from `sat_solver` (VSIDS, no restarts), written in the SAT Solving lab WS 2023/24|

The solver can also be selected next to the Solve button in the frontend.

To build the project, open a terminal in the project root directory and execute:

`cargo build --release`

To execute the rlease build for `test1` and print the reconstructed solution:

`../target/release/backend  --mode esr --input ./input/test1.txt --output ./input/test1.minisat-output.txt`

To execute the integration test containing all given test scenarios and some aditional edge cases:

`cargo test --package backend --test integration_test -- test_integration --exact --nocapture`

### Benchmarking

To execute the criterion benchmarking open a terminal in the project backend directory and execute: 

`cargo bench`

See [Evaluation](hashi_report.pdf)

## SAT Conversion

See [Clauses](Clauses.md)
