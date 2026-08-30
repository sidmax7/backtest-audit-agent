//! Audits a backtest this crate did not author, from exported CSV.
//!
//! The seeded battery and both LLM binaries can only audit strategies
//! registered in `strategies::all_strategies()`, because `verify::shift_test`
//! needs a runnable `Strategy` to recompute features from. That is the single
//! biggest gap between what this project claims (audit a quant's backtest)
//! and what it could do (audit our own eight). This binary closes it for the
//! temporal axis: the perturbation underneath never needed the trait, only a
//! price series and an array of numbers, so a backtest written in *any*
//! language can be checked by exporting two columns.
//!
//! Deliberately LLM-free: the shift-test is deterministic, so a verdict on a
//! user's own data costs nothing, needs no API key, and is reproducible. The
//! LLM in `advanced` exists to form and narrate a *hypothesis about source
//! code*; with only numbers and no source, it would have nothing to read.

use anyhow::{Context, Result};
use clap::Parser;
use frontier_challenge::csv_input;
use frontier_challenge::telemetry::Trajectory;
use frontier_challenge::verify::{
    audit, shift_test_features, Verdict, DELTA_ROBUSTNESS_THRESHOLD, IMPLAUSIBLE_SHARPE_CEILING,
};
use std::path::PathBuf;

/// Below this absolute Sharpe, a feature has no meaningful edge in either
/// direction, so the shift-test's delta signal cannot distinguish "leak" from
/// "nothing to lose". Not a leakage threshold -- a guard against reporting a
/// confident verdict where the test is structurally uninformative.
const NO_EDGE_SHARPE: f64 = 0.5;

#[derive(Parser)]
#[command(about = "Shift-test a backtest exported as CSV (close + one or more feature columns).")]
struct Args {
    /// CSV with a header row: a `close` column, at least one `feature*`
    /// column, and optionally open/high/low/volume.
    csv_file: PathBuf,

    /// Bars between the feature and the return it is applied to. `1` is the
    /// honest convention (decide on bar i, earn bar i+1's return); `0` means
    /// the backtest trades on the same bar's return its feature came from.
    #[arg(long, default_value_t = 1)]
    execution_offset: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let series = csv_input::load(&args.csv_file)?;
    let name = args
        .csv_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("csv_input")
        .to_string();

    let mut trajectory = Trajectory::new(
        "audit_csv",
        &name,
        "none",
        "n/a (deterministic, no LLM call)",
    );
    trajectory.record_reasoning(format!(
        "Loaded {} bars and {} feature column(s) from CSV; execution_offset={}. No LLM in this path -- the shift-test is deterministic.",
        series.bars.len(),
        series.features.len(),
        args.execution_offset
    ));

    println!("source:  {}", args.csv_file.display());
    println!(
        "loaded:  {} bars, {} feature column(s), execution_offset={}",
        series.bars.len(),
        series.features.len(),
        args.execution_offset
    );
    println!();

    let mut any_leaky = false;
    let mut worst_delta = f64::INFINITY;
    let mut no_edge_warnings: Vec<String> = Vec::new();

    for (col, values) in &series.features {
        trajectory.record_tool_call(
            "shift_test",
            serde_json::json!({"column": col, "execution_offset": args.execution_offset}),
        );
        let r = shift_test_features(&series.bars, values, args.execution_offset);
        let v = audit(&r);
        trajectory.record_tool_output(serde_json::json!({
            "column": col,
            "sharpe_original": r.sharpe_original,
            "sharpe_shifted": r.sharpe_shifted,
            "delta": r.delta,
            "verdict": format!("{v:?}"),
        }));

        let robust = r.delta < DELTA_ROBUSTNESS_THRESHOLD;
        let implausible = r.sharpe_original > IMPLAUSIBLE_SHARPE_CEILING;
        let why = match (robust, implausible) {
            (true, true) => "robust to shift AND implausibly high",
            (true, false) => "robust to shift",
            (false, true) => "implausibly high Sharpe (see caveat below)",
            (false, false) => "collapses under shift, plausible Sharpe",
        };
        println!(
            "{col}: sharpe_original={:.3} sharpe_shifted={:.3} delta={:.3} -> {:?} ({why})",
            r.sharpe_original, r.sharpe_shifted, r.delta, v
        );

        // The delta signal asks "did shifting destroy this strategy's edge?"
        // That question is only meaningful if there was an edge to destroy.
        // A feature with no real predictive power is *trivially* robust to
        // the shift -- nothing collapses because nothing was there -- which
        // reads as a leak under the delta rule alone. Found while testing
        // this adapter against a no-edge random series, which it duly
        // flagged Leaky. Surfaced rather than silently mis-verdicted.
        if robust && !implausible && r.sharpe_original.abs() < NO_EDGE_SHARPE {
            no_edge_warnings.push(col.clone());
            println!(
                "  ^ WARNING: |sharpe_original| < {NO_EDGE_SHARPE} -- this feature has essentially no\n\
                 \x20   edge either way, so \"robust to shift\" is uninformative here rather than\n\
                 \x20   evidence of leakage. Treat this column as INCONCLUSIVE, not Leaky."
            );
        }

        any_leaky |= v == Verdict::Leaky;
        worst_delta = worst_delta.min(r.delta);
    }

    let overall = if any_leaky {
        Verdict::Leaky
    } else {
        Verdict::Clean
    };
    println!();
    println!(
        "VERDICT: {}",
        match overall {
            Verdict::Leaky => "LEAKY",
            Verdict::Clean => "CLEAN",
        }
    );

    // The honest part. The delta signal is a *relative* comparison inside the
    // user's own series, so it transfers to their market. The absolute
    // ceiling does not: it was calibrated against this crate's synthetic
    // generator (~1% daily vol, small mean-reversion edge). Reporting it as
    // if it were universally valid would be exactly the unearned credibility
    // this project's own README warns about, so it is flagged, not hidden.
    println!();
    println!("Caveats for external data:");
    println!(
        "  - The delta signal (shift collapses the edge or it doesn't) is relative to your own"
    );
    println!("    series and transfers to your market.");
    println!(
        "  - IMPLAUSIBLE_SHARPE_CEILING={IMPLAUSIBLE_SHARPE_CEILING} was calibrated on this crate's synthetic generator,"
    );
    println!(
        "    NOT on your market. Treat a ceiling-only verdict as a prompt to check, not a finding."
    );
    println!("  - Only the temporal axis is checked here. slippage_test needs your fill logic and");
    println!("    static_scan needs Rust source, so neither applies to a CSV export.");
    if !no_edge_warnings.is_empty() {
        println!(
            "  - {} column(s) had essentially no edge ({}), where the delta signal is",
            no_edge_warnings.len(),
            no_edge_warnings.join(", ")
        );
        println!("    uninformative by construction -- see the warning(s) above.");
    }

    trajectory.record_verification(format!(
        "worst (most robust) delta across {} column(s) = {:.3}; threshold {DELTA_ROBUSTNESS_THRESHOLD}",
        series.features.len(),
        worst_delta
    ));
    trajectory.record_final_verdict(
        format!("{overall:?}").to_uppercase(),
        worst_delta,
        "deterministic shift-test on user-supplied CSV; no LLM, temporal axis only; absolute-Sharpe ceiling not calibrated for this market",
    );
    let path = trajectory
        .finish_and_write(&PathBuf::from("trajectories"))
        .context("writing trajectory")?;
    eprintln!("\ntrajectory written to {}", path.display());
    Ok(())
}
