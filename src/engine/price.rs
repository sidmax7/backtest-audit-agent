//! Deterministic synthetic OHLCV price generator.
//!
//! The return-generating process is a mean-reverting AR(1) on log returns:
//! `r[t] = mu + phi * r[t-1] + sigma * z[t]`, with `phi` slightly negative.
//! This gives a small, genuine, exploitable edge (real predictability from the
//! previous bar's already-known return) without straying far from a random
//! walk -- which is what makes a leakage-inflated Sharpe ratio stand out as
//! implausible against a clean strategy's modest one.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PriceParams {
    pub seed: u64,
    pub n_bars: usize,
    pub mu: f64,
    pub phi: f64,
    pub sigma: f64,
    pub start_price: f64,
}

impl Default for PriceParams {
    fn default() -> Self {
        PriceParams {
            seed: 42,
            n_bars: 5000,
            mu: 0.0002,
            phi: -0.12,
            sigma: 0.01,
            start_price: 100.0,
        }
    }
}

/// Box-Muller transform for a standard normal draw from two uniforms.
fn standard_normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1: f64 = rng.gen_range(f64::EPSILON..1.0);
    let u2: f64 = rng.gen_range(0.0..1.0);
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Generates a deterministic synthetic OHLCV bar series from `params`.
///
/// Same `params` always produces the exact same bars -- this is the
/// reproducibility guarantee the whole evaluation harness depends on.
pub fn generate_series(params: PriceParams) -> Vec<Bar> {
    let mut rng = ChaCha8Rng::seed_from_u64(params.seed);
    let mut bars = Vec::with_capacity(params.n_bars);

    let mut price = params.start_price;
    let mut prev_ret = 0.0_f64;

    for _ in 0..params.n_bars {
        let z = standard_normal(&mut rng);
        let ret = params.mu + params.phi * prev_ret + params.sigma * z;
        prev_ret = ret;

        let open_gap = params.sigma * 0.1 * standard_normal(&mut rng);
        let open = price * (1.0 + open_gap);
        let close = open * (1.0 + ret);

        let intrabar_noise = rng.gen_range(0.0_f64..params.sigma.abs() * 0.5);
        let high = open.max(close) * (1.0 + intrabar_noise);
        let low = open.min(close) * (1.0 - intrabar_noise);
        let volume = 1_000_000.0 * (1.0 + rng.gen_range(-0.2_f64..0.2));

        bars.push(Bar {
            open,
            high,
            low,
            close,
            volume,
        });
        price = close;
    }

    bars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_identical_series() {
        let params = PriceParams::default();
        let a = generate_series(params);
        let b = generate_series(params);
        assert_eq!(a, b);
    }

    #[test]
    fn different_seed_produces_different_series() {
        let a = generate_series(PriceParams::default());
        let b = generate_series(PriceParams {
            seed: 43,
            ..PriceParams::default()
        });
        assert_ne!(a, b);
    }

    #[test]
    fn prices_stay_positive_and_finite() {
        let bars = generate_series(PriceParams::default());
        for bar in &bars {
            assert!(bar.open > 0.0 && bar.open.is_finite());
            assert!(bar.close > 0.0 && bar.close.is_finite());
            assert!(bar.high >= bar.open.max(bar.close));
            assert!(bar.low <= bar.open.min(bar.close));
        }
    }
}
