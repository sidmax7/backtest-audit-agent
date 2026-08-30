use frontier_challenge::engine::{generate_series, PriceParams};
use frontier_challenge::strategies::all_strategies;
use frontier_challenge::verify::{audit, shift_test};

fn main() {
    for seed in [42u64, 7, 123, 999] {
        let bars = generate_series(PriceParams {
            seed,
            ..PriceParams::default()
        });
        println!("\n=== seed {seed} ===");
        println!(
            "{:<28} {:>10} {:>10} {:>10} {:>8} {:>8}",
            "strategy", "sharpe_0", "sharpe_1", "delta", "leaky?", "verdict"
        );
        let mut correct = 0;
        let mut total = 0;
        for entry in all_strategies() {
            let result = shift_test(&bars, entry.strategy.as_ref());
            let verdict = audit(&result);
            let predicted_leaky = matches!(verdict, frontier_challenge::verify::Verdict::Leaky);
            total += 1;
            if predicted_leaky == entry.is_leaky {
                correct += 1;
            }
            println!(
                "{:<28} {:>10.3} {:>10.3} {:>10.3} {:>8} {:>8?}",
                entry.name,
                result.sharpe_original,
                result.sharpe_shifted,
                result.delta,
                entry.is_leaky,
                verdict
            );
        }
        println!("accuracy: {correct}/{total}");
    }
}
