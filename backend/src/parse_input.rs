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

#[derive(Debug, Clone)]
pub struct GameBoard {
    pub rows: usize,
    pub cols: usize,
    pub islands: Vec<Island>,
    pub bridges: Vec<Bridge>,
}

pub fn parse_input(filename: &str) -> io::Result<GameBoard> {
    if !filename.ends_with(".txt") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only .txt files are allowed.",
        ));
    }

    let input = fs::read_to_string(filename)?;
    let lines: Vec<&str> = input.lines().collect();

    let (rows, cols) = parse_rows_and_cols(&lines[0])?;

    check_game_board_format(&lines[1..], rows, cols)?;

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

pub fn parse_rows_and_cols(header: &str) -> io::Result<(usize, usize)> {
    let mut parts = header.split_whitespace();

    let first_part = parts.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid header: missing size information",
    ))?;

    if first_part.chars().all(|c| c.is_digit(10)) && first_part.len() <= 2 {
        if let Ok(size) = parse_usize(first_part, "Invalid size") {
            if size >= 1 {
                if let Some(second_part) = parts.next() {
                    if second_part.chars().all(|c| c.is_digit(10)) {
                        if let Ok(cols) = parse_usize(second_part, "Invalid number of columns") {
                            if parts.next().is_none() && size > 1 && cols > 1 {
                                return Ok((size, cols));
                            }
                        }
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid number of columns",
                        ));
                    }
                } else {
                    return Ok((size, size));
                }
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid size: size must be at least 1",
                ));
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid header format",
    ))
}

pub fn parse_usize(s: &str, error_message: &str) -> io::Result<usize> {
    s.parse().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", error_message, e),
        )
    })
}

fn parse_islands(lines: &[&str]) -> io::Result<Vec<Island>> {
    let mut islands = Vec::new();

    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch != '.' {
                let connections = parse_usize(&ch.to_string(), "Invalid island connection count")?;
                islands.push(Island {
                    x,
                    y,
                    connections: connections as u32,
                });
            }
        }
    }

    Ok(islands)
}

pub fn build_bridges(board: &mut GameBoard) -> io::Result<()> {
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

                if let Some(current_index) = islands
                    .iter()
                    .position(|island| (island.x, island.y) == current_coords)
                {
                    if index < current_index {
                        let valid_bridge = if x == current_coords.0 {
                            (y.min(current_coords.1) + 1..y.max(current_coords.1)).all(|i| {
                                !islands.iter().any(|island| island.x == x && island.y == i)
                            })
                        } else {
                            (x.min(current_coords.0) + 1..x.max(current_coords.0)).all(|i| {
                                !islands.iter().any(|island| island.x == i && island.y == y)
                            })
                        };

                        if valid_bridge {
                            let bridge = Bridge {
                                from: (x, y),
                                to: current_coords,
                            };
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

pub fn check_game_board_format(lines: &[&str], rows: usize, cols: usize) -> io::Result<()> {
    // Check if the number of lines matches the specified rows
    let non_empty_lines: Vec<&str> = lines.iter().filter(|line| !line.trim().is_empty()).cloned().collect();

    if non_empty_lines.len() != rows {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid game board format: incorrect number of rows (expected {}, found {}). Check for empty lines!", rows, non_empty_lines.len()),
        ));
    }

    for (i, line) in lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
    {
        // Check if the length of each line matches the specified columns
        if line.len() != cols {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid game board format: incorrect number of columns in row {} (expected {}, found {})", i+1, cols, line.len()),
            ));
        }

        // Check if each character is either '.' or a digit between 1 and 8
        for (_, c) in line.chars().enumerate() {
            if c != '.' && !c.is_digit(10) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid game board format: invalid character: {}", c),
                ));
            }
            if c.is_digit(10) && (c.to_digit(10).unwrap() < 1 || c.to_digit(10).unwrap() > 8) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid game board format: invalid digit: {}", c),
                ));
            }
        }
    }

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
