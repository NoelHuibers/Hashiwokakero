use crate::profiling;
use std::fmt;
use std::fs;
use std::num::ParseIntError;
use std::path::Path;

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Parse(ParseIntError),
    InvalidInput(String),
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<ParseIntError> for ParseError {
    fn from(err: ParseIntError) -> Self {
        Self::Parse(err)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse(err) => write!(f, "Invalid number: {err}"),
            Self::InvalidInput(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ParseError {}

// Due to clauses being able to span over multiple lines (see Zulip), it's not possible to parallelize the parsing.
pub fn parse(input_file: &str) -> Result<(Vec<Vec<i32>>, usize, usize), ParseError> {
    profiling::start("parse");
    if !Path::new(input_file)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cnf"))
    {
        return Err(ParseError::InvalidInput(
            "Only .cnf files are allowed.".to_string(),
        ));
    }

    let input = fs::read_to_string(input_file)?;
    // Get the lines and skip the comments
    let mut lines = input.lines().skip_while(|line| line.starts_with('c'));

    // Get the metainfo
    let p_line = lines
        .next()
        .ok_or_else(|| ParseError::InvalidInput("Missing 'p' line".to_string()))?;
    let meta_info = p_line.split_whitespace().collect::<Vec<&str>>();
    // Anything else used to panic on a short line
    let [_, _, vars, count] = meta_info[..] else {
        return Err(ParseError::InvalidInput(format!(
            "Invalid problem line '{p_line}'"
        )));
    };
    if meta_info[..2] != ["p", "cnf"] {
        return Err(ParseError::InvalidInput(format!(
            "Invalid problem line '{p_line}'"
        )));
    }
    let var_count = vars.parse()?;
    let clause_count: usize = count.parse()?;
    // The header can claim any number, the clauses that follow are what counts
    let mut clauses = Vec::with_capacity(clause_count.min(1 << 16));

    // Handle clauses
    let mut clause = Vec::new();
    for line in lines {
        for lit in line.split_ascii_whitespace() {
            let num = lit.parse()?;
            if num == 0 {
                clauses.push(clause);
                clause = Vec::new();
            } else {
                clause.push(num);
            }
        }
    }
    if !clause.is_empty() {
        clauses.push(clause);
    }

    profiling::end("parse");
    Ok((clauses, var_count, clause_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_text(name: &str, text: &str) -> Result<(Vec<Vec<i32>>, usize, usize), ParseError> {
        let path =
            std::env::temp_dir().join(format!("sat-parse-{}-{name}.cnf", std::process::id()));
        fs::write(&path, text).unwrap();
        let result = parse(path.to_str().unwrap());
        fs::remove_file(&path).unwrap();
        result
    }

    #[test]
    fn reads_clauses() {
        let (clauses, vars, count) = parse_text("ok", "c x\np cnf 3 2\n1 -2\n 0 3 0\n").unwrap();
        assert_eq!((clauses, vars, count), (vec![vec![1, -2], vec![3]], 3, 2));
    }

    /// Found by the property tests (tests/props.rs): these panicked.
    #[test]
    fn bad_headers_are_errors() {
        for (name, text) in [
            ("short", "p\n"),
            ("no-counts", "p cnf\n1 0\n"),
            ("one-count", "c\np cnf 3\n"),
            ("long", "p cnf 1 1 1\n1 0\n"),
            ("not-cnf", "p dnf 1 1\n1 0\n"),
        ] {
            assert!(
                matches!(parse_text(name, text), Err(ParseError::InvalidInput(_))),
                "{name}"
            );
        }
        // Allocated room for that many clauses up front
        let (clauses, _, count) =
            parse_text("huge", "p cnf 1 18446744073709551615\n1 0\n").unwrap();
        assert_eq!((clauses.len(), count), (1, usize::MAX));
    }
}
