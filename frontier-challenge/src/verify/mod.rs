//! The shift-test verifier: the one empirical tool both binaries call into.
//!
//! Perturbation: shift the strategy's own computed feature series forward by
//! exactly one bar (`shifted[i] = feature[i-1]`) and rerun the backtest with
//! the strategy's execution offset unchanged, then compare Sharpe ratios.
//!
//! This single perturbation is enough to catch two of our three seeded bug
//! categories (`ForwardWindowLookahead`, `GlobalNormalizationLeak`): their
//! leaks are *persistent* across a one-bar reindex, so Sharpe stays robust.
//! It structurally cannot catch the third (`ExecutionTimingMismatch`): that
//! bug is itself an exactly-one-bar offset error, and shifting the feature
//! array by exactly one bar happens to repair it, collapsing its Sharpe just
//! like a clean strategy's. See `audit` below for how we cover that blind
//! spot, and `CHANGELOG.md` for the empirical numbers that justify the
//! thresholds chosen here.

use crate::engine::backtest::run_backtest_from_features;
use crate::engine::{Bar, Strategy};

pub mod slippage;
pub use slippage::{audit_slippage, slippage_test, SlippageTestResult};

#[derive(Debug, Clone, Copy)]
pub struct ShiftTestResult {
    pub sharpe_original: f64,
    pub sharpe_shifted: f64,
    /// `sharpe_original - sharpe_shifted`. Large and positive = collapsed
    /// under perturbation. Near zero (or negative) = stayed robust.
    pub delta: f64,
}

/// Shifts `feature` forward by one bar: `shifted[i] = feature[i-1]`,
/// `shifted[0] = 0.0` (bar 0 never contributes to realized PnL regardless of
/// execution offset, so its value is unused).
fn shift_forward_one_bar(feature: &[f64]) -> Vec<f64> {
    let mut shifted = vec![0.0; feature.len()];
    if !feature.is_empty() {
        shifted[1..].copy_from_slice(&feature[..feature.len() - 1]);
    }
    shifted
}

/// Runs `feature` under both its original and one-bar-shifted form and
/// reports the Sharpe delta. Pure and strategy-agnostic: it needs a price
/// series and an array of numbers, not a `Strategy` impl, which is what
/// lets `csv_input`-loaded data from a backtest written in any language be
/// audited on exactly the same code path as this crate's own seeded
/// strategies. `shift_test` below is a thin wrapper over this using a
/// strategy's own whole feature vector; `localize` reuses it directly on
/// individual named components.
pub fn shift_test_features(bars: &[Bar], feature: &[f64], offset: usize) -> ShiftTestResult {
    let original = run_backtest_from_features(bars, feature, offset);
    let shifted_feature = shift_forward_one_bar(feature);
    let shifted = run_backtest_from_features(bars, &shifted_feature, offset);

    ShiftTestResult {
        sharpe_original: original.sharpe,
        sharpe_shifted: shifted.sharpe,
        delta: original.sharpe - shifted.sharpe,
    }
}

/// Runs the strategy under both the original and one-bar-shifted feature
/// series and reports the Sharpe delta. This is the one tool `advanced.rs`
/// calls into to empirically verify (or falsify) a leakage hypothesis.
pub fn shift_test(bars: &[Bar], strategy: &dyn Strategy) -> ShiftTestResult {
    shift_test_features(
        bars,
        &strategy.compute_features(bars),
        strategy.execution_offset(),
    )
}

/// Runs `shift_test`'s perturbation independently on each of a strategy's
/// named feature components (see `Strategy::feature_components`), so the
/// result can point at *which* component carries a leak rather than only
/// whether the strategy as a whole does. Reuses `audit`'s existing
/// calibrated thresholds directly -- no separate calibration needed, since
/// each component is shift-tested exactly the way a whole feature vector
/// is. Strategies that don't override `feature_components` get back a
/// single entry identical to `shift_test`'s own result.
pub fn localize(
    bars: &[Bar],
    strategy: &dyn Strategy,
) -> Vec<(&'static str, ShiftTestResult, Verdict)> {
    let offset = strategy.execution_offset();
    strategy
        .feature_components(bars)
        .into_iter()
        .map(|(name, component)| {
            let result = shift_test_features(bars, &component, offset);
            let verdict = audit(&result);
            (name, result, verdict)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Leaky,
    Clean,
}

/// Empirically justified thresholds (see `CHANGELOG.md` Iteration 3 for the
/// measured Sharpe table these were picked from):
///
/// - `DELTA_ROBUSTNESS_THRESHOLD`: below this, the shift barely dented
///   performance -> primary leak signal. Clean controls collapse by several
///   Sharpe points under shift; leaky strategies with a persistent leak drop
///   by well under one point. `1.0` sits between the two clusters.
/// - `IMPLAUSIBLE_SHARPE_CEILING`: catches `ExecutionTimingMismatch`, whose
///   leak the shift-test's own perturbation happens to repair (see module
///   docs). No legitimate strategy on this synthetic generator clears this
///   Sharpe -- it is calibrated well above the honest controls' range.
pub const DELTA_ROBUSTNESS_THRESHOLD: f64 = 1.0;
pub const IMPLAUSIBLE_SHARPE_CEILING: f64 = 3.0;

/// The two-signal verdict rule described above. `advanced.rs` uses this as
/// its second hypothesis when the shift-test delta alone is ambiguous; see
/// `problem.md` §03 step 4 ("If inconclusive, it explores an alternative
/// hypothesis") -- this ceiling *is* that alternative hypothesis.
pub fn audit(result: &ShiftTestResult) -> Verdict {
    let robust_under_shift = result.delta < DELTA_ROBUSTNESS_THRESHOLD;
    let implausibly_high = result.sharpe_original > IMPLAUSIBLE_SHARPE_CEILING;
    if robust_under_shift || implausibly_high {
        Verdict::Leaky
    } else {
        Verdict::Clean
    }
}

/// What `advanced.rs` actually reports, as distinct from [`Verdict`] (the
/// pure empirical primitive above, which stays binary and untouched). Adds
/// a third state for when the agent's two tools genuinely disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentVerdict {
    Leaky,
    Clean,
    Inconclusive,
}

/// Combines `static_scan`'s hint with `shift_test`'s empirical verdict into
/// what the agent should report.
///
/// Absence of a static finding is never a signal either way -- the static
/// layer is documented as narrow, so "no findings" never claims clean. The
/// one genuine conflict worth surfacing is the opposite case: `static_scan`
/// positively flagged something, but the empirical two-signal rule -- the
/// thing a verdict actually has to be grounded in -- came back Clean
/// anyway. That specific combination is reported as `Inconclusive` rather
/// than silently deferring to the empirical side, which is what happened
/// before this existed.
pub fn combine_signals(static_findings_present: bool, empirical: Verdict) -> AgentVerdict {
    match (static_findings_present, empirical) {
        (true, Verdict::Clean) => AgentVerdict::Inconclusive,
        (_, Verdict::Leaky) => AgentVerdict::Leaky,
        (false, Verdict::Clean) => AgentVerdict::Clean,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{generate_series, PriceParams};
    use crate::strategies::all_strategies;

    /// `localize` on a strategy that never overrides `feature_components`
    /// must degenerate to exactly `shift_test`'s own result under a single
    /// `"feature"` component -- the default has to be a true no-op.
    #[test]
    fn localize_defaults_to_a_single_component_matching_shift_test() {
        let bars = generate_series(PriceParams::default());
        let strategy = crate::strategies::honest_mean_reversion::PriorReturnFade;
        let whole = shift_test(&bars, &strategy);
        let components = localize(&bars, &strategy);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].0, "feature");
        assert_eq!(components[0].1.delta, whole.delta);
    }

    /// The concrete proof that `localize` actually *points* at the leak
    /// rather than only confirming one exists somewhere in the sum:
    /// `MixedSignalReversion` sums an honest component with a forward-window
    /// one, and is genuinely Leaky as a whole -- but `localize` must
    /// separate the two and verdict them differently.
    #[test]
    fn localize_points_at_which_component_carries_the_leak() {
        let bars = generate_series(PriceParams::default());
        let strategy =
            crate::strategies::mixed_signal_reversion::MixedSignalReversion { window: 5 };

        let whole = audit(&shift_test(&bars, &strategy));
        assert_eq!(
            whole,
            Verdict::Leaky,
            "the combined strategy must itself be leaky for this fixture to be meaningful"
        );

        let components = localize(&bars, &strategy);
        assert_eq!(components.len(), 2);
        let honest = components
            .iter()
            .find(|(name, _, _)| *name == "prior_return_component")
            .expect("prior_return_component present");
        let forward = components
            .iter()
            .find(|(name, _, _)| *name == "forward_window_component")
            .expect("forward_window_component present");
        assert_eq!(
            honest.2,
            Verdict::Clean,
            "the honest component alone must localize as clean"
        );
        assert_eq!(
            forward.2,
            Verdict::Leaky,
            "the forward-window component alone must localize as leaky"
        );
    }

    /// **Documents a known false-positive mode, discovered while building the
    /// CSV adapter** (`csv_input` / `bin/audit_csv.rs`), not a bug to fix
    /// silently: `audit`'s delta signal asks "did shifting destroy this
    /// strategy's edge?", which is only meaningful if there *was* an edge.
    /// A structurally honest strategy (only past data, honest offset) that
    /// simply has no predictive power is trivially "robust to shift" --
    /// nothing collapses because nothing was there -- and is therefore
    /// misclassified as Leaky.
    ///
    /// The seeded battery never exposed this because both honest controls
    /// have a real AR(1) edge (Sharpe ~1.74, delta ~2.27). This test pins the
    /// limitation in place so it is impossible to forget and impossible to
    /// regress into silently: if a future threshold change fixes it, this
    /// test fails loudly and should be rewritten as a passing guarantee.
    /// `bin/audit_csv.rs` already guards against it for external data via
    /// `NO_EDGE_SHARPE`; `audit` itself is unchanged because v1 is locked.
    #[test]
    fn known_limitation_no_edge_honest_strategy_is_misclassified_as_leaky() {
        struct HonestNoEdge;
        impl Strategy for HonestNoEdge {
            fn name(&self) -> &str {
                "honest_no_edge"
            }
            fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
                let mut f = vec![0.0; bars.len()];
                for i in 5..bars.len() {
                    // Honest: reads only bars strictly before i. But keyed to
                    // lag 5, where the generator's phi^5 is ~0, so there is no
                    // real signal to find.
                    f[i] = -((bars[i - 4].close - bars[i - 5].close) / bars[i - 5].close) * 5.0;
                }
                f
            }
        }

        for seed in [42u64, 7, 123, 999] {
            let bars = generate_series(PriceParams {
                seed,
                ..PriceParams::default()
            });
            let r = shift_test(&bars, &HonestNoEdge);
            assert!(
                r.sharpe_original.abs() < 0.5,
                "seed {seed}: fixture must genuinely have no edge, got sharpe {:.3}",
                r.sharpe_original
            );
            assert_eq!(
                audit(&r),
                Verdict::Leaky,
                "seed {seed}: this documents the KNOWN false positive; if this now returns Clean, \
                 the rule was improved -- update this test to assert the fix rather than deleting it"
            );
        }
    }

    /// Regression test for the empirical claim this whole tool rests on:
    /// the shift-test + two-signal `audit` rule correctly separates every
    /// seeded strategy, across multiple independent seeds (not just the
    /// canonical one), without needing an LLM in the loop at all. If this
    /// ever regresses, the thresholds or a strategy's construction changed
    /// in a way that broke the empirical evidence in `CHANGELOG.md`.
    ///
    /// `execution_realism_leak` is deliberately excluded: it is genuinely
    /// Clean under the temporal shift-test alone by design (its leak is on
    /// a different axis entirely -- see `verify::slippage`), so its
    /// `is_leaky: true` ground truth is only correct once `slippage_test`
    /// is also in the mix. `verify::slippage::tests` asserts that combined
    /// claim directly, with the same across-seeds rigor as this test.
    #[test]
    fn shift_test_correctly_classifies_every_seeded_strategy_across_seeds() {
        for seed in [42u64, 7, 123, 999] {
            let bars = generate_series(PriceParams {
                seed,
                ..PriceParams::default()
            });
            for entry in all_strategies() {
                if entry.name == "execution_realism_leak" {
                    continue;
                }
                let result = shift_test(&bars, entry.strategy.as_ref());
                let verdict = audit(&result);
                let predicted_leaky = verdict == Verdict::Leaky;
                assert_eq!(
                    predicted_leaky, entry.is_leaky,
                    "seed {seed}, strategy {}: expected leaky={}, got {verdict:?} (sharpe_original={:.3}, sharpe_shifted={:.3}, delta={:.3})",
                    entry.name, entry.is_leaky, result.sharpe_original, result.sharpe_shifted, result.delta
                );
            }
        }
    }

    /// Fixture for `combine_signals`: a genuine static-vs-empirical
    /// disagreement, not a mocked one. `BORDERLINE_SOURCE` is real source
    /// text with a harmless `let idx = i + 0;` no-op -- syntactically
    /// identical to the shape `SA-FORWARD-WINDOW` pattern-matches on,
    /// without the code actually reaching forward at all -- so
    /// `static_checks::scan` trips a real false positive on it. The *same*
    /// logic, actually run (`PriorReturnFade`: prior-bar-return fade, only
    /// past data, honest offset), is genuinely `Verdict::Clean` under the
    /// real `shift_test`/`audit`. `combine_signals` must report this
    /// combination as `Inconclusive`, not silently pick a side.
    #[test]
    fn combine_signals_reports_inconclusive_on_a_genuine_static_vs_empirical_disagreement() {
        const BORDERLINE_SOURCE: &str = r#"
            impl Strategy for HonestButSuspiciousLooking {
                fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
                    let mut feature = vec![0.0; bars.len()];
                    for i in 1..bars.len() {
                        let idx = i + 0;
                        let prev_ret = (bars[idx].close - bars[idx - 1].close) / bars[idx - 1].close;
                        feature[i] = -prev_ret * 5.0;
                    }
                    feature
                }
            }
        "#;
        let findings = crate::static_checks::scan(BORDERLINE_SOURCE).unwrap();
        assert!(
            !findings.is_empty(),
            "expected the harmless `i + 0` no-op to trip a static false positive"
        );

        let bars = generate_series(PriceParams::default());
        let honest_strategy = crate::strategies::honest_mean_reversion::PriorReturnFade;
        let result = shift_test(&bars, &honest_strategy);
        let empirical_verdict = audit(&result);
        assert_eq!(
            empirical_verdict,
            Verdict::Clean,
            "the actual logic behind the borderline source is genuinely clean"
        );

        assert_eq!(
            combine_signals(true, empirical_verdict),
            AgentVerdict::Inconclusive
        );
    }
}
