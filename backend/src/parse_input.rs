use std::fs;
use std::io;

#[derive(Debug)]
pub struct Island {
    pub x: usize,
    pub y: usize,
    pub connections: u32,
}

#[derive(Debug)]
pub struct Bridge {
    pub from: (usize, usize),
    pub to: (usize, usize),
}

#[derive(Debug)]
pub struct GameBoard {
    pub rows: usize,
    pub cols: usize,
    pub islands: Vec<Island>,
    pub bridges: Vec<Bridge>,
}

// Function to parse the input data and create the gameboard
pub(crate) fn parse_input(filename: &str) -> io::Result<GameBoard> {
    let input = fs::read_to_string(filename)?;

    let lines: Vec<&str> = input.lines().collect();

    let (rows, cols) = parse_rows_and_cols(lines[0])?;
    let (islands, bridges) = parse_islands_and_bridges(&lines[1..])?;

    let game_board = GameBoard {
        rows,
        cols,
        islands,
        bridges,
    };

    Ok(game_board)
}

fn parse_rows_and_cols(header: &str) -> io::Result<(usize, usize)> {
    let mut parts = header.split_whitespace();
    let rows_str = parts.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidData,
        "Missing number of rows",
    ))?;
    let cols_str = parts.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidData,
        "Missing number of columns",
    ))?;

    let rows = rows_str.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let cols = cols_str.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok((rows, cols))
}


fn parse_islands_and_bridges(lines: &[&str]) -> io::Result<(Vec<Island>, Vec<Bridge>)> {
    let mut islands = Vec::new();
    let mut bridges = Vec::new();

    for (x, line) in lines.iter().enumerate() {
        for (y, ch) in line.chars().enumerate() {
            if ch.is_digit(10) {
                let connections = ch.to_digit(10).ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid island connection count",
                ))? as u32;
                islands.push(Island { x, y, connections });
            }
        }
    }

    Ok((islands, bridges))
}
