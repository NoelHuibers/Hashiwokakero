//! Running a submission on one formula, sandboxed and metered.
//!
//! - WASI preview 1 with stdin (the formula), stdout (the answer, at most 8 MiB) and one file:
//!   the proof of an UNSAT answer, at the path in `argv[1]` (`/proof/proof.out`, see
//!   [`crate::proof`]). No other files, no environment, no network.
//! - The proof goes to a real file on disk (proofs can be far larger than memory should hold),
//!   in a fresh directory per run that the solver sees as `/proof`. The solver can only open
//!   `proof.out` there and write at most [`Limits::proof_bytes`] in total; beyond that writes
//!   fail with `EFBIG` and the proof counts as missing (not a crash: the run goes on). It can't
//!   create other files or directories, links or renames, so the disk it uses stays bounded.
//! - Fuel: every executed operator costs what the calibrated cost model says
//!   ([`crate::fuel::MODEL`]: loads and stores three, locals and constants nothing, the rest
//!   one, bulk memory and table operations one per byte or element). The budget replaces a time
//!   limit, so results are exactly reproducible.
//! - Clocks only move when they are read and random numbers come from a seed, so even solvers
//!   that use time or randomness give the same result every time.
//! - Floating point NaNs are canonicalised and relaxed SIMD is deterministic, so results are the
//!   same on every CPU.
//! - Memory is capped and a wall clock limit catches anything fuel doesn't (e.g. a host call
//!   that blocks).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rand::SeedableRng;
use rand::rngs::StdRng;
use wasmtime::{
    AsContextMut, Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, Trap,
};
use wasmtime_wasi::p1::wasi_snapshot_preview1::{self as wasi, WasiSnapshotPreview1 as _};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::{FsPerms, HostMonotonicClock, HostWallClock, I32Exit, WasiCtxBuilder};
use wiggle::GuestMemory;

use crate::Rejection;
use crate::fuel::Weights;
use crate::proof::{GUEST_DIR, GUEST_PATH, PROOF_FILE};
use crate::validate::validate;

/// How often the wall clock limit is checked.
const TICK: Duration = Duration::from_millis(10);
const MAX_STDOUT: usize = 8 << 20;

/// The most a solver may write to its proof file: 256 MiB.
pub const MAX_PROOF_BYTES: u64 = 256 << 20;

/// WASI errno values (preview 1).
const ERRNO_ACCES: i32 = 2;
const ERRNO_FBIG: i32 = 22;

#[derive(Clone, Debug)]
pub struct Limits {
    pub fuel: u64,
    pub memory_bytes: usize,
    pub wall: Duration,
    /// Bytes the solver may write to its proof file, see the module docs.
    pub proof_bytes: u64,
}

impl Default for Limits {
    /// The leaderboard's: runs go on to [`crate::RUN_BUDGET`] (scores are computed at the
    /// ranking budget from them, see [`crate::Outcome::at`]). The wall clock limit is well
    /// beyond what the run budget takes, so it only catches what fuel doesn't and never decides
    /// a result that fuel would have: 3 to 7 seconds on a fast desktop core, one to two and a
    /// half minutes on the worker, whose four shared CPUs get a quarter of one core between
    /// them once their burst is used up (dojo/CALIBRATION.md, "Larger budgets").
    fn default() -> Self {
        Self {
            fuel: crate::RUN_BUDGET,
            memory_bytes: 1 << 30,
            wall: Duration::from_secs(300),
            proof_bytes: MAX_PROOF_BYTES,
        }
    }
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    /// Returned from `_start` or called `proc_exit`.
    Exited,
    OutOfFuel,
    WallClock,
    /// A trap (e.g. `unreachable`, a Rust panic, out of bounds) or a failed allocation.
    Crashed(String),
}

#[derive(Clone, Debug)]
pub struct Run {
    pub end: End,
    pub stdout: String,
    pub fuel: u64,
    pub wall: Duration,
    /// The solver tried to write more than [`Limits::proof_bytes`] to its proof.
    pub proof_too_large: bool,
}

/// Time that only moves when the solver looks at it: one microsecond per reading.
struct VirtualClock(AtomicU64);

impl HostMonotonicClock for VirtualClock {
    fn resolution(&self) -> u64 {
        1_000
    }
    fn now(&self) -> u64 {
        self.0.fetch_add(1_000, Ordering::Relaxed)
    }
}

impl HostWallClock for VirtualClock {
    fn resolution(&self) -> Duration {
        Duration::from_micros(1)
    }
    fn now(&self) -> Duration {
        // 2027-01-15, a fixed date
        Duration::from_hours(500_000)
            + Duration::from_nanos(self.0.fetch_add(1_000, Ordering::Relaxed))
    }
}

struct State {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
    /// Bytes written to files (the proof) so far.
    proof_written: u64,
    proof_cap: u64,
    proof_too_large: bool,
}

/// Calls wasmtime's own (async) implementation of WASI function `$f` from ours, with the
/// guest's memory as wiggle sees it.
macro_rules! call_wasi {
    ($caller:ident, $f:ident($($arg:expr),*)) => {{
        let Some(Extern::Memory(memory)) = $caller.get_export("memory") else {
            return Err(wasmtime::Error::msg("missing required memory export"));
        };
        // What wasmtime's own glue does: without it the call may copy no bytes at all (ENOMEM)
        let fuel = $caller.as_context_mut().hostcall_fuel();
        let (bytes, state) = memory.data_and_store_mut(&mut $caller);
        state.wasi.set_hostcall_fuel(fuel);
        wasi::$f(&mut state.wasi, &mut GuestMemory::Unshared(bytes), $($arg),*).await
    }};
}

/// `len` bytes of guest memory at `ptr`, if they are there.
fn guest_bytes(caller: &mut Caller<'_, State>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return None;
    };
    let start = usize::try_from(ptr.cast_unsigned()).ok()?;
    let len = usize::try_from(len.cast_unsigned()).ok()?;
    memory
        .data(caller)
        .get(start..start.checked_add(len)?)
        .map(<[u8]>::to_vec)
}

/// How much a write of the iovecs at `iovs` writes at most: wasmtime writes the first non-empty
/// buffer only (the caller loops for the rest).
fn first_iovec_len(caller: &mut Caller<'_, State>, iovs: i32, count: i32) -> u64 {
    let count = count.clamp(0, 1024);
    guest_bytes(caller, iovs, count.saturating_mul(8))
        .unwrap_or_default()
        .as_chunks::<8>()
        .0
        .iter()
        .map(|iovec| u64::from(u32::from_le_bytes([iovec[4], iovec[5], iovec[6], iovec[7]])))
        .find(|&len| len > 0)
        .unwrap_or(0)
}

/// Counts a write of `len` bytes to file `fd` ending at `end`: `false` (and the proof is too
/// large) if it would go beyond the limit. Writes to stdout and stderr aren't counted.
fn allow_write(state: &mut State, fd: i32, len: u64, end: u64) -> bool {
    if (0..=2).contains(&fd) {
        return true;
    }
    let written = state.proof_written.saturating_add(len);
    if state.proof_too_large || written > state.proof_cap || end > state.proof_cap {
        state.proof_too_large = true;
        return false;
    }
    state.proof_written = written;
    true
}

/// Replaces wasmtime's WASI functions that could fill the disk: writes to files are counted
/// and capped, only the proof file can be opened, nothing else can be created. Each one calls
/// wasmtime's own implementation when the call is allowed.
fn guard_files(linker: &mut Linker<State>) -> wasmtime::Result<()> {
    const M: &str = "wasi_snapshot_preview1";
    linker.allow_shadowing(true);
    linker.func_wrap_async(
        M,
        "fd_write",
        |mut caller: Caller<'_, State>, (fd, iovs, n, out): (i32, i32, i32, i32)| {
            Box::new(async move {
                let len = first_iovec_len(&mut caller, iovs, n);
                if !allow_write(caller.data_mut(), fd, len, 0) {
                    return Ok(ERRNO_FBIG);
                }
                call_wasi!(caller, fd_write(fd, iovs, n, out))
            })
        },
    )?;
    linker.func_wrap_async(
        M,
        "fd_pwrite",
        |mut caller: Caller<'_, State>, (fd, iovs, n, offset, out): (i32, i32, i32, i64, i32)| {
            Box::new(async move {
                let len = first_iovec_len(&mut caller, iovs, n);
                let end = offset.cast_unsigned().saturating_add(len);
                if !allow_write(caller.data_mut(), fd, len, end) {
                    return Ok(ERRNO_FBIG);
                }
                call_wasi!(caller, fd_pwrite(fd, iovs, n, offset, out))
            })
        },
    )?;
    linker.func_wrap_async(
        M,
        "fd_filestat_set_size",
        |mut caller: Caller<'_, State>, (fd, size): (i32, i64)| {
            Box::new(async move {
                if !allow_write(caller.data_mut(), fd, 0, size.cast_unsigned()) {
                    return Ok(ERRNO_FBIG);
                }
                call_wasi!(caller, fd_filestat_set_size(fd, size))
            })
        },
    )?;
    linker.func_wrap_async(
        M,
        "path_open",
        |mut caller: Caller<'_, State>,
         (dir, flags, path, len, oflags, base, inheriting, fdflags, out): (
            i32,
            i32,
            i32,
            i32,
            i32,
            i64,
            i64,
            i32,
            i32,
        )| {
            Box::new(async move {
                let name = guest_bytes(&mut caller, path, len).unwrap_or_default();
                let name = name.strip_prefix(b"./").unwrap_or(&name);
                if name != PROOF_FILE.as_bytes() {
                    return Ok(ERRNO_ACCES);
                }
                call_wasi!(
                    caller,
                    path_open(
                        dir, flags, path, len, oflags, base, inheriting, fdflags, out
                    )
                )
            })
        },
    )?;
    linker.func_wrap(M, "path_create_directory", |_: i32, _: i32, _: i32| {
        ERRNO_ACCES
    })?;
    linker.func_wrap(
        M,
        "path_symlink",
        |_: i32, _: i32, _: i32, _: i32, _: i32| ERRNO_ACCES,
    )?;
    linker.func_wrap(
        M,
        "path_link",
        |_: i32, _: i32, _: i32, _: i32, _: i32, _: i32, _: i32| ERRNO_ACCES,
    )?;
    linker.func_wrap(
        M,
        "path_rename",
        |_: i32, _: i32, _: i32, _: i32, _: i32, _: i32| ERRNO_ACCES,
    )?;
    linker.allow_shadowing(false);
    Ok(())
}

/// A compiled submission.
#[derive(Debug)]
pub struct Solver {
    module: Module,
}

pub struct Sandbox {
    engine: Engine,
    linker: Arc<Linker<State>>,
    runtime: tokio::runtime::Runtime,
}

impl std::fmt::Debug for Sandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sandbox")
            .field("engine", &self.engine)
            .finish_non_exhaustive()
    }
}

impl Sandbox {
    /// The sandbox of the leaderboard, with the Dojo's cost model ([`crate::fuel::MODEL`]).
    pub fn new() -> Result<Self, String> {
        Self::with_weights(&crate::fuel::MODEL)
    }

    /// A sandbox with another cost model, e.g. one that only counts one category of operators
    /// (the calibration study, see [`crate::calibrate`]).
    pub fn with_weights(weights: &Weights) -> Result<Self, String> {
        // Without the `threads` feature wasmtime has no shared memories at all
        let mut config = Config::new();
        config
            .consume_fuel(true)
            .operator_cost(weights.table())
            .epoch_interruption(true)
            .wasm_memory64(false)
            .wasm_multi_memory(false)
            .cranelift_nan_canonicalization(true)
            .relaxed_simd_deterministic(true);
        let engine = Engine::new(&config).map_err(|e| e.to_string())?;

        let mut linker: Linker<State> = Linker::new(&engine);
        p1::add_to_linker_async(&mut linker, |s| &mut s.wasi).map_err(|e| e.to_string())?;
        guard_files(&mut linker).map_err(|e| e.to_string())?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_time()
            .build()
            .map_err(|e| e.to_string())?;

        // The wall clock limit: interrupts stores whose deadline (in ticks) has passed
        let ticker = engine.weak();
        std::thread::spawn(move || {
            while let Some(engine) = ticker.upgrade() {
                engine.increment_epoch();
                drop(engine);
                std::thread::sleep(TICK);
            }
        });

        Ok(Self {
            engine,
            linker: Arc::new(linker),
            runtime,
        })
    }

    /// Checks and compiles a submission.
    pub fn load(&self, wasm: &[u8]) -> Result<Solver, Rejection> {
        validate(wasm)?;
        let module =
            Module::new(&self.engine, wasm).map_err(|e| Rejection::Invalid(format!("{e:#}")))?;
        Ok(Solver { module })
    }

    /// Runs the solver on `input` (DIMACS text). `seed` seeds its random numbers. With
    /// `proof_dir` (an empty directory) the solver can write its proof to `proof.out` there,
    /// it gets the path as its first argument.
    pub fn run(
        &self,
        solver: &Solver,
        input: &str,
        seed: u64,
        limits: &Limits,
        show_stderr: bool,
        proof_dir: Option<&Path>,
    ) -> Run {
        let stdout = MemoryOutputPipe::new(MAX_STDOUT);
        let mut wasi = WasiCtxBuilder::new();
        wasi.arg("solver");
        if let Some(dir) = proof_dir {
            wasi.arg(GUEST_PATH);
            if let Err(e) = wasi.preopened_dir(dir, GUEST_DIR, FsPerms::ReadWrite) {
                return Run {
                    end: End::Crashed(format!("no proof directory: {e}")),
                    stdout: String::new(),
                    fuel: 0,
                    wall: Duration::ZERO,
                    proof_too_large: false,
                };
            }
        }
        wasi.stdin(MemoryInputPipe::new(input.as_bytes().to_vec()))
            .stdout(stdout.clone())
            .secure_random(StdRng::seed_from_u64(seed))
            .insecure_random(StdRng::seed_from_u64(seed ^ 0x9e37_79b9_7f4a_7c15))
            .insecure_random_seed(u128::from(seed))
            .monotonic_clock(VirtualClock(AtomicU64::new(0)))
            .wall_clock(VirtualClock(AtomicU64::new(0)));
        if show_stderr {
            wasi.inherit_stderr();
        }
        let state = State {
            wasi: wasi.build_p1(),
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_bytes)
                .memories(1)
                .tables(16)
                .table_elements(1 << 20)
                .instances(1)
                .build(),
            proof_written: 0,
            proof_cap: limits.proof_bytes,
            proof_too_large: false,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        let _ = store.set_fuel(limits.fuel);
        let ticks = (limits.wall.as_millis() / TICK.as_millis()).max(1);
        store.set_epoch_deadline(u64::try_from(ticks).unwrap_or(u64::MAX));
        store.epoch_deadline_trap();

        let started = Instant::now();
        // Epochs interrupt running Wasm, but cannot wake a host call such as poll_oneoff.
        // Async WASI yields those calls, so dropping the timed-out future cancels them too.
        let result = self.runtime.block_on(async {
            tokio::time::timeout(limits.wall, async {
                let instance = self
                    .linker
                    .instantiate_async(&mut store, &solver.module)
                    .await?;
                let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
                start.call_async(&mut store, ()).await
            })
            .await
        });
        let wall = started.elapsed();
        let fuel = limits.fuel - store.get_fuel().unwrap_or(0);
        let end = match result {
            Err(_) => End::WallClock,
            Ok(Ok(())) => End::Exited,
            Ok(Err(e)) if e.downcast_ref::<I32Exit>().is_some() => End::Exited,
            Ok(Err(e)) => match e.downcast_ref::<Trap>() {
                Some(Trap::OutOfFuel) => End::OutOfFuel,
                Some(Trap::Interrupt) => End::WallClock,
                _ => End::Crashed(format!("{e:#}")),
            },
        };
        Run {
            end,
            stdout: String::from_utf8_lossy(&stdout.contents()).into_owned(),
            fuel,
            wall,
            proof_too_large: store.data().proof_too_large,
        }
    }
}

/// Test solvers written in WebAssembly text.
#[cfg(test)]
pub(crate) mod testing {
    /// A byte string as a WAT data string.
    fn escaped(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "\\{b:02x}");
            s
        })
    }

    /// The WAT of a solver that answers UNSAT, prints `stdout` before it, and with `proof`
    /// writes that to the proof file given as `argv[1]` (opened at `path`, `proof.out` in the
    /// preopened directory, fd 3, by default).
    pub(crate) fn unsat_wat(proof: Option<&[u8]>, stdout: &str, path: &str) -> String {
        let out = format!("{stdout}s UNSATISFIABLE\n");
        let write_proof = proof.map_or(String::new(), |proof| {
            format!(
                r"(drop (call $open (i32.const 3) (i32.const 0) (i32.const 4096) (i32.const {})
                      (i32.const 9) (i64.const 0x1fffffff) (i64.const 0x1fffffff) (i32.const 0) (i32.const 200)))
                   (i32.store (i32.const 16) (i32.const 8192))
                   (i32.store (i32.const 20) (i32.const {}))
                   (drop (call $write (i32.load (i32.const 200)) (i32.const 16) (i32.const 1) (i32.const 24)))",
                path.len(),
                proof.len()
            )
        });
        format!(
            r#"(module
                (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
                (memory (export "memory") 2)
                (data (i32.const 4096) "{path}")
                (data (i32.const 8192) "{proof}")
                (data (i32.const 1024) "{out}")
                (func (export "_start")
                    {write_proof}
                    (i32.store (i32.const 0) (i32.const 1024))
                    (i32.store (i32.const 4) (i32.const {len}))
                    (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#,
            path = escaped(path.as_bytes()),
            proof = escaped(proof.unwrap_or_default()),
            out = escaped(out.as_bytes()),
            len = out.len(),
        )
    }

    /// [`unsat_wat`] compiled.
    pub(crate) fn unsat_solver(proof: Option<&[u8]>, stdout: &str) -> Vec<u8> {
        wat::parse_str(unsat_wat(proof, stdout, crate::proof::PROOF_FILE)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WASI: &str = r#"
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 100) "s UNSATISFIABLE\n")
        ;; iovec at 0: 16 bytes at 100
        (data (i32.const 0) "\64\00\00\00\10\00\00\00")"#;

    fn solver(sandbox: &Sandbox, body: &str) -> Solver {
        let wasm = wat::parse_str(format!(
            r#"(module {WASI} (func (export "_start") {body}))"#
        ))
        .unwrap();
        sandbox.load(&wasm).unwrap()
    }

    fn limits(fuel: u64) -> Limits {
        Limits {
            fuel,
            ..Default::default()
        }
    }

    /// A module can pass our byte/operator limits and Wasm validation but exceed the
    /// compiler's jump-table limit. Return a submission rejection and keep serving runs.
    #[test]
    fn compiler_limits_reject_submissions_without_poisoning_the_sandbox() {
        let sandbox = Sandbox::new().unwrap();
        let targets = "0 ".repeat(65_521);
        let wasm = wat::parse_str(format!(
            r#"(module {WASI}
                (func (export "_start")
                    (block (br_table {targets} 0 (i32.const 0)))))"#
        ))
        .unwrap();
        validate(&wasm).expect("within the application's upload and validation limits");
        let rejection = sandbox.load(&wasm).unwrap_err();
        assert!(
            matches!(&rejection, Rejection::Invalid(message)
                if message.contains("br_table") && message.contains("implementation limit")),
            "{rejection:?}"
        );

        let print = "(drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))";
        let run = sandbox.run(&solver(&sandbox, print), "", 1, &limits(1_000), false, None);
        assert_eq!(run.end, End::Exited);
        assert_eq!(run.stdout, "s UNSATISFIABLE\n");
    }

    #[test]
    fn answers_and_meters() {
        let sandbox = Sandbox::new().unwrap();
        let print = "(drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))";
        let run = sandbox.run(
            &solver(&sandbox, print),
            "p cnf 1 1\n1 0\n",
            1,
            &limits(1_000_000),
            false,
            None,
        );
        assert_eq!(run.end, End::Exited);
        assert_eq!(run.stdout, "s UNSATISFIABLE\n", "{run:?}");
        let small = run.fuel;
        assert!(small > 0 && small < 1_000);

        // Same program and input: same fuel, every time
        let again = sandbox.run(
            &solver(&sandbox, print),
            "p cnf 1 1\n1 0\n",
            1,
            &limits(1_000_000),
            false,
            None,
        );
        assert_eq!(again.fuel, small);

        // Bulk operations cost one unit per byte (in the leaderboard's model too): filling 64 KiB
        // costs 65535 more than one byte
        let fill =
            |n: u32| format!("(memory.fill (i32.const 0) (i32.const 0) (i32.const {n})) {print}");
        let one = sandbox
            .run(
                &solver(&sandbox, &fill(1)),
                "",
                1,
                &limits(1_000_000),
                false,
                None,
            )
            .fuel;
        let many = sandbox
            .run(
                &solver(&sandbox, &fill(65536)),
                "",
                1,
                &limits(1_000_000),
                false,
                None,
            )
            .fuel;
        assert_eq!(many - one, 65535);
        // ... and runs out of fuel if the budget is smaller than that
        let poor = sandbox.run(
            &solver(&sandbox, &fill(65536)),
            "",
            1,
            &limits(10_000),
            false,
            None,
        );
        assert_eq!(poor.end, End::OutOfFuel);
        assert_eq!(poor.fuel, 10_000);
    }

    /// Every cost table charges its categories: ten loads (each with a constant address),
    /// a fill of 100 bytes and a grow by two pages, counted one category at a time. Entering
    /// `_start` costs one unit in every table (wasmtime charges function entries).
    #[test]
    fn cost_tables_apply() {
        use crate::fuel::{Category, Weights};
        let body = format!(
            "{} (memory.fill (i32.const 0) (i32.const 0) (i32.const 100)) \
             (drop (memory.grow (i32.const 2)))",
            "(drop (i32.load (i32.const 0))) ".repeat(10)
        );
        let wasm = wat::parse_str(format!(
            r#"(module {WASI} (func (export "_start") {body}))"#
        ))
        .unwrap();
        let fuel = |weights: &Weights| {
            let sandbox = Sandbox::with_weights(weights).unwrap();
            let run = sandbox.run(
                &sandbox.load(&wasm).unwrap(),
                "",
                1,
                &limits(1_000_000),
                false,
                None,
            );
            assert_eq!(run.end, End::Exited);
            run.fuel
        };
        assert_eq!(fuel(&Weights::ZERO), 1);
        assert_eq!(fuel(&Weights::only(Category::Load)), 10 + 1);
        // The loads' addresses, the fill's three operands, the grow's one
        assert_eq!(fuel(&Weights::only(Category::Local)), 14 + 1);
        assert_eq!(fuel(&Weights::only(Category::Bulk)), 1 + 100 + 1);
        assert_eq!(fuel(&Weights::only(Category::Grow)), 1 + 2 + 1);
        assert_eq!(fuel(&Weights::only(Category::Alu)), 1);
        assert_eq!(fuel(&Weights::UNIFORM), 10 + 14 + 101 + 3 + 1);
        let mut heavy = Weights::UNIFORM;
        heavy.set(Category::Load, 5);
        assert_eq!(fuel(&heavy), 50 + 14 + 101 + 3 + 1);
    }

    #[test]
    fn limits_hold() {
        let sandbox = Sandbox::new().unwrap();
        let spin = solver(&sandbox, "(loop $l (br $l))");
        assert_eq!(
            sandbox.run(&spin, "", 1, &limits(100_000), false, None).end,
            End::OutOfFuel
        );
        let wall = Limits {
            fuel: u64::MAX / 2,
            wall: Duration::from_millis(50),
            ..Default::default()
        };
        assert_eq!(
            sandbox.run(&spin, "", 1, &wall, false, None).end,
            End::WallClock
        );
        let crash = solver(&sandbox, "unreachable");
        assert!(matches!(
            sandbox.run(&crash, "", 1, &limits(1_000), false, None).end,
            End::Crashed(_)
        ));
        let greedy = solver(&sandbox, "(drop (memory.grow (i32.const 30000)))");
        let small = Limits {
            memory_bytes: 1 << 20,
            ..limits(1_000)
        };
        // A failed grow returns -1, the program goes on
        assert_eq!(
            sandbox.run(&greedy, "", 1, &small, false, None).end,
            End::Exited
        );
    }

    fn run_with_proof(sandbox: &Sandbox, wasm: &[u8], limits: &Limits) -> (Run, Option<Vec<u8>>) {
        let dir = crate::proof::TempDir::new(&std::env::temp_dir(), "dojo-sandbox-test").unwrap();
        let solver = sandbox.load(wasm).unwrap();
        let run = sandbox.run(
            &solver,
            "p cnf 1 2\n1 0\n-1 0\n",
            1,
            limits,
            false,
            Some(dir.path()),
        );
        let proof = std::fs::read(dir.path().join(PROOF_FILE)).ok();
        (run, proof)
    }

    /// The proof file named by argv[1] is written to the run's directory on disk.
    #[test]
    fn proofs_go_to_the_proof_file() {
        let sandbox = Sandbox::new().unwrap();
        let wasm = testing::unsat_solver(Some(b"1 0\n0\n"), "c proof drat\n");
        let (run, proof) = run_with_proof(&sandbox, &wasm, &Limits::default());
        assert_eq!(run.end, End::Exited, "{run:?}");
        assert_eq!(run.stdout, "c proof drat\ns UNSATISFIABLE\n");
        assert_eq!(proof.as_deref(), Some(&b"1 0\n0\n"[..]));
        assert!(!run.proof_too_large);
    }

    /// Writing more than the limit fails (EFBIG), the run goes on and the proof is too large.
    #[test]
    fn proofs_are_capped() {
        let sandbox = Sandbox::new().unwrap();
        let wasm = testing::unsat_solver(Some(b"1 0\n0\n"), "");
        let small = Limits {
            proof_bytes: 4,
            ..Limits::default()
        };
        let (run, proof) = run_with_proof(&sandbox, &wasm, &small);
        assert_eq!(run.end, End::Exited);
        assert_eq!(run.stdout, "s UNSATISFIABLE\n");
        assert!(run.proof_too_large);
        assert_eq!(proof.as_deref(), Some(&b""[..]));
        // Without a proof directory there is nothing to open, the answer still comes
        let solver = sandbox.load(&wasm).unwrap();
        let run = sandbox.run(&solver, "", 1, &Limits::default(), false, None);
        assert_eq!(
            (run.end, run.stdout.as_str()),
            (End::Exited, "s UNSATISFIABLE\n")
        );
    }

    /// Only `proof.out` can be opened, nothing can be created, linked or renamed, and files
    /// can't grow beyond the limit by seeking or truncating.
    #[test]
    fn nothing_but_the_proof_file() {
        let sandbox = Sandbox::new().unwrap();
        let other = wat::parse_str(testing::unsat_wat(Some(b"0\n"), "", "other.out")).unwrap();
        let (run, proof) = run_with_proof(&sandbox, &other, &Limits::default());
        assert_eq!(run.end, End::Exited);
        assert_eq!(proof, None);
        let dot = wat::parse_str(testing::unsat_wat(Some(b"0\n"), "", "./proof.out")).unwrap();
        assert_eq!(
            run_with_proof(&sandbox, &dot, &Limits::default())
                .1
                .as_deref(),
            Some(&b"0\n"[..])
        );

        // Every call has to fail with the expected errno for the answer to be printed
        let calls = format!(
            r#"(module
                (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_create_directory" (func $mkdir (param i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_symlink" (func $symlink (param i32 i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_link" (func $link (param i32 i32 i32 i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_rename" (func $rename (param i32 i32 i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_pwrite" (func $pwrite (param i32 i32 i32 i64 i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_filestat_set_size" (func $truncate (param i32 i64) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 100) "s UNSATISFIABLE\n")
                (data (i32.const 0) "\64\00\00\00\10\00\00\00")
                (data (i32.const 300) "{PROOF_FILE}")
                (data (i32.const 400) "dir")
                (func $expect (param i32 i32) (if (i32.ne (local.get 0) (local.get 1)) (then unreachable)))
                (func (export "_start")
                    (call $expect (call $open (i32.const 3) (i32.const 0) (i32.const 300) (i32.const 9) (i32.const 9)
                        (i64.const 0x1fffffff) (i64.const 0x1fffffff) (i32.const 0) (i32.const 200)) (i32.const 0))
                    (call $expect (call $mkdir (i32.const 3) (i32.const 400) (i32.const 3)) (i32.const {ERRNO_ACCES}))
                    (call $expect (call $symlink (i32.const 300) (i32.const 9) (i32.const 3) (i32.const 400) (i32.const 3)) (i32.const {ERRNO_ACCES}))
                    (call $expect (call $link (i32.const 3) (i32.const 0) (i32.const 300) (i32.const 9) (i32.const 3) (i32.const 400) (i32.const 3)) (i32.const {ERRNO_ACCES}))
                    (call $expect (call $rename (i32.const 3) (i32.const 300) (i32.const 9) (i32.const 3) (i32.const 400) (i32.const 3)) (i32.const {ERRNO_ACCES}))
                    (call $expect (call $truncate (i32.load (i32.const 200)) (i64.const 10)) (i32.const 0))
                    (call $expect (call $pwrite (i32.load (i32.const 200)) (i32.const 0) (i32.const 1) (i64.const 0) (i32.const 8)) (i32.const 0))
                    ;; Beyond the limit of 1000: growing the file, or sixteen bytes at 2000
                    (call $expect (call $truncate (i32.load (i32.const 200)) (i64.const 2000)) (i32.const {ERRNO_FBIG}))
                    (call $expect (call $pwrite (i32.load (i32.const 200)) (i32.const 0) (i32.const 1) (i64.const 2000) (i32.const 8)) (i32.const {ERRNO_FBIG}))
                    (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))))"#
        );
        let limits = Limits {
            proof_bytes: 1000,
            ..Limits::default()
        };
        let (run, proof) = run_with_proof(&sandbox, &wat::parse_str(calls).unwrap(), &limits);
        assert_eq!(run.end, End::Exited, "{run:?}");
        assert_eq!(run.stdout, "s UNSATISFIABLE\n");
        // Truncated to ten bytes, then sixteen written at the start
        assert_eq!(proof.map(|p| p.len()), Some(16));
        assert!(run.proof_too_large);
    }

    #[test]
    fn blocking_host_calls_obey_the_wall_deadline() {
        let sandbox = Sandbox::new().unwrap();
        let wasm = wat::parse_str(r#"(module
            (import "wasi_snapshot_preview1" "poll_oneoff" (func $poll (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "_start")
                (i32.store (i32.const 16) (i32.const 1))
                (i64.store (i32.const 24) (i64.const 5000000000))
                (drop (call $poll (i32.const 0) (i32.const 128) (i32.const 1) (i32.const 256)))))"#).unwrap();
        let solver = sandbox.load(&wasm).unwrap();
        let limits = Limits {
            wall: Duration::from_millis(50),
            ..Limits::default()
        };
        let run = sandbox.run(&solver, "", 1, &limits, false, None);
        assert_eq!(run.end, End::WallClock);
        assert!(
            run.wall < Duration::from_secs(2),
            "host sleep escaped the deadline: {:?}",
            run.wall
        );
    }
}
