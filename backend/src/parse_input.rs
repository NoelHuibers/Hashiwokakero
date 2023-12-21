use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::io;

#[derive(Debug, Eq, PartialEq, Hash, Clone, Ord, PartialOrd)]
pub struct Bridge {
    pub from: (u8, u8),
    pub to: (u8, u8),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Island {
    pub x: u8,
    pub y: u8,
    pub connections: u32,
}

#[derive(Debug, Clone)]
pub struct GameBoard {
    pub rows: u8,
    pub cols: u8,
    pub islands: Vec<Island>,
    pub bridges: Vec<Bridge>,
}

pub fn parse_vec_input(input: Vec<Vec<u8>>) -> io::Result<GameBoard> {
    // Get the row and column count from the Vec input length
    let rows = input.len() as u8;
    let cols = input[0].len() as u8;
    //Get the islands from the Vec input and generate the bridges
    let mut islands = Vec::new();
    for (y, row) in input.iter().enumerate() {
        for (x, &connections) in row.iter().enumerate() {
            if connections != 0 {
                let x = x as u8;
                let y = y as u8;
                islands.push(Island {
                    x,
                    y,
                    connections: connections as u32,
                });
            }
        }
    }
    let mut game_board = GameBoard {
        rows,
        cols,
        islands,
        bridges: Vec::new(),
    };
    build_bridges(&mut game_board)?;
    Ok(game_board)
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

fn parse_rows_and_cols(header: &str) -> io::Result<(u8, u8)> {
    let mut parts = header.split_whitespace();

    let first_part = parts.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid header: missing size information",
    ))?;

    if first_part.chars().all(|c| c.is_digit(10)) && first_part.len() <= 2 {
        if let Ok(size) = parse_u8(first_part, "Invalid size") {
            if size >= 1 {
                if let Some(second_part) = parts.next() {
                    if second_part.chars().all(|c| c.is_digit(10)) {
                        if let Ok(cols) = parse_u8(second_part, "Invalid number of columns") {
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

fn parse_u8(s: &str, error_message: &str) -> io::Result<u8> {
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
                let connections = parse_u8(&ch.to_string(), "Invalid island connection count")?;
                islands.push(Island {
                    x: x.try_into().unwrap(),
                    y: y.try_into().unwrap(),
                    connections: connections as u32,
                });
            }
        }
    }

    Ok(islands)
}

fn build_bridges(board: &mut GameBoard) -> io::Result<()> {
    let islands = &board.islands;
    let mut bridges: Vec<Bridge> = Vec::new();
    let mut connected_islands: HashSet<(u8, u8)> = HashSet::new();

    let island_map: HashMap<(u8, u8), &Island> = islands
        .iter()
        .map(|island| ((island.x, island.y), island))
        .collect();

    for (index, island) in islands.iter().enumerate() {
        let (x, y) = (island.x, island.y);

        for &(dx, dy) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cx = x as i32;
            let mut cy = y as i32;

            while (0..board.rows as i32).contains(&cy) && (0..board.cols as i32).contains(&cx) {
                let current_coords = (cx as u8, cy as u8);

                if let Some(current_island) = island_map.get(&current_coords) {
                    if index < islands.iter().position(|i| i == *current_island).unwrap() {
                        let valid_bridge = if x == current_coords.0 {
                            (y.min(current_coords.1) + 1..y.max(current_coords.1))
                                .all(|i| !island_map.contains_key(&(x, i)))
                        } else {
                            (x.min(current_coords.0) + 1..x.max(current_coords.0))
                                .all(|i| !island_map.contains_key(&(i, y)))
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

fn check_game_board_format(lines: &[&str], rows: u8, cols: u8) -> io::Result<()> {
    // Check if the number of lines matches the specified rows
    let non_empty_lines: Vec<&str> = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .cloned()
        .collect();

    if non_empty_lines.len() != <u8 as Into<usize>>::into(rows) {
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
        if line.len() != <u8 as Into<usize>>::into(cols) {
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

