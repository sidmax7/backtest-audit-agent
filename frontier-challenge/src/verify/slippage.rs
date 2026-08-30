//! A second empirical verification axis, alongside `shift_test`: instead of
//! perturbing *when* a feature is used, this perturbs *what fill price* the
//! backtest assumes, re-running the exact same feature/offset once with the
//! strategy's own declared `Strategy::realized_return` and once forced to
//! the honest close-to-close default, and comparing Sharpe. A strategy
//! whose feature has zero temporal leak (Clean under `shift_test`) can
//! still be leaky here if its PnL calculation assumes an unrealistically
//! favorable fill -- proving the agent's tool-use architecture generalizes
//! to a structurally different bug class, not just more instances of
//! temporal leakage. See `strategies::execution_realism_leak` for the
//! seeded strategy this exists to catch, and `CHANGELOG.md` for the
//! empirical numbers the threshold below was picked from.

use super::Verdict;
use crate::engine::backtest::run_backtest_with_realized_return;
use crate::engine::{Bar, Strategy};

#[derive(Debug, Clone, Copy)]
pub struct SlippageTestResult {
    pub sharpe_as_declared: f64,
    pub sharpe_honest_fill: f64,
    /// `sharpe_as_declared - sharpe_honest_fill`. Large and positive = the
    /// strategy's own fill assumption is doing real, unearned work. Near
    /// zero = its `realized_return` is the honest default (or has no
    /// material effect).
    pub delta: f64,
}

/// A strategy whose only role is supplying the honest default
/// `realized_return` for `slippage_test`'s baseline leg. Its own
/// `compute_features`/`execution_offset` are never called -- the real
/// feature and offset come from the strategy actually under test.
struct HonestFillBaseline;

impl Strategy for HonestFillBaseline {
    fn name(&self) -> &str {
        "honest_fill_baseline"
    }
    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        vec![0.0; bars.len()]
    }
}

/// Runs `strategy`'s own feature and offset through the backtest twice --
/// once with its own declared `realized_return`, once with the honest
/// default forced -- and reports the Sharpe delta between them.
pub fn slippage_test(bars: &[Bar], strategy: &dyn Strategy) -> SlippageTestResult {
    let feature = strategy.compute_features(bars);
    let offset = strategy.execution_offset();

    let as_declared = run_backtest_with_realized_return(bars, &feature, offset, strategy);
    let honest_fill =
        run_backtest_with_realized_return(bars, &feature, offset, &HonestFillBaseline);

    SlippageTestResult {
        sharpe_as_declared: as_declared.sharpe,
        sharpe_honest_fill: honest_fill.sharpe,
        delta: as_declared.sharpe - honest_fill.sharpe,
    }
}

/// Empirically justified (see `CHANGELOG.md` for the full measured table):
/// every strategy using the honest default `realized_return` produces
/// `delta == 0.0` exactly, since "as declared" and "honest fill" are then
/// the same computation. `FavorableFillReversion`'s own delta measures
/// ~4.36-4.64 across 4 independent seeds. `0.5` sits comfortably above
/// floating-point noise around zero and comfortably below that cluster --
/// the same two-clusters-with-a-gap shape `DELTA_ROBUSTNESS_THRESHOLD` was
/// picked from, on this axis instead.
pub const SLIPPAGE_DELTA_THRESHOLD: f64 = 0.5;

/// Verdict rule for the execution-realism axis, independent of (and
/// composable with) `audit`'s temporal-shift rule -- `advanced.rs` treats
/// either tool reporting Leaky as sufficient, mirroring how
/// `IMPLAUSIBLE_SHARPE_CEILING` already covers `shift_test`'s own blind
/// spot on a different bug.
pub fn audit_slippage(result: &SlippageTestResult) -> Verdict {
    if result.delta > SLIPPAGE_DELTA_THRESHOLD {
        Verdict::Leaky
    } else {
        Verdict::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{generate_series, PriceParams};
    use crate::strategies::all_strategies;

    /// Every currently-registered strategy uses the honest default
    /// `realized_return` except `execution_realism_leak`'s -- confirms the
    /// baseline leg of `slippage_test` really is a no-op for everything
    /// that doesn't declare an unrealistic fill.
    #[test]
    fn slippage_test_is_a_no_op_for_every_strategy_using_the_honest_default_fill() {
        let bars = generate_series(PriceParams::default());
        for entry in all_strategies() {
            if entry.name == "execution_realism_leak" {
                continue;
            }
            let result = slippage_test(&bars, entry.strategy.as_ref());
            assert!(
                result.delta.abs() < 1e-9,
                "{}: expected delta ~= 0.0 under the honest default fill, got {}",
                entry.name,
                result.delta
            );
            assert_eq!(audit_slippage(&result), Verdict::Clean);
        }
    }

    /// The seeded strategy this tool exists to catch: Clean under the
    /// ordinary temporal `shift_test` (its feature has no look-ahead at
    /// all), but Leaky under `slippage_test` because of its declared
    /// favorable-fill assumption -- across multiple independent seeds.
    #[test]
    fn slippage_test_catches_the_favorable_fill_leak_shift_test_is_blind_to() {
        use crate::strategies::execution_realism_leak::FavorableFillReversion;
        use crate::verify::{audit, shift_test};

        for seed in [42u64, 7, 123, 999] {
            let bars = generate_series(PriceParams {
                seed,
                ..PriceParams::default()
            });
            let strategy = FavorableFillReversion;

            let temporal = audit(&shift_test(&bars, &strategy));
            assert_eq!(
                temporal,
                Verdict::Clean,
                "seed {seed}: the temporal shift-test should be blind to this bug class"
            );

            let slippage_result = slippage_test(&bars, &strategy);
            assert_eq!(
                audit_slippage(&slippage_result),
                Verdict::Leaky,
                "seed {seed}: slippage_test should catch what shift_test misses (delta={})",
                slippage_result.delta
            );
        }
    }
}
