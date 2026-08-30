//! The correctness proof for the CSV adapter: for every seeded strategy,
//! exporting its real price series and computed features to CSV and auditing
//! *that* must reproduce the Rust-native `shift_test` result bit-for-bit.
//!
//! Without this, "you can audit your own backtest via CSV" would be an
//! unverified claim -- exactly the kind this project refuses to make about
//! anything else. Fully offline and deterministic; no API key, no network.

use frontier_challenge::csv_input;
use frontier_challenge::engine::{generate_series, PriceParams};
use frontier_challenge::strategies::all_strategies;
use frontier_challenge::verify::{audit, shift_test, shift_test_features};

/// Serializes bars + a feature column the way a user's `to_csv()` would.
/// Uses 17 significant digits so the round-trip is exact for f64, which is
/// what lets the assertions below demand equality rather than approximation.
fn to_csv(bars: &[frontier_challenge::engine::Bar], feature: &[f64]) -> String {
    let mut s = String::from("open,high,low,close,volume,feature\n");
    for (b, f) in bars.iter().zip(feature) {
        s.push_str(&format!(
            "{:.17e},{:.17e},{:.17e},{:.17e},{:.17e},{:.17e}\n",
            b.open, b.high, b.low, b.close, b.volume, f
        ));
    }
    s
}

#[test]
fn csv_round_trip_reproduces_the_native_shift_test_for_every_seeded_strategy() {
    let bars = generate_series(PriceParams::default());

    for entry in all_strategies() {
        let strategy = entry.strategy.as_ref();
        let native = shift_test(&bars, strategy);

        let feature = strategy.compute_features(&bars);
        let csv = to_csv(&bars, &feature);
        let parsed = csv_input::parse(&csv)
            .unwrap_or_else(|e| panic!("{}: CSV parse failed: {e:#}", entry.name));

        assert_eq!(parsed.bars.len(), bars.len(), "{}: bar count", entry.name);
        assert_eq!(parsed.features.len(), 1, "{}: feature columns", entry.name);

        let via_csv = shift_test_features(
            &parsed.bars,
            &parsed.features[0].1,
            strategy.execution_offset(),
        );

        assert_eq!(
            via_csv.sharpe_original, native.sharpe_original,
            "{}: sharpe_original diverged between native and CSV paths",
            entry.name
        );
        assert_eq!(
            via_csv.sharpe_shifted, native.sharpe_shifted,
            "{}: sharpe_shifted diverged between native and CSV paths",
            entry.name
        );
        assert_eq!(
            audit(&via_csv),
            audit(&native),
            "{}: verdict diverged between native and CSV paths",
            entry.name
        );
    }
}

/// The adapter must be able to reach a Leaky verdict on data it has never
/// seen as Rust code -- i.e. the CSV path is genuinely usable on its own,
/// not just as a mirror of the native path.
#[test]
fn csv_path_independently_flags_a_leaky_export_and_clears_an_honest_one() {
    let bars = generate_series(PriceParams::default());
    let mut leaky_seen = false;
    let mut clean_seen = false;

    for entry in all_strategies() {
        // execution_realism_leak leaks on the fill axis, which a CSV export
        // cannot express (see audit_csv's caveats) -- excluded deliberately.
        if entry.name == "execution_realism_leak" {
            continue;
        }
        let strategy = entry.strategy.as_ref();
        let csv = to_csv(&bars, &strategy.compute_features(&bars));
        let parsed = csv_input::parse(&csv).unwrap();
        let r = shift_test_features(
            &parsed.bars,
            &parsed.features[0].1,
            strategy.execution_offset(),
        );
        let predicted_leaky = audit(&r) == frontier_challenge::verify::Verdict::Leaky;
        assert_eq!(
            predicted_leaky, entry.is_leaky,
            "{}: CSV-path verdict disagrees with ground truth",
            entry.name
        );
        leaky_seen |= entry.is_leaky;
        clean_seen |= !entry.is_leaky;
    }
    assert!(leaky_seen && clean_seen, "battery must cover both outcomes");
}
