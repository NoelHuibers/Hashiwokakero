//! A benchmark set as files: what `dojo export` writes and what the website publishes for every
//! finished season (`/dojo/seasons/<n>/set.tar.gz`).
//!
//! - `README.txt`: where the set comes from (seed, its commitment, season, generator version)
//!   and how to check it, in `key: value` lines that [`read_tar_gz`] reads back
//! - `answers.txt`: `<name> SAT|UNSAT` per formula, in set order
//! - `witnesses.txt`: `<name> <literals> 0`, a model of every satisfiable formula
//! - `<name>.cnf`: the formulas in DIMACS
//!
//! Regenerating a set from its seed only gives the same formulas with the same generator
//! ([`GENERATOR_VERSION`]), the archive keeps them for good. The tarball is deterministic: the
//! same set always gives the same bytes.

use std::fmt::Write as _;
use std::io::{Read as _, Write as _};

use sha2::{Digest, Sha256};

use crate::bench::Instance;
use crate::cnf::Cnf;
use crate::{Answer, DEFAULT_BUDGET, Family, GENERATOR_VERSION};

/// Where a set comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetInfo {
    pub seed: String,
    /// The season whose hidden set it is, `None` for the dev set or any other seed.
    pub season: Option<u32>,
    pub generator_version: u32,
}

impl SetInfo {
    /// A set made by this generator.
    pub fn new(seed: &str, season: Option<u32>) -> Self {
        Self {
            seed: seed.to_string(),
            season,
            generator_version: GENERATOR_VERSION,
        }
    }

    /// The directory inside the tarball.
    fn dir(&self) -> String {
        match self.season {
            Some(season) => format!("dojo-season-{season}"),
            None => "dojo-set".into(),
        }
    }
}

/// The commitment to a seed, published before the seed itself: the SHA-256 of its text, in
/// lowercase hex (`printf %s <seed> | sha256sum`).
pub fn commitment(seed: &str) -> String {
    Sha256::digest(seed.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn readme(set: &[Instance], info: &SetInfo) -> String {
    let what = match info.season {
        Some(season) => format!("The hidden set of season {season}"),
        None => "A benchmark set".into(),
    };
    let mut text = format!("{what} of the Hashi SAT Dojo (https://hashi.rs/dojo).\n\n");
    let _ = writeln!(text, "seed: {}", info.seed);
    let _ = writeln!(text, "commitment: {}", commitment(&info.seed));
    if let Some(season) = info.season {
        let _ = writeln!(text, "season: {season}");
    }
    let _ = writeln!(text, "generator: {}", info.generator_version);
    let _ = writeln!(text, "formulas: {}", set.len());
    let _ = writeln!(text, "budget: {DEFAULT_BUDGET}");
    let seed = &info.seed;
    let _ = write!(
        text,
        "
One DIMACS CNF file per formula. answers.txt has the expected answer of each, witnesses.txt a
model of every satisfiable one. Every UNSAT label is backed by a proof or by construction.

The commitment is the SHA-256 of the seed, published before the season started. Check it:
    printf %s {seed} | sha256sum
Regenerate the formulas from the seed (with a dojo tool of the same generator version):
    dojo export regenerated/ --seed {seed}
Score a solver on exactly these formulas, the way the leaderboard did:
    dojo run my-solver.wasm --set <this .tar.gz>
"
    );
    text
}

/// The files of a set: name and content, in a fixed order.
pub fn files(set: &[Instance], info: &SetInfo) -> Vec<(String, Vec<u8>)> {
    let mut answers = String::new();
    let mut witnesses = String::new();
    for instance in set {
        let _ = writeln!(answers, "{} {}", instance.name, instance.expected.id());
        if let Some(model) = &instance.witness {
            let _ = write!(witnesses, "{}", instance.name);
            for lit in model {
                let _ = write!(witnesses, " {lit}");
            }
            witnesses.push_str(" 0\n");
        }
    }
    let mut files = vec![
        ("README.txt".to_string(), readme(set, info).into_bytes()),
        ("answers.txt".to_string(), answers.into_bytes()),
        ("witnesses.txt".to_string(), witnesses.into_bytes()),
    ];
    for instance in set {
        let comment = format!("{} from the Hashi SAT Dojo", instance.name);
        files.push((
            format!("{}.cnf", instance.name),
            instance.cnf.dimacs(&comment).into_bytes(),
        ));
    }
    files
}

/// Writes the files of the set into `dir`.
pub fn write_dir(dir: &std::path::Path, set: &[Instance], info: &SetInfo) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, content) in files(set, info) {
        let path = dir.join(name);
        std::fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// The set as a gzipped tarball, every file in one directory (`dojo-season-<n>/`).
pub fn write_tar_gz(set: &[Instance], info: &SetInfo) -> Vec<u8> {
    let gz = flate2::GzBuilder::new().write(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(gz);
    let dir = info.dir();
    for (name, content) in files(set, info) {
        let mut header = tar::Header::new_ustar();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_entry_type(tar::EntryType::Regular);
        // Into memory: nothing to fail but a name too long for a tar header, and ours are short
        tar.append_data(&mut header, format!("{dir}/{name}"), content.as_slice())
            .expect("names fit in a tar header");
    }
    tar.into_inner()
        .and_then(flate2::write::GzEncoder::finish)
        .expect("writing to memory")
}

/// The family of an instance from its name (`random-3sat-04`).
fn family_of(name: &str) -> Option<Family> {
    name.rsplit_once('-')
        .and_then(|(family, _)| Family::from_id(family))
}

/// Reads a set written by [`write_tar_gz`] (or [`write_dir`] and packed by hand).
pub fn read_tar_gz(bytes: &[u8]) -> Result<(SetInfo, Vec<Instance>), String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut files = std::collections::HashMap::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if entry.header().entry_type() != tar::EntryType::Regular {
            continue;
        }
        let path = entry.path().map_err(|e| e.to_string())?;
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|e| format!("{name}: {e}"))?;
        files.insert(name, text);
    }
    let file = |name: &str| {
        files
            .get(name)
            .ok_or_else(|| format!("no {name} in the archive"))
    };
    let readme = file("README.txt")?;
    let value = |key: &str| {
        readme
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(": "))
            .map(str::trim)
    };
    let info = SetInfo {
        seed: value("seed").ok_or("no seed in README.txt")?.to_string(),
        season: value("season").and_then(|s| s.parse().ok()),
        generator_version: value("generator")
            .and_then(|s| s.parse().ok())
            .ok_or("no generator in README.txt")?,
    };
    let mut witnesses = std::collections::HashMap::new();
    for line in file("witnesses.txt")?.lines() {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let lits: Vec<i32> = parts
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| format!("witnesses.txt: bad line for {name}"))?;
        if lits.last() != Some(&0) {
            return Err(format!(
                "witnesses.txt: the model of {name} isn't ended by 0"
            ));
        }
        witnesses.insert(name.to_string(), lits[..lits.len() - 1].to_vec());
    }
    let mut set = Vec::new();
    for line in file("answers.txt")?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let (name, answer) = line
            .split_once(' ')
            .ok_or_else(|| format!("answers.txt: bad line {line}"))?;
        let expected = Answer::from_id(answer.trim())
            .ok_or_else(|| format!("answers.txt: bad answer {answer}"))?;
        let family = family_of(name).ok_or_else(|| format!("unknown family of {name}"))?;
        let cnf = Cnf::parse(file(&format!("{name}.cnf"))?).map_err(|e| format!("{name}: {e}"))?;
        let witness = witnesses.remove(name);
        if (expected == Answer::Sat) != witness.is_some() {
            return Err(format!("{name}: a model goes with SAT, and only with SAT"));
        }
        set.push(Instance {
            name: name.to_string(),
            family,
            expected,
            cnf,
            witness,
        });
    }
    Ok((info, set))
}

/// Writes a tarball read from somewhere else, unchanged (for the CLI).
pub fn save(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::File::create(path)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> Vec<Instance> {
        vec![
            Instance {
                name: "hashi-01".into(),
                family: Family::Hashi,
                expected: Answer::Sat,
                cnf: Cnf::new(2, vec![vec![1, -2], vec![2]]),
                witness: Some(vec![1, 2]),
            },
            Instance {
                name: "random-3sat-01".into(),
                family: Family::Random3Sat,
                expected: Answer::Unsat,
                cnf: Cnf::new(1, vec![vec![1], vec![-1]]),
                witness: None,
            },
        ]
    }

    /// What a season's archive says about its seed, and that it reads back as the same set.
    #[test]
    fn round_trip() {
        let info = SetInfo::new("abc123", Some(4));
        let bytes = write_tar_gz(&set(), &info);
        // Deterministic: the same set, the same bytes
        assert_eq!(bytes, write_tar_gz(&set(), &info));
        let (read, instances) = read_tar_gz(&bytes).unwrap();
        assert_eq!(read, info);
        assert_eq!(instances.len(), 2);
        for (a, b) in instances.iter().zip(set()) {
            assert_eq!(
                (&a.name, a.family, a.expected, &a.cnf, &a.witness),
                (&b.name, b.family, b.expected, &b.cnf, &b.witness)
            );
        }

        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
        let names: Vec<String> = archive
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().display().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "dojo-season-4/README.txt",
                "dojo-season-4/answers.txt",
                "dojo-season-4/witnesses.txt",
                "dojo-season-4/hashi-01.cnf",
                "dojo-season-4/random-3sat-01.cnf",
            ]
        );
        let files = files(&set(), &info);
        let readme = String::from_utf8(files[0].1.clone()).unwrap();
        for line in [
            "The hidden set of season 4 of the Hashi SAT Dojo",
            "seed: abc123\n",
            "commitment: 6ca13d52ca70c883e0f0bb101e425a89e8624de51db2d2392593af6a84118090\n",
            "season: 4\n",
            "generator: 1\n",
            "formulas: 2\n",
            "printf %s abc123 | sha256sum",
            "dojo export regenerated/ --seed abc123",
        ] {
            assert!(readme.contains(line), "{line} missing in\n{readme}");
        }
        assert_eq!(files[1].1, b"hashi-01 SAT\nrandom-3sat-01 UNSAT\n");
        assert_eq!(files[2].1, b"hashi-01 1 2 0\n");

        // Without a season
        let (read, _) = read_tar_gz(&write_tar_gz(&set(), &SetInfo::new("dev", None))).unwrap();
        assert_eq!(read.season, None);
    }

    #[test]
    fn written_to_a_directory() {
        let dir = std::env::temp_dir().join(format!("dojo-archive-{}", std::process::id()));
        write_dir(&dir, &set(), &SetInfo::new("dev", None)).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("answers.txt")).unwrap(),
            "hashi-01 SAT\nrandom-3sat-01 UNSAT\n"
        );
        let cnf = std::fs::read_to_string(dir.join("hashi-01.cnf")).unwrap();
        assert_eq!(Cnf::parse(&cnf).unwrap(), set()[0].cnf);
        let copy = dir.join("copy.tar.gz");
        save(&copy, &write_tar_gz(&set(), &SetInfo::new("dev", None))).unwrap();
        assert!(read_tar_gz(&std::fs::read(&copy).unwrap()).is_ok());
        // Not into a file
        assert!(write_dir(&copy.join("x"), &set(), &SetInfo::new("dev", None)).is_err());
        assert!(save(&dir.join("no/such/dir.tar.gz"), b"x").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    type Files = Vec<(String, Vec<u8>)>;

    #[test]
    fn broken_archives_are_refused() {
        assert!(read_tar_gz(b"not gzip").is_err());
        let info = SetInfo::new("s", Some(1));
        let pack = |change: &dyn Fn(&mut Files)| {
            let mut files = files(&set(), &info);
            change(&mut files);
            let gz = flate2::GzBuilder::new().write(Vec::new(), flate2::Compression::fast());
            let mut tar = tar::Builder::new(gz);
            for (name, content) in files {
                let mut header = tar::Header::new_ustar();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                tar.append_data(&mut header, name, content.as_slice())
                    .unwrap();
            }
            read_tar_gz(&tar.into_inner().unwrap().finish().unwrap())
        };
        // Unchanged it reads, from the top level too
        assert!(pack(&|_| {}).is_ok());
        let replace = |name: &'static str, content: &'static str| {
            move |files: &mut Files| {
                files
                    .iter_mut()
                    .find(|f| f.0 == name)
                    .unwrap()
                    .1
                    .clone_from(&content.as_bytes().to_vec());
            }
        };
        for (label, error) in [
            (
                "no readme",
                pack(&|f| f.retain(|f| f.0 != "README.txt")).unwrap_err(),
            ),
            (
                "no seed",
                pack(&replace("README.txt", "generator: 1\n")).unwrap_err(),
            ),
            (
                "no generator",
                pack(&replace("README.txt", "seed: s\n")).unwrap_err(),
            ),
            (
                "bad answer",
                pack(&replace("answers.txt", "hashi-01 MAYBE\n")).unwrap_err(),
            ),
            (
                "bad line",
                pack(&replace("answers.txt", "hashi-01\n")).unwrap_err(),
            ),
            (
                "unknown family",
                pack(&replace("answers.txt", "sudoku-01 SAT\n")).unwrap_err(),
            ),
            (
                "missing formula",
                pack(&|f| f.retain(|f| f.0 != "hashi-01.cnf")).unwrap_err(),
            ),
            (
                "bad formula",
                pack(&replace("hashi-01.cnf", "p cnf 1 1\n5 0\n")).unwrap_err(),
            ),
            (
                "sat without model",
                pack(&replace("witnesses.txt", "")).unwrap_err(),
            ),
            (
                "unended model",
                pack(&replace("witnesses.txt", "hashi-01 1 2\n")).unwrap_err(),
            ),
            (
                "bad model",
                pack(&replace("witnesses.txt", "hashi-01 x 0\n")).unwrap_err(),
            ),
        ] {
            assert!(!error.is_empty(), "{label}");
        }
    }

    #[test]
    fn commitments() {
        // printf %s hashi-dojo-dev-1 | sha256sum
        assert_eq!(
            commitment("hashi-dojo-dev-1"),
            "30fef57f13d03a6413a17b60d6d93e03a8d09e81b1884f774b624644af5c3ba1"
        );
        assert_eq!(family_of("random-3sat-12"), Some(Family::Random3Sat));
        assert_eq!(family_of("hashi-parity-01"), Some(Family::HashiParity));
        assert_eq!(family_of("hashi"), None);
    }
}
