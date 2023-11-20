use rand::seq::SliceRandom;
use rand::Rng;
use std::fs::File;
use std::io::{Result, Write};

#[derive(Debug, Clone)]
pub enum Part {
    Corner(Corner),
    Edge(Edge),
    Normal,
}
#[derive(Debug, Clone)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

pub fn generator(rows: usize, columns: usize) -> Vec<Vec<u8>> {
    let mut grid: Vec<Vec<u8>> = vec![vec![0; columns]; rows];
    //TODO: Have a second grid where all blocked cells are marked with true (Bridges and islands)
    let mut _2grid: Vec<Vec<bool>> = vec![vec![false; columns]; rows];

    let x = rand::thread_rng().gen_range(0..=columns - 1);
    let y = rand::thread_rng().gen_range(0..=rows - 1);
    let part = get_cell_position(x, y, rows, columns);
    let degree = get_max_degree(&part, 1, 0);
    grid[y][x] = degree;

    let new_points = get_new_points(&degree, &part);
    println!("New points: {}", new_points);
    if new_points == 1 {
        connect_one(&mut grid, x, y, rows, columns, degree, part);
    } else if new_points == 2 {
        connect_two(&mut grid, x, y, rows, columns, degree, part);
    } else if new_points == 3 {
        connect_three(&mut grid, x, y, rows, columns, degree, part);
    } else if new_points == 4 {
        connect_four(&mut grid, x, y, rows, columns, degree);
    }
    grid
}

// TODO: Implement making all Satisfied.
// To get a depth first search, we need to make sure that all cells are satisfied.
// After this we can check if the grid is full and continue with the newly generated points that satsifyed the
// current configuration.
// Step 1: Generate for all new points all points needed to make a puzzle that is SAT.
// How can we achieve this? Safe all points that are from
// the second round in a new vec only for this round, because we can have circles! If a point is new and a 2 and another point
// needs the same connection just add the value that is needed to satisfy the other point.
// Step 2: Check if the grid is full and if not, continue with the newly generated points
// that satsifyed the current configuration) [random generate if we continue and what higher degree is possible]
// If this text is still here, I didn't implement it yet. For questions, ask me.

// fn second_round(
//     points: u8,
//     grid: &mut Vec<Vec<u8>>,
//     grid_size: usize,
//     x: Vec<usize>,
//     y: Vec<usize>,
//     degree: Vec<u8>,
//     part: Vec<Part>,
// ) {
//     for i in 0..points {
//         let new_points = get_new_points(&degree[i as usize], &part[i as usize]);
//         println!("New points: {}", new_points);
//         if new_points == 1 {
//             connect_one(
//                 &mut grid,
//                 x[i as usize],
//                 y[i as usize],
//                 grid_size,
//                 degree[i as usize],
//             );
//         } else if new_points == 2 {
//             connect_two(
//                 &mut grid,
//                 x[i as usize],
//                 y[i as usize],
//                 grid_size,
//                 degree[i as usize],
//             );
//         } else if new_points == 3 {
//             connect_three(
//                 &mut grid,
//                 x[i as usize],
//                 y[i as usize],
//                 grid_size,
//                 degree[i as usize],
//             );
//         } else if new_points == 4 {
//             connect_four(
//                 &mut grid,
//                 x[i as usize],
//                 y[i as usize],
//                 grid_size,
//                 degree[i as usize],
//             );
//         }
//     }
// }

fn get_cell_position(x: usize, y: usize, rows: usize, columns: usize) -> Part {
    let near_left = x == 0 || x == 1;
    let near_right = x == columns - 1 || x == columns - 2;
    let near_top = y == 0 || y == 1;
    let near_bottom = y == rows - 1 || y == rows - 2;

    match (near_left, near_right, near_top, near_bottom) {
        (true, false, true, false) => Part::Corner(Corner::TopLeft),
        (true, false, false, true) => Part::Corner(Corner::BottomLeft),
        (false, true, true, false) => Part::Corner(Corner::TopRight),
        (false, true, false, true) => Part::Corner(Corner::BottomRight),
        (true, false, _, _) => Part::Edge(Edge::Left),
        (false, true, _, _) => Part::Edge(Edge::Right),
        (_, _, true, false) => Part::Edge(Edge::Top),
        (_, _, false, true) => Part::Edge(Edge::Bottom),
        _ => Part::Normal,
    }
}

fn get_max_degree(part: &Part, min: u8, subtract: u8) -> u8 {
    match part {
        Part::Corner(_) => rand::thread_rng().gen_range(min..=4 - subtract),
        Part::Edge(_) => rand::thread_rng().gen_range(min..=6 - subtract),
        Part::Normal => rand::thread_rng().gen_range(min..=8 - subtract),
    }
}

fn get_new_points(degree: &u8, part: &Part) -> u8 {
    match part {
        Part::Corner(_) => match degree {
            1 => 1,
            2 => rand::thread_rng().gen_range(1..=2),
            3 | 4 => 2,
            _ => 0,
        },
        Part::Edge(_) => match degree {
            1 => 1,
            2 => rand::thread_rng().gen_range(1..=2),
            3 | 4 => rand::thread_rng().gen_range(2..=3),
            5 | 6 => 3,
            _ => 0,
        },
        Part::Normal => match degree {
            1 => 1,
            2 => rand::thread_rng().gen_range(1..=2),
            3 => rand::thread_rng().gen_range(2..=3),
            4 => rand::thread_rng().gen_range(2..=4),
            5 | 6 => rand::thread_rng().gen_range(3..=4),
            7 | 8 => 4,
            _ => 0,
        },
    }
}

fn get_possible_directions(part: &Part) -> &[Direction] {
    match part {
        Part::Corner(corner) => match corner {
            Corner::TopLeft => &[Direction::South, Direction::East],
            Corner::TopRight => &[Direction::South, Direction::West],
            Corner::BottomLeft => &[Direction::North, Direction::East],
            Corner::BottomRight => &[Direction::North, Direction::West],
        },
        Part::Edge(edge) => match edge {
            Edge::Top => &[Direction::South, Direction::East, Direction::West],
            Edge::Bottom => &[Direction::North, Direction::East, Direction::West],
            Edge::Left => &[Direction::North, Direction::South, Direction::West],
            Edge::Right => &[Direction::North, Direction::South, Direction::East],
        },
        Part::Normal => &[
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ],
    }
}

fn get_directions(part: &Part, num_directions: usize) -> Vec<Direction> {
    let mut rng = rand::thread_rng();
    let possible_directions = get_possible_directions(&part);
    let mut directions = Vec::new();

    for _ in 0..num_directions {
        let mut direction = possible_directions.choose(&mut rng).unwrap();
        while directions.contains(direction) {
            direction = possible_directions.choose(&mut rng).unwrap();
        }
        directions.push(*direction);
    }

    directions
}

fn get_new_coordinate(
    direction: &Direction,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
) -> (usize, usize) {
    let mut rng = rand::thread_rng();
    match direction {
        Direction::North => (x, rng.gen_range(0..y - 1)),
        Direction::East => (rng.gen_range(x + 1..columns), y),
        Direction::South => (x, rng.gen_range(y + 1..rows)),
        Direction::West => (rng.gen_range(0..x - 1), y),
    }
}

fn connect_one(
    grid: &mut Vec<Vec<u8>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
    part: Part,
) {
    println!("Connecting one with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let directions = get_directions(&part, 1);
    let direction = directions.get(0).unwrap();
    println!("Direction: {:?}", direction);
    let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
    println!("New point:");
    println!("x: {}, y: {}", new_x, new_y);
    grid[new_y][new_x] = get_max_degree(
        &part,
        if degree == 1 { 1 } else { 2 },
        if degree == 1 { 1 } else { 0 },
    );
}

fn connect_two(
    grid: &mut Vec<Vec<u8>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
    part: Part,
) {
    println!("Connecting two with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let directions = get_directions(&part, 2);
    println!("Directions: {:?}", directions);

    let mut isset = false;
    for i in 0..2 {
        let direction = directions.get(i).unwrap();
        let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
        println!("{}. new point:", i + 1);
        println!("x: {}, y: {}", new_x, new_y);

        let part = get_cell_position(new_x, new_y, rows, columns);
        match isset {
            true => grid[new_y][new_x] = get_max_degree(&part, 2, 0),
            false => match degree {
                2 => grid[new_y][new_x] = get_max_degree(&part, 1, 0),
                3 => {
                    let newdegree = get_max_degree(&part, 1, 0);
                    grid[new_y][new_x] = newdegree;
                    if newdegree == 1 {
                        isset = true
                    }
                }
                _ => grid[new_y][new_x] = get_max_degree(&part, 2, 0),
            },
        }
    }
}

fn connect_three(
    grid: &mut Vec<Vec<u8>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
    part: Part,
) {
    println!("Connecting three with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let directions = get_directions(&part, 3);
    println!("Directions: {:?}", directions);

    let mut alreadyset = false;

    for i in 0..3 {
        let direction = directions.get(i).unwrap();
        let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
        println!("{}. new point:", i + 1);
        println!("x: {}, y: {}", new_x, new_y);

        let part = get_cell_position(new_x, new_y, rows, columns);
        if degree == 5 && alreadyset == false {
            let new_degree = get_max_degree(&part, 1, 0);
            grid[new_y][new_x] = new_degree;
            if new_degree == 1 {
                alreadyset = true
            }
        } else {
            grid[new_y][new_x] = get_max_degree(&part, 2, 0)
        }
    }
}

fn connect_four(
    grid: &mut Vec<Vec<u8>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
) {
    println!("Connecting 4 with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let directions = get_directions(&Part::Normal, 3);
    println!("Directions: {:?}", directions);

    let mut alreadyset = false;

    for i in 0..4 {
        let direction = directions.get(i).unwrap();
        let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
        println!("{}. new point:", i + 1);
        println!("x: {}, y: {}", new_x, new_y);

        let part = get_cell_position(new_x, new_y, rows, columns);
        if degree == 7 && alreadyset == false {
            let new_degree = get_max_degree(&part, 1, 0);
            grid[new_y][new_x] = new_degree;
            if new_degree == 1 {
                alreadyset = true
            }
        } else {
            grid[new_y][new_x] = get_max_degree(&part, 2, 0)
        }
    }
}

pub fn output_to_file(grid: &Vec<Vec<u8>>, _filename: &str) -> Result<()> {
    let mut file = File::create("./backend/output/testpuzzle.txt")?;

    let rows = grid.len();
    let columns = grid[0].len();
    writeln!(file, "{} {}", rows, columns)?;

    for row in grid {
        for &cell in row {
            write!(
                file,
                "{}",
                if cell > 0 {
                    cell.to_string()
                } else {
                    ".".to_string()
                }
            )?;
        }
        writeln!(file)?;
    }

    Ok(())
}

