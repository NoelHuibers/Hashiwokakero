# Dojo Wasmtime security patch — 2026-10-21

Base: `83c2b6d`, including the compatible TLS patches assessed in
[the preceding dependency report](2026-10-20-dependency-advisories.md).
This follow-up changes only the Dojo runtime dependency family and its tests.

## Supported update and dependency graph

Update the three direct requirements together: `wasmtime` and `wasmtime-wasi`
to `49.0.4`, and the exact `wiggle` pin to `=49.0.4`. The exact pin matters because
our bounded proof-file wrappers pass Wiggle guest memory to WASI's own functions.
Cargo resolves one coherent family; forcing only a lockfile version would conflict
with the old exact requirement. Crates.io reports all three `49.0.4` releases and
`cranelift-codegen 0.136.4` as published on October 19 and not yanked.

Relevant resolved worker graph (`cargo tree --locked -p dojo --features worker`):

```text
dojo [worker → runner]
├── wasmtime 49.0.4 [Cranelift, runtime, std, parallel-compilation]
│   └── wasmtime-internal-cranelift 49.0.4
│       └── cranelift-codegen 0.136.4
├── wasmtime-wasi 49.0.4 [p1 → p2]
│   ├── wasmtime 49.0.4 [async, component-model]
│   ├── wasmtime-wasi-io 49.0.4
│   └── wiggle 49.0.4
└── wiggle 49.0.4 [wasmtime]
```

The lockfile changes exactly 32 Wasmtime, WASI, Wiggle, Pulley and Cranelift
packages from `49.0.3`/`0.136.3` to `49.0.4`/`0.136.4`. No unrelated package
changes. In particular, `rustls-webpki 0.103.16` and `webpki-roots 1.0.10` from
the TLS patch remain. The default website SSR graph does not select Wasmtime.

## Advisory prerequisites and application behavior

- [RUSTSEC-2026-0378](https://github.com/rustsec/advisory-db/blob/e6f94f3760d5e6c54c1b05f204002263bc025524/crates/wasmtime/RUSTSEC-2026-0378.md)
  / [GHSA-9pvw-hwfg-92h5](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-9pvw-hwfg-92h5):
  a host-created component stream with an incompatible payload can panic when
  lowered into a guest. [Upstream fix #14827](https://github.com/bytecodealliance/wasmtime/pull/14827)
  centralizes the type checks. WASI p1 transitively compiles p2 and Wasmtime's
  component-model support, but `validate` explicitly disables component inputs;
  `Sandbox::load` uses core `Module`, and execution links only
  `wasi_snapshot_preview1` with `WasiP1Ctx`. No component instantiation or
  host-created component future/stream path exists here. The package is patched
  regardless of that exposure limit.
- [RUSTSEC-2026-0379](https://github.com/rustsec/advisory-db/blob/e6f94f3760d5e6c54c1b05f204002263bc025524/crates/wasmtime/RUSTSEC-2026-0379.md)
  / [GHSA-mghf-wjfp-p5xm](https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-mghf-wjfp-p5xm):
  a very large core Wasm `br_table` can produce an incorrect branch target in
  Cranelift's aarch64 backend, risking a sandbox escape. Core submissions are the
  worker's real untrusted input. Byte and operator-count limits do not bound the
  number of targets in a single `br_table` sufficiently to exclude this bug.
  [Upstream fix #14826](https://github.com/bytecodealliance/wasmtime/pull/14826)
  repairs the aarch64 jump sequence. The release also caps non-default targets at
  65,520 before translation, even for unreachable operators, on every architecture.

Both RustSec records list `49.0.4` as patched. Checked the actual
[49.0.3...49.0.4 source diff](https://github.com/bytecodealliance/wasmtime/compare/v49.0.3...v49.0.4)
and [release notes](https://github.com/bytecodealliance/wasmtime/blob/v49.0.4/RELEASES.md),
including the backport rather than assuming the main-branch fix was released.
Other release changes concern WASI HTTP (not selected here) and Rustls (already
`0.23.46` in this lockfile).

The configured packaged-worker build uses `ubuntu-24.04`, native Cargo compilation
without a cross target, and Debian trixie runtime images. That CI path produces
an x86_64 worker, matching this test host. `dojo/fly.toml` has no explicit
architecture setting. No live Fly machine or production configuration was
queried, and no local aarch64 execution or cross-compilation is claimed.

## Fuel, scoring and cache compatibility

The patch diff does not change operator-cost tables, fuel instrumentation, WASI
p1 glue or the application's deterministic clocks/random seeds. Existing tests
check exact category costs, bulk-operation charges, repeatability, exhaustion,
wall deadlines and proof-file limits using the real sandbox. The existing MiniSat
golden test retains exactly **34,301,392** and **23,972,394** fuel on its fixed
UNSAT pigeonhole and SAT factoring instances, respectively. `RULES_VERSION`
and `GENERATOR_VERSION` remain unchanged; there is no cost-model migration or
automatic rescoring introduced here. The deliberate admission change is that
previously compilable modules exceeding the new jump-table limit are now invalid.
This does not guarantee identical behavior for every possible adversarial module.

The engine is recreated on worker startup and modules are compiled with
`Module::new`. Wasmtime's persistent cache feature is not enabled, and the
application does not serialize/deserialize compiled modules. `DOJO_CACHE_DIR`
contains benchmark sets/community pools, keyed by generator version and digest,
not native code. No compiled-cache migration or benchmark-cache invalidation is
needed. Deploying the updated worker replaces the engine and its in-memory modules.

The added integration regression constructs an otherwise accepted WASI upload
with 65,521 branch targets. It verifies that application validation succeeds,
`Sandbox::load` returns `Rejection::Invalid` with the compiler-limit diagnostic,
and a subsequent ordinary WASI program still loads, runs and writes stdout on
the same sandbox. This covers our submission error boundary without copying the
upstream compiler/backend test matrix. It does not reproduce an aarch64 exploit.

## Validation and remaining findings

On x86_64 Linux with Rust/Cargo `1.99.1`, all Cargo builds used
`CARGO_BUILD_JOBS=2` and this isolated worktree. The real checkers were explicitly
selected with `DOJO_DRAT_TRIM=/tmp/hashi-test-checkers/drat-trim` and
`DOJO_CAKE_LPR=/tmp/hashi-test-checkers/cake_lpr`.

- `cargo test --locked -p dojo --features worker -- --test-threads=2`:
  **135 library tests and 4 CLI integration tests passed, 0 failed, 0 ignored**.
  Includes the new compiler-rejection regression, real house/starter/reference
  solver proof verification (drat-trim and cake_lpr), exact golden fuel,
  deterministic sandbox limits, cached benchmark compatibility, and loopback
  HTTP worker evaluation/reporting and process shutdown. Binary/doc test targets
  contain zero tests and also passed.
- `cargo clippy --locked -p common -p sat_solver -p backend -p dojo -p house
  --all-targets --features dojo/worker -- -D warnings`: **passed**.
- `cargo fmt --all -- --check` and `git diff --check`: **passed**.
- Compared parsed before/after lockfiles: **32 family packages updated, no
  remaining 49.0.3 package, no unrelated package changed**. Checked the worker
  feature tree and confirmed Wasmtime is absent from the default SSR graph.

Using the same local RustSec database at
`e6f94f3760d5e6c54c1b05f204002263bc025524`, `cargo audit --no-fetch --file
Cargo.lock --json` changes from 14 to 12 package/advisory matches: exactly
`RUSTSEC-2026-0378` and `RUSTSEC-2026-0379` disappear. There are no new matches;
all other matches and the three unmaintained-package warnings are identical.
Audit still exits 1. No advisory suppressions were added. The legacy TLS/parser,
zip and maintenance issues assessed in the preceding report remain unresolved.

No full release/site/image build, browser validation, coverage run or production
access was performed. The existing starter-proof test builds only its small WASI
fixture in release mode as part of the worker suite. Integration still requires
the repository's six CI checks before merge.
