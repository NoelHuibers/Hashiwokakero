use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

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

pub fn parse_input(filename: &str) -> io::Result<GameBoard> {
    if Path::new(filename)
        .extension()
        .is_none_or(|ext| ext != "txt")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only .txt files are allowed.",
        ));
    }

    let input = fs::read_to_string(filename)?;
    let lines: Vec<&str> = input.lines().collect();
    // An empty file used to panic here
    let Some((header, rows_text)) = lines.split_first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The file is empty",
        ));
    };

    let (rows, cols) = parse_rows_and_cols(header)?;

    check_game_board_format(rows_text, rows, cols)?;

    let islands = parse_islands(rows_text)?;
    let mut game_board = GameBoard {
        rows,
        cols,
        islands,
        bridges: Vec::new(),
    };

    build_bridges(&mut game_board);

    Ok(game_board)
}

fn parse_rows_and_cols(header: &str) -> io::Result<(u8, u8)> {
    let mut parts = header.split_whitespace();

    let first_part = parts.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid header: missing size information",
        )
    })?;

    if first_part.chars().all(|c| c.is_ascii_digit())
        && first_part.len() <= 2
        && let Ok(size) = parse_u8(first_part, "Invalid size")
    {
        if size >= 1 {
            if let Some(second_part) = parts.next() {
                if second_part.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(cols) = parse_u8(second_part, "Invalid number of columns")
                        && parts.next().is_none()
                        && size > 1
                        && cols > 1
                    {
                        return Ok((size, cols));
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

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid header format",
    ))
}

fn parse_u8(s: &str, error_message: &str) -> io::Result<u8> {
    s.parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{error_message}: {e}")))
}

fn parse_islands(lines: &[&str]) -> io::Result<Vec<Island>> {
    let mut islands = Vec::new();

    // Blank lines don't count as rows, as in `check_game_board_format`. Counting them put
    // islands below the grid (or panicked after 255 of them).
    for (y, line) in lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
    {
        for (x, ch) in line.chars().enumerate() {
            if ch != '.' {
                let connections = parse_u8(&ch.to_string(), "Invalid island connection count")?;
                islands.push(Island {
                    x: x.try_into().unwrap(),
                    y: y.try_into().unwrap(),
                    connections: u32::from(connections),
                });
            }
        }
    }

    Ok(islands)
}

fn build_bridges(board: &mut GameBoard) {
    let islands = &board.islands;
    let mut bridges: Vec<Bridge> = Vec::new();

    let island_map: HashMap<(u8, u8), &Island> = islands
        .iter()
        .map(|island| ((island.x, island.y), island))
        .collect();

    for (index, island) in islands.iter().enumerate() {
        let (x, y) = (island.x, island.y);

        for &(dx, dy) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut cx = i32::from(x);
            let mut cy = i32::from(y);

            while (0..i32::from(board.rows)).contains(&cy)
                && (0..i32::from(board.cols)).contains(&cx)
            {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "within 0..cols and 0..rows (u8), checked by the loop condition"
                )]
                let current_coords = (cx as u8, cy as u8);

                if let Some(current_island) = island_map.get(&current_coords)
                    && index < islands.iter().position(|i| i == *current_island).unwrap()
                {
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
                    }
                }

                cx += dx;
                cy += dy;
            }
        }
    }

    board.bridges = bridges;
}

fn check_game_board_format(lines: &[&str], rows: u8, cols: u8) -> io::Result<()> {
    // Check if the number of lines matches the specified rows
    let non_empty_lines: Vec<&str> = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .copied()
        .collect();

    if non_empty_lines.len() != <u8 as Into<usize>>::into(rows) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Invalid game board format: incorrect number of rows (expected {}, found {}). Check for empty lines!",
                rows,
                non_empty_lines.len()
            ),
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
                format!(
                    "Invalid game board format: incorrect number of columns in row {} (expected {}, found {})",
                    i + 1,
                    cols,
                    line.len()
                ),
            ));
        }

        // Check if each character is either '.' or a digit between 1 and 8
        for c in line.chars() {
            if c != '.' && !c.is_ascii_digit() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid game board format: invalid character: {c}"),
                ));
            }
            if c.is_ascii_digit() && (c.to_digit(10).unwrap() < 1 || c.to_digit(10).unwrap() > 8) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid game board format: invalid digit: {c}"),
                ));
            }
        }
    }

    Ok(())
}
