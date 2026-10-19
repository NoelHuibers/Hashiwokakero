//! Golden tests: exact puzzles for fixed parameters, checked in. The generator is a pure
//! function of its parameters (see `backend::generator`), and production relies on that: the
//! daily of a day is generated from a seed derived from the day, and the Dojo's hidden sets are
//! generated from a season's seed by every worker that evaluates a submission. A dependency
//! update (rand, splr) or a change of the generator that changes these puzzles has to be
//! deliberate.

use backend::api;
use common::{Difficulty, GenerateParams};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt, SeedableRng};

const PUZZLES_CHANGE: &str = "the same parameters give another puzzle now: the dailies not \
                              generated yet and the Dojo's hidden sets of new seasons change, \
                              and seeds stop reproducing puzzles. If that is intended (a better \
                              generator, a deliberate rand or splr update), update the expected \
                              puzzles; stored dailies stay as they are.";

/// A puzzle as text (see `Puzzle::to_txt`) and its solution, one digit per edge.
fn golden(params: GenerateParams) -> String {
    let g = api::generate(&params);
    assert!(g.unique, "{params:?}: not unique");
    let solution: String = g.solution.iter().map(u8::to_string).collect();
    format!("{}solution {solution}", g.puzzle.to_txt())
}

fn params(difficulty: Difficulty, size: u8, seed: u64) -> GenerateParams {
    GenerateParams {
        width: size,
        height: size,
        difficulty,
        seed,
    }
}

#[test]
fn std_rng_streams_are_unchanged() {
    // The generator draws everything from StdRng, seeded from a u64 for puzzles and from a
    // SHA-256 digest for the Dojo's instances. rand documents StdRng as not portable: a new
    // version may replace its algorithm (ChaCha12 now), and the samplers may change their values
    // too
    let mut rng = StdRng::seed_from_u64(42);
    let words: Vec<u64> = (0..3).map(|_| rng.next_u64()).collect();
    let range: Vec<i32> = (0..5).map(|_| rng.random_range(0..30)).collect();
    let float = rng.random_range(0.0..1.0f64);
    let coin: Vec<bool> = (0..8).map(|_| rng.random_bool(0.3)).collect();
    let mut cards: Vec<u8> = (0..10).collect();
    cards.shuffle(&mut rng);
    let mut from_digest = StdRng::from_seed([7; 32]);
    let digest_word = from_digest.next_u64();
    let actual = format!("{words:?} {range:?} {float} {coin:?} {cards:?} {digest_word}");
    assert_eq!(
        actual,
        "[9713269763989775522, 10011513049433592189, 11740708795755607249] [29, 12, 29, 1, 18] \
         0.3477855314069791 [true, false, false, false, false, false, false, false] \
         [9, 1, 5, 8, 6, 7, 4, 2, 0, 3] 2363194108971422454",
        "rand's StdRng (or one of the samplers the generator uses) gives other numbers: {PUZZLES_CHANGE}"
    );
}

#[test]
fn generated_puzzles_are_unchanged() {
    // One per difficulty, at the default size where that is fast in debug builds
    let cases = [
        (
            params(Difficulty::Easy, 7, 1),
            "\
7 7
.......
.1..3.3
.......
.......
.4..6..
.......
.1..2.2
solution 0112221200",
        ),
        (
            params(Difficulty::Medium, 10, 2),
            "\
10 10
..........
.2..2..2.3
..........
.6.3.....3
....6..3..
.4.1..2..5
..........
.4..4.3..3
..........
.2..1.3.2.
solution 02021122201122020202111011102",
        ),
        (
            params(Difficulty::Hard, 14, 3),
            "\
14 14
.2...3..2.1..3
.......2......
.3..1...2.1...
...3.3.4.3.4.5
1.....1.......
.2.4.3.....2..
......3..6..3.
3....5.1.....3
..3.1.........
......2..4..2.
5.6..5.1...1..
......1..3...3
............1.
3.4..5...4.5.4
solution 1111011022110011210101112201111020202111111001122001122111201101212122",
        ),
        (
            params(Difficulty::Expert, 12, 4),
            "\
12 12
3.3..3..3..1
............
..3.2.4....2
............
..3...7...2.
............
..3..1..3..4
...2..6.....
..3.1...2.1.
...1..4....3
............
3.3........2
solution 121120100111012111220110211202110111111",
        ),
        (
            params(Difficulty::Oni, 12, 5),
            "\
12 12
.1..3...1..1
............
1.4.4..2...2
............
..2...1.4.2.
4...6..2.2.4
............
.1..2...4.1.
3...........
.2..3...3..4
............
3...4..3.2.1
solution 1011001101211100012102221002020110111111112120",
        ),
        // Not square, and a seed beyond 32 bits like the ones the pages make
        (
            GenerateParams {
                width: 12,
                height: 8,
                difficulty: Difficulty::Medium,
                seed: 0xdead_beef_cafe_f00d,
            },
            "\
8 12
.1.4..5..3.4
............
1.....6.2...
.......1...3
3..3..6..2..
........2...
.........2.1
2..1..4.6.2.
solution 1211220201220101110121211022",
        ),
    ];
    // All of them at once, an intended change needs all the new puzzles
    let changed: Vec<String> = cases
        .into_iter()
        .filter_map(|(params, expected)| {
            let actual = golden(params);
            (actual != expected).then(|| format!("{params:?}:\n{actual}"))
        })
        .collect();
    assert!(
        changed.is_empty(),
        "{PUZZLES_CHANGE}\n\n{}",
        changed.join("\n\n")
    );
}
