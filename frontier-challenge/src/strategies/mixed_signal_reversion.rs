//! LEAKY. A deliberately mixed strategy: sums an honest prior-return
//! component (using only past data -- identical mechanism to
//! `honest_mean_reversion.rs`) with a small forward-window component (same
//! off-by-one mechanism as `forward_window_lookahead.rs`). Exists as a
//! discriminating fixture for `verify::localize`: the strategy as a whole
//! is genuinely leaky (the combined feature is what `compute_features`
//! returns, and what the ordinary whole-vector `shift_test` sees), but
//! `feature_components` exposes the two ingredients separately so
//! `localize` can verdict them differently -- proving the tool actually
//! points at which component carries the leak, not just that one exists
//! somewhere in the sum. Both component method names are deliberately
//! mechanism-only, not judgment-bearing (`forward_window_component`, not
//! anything containing "leak"), since real source text -- unlike the file
//! path -- is what the audit binaries actually show the LLM; see
//! `audit_source.rs` and CHANGELOG Iteration 5 for why that distinction
//! matters here.

use crate::engine::{Bar, Strategy};

pub struct MixedSignalReversion {
    pub window: usize,
}

impl MixedSignalReversion {
    fn prior_return_component(&self, bars: &[Bar]) -> Vec<f64> {
        let mut feature = vec![0.0; bars.len()];
        for i in 1..bars.len() {
            let prev_ret = (bars[i].close - bars[i - 1].close) / bars[i - 1].close;
            feature[i] = -prev_ret * 5.0;
        }
        feature
    }

    fn forward_window_component(&self, bars: &[Bar]) -> Vec<f64> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let n = closes.len();
        (0..n)
            .map(|i| {
                let end = (i + self.window - 1).min(n - 1);
                let slice = &closes[i..=end];
                let forward_mean = slice.iter().sum::<f64>() / slice.len() as f64;
                (forward_mean - closes[i]) / closes[i]
            })
            .collect()
    }
}

impl Strategy for MixedSignalReversion {
    fn name(&self) -> &str {
        "mixed_signal_reversion"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let prior_return = self.prior_return_component(bars);
        let forward_window = self.forward_window_component(bars);
        prior_return
            .iter()
            .zip(forward_window.iter())
            .map(|(p, f)| p + f)
            .collect()
    }

    fn feature_components(&self, bars: &[Bar]) -> Vec<(&'static str, Vec<f64>)> {
        vec![
            ("prior_return_component", self.prior_return_component(bars)),
            (
                "forward_window_component",
                self.forward_window_component(bars),
            ),
        ]
    }
}
