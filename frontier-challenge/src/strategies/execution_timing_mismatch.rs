//! LEAKY. Category 2 (problem.md): execution timing mismatch -- simulates
//! order execution at the current bar's close instead of the next bar's
//! open.
//!
//! The feature itself is an honest, causal trailing-window signal (uses
//! only `bars[..=i]`, which is legitimate for "the reading as of now").
//! `execution_offset() == 0` is the bug: it bets on the return realized
//! *over the same bar* the feature was computed from, instead of the next
//! bar. Because the feature legitimately depends on `close[i]` and the
//! return it's paired with is also a direct function of `close[i]`, this is
//! a real look-ahead leak.

use crate::engine::{Bar, Strategy};
use crate::strategies::trailing_mean;

pub struct MeanDeviationCrossover {
    pub window: usize,
}

impl Strategy for MeanDeviationCrossover {
    fn name(&self) -> &str {
        "mean_deviation_crossover"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        (0..closes.len())
            .map(|i| {
                let mean = trailing_mean(&closes, i, self.window);
                (closes[i] - mean) / mean
            })
            .collect()
    }

    fn execution_offset(&self) -> usize {
        0
    }
}
