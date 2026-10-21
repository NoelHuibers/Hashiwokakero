//! Tests of the metrics: hand-computed examples and the Shapley value's closed form against its
//! definition over every coalition.

use proptest::prelude::*;
use proptest::test_runner::{RngSeed, contextualize_config};

use super::*;
use crate::Outcome;

const B: u64 = 1_000_000_000;

fn result(name: &str, family: Family, expected: Answer, fuel: Option<u64>) -> InstanceResult {
    InstanceResult {
        name: name.into(),
        family,
        expected,
        outcome: match fuel {
            Some(_) => Outcome::Solved(expected),
            None => Outcome::Timeout,
        },
        fuel: fuel.unwrap_or(B),
    }
}

/// Results of one entry on formulas `f0`, `f1`, ... (fuel when solved).
fn entry(fuel: &[Option<u64>]) -> Vec<InstanceResult> {
    fuel.iter()
        .enumerate()
        .map(|(i, &f)| result(&format!("f{i}"), Family::Hashi, Answer::Sat, f))
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// Three entries on three formulas, in billions:
///
/// | | f0 | f1 | f2 |
/// |---|---|---|---|
/// | A | 0.2 | 0.5 | – |
/// | B | 0.4 | – | – |
/// | C | – | 0.5 | 1.0 |
fn example() -> Portfolio {
    let a = entry(&[Some(B / 5), Some(B / 2), None]);
    let b = entry(&[Some(2 * B / 5), None, None]);
    let c = entry(&[None, Some(B / 2), Some(B)]);
    let (portfolio, member) = Portfolio::new(&[&a, &b, &c], B);
    assert_eq!(member, [true, true, true]);
    portfolio
}

#[test]
fn the_virtual_best_solver() {
    let p = example();
    assert_eq!((p.len(), p.formulas()), (3, 3));
    let vbs = p.vbs();
    assert_eq!((vbs.solved, vbs.total), (3, 3));
    assert!(close(vbs.par2, 1.7 / 3.0));
    assert_eq!(p.vbs_curve(), [B / 5, B / 2, B]);
}

#[test]
fn marginal_contributions() {
    let p = example();
    let marginal = p.marginal();
    // Without A, B's 0.4 takes f0; without B nothing changes; without C nobody solves f2
    let expected = [
        (0.2 / 3.0, (1.9f64 / 1.7).log10()),
        (0.0, 0.0),
        (1.0 / 3.0, (2.7f64 / 1.7).log10()),
    ];
    for (got, want) in marginal.iter().zip(expected) {
        assert!(
            close(got.0, want.0) && close(got.1, want.1),
            "{got:?} vs {want:?}"
        );
    }
}

#[test]
fn state_of_the_art_contributions() {
    // f0 shared by A and B, f1 by A and C, f2 C's alone
    assert_eq!(example().sota(), [1.0, 0.5, 1.5]);
}

#[test]
fn shapley_values() {
    let p = example();
    let phi = p.shapley();
    // f0: C 0, B 1.6/2, A 0.8 + 0.2; f1: A and C 1.5/2 each; f2: C 1.0
    for (got, want) in phi.iter().zip([1.75 / 3.0, 0.8 / 3.0, 1.75 / 3.0]) {
        assert!(close(*got, want), "{phi:?}");
    }
    let contributions = p.contributions();
    assert!(close(contributions[1].shapley, phi[1]));
    assert_eq!(contributions[2].sota, 1.5);
    // Sparkle's relative contribution: shares of the summed log ratios
    let (a, c) = ((1.9f64 / 1.7).log10(), (2.7f64 / 1.7).log10());
    assert!(close(contributions[0].relative, a / (a + c)));
    assert_eq!(contributions[1].relative, 0.0);
    assert!(close(contributions[2].relative, c / (a + c)));
}

#[test]
fn degenerate_portfolios() {
    // Nobody: nothing solved
    let (empty, member) = Portfolio::new(&[], B);
    assert!(empty.is_empty() && member.is_empty());
    assert_eq!(empty.vbs(), Split::default());
    // One entry is its own VBS and gets everything it saves
    let a = entry(&[Some(B / 2), None]);
    let (one, _) = Portfolio::new(&[&a], B);
    let c = one.contributions()[0];
    assert!(close(c.marginal, (2.0 - 0.5) / 2.0));
    assert!(close(c.log_ratio, (2.0f64 / 1.25).log10()));
    assert_eq!(c.relative, 1.0);
    assert!(close(c.shapley, c.marginal));
    assert_eq!(c.sota, 1.0);
    // Solving nothing with nothing: no ratio to take
    let none = entry(&[None]);
    let (nothing, _) = Portfolio::new(&[&none], B);
    assert_eq!(nothing.marginal(), [(0.0, 0.0)]);
    assert_eq!(nothing.contributions()[0].relative, 0.0);
}

#[test]
fn entries_on_other_formulas_stay_out() {
    let a = entry(&[Some(B / 5), None]);
    let shorter = entry(&[Some(B / 5)]);
    let mut renamed = entry(&[Some(B / 5), None]);
    renamed[1].name = "other".into();
    let (p, member) = Portfolio::new(&[&[], &a, &shorter, &renamed, &a], B);
    assert_eq!(member, [false, true, false, false, true]);
    assert_eq!(p.len(), 2);
    // Two copies share what one of them would get alone, and add nothing to each other
    let c = p.contributions();
    assert_eq!((c[0].marginal, c[0].sota), (0.0, 0.5));
    assert!(close(c[0].shapley, c[1].shapley));
    assert!(close(c[0].shapley * 2.0, 2.0 - p.vbs().par2));
}

#[test]
fn cactus_curves_are_sorted_and_rounded() {
    let mut rs = entry(&[Some(5_432_109), None, Some(1_000), Some(123_456_789)]);
    rs[2].outcome = Outcome::Unverified;
    assert_eq!(curve(&rs), [5_430_000, 123_000_000]);
    assert_eq!(round(0), 0);
    assert_eq!(round(999), 999);
    assert_eq!(round(1_995), 2_000);
    assert_eq!(round(6_999_600_000), 7_000_000_000);
    assert_eq!(round(1_234_567_890), 1_230_000_000);
}

#[test]
fn sat_unsat_and_all() {
    let mut rs = vec![
        result("hashi-01", Family::Hashi, Answer::Sat, Some(B / 2)),
        result("hashi-02", Family::Hashi, Answer::Sat, None),
        result("parity-01", Family::HashiParity, Answer::Unsat, Some(B)),
        result("parity-02", Family::HashiParity, Answer::Unsat, None),
        result("tseitin-01", Family::Tseitin, Answer::Unsat, Some(B / 4)),
        result("tseitin-02", Family::Tseitin, Answer::Sat, None),
    ];
    rs[3].outcome = Outcome::Unverified;
    let running = breakdown(&rs, B, false);
    assert_eq!(running.held_out, 2);
    assert_eq!(running.total.all.solved, 3);
    assert_eq!(running.total.all.total, 6);
    assert!(close(
        running.total.all.par2,
        (0.5 + 2.0 + 1.0 + 2.0 + 0.25 + 2.0) / 6.0
    ));
    // The held-out family stays out of SAT and UNSAT
    let sat = running.total.sat.unwrap();
    let unsat = running.total.unsat.unwrap();
    assert_eq!(
        (sat.solved, sat.total, unsat.solved, unsat.total),
        (1, 2, 1, 2)
    );
    assert!(close(sat.par2, 1.25) && close(unsat.par2, 1.5));
    assert_eq!(running.total.unverified, 1);
    let families: Vec<Family> = running.families.iter().map(|f| f.family).collect();
    assert_eq!(
        families,
        [Family::Hashi, Family::HashiParity, Family::Mystery]
    );
    let mystery = &running.families[2].breakdown;
    assert_eq!(
        (mystery.sat, mystery.unsat, mystery.all.total),
        (None, None, 2)
    );
    assert_eq!(running.families[1].breakdown.unverified, 1);
    assert_eq!(running.families[1].breakdown.sat.unwrap().total, 0);

    // Once the season is over, under its own name
    let archived = breakdown(&rs, B, true);
    assert_eq!(archived.held_out, 0);
    let families: Vec<Family> = archived.families.iter().map(|f| f.family).collect();
    assert_eq!(
        families,
        [Family::Hashi, Family::HashiParity, Family::Tseitin]
    );
    let tseitin = &archived.families[2].breakdown;
    assert_eq!(tseitin.sat.unwrap().total, 1);
    assert_eq!(archived.total.unsat.unwrap().total, 3);
    assert_eq!(breakdown(&[], B, true).total.all, Split::default());
}

/// Results of rules version 5 go on past the ranking budget: the official metrics count what
/// happened within it, the cactus curve shows everything.
#[test]
fn the_ranking_counts_only_its_budget() {
    let mut late = entry(&[Some(B / 2), Some(3 * B), Some(9 * B)]);
    late[2].outcome = Outcome::Wrong;
    let early = entry(&[Some(B), None, None]);
    let split = breakdown(&late, B, true).total.all;
    assert_eq!((split.solved, split.total), (1, 3));
    assert!(close(split.par2, (0.5 + 2.0 + 2.0) / 3.0));
    let (p, _) = Portfolio::new(&[&late, &early], B);
    assert_eq!(p.vbs().solved, 1);
    assert!(close(p.vbs().par2, (0.5 + 2.0 + 2.0) / 3.0));
    assert_eq!(p.vbs_curve(), [B / 2]);
    // The same portfolio with ten times the budget
    let (wide, _) = Portfolio::new(&[&late, &early], 10 * B);
    assert_eq!(wide.vbs_curve(), [B / 2, 3 * B]);
    assert_eq!(curve(&late), [B / 2, 3 * B]);
}

#[test]
fn scores_at_any_budget() {
    let mut rs = vec![
        result("hashi-01", Family::Hashi, Answer::Sat, Some(B / 10)),
        result("hashi-02", Family::Hashi, Answer::Sat, Some(4 * B)),
        result("parity-01", Family::HashiParity, Answer::Unsat, Some(B)),
        result("parity-02", Family::HashiParity, Answer::Unsat, Some(2 * B)),
        result("parity-03", Family::HashiParity, Answer::Unsat, Some(5 * B)),
        result("tseitin-01", Family::Tseitin, Answer::Unsat, Some(B / 2)),
        result("tseitin-02", Family::Tseitin, Answer::Sat, None),
    ];
    // Unverified at 2B, wrong at 5B, the timeout at the run budget
    rs[3].outcome = Outcome::Unverified;
    rs[4].outcome = Outcome::Wrong;
    rs[6].fuel = 10 * B;
    let at = |budget, reveal| score_at(&rs, budget, reveal);

    let small = at(B / 10, false);
    assert_eq!((small.budget, small.solved, small.total), (B / 10, 1, 7));
    assert!(close(small.par2, (0.1 + 6.0 * 0.2) / 7.0));
    assert_eq!(
        (small.sat, small.unsat, small.wrong, small.unverified),
        (1, 0, 0, 0)
    );

    // The held-out family counts in solved, not in SAT and UNSAT while the season runs
    let one = at(B, false);
    assert_eq!((one.solved, one.sat, one.unsat), (3, 1, 1));
    assert!(close(
        one.par2,
        (0.1 + 2.0 + 1.0 + 2.0 + 2.0 + 0.5 + 2.0) / 7.0
    ));
    let revealed = at(B, true);
    assert_eq!((revealed.sat, revealed.unsat), (1, 2));
    assert_eq!(one.par2, revealed.par2);

    // An unverified answer counts from its fuel on, and stays unsolved
    let two = at(2 * B, false);
    assert_eq!((two.solved, two.unverified, two.wrong), (3, 1, 0));
    // A wrong answer only from its fuel on
    let four = at(4 * B, false);
    assert_eq!((four.solved, four.sat, four.wrong), (4, 2, 0));
    let five = at(5 * B, true);
    assert_eq!((five.solved, five.wrong, five.unverified), (4, 1, 1));
    assert!(close(
        five.par2,
        (0.1 + 4.0 + 1.0 + 10.0 + 10.0 + 0.5 + 10.0) / 7.0
    ));
    // The run budget: the timeout is still one
    let all = at(10 * B, true);
    assert_eq!((all.solved, all.total), (4, 7));
    // The same as the official breakdown at every budget
    for budget in [B / 10, B, 2 * B, 5 * B] {
        let official = breakdown(&rs, budget, true).total.all;
        let score = at(budget, true);
        assert_eq!(
            (score.solved, score.total),
            (official.solved, official.total)
        );
        assert!(close(score.par2, official.par2));
    }
    assert_eq!(
        score_at(&[], B, true),
        BudgetScore {
            budget: B,
            ..BudgetScore::default()
        }
    );
}

#[test]
fn budget_grids() {
    let grid = budget_grid(7_000_000_000, 70_000_000_000);
    // A hundredth of the budget to ten times it, four per power of ten
    assert_eq!(grid.len(), 13);
    assert_eq!(grid.first(), Some(&70_000_000));
    for exact in [700_000_000, 7_000_000_000, 70_000_000_000] {
        assert!(grid.contains(&exact), "{exact} in {grid:?}");
    }
    assert_eq!(grid[1], 124_479_559);
    assert!(grid.windows(2).all(|w| w[0] < w[1]));
    // Older rules went no further than the budget
    let old = budget_grid(7_000_000_000, 7_000_000_000);
    assert_eq!((old.len(), old.last()), (9, Some(&7_000_000_000)));
    // A run budget off the grid ends it
    let odd = budget_grid(1_000, 5_000);
    assert_eq!(
        odd,
        [
            10, 18, 32, 56, 100, 178, 316, 562, 1_000, 1_778, 3_162, 5_000
        ]
    );
    assert_eq!(budget_grid(0, 0), [0]);

    let rs = entry(&[Some(B / 2), Some(3 * B), None]);
    let points = budget_curve(&rs, &[B, 3 * B], true);
    assert_eq!(points, [score_at(&rs, B, true), score_at(&rs, 3 * B, true)]);
    assert_eq!((points[0].solved, points[1].solved), (1, 2));
}

/// The example of the docs: the first entry solves two formulas quickly, the second three
/// slowly. The first leads from a hundredth of B, the second's lower penalty catches up within
/// the last segment, at 1.34 B.
fn racers() -> (Vec<InstanceResult>, Vec<InstanceResult>) {
    let quick = entry(&[Some(B / 100), Some(B / 100), None, None]);
    let slow = entry(&[Some(9 * B / 10), Some(9 * B / 10), Some(9 * B / 10), None]);
    (quick, slow)
}

#[test]
fn crossovers_are_exact() {
    let (quick, slow) = racers();
    let found = crossovers(&quick, &slow, B / 100, 10 * B);
    assert_eq!(
        found,
        [Crossover {
            budget: 1_340_000_001,
            first: false
        }]
    );
    // Just below the crossover the first entry is ahead, at it the second
    let par2 = |rs: &[InstanceResult], b| score_at(rs, b, true).par2;
    assert!(par2(&quick, 1_340_000_000) <= par2(&slow, 1_340_000_000));
    assert!(par2(&slow, 1_340_000_001) < par2(&quick, 1_340_000_001));
    // The other way round, the other one takes the lead
    assert_eq!(
        crossovers(&slow, &quick, B / 100, 10 * B),
        [Crossover {
            budget: 1_340_000_001,
            first: true
        }]
    );
    // Outside the range, nothing
    assert_eq!(crossovers(&quick, &slow, B / 100, B), []);
    assert_eq!(crossovers(&quick, &slow, 2 * B, 10 * B), []);
    assert_eq!(crossovers(&quick, &slow, 2 * B, B), []);

    // At a jump: the second one solves the last formula at 3 B
    let a = entry(&[Some(B / 10), None]);
    let b = entry(&[Some(B / 2), Some(3 * B)]);
    assert_eq!(
        crossovers(&a, &b, B / 100, 10 * B),
        [Crossover {
            budget: 3 * B,
            first: false
        }]
    );
}

#[test]
fn crossovers_and_their_edge_cases() {
    let (quick, mut slow) = racers();
    // A wrong answer would disqualify the second one from 1.2 B on: no crossover after it
    slow[3].outcome = Outcome::Wrong;
    slow[3].fuel = 6 * B / 5;
    assert_eq!(crossovers(&quick, &slow, B / 100, 10 * B), []);
    // ... but one at 2 B is beyond the crossover at 1.34 B
    slow[3].fuel = 2 * B;
    assert_eq!(crossovers(&quick, &slow, B / 100, 10 * B).len(), 1);
    // A wrong answer right at the start leaves nothing to compare
    slow[3].fuel = 0;
    assert_eq!(crossovers(&quick, &slow, B / 100, 10 * B), []);
    // An unverified answer is never solved: the second entry's three proofs failing hands the
    // lead to the first for good
    let (quick, mut slow) = racers();
    for r in &mut slow[..3] {
        r.outcome = Outcome::Unverified;
    }
    assert_eq!(crossovers(&quick, &slow, B / 100, 10 * B), []);
    // Equal entries never overtake each other, ties keep the lead
    assert_eq!(crossovers(&quick, &quick, B / 100, 10 * B), []);
    let a = entry(&[Some(B), None]);
    let b = entry(&[None, Some(B)]);
    assert_eq!(crossovers(&a, &b, B / 100, 10 * B), []);
    // Results on other formulas aren't compared
    assert_eq!(crossovers(&quick, &quick[..2], B / 100, 10 * B), []);
    let mut renamed = quick.clone();
    renamed[0].name = "other".into();
    assert_eq!(crossovers(&quick, &renamed, B / 100, 10 * B), []);
    assert_eq!(crossovers(&[], &[], B / 100, 10 * B), []);
}

/// Who leads at every whole budget from `low` to `high`, by brute force: lead changes as
/// [`crossovers`] reports them.
fn crossovers_by_scanning(
    first: &[InstanceResult],
    second: &[InstanceResult],
    low: u64,
    high: u64,
) -> Vec<Crossover> {
    let wrong = first
        .iter()
        .chain(second)
        .filter(|r| r.outcome == Outcome::Wrong)
        .map(|r| r.fuel)
        .min()
        .unwrap_or(u64::MAX);
    let n_par2 = |rs: &[InstanceResult], x: u64| -> i128 {
        rs.iter()
            .map(|r| {
                if r.outcome_at(x).solved() {
                    i128::from(r.fuel)
                } else {
                    2 * i128::from(x)
                }
            })
            .sum()
    };
    let mut leader = None;
    let mut found = Vec::new();
    for x in low..=high.min(wrong.saturating_sub(1)) {
        let d = n_par2(first, x) - n_par2(second, x);
        let now = match d.signum() {
            -1 => true,
            1 => false,
            _ => continue,
        };
        if leader.is_some_and(|l| l != now) {
            found.push(Crossover {
                budget: x,
                first: now,
            });
        }
        leader = Some(now);
    }
    found
}

fn config() -> ProptestConfig {
    contextualize_config(ProptestConfig {
        cases: 300,
        rng_seed: RngSeed::Fixed(0x5ab1e),
        failure_persistence: None,
        ..ProptestConfig::default()
    })
}

/// 1 to 6 entries on 1 to 5 formulas, some solved with up to the whole budget. Small fuel
/// values make ties likely.
fn portfolio() -> impl Strategy<Value = Vec<Vec<Option<u64>>>> {
    let fuel = prop_oneof![Just(None), (0..=4u64).prop_map(|k| Some(k * B / 4))];
    (1..=6usize, 1..=5usize).prop_flat_map(move |(n, m)| {
        prop::collection::vec(prop::collection::vec(fuel.clone(), m), n)
    })
}

/// The worth of a coalition (a bit set of entries): what its VBS saves on every formula over
/// solving nothing, in fuel.
fn worth(costs: &[Vec<u64>], coalition: usize) -> f64 {
    let formulas = costs[0].len();
    (0..formulas)
        .map(|f| {
            let best = (0..costs.len())
                .filter(|e| coalition >> e & 1 == 1)
                .map(|e| costs[e][f])
                .min()
                .unwrap_or(2 * B);
            #[expect(clippy::cast_precision_loss, reason = "small test numbers")]
            let saved = (2 * B - best) as f64;
            saved
        })
        .sum()
}

fn factorial(n: usize) -> f64 {
    (1..=n)
        .map(|k| f64::from(u32::try_from(k).unwrap()))
        .product()
}

/// The Shapley value by its definition: the weighted marginal contribution to every coalition
/// without the entry.
fn shapley_by_definition(costs: &[Vec<u64>]) -> Vec<f64> {
    let n = costs.len();
    let formulas = f64::from(u32::try_from(costs[0].len()).unwrap());
    (0..n)
        .map(|i| {
            (0..1usize << n)
                .filter(|s| s >> i & 1 == 0)
                .map(|s| {
                    let size = s.count_ones() as usize;
                    let weight = factorial(size) * factorial(n - size - 1) / factorial(n);
                    weight * (worth(costs, s | 1 << i) - worth(costs, s))
                })
                .sum::<f64>()
                / formulas
                / 1e9
        })
        .collect()
}

/// How one entry did on a formula in [`pair`]: `None` for a timeout, else what it gave (see
/// [`entry_of`]) after how much fuel.
type Run = Option<(u8, u64)>;

/// Two entries on 1 to 5 formulas with small fuel, so ties, jumps and crossings inside
/// segments all happen, and the outcomes that count differently at different budgets.
fn pair() -> impl Strategy<Value = Vec<(Run, Run)>> {
    let result = prop_oneof![
        3 => Just(None),
        8 => (0..4u8, 0..=60u64).prop_map(Some),
    ];
    prop::collection::vec((result.clone(), result), 1..=5)
}

/// An entry's results from (kind, fuel): solved, wrong, unverified or unknown.
fn entry_of(results: &[Run]) -> Vec<InstanceResult> {
    results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut result = result(&format!("f{i}"), Family::Hashi, Answer::Sat, None);
            result.fuel = 60;
            if let Some((kind, fuel)) = *r {
                result.fuel = fuel;
                result.outcome = match kind {
                    0 => Outcome::Wrong,
                    1 => Outcome::Unverified,
                    2 => Outcome::Unknown,
                    _ => Outcome::Solved(Answer::Sat),
                };
            }
            result
        })
        .collect()
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn crossovers_match_a_scan_of_every_budget(results in pair(), low in 0..20u64) {
        let (a, b): (Vec<_>, Vec<_>) = results.into_iter().unzip();
        let (a, b) = (entry_of(&a), entry_of(&b));
        prop_assert_eq!(crossovers(&a, &b, low, 60), crossovers_by_scanning(&a, &b, low, 60));
    }

    #[test]
    fn shapley_matches_the_definition(fuel in portfolio()) {
        let entries: Vec<Vec<InstanceResult>> = fuel.iter().map(|f| entry(f)).collect();
        let refs: Vec<&[InstanceResult]> = entries.iter().map(Vec::as_slice).collect();
        let (p, member) = Portfolio::new(&refs, B);
        prop_assert!(member.iter().all(|&m| m));
        let costs: Vec<Vec<u64>> = fuel
            .iter()
            .map(|f| f.iter().map(|c| c.unwrap_or(2 * B)).collect())
            .collect();
        let fast = p.shapley();
        let slow = shapley_by_definition(&costs);
        for (a, b) in fast.iter().zip(&slow) {
            prop_assert!(close(*a, *b), "closed form {fast:?}, definition {slow:?}");
        }
        // Efficiency: the values add up to what the whole portfolio saves
        let all = (1usize << costs.len()) - 1;
        let formulas = f64::from(u32::try_from(costs[0].len()).unwrap());
        prop_assert!(close(fast.iter().sum::<f64>(), worth(&costs, all) / formulas / 1e9));
        prop_assert!(close(fast.iter().sum::<f64>(), 2.0 - p.vbs().par2));
        // Shares of the solved formulas add up to the VBS's, marginal contributions are gains
        prop_assert!(close(p.sota().iter().sum::<f64>(), f64::from(p.vbs().solved)));
        prop_assert!(p.marginal().iter().all(|&(m, r)| m >= 0.0 && r >= 0.0));
        let relative: f64 = p.contributions().iter().map(|c| c.relative).sum();
        prop_assert!(relative == 0.0 || close(relative, 1.0));
        // No entry gets less than its marginal contribution in this game
        for ((m, _), phi) in p.marginal().iter().zip(&fast) {
            prop_assert!(*phi + 1e-9 >= *m);
        }
    }
}
