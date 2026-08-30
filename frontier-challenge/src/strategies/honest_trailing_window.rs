//! CLEAN control -- the honest counterpart of `execution_timing_mismatch`.
//!
//! Same trailing-window reading (a legitimate "price vs. its recent average
//! as of bar `i`" -- using `bars[..=i]` to describe bar `i` itself is not a
//! leak), but betting *reversion* for the next bar rather than continuation,
//! and relying on the engine's honest default `execution_offset() == 1`
//! (not overridden here) to earn bar `i+1`'s return rather than bar `i`'s.

use crate::engine::{Bar, Strategy};
use crate::strategies::trailing_mean;

pub struct TrailingMeanFade {
    pub window: usize,
}

impl Strategy for TrailingMeanFade {
    fn name(&self) -> &str {
        "trailing_mean_fade"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        (0..closes.len())
            .map(|i| {
                let mean = trailing_mean(&closes, i, self.window);
                (mean - closes[i]) / mean
            })
            .collect()
    }
}
