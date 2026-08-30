//! LEAKY. Category 1 variant (problem.md): same off-by-one as
//! `forward_window_lookahead.rs` -- the window meant to trail bar `i` is
//! coded as a forward/centered window that reaches into bars not yet
//! seen -- but computed as a volatility breakout signal instead of a
//! mean-reversion one, so it's a genuinely distinct strategy rather than a
//! copy. Added to prove out the "Extending this" section in
//! REPRODUCTION.md: implement `Strategy`, register one `StrategyEntry`,
//! done -- no other file needs to change (`all_strategies()` is the single
//! source every test, example, and binary iterates).

use crate::engine::{Bar, Strategy};

pub struct ForwardVolatilityBreakout {
    pub window: usize,
}

impl ForwardVolatilityBreakout {
    /// Per-bar `(forward_mean, forward_std)` over the same forward-reaching
    /// window both published components are built from. Shared so
    /// `compute_features` and `feature_components` can't silently drift
    /// apart into two different windows.
    fn forward_moments(&self, bars: &[Bar]) -> Vec<(f64, f64)> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let n = closes.len();
        (0..n)
            .map(|i| {
                let end = (i + self.window - 1).min(n - 1);
                let slice = &closes[i..=end];
                let forward_mean = slice.iter().sum::<f64>() / slice.len() as f64;
                let forward_std = (slice
                    .iter()
                    .map(|c| (c - forward_mean).powi(2))
                    .sum::<f64>()
                    / slice.len() as f64)
                    .sqrt();
                (forward_mean, forward_std)
            })
            .collect()
    }
}

impl Strategy for ForwardVolatilityBreakout {
    fn name(&self) -> &str {
        "forward_volatility_breakout"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let moments = self.forward_moments(bars);
        bars.iter()
            .zip(moments.iter())
            .map(|(bar, &(_, forward_std))| {
                // Signal: go long in proportion to how much the *upcoming*
                // window's volatility exceeds a baseline -- only knowable
                // once those future bars actually exist.
                (forward_std / bar.close - 0.01) * 10.0
            })
            .collect()
    }

    /// Exposes the two real sub-computations the blended feature above is
    /// built from, so `verify::localize` can shift-test each independently.
    /// Both happen to be forward-looking here (they're drawn from the same
    /// forward-reaching window), so `localize` correctly flags both -- the
    /// honest result for this strategy, not contrived to look more
    /// discriminating than it actually is.
    fn feature_components(&self, bars: &[Bar]) -> Vec<(&'static str, Vec<f64>)> {
        let moments = self.forward_moments(bars);
        let forward_mean_deviation = bars
            .iter()
            .zip(moments.iter())
            .map(|(bar, &(forward_mean, _))| (forward_mean - bar.close) / bar.close)
            .collect();
        let forward_std_deviation = bars
            .iter()
            .zip(moments.iter())
            .map(|(bar, &(_, forward_std))| (forward_std / bar.close - 0.01) * 10.0)
            .collect();
        vec![
            ("forward_mean_deviation", forward_mean_deviation),
            ("forward_std_deviation", forward_std_deviation),
        ]
    }
}
