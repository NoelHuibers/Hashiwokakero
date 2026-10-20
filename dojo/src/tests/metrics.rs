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

proptest! {
    #![proptest_config(config())]

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
