use backend::parse_input::parse_input;
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[test]
fn test_game_board_creation_valid() {
    // Setup - Create a temporary file with test data
    let path = Path::new("temp_test_file1.txt");
    let mut file = File::create(path).expect("Failed to create test file.");
    writeln!(file, "4 4").expect("Failed to write to test file.");
    writeln!(file, "1...").expect("Failed to write to test file.");
    writeln!(file, "....").expect("Failed to write to test file.");
    writeln!(file, "....").expect("Failed to write to test file.");
    writeln!(file, "...2").expect("Failed to write to test file.");

    let game_board = parse_input(path.to_str().unwrap()).expect("Failed to parse input.");

    assert_eq!(game_board.rows, 4);
    assert_eq!(game_board.cols, 4);
    assert_eq!(game_board.islands.len(), 2);

    std::fs::remove_file(path).expect("Failed to clean up test file.");
}

#[test]
fn test_game_board_creation_invalid_length() {
    let path = Path::new("temp_test_file2.txt");
    let mut file = File::create(path).expect("Failed to create test file.");
    writeln!(file, "2 2 2").expect("Failed to write to test file.");
    writeln!(file, "1.").expect("Failed to write to test file.");
    writeln!(file, ".2").expect("Failed to write to test file.");

    let result = parse_input(path.to_str().unwrap());

    assert!(result.is_err());

    std::fs::remove_file(path).expect("Failed to clean up test file.");
}

#[test]
fn test_parse_input_invalid_char() {
    let path = Path::new("temp_test_file3.txt");
    let mut file = File::create(path).expect("Failed to create test file.");
    writeln!(file, "3 3").expect("Failed to write to test file.");
    writeln!(file, "a..").expect("Failed to write to test file.");
    writeln!(file, ".22").expect("Failed to write to test file.");
    writeln!(file, "..4").expect("Failed to write to test file.");

    let result = parse_input(path.to_str().unwrap());

    assert!(result.is_err());

    std::fs::remove_file(path).expect("Failed to clean up test file.");
}

#[test]
fn test_parse_input_invalid_zero() {
    let path = Path::new("temp_test_file4.txt");
    let mut file = File::create(path).expect("Failed to create test file.");
    writeln!(file, "3 3").expect("Failed to write to test file.");
    writeln!(file, "1..").expect("Failed to write to test file.");
    writeln!(file, ".22").expect("Failed to write to test file.");
    writeln!(file, "..0").expect("Failed to write to test file.");

    let result = parse_input(path.to_str().unwrap());
    assert!(result.is_err());

    std::fs::remove_file(path).expect("Failed to clean up test file.");
}

#[test]
fn test_parse_input_invalid_gt8() {
    let path = Path::new("temp_test_file5.txt");
    let mut file = File::create(path).expect("Failed to create test file.");
    writeln!(file, "3 3").expect("Failed to write to test file.");
    writeln!(file, "1..").expect("Failed to write to test file.");
    writeln!(file, ".29").expect("Failed to write to test file.");
    writeln!(file, "..4").expect("Failed to write to test file.");

    let result = parse_input(path.to_str().unwrap());

    assert!(result.is_err());

    std::fs::remove_file(path).expect("Failed to clean up test file.");
}

#[test]
fn test_parse_input_file_not_found() {
    let result = parse_input("nonexistent_file.txt");

    assert!(result.is_err());
}

/// Found by the property tests (tests/props.rs): an empty file panicked.
#[test]
fn test_parse_input_empty_file() {
    let path = Path::new("temp_test_file6.txt");
    File::create(path).expect("Failed to create test file.");

    let result = parse_input(path.to_str().unwrap());
    std::fs::remove_file(path).expect("Failed to clean up test file.");

    assert_eq!(result.unwrap_err().to_string(), "The file is empty");
}

/// Found by the property tests: blank lines between the rows were counted as rows, so islands
/// ended up below the grid, and after 255 of them the parser panicked.
#[test]
fn test_parse_input_blank_lines() {
    for (name, blank) in [("temp_test_file7.txt", 1), ("temp_test_file8.txt", 300)] {
        let path = Path::new(name);
        let text = format!("2 2\n1.\n{}.1\n", "\n".repeat(blank));
        std::fs::write(path, text).expect("Failed to write test file.");

        let result = parse_input(path.to_str().unwrap());
        std::fs::remove_file(path).expect("Failed to clean up test file.");

        let islands: Vec<(u8, u8)> = result
            .expect("blank lines are skipped")
            .islands
            .iter()
            .map(|i| (i.x, i.y))
            .collect();
        assert_eq!(islands, [(0, 0), (1, 1)], "{blank} blank lines");
    }
}
