use backend::parse_input::parse_input; 
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[test]
fn test_game_board_creation_valid() {
    // Setup - Create a temporary file with test data
    let path = Path::new("temp_test_file1.txt");
    let mut file = File::create(&path).expect("Failed to create test file.");
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
    let mut file = File::create(&path).expect("Failed to create test file.");
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
    let mut file = File::create(&path).expect("Failed to create test file.");
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
    let mut file = File::create(&path).expect("Failed to create test file.");
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
    let mut file = File::create(&path).expect("Failed to create test file.");
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
