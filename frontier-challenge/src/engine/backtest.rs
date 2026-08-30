//! Minimal backtest loop: a strategy's per-bar feature values in, per-bar
//! returns and a Sharpe ratio out.
//!
//! The engine does not impose a single fixed decision-to-execution lag.
//! Instead each `Strategy` declares its own `execution_offset` -- how many
//! bars separate the feature it computed from the return it is applied to.
//! `1` is the honest convention (decide using information through bar `i`,
//! earn the return realized over bar `i+1`). `0` is the "execution timing
//! mismatch" bug from `problem.md` (bet on the very return whose bar the
//! feature was computed from). Keeping this as a per-strategy declaration
//! rather than a hard-coded engine constant is what lets one seeded strategy
//! carry that specific bug without special-casing the engine.

use super::price::Bar;

pub trait Strategy {
    fn name(&self) -> &str;

    /// Returns one feature value per bar. Implementations are free (and, for
    /// the seeded leaky strategies, deliberately wrong) about which bars they
    /// read to compute `feature[i]` -- that is where look-ahead bias lives.
    fn compute_features(&self, bars: &[Bar]) -> Vec<f64>;

    /// Bars between the feature index and the return it is applied to.
    /// `1` = correct ("trade at the next bar"). `0` = the execution-timing bug.
    fn execution_offset(&self) -> usize {
        1
    }

    /// Named component feature series this strategy's final signal is
    /// built from, for leak localization (`verify::localize`). Defaults to
    /// a single component, `"feature"`, equal to `compute_features`'s own
    /// output -- existing strategies need no changes. A strategy overrides
    /// this only when it wants to expose real sub-computations so
    /// `localize` can shift-test each one independently and report which
    /// one actually carries a leak, rather than only whether the strategy
    /// as a whole does.
    fn feature_components(&self, bars: &[Bar]) -> Vec<(&'static str, Vec<f64>)> {
        vec![("feature", self.compute_features(bars))]
    }

    /// The per-bar return this strategy assumes it realizes when holding
    /// `position` (already offset/lagged, in `[-1, 1]`) into `bar`.
    /// Defaults to the honest close-to-close return, ignoring `position`'s
    /// sign entirely -- the same fill for a long or a short, like every
    /// other seeded strategy in this crate. Overriding this to assume a
    /// systematically better fill for whichever side you're actually on is
    /// a *different* leakage axis than everything else here: not a
    /// temporal look-ahead in the feature, but an unrealistic assumption
    /// about what price you'd actually get filled at, baked directly into
    /// the PnL calculation (see `verify::slippage_test`).
    fn realized_return(&self, prev_bar: &Bar, bar: &Bar, _position: f64) -> f64 {
        (bar.close - prev_bar.close) / prev_bar.close
    }
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub returns: Vec<f64>,
    pub sharpe: f64,
    pub mean_return: f64,
    pub std_return: f64,
}

/// Per-bar percentage return, close-to-close. `returns[i]` is the return
/// realized over bar `i` (from `close[i-1]` to `close[i]`); index 0 is
/// undefined and omitted, so `returns.len() == bars.len() - 1`.
pub fn close_to_close_returns(bars: &[Bar]) -> Vec<f64> {
    bars.windows(2)
        .map(|w| (w[1].close - w[0].close) / w[0].close)
        .collect()
}

/// Runs `strategy` against `bars` using its declared execution offset and
/// its own `realized_return` (defaults to honest close-to-close, so this
/// is a drop-in identical result for every strategy that doesn't override
/// it), returning the realized PnL series and Sharpe ratio.
///
/// Position sizing: `feature[i]` is clamped to `[-1, 1]` and used directly as
/// exposure -- no leverage, no compounding, so the Sharpe ratio reflects pure
/// signal quality rather than money-management choices.
pub fn run_backtest(bars: &[Bar], strategy: &dyn Strategy) -> BacktestResult {
    let feature = strategy.compute_features(bars);
    assert_eq!(
        feature.len(),
        bars.len(),
        "{} must return one feature value per bar",
        strategy.name()
    );
    run_backtest_with_realized_return(bars, &feature, strategy.execution_offset(), strategy)
}

/// Same PnL/Sharpe computation as [`run_backtest`], but takes an already
/// -computed feature vector directly. This is what lets `verify::shift_test`
/// rerun a strategy against a *perturbed* feature series without needing a
/// second `Strategy` impl per strategy under test.
pub fn run_backtest_from_features(bars: &[Bar], feature: &[f64], offset: usize) -> BacktestResult {
    let rets = close_to_close_returns(bars);

    // Bar 0 has no realized return regardless of offset, so iteration always
    // starts at 1; `offset` only changes which feature index pairs with it
    // (offset=1: feature[i-1], the honest "decide before, earn after"
    // contract; offset=0: feature[i], the execution-timing bug that pairs a
    // feature with the very bar's return it was computed from).
    let start = offset.max(1);
    let mut pnl = Vec::with_capacity(bars.len().saturating_sub(start));
    for i in start..bars.len() {
        let position = feature[i - offset].clamp(-1.0, 1.0);
        let ret = rets[i - 1];
        pnl.push(position * ret);
    }

    sharpe_from_returns(pnl)
}

/// Like [`run_backtest_from_features`], but sources each bar's return from
/// `strategy.realized_return` instead of the market's own close-to-close
/// return -- letting a strategy's declared (possibly optimistic) fill
/// assumption actually show up in its Sharpe. Deliberately separate from
/// `run_backtest_from_features`, which `verify::shift_test` depends on for
/// its own proven regression and stays untouched by this.
pub fn run_backtest_with_realized_return(
    bars: &[Bar],
    feature: &[f64],
    offset: usize,
    strategy: &dyn Strategy,
) -> BacktestResult {
    let start = offset.max(1);
    let mut pnl = Vec::with_capacity(bars.len().saturating_sub(start));
    for i in start..bars.len() {
        let position = feature[i - offset].clamp(-1.0, 1.0);
        let ret = strategy.realized_return(&bars[i - 1], &bars[i], position);
        pnl.push(position * ret);
    }

    sharpe_from_returns(pnl)
}

fn sharpe_from_returns(returns: Vec<f64>) -> BacktestResult {
    let n = returns.len() as f64;
    if returns.is_empty() {
        return BacktestResult {
            returns,
            sharpe: 0.0,
            mean_return: 0.0,
            std_return: 0.0,
        };
    }
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    let std = variance.sqrt();
    // Annualize assuming daily-ish bars (252 periods/year), the standard convention.
    let sharpe = if std > 1e-12 {
        (mean / std) * (252.0_f64).sqrt()
    } else {
        0.0
    };
    BacktestResult {
        returns,
        sharpe,
        mean_return: mean,
        std_return: std,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::price::{generate_series, PriceParams};

    struct FlatStrategy;
    impl Strategy for FlatStrategy {
        fn name(&self) -> &str {
            "flat"
        }
        fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
            vec![0.0; bars.len()]
        }
    }

    #[test]
    fn flat_strategy_has_zero_sharpe() {
        let bars = generate_series(PriceParams::default());
        let result = run_backtest(&bars, &FlatStrategy);
        assert_eq!(result.sharpe, 0.0);
        assert!(result.returns.iter().all(|&r| r == 0.0));
    }

    #[test]
    fn execution_offset_shrinks_the_return_series_correctly() {
        let bars = generate_series(PriceParams {
            n_bars: 100,
            ..PriceParams::default()
        });
        struct OffsetZero;
        impl Strategy for OffsetZero {
            fn name(&self) -> &str {
                "offset0"
            }
            fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
                vec![0.5; bars.len()]
            }
            fn execution_offset(&self) -> usize {
                0
            }
        }
        let result = run_backtest(&bars, &OffsetZero);
        assert_eq!(result.returns.len(), bars.len() - 1);
    }
}
