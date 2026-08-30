//! CLEAN control. Bets against the previous bar's already-realized return,
//! sized by the magnitude of that return -- uses only `bars[..i]` to compute
//! `feature[i]`, and relies on the engine's honest default
//! `execution_offset() == 1` to earn the *next* bar's return.
//!
//! The synthetic price generator (`engine::price`) bakes in a small negative
//! AR(1) coefficient on returns, so this captures a real, modest, legitimate
//! edge -- it is not a strawman with zero expected edge.

use crate::engine::{Bar, Strategy};

pub struct PriorReturnFade;

impl Strategy for PriorReturnFade {
    fn name(&self) -> &str {
        "prior_return_fade"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let mut feature = vec![0.0; bars.len()];
        for i in 1..bars.len() {
            let prev_ret = (bars[i].close - bars[i - 1].close) / bars[i - 1].close;
            feature[i] = -prev_ret * 5.0;
        }
        feature
    }
}
