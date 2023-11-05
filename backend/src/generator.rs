use rand::Rng;
use std::fs::File;
use std::io::{Result, Write};

pub fn generator(grid_size: usize) -> Vec<Vec<u8>> {
    let mut grid: Vec<Vec<u8>> = vec![vec![0; grid_size]; grid_size];
    //TODO: Use Bridges or maybe a second grid with all blocked cells?
    //Undecided, because of checking if bridges are crossing each other. (Maybe able to read out of the second grid?)
    let mut _bridges: Vec<Vec<u8>> = vec![vec![0; grid_size]; grid_size];

    let x = rand::thread_rng().gen_range(0..grid_size - 1);
    let y = rand::thread_rng().gen_range(0..grid_size - 1);
    let degree = get_max_degree(grid.len(), x, y, 1);
    grid[x][y] = degree;

    let new_points = get_new_points(degree);
    println!("New points: {}", new_points);
    if new_points == 1 {
        connect_one(&mut grid, x, y, grid_size, degree);
    } else if new_points == 2 {
        connect_two(&mut grid, x, y, grid_size, degree);
    } else if new_points == 3 {
        connect_three(&mut grid, x, y, grid_size, degree);
    } else if new_points == 4 {
        connect_four(&mut grid, x, y, grid_size, degree);
    }
    grid
}

fn get_max_degree(grid_size: usize, x: usize, y: usize, min: u8) -> u8 {
    if (x == 0 && y == 0)
        || (x == 0 && y == grid_size - 1)
        || (x == grid_size - 1 && y == 0)
        || (x == grid_size - 1 && y == grid_size - 1)
    {
        return rand::thread_rng().gen_range(min..=4);
    } else if x == 0 || x == grid_size - 1 || y == 0 || y == grid_size - 1 {
        return rand::thread_rng().gen_range(min..=6);
    } else {
        return rand::thread_rng().gen_range(min..=8);
    }
}

//FIXME: Overflow error, if degree is 4-6 and its an edge point we cant have 4 connections
fn get_new_points(degree: u8) -> u8 {
    match degree {
        1 => 1,
        2 => rand::thread_rng().gen_range(1..=2),
        3 => rand::thread_rng().gen_range(2..=3),
        4 => rand::thread_rng().gen_range(2..=4),
        5 | 6 => rand::thread_rng().gen_range(3..=4),
        7 | 8 => 4,
        _ => 0,
    }
}

fn connect_one(grid: &mut Vec<Vec<u8>>, x: usize, y: usize, grid_size: usize, degree: u8) {
    println!("Connecting one with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let mut rng = rand::thread_rng();

    let axis = rng.gen_bool(0.5);
    let first_axis = if axis { x } else { y };
    let other_axis = if axis { y } else { x };

    let second_axis = generate_other_axis(other_axis, grid_size);

    if axis {
        if degree == 1 {
            grid[first_axis][second_axis] = get_max_degree(grid_size, first_axis, second_axis, 1);
        } else {
            grid[first_axis][second_axis] = get_max_degree(grid_size, first_axis, second_axis, 2);
        }
    } else {
        if degree == 1 {
            grid[second_axis][first_axis] = get_max_degree(grid_size, first_axis, second_axis, 1);
        } else {
            grid[second_axis][first_axis] = get_max_degree(grid_size, first_axis, second_axis, 2);
        }
    };
}

//FIXME: Make this right if its an edge point.
fn connect_two(grid: &mut Vec<Vec<u8>>, x: usize, y: usize, grid_size: usize, degree: u8) {
    println!("Connecting two with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    let mut rng = rand::thread_rng();

    let first_random_number = rng.gen_range(0..4);

    let mut second_random_number;
    loop {
        second_random_number = rng.gen_range(0..4);
        if second_random_number != first_random_number {
            break;
        }
    }

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
        grid[first_axis][second_axis] = get_max_degree(grid_size, first_axis, second_axis, 1)
    }
}

//TODO: Make this right and not just a copy of connect_one
fn connect_three(grid: &mut Vec<Vec<u8>>, x: usize, y: usize, grid_size: usize, degree: u8) {
    println!("Connecting three with {} degree", degree);
    println!("x: {}, y: {}", x, y);
    println!("NOT IMPLEMENTED");
    let mut rng = rand::thread_rng();

    let axis = rng.gen_bool(0.5);
    let first_axis = if axis { x } else { y };
    let other_axis = if axis { y } else { x };

    let second_axis = generate_other_axis(other_axis, grid_size);

    if axis {
        grid[first_axis][second_axis] = get_max_degree(grid_size, first_axis, second_axis, 3);
    } else {
        grid[second_axis][first_axis] = get_max_degree(grid_size, first_axis, second_axis, 3);
    };
}

//TODO: What if 4-6 degrees have four connections?
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

        if degree == 7 && alreadyset == false {
            let new_degree = get_max_degree(grid_size, first_axis, second_axis, 1);
            grid[first_axis][second_axis] = new_degree;
            if new_degree == 1 {
                alreadyset = true
            }
        } else {
            grid[first_axis][second_axis] = get_max_degree(grid_size, first_axis, second_axis, 2)
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
