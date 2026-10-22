//! The app's offline practice: the puzzle server functions of the website (`src/api/puzzle.rs`:
//! `generate_puzzle`, `solve_puzzle`, `puzzle_cnf`) on the phone. They call the same `backend`
//! functions with the same arguments and limits, so the phone's answers are the server's: a seed
//! gives the puzzle the server generates for it, on any machine (the generator counts work, not
//! time).
//!
//! The Tauri commands in `../src/lib.rs` only call [`json`]. Without Tauri here, the tests build
//! and run on any machine, without the system libraries Tauri needs on the desktop.
//!
//! The server also refuses to solve today's daily for players who haven't finished it; the page
//! checks that itself before it calls a command (`src/api/practice.rs`).

use std::future::Future;

use common::{GenerateParams, GeneratedPuzzle, Puzzle, SolveReport, SolverConfig};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;

/// The error of a puzzle that can't be solved, as the server sends it (`ApiError::WrongPuzzle`
/// on the website, `E:<code>`), so the page shows the same text.
pub const WRONG_PUZZLE: &str = "E:wrong_puzzle";
/// Too many computations at once (`ApiError::TooManyAttempts`).
pub const BUSY: &str = "E:too_many_attempts";

/// The phone's computations: two at a time like the server's, a few more waiting. Every one of
/// them ends on its own (the generator's work budget, the solver's time limit), so the waiting
/// ones don't need a time limit of their own. A computation can't be stopped once it runs: the
/// page ignores the answer of one it no longer needs (the epochs of `src/state/solver.rs`).
static PUZZLES: Pool = Pool::new(2, 6);

/// Admission to the computations: `running` at a time, `waiting` more in the queue, every
/// further one is refused.
#[derive(Debug)]
pub struct Pool {
    running: Semaphore,
    admitted: Semaphore,
}

impl Pool {
    pub const fn new(running: usize, waiting: usize) -> Self {
        Self {
            running: Semaphore::const_new(running),
            admitted: Semaphore::const_new(running + waiting),
        }
    }

    /// Runs `job` on a thread of the async runtime's blocking pool, off the thread that called.
    pub async fn run<T: Send + 'static>(
        &'static self,
        job: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let admitted = self.admitted.try_acquire().map_err(|_| BUSY.to_string())?;
        let permit = self.running.acquire().await.map_err(|e| e.to_string())?;
        tokio::task::spawn_blocking(move || {
            let (_permit, _admitted) = (permit, admitted);
            job()
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Generates a new puzzle with a unique solution, as the server function `generate_puzzle`.
pub async fn generate_puzzle(params: GenerateParams) -> Result<GeneratedPuzzle, String> {
    PUZZLES
        .run(move || Ok(backend::api::generate(&params)))
        .await
}

/// Solves the puzzle with the chosen SAT solver, with `unique` also whether the solution is the
/// only one, as the server function `solve_puzzle`: the same time limits.
pub async fn solve_puzzle(
    puzzle: Puzzle,
    config: SolverConfig,
    unique: bool,
) -> Result<SolveReport, String> {
    let puzzle = normalized(puzzle)?;
    let config = backend::api::player_config(config);
    PUZZLES
        .run(move || {
            if unique {
                backend::api::solve_unique(&puzzle, &config)
            } else {
                backend::api::solve(&puzzle, &config)
            }
        })
        .await
}

/// The CNF encoding of the puzzle in DIMACS format, as the server function `puzzle_cnf`.
pub async fn puzzle_cnf(puzzle: Puzzle) -> Result<String, String> {
    let puzzle = normalized(puzzle)?;
    PUZZLES.run(move || backend::api::to_dimacs(&puzzle)).await
}

fn normalized(puzzle: Puzzle) -> Result<Puzzle, String> {
    backend::api::player_puzzle(puzzle).map_err(|_| WRONG_PUZZLE.to_string())
}

/// The functions above as the app's commands: they take the server functions' request bodies
/// (the JSON in `src/golden/files/requests.txt`) and answer with the JSON of the server
/// functions' answers, both as text. The page sends them as strings because JavaScript's numbers
/// would round the 64-bit seeds (`src/api/native.rs`).
pub mod json {
    use serde::Deserialize;

    use super::{GenerateParams, Puzzle, SolverConfig, answer};

    #[derive(Deserialize)]
    struct Generate {
        params: GenerateParams,
    }

    #[derive(Deserialize)]
    struct Solve {
        puzzle: Puzzle,
        config: SolverConfig,
        unique: bool,
    }

    #[derive(Deserialize)]
    struct Encode {
        puzzle: Puzzle,
    }

    /// [`super::generate_puzzle`] on `{"params": …}`.
    pub async fn generate_puzzle(args: &str) -> Result<String, String> {
        answer(args, |a: Generate| super::generate_puzzle(a.params)).await
    }

    /// [`super::solve_puzzle`] on `{"puzzle": …, "config": …, "unique": …}`.
    pub async fn solve_puzzle(args: &str) -> Result<String, String> {
        answer(args, |a: Solve| {
            super::solve_puzzle(a.puzzle, a.config, a.unique)
        })
        .await
    }

    /// [`super::puzzle_cnf`] on `{"puzzle": …}`.
    pub async fn puzzle_cnf(args: &str) -> Result<String, String> {
        answer(args, |a: Encode| super::puzzle_cnf(a.puzzle)).await
    }
}

async fn answer<A, T, F>(args: &str, run: impl FnOnce(A) -> F) -> Result<String, String>
where
    A: DeserializeOwned,
    T: Serialize,
    F: Future<Output = Result<T, String>>,
{
    let args = serde_json::from_str(args).map_err(|e| e.to_string())?;
    let answer = run(args).await?;
    serde_json::to_string(&answer).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use common::{Difficulty, Heuristic, Island, Pos, SolveOutcome, SolverKind};

    use super::*;

    fn params(difficulty: Difficulty, seed: u64) -> GenerateParams {
        let (width, height) = difficulty.size();
        GenerateParams {
            width,
            height,
            difficulty,
            seed,
        }
    }

    /// A seed gives the phone the server's puzzle: the same function with the same arguments.
    #[tokio::test]
    async fn a_seed_gives_the_servers_puzzle() {
        for (difficulty, seed) in [(Difficulty::Easy, 7), (Difficulty::Hard, 2026)] {
            let phone = generate_puzzle(params(difficulty, seed)).await.unwrap();
            let server = backend::api::generate(&params(difficulty, seed));
            assert_eq!(phone.puzzle, server.puzzle, "{difficulty:?}");
            assert_eq!(phone.solution, server.solution);
            assert_eq!(
                (phone.params, phone.unique, phone.attempts),
                (server.params, server.unique, server.attempts)
            );
            assert!(phone.unique);
        }
    }

    /// Solving and encoding answer like the server functions, also for islands out of order
    /// (what a page may send), and refuse what can't be encoded with the server's error.
    #[tokio::test]
    async fn solves_like_the_server() {
        let generated = backend::api::generate(&params(Difficulty::Medium, 11));
        let mut shuffled = generated.puzzle.clone();
        shuffled.islands.reverse();
        let config = SolverConfig::splr();
        let report = solve_puzzle(shuffled.clone(), config, true).await.unwrap();
        assert_eq!(
            report.outcome,
            SolveOutcome::Solved {
                bridges: generated.solution.clone(),
                unique: Some(true),
            }
        );
        let report = solve_puzzle(shuffled.clone(), config, false).await.unwrap();
        assert!(matches!(
            report.outcome,
            SolveOutcome::Solved { unique: None, .. }
        ));
        assert_eq!(
            puzzle_cnf(shuffled).await.unwrap(),
            backend::api::to_dimacs(&generated.puzzle).unwrap()
        );

        let empty = Puzzle::new(5, 5, Vec::new());
        let island = |x, y, required| Island {
            pos: Pos { x, y },
            required,
        };
        let outside = Puzzle {
            width: 3,
            height: 3,
            islands: vec![island(0, 0, 1), island(5, 0, 1)],
        };
        for puzzle in [empty, outside] {
            let solved = solve_puzzle(puzzle.clone(), SolverConfig::splr(), false).await;
            assert_eq!(solved.unwrap_err(), WRONG_PUZZLE);
            assert_eq!(puzzle_cnf(puzzle).await.unwrap_err(), WRONG_PUZZLE);
        }
    }

    /// The server's time limits: at least 100 ms (0 would be the backend's default of 10 s),
    /// at most 20 s however long the page asks for.
    #[tokio::test]
    async fn the_servers_time_limits() {
        let puzzle = backend::api::generate(&params(Difficulty::Easy, 3)).puzzle;
        let limit = |timeout_ms| {
            let puzzle = puzzle.clone();
            async move {
                let config = SolverConfig {
                    timeout_ms,
                    ..SolverConfig::splr()
                };
                let report = solve_puzzle(puzzle, config, false).await.unwrap();
                report.stats.config.timeout_ms
            }
        };
        assert_eq!(limit(0).await, 100);
        assert_eq!(limit(u32::MAX).await, 20_000);
        assert_eq!(limit(5_000).await, 5_000);
    }

    /// A search that takes longer than its time limit stops there and says so.
    #[tokio::test]
    async fn a_slow_search_times_out() {
        let puzzle = backend::api::generate(&GenerateParams {
            width: 30,
            height: 30,
            difficulty: Difficulty::Oni,
            seed: 5,
        })
        .puzzle;
        // The plain DPLL without learning, at the shortest limit there is
        let config = SolverConfig {
            kind: SolverKind::Dpll,
            heuristic: Heuristic::Arbitrary,
            timeout_ms: 1,
            ..SolverConfig::default()
        };
        let start = Instant::now();
        let report = solve_puzzle(puzzle, config, true).await.unwrap();
        assert_eq!(report.outcome, SolveOutcome::Timeout);
        assert_eq!(report.stats.config.timeout_ms, 100);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }

    /// The commands take what the page sends the server functions: their request bodies as the
    /// website's golden files have them, and answer with the server functions' JSON. The seed
    /// of the golden request is `u64::MAX`, which a JavaScript number can't hold.
    #[tokio::test]
    async fn the_commands_take_the_server_functions_requests() {
        let requests = include_str!("../../../../src/golden/files/requests.txt");
        let body = |name: &str| {
            let path = format!("/api/{name} ");
            requests
                .lines()
                .find_map(|l| l.strip_prefix(&path))
                .unwrap()
        };

        let answer = json::generate_puzzle(body("generate_puzzle"))
            .await
            .unwrap();
        let generated: GeneratedPuzzle = serde_json::from_str(&answer).unwrap();
        let params = GenerateParams {
            width: 7,
            height: 7,
            difficulty: Difficulty::Hard,
            seed: u64::MAX,
        };
        assert_eq!(generated.params, params);
        assert_eq!(generated.puzzle, backend::api::generate(&params).puzzle);

        let answer = json::solve_puzzle(body("solve_puzzle")).await.unwrap();
        let report: SolveReport = serde_json::from_str(&answer).unwrap();
        assert!(matches!(
            report.outcome,
            SolveOutcome::Solved {
                unique: Some(_),
                ..
            }
        ));
        let answer = json::puzzle_cnf(body("puzzle_cnf")).await.unwrap();
        let cnf: String = serde_json::from_str(&answer).unwrap();
        assert!(cnf.starts_with("c "), "{cnf}");

        // What isn't such a body
        assert!(json::generate_puzzle("{}").await.is_err());
        assert!(json::puzzle_cnf("{\"puzzle\": 1}").await.is_err());
    }

    /// The packages of a `Cargo.lock`: (name, version) and the dependencies, each named as in
    /// the file (`name` or `name version` when there are several versions).
    fn packages(lock: &str) -> Vec<((&str, &str), Vec<&str>)> {
        fn quoted<'a>(line: &'a str, key: &str) -> Option<&'a str> {
            let value = line.strip_prefix(key)?.strip_prefix(" = \"")?;
            value.strip_suffix('"')
        }
        lock.split("[[package]]")
            .skip(1)
            .map(|entry| {
                let lines: Vec<&str> = entry.lines().map(str::trim).collect();
                let field = |key| lines.iter().find_map(|l| quoted(l, key)).unwrap();
                let dependencies = lines
                    .iter()
                    .skip_while(|l| !l.starts_with("dependencies"))
                    .skip(1)
                    .take_while(|l| **l != "]")
                    .map(|l| l.trim_matches(|c| c == '"' || c == ','))
                    .collect();
                ((field("name"), field("version")), dependencies)
            })
            .collect()
    }

    /// The app's `Cargo.lock` has the versions of the server's (the repository's) for everything
    /// the generator and the solvers use: another rand or splr generates other puzzles from the
    /// same seed (see `backend/tests/test_golden.rs`). After an update in one of them, update
    /// the other: `cargo update -p <name> --precise <version>`.
    #[test]
    fn the_server_and_the_app_lock_the_same_solvers() {
        let app = packages(include_str!("../../Cargo.lock"));
        let server = packages(include_str!("../../../../Cargo.lock"));
        let find = |name: &str| {
            let (name, version) = name
                .split_once(' ')
                .map_or((name, None), |(n, v)| (n, Some(v)));
            app.iter()
                .find(|((n, v), _)| *n == name && version.is_none_or(|version| version == *v))
                .unwrap_or_else(|| panic!("{name} is not in the app's Cargo.lock"))
        };
        let mut todo = vec!["backend"];
        let mut seen = Vec::new();
        while let Some(name) = todo.pop() {
            let (package, dependencies) = find(name);
            if !seen.contains(package) {
                seen.push(*package);
                todo.extend(dependencies);
            }
        }
        assert!(seen.iter().any(|(name, _)| *name == "splr"));
        for package in seen {
            assert!(
                server.iter().any(|(p, _)| *p == package),
                "{package:?} of the app's Cargo.lock is not in the server's"
            );
        }
    }

    /// Two computations run at a time, the next ones wait their turn, and beyond the queue the
    /// pool refuses with the server's error. Runs off the caller's thread.
    #[tokio::test(flavor = "multi_thread")]
    async fn two_at_a_time_and_a_short_queue() {
        static POOL: Pool = Pool::new(2, 1);
        let caller = std::thread::current().id();
        let (started, starts) = mpsc::channel();
        let mut gates = Vec::new();
        let mut jobs = Vec::new();
        for i in 0..3 {
            let (open, gate) = mpsc::channel::<()>();
            gates.push(open);
            let started = started.clone();
            jobs.push(tokio::spawn(POOL.run(move || {
                started.send((i, std::thread::current().id())).unwrap();
                gate.recv().unwrap();
                Ok(i)
            })));
        }
        let first: Vec<_> = (0..2)
            .map(|_| starts.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect();
        assert!(first.iter().all(|&(_, thread)| thread != caller));
        // Both threads are taken, the third job waits in the queue, which is full then
        let queued = Instant::now();
        while POOL.admitted.available_permits() > 0 {
            assert!(queued.elapsed() < Duration::from_secs(5));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(starts.recv_timeout(Duration::from_millis(200)).is_err());
        let refused = POOL.run(|| Ok(9)).await;
        assert_eq!(refused, Err(BUSY.to_string()));

        for gate in &gates {
            gate.send(()).unwrap();
        }
        let mut results = Vec::new();
        for job in jobs {
            results.push(job.await.unwrap().unwrap());
        }
        assert_eq!(results, [0, 1, 2]);
        // Room again
        assert_eq!(POOL.run(|| Ok(4)).await, Ok(4));
    }
}
