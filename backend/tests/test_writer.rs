use backend::writer::generate_dimacs;

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Read};

    use super::*;

    #[test]
    fn test_generate_dimacs() -> std::io::Result<()> {
        let clauses = vec![vec![1, 2], vec![-1, 3], vec![-2, -3]];
        let variables = 3;
        let output_filename = "test.dimacs";

        generate_dimacs(&clauses, variables, output_filename)?;

        let expected_output = "c DIMACS file generated\np cnf 3 3\n1 2 0\n-1 3 0\n-2 -3 0\n";
        let mut file = File::open(output_filename)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        assert_eq!(contents, expected_output);

        std::fs::remove_file(output_filename)?;

        Ok(())
    }
}
