use std::{io, fs};
use flame;

pub fn parse(input_file: &str) -> io::Result<(Vec<Vec<i32>>, usize, usize)> {
    flame::start("parse");
    if !input_file.ends_with(".cnf") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only .cnf files are allowed.",
        ));
    }

    let input = fs::read_to_string(input_file)?;
    let lines: Vec<&str> = input.lines().collect();
    let mut clauses = Vec::new();
    let mut var_count = 0;
    let mut clause_count = 0;

    for line in lines {
        {
            if line.starts_with('c') {
                continue;
            }

            if line.starts_with('p') {
                let meta_info = line.split_whitespace().collect::<Vec<&str>>();
                var_count = meta_info[2].parse().unwrap();
                clause_count = meta_info[3].parse().unwrap();
                continue;
            }

            let clause = line
                .split_whitespace()
                .filter_map(|lit| {
                    let num = lit.parse::<i32>().ok()?;
                    if num == 0 {
                        None
                    } else {
                        Some(num)
                    }
                })
                .collect::<Vec<i32>>();
            clauses.push(clause);
        }
    }
    flame::end("parse");
    Ok((clauses, var_count, clause_count))
}
