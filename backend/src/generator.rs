use rand::Rng;
use std::fs::File;
use std::io::{Result, Write};

// Enum with corner, edge, normal
pub enum Part {
    Corner(Corner),
    Edge(Edge),
    Normal,
}
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

pub fn generator(grid_size: usize) -> Vec<Vec<u8>> {
    let mut grid: Vec<Vec<u8>> = vec![vec![0; grid_size]; grid_size];
    //TODO: Use Bridges or maybe a second grid with all blocked cells?
    //Undecided, because of checking if bridges are crossing each other. (Maybe able to read out of the second grid?)
    let mut _bridges: Vec<Vec<u8>> = vec![vec![0; grid_size]; grid_size];

    let x = rand::thread_rng().gen_range(0..grid_size - 1);
    let y = rand::thread_rng().gen_range(0..grid_size - 1);
    let part = get_cell_position(x, y, grid_size);
    let degree = get_max_degree(&part, 1);
    grid[x][y] = degree;

    let new_points = get_new_points(&degree, &part);
    println!("New points: {}", new_points);
    if new_points == 1 {
        connect_one(&mut grid, x, y, grid_size, degree);
    } else if new_points == 2 {
        connect_two(&mut grid, x, y, grid_size, degree, part);
    } else if new_points == 3 {
        connect_three(&mut grid, x, y, grid_size, degree, part);
    } else if new_points == 4 {
        connect_four(&mut grid, x, y, grid_size, degree);
    }
    grid
}

fn get_cell_position(x: usize, y: usize, grid_size: usize) -> Part {
    match (x, y) {
        (0, 0) => Part::Corner(Corner::TopLeft),
        (0, _) if y == grid_size - 1 => Part::Corner(Corner::TopRight),
        _ if x == grid_size - 1 && y == 0 => Part::Corner(Corner::BottomLeft),
        _ if x == grid_size - 1 && y == grid_size - 1 => Part::Corner(Corner::BottomRight),
        (0, _) => Part::Edge(Edge::Top),
        _ if x == grid_size - 1 => Part::Edge(Edge::Bottom),
        (_, 0) => Part::Edge(Edge::Left),
        _ if y == grid_size - 1 => Part::Edge(Edge::Right),
        _ => Part::Normal,
    }
}

fn get_max_degree(part: &Part, min: u8) -> u8 {
    match part {
        Part::Corner(_) => rand::thread_rng().gen_range(min..=4),
        Part::Edge(_) => rand::thread_rng().gen_range(min..=6),
        Part::Normal => rand::thread_rng().gen_range(min..=8),
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

//TODO: Handling max degree
fn connect_one(grid: &mut Vec<Vec<u8>>, x: usize, y: usize, grid_size: usize, degree: u8) {
    println!("Connecting one with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let mut rng = rand::thread_rng();

    let axis = rng.gen_bool(0.5);
    let first_axis = if axis { x } else { y };
    let other_axis = if axis { y } else { x };

    let second_axis = generate_other_axis(other_axis, grid_size);

    if axis {
        let part = get_cell_position(first_axis, second_axis, grid_size);
        if degree == 1 {
            grid[first_axis][second_axis] = get_max_degree(&part, 1);
        } else {
            grid[first_axis][second_axis] = get_max_degree(&part, 2);
        }
    } else {
        let part = get_cell_position(second_axis, first_axis, grid_size);
        if degree == 1 {
            grid[second_axis][first_axis] = get_max_degree(&part, 1);
        } else {
            grid[second_axis][first_axis] = get_max_degree(&part, 2);
        }
    };
}

//TODO: Handling max degree
//TODO: Spaghetti code should be refactored
fn connect_two(
    grid: &mut Vec<Vec<u8>>,
    x: usize,
    y: usize,
    grid_size: usize,
    degree: u8,
    part: Part,
) {
    println!("Connecting two with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let mut rng = rand::thread_rng();
    let first_random_number;
    let mut second_random_number;

    match part {
        Part::Corner(Corner::TopLeft) => {
            first_random_number = 0;
            second_random_number = 2;
        }
        Part::Corner(Corner::TopRight) => {
            first_random_number = 0;
            second_random_number = 3;
        }
        Part::Corner(Corner::BottomLeft) => {
            first_random_number = 1;
            second_random_number = 2;
        }
        Part::Corner(Corner::BottomRight) => {
            first_random_number = 1;
            second_random_number = 3;
        }
        Part::Edge(Edge::Top) => {
            let not = rng.gen_range(0..=2);
            (first_random_number, second_random_number) = match not {
                0 => (0, 3),
                1 => (0, 2),
                _ => (2, 3),
            };
        }
        Part::Edge(Edge::Bottom) => {
            let not = rng.gen_range(0..=2);
            (first_random_number, second_random_number) = match not {
                0 => (1, 3),
                1 => (1, 2),
                _ => (2, 3),
            };
        }
        Part::Edge(Edge::Left) => {
            let not = rng.gen_range(0..=2);
            (first_random_number, second_random_number) = match not {
                0 => (0, 1),
                1 => (0, 2),
                _ => (1, 2),
            };
        }
        Part::Edge(Edge::Right) => {
            let not = rng.gen_range(0..=2);
            (first_random_number, second_random_number) = match not {
                0 => (0, 1),
                1 => (0, 3),
                _ => (1, 3),
            };
        }
        Part::Normal => {
            first_random_number = rng.gen_range(1..=3);
            loop {
                second_random_number = rng.gen_range(1..=3);
                if second_random_number != first_random_number {
                    break;
                }
            }
        }
    }

    let mut isset = false;
    for i in 0..2 {
        let mut first_axis = x;
        let mut second_axis = y;

        let compared_number = if i == 0 {
            first_random_number
        } else {
            second_random_number
        };

        if compared_number == 0 {
            if x + 1 == grid_size - 1 {
                first_axis = x + 1;
            } else {
                first_axis = rng.gen_range(x + 1..grid_size);
            }
        }
        if compared_number == 1 {
            if x == 1 {
                first_axis = 0;
            } else {
                first_axis = rng.gen_range(0..x);
            }
        }
        if compared_number == 2 {
            if y + 1 == grid_size - 1 {
                second_axis = y + 1;
            } else {
                second_axis = rng.gen_range(y + 1..grid_size);
            }
        }
        if compared_number == 3 {
            if y == 1 {
                second_axis = 0;
            } else {
                second_axis = rng.gen_range(0..y);
            }
        }
        println!("New point:");
        println!("x: {}, y: {}", first_axis, second_axis);
        let part = get_cell_position(first_axis, second_axis, grid_size);
        match isset {
            true => grid[first_axis][second_axis] = get_max_degree(&part, 2),
            false => match degree {
                2 => grid[first_axis][second_axis] = get_max_degree(&part, 1),
                3 => {
                    let newdegree = get_max_degree(&part, 1);
                    grid[first_axis][second_axis] = newdegree;
                    if newdegree == 1 {
                        isset = true
                    }
                }
                _ => grid[first_axis][second_axis] = get_max_degree(&part, 2),
            },
        }
    }
}

//TODO: Handling min & max degree
//TODO: Spaghetti code should be refactored
fn connect_three(
    grid: &mut Vec<Vec<u8>>,
    x: usize,
    y: usize,
    grid_size: usize,
    degree: u8,
    part: Part,
) {
    println!("Connecting three with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let mut rng = rand::thread_rng();

    let mut first_random_number = 0;
    let mut second_random_number = 0;
    let mut third_random_number = 0;

    match part {
        Part::Edge(Edge::Top) => {
            first_random_number = 0;
            second_random_number = 2;
            third_random_number = 3;
        }
        Part::Edge(Edge::Bottom) => {
            first_random_number = 1;
            second_random_number = 2;
            third_random_number = 3;
        }
        Part::Edge(Edge::Left) => {
            first_random_number = 0;
            second_random_number = 1;
            third_random_number = 2;
        }
        Part::Edge(Edge::Right) => {
            first_random_number = 0;
            second_random_number = 1;
            third_random_number = 3;
        }
        Part::Normal => {
            let not = rng.gen_range(0..=3);
            (
                first_random_number,
                second_random_number,
                third_random_number,
            ) = match not {
                0 => (0, 2, 3),
                1 => (1, 2, 3),
                2 => (0, 1, 2),
                _ => (0, 1, 3),
            }
        }
        _ => (),
    };

    let mut alreadyset = false;

    for i in 0..3 {
        let mut first_axis = x;
        let mut second_axis = y;
        let compared_number = match i {
            0 => first_random_number,
            1 => second_random_number,
            _ => third_random_number,
        };

        if compared_number == 0 {
            if x + 1 == grid_size - 1 {
                first_axis = x + 1;
            } else {
                first_axis = rng.gen_range(x + 1..grid_size);
            }
        }
        if compared_number == 1 {
            if x == 1 {
                first_axis = 0;
            } else {
                first_axis = rng.gen_range(0..x);
            }
        }
        if compared_number == 2 {
            if y + 1 == grid_size - 1 {
                second_axis = y + 1;
            } else {
                second_axis = rng.gen_range(y + 1..grid_size);
            }
        }
        if compared_number == 3 {
            if y == 1 {
                second_axis = 0;
            } else {
                second_axis = rng.gen_range(0..y);
            }
        }
        println!(
            "i: {}, first_axis: {}, second_axis: {} ",
            i, first_axis, second_axis
        );
        let part = get_cell_position(first_axis, second_axis, grid_size);
        if degree == 5 && alreadyset == false {
            let new_degree = get_max_degree(&part, 1);
            grid[first_axis][second_axis] = new_degree;
            if new_degree == 1 {
                alreadyset = true
            }
        } else {
            grid[first_axis][second_axis] = get_max_degree(&part, 2)
        }
    }
}

//TODO: Handling min & max degree
fn connect_four(grid: &mut Vec<Vec<u8>>, x: usize, y: usize, grid_size: usize, degree: u8) {
    println!("Connecting 4 with {} degree", degree);
    println!("x: {}, y: {}", x, y);

    let mut rng = rand::thread_rng();

    let mut alreadyset = false;
    for i in 0..4 {
        let mut first_axis = x;
        let mut second_axis = y;

        if i == 0 {
            if x + 1 == grid_size - 1 {
                first_axis = x + 1;
            } else {
                first_axis = rng.gen_range(x + 1..grid_size);
            }
        } else if i == 1 {
            if x == 1 {
                first_axis = 0;
            } else {
                first_axis = rng.gen_range(0..x);
            }
        } else if i == 2 {
            if y + 1 == grid_size - 1 {
                second_axis = y + 1;
            } else {
                second_axis = rng.gen_range(y + 1..grid_size);
            }
        } else if i == 3 {
            if y == 1 {
                second_axis = 0;
            } else {
                second_axis = rng.gen_range(0..y);
            }
        }

        println!(
            "i: {}, first_axis: {}, second_axis: {} ",
            i, first_axis, second_axis
        );

        let part = get_cell_position(first_axis, second_axis, grid_size);
        if degree == 7 && alreadyset == false {
            let new_degree = get_max_degree(&part, 1);
            grid[first_axis][second_axis] = new_degree;
            if new_degree == 1 {
                alreadyset = true
            }
        } else {
            grid[first_axis][second_axis] = get_max_degree(&part, 2)
        }
    }
}

fn generate_other_axis(other_axis: usize, grid_size: usize) -> usize {
    let mut second_axis = rand::thread_rng().gen_range(0..grid_size);
    while other_axis == second_axis {
        second_axis = rand::thread_rng().gen_range(0..grid_size);
    }
    second_axis
}

pub fn output_to_file(grid: &Vec<Vec<u8>>, _filename: &str) -> Result<()> {
    let mut file = File::create("./backend/output/testpuzzle.txt")?;

    let size = grid.len();
    writeln!(file, "{} {}", size, size)?;

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
