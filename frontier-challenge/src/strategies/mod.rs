pub mod execution_realism_leak;
pub mod execution_timing_mismatch;
pub mod forward_volatility_breakout;
pub mod forward_window_lookahead;
pub mod global_normalization_leak;
pub mod honest_mean_reversion;
pub mod honest_trailing_window;
pub mod mixed_signal_reversion;

use crate::engine::Strategy;

/// Mean of `closes[end_inclusive - window + 1 ..= end_inclusive]`. Shared by
/// `execution_timing_mismatch::ExecutionTimingMismatch` and
/// `honest_trailing_window::HonestTrailingWindow`, which are deliberately
/// built from the *identical* trailing-window feature -- the only thing
/// that separates the honest strategy from the buggy one is
/// `execution_offset` (1 vs 0), not the feature math itself.
pub(crate) fn trailing_mean(closes: &[f64], end_inclusive: usize, window: usize) -> f64 {
    let start = end_inclusive.saturating_sub(window - 1);
    let slice = &closes[start..=end_inclusive];
    slice.iter().sum::<f64>() / slice.len() as f64
}

/// One seeded strategy: its runnable implementation (for the real shift-test
/// tool call), the ground truth used to score accuracy, and the path to its
/// own source file -- what `baseline`/`advanced` hand the LLM to read, per
/// `cargo run --bin baseline -- <strategy-file>` in the Definition of Done.
pub struct StrategyEntry {
    pub name: &'static str,
    pub strategy: Box<dyn Strategy>,
    pub is_leaky: bool,
    pub source_path: &'static str,
}

/// Every seeded strategy, in the fixed order used by the evaluation harness
/// and the acceptance tests.
pub fn all_strategies() -> Vec<StrategyEntry> {
    vec![
        StrategyEntry {
            name: "forward_window_lookahead",
            strategy: Box::new(forward_window_lookahead::WindowMeanReversion { window: 5 }),
            is_leaky: true,
            source_path: "src/strategies/forward_window_lookahead.rs",
        },
        StrategyEntry {
            name: "global_normalization_leak",
            strategy: Box::new(global_normalization_leak::ZScoreReversion),
            is_leaky: true,
            source_path: "src/strategies/global_normalization_leak.rs",
        },
        StrategyEntry {
            name: "execution_timing_mismatch",
            strategy: Box::new(execution_timing_mismatch::MeanDeviationCrossover { window: 5 }),
            is_leaky: true,
            source_path: "src/strategies/execution_timing_mismatch.rs",
        },
        StrategyEntry {
            name: "forward_volatility_breakout",
            strategy: Box::new(forward_volatility_breakout::ForwardVolatilityBreakout {
                window: 5,
            }),
            is_leaky: true,
            source_path: "src/strategies/forward_volatility_breakout.rs",
        },
        StrategyEntry {
            name: "mixed_signal_reversion",
            strategy: Box::new(mixed_signal_reversion::MixedSignalReversion { window: 5 }),
            is_leaky: true,
            source_path: "src/strategies/mixed_signal_reversion.rs",
        },
        StrategyEntry {
            // Genuinely Clean under the whole-vector shift-test alone (its
            // feature has zero temporal look-ahead) and undetected by
            // static_scan (no rule for this axis exists) -- deliberately
            // excluded from both of those generic regression tests' loops,
            // with a comment at each exclusion explaining why. Only
            // correctly Leaky once `verify::slippage_test` is in the mix;
            // see verify::slippage and CHANGELOG for the full story.
            name: "execution_realism_leak",
            strategy: Box::new(execution_realism_leak::FavorableFillReversion),
            is_leaky: true,
            source_path: "src/strategies/execution_realism_leak.rs",
        },
        StrategyEntry {
            name: "honest_mean_reversion",
            strategy: Box::new(honest_mean_reversion::PriorReturnFade),
            is_leaky: false,
            source_path: "src/strategies/honest_mean_reversion.rs",
        },
        StrategyEntry {
            name: "honest_trailing_window",
            strategy: Box::new(honest_trailing_window::TrailingMeanFade { window: 2 }),
            is_leaky: false,
            source_path: "src/strategies/honest_trailing_window.rs",
        },
    ]
}

/// Looks up a seeded strategy by name (a source file's stem, e.g.
/// `"forward_window_lookahead"` from `.../forward_window_lookahead.rs`).
pub fn find_by_name(name: &str) -> Option<StrategyEntry> {
    all_strategies().into_iter().find(|e| e.name == name)
}
