//! LEAKY. Category 1 (problem.md): off-by-one in a rolling window indicator.
//!
//! The window is meant to be trailing (`close[i-K+1..=i]`, shifted back one
//! bar before use) but was coded as a forward/centered window that includes
//! bar `i` and the next `K-1` bars.

use crate::engine::{Bar, Strategy};

pub struct WindowMeanReversion {
    pub window: usize,
}

impl Strategy for WindowMeanReversion {
    fn name(&self) -> &str {
        "window_mean_reversion"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let n = closes.len();
        (0..n)
            .map(|i| {
                let end = (i + self.window - 1).min(n - 1);
                let slice = &closes[i..=end];
                let forward_mean = slice.iter().sum::<f64>() / slice.len() as f64;
                // Signal: bet that price will move toward the forward-window
                // mean -- except that mean is built from bars the strategy
                // could not have seen yet.
                (forward_mean - closes[i]) / closes[i]
            })
            .collect()
    }
}
