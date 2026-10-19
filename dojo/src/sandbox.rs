//! Running a submission on one formula, sandboxed and metered.
//!
//! - WASI preview 1 with stdin (the formula), stdout (the answer, at most 8 MiB) and nothing
//!   else: no files, no environment, no network.
//! - Fuel: every executed instruction costs one unit, bulk memory and table operations one per
//!   byte or element. The budget replaces a time limit, so results are exactly reproducible.
//! - Clocks only move when they are read and random numbers come from a seed, so even solvers
//!   that use time or randomness give the same result every time.
//! - Floating point NaNs are canonicalised and relaxed SIMD is deterministic, so results are the
//!   same on every CPU.
//! - Memory is capped and a wall clock limit catches anything fuel doesn't (e.g. a host call
//!   that blocks).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rand::SeedableRng;
use rand::rngs::StdRng;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::{HostMonotonicClock, HostWallClock, I32Exit, WasiCtxBuilder};

use crate::Rejection;
use crate::validate::validate;

/// How often the wall clock limit is checked.
const TICK: Duration = Duration::from_millis(10);
const MAX_STDOUT: usize = 8 << 20;

#[derive(Clone, Debug)]
pub struct Limits {
    pub fuel: u64,
    pub memory_bytes: usize,
    pub wall: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: crate::DEFAULT_BUDGET,
            memory_bytes: 1 << 30,
            wall: Duration::from_secs(120),
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
    pub fn new() -> Result<Self, String> {
        // Without the `threads` feature wasmtime has no shared memories at all
        let mut config = Config::new();
        config
            .consume_fuel(true)
            .epoch_interruption(true)
            .wasm_memory64(false)
            .wasm_multi_memory(false)
            .cranelift_nan_canonicalization(true)
            .relaxed_simd_deterministic(true);
        let engine = Engine::new(&config).map_err(|e| e.to_string())?;

        let mut linker: Linker<State> = Linker::new(&engine);
        p1::add_to_linker_async(&mut linker, |s| &mut s.wasi).map_err(|e| e.to_string())?;
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

    /// Runs the solver on `input` (DIMACS text). `seed` seeds its random numbers.
    pub fn run(
        &self,
        solver: &Solver,
        input: &str,
        seed: u64,
        limits: &Limits,
        show_stderr: bool,
    ) -> Run {
        let stdout = MemoryOutputPipe::new(MAX_STDOUT);
        let mut wasi = WasiCtxBuilder::new();
        wasi.args(&["solver"])
            .stdin(MemoryInputPipe::new(input.as_bytes().to_vec()))
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
        }
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
        );
        assert_eq!(run.end, End::Exited);
        assert_eq!(run.stdout, "s UNSATISFIABLE\n");
        let small = run.fuel;
        assert!(small > 0 && small < 1_000);

        // Same program and input: same fuel, every time
        let again = sandbox.run(
            &solver(&sandbox, print),
            "p cnf 1 1\n1 0\n",
            1,
            &limits(1_000_000),
            false,
        );
        assert_eq!(again.fuel, small);

        // Bulk operations cost one unit per byte: filling 64 KiB costs 65535 more than one byte
        let fill =
            |n: u32| format!("(memory.fill (i32.const 0) (i32.const 0) (i32.const {n})) {print}");
        let one = sandbox
            .run(
                &solver(&sandbox, &fill(1)),
                "",
                1,
                &limits(1_000_000),
                false,
            )
            .fuel;
        let many = sandbox
            .run(
                &solver(&sandbox, &fill(65536)),
                "",
                1,
                &limits(1_000_000),
                false,
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
        );
        assert_eq!(poor.end, End::OutOfFuel);
        assert_eq!(poor.fuel, 10_000);
    }

    #[test]
    fn limits_hold() {
        let sandbox = Sandbox::new().unwrap();
        let spin = solver(&sandbox, "(loop $l (br $l))");
        assert_eq!(
            sandbox.run(&spin, "", 1, &limits(100_000), false).end,
            End::OutOfFuel
        );
        let wall = Limits {
            fuel: u64::MAX / 2,
            wall: Duration::from_millis(50),
            ..Default::default()
        };
        assert_eq!(sandbox.run(&spin, "", 1, &wall, false).end, End::WallClock);
        let crash = solver(&sandbox, "unreachable");
        assert!(matches!(
            sandbox.run(&crash, "", 1, &limits(1_000), false).end,
            End::Crashed(_)
        ));
        let greedy = solver(&sandbox, "(drop (memory.grow (i32.const 30000)))");
        let small = Limits {
            memory_bytes: 1 << 20,
            ..limits(1_000)
        };
        // A failed grow returns -1, the program goes on
        assert_eq!(sandbox.run(&greedy, "", 1, &small, false).end, End::Exited);
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
        let run = sandbox.run(&solver, "", 1, &limits, false);
        assert_eq!(run.end, End::WallClock);
        assert!(
            run.wall < Duration::from_secs(2),
            "host sleep escaped the deadline: {:?}",
            run.wall
        );
    }
}
