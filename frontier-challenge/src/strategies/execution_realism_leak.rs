//! LEAKY -- but on a different axis than every other strategy in this
//! crate. The feature is mechanically identical to `PriorReturnFade`
//! (`honest_mean_reversion.rs`): bets against the previous bar's realized
//! return, using only past closes, honest `execution_offset` of 1. Under
//! the ordinary whole-vector `shift_test` this strategy is genuinely
//! Clean -- there is no temporal look-ahead in the feature at all. The
//! leak lives in `realized_return`: it assumes a systematically favorable
//! fill for whichever side the position is actually on (bought at the
//! bar's low, sold at the bar's high), which no real order type
//! guarantees without slippage, partial fills, or genuine luck. Since
//! `low <= close <= high` always holds (see `engine::price`'s own tested
//! invariant), this bonus is never negative and always aligned with the
//! position's own direction -- a real, systematic Sharpe inflator, not a
//! coin flip. See `verify::slippage_test` for the tool built to catch it.

use crate::engine::{Bar, Strategy};

pub struct FavorableFillReversion;

impl Strategy for FavorableFillReversion {
    fn name(&self) -> &str {
        "favorable_fill_reversion"
    }

    fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
        let mut feature = vec![0.0; bars.len()];
        for i in 1..bars.len() {
            let prev_ret = (bars[i].close - bars[i - 1].close) / bars[i - 1].close;
            feature[i] = -prev_ret * 5.0;
        }
        feature
    }

    fn realized_return(&self, prev_bar: &Bar, bar: &Bar, position: f64) -> f64 {
        let close_to_close = (bar.close - prev_bar.close) / prev_bar.close;
        // Realized PnL is `position * realized_return`, so a bonus *return*
        // that's added here gets multiplied by `position`'s own sign before
        // it becomes PnL -- for a short (negative position), adding a
        // positive return bonus would flip into a *penalty*. Longs add the
        // low-to-close recovery; shorts subtract the close-to-high move so
        // that, once multiplied by a negative position, it comes out as
        // the same kind of non-negative, direction-aligned PnL bonus.
        if position > 0.0 {
            // "Bought at the low, marked at the close" -- captures the
            // intrabar recovery from low to close as free, direction-
            // aligned alpha no real limit or market order guarantees.
            close_to_close + (bar.close - bar.low) / prev_bar.close
        } else if position < 0.0 {
            // "Sold at the high, marked at the close."
            close_to_close - (bar.high - bar.close) / prev_bar.close
        } else {
            0.0
        }
    }
}
