//! Regenerates the example CSVs in `examples/csv/` -- the same format a
//! pandas user would get from `df.to_csv(index=False)`, so the adapter can
//! be tried immediately without anyone having to build an export first.
use frontier_challenge::engine::{generate_series, PriceParams};
use frontier_challenge::strategies::find_by_name;

fn main() {
    let bars = generate_series(PriceParams {
        n_bars: 400,
        ..PriceParams::default()
    });
    for name in ["forward_window_lookahead", "honest_mean_reversion"] {
        let entry = find_by_name(name).expect("registered strategy");
        let f = entry.strategy.compute_features(&bars);
        let mut s = String::from("open,high,low,close,volume,feature\n");
        for (b, v) in bars.iter().zip(&f) {
            s.push_str(&format!(
                "{:.6},{:.6},{:.6},{:.6},{:.0},{:.6}\n",
                b.open, b.high, b.low, b.close, b.volume, v
            ));
        }
        let path = format!("examples/csv/{name}.csv");
        std::fs::write(&path, s).unwrap();
        println!("wrote {path}");
    }
}
