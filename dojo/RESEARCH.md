# SAT benchmarking state of the art (2024–2026), researched 2026-10-20

## Proofs
- SAT Competition 2026 main track: SAT needs a model, UNSAT needs a proof written to `proof.out` (https://satcompetition.github.io/2026/output.html). Partial models OK if every clause is satisfied.
- Base format: DRAT, text or binary (binary ~3x smaller), unchanged since 2014.
- Since 2023 solvers pick a checker at registration. 2026 options: GRAT, DPR-trim, VeriPB, SR (https://satcompetition.github.io/2026/downloads/satcomp26slides.pdf). Each ends in a formally verified checker:
  - DPR-trim / cake_lpr: drat-trim or dpr-trim turns the proof into LRAT/LPR, then cake_lpr (CakeML, verified in HOL4) checks it, printing `s VERIFIED UNSAT`. Solvers that emit LRAT directly skip the conversion (https://satcompetition.github.io/2026/downloads/checkers/cakelpr.pdf).
  - GRAT: unverified gratgen adds hints, gratchk (verified in Isabelle/HOL) checks.
  - VeriPB/CakePB for pseudo-Boolean proofs; SR (substitution redundancy, Codel/Avigad/Heule FMCAD 2024).
  - Plain drat-trim no longer seems to be offered as the final checker (inferred from the checker list).
- A checker timeout counts as unsolved. The checker gets 45,000 s against a 5,000 s solver limit (2025 and 2026 slides).
- A wrong model, a rejected proof, or UNSAT on a known-SAT instance disqualifies. 2026: 2 solvers were disqualified; organizers plan to also disqualify solvers that crash too often.
- CaDiCaL writes LRAT natively (`--lrat`). lrat-trim (Pollitt/Fleury/Biere SAT 2023) checks and trims LRAT. CaDiCaL 3.0 (SAT 2026) calls linear proof production with hints its main challenge.

## Deterministic effort measures
- Knuth's mems: one mem = one 64-bit memory access; his SAT programs report mems, excluding parsing and output.
- Ticks (Kissat/CaDiCaL): roughly the cache lines touched. Coutelier, Rath, Rawson, Biere, Kovács 2024 (https://arxiv.org/pdf/2412.16058): wall time depends on machine and load, and "instructions easily change when refactoring code", so both are unsuitable for deterministic progress; they count ticks instead. This is the main published criticism of instruction-count fuel, and we must address it honestly. CaDiCaL 3.0 schedules inprocessing deterministically by ticks.
- 2025 study (arXiv 2505.07208): mems track runtime well within one program, poorly across programs.
- No published quantitative correlation study of ticks or instructions vs wall time for SAT solvers, and nothing on SAT solvers in Wasm, so our validation would be new.
- Wasm runs 45–55% slower than native on SPEC (USENIX ATC'19). Battlecode meters bots by JVM bytecodes (deterministic); that's the closest precedent.

## Benchmark selection
- Bring your own benchmarks: each team submits 20 new instances, at least 10 "interesting" (MiniSat can't solve within 1 min, own solver can within 1 h) (https://satcompetition.github.io/2026/rules.html).
- 2026 selection script (select26-deterministic.py):
  - Target 400 instances, deduplicated by isohash2.
  - New instances: up to 12 random per participating author, up to 16 per family from non-participants. The rest comes from old instances, excluding MiniSat-easy ones, random families and huge files.
  - Old instances are split roughly into thirds sat/unsat/unknown and weighted toward rare authors, rare families and recent first use.
  - The seed is the SHA-256 of the selected new instances' hashes, so nobody (organizers included) can predict the selection.
- 2026 warning: the AI-generated solver Lymphosat used more than 100 family-specific LLM-generated solvers and beat every main-track solver on SAT instances. Organizers flagged "memorization vs generalization" as an open problem. For us this argues for held-out families/sizes and recognition-resistant scrambling.
- GBD (Iser & Jabs SAT 2024, https://benchmark-database.de/):
  - More than 100k instances, with instance hash and isohash, feature extractors, gbd-tools, and a REST API (`/file/<hash>`, queries like `?track=main_2026`).
  - Families include miter, station-repacking, hamiltonian, sudoku and more.
- Sustainable Benchmarking Tool (Iser et al. SAT 2026): small subsets that still give statistically sound comparisons.

## Scoring
- PAR-2 is still the official ranking, reported as SAT/UNSAT/ALL with cactus plots and the VBS.
- 2026: AI-generated/AI-tuned subtracks added; No-Limits renamed Experimental.
- Shapley value (Fréchette et al., AAAI 2016): standalone scores reward near-clones, marginal contribution punishes correlated solvers; the Shapley value fixes both. Temporal Shapley (Kotthoff et al. 2018).
- CASC-style SOTA contribution: per problem 1/(number of solvers that solved it).
- Sparkle SAT Challenge 2018: live leaderboard ranked by relative marginal contribution to the portfolio (log10 of PAR-2 without the solver / PAR-2 with it).

## Transparency
- Platforms: 2024 StarExec, 2025 BenchCloud (LMU), 2026 HoreKa (KIT). StarExec is being decommissioned.
- Solvers ship build.sh/run.sh on a Docker image; sources must be made public after the deadline.
- Published per year: per-instance scores.csv, solver sources, the selection script, the instance list, GBD tags.

## Novelty
- No always-open online SAT leaderboard found, and nothing that uses instruction or fuel metering.
- Relatives: Sparkle SAT Challenge 2018 (live leaderboard), optil.io (PACE online judge with public and private sets), Battlecode bytecode limits, WASM OJ (no proofs).
- The combination of Wasm fuel, verified UNSAT and seeded generators looks novel.
