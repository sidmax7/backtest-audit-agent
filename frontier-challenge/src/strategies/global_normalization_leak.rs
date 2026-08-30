//! LEAKY. Category 3 (problem.md): improper chronological split via
//! whole-series standardization.
//!
//! `mean`/`std` are fit once over the *entire* series (including bars far in
//! the future of whatever index is being scored) instead of an expanding or
//! trailing window computed only from bars available up to that index. This
//! is the classic "standardize before splitting" bug -- equivalent to
//! `df[:split_date]` after a global `.fit()` in Python.

use crate::engine::{Bar, Strategy};

pub struct ZScoreReversion;

impl Strategy for ZScoreReversion {
    fn name(&self) -> &str {
        "zscore_reversion"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let n = closes.len() as f64;
        let global_mean = closes.iter().sum::<f64>() / n;
        let global_var = closes
            .iter()
            .map(|c| (c - global_mean).powi(2))
            .sum::<f64>()
            / n;
        let global_std = global_var.sqrt().max(1e-9);

        closes
            .iter()
            .map(|&c| {
                let z = (c - global_mean) / global_std;
                // Mean-revert toward the *whole-series* mean -- only knowable
                // in hindsight, once every bar (including future ones) exists.
                -z * 0.1
            })
            .collect()
    }
}
