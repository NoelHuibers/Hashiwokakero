use flame;
use std::fs;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Parse(ParseIntError),
    InvalidInput(String),
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> ParseError {
        ParseError::Io(err)
    }
}

impl From<ParseIntError> for ParseError {
    fn from(err: ParseIntError) -> ParseError {
        ParseError::Parse(err)
    }
}

// Due to clauses being able to span over multiple lines (see Zulip), it's not possible to parallelize the parsing.
pub fn parse(input_file: &str) -> Result<(Vec<Vec<i32>>, usize, usize), ParseError> {
    flame::start("parse");
    if !input_file.ends_with(".cnf") {
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
        .ok_or(ParseError::InvalidInput("Missing 'p' line".to_string()))?;
    let meta_info = p_line.split_whitespace().collect::<Vec<&str>>();
    let var_count = meta_info[2].parse()?;
    let clause_count = meta_info[3].parse()?;
    let mut clauses = Vec::with_capacity(clause_count);

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

    flame::end("parse");
    Ok((clauses, var_count, clause_count))
}
