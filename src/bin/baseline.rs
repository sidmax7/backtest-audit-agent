//! Naive baseline: one LLM call on raw strategy source, no tool use, no
//! empirical check behind the verdict. This is the thing `advanced.rs` has
//! to beat -- see `problem.md` §03's workflow comparison table.

use anyhow::{Context, Result};
use clap::Parser;
use frontier_challenge::audit_source::strip_comments;
use frontier_challenge::llm::{Client, Message};
use frontier_challenge::telemetry::Trajectory;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    /// Path to a strategy source file, e.g.
    /// src/strategies/forward_window_lookahead.rs
    strategy_file: PathBuf,
}

const SYSTEM_PROMPT: &str = "You are auditing a quantitative trading strategy's Rust source code for data leakage and look-ahead bias -- e.g. features computed from future bars, execution against a return that already happened, or normalization/statistics fit on the whole series instead of only past data. You get ONE look at the source. You cannot run any code or ask for more information. Respond with your verdict as exactly one word on the first line, LEAKY or CLEAN, followed by a short justification.";

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    let source = std::fs::read_to_string(&args.strategy_file)
        .with_context(|| format!("reading {}", args.strategy_file.display()))?;
    let strategy_name = args
        .strategy_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown_strategy")
        .to_string();

    let client = Client::from_env()?;
    let mut trajectory = Trajectory::new(
        "baseline",
        &strategy_name,
        client.provider_label(),
        client.model(),
    );

    // Comments are stripped, and the file path/name is never included in
    // the prompt: both would otherwise hand the LLM a spoiler (e.g. a doc
    // comment or a struct name literally admitting "leaky") that trivializes
    // "detection" and invalidates the whole baseline-vs-advanced comparison.
    // See CHANGELOG for how this was found.
    let sanitized_source = strip_comments(&source);
    let prompt = format!("Strategy source:\n\n```rust\n{sanitized_source}\n```");
    trajectory.record_reasoning(format!(
        "Single-shot read of a {}-byte strategy (comments stripped before showing the LLM), no tool use available.",
        sanitized_source.len()
    ));

    let response = client.send(SYSTEM_PROMPT, &[Message::user_text(&prompt)], &[])?;
    trajectory.add_tokens(response.input_tokens, response.output_tokens);

    let text = response.text();
    let verdict_word = text
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.')
        .to_uppercase();
    let verdict = if verdict_word.starts_with("LEAKY") {
        "LEAKY"
    } else {
        "CLEAN"
    };

    trajectory.record_final_verdict(
        verdict,
        0.0,
        "unverified (single LLM call, no empirical check)",
    );

    println!("strategy: {strategy_name}");
    println!("verdict:  {verdict}  (baseline -- no empirical verification)");
    // Machine-parseable line for tests/acceptance.rs; kept separate from the
    // human-readable line above so reformatting that one can't break parsing.
    println!("VERDICT: {verdict}");
    println!("\n--- model response ---\n{text}");

    let trajectory_dir = PathBuf::from("trajectories");
    let path = trajectory.finish_and_write(&trajectory_dir)?;
    eprintln!("\ntrajectory written to {}", path.display());

    Ok(())
}
