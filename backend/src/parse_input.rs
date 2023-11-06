use std::collections::BTreeSet;
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
    let input = fs::read_to_string(filename)?;
    let lines: Vec<&str> = input.lines().collect();
    let (rows, cols) = parse_rows_and_cols(lines[0])?;
    let islands = parse_islands(&lines[1..])?;
    let mut game_board = GameBoard {
        rows,
        cols,
        islands,
        bridges: Vec::new(),
    };

    build_bridges(&mut game_board);

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

    let rows = rows_str
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let cols = cols_str
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok((rows, cols))
}

fn parse_islands(lines: &[&str]) -> io::Result<Vec<Island>> {
    let mut islands = Vec::new();

    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch != '.' {
                let connections = ch
                    .to_digit(10)
                    .ok_or(io::Error::new(io::ErrorKind::InvalidData, "Invalid island connection count"))? as u32;
                islands.push(Island { x, y, connections });
            }
        }
    }

    Ok(islands)
}

fn build_bridges(board: &mut GameBoard) {
    let rows = board.rows;
    let cols = board.cols;
    let islands = &board.islands;
    let mut bridges: Vec<Bridge> = Vec::new();

    for island in islands {
        let (x, y) = (island.x, island.y);

        for (dx, dy) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (mut cx, mut cy) = (x as i32 + dx, y as i32 + dy);

            while (0..rows as i32).contains(&cx) && (0..cols as i32).contains(&cy) {
                // If the target coordinates are within the boundaries of the game board
                let to_island = islands.iter().find(|&island| island.x == cx as usize && island.y == cy as usize);
                if let Some(to_island) = to_island {
                    // If there is an island at the target coordinates, create a bridge
                    let bridge = Bridge { from: (x, y), to: (to_island.x, to_island.y) }; // Swap the coordinates
                    if !bridges.contains(&bridge) {
                        bridges.push(bridge);
                    }
                    break; // Do not create more bridges in this direction
                }

                // Move the coordinates in the direction of motion
                cx += dx;
                cy += dy;
            }
        }
    }

    // Sort the bridges to ensure order
    bridges.sort_by(|a, b| {
        let a_sorted = sort_coordinates(a.from, a.to);
        let b_sorted = sort_coordinates(b.from, b.to);
        a_sorted.cmp(&b_sorted)
    });
    
    // Deduplicate bridges
    let mut unique_bridges = Vec::new();
    let mut seen_bridges = BTreeSet::new();
    
    for bridge in bridges {
        let sorted_bridge = sort_coordinates(bridge.from, bridge.to);
        if seen_bridges.insert(sorted_bridge) {
            unique_bridges.push(bridge);
        }
    }
    
    board.bridges = unique_bridges;
    
    // Helper function to sort coordinates
    fn sort_coordinates(coord1: (usize, usize), coord2: (usize, usize)) -> ((usize, usize), (usize, usize)) {
        if coord1 <= coord2 {
            (coord1, coord2)
        } else {
            (coord2, coord1)
        }
    }
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