use serde::{Deserialize, Serialize};

/// A cell on the grid, `x` is the column and `y` the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Pos {
    pub x: u8,
    pub y: u8,
}

impl Pos {
    pub const fn new(x: u8, y: u8) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Island {
    pub pos: Pos,
    /// Number of bridges that have to end at this island (1..=8).
    pub required: u8,
}

/// A Hashiwokakero puzzle: a grid with numbered islands.
///
/// Islands are kept sorted in reading order (row by row), so two puzzles with the same
/// islands compare equal and island indices are stable.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Puzzle {
    pub width: u8,
    pub height: u8,
    pub islands: Vec<Island>,
}

impl Puzzle {
    pub fn new(width: u8, height: u8, mut islands: Vec<Island>) -> Self {
        islands.sort_by_key(|i| (i.pos.y, i.pos.x));
        Self {
            width,
            height,
            islands,
        }
    }

    pub fn empty(width: u8, height: u8) -> Self {
        Self::new(width, height, vec![])
    }

    /// Builds a puzzle from a row-major grid (`grid[y][x]`), 0 means water.
    ///
    /// Panics if the grid has more than 255 rows or columns.
    pub fn from_grid(grid: &[Vec<u8>]) -> Self {
        let height = u8::try_from(grid.len()).expect("a grid has at most 255 rows");
        let width =
            u8::try_from(grid.first().map_or(0, Vec::len)).expect("a grid has at most 255 columns");
        let islands = grid
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(_, n)| **n > 0)
                    .map(move |(x, &n)| Island {
                        pos: Pos::new(
                            u8::try_from(x).expect("a grid has at most 255 columns"),
                            u8::try_from(y).expect("a grid has at most 255 rows"),
                        ),
                        required: n,
                    })
            })
            .collect();
        Self::new(width, height, islands)
    }

    /// Row-major grid (`grid[y][x]`), 0 means water.
    pub fn to_grid(&self) -> Vec<Vec<u8>> {
        let mut grid = vec![vec![0; self.width as usize]; self.height as usize];
        for island in &self.islands {
            grid[island.pos.y as usize][island.pos.x as usize] = island.required;
        }
        grid
    }

    pub fn island_at(&self, pos: Pos) -> Option<usize> {
        self.islands
            .binary_search_by_key(&(pos.y, pos.x), |i| (i.pos.y, i.pos.x))
            .ok()
    }

    /// Sets the number of an island, 0 removes it. Out of range positions are ignored.
    pub fn set(&mut self, pos: Pos, required: u8) {
        if pos.x >= self.width || pos.y >= self.height {
            return;
        }
        match (self.island_at(pos), required) {
            (Some(i), 0) => {
                self.islands.remove(i);
            }
            (Some(i), n) => self.islands[i].required = n,
            (None, 0) => {}
            (None, n) => {
                self.islands.push(Island { pos, required: n });
                self.islands.sort_by_key(|i| (i.pos.y, i.pos.x));
            }
        }
    }

    /// Resizes the grid, dropping islands that no longer fit.
    #[must_use]
    pub fn resized(&self, width: u8, height: u8) -> Self {
        let islands = self
            .islands
            .iter()
            .filter(|i| i.pos.x < width && i.pos.y < height)
            .cloned()
            .collect();
        Self::new(width, height, islands)
    }

    /// Parses the ASCII format used by the test inputs:
    ///
    /// ```text
    /// 3 5        <- rows columns (a single number means a square grid)
    /// 2.3.1
    /// .....
    /// 1.4.2
    /// ```
    pub fn parse_txt(input: &str) -> Result<Self, String> {
        let mut lines = input.lines().map(str::trim).filter(|l| !l.is_empty());
        let header = lines.next().ok_or("The file is empty")?;
        let mut sizes = header.split_whitespace().map(str::parse::<u8>);
        let rows = sizes
            .next()
            .ok_or("Missing grid size in the first line")?
            .map_err(|_| format!("Invalid grid size '{header}'"))?;
        let cols = match sizes.next() {
            Some(c) => c.map_err(|_| format!("Invalid grid size '{header}'"))?,
            None => rows,
        };
        if rows < 2 || cols < 2 {
            return Err("The grid must be at least 2×2".into());
        }
        let mut grid = Vec::with_capacity(rows as usize);
        for (y, line) in lines.enumerate() {
            if y >= rows as usize {
                return Err(format!("Expected {rows} rows, found more"));
            }
            let row = line
                .chars()
                .map(|c| match c {
                    '.' | '0' => Ok(0),
                    '1'..='8' => Ok(c as u8 - b'0'),
                    _ => Err(format!("Invalid character '{c}' in row {}", y + 1)),
                })
                .collect::<Result<Vec<u8>, String>>()?;
            if row.len() != cols as usize {
                return Err(format!(
                    "Row {} has {} columns, expected {cols}",
                    y + 1,
                    row.len()
                ));
            }
            grid.push(row);
        }
        if grid.len() != rows as usize {
            return Err(format!("Expected {rows} rows, found {}", grid.len()));
        }
        Ok(Self::from_grid(&grid))
    }

    /// Inverse of [`Puzzle::parse_txt`].
    pub fn to_txt(&self) -> String {
        let mut out = format!("{} {}\n", self.height, self.width);
        for row in self.to_grid() {
            for n in row {
                out.push(if n == 0 { '.' } else { (b'0' + n) as char });
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_round_trip() {
        let txt = "3 5\n2.3.1\n.....\n1.4.2\n";
        let puzzle = Puzzle::parse_txt(txt).unwrap();
        assert_eq!(puzzle.width, 5);
        assert_eq!(puzzle.height, 3);
        assert_eq!(puzzle.islands.len(), 6);
        assert_eq!(puzzle.to_txt(), txt);
    }

    #[test]
    fn square_header() {
        let puzzle = Puzzle::parse_txt("2\n1.\n.1\n").unwrap();
        assert_eq!((puzzle.width, puzzle.height), (2, 2));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Puzzle::parse_txt("").is_err());
        assert!(Puzzle::parse_txt("2 2\n1.\n").is_err());
        assert!(Puzzle::parse_txt("2 2\n19\n..\n").is_err());
        assert!(Puzzle::parse_txt("2 2\n1..\n..\n").is_err());
    }

    #[test]
    fn set_keeps_order() {
        let mut puzzle = Puzzle::empty(4, 4);
        puzzle.set(Pos::new(3, 3), 2);
        puzzle.set(Pos::new(0, 0), 1);
        puzzle.set(Pos::new(2, 0), 3);
        assert_eq!(puzzle.island_at(Pos::new(2, 0)), Some(1));
        puzzle.set(Pos::new(2, 0), 0);
        assert_eq!(puzzle.islands.len(), 2);
        assert_eq!(puzzle.island_at(Pos::new(3, 3)), Some(1));
    }
}
