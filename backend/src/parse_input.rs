use std::collections::HashSet;
use std::fs;
use std::io;

#[derive(Debug, Eq, PartialEq, Hash, Clone, Ord, PartialOrd)]
pub struct Bridge {
    pub from: (usize, usize),
    pub to: (usize, usize),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Island {
    pub x: usize,
    pub y: usize,
    pub connections: u32,
}

#[derive(Debug)]
pub struct GameBoard {
    pub rows: usize,
    pub cols: usize,
    pub islands: Vec<Island>,
    pub bridges: Vec<Bridge>,
}

pub(crate) fn parse_input(filename: &str) -> io::Result<GameBoard> {
    if !filename.ends_with(".txt") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only .txt files are allowed.",
        ));
    }

    let input = fs::read_to_string(filename)?;

    let lines: Vec<&str> = input.lines().collect();

    let (rows, cols) = parse_rows_and_cols(&lines[0])?;
    let islands = parse_islands(&lines[1..])?;
    let mut game_board = GameBoard {
        rows,
        cols,
        islands,
        bridges: Vec::new(),
    };

    build_bridges(&mut game_board)?;

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

    let rows = parse_usize(rows_str, "Invalid number of rows")?;
    let cols = parse_usize(cols_str, "Invalid number of columns")?;

    Ok((rows, cols))
}

fn parse_usize(s: &str, error_message: &str) -> io::Result<usize> {
    s.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {}", error_message, e)))
}

fn parse_islands(lines: &[&str]) -> io::Result<Vec<Island>> {
    let mut islands = Vec::new();

    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch != '.' {
                let connections = parse_usize(&ch.to_string(), "Invalid island connection count")?;
                islands.push(Island { x, y, connections: connections as u32 });

            }
        }
    }

    Ok(islands)
}

fn build_bridges(board: &mut GameBoard) -> io::Result<()> {
    let islands = &board.islands;
    let mut bridges: Vec<Bridge> = Vec::new();
    let mut connected_islands: HashSet<(usize, usize)> = HashSet::new();

    for (index, island) in islands.iter().enumerate() {
        let (x, y) = (island.x, island.y);

        for &(dx, dy) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cx = x as i32;
            let mut cy = y as i32;

            while (0..board.rows as i32).contains(&cy) && (0..board.cols as i32).contains(&cx) {
                let current_coords = (cx as usize, cy as usize);

                if let Some(current_index) = islands.iter().position(|island| (island.x, island.y) == current_coords) {
                    if index < current_index {
                        let valid_bridge = if x == current_coords.0 {
                            (y.min(current_coords.1) + 1..y.max(current_coords.1))
                                .all(|i| !islands.iter().any(|island| island.x == x && island.y == i))
                        } else {
                            (x.min(current_coords.0) + 1..x.max(current_coords.0))
                                .all(|i| !islands.iter().any(|island| island.x == i && island.y == y))
                        };

                        if valid_bridge {
                            let bridge = Bridge { from: (x, y), to: current_coords };
                            bridges.push(bridge);
                            connected_islands.insert((x, y));
                            connected_islands.insert(current_coords);
                        }
                    }
                }

                cx += dx;
                cy += dy;
            }
        }
    }

    board.bridges = bridges;
    Ok(())
}

pub fn print_infos(game_board: &GameBoard) {
    println!("Puzzle Infos:");
    println!("Rows: {}", game_board.rows);
    println!("Cols: {}", game_board.cols);
    println!("Islands:");
    for island in &game_board.islands {
        println!(
            "Island at ({}, {}), Allowed connections: {}",
            island.x, island.y, island.connections
        );
    }
    println!("Bridges:");
    for bridge in &game_board.bridges {
        println!(
            "Bridge from ({}, {}) to ({}, {})",
            bridge.from.0, bridge.from.1, bridge.to.0, bridge.to.1
        );
    }
}
