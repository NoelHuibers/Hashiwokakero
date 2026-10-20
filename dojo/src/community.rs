//! Community benchmarks ("bring your own benchmark"), like the SAT Competition's: what the
//! website and the worker share about them.
//!
//! - Identity: [`gbd_hash`] is the instance hash of the Global Benchmark Database (the MD5 of the
//!   formula's numbers without comments and header, one space between them), so a benchmark
//!   fetched from GBD and the same file uploaded are one; [`shape_hash`] stays the same when
//!   variables are renamed or negated and clauses or literals reordered (in the spirit of GBD's
//!   isohash), so a scrambled copy is caught too.
//! - Selection: every season's hidden set takes a few accepted benchmarks of the pool
//!   ([`select`]), drawn like the SAT Competition 2026 picks its instances: at most
//!   [`PER_AUTHOR`] per author and [`PER_FAMILY`] per family, rare authors and families more
//!   likely, as many SAT as UNSAT. The draw is seeded with the SHA-256 of the season's secret
//!   seed and the sorted hashes of the pool, so nobody can predict it, and anyone can repeat it
//!   once the seed is revealed.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Answer;

/// Community benchmarks in a hidden set, if the pool has that many.
pub const PER_SET: usize = 6;
/// At most this many benchmarks of one author in a hidden set.
pub const PER_AUTHOR: usize = 2;
/// At most this many of one family.
pub const PER_FAMILY: usize = 2;
/// Largest benchmark, as DIMACS text.
pub const MAX_BYTES: usize = 2 << 20;
pub const MAX_VARS: u32 = 200_000;
pub const MAX_CLAUSES: usize = 1_000_000;
/// The licence a submitter grants, so the instance can be published with the season.
pub const LICENSE: &str = "CC-BY-4.0";
pub const LICENSE_URL: &str = "https://creativecommons.org/licenses/by/4.0/";

/// A DIMACS formula, read leniently like SAT Competition files: comments anywhere, the header's
/// clause count may be off, a missing final 0 closes the last clause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formula {
    pub vars: u32,
    pub clauses: Vec<Vec<i32>>,
}

/// Why a file isn't a formula the Dojo takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormulaError {
    /// More than [`MAX_BYTES`], [`MAX_VARS`] or [`MAX_CLAUSES`].
    TooLarge,
    /// Not DIMACS CNF, with a hint.
    Invalid(String),
    /// No clauses.
    Empty,
}

impl Formula {
    pub fn parse(text: &str) -> Result<Self, FormulaError> {
        if text.len() > MAX_BYTES {
            return Err(FormulaError::TooLarge);
        }
        let invalid = |what: String| FormulaError::Invalid(what);
        let mut header: Option<u32> = None;
        let mut clauses = Vec::new();
        let mut clause = Vec::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('c') || line.starts_with('%') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('p') {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let vars = match parts.as_slice() {
                    ["cnf", vars, _] => vars.parse::<u32>().ok(),
                    _ => None,
                }
                .ok_or_else(|| invalid(format!("bad header: {line}")))?;
                if vars > MAX_VARS {
                    return Err(FormulaError::TooLarge);
                }
                header = Some(vars);
                continue;
            }
            let vars = header.ok_or_else(|| invalid("clauses before the p cnf line".into()))?;
            for token in line.split_whitespace() {
                let lit: i32 = token
                    .parse()
                    .map_err(|_| invalid(format!("not a literal: {token}")))?;
                if lit == 0 {
                    clauses.push(std::mem::take(&mut clause));
                    if clauses.len() > MAX_CLAUSES {
                        return Err(FormulaError::TooLarge);
                    }
                } else if lit.unsigned_abs() > vars {
                    return Err(invalid(format!("literal {lit} beyond {vars} variables")));
                } else {
                    clause.push(lit);
                }
            }
        }
        let vars = header.ok_or_else(|| invalid("no p cnf line".into()))?;
        if !clause.is_empty() {
            clauses.push(clause);
        }
        if clauses.is_empty() {
            return Err(FormulaError::Empty);
        }
        Ok(Self { vars, clauses })
    }

    /// The formula in DIMACS, without comments: what the Dojo stores and hands on.
    pub fn dimacs(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(self.clauses.len() * 12);
        let _ = writeln!(out, "p cnf {} {}", self.vars, self.clauses.len());
        for clause in &self.clauses {
            for lit in clause {
                let _ = write!(out, "{lit} ");
            }
            out.push_str("0\n");
        }
        out
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The GBD instance hash of a formula: the MD5 of its literals and clause-ending zeros, one
/// space apart (what <https://benchmark-database.de/file/HASH> serves the file under).
pub fn gbd_hash(formula: &Formula) -> String {
    use md5::Digest as _;
    let mut md5 = md5::Md5::new();
    let mut first = true;
    let mut put = |token: &str| {
        if !first {
            md5.update(b" ");
        }
        first = false;
        md5.update(token.as_bytes());
    };
    for clause in &formula.clauses {
        for lit in clause {
            put(&lit.to_string());
        }
        put("0");
    }
    hex(&md5.finalize())
}

/// A hash that renaming or negating variables and reordering clauses or literals doesn't change.
///
/// It hashes the sorted occurrence counts of every variable (the smaller polarity first) and the
/// sorted clause lengths. Different formulas can share it, but rarely; a scrambled copy always
/// does.
pub fn shape_hash(formula: &Formula) -> String {
    let mut counts = vec![(0u32, 0u32); formula.vars as usize + 1];
    for &lit in formula.clauses.iter().flatten() {
        let entry = &mut counts[lit.unsigned_abs() as usize];
        if lit > 0 {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let mut vars: Vec<(u32, u32)> = counts[1..]
        .iter()
        .map(|&(p, n)| (p.min(n), p.max(n)))
        .collect();
    vars.sort_unstable();
    let mut lengths: Vec<usize> = formula.clauses.iter().map(Vec::len).collect();
    lengths.sort_unstable();
    let mut sha = Sha256::new();
    sha.update(format!("{} {}\n", formula.vars, formula.clauses.len()));
    for (a, b) in vars {
        sha.update(format!("{a},{b} "));
    }
    sha.update("\n");
    for len in lengths {
        sha.update(format!("{len} "));
    }
    hex(&sha.finalize())
}

/// An accepted benchmark, as the selection sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolEntry {
    /// [`gbd_hash`].
    pub hash: String,
    /// Who submitted it (any stable key, e.g. the account id).
    pub author: String,
    pub family: String,
    pub answer: Answer,
}

/// The seed of a season's draw from the pool: SHA-256 of the season's seed and the pool's
/// sorted hashes.
pub fn selection_seed(test_seed: &str, pool: &[PoolEntry]) -> [u8; 32] {
    let mut hashes: Vec<&str> = pool.iter().map(|e| e.hash.as_str()).collect();
    hashes.sort_unstable();
    let mut sha = Sha256::new();
    sha.update(test_seed.as_bytes());
    for hash in hashes {
        sha.update(b"\n");
        sha.update(hash.as_bytes());
    }
    sha.finalize().into()
}

/// Random numbers from a seed: SHA-256 in counter mode, the same everywhere.
struct Draws {
    seed: [u8; 32],
    counter: u64,
}

impl Draws {
    fn below(&mut self, n: u64) -> u64 {
        let mut sha = Sha256::new();
        sha.update(self.seed);
        sha.update(self.counter.to_le_bytes());
        self.counter += 1;
        let digest = sha.finalize();
        u64::from_le_bytes(digest[..8].try_into().unwrap_or_default()) % n.max(1)
    }
}

/// The benchmarks of the pool for the hidden set of `test_seed`: indices into `pool`, in the
/// order they were drawn, at most `count`.
///
/// SAT and UNSAT take turns (when one runs out, the other fills up). Each turn draws one of the
/// benchmarks with that answer whose author and family are still below their caps, with a
/// weight of `1 / (benchmarks of its author · benchmarks of its family)` in the whole pool, so
/// an author with one benchmark has as good a chance as one with ten together, and so has a
/// family.
pub fn select(test_seed: &str, pool: &[PoolEntry], count: usize) -> Vec<usize> {
    // A fixed order, whatever order the pool came in
    let mut order: Vec<usize> = (0..pool.len()).collect();
    order.sort_by(|&a, &b| pool[a].hash.cmp(&pool[b].hash));
    let tally = |key: &dyn Fn(&PoolEntry) -> &str| {
        let mut map = std::collections::HashMap::<&str, u64>::new();
        for entry in pool {
            *map.entry(key(entry)).or_default() += 1;
        }
        map
    };
    let by_author = tally(&|e| e.author.as_str());
    let by_family = tally(&|e| e.family.as_str());
    let weight = |e: &PoolEntry| {
        (1u64 << 32) / (by_author[e.author.as_str()] * by_family[e.family.as_str()]).max(1)
    };
    let mut draws = Draws {
        seed: selection_seed(test_seed, pool),
        counter: 0,
    };
    let mut chosen: Vec<usize> = Vec::new();
    let mut authors = std::collections::HashMap::<&str, usize>::new();
    let mut families = std::collections::HashMap::<&str, usize>::new();
    let mut exhausted = [false, false];
    let mut turn = 0;
    while chosen.len() < count && !(exhausted[0] && exhausted[1]) {
        let answer = if turn % 2 == 0 {
            Answer::Sat
        } else {
            Answer::Unsat
        };
        turn += 1;
        if exhausted[usize::from(answer == Answer::Unsat)] {
            continue;
        }
        let candidates: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&i| {
                let e = &pool[i];
                e.answer == answer
                    && !chosen.contains(&i)
                    && authors.get(e.author.as_str()).copied().unwrap_or(0) < PER_AUTHOR
                    && families.get(e.family.as_str()).copied().unwrap_or(0) < PER_FAMILY
            })
            .collect();
        let total: u64 = candidates.iter().map(|&i| weight(&pool[i])).sum();
        if candidates.is_empty() || total == 0 {
            exhausted[usize::from(answer == Answer::Unsat)] = true;
            continue;
        }
        let mut ticket = draws.below(total);
        let pick = candidates
            .iter()
            .copied()
            .find(|&i| {
                let w = weight(&pool[i]);
                if ticket < w {
                    true
                } else {
                    ticket -= w;
                    false
                }
            })
            .unwrap_or(candidates[0]);
        *authors.entry(pool[pick].author.as_str()).or_default() += 1;
        *families.entry(pool[pick].family.as_str()).or_default() += 1;
        chosen.push(pick);
    }
    chosen
}

/// Where a formula of a hidden set comes from when the community contributed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// The benchmark's number on the website.
    pub benchmark: i64,
    /// [`gbd_hash`] of the formula as submitted (before scrambling).
    pub hash: String,
    /// The family its author named.
    pub family: String,
    /// The author's name, empty if they deleted their account.
    pub author: String,
}

/// An accepted benchmark of a season's draw, as the website hands it to the worker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolInstance {
    pub origin: Origin,
    pub answer: Answer,
    /// The formula in DIMACS ([`Formula::dimacs`]).
    pub dimacs: String,
    /// A model, for satisfiable ones.
    pub model: Option<Vec<i32>>,
}

/// What names a season's draw: the SHA-256 of its benchmarks' numbers, hashes and answers, in
/// order. Part of the name of the worker's cached set, so a set never mixes up two draws.
pub fn pool_digest(pool: &[PoolInstance]) -> String {
    digest_of(
        pool.iter()
            .map(|p| (p.origin.benchmark, p.origin.hash.as_str(), p.answer)),
    )
}

/// [`pool_digest`] from the benchmarks' numbers, hashes and answers.
pub fn digest_of<'a>(draw: impl IntoIterator<Item = (i64, &'a str, Answer)>) -> String {
    let mut sha = Sha256::new();
    for (benchmark, hash, answer) in draw {
        sha.update(format!("{benchmark} {hash} {}\n", answer.id()));
    }
    hex(&sha.finalize())
}

/// Why the worker didn't take a benchmark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// Not a formula (see [`FormulaError`]).
    Invalid(String),
    TooLarge,
    /// `MiniSat` solves it in the sandbox with a tenth of an entry's fuel (the worker's
    /// `reference::EASY_FUEL`), like the SAT Competition's "`MiniSat` solves it within a minute".
    TooEasy,
    /// Neither a model nor a proof the checker accepts within the time the labelling gets.
    NoAnswer,
}

impl Refusal {
    /// A short code for the page (see `dojo.community.reason` in locales).
    pub fn id(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid",
            Self::TooLarge => "too_large",
            Self::TooEasy => "too_easy",
            Self::NoAnswer => "no_answer",
        }
    }
}

/// The worker's verdict on a benchmark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Verdict {
    /// The answer is certain (a checked model, or a proof the checker accepted) and the
    /// benchmark isn't too easy.
    Verified {
        answer: Answer,
        model: Option<Vec<i32>>,
        /// How long splr took to settle it.
        solve_ms: u64,
    },
    Refused {
        reason: Refusal,
    },
    /// Our fault: the website tries again later.
    Failed {
        message: String,
    },
}

/// The worker's report about a benchmark, posted to `/dojo/benchmark-report`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub id: i64,
    pub verdict: Verdict,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_parse_leniently() {
        let f = Formula::parse("c hi\np cnf 3 9\n1 -2 0\n2 3\n0\n%\n-1").unwrap();
        assert_eq!(f.clauses, [vec![1, -2], vec![2, 3], vec![-1]]);
        assert_eq!(f.dimacs(), "p cnf 3 3\n1 -2 0\n2 3 0\n-1 0\n");
        assert_eq!(Formula::parse(&f.dimacs()), Ok(f));
        for (text, error) in [
            (
                "1 0\n",
                FormulaError::Invalid("clauses before the p cnf line".into()),
            ),
            (
                "p cnf 1 1\n2 0\n",
                FormulaError::Invalid("literal 2 beyond 1 variables".into()),
            ),
            (
                "p cnf 1 1\nx 0\n",
                FormulaError::Invalid("not a literal: x".into()),
            ),
            (
                "p dnf 1 1\n",
                FormulaError::Invalid("bad header: p dnf 1 1".into()),
            ),
            ("c nothing\n", FormulaError::Invalid("no p cnf line".into())),
            ("p cnf 2 0\n", FormulaError::Empty),
            ("p cnf 300000 1\n1 0\n", FormulaError::TooLarge),
        ] {
            assert_eq!(Formula::parse(text), Err(error), "{text:?}");
        }
        assert_eq!(
            Formula::parse(&" ".repeat(MAX_BYTES + 1)),
            Err(FormulaError::TooLarge)
        );
    }

    #[test]
    fn hashes() {
        // md5 of "1 -2 0 2 3 0"
        let f = Formula::parse("p cnf 3 2\n1 -2 0\n2   3 0\n").unwrap();
        assert_eq!(gbd_hash(&f), "5c199826eee734c6484e691f3c129214");
        // Renamed, negated and shuffled: another GBD hash, the same shape
        let g = Formula::parse("p cnf 3 2\n-1 3 0\n2 -3 0\n").unwrap();
        assert_ne!(gbd_hash(&f), gbd_hash(&g));
        assert_eq!(shape_hash(&f), shape_hash(&g));
        let h = Formula::parse("p cnf 3 2\n1 2 0\n2 3 0\n").unwrap();
        assert_ne!(shape_hash(&f), shape_hash(&h));
    }

    fn entry(hash: &str, author: &str, family: &str, answer: Answer) -> PoolEntry {
        PoolEntry {
            hash: hash.into(),
            author: author.into(),
            family: family.into(),
            answer,
        }
    }

    #[test]
    fn selection_is_capped_balanced_and_deterministic() {
        let mut pool = Vec::new();
        for i in 0..20 {
            let answer = if i % 3 == 0 {
                Answer::Unsat
            } else {
                Answer::Sat
            };
            pool.push(entry(
                &format!("h{i:02}"),
                &format!("a{}", i % 4),
                &format!("f{}", i % 5),
                answer,
            ));
        }
        let picked = select("seed", &pool, PER_SET);
        assert_eq!(picked.len(), PER_SET);
        let count =
            |f: &dyn Fn(&PoolEntry) -> bool| picked.iter().filter(|&&i| f(&pool[i])).count();
        assert_eq!(count(&|e| e.answer == Answer::Sat), PER_SET / 2);
        for k in 0..5 {
            assert!(count(&|e| e.author == format!("a{k}")) <= PER_AUTHOR);
            assert!(count(&|e| e.family == format!("f{k}")) <= PER_FAMILY);
        }
        // The same with the pool in another order, another draw with another seed
        let mut reversed = pool.clone();
        reversed.reverse();
        let again: Vec<&str> = select("seed", &reversed, PER_SET)
            .into_iter()
            .map(|i| reversed[i].hash.as_str())
            .collect();
        let first: Vec<&str> = picked.iter().map(|&i| pool[i].hash.as_str()).collect();
        assert_eq!(first, again);
        assert_ne!(select("other seed", &pool, PER_SET), picked);
        // A new benchmark in the pool changes the draw: nobody can aim for a slot
        pool.push(entry("h99", "a9", "f9", Answer::Sat));
        assert_ne!(
            selection_seed("seed", &pool),
            selection_seed("seed", &pool[..20])
        );
    }

    #[test]
    fn small_pools_and_one_sided_ones() {
        assert_eq!(select("s", &[], PER_SET), Vec::<usize>::new());
        // Only SAT: the UNSAT turns go to SAT, the caps still hold
        let pool: Vec<PoolEntry> = (0..10)
            .map(|i| {
                entry(
                    &format!("h{i}"),
                    &format!("a{i}"),
                    &format!("f{}", i % 2),
                    Answer::Sat,
                )
            })
            .collect();
        let picked = select("s", &pool, PER_SET);
        assert_eq!(picked.len(), 2 * PER_FAMILY);
    }

    /// An author with one benchmark is drawn about as often as one with nine together.
    #[test]
    fn rare_authors_are_favoured() {
        let mut pool = vec![entry("solo", "solo", "x", Answer::Sat)];
        for i in 0..9 {
            pool.push(entry(
                &format!("many{i}"),
                "prolific",
                &format!("y{i}"),
                Answer::Sat,
            ));
        }
        let solo = (0..200)
            .filter(|i| select(&format!("seed {i}"), &pool, 1).first() == Some(&0))
            .count();
        assert!((60..=140).contains(&solo), "{solo}");
    }
}
