use rand::seq::SliceRandom;
use rand::Rng;
use std::cmp::{max, min};
use std::fs::File;
use std::io::{Result, Write};
use std::ops::Range;
use std::vec;

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

enum BoxedIn {
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
    let refgrid: Vec<Vec<bool>> = vec![vec![false; columns]; rows];

    let x = rand::thread_rng().gen_range(0..=columns - 1);
    let y = rand::thread_rng().gen_range(0..=rows - 1);
    let part = get_cell_position(x, y, rows, columns);
    let degree = get_max_degree(&part, 1, 0);
    grid[y][x] = degree;

    let new_points = get_new_points(&degree, &part);
    // TODO: Init must be 0 not true or false.
    generatepoints(
        new_points,
        &mut grid,
        refgrid,
        rows,
        columns,
        (x, y, false),
        degree,
        part,
    );
    grid
}

pub fn generatepoints(
    new_points: u8,
    grid: &mut Vec<Vec<u8>>,
    mut refgrid: Vec<Vec<bool>>,
    rows: usize,
    columns: usize,
    touple: (usize, usize, bool),
    degree: u8,
    part: Part,
) -> () {
    let newpoints = match new_points {
        1 => connect_one(
            grid,
            &mut refgrid,
            touple.0,
            touple.1,
            rows,
            columns,
            degree,
            part,
        ),
        2 => connect_two(
            grid,
            &mut refgrid,
            touple.0,
            touple.1,
            rows,
            columns,
            degree,
            part,
        ),
        3 => connect_three(
            grid,
            &mut refgrid,
            touple.0,
            touple.1,
            rows,
            columns,
            degree,
            part,
        ),
        4 => connect_four(
            grid,
            &mut refgrid,
            touple.0,
            touple.1,
            rows,
            columns,
            degree,
        ),
        _ => panic!("Invalid grow size"),
    };
    satsifythegrid(grid, &mut refgrid, rows, columns, newpoints);
}

// the second round in a new vec only for this round, because we can have circles! If a point is new and a 2 and another point
// needs the same connection just add the value that is needed to satisfy the other point.
// Step 2: Check if the grid is full and if not, continue with the newly generated points
// that satsifyed the current configuration) [random generate if we continue and what higher degree is possible]
// If this text is still here, I didn't implement it yet. For questions, ask me.

fn satsifythegrid(
    grid: &mut Vec<Vec<u8>>,
    refgrid: &mut Vec<Vec<bool>>,
    rows: usize,
    columns: usize,
    points: Vec<(usize, usize, bool)>,
) {
    for (i, _points) in points.iter().enumerate() {
        let x = points[i].1;
        let y = points[i].0;
        let mut degree = grid[y][x];
        let part = get_cell_position(x, y, rows, columns);
        let possible_directions = get_possible_directions(&part);
        let mut real_directions = vec![];
        for direction in possible_directions {
            match direction {
                Direction::North => {
                    if !(refgrid[y - 1][x] || refgrid[y - 2][x]) {
                        real_directions.push(direction);
                    }
                }
                Direction::East => {
                    if !(refgrid[y][x + 1] || refgrid[y][x + 2]) {
                        real_directions.push(direction);
                    }
                }
                Direction::South => {
                    if !(refgrid[y + 1][x] || refgrid[y + 2][x]) {
                        real_directions.push(direction);
                    }
                }
                Direction::West => {
                    if !(refgrid[y][x - 1] || refgrid[y][x - 2]) {
                        real_directions.push(direction);
                    }
                }
            }
        }
        if points[i].2 == true {
            degree = degree - 2;
        } else {
            degree = degree - 1;
        }
        if degree == 0 {
            continue;
        }

        let (satpoints, minuspoints) = get_new_points2(&degree, &real_directions, x, y);
        grid[y][x] = grid[y][x] - minuspoints;
        degree = degree - minuspoints;
        if satpoints == 0 {
            grid[y][x] = grid[y][x] - degree;
        }

        real_directions.shuffle(&mut rand::thread_rng());
        let mut twos = degree as usize - satpoints;
        for (iteration, &direction) in real_directions[0..satpoints].to_vec().iter().enumerate() {
            match direction {
                Direction::North => {
                    let mut min = y - 2;
                    for i in (0..y - 2).rev() {
                        if !refgrid[i][x] {
                            min = i;
                        } else {
                            break;
                        }
                    }
                    let new_y = random(min..y - 2);
                    place_bridge(refgrid, x, y, x, new_y);
                    if twos != 0 {
                        grid[new_y][x] = 2;
                        degree = degree - 2;
                        twos = twos - 1;
                    } else {
                        grid[new_y][x] = 1;
                        degree = degree - 1;
                    }
                }
                Direction::East => {
                    let mut max: usize = x + 2;
                    for i in x + 2..columns {
                        if !refgrid[y][i] {
                            max = i;
                        } else {
                            break;
                        }
                    }
                    let new_x = random(x + 2..max);
                    place_bridge(refgrid, x, y, new_x, y);
                    if twos != 0 {
                        grid[y][new_x] = 2;
                        degree = degree - 2;
                        twos = twos - 1;
                    } else {
                        grid[y][new_x] = 1;
                        degree = degree - 1;
                    }
                }
                Direction::South => {
                    let mut max = y + 2;
                    for i in y + 2..rows {
                        if !refgrid[i][x] {
                            max = i;
                        } else {
                            break;
                        }
                    }
                    let new_y = random(y + 2..max);
                    place_bridge(refgrid, x, y, x, new_y);
                    if twos != 0 {
                        grid[new_y][x] = 2;
                        degree = degree - 2;
                        twos = twos - 1;
                    } else {
                        grid[new_y][x] = 1;
                        degree = degree - 1;
                    }
                }
                Direction::West => {
                    let mut min = x - 2;
                    for i in (0..x - 2).rev() {
                        if !refgrid[y][i] {
                            min = i;
                        } else {
                            break;
                        }
                    }
                    let new_x = random(min..x - 2);
                    place_bridge(refgrid, x, y, new_x, y);
                    if twos != 0 {
                        grid[y][new_x] = 2;
                        degree = degree - 2;
                        twos = twos - 1;
                    } else {
                        grid[y][new_x] = 1;
                        degree = degree - 1;
                    }
                }
            }
        }
    }
}

fn get_new_points2(degree: &u8, dir: &Vec<&Direction>, x: usize, y: usize) -> (usize, u8) {
    match dir.len() {
        1 => match degree {
            1 | 2 => (1, 0),
            3 | 4 => (1, 2),
            _ => (1, 4),
        },
        2 => match degree {
            0 => (0, 0),
            1 => (1, 0),
            2 => (random(1..2), 0),
            3 | 4 => (2, 0),
            _ => (2, 2),
        },
        3 => match degree {
            0 => (0, 0),
            1 => (1, 0),
            2 => (random(1..2), 0),
            3 => (random(2..3), 0),
            4 => (random(2..3), 0),
            _ => (3, 0),
        },
        _ => (0, 0),
    }
}

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
            Edge::Left => &[Direction::North, Direction::South, Direction::East],
            Edge::Right => &[Direction::North, Direction::South, Direction::West],
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

fn random(range: Range<usize>) -> usize {
    if range.start == range.end {
        return range.start;
    }
    if range.is_empty() {
        println!("{:?}, {}", range.start, range.end);
        panic!("Empty Range");
    }
    let mut rng = rand::thread_rng();
    rng.gen_range(range)
}

fn get_new_coordinate(
    direction: &Direction,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
) -> (usize, usize) {
    match direction {
        Direction::North => (x, random(0..y - 2)),
        Direction::East => (random(x + 2..columns), y),
        Direction::South => (x, random(y + 2..rows)),
        Direction::West => (random(0..x - 2), y),
    }
}

fn connect_one(
    grid: &mut Vec<Vec<u8>>,
    refgrid: &mut Vec<Vec<bool>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
    part: Part,
) -> Vec<(usize, usize, bool)> {
    let directions = get_directions(&part, 1);
    let direction = directions.get(0).unwrap();
    let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
    // if bridge crosses island or other bridge
    place_bridge(refgrid, x, y, new_x, new_y);

    if degree == 1 {
        grid[new_y][new_x] = get_max_degree(&part, 1, 1);
        vec![(new_y, new_x, false)]
    } else {
        grid[new_y][new_x] = get_max_degree(&part, 2, 0);
        vec![(new_y, new_x, true)]
    }
}

fn place_bridge(
    refgrid: &mut Vec<Vec<bool>>,
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
) -> () {
    match (start_x, start_y, end_x, end_y) {
        _ if start_x == end_x => (min(start_y, end_y)..=max(start_y, end_y))
            .into_iter()
            .for_each(|y| {
                refgrid[y][start_x] = true;
            }),
        _ if start_y == end_y => (min(start_x, end_x)..=max(start_x, end_x))
            .into_iter()
            .for_each(|x| {
                refgrid[start_y][x] = true;
            }),
        _ => panic!("Bridge is not diagnal or horizontal"),
    };
}

fn connect_two(
    grid: &mut Vec<Vec<u8>>,
    refgrid: &mut Vec<Vec<bool>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
    part: Part,
) -> Vec<(usize, usize, bool)> {
    let directions = get_directions(&part, 2);

    let mut newpoints = Vec::new();
    for i in 0..2 {
        let direction = directions.get(i).unwrap();
        let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
        place_bridge(refgrid, x, y, new_x, new_y);

        let part = get_cell_position(new_x, new_y, rows, columns);
        grid[new_y][new_x] = get_max_degree(&part, 2, 0);
        newpoints.push((new_y, new_x, true));
    }
    match degree {
        2 => {
            for point in &mut newpoints {
                grid[point.0][point.1] = grid[point.0][point.1] - 1;
                *point = (point.0, point.1, false);
            }
        }
        3 => {
            let index = rand::thread_rng().gen_range(0..newpoints.len());
            let point = newpoints.get_mut(index).unwrap();
            grid[point.0][point.1] = grid[point.0][point.1] - 1;
            *point = (point.0, point.1, false);
        }
        _ => {}
    }
    newpoints
}

fn connect_three(
    grid: &mut Vec<Vec<u8>>,
    refgrid: &mut Vec<Vec<bool>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
    part: Part,
) -> Vec<(usize, usize, bool)> {
    let directions = get_directions(&part, 3);

    let mut newpoints = Vec::new();
    for i in 0..3 {
        let direction = directions.get(i).unwrap();
        let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
        place_bridge(refgrid, x, y, new_x, new_y);

        let part = get_cell_position(new_x, new_y, rows, columns);
        grid[new_y][new_x] = get_max_degree(&part, 2, 0);
        newpoints.push((new_y, new_x, true));
    }
    match degree {
        3 => {
            for point in &mut newpoints {
                grid[point.0][point.1] = grid[point.0][point.1] - 1;
                *point = (point.0, point.1, false);
            }
        }
        4 => {
            newpoints.shuffle(&mut rand::thread_rng());
            for point in &mut newpoints[0..2] {
                grid[point.0][point.1] = grid[point.0][point.1] - 1;
                *point = (point.0, point.1, false);
            }
        }
        5 => {
            let index = rand::thread_rng().gen_range(0..newpoints.len());
            let point = newpoints.get_mut(index).unwrap();
            grid[point.0][point.1] = grid[point.0][point.1] - 1;
            *point = (point.0, point.1, false);
        }
        _ => {}
    }
    newpoints
}

fn connect_four(
    grid: &mut Vec<Vec<u8>>,
    refgrid: &mut Vec<Vec<bool>>,
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
    degree: u8,
) -> Vec<(usize, usize, bool)> {
    let directions = get_directions(&Part::Normal, 4);

    let mut newpoints = Vec::new();

    for i in 0..4 {
        let direction = directions.get(i).unwrap();
        let (new_x, new_y) = get_new_coordinate(direction, x, y, rows, columns);
        place_bridge(refgrid, x, y, new_x, new_y);

        let part = get_cell_position(new_x, new_y, rows, columns);
        grid[new_y][new_x] = get_max_degree(&part, 2, 0);
        newpoints.push((new_y, new_x, true));
    }
    let mut rng = rand::thread_rng();
    match degree {
        4 => {
            for point in &mut newpoints {
                grid[point.0][point.1] = grid[point.0][point.1] - 1;
                *point = (point.0, point.1, false);
            }
        }
        5 => {
            newpoints.shuffle(&mut rng);
            for point in &mut newpoints[0..3] {
                grid[point.0][point.1] = grid[point.0][point.1] - 1;
                *point = (point.0, point.1, false);
            }
        }
        6 => {
            newpoints.shuffle(&mut rng);
            for point in &mut newpoints[0..2] {
                grid[point.0][point.1] = grid[point.0][point.1] - 1;
                *point = (point.0, point.1, false);
            }
        }
        7 => {
            let index = rng.gen_range(0..newpoints.len());
            let point = newpoints.get_mut(index).unwrap();
            grid[point.0][point.1] = grid[point.0][point.1] - 1;
            *point = (point.0, point.1, false);
        }
        _ => {}
    }
    newpoints
}

pub fn output_to_file(grid: &Vec<Vec<u8>>, filename: &str) -> Result<()> {
    let mut file = File::create(filename)?;

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

#[test]
fn should_gen() {
    for i in 0..=10000 {
        println!("Iteration: {}", i);
        let game = generator(30, 30);
        let name = "./backend/output/testpuzzle.txt";
        output_to_file(&game, name).unwrap();
        let input_file = name;
        let output_file = format!("{}.out.txt", input_file);
        match backend::parse_input::parse_input(&input_file) {
            Ok(game_board) => {
                let (clauses, var_map) = backend::generate_clauses::generate(&game_board);
                let dimacs_generated =
                    backend::writer::generate_dimacs(&clauses, var_map.keys().len(), &output_file);
                match dimacs_generated {
                    Ok(_) => match backend::solver::solve(&output_file) {
                        Ok(splr::Certificate::SAT(certificate)) => {
                            match backend::solver::write_solution(
                                splr::Certificate::SAT(certificate),
                                &output_file,
                            ) {
                                Ok(_) => {
                                    let s = backend::reconstruct::reconstruct_puzzle(
                                        &output_file.to_string(),
                                        &var_map,
                                        &game_board,
                                    );
                                    println!("{}", s);
                                }
                                Err(err) => {
                                    eprintln!("Error: {}", err);
                                }
                            }
                        }

                        Ok(splr::Certificate::UNSAT) => {
                            println!("UNSAT at iteration {}", i);
                            break;
                        }
                        Err(err) => {
                            eprintln!("Error: {}", err);
                        }
                    },
                    Err(e) => eprint!("{}", e),
                }
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }
}

