# Ignored-test cleanup

The review found five `#[ignore]` annotations. They did not represent five known product bugs.
Two were slow correctness checks, two were performance/diagnostic utilities, and one used a
heuristic unsuited to its large fixture. All five annotations have now been removed.

| Original test | Why ignored | Resolution |
| --- | --- | --- |
| `generates_unique_puzzles_default_sizes` | Expensive generation in debug builds | Enabled all 25 difficulty/seed cases, retaining repeat generation and independent uniqueness checks. Optimize `backend` in development, as already done for solver crates, so coverage/debug overhead is less likely to reach the generator's wall safety limit. |
| `splr_agrees_with_cdcl_on_perturbed_puzzles` | Described as slow in debug | Enabled the existing 240 perturbed cases; additionally assert both SAT and UNSAT paths were exercised. Before changes it passed in 3.69 seconds locally. |
| `generation_time` | Hardware-dependent release timing assertions | Removed the duplicate timing test. Existing Criterion `generate` benchmarks already cover all difficulties at their real sizes; use them for measurements and comparisons. Correctness does not depend on an arbitrary runner's speed. |
| `generator_report` | Only prints diagnostic statistics | Moved to `backend/examples/generator_report.rs`; run with `cargo run --release -p backend --example generator_report`. Existing SEEDS/SIZE/QUIET/SHOW options remain. |
| `bug_jan_2nd_should_be_sat` | Arbitrary DPLL branching takes over five minutes on `ssa7552-038` | Replaced with `historical_sat_regressions_return_valid_models`: DLIS solves the same large input in about four seconds; Arbitrary still covers the small `uf50-06` input. Require SAT, consistent assignments and satisfaction of every original clause; errors/timeouts no longer pass. A 30-second solver deadline bounds failures. |

No formula or assertion was deleted from either generator correctness corpus. Changing the
DPLL heuristic intentionally changes which search path the large fixture exercises; existing
small-formula property tests still exercise all DPLL heuristics, and CDCL retains the same
historical large fixture separately.

The Dojo also had conditional coverage that could silently disappear locally: real proof
checkers, comparison with production checkers and the WASI starter build. These now require
their prerequisites and fail with setup instructions instead of returning success, falling
back to a different checker or omitting the comparison. Runtime fallback for ordinary local
`dojo run` is unchanged. CI already builds the checkers; the obsolete `DOJO_REQUIRE_CHECKERS`
switch was removed. Local setup is documented in the root README.

## Validation

- Core command: `cargo test --locked -p common -p backend -p sat_solver`: 171 passed, zero failed/ignored.
- Worker command: `cargo test --locked -p dojo --features worker`, using the pinned real `drat-trim` and `cake_lpr` built by `dojo/checkers/build.sh`: 138 passed, zero failed/ignored, including production checker and WASI starter checks.
- Total: 309 passed, zero ignored. Original review arithmetic corrected: 168 + 138 was 306, not 302.
- The full 171-test core suite also passes with `cargo llvm-cov --no-report --locked -p common -p backend -p sat_solver`; this verifies the newly enabled cases under CI's coverage instrumentation. A full workspace coverage report/baseline was not regenerated.
- Negative control: removing checker paths and PATH entries makes the real-tools test fail explicitly with the setup message.
- `SEEDS=1 SIZE=5 QUIET=1 cargo run --locked -p backend --example generator_report` succeeds.
- Clippy passes for all backend/solver/worker targets with warnings denied; formatting and whitespace checks pass.

The changes affect test contracts, diagnostics, documentation, CI setup and development
optimization. Production solver and generator algorithms are unchanged.
