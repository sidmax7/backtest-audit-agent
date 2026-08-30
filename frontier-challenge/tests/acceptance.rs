//! Evaluation harness: runs both `baseline` and `advanced` against every
//! seeded strategy and reports detection accuracy -- the "Measured
//! Improvement" evidence in agents.md's Definition of Done.
//!
//! This test makes real LLM API calls and costs real money, so it is
//! opt-in: it skips (not fails) when no API key is configured for the
//! active `LLM_PROVIDER`, keeping a plain `cargo test` free and fully
//! offline. Set the relevant key (see .env.example) and run
//! `cargo test --test acceptance -- --nocapture` to see the accuracy table.
//! Each invocation writes its own trajectory file to trajectories/ as a
//! side effect (via the telemetry module both binaries use), satisfying the
//! "trajectories/ contains a trace for every seeded strategy, for both
//! binaries" Definition of Done item.

use frontier_challenge::strategies::all_strategies;
use frontier_challenge::verify::AgentVerdict;
use std::process::Command;

/// Treats a set-but-empty variable (`ANTHROPIC_API_KEY=` in a `.env`, which
/// `std::env::var` reports as `Ok("")`, not an error) as absent. Mirrors
/// `llm::env_var_nonempty`; using a bare `.is_ok()` here made this test
/// believe a key was configured and *fail* rather than skip -- see
/// CHANGELOG Iteration 5, where the same bug was fixed in the client.
fn env_var_nonempty(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
}

fn active_api_key_is_configured() -> bool {
    dotenvy::dotenv().ok();
    let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
    match provider.to_lowercase().as_str() {
        "gemini" => env_var_nonempty("GEMINI_API_KEY"),
        _ => env_var_nonempty("ANTHROPIC_API_KEY"),
    }
}

/// `None` means no `VERDICT:` line matched any known word -- a genuine
/// parse failure. `Inconclusive` is a real, meaningful outcome (advanced
/// only, when its two tools disagree -- see `verify::combine_signals`), not
/// a parse failure, so it gets its own tally rather than being scored right
/// or wrong.
fn parse_verdict_line(stdout: &str) -> Option<AgentVerdict> {
    for line in stdout.lines() {
        if let Some(rest) = line.trim().strip_prefix("VERDICT:") {
            let rest = rest.trim();
            if rest == "LEAKY" {
                return Some(AgentVerdict::Leaky);
            }
            if rest == "CLEAN" {
                return Some(AgentVerdict::Clean);
            }
            if rest == "INCONCLUSIVE" {
                return Some(AgentVerdict::Inconclusive);
            }
        }
    }
    None
}

fn run_binary(bin_path: &str, source_path: &str) -> Option<AgentVerdict> {
    let output = Command::new(bin_path)
        .arg(source_path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|e| panic!("failed to run {bin_path}: {e}"));
    parse_verdict_line(&String::from_utf8_lossy(&output.stdout))
}

fn expected(is_leaky: bool) -> AgentVerdict {
    if is_leaky {
        AgentVerdict::Leaky
    } else {
        AgentVerdict::Clean
    }
}

#[test]
fn baseline_and_advanced_detection_accuracy() {
    if !active_api_key_is_configured() {
        eprintln!(
            "SKIPPED: no API key configured for the active LLM_PROVIDER (see .env.example). \
             This test makes real, billed LLM calls and is opt-in by design -- \
             set ANTHROPIC_API_KEY or GEMINI_API_KEY and rerun with \
             `cargo test --test acceptance -- --nocapture` to see the accuracy table."
        );
        return;
    }

    let baseline_bin = env!("CARGO_BIN_EXE_baseline");
    let advanced_bin = env!("CARGO_BIN_EXE_advanced");

    let mut baseline_correct = 0;
    let mut advanced_correct = 0;
    let mut advanced_inconclusive = 0;
    let mut parse_failures = Vec::new();
    let mut total = 0;

    println!(
        "{:<28} {:>6} {:>10} {:>10}",
        "strategy", "truth", "baseline", "advanced"
    );
    for entry in all_strategies() {
        total += 1;
        let baseline_verdict = run_binary(baseline_bin, entry.source_path);
        let advanced_verdict = run_binary(advanced_bin, entry.source_path);

        if baseline_verdict.is_none() {
            parse_failures.push(format!("baseline/{}", entry.name));
        }
        if advanced_verdict.is_none() {
            parse_failures.push(format!("advanced/{}", entry.name));
        }
        if baseline_verdict == Some(expected(entry.is_leaky)) {
            baseline_correct += 1;
        }
        if advanced_verdict == Some(expected(entry.is_leaky)) {
            advanced_correct += 1;
        } else if advanced_verdict == Some(AgentVerdict::Inconclusive) {
            advanced_inconclusive += 1;
        }

        println!(
            "{:<28} {:>6} {:>10} {:>10}",
            entry.name,
            if entry.is_leaky { "leaky" } else { "clean" },
            fmt_verdict(baseline_verdict),
            fmt_verdict(advanced_verdict),
        );
    }

    // Inconclusive is excluded from the accuracy denominator -- it's a
    // real, meaningful outcome (a genuine tool disagreement), not a wrong
    // answer scored against the ground truth the same way Leaky/Clean are.
    println!("\nbaseline accuracy: {baseline_correct}/{total}");
    println!(
        "advanced accuracy: {advanced_correct}/{} ({advanced_inconclusive} inconclusive, not scored)",
        total - advanced_inconclusive
    );

    // Structural sanity check, not a model-quality assertion: every run
    // must produce a parseable verdict at all (i.e. the plumbing -- CLI
    // args, LLM call, tool loop, output format -- actually works end to
    // end). Whether the *content* of each verdict is correct is reported
    // above, not asserted here, since a single live run's accuracy against
    // a non-deterministic model is evidence to record, not a pass/fail gate.
    assert!(
        parse_failures.is_empty(),
        "these runs produced no parseable VERDICT: line: {parse_failures:?}"
    );
}

fn fmt_verdict(v: Option<AgentVerdict>) -> &'static str {
    match v {
        Some(AgentVerdict::Leaky) => "leaky",
        Some(AgentVerdict::Clean) => "clean",
        Some(AgentVerdict::Inconclusive) => "inconclusive",
        None => "PARSE_FAIL",
    }
}
